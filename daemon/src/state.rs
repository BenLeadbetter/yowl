use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct DaemonState {
    buffer: Mutex<String>,
    recording: AtomicBool,
    counter_thread: Mutex<Option<JoinHandle<()>>>,
}

impl DaemonState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            buffer: Mutex::new(String::new()),
            recording: AtomicBool::new(false),
            counter_thread: Mutex::new(None),
        })
    }

    pub fn start_recording(self: &Arc<Self>) -> &'static str {
        if self.recording.swap(true, Ordering::SeqCst) {
            return "ERROR already recording";
        }

        let state = Arc::clone(self);
        let handle = thread::spawn(move || {
            let mut counter = 1u32;
            while state.recording.load(Ordering::SeqCst) {
                {
                    let mut buf = state.buffer.lock().unwrap();
                    buf.push_str(&format!("{counter}\n"));
                    log::debug!("buffered: {counter}");
                }
                counter += 1;
                thread::sleep(Duration::from_secs(1));
            }
            log::debug!("counter thread exiting");
        });

        *self.counter_thread.lock().unwrap() = Some(handle);
        log::info!("recording started");
        "OK"
    }

    pub fn stop_recording(&self) -> &'static str {
        if !self.recording.swap(false, Ordering::SeqCst) {
            return "ERROR not recording";
        }

        if let Some(handle) = self.counter_thread.lock().unwrap().take() {
            let _ = handle.join();
        }

        log::info!("recording stopped");
        "OK"
    }

    pub fn poll(&self) -> String {
        if !self.recording.load(Ordering::SeqCst) {
            return "IDLE:".to_string();
        }
        let mut buf = self.buffer.lock().unwrap();
        let text = std::mem::take(&mut *buf);
        if !text.is_empty() {
            log::debug!("polled {} bytes", text.len());
        }
        format!("RECORDING:{}", text)
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            buffer: Mutex::new(String::new()),
            recording: AtomicBool::new(false),
            counter_thread: Mutex::new(None),
        }
    }
}
