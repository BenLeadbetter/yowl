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
    /// Tracks how much of the transcript has been sent to the client
    last_polled_len: Mutex<usize>,
}

impl DaemonState {
    pub fn new() -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let transcriber = StreamingTranscriber::new(Duration::from_secs(BUFFER_DURATION_SECS))?;

        Ok(Arc::new(Self {
            transcriber,
            recording: AtomicBool::new(false),
            worker_thread: Mutex::new(None),
            last_polled_len: Mutex::new(0),
        }))
    }

    pub fn start_recording(self: &Arc<Self>) -> &'static str {
        if self.recording.swap(true, Ordering::SeqCst) {
            return "ERROR already recording";
        }

        // Reset transcriber state from any previous recording
        self.transcriber.reset();
        *self.last_polled_len.lock().unwrap() = 0;

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

        let text = self.transcriber.current_transcript();
        let mut last_len = self.last_polled_len.lock().unwrap();

        // Only return new text since last poll
        let delta = if text.len() > *last_len {
            &text[*last_len..]
        } else {
            ""
        };

        *last_len = text.len();
        format!("RECORDING:{}", delta)
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}
