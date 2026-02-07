use crate::audio::AudioCapture;
use crate::diff::TextTracker;
use crate::whisper::StreamingTranscriber;

const TRANSCRIBE_INTERVAL_MS: u64 = 500;
const BUFFER_DURATION_SECS: u64 = 10;

pub struct DaemonState {
    transcriber: StreamingTranscriber,
    recording: std::sync::atomic::AtomicBool,
    worker_thread: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    text_tracker: std::sync::Mutex<TextTracker>,
}

impl DaemonState {
    pub fn new() -> Result<std::sync::Arc<Self>, Box<dyn std::error::Error>> {
        let transcriber = StreamingTranscriber::new(std::time::Duration::from_secs(BUFFER_DURATION_SECS))?;

        Ok(std::sync::Arc::new(Self {
            transcriber,
            recording: std::sync::atomic::AtomicBool::new(false),
            worker_thread: std::sync::Mutex::new(None),
            text_tracker: std::sync::Mutex::new(TextTracker::new()),
        }))
    }

    pub fn start_recording(self: &std::sync::Arc<Self>) -> &'static str {
        if self.recording.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return "ERROR already recording";
        }

        // reset any previous recording session
        self.transcriber.reset();
        self.text_tracker.lock().unwrap().reset();

        let state = std::sync::Arc::clone(self);
        let handle = std::thread::spawn(move || {
            let capture = match AudioCapture::new() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to create audio capture: {}", e);
                    state.recording.store(false, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
            };

            if let Err(e) = capture.start() {
                log::error!("Failed to start audio capture: {}", e);
                state.recording.store(false, std::sync::atomic::Ordering::SeqCst);
                return;
            }

            let mut last_transcribe = std::time::Instant::now();
            let transcribe_interval = std::time::Duration::from_millis(TRANSCRIBE_INTERVAL_MS);

            while state.recording.load(std::sync::atomic::Ordering::SeqCst) {
                while let Some(samples) = capture.recv() {
                    state.transcriber.push_audio(&samples);
                }

                if last_transcribe.elapsed() >= transcribe_interval {
                    match state.transcriber.transcribe() {
                        Ok(Some(text)) => {
                            log::debug!("transcribed: {}", text);
                        }
                        Ok(None) => {
                            // no change
                        }
                        Err(e) => {
                            log::error!("Transcription error: {}", e);
                        }
                    }
                    last_transcribe = std::time::Instant::now();
                }

                std::thread::sleep(std::time::Duration::from_millis(10));
            }

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
        if !self.recording.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return "ERROR not recording";
        }

        if let Some(handle) = self.worker_thread.lock().unwrap().take() {
            let _ = handle.join();
        }

        log::info!("recording stopped");
        "OK"
    }

    pub fn poll(&self) -> String {
        if !self.recording.load(std::sync::atomic::Ordering::SeqCst) {
            return "IDLE:".to_string();
        }

        let new_transcript = self.transcriber.current_transcript();
        let mut tracker = self.text_tracker.lock().unwrap();

        match tracker.update(&new_transcript) {
            Some(result) => format!("RECORDING:{}:{}", result.backspaces, result.new_text),
            None => "RECORDING:0:".to_string(),
        }
    }
}
