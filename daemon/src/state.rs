use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::audio::AudioCapture;
use crate::whisper::StreamingTranscriber;

/// How often to run whisper inference (ms)
const TRANSCRIBE_INTERVAL_MS: u64 = 500;

/// Rolling buffer duration for audio context
const BUFFER_DURATION_SECS: u64 = 10;

pub struct DaemonState {
    transcriber: StreamingTranscriber,
    recording: AtomicBool,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
    /// Text that has been finalized (audio aged out of buffer) - never backspace into this
    committed_text: Mutex<String>,
    /// Text we've sent but may still revise via backspaces
    provisional_text: Mutex<String>,
}

impl DaemonState {
    pub fn new() -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let transcriber = StreamingTranscriber::new(Duration::from_secs(BUFFER_DURATION_SECS))?;

        Ok(Arc::new(Self {
            transcriber,
            recording: AtomicBool::new(false),
            worker_thread: Mutex::new(None),
            committed_text: Mutex::new(String::new()),
            provisional_text: Mutex::new(String::new()),
        }))
    }

    pub fn start_recording(self: &Arc<Self>) -> &'static str {
        if self.recording.swap(true, Ordering::SeqCst) {
            return "ERROR already recording";
        }

        // Reset transcriber state from any previous recording
        self.transcriber.reset();
        *self.committed_text.lock().unwrap() = String::new();
        *self.provisional_text.lock().unwrap() = String::new();

        // Spawn worker thread - AudioCapture must be created on this thread
        // because cpal::Stream is not Send
        let state = Arc::clone(self);
        let handle = thread::spawn(move || {
            // Create audio capture on this thread
            let capture = match AudioCapture::new() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to create audio capture: {}", e);
                    state.recording.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = capture.start() {
                log::error!("Failed to start audio capture: {}", e);
                state.recording.store(false, Ordering::SeqCst);
                return;
            }

            let mut last_transcribe = Instant::now();
            let transcribe_interval = Duration::from_millis(TRANSCRIBE_INTERVAL_MS);

            while state.recording.load(Ordering::SeqCst) {
                // Collect audio samples from capture
                while let Some(samples) = capture.try_recv() {
                    state.transcriber.push_audio(&samples);
                }

                // Run transcription periodically
                if last_transcribe.elapsed() >= transcribe_interval {
                    match state.transcriber.transcribe() {
                        Ok(Some(text)) => {
                            log::debug!("transcribed: {}", text);
                        }
                        Ok(None) => {
                            // No change
                        }
                        Err(e) => {
                            log::error!("Transcription error: {}", e);
                        }
                    }
                    last_transcribe = Instant::now();
                }

                thread::sleep(Duration::from_millis(10));
            }

            // Stop audio capture
            if let Err(e) = capture.stop() {
                log::warn!("Error stopping capture: {}", e);
            }

            log::debug!("worker thread exiting");
        });

        *self.worker_thread.lock().unwrap() = Some(handle);
        log::info!("recording started");
        "OK"
    }

    pub fn stop_recording(&self) -> &'static str {
        if !self.recording.swap(false, Ordering::SeqCst) {
            return "ERROR not recording";
        }

        // Wait for worker thread to finish
        if let Some(handle) = self.worker_thread.lock().unwrap().take() {
            let _ = handle.join();
        }

        log::info!("recording stopped");
        "OK"
    }

    pub fn poll(&self) -> String {
        if !self.recording.load(Ordering::SeqCst) {
            return "IDLE:".to_string();
        }

        let new_transcript = self.transcriber.current_transcript();
        let mut committed = self.committed_text.lock().unwrap();
        let mut provisional = self.provisional_text.lock().unwrap();

        if new_transcript.is_empty() {
            return "RECORDING:0:".to_string();
        }

        // Find where the new transcript "picks up" relative to our provisional text.
        // As audio ages out of the rolling buffer, text at the start of provisional
        // will no longer appear in the new transcript.
        let aging_point = Self::find_aging_point(&provisional, &new_transcript);

        if aging_point > 0 {
            // Text before aging_point has aged out - commit it
            let to_commit: String = provisional.chars().take(aging_point).collect();
            committed.push_str(&to_commit);
            *provisional = provisional.chars().skip(aging_point).collect();
        }

        // Now diff new_transcript against the remaining provisional text
        let common_len = provisional
            .chars()
            .zip(new_transcript.chars())
            .take_while(|(a, b)| a == b)
            .count();

        let backspace_count = provisional.chars().count() - common_len;
        let new_chars: String = new_transcript.chars().skip(common_len).collect();

        // Update provisional to the new transcript
        *provisional = new_transcript;

        format!("RECORDING:{}:{}", backspace_count, new_chars)
    }

    /// Find how many characters from the start of provisional have "aged out"
    /// by looking for where new_transcript's content appears in provisional.
    fn find_aging_point(provisional: &str, new_transcript: &str) -> usize {
        if provisional.is_empty() || new_transcript.is_empty() {
            return 0;
        }

        // If new_transcript starts with provisional content, nothing has aged
        if new_transcript.starts_with(provisional) || provisional.starts_with(new_transcript) {
            return 0;
        }

        // Look for the start of new_transcript within provisional
        // Try progressively shorter prefixes of new_transcript as search keys
        let max_search_len = new_transcript.chars().count().min(30);

        for key_len in (5..=max_search_len).rev() {
            let search_key: String = new_transcript.chars().take(key_len).collect();
            if let Some(byte_pos) = provisional.find(&search_key) {
                // Found the key - everything before it has aged out
                return provisional[..byte_pos].chars().count();
            }
        }

        // No overlap found - likely a complete refresh, commit all provisional
        provisional.chars().count()
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}
