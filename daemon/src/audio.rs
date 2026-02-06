use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream, StreamConfig};
use std::sync::mpsc::{self, Receiver, Sender};

use crate::whisper::SAMPLE_RATE;

const WHISPER_SAMPLE_RATE: u32 = SAMPLE_RATE as u32;

/// Audio capture from the system microphone.
/// Captures audio and resamples to 16kHz mono f32 for Whisper.
pub struct AudioCapture {
    stream: Stream,
    receiver: Receiver<Vec<f32>>,
}

impl AudioCapture {
    /// Create a new audio capture from the default input device.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
        log::info!("Using input device: {}", device_name);

        // Get the default config
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        log::info!(
            "Input config: {} Hz, {} channels, {:?}",
            sample_rate,
            channels,
            config.sample_format()
        );

        let (sender, receiver) = mpsc::channel::<Vec<f32>>();

        // Calculate resampling ratio
        let resample_ratio = WHISPER_SAMPLE_RATE as f64 / sample_rate as f64;

        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                build_stream::<f32>(&device, &config.into(), sender, channels, resample_ratio)?
            }
            SampleFormat::I16 => {
                build_stream::<i16>(&device, &config.into(), sender, channels, resample_ratio)?
            }
            SampleFormat::U16 => {
                build_stream::<u16>(&device, &config.into(), sender, channels, resample_ratio)?
            }
            format => return Err(format!("Unsupported sample format: {:?}", format).into()),
        };

        Ok(Self { stream, receiver })
    }

    /// Start capturing audio.
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.play()?;
        log::info!("Audio capture started");
        Ok(())
    }

    /// Stop capturing audio.
    pub fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.pause()?;
        log::info!("Audio capture stopped");
        Ok(())
    }

    /// Receive captured audio samples (16kHz mono f32).
    /// Returns None if no samples are available (non-blocking).
    pub fn try_recv(&self) -> Option<Vec<f32>> {
        self.receiver.try_recv().ok()
    }

    /// Receive captured audio samples, blocking until available.
    pub fn recv(&self) -> Option<Vec<f32>> {
        self.receiver.recv().ok()
    }
}

/// Build an input stream for the given sample type.
fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    sender: Sender<Vec<f32>>,
    channels: usize,
    resample_ratio: f64,
) -> Result<Stream, Box<dyn std::error::Error>>
where
    T: cpal::Sample + cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let err_fn = |err| log::error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert to f32 and mix to mono
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|frame| {
                    let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                    sum / channels as f32
                })
                .collect();

            // Resample to 16kHz
            let resampled = resample(&mono, resample_ratio);

            if sender.send(resampled).is_err() {
                log::warn!("Audio receiver dropped");
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

/// Simple linear interpolation resampling.
/// For ratio < 1.0, this downsamples (e.g., 48kHz -> 16kHz).
/// For ratio > 1.0, this upsamples.
fn resample(samples: &[f32], ratio: f64) -> Vec<f32> {
    if (ratio - 1.0).abs() < 0.001 {
        return samples.to_vec();
    }

    let output_len = ((samples.len() as f64) * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 / ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(samples.len() - 1);
        let frac = src_idx - idx0 as f64;

        let sample = if idx0 < samples.len() {
            samples[idx0] * (1.0 - frac as f32) + samples[idx1] * frac as f32
        } else {
            0.0
        };

        output.push(sample);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_resample_downsample() {
        // 48kHz -> 16kHz = ratio of 1/3
        let input: Vec<f32> = (0..48).map(|i| i as f32).collect();
        let output = resample(&input, 16.0 / 48.0);

        // Should produce ~16 samples
        assert_eq!(output.len(), 16);
    }

    #[test]
    fn test_resample_no_change() {
        let input: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let output = resample(&input, 1.0);
        assert_eq!(output, input);
    }

    #[test]
    #[ignore] // Run manually: cargo test test_capture_audio -- --ignored --nocapture
    fn test_capture_audio() {
        let capture = AudioCapture::new().expect("Failed to create audio capture");
        capture.start().expect("Failed to start capture");

        println!("Recording for 2 seconds...");
        std::thread::sleep(Duration::from_secs(2));

        let mut total_samples = 0;
        while let Some(samples) = capture.try_recv() {
            total_samples += samples.len();
        }

        capture.stop().expect("Failed to stop capture");

        println!(
            "Captured {} samples (~{:.1}s at 16kHz)",
            total_samples,
            total_samples as f64 / 16000.0
        );
        assert!(total_samples > 0, "Should have captured some audio");
    }

    #[test]
    #[ignore] // Run manually: cargo test test_live_transcription -- --ignored --nocapture
    fn test_live_transcription() {
        use crate::whisper::StreamingTranscriber;

        println!("\n=== Live Transcription Test ===");
        println!("Speak into your microphone for 5 seconds...\n");

        let transcriber =
            StreamingTranscriber::new(Duration::from_secs(10)).expect("Failed to create transcriber");
        let capture = AudioCapture::new().expect("Failed to create audio capture");

        capture.start().expect("Failed to start capture");

        let start = Instant::now();
        let duration = Duration::from_secs(5);
        let transcribe_interval = Duration::from_millis(500);
        let mut last_transcribe = Instant::now();

        while start.elapsed() < duration {
            // Collect audio samples
            while let Some(samples) = capture.try_recv() {
                transcriber.push_audio(&samples);
            }

            // Run transcription periodically
            if last_transcribe.elapsed() >= transcribe_interval {
                match transcriber.transcribe() {
                    Ok(Some(text)) => {
                        println!("[{:.1}s] {}", start.elapsed().as_secs_f32(), text);
                    }
                    Ok(None) => {
                        // No change
                    }
                    Err(e) => {
                        println!("Transcription error: {}", e);
                    }
                }
                last_transcribe = Instant::now();
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        capture.stop().expect("Failed to stop capture");

        println!("\n=== Final transcript ===");
        println!("{}", transcriber.current_transcript());
        println!("=== Test complete ===\n");
    }
}
