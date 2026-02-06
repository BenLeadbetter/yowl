use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

// TODO: allow model selection and download at runtime
const MODEL_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/models/ggml-base.en.bin");
pub const SAMPLE_RATE: usize = 16000;

/// Rolling buffer for audio samples with a fixed capacity.
/// New samples push out old ones when capacity is exceeded.
pub struct RollingBuffer {
    samples: Vec<f32>,
    capacity: usize,
}

impl RollingBuffer {
    /// Create a new buffer with capacity for the given duration of audio at 16kHz.
    pub fn new(duration: std::time::Duration) -> Self {
        let capacity = (duration.as_secs() as usize) * SAMPLE_RATE;
        Self {
            samples: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Append new samples, discarding old ones if we exceed capacity.
    pub fn push(&mut self, new_samples: &[f32]) {
        self.samples.extend_from_slice(new_samples);

        if self.samples.len() > self.capacity {
            let excess = self.samples.len() - self.capacity;
            self.samples.drain(0..excess);
        }
    }

    /// Get a slice of all samples in the buffer.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Returns the number of samples currently in the buffer.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.samples.len()
    }
}

/// Streaming transcriber optimized for real-time audio.
/// Maintains a rolling buffer and tracks transcript changes.
pub struct StreamingTranscriber {
    ctx: WhisperContext,
    buffer: Mutex<RollingBuffer>,
    last_transcript: Mutex<String>,
}

impl StreamingTranscriber {
    /// Create a new streaming transcriber with the given buffer duration.
    pub fn new(buffer_duration: std::time::Duration) -> Result<Self, Box<dyn std::error::Error>> {
        let path = Path::new(MODEL_PATH);
        if !path.exists() {
            return Err(format!("Model not found: {MODEL_PATH}").into());
        }

        log::info!("Loading whisper model from {MODEL_PATH}");
        let ctx = WhisperContext::new_with_params(MODEL_PATH, WhisperContextParameters::default())
            .map_err(|e| format!("Failed to load model: {e}"))?;

        log::info!(
            "Whisper streaming transcriber ready ({}s buffer)",
            buffer_duration.as_secs()
        );

        Ok(Self {
            ctx,
            buffer: Mutex::new(RollingBuffer::new(buffer_duration)),
            last_transcript: Mutex::new(String::new()),
        })
    }

    /// Push new audio samples into the buffer.
    pub fn push_audio(&self, samples: &[f32]) {
        self.buffer.lock().unwrap().push(samples);
    }

    /// Run transcription on the current buffer contents.
    /// Returns the new transcript if it changed, or None if unchanged.
    pub fn transcribe(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let samples = {
            let buffer = self.buffer.lock().unwrap();
            buffer.samples().to_vec()
        };

        if samples.is_empty() {
            return Ok(None);
        }

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("Failed to create state: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_nst(true);
        params.set_no_context(true);

        state
            .full(params, &samples)
            .map_err(|e| format!("Inference failed: {e}"))?;

        let num_segments = state.full_n_segments();
        let mut result = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(text) = segment.to_str() {
                    result.push_str(text);
                }
            }
        }

        let transcript = result.trim().to_string();
        let mut last = self.last_transcript.lock().unwrap();

        if transcript != *last {
            *last = transcript.clone();
            Ok(Some(transcript))
        } else {
            Ok(None)
        }
    }

    /// Get the current full transcript without running inference.
    pub fn current_transcript(&self) -> String {
        self.last_transcript.lock().unwrap().clone()
    }

    /// Clear the buffer and transcript (call when stopping recording).
    pub fn reset(&self) {
        self.buffer.lock().unwrap().clear();
        *self.last_transcript.lock().unwrap() = String::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rolling_buffer() {
        let mut buffer = RollingBuffer::new(Duration::from_secs(2)); // 2 seconds = 32000 samples

        // Add 1 second of audio
        let chunk1: Vec<f32> = vec![0.1; SAMPLE_RATE];
        buffer.push(&chunk1);
        assert_eq!(buffer.len(), SAMPLE_RATE);

        // Add another second
        let chunk2: Vec<f32> = vec![0.2; SAMPLE_RATE];
        buffer.push(&chunk2);
        assert_eq!(buffer.len(), 2 * SAMPLE_RATE);

        // Add a third second - should push out the first
        let chunk3: Vec<f32> = vec![0.3; SAMPLE_RATE];
        buffer.push(&chunk3);
        assert_eq!(buffer.len(), 2 * SAMPLE_RATE);

        // First samples should be from chunk2
        assert!((buffer.samples()[0] - 0.2).abs() < 0.001);
        // Last samples should be from chunk3
        assert!((buffer.samples()[buffer.len() - 1] - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_streaming_transcriber() {
        let transcriber =
            StreamingTranscriber::new(Duration::from_secs(8)).expect("Failed to create transcriber");

        println!("\n=== Streaming transcriber test ===");

        // Push 1 second of silence
        let silence: Vec<f32> = vec![0.0; SAMPLE_RATE];
        transcriber.push_audio(&silence);

        // First transcription
        let result1 = transcriber.transcribe().expect("Transcription failed");
        println!("After 1s silence: {:?}", result1);

        // Push another second
        transcriber.push_audio(&silence);

        // Second transcription - should return None if unchanged
        let result2 = transcriber.transcribe().expect("Transcription failed");
        println!("After 2s silence: {:?}", result2);

        // Reset and verify empty
        transcriber.reset();
        assert!(transcriber.current_transcript().is_empty());

        println!("=== Test complete ===\n");
    }
}
