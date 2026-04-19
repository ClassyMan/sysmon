use std::time::Duration;

use anyhow::Result;

use crate::capture::AudioCapture;
use crate::spectrum::SpectrumAnalyzer;

pub const FFT_SIZE: usize = 4096;
pub const REFRESH_MS: u64 = 33;

pub struct App {
    pub capture: AudioCapture,
    pub analyzer: SpectrumAnalyzer,
}

impl App {
    pub fn new() -> Result<Self> {
        let capture = AudioCapture::start_monitor()?;
        let analyzer = SpectrumAnalyzer::new(FFT_SIZE);
        Ok(Self { capture, analyzer })
    }

    pub fn tick(&mut self) {
        let samples = self.capture.take_samples(FFT_SIZE);
        self.analyzer.process(&samples);
    }

    pub fn refresh_rate(&self) -> Duration {
        Duration::from_millis(REFRESH_MS)
    }

    pub fn sample_rate(&self) -> u32 {
        self.capture.sample_rate
    }

    pub fn device_name(&self) -> String {
        self.capture.device_name()
    }

    pub fn capture_error(&self) -> Option<String> {
        self.capture.error()
    }

    pub fn buffer_len(&self) -> usize {
        self.capture.buffer_len()
    }

    pub fn peak_amplitude(&self) -> f32 {
        self.capture.peak_amplitude()
    }
}
