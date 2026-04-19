use rustfft::{FftPlanner, num_complex::Complex};

pub struct SpectrumAnalyzer {
    fft_size: usize,
    planner: FftPlanner<f32>,
    window: Vec<f32>,
    pub bins: Vec<f32>,
    pub peak_bins: Vec<f32>,
    decay_rate: f32,
    bar_fall_rate: f32,
    floor_db: f32,
    range_db: f32,
}

impl SpectrumAnalyzer {
    pub fn new(fft_size: usize) -> Self {
        let window: Vec<f32> = (0..fft_size)
            .map(|i| {
                let t = i as f32 / (fft_size - 1) as f32;
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * t).cos())
            })
            .collect();

        let bin_count = fft_size / 2;

        Self {
            fft_size,
            planner: FftPlanner::new(),
            window,
            bins: vec![0.0; bin_count],
            peak_bins: vec![0.0; bin_count],
            decay_rate: 0.985,
            bar_fall_rate: 0.88,
            floor_db: -80.0,
            range_db: 50.0,
        }
    }

    pub fn process(&mut self, samples: &[f32]) {
        if samples.len() < self.fft_size {
            // Gently decay existing bins toward zero (visible fade).
            for bin in &mut self.bins {
                *bin *= self.bar_fall_rate;
            }
            for (peak, current) in self.peak_bins.iter_mut().zip(self.bins.iter()) {
                if *current > *peak {
                    *peak = *current;
                } else {
                    *peak *= self.decay_rate;
                }
            }
            return;
        }

        let start = samples.len() - self.fft_size;
        let mut buffer: Vec<Complex<f32>> = samples[start..]
            .iter()
            .zip(self.window.iter())
            .map(|(&sample, &win)| Complex::new(sample * win, 0.0))
            .collect();

        let fft = self.planner.plan_fft_forward(self.fft_size);
        fft.process(&mut buffer);

        let bin_count = self.fft_size / 2;
        let scale = 2.0 / self.fft_size as f32;

        // Convert to dB and find the peak
        let mut db_values = vec![self.floor_db; bin_count];
        let mut max_db = self.floor_db;

        for idx in 0..bin_count {
            let magnitude = buffer[idx].norm() * scale;
            let db = 20.0 * (magnitude + 1e-10).log10();
            db_values[idx] = db;
            if db > max_db {
                max_db = db;
            }
        }

        // Auto-range: the ceiling follows the peak, floor is ceiling - range_db
        let ceiling = (max_db + 3.0).max(-10.0);
        let floor = ceiling - self.range_db;

        for idx in 0..bin_count {
            let normalized = ((db_values[idx] - floor) / (ceiling - floor)).clamp(0.0, 1.0);
            // Apply a sqrt curve to boost quieter values visually
            let target = normalized.sqrt();
            // Fast attack: snap up instantly. Slow decay: glide down by fall_rate.
            self.bins[idx] = target.max(self.bins[idx] * self.bar_fall_rate);
        }

        for (peak, current) in self.peak_bins.iter_mut().zip(self.bins.iter()) {
            if *current > *peak {
                *peak = *current;
            } else {
                *peak *= self.decay_rate;
            }
        }
    }

    pub fn frequency_at_bin(&self, bin: usize, sample_rate: u32) -> f32 {
        bin as f32 * sample_rate as f32 / self.fft_size as f32
    }

    pub fn get_bar_values(&self, bar_count: usize, sample_rate: u32) -> Vec<f32> {
        self.map_bins_to_bars(&self.bins, bar_count, sample_rate)
    }

    pub fn get_peak_values(&self, bar_count: usize, sample_rate: u32) -> Vec<f32> {
        self.map_bins_to_bars(&self.peak_bins, bar_count, sample_rate)
    }

    fn map_bins_to_bars(&self, bins: &[f32], bar_count: usize, sample_rate: u32) -> Vec<f32> {
        if bins.is_empty() || bar_count == 0 {
            return vec![0.0; bar_count];
        }

        let max_freq = 12000.0_f32;

        (0..bar_count)
            .map(|bar_idx| {
                let freq_low = bar_idx as f32 / bar_count as f32 * max_freq;
                let freq_high = (bar_idx + 1) as f32 / bar_count as f32 * max_freq;

                let bin_low = (freq_low / sample_rate as f32 * self.fft_size as f32)
                    .floor() as usize;
                let bin_high = (freq_high / sample_rate as f32 * self.fft_size as f32)
                    .ceil() as usize;

                let bin_low = bin_low.clamp(0, bins.len().saturating_sub(1));
                let bin_high = bin_high.clamp(bin_low + 1, bins.len());

                bins[bin_low..bin_high]
                    .iter()
                    .copied()
                    .fold(0.0_f32, f32::max)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_zeroed_bins() {
        let analyzer = SpectrumAnalyzer::new(1024);
        assert_eq!(analyzer.bins.len(), 512);
        assert!(analyzer.bins.iter().all(|&b| b == 0.0));
    }

    #[test]
    fn test_process_with_silence() {
        let mut analyzer = SpectrumAnalyzer::new(1024);
        let silence = vec![0.0_f32; 1024];
        analyzer.process(&silence);
        assert!(analyzer.bins.iter().all(|&b| b < 0.1));
    }

    #[test]
    fn test_process_with_sine() {
        let mut analyzer = SpectrumAnalyzer::new(1024);
        let sample_rate = 44100.0;
        let freq = 440.0;
        let samples: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect();
        analyzer.process(&samples);

        let peak_bin = analyzer.bins.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(idx, _)| idx)
            .unwrap();

        let peak_freq = analyzer.frequency_at_bin(peak_bin, 44100);
        assert!((peak_freq - 440.0).abs() < 50.0);
    }

    #[test]
    fn test_get_bar_values_correct_count() {
        let analyzer = SpectrumAnalyzer::new(1024);
        let bars = analyzer.get_bar_values(32, 44100);
        assert_eq!(bars.len(), 32);
    }

    #[test]
    fn test_frequency_at_bin() {
        let analyzer = SpectrumAnalyzer::new(1024);
        let freq = analyzer.frequency_at_bin(10, 44100);
        let expected = 10.0 * 44100.0 / 1024.0;
        assert!((freq - expected).abs() < 0.01);
    }

    #[test]
    fn test_frequency_at_bin_zero() {
        let analyzer = SpectrumAnalyzer::new(1024);
        assert_eq!(analyzer.frequency_at_bin(0, 44100), 0.0);
    }

    #[test]
    fn test_get_bar_values_empty_bins() {
        let analyzer = SpectrumAnalyzer::new(1024);
        let bars = analyzer.get_bar_values(0, 44100);
        assert!(bars.is_empty());
    }

    #[test]
    fn test_get_peak_values_correct_count() {
        let analyzer = SpectrumAnalyzer::new(1024);
        let peaks = analyzer.get_peak_values(64, 44100);
        assert_eq!(peaks.len(), 64);
    }

    #[test]
    fn test_process_short_input_decays() {
        let mut analyzer = SpectrumAnalyzer::new(1024);
        let loud: Vec<f32> = (0..1024)
            .map(|idx| (2.0 * std::f32::consts::PI * 440.0 * idx as f32 / 44100.0).sin())
            .collect();
        analyzer.process(&loud);
        let max_before = analyzer.bins.iter().copied().fold(0.0_f32, f32::max);

        analyzer.process(&[0.0; 100]); // shorter than fft_size triggers decay
        let max_after = analyzer.bins.iter().copied().fold(0.0_f32, f32::max);
        assert!(max_after < max_before, "bins should decay with short input");
    }

    #[test]
    fn test_peaks_decay() {
        let mut analyzer = SpectrumAnalyzer::new(1024);
        let loud: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        analyzer.process(&loud);
        let peak_after_loud = analyzer.peak_bins.iter().copied().fold(0.0_f32, f32::max);

        let silence = vec![0.0_f32; 1024];
        for _ in 0..50 {
            analyzer.process(&silence);
        }
        let peak_after_silence = analyzer.peak_bins.iter().copied().fold(0.0_f32, f32::max);
        assert!(peak_after_silence < peak_after_loud);
    }
}
