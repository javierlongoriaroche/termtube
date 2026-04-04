use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

/// Number of FFT samples per window. Must be a power of 2.
const FFT_SIZE: usize = 2048;

/// Spectrum analyzer: takes PCM samples and produces frequency-band magnitudes.
pub struct SpectrumAnalyzer {
    fft_size: usize,
    num_bars: usize,
    decay: f64,
    sample_rate: u32,
    /// Current bar heights after smoothing (0.0..1.0).
    bars: Vec<f64>,
    /// Hann window coefficients, precomputed.
    window: Vec<f64>,
}

impl SpectrumAnalyzer {
    pub fn new(num_bars: usize, decay: f64, sample_rate: u32) -> Self {
        let window = hann_window(FFT_SIZE);
        Self {
            fft_size: FFT_SIZE,
            num_bars,
            decay: decay.clamp(0.0, 0.999),
            sample_rate,
            bars: vec![0.0; num_bars],
            window,
        }
    }

    /// Feed new PCM samples (mono or interleaved — caller should provide mono).
    /// Returns the current bar heights (0.0..1.0).
    pub fn process(&mut self, samples: &[f32]) -> &[f64] {
        if samples.len() < self.fft_size {
            // Apply decay even when there aren't enough samples
            for bar in &mut self.bars {
                *bar *= self.decay;
            }
            return &self.bars;
        }

        // Take the last fft_size samples
        let start = samples.len().saturating_sub(self.fft_size);
        let window_samples = &samples[start..start + self.fft_size];

        // Apply Hann window and convert to complex
        let mut buffer: Vec<Complex<f64>> = window_samples
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| Complex::new(s as f64 * w, 0.0))
            .collect();

        // Run FFT in-place
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(self.fft_size);
        fft.process(&mut buffer);

        // Compute magnitudes for the positive frequencies (first half)
        let half = self.fft_size / 2;
        let magnitudes: Vec<f64> = buffer[..half]
            .iter()
            .map(|c| c.norm() / half as f64)
            .collect();

        // Map frequency bins to bars using logarithmic scale
        let new_bars = map_to_bars(&magnitudes, self.num_bars, self.sample_rate, half);

        // Apply smoothing (decay): bar = max(new_value, old_value * decay)
        for (i, &new_val) in new_bars.iter().enumerate() {
            let decayed = self.bars[i] * self.decay;
            self.bars[i] = new_val.max(decayed);
        }

        &self.bars
    }

    /// Reset all bars to zero.
    pub fn reset(&mut self) {
        self.bars.fill(0.0);
    }

    pub fn num_bars(&self) -> usize {
        self.num_bars
    }
}

/// Map FFT magnitude bins to N bars using logarithmic frequency grouping.
/// Low frequencies get fewer bins per bar, high frequencies get more,
/// matching human perception (log scale on frequency axis).
fn map_to_bars(magnitudes: &[f64], num_bars: usize, sample_rate: u32, num_bins: usize) -> Vec<f64> {
    let mut bars = vec![0.0f64; num_bars];

    if num_bins == 0 || num_bars == 0 {
        return bars;
    }

    // Frequency range: ~20 Hz to Nyquist
    let freq_min = 20.0_f64;
    let freq_max = (sample_rate as f64 / 2.0).min(20_000.0);
    let log_min = freq_min.ln();
    let log_max = freq_max.ln();

    let bin_resolution = sample_rate as f64 / (num_bins as f64 * 2.0);

    for bar_idx in 0..num_bars {
        // Logarithmic frequency boundaries for this bar
        let t0 = bar_idx as f64 / num_bars as f64;
        let t1 = (bar_idx + 1) as f64 / num_bars as f64;
        let freq_lo = (log_min + t0 * (log_max - log_min)).exp();
        let freq_hi = (log_min + t1 * (log_max - log_min)).exp();

        // Convert to bin indices
        let bin_lo = ((freq_lo / bin_resolution) as usize).max(1);
        let bin_hi = ((freq_hi / bin_resolution) as usize).min(num_bins - 1);

        if bin_lo > bin_hi || bin_lo >= num_bins {
            continue;
        }

        // Average magnitude in this frequency range
        let mut sum = 0.0;
        let mut count = 0;
        for bin in bin_lo..=bin_hi {
            sum += magnitudes[bin];
            count += 1;
        }

        if count > 0 {
            // Convert to dB-like scale for better visual dynamic range
            let avg = sum / count as f64;
            // Apply a mild log scale to compress dynamic range
            let db = if avg > 1e-10 {
                (1.0 + avg * 500.0).log10() / 2.5
            } else {
                0.0
            };
            bars[bar_idx] = db.clamp(0.0, 1.0);
        }
    }

    bars
}

/// Generate a Hann window of the given size.
fn hann_window(size: usize) -> Vec<f64> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (size - 1) as f64).cos()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hann_window_endpoints() {
        let w = hann_window(1024);
        assert_eq!(w.len(), 1024);
        // Hann window is ~0 at the endpoints
        assert!(w[0].abs() < 1e-10);
        assert!(w[1023].abs() < 1e-10);
        // Peak at center
        assert!(w[512] > 0.99);
    }

    #[test]
    fn test_spectrum_analyzer_with_silence() {
        let mut analyzer = SpectrumAnalyzer::new(24, 0.85, 48000);
        let silence = vec![0.0f32; FFT_SIZE];
        let bars = analyzer.process(&silence);
        assert_eq!(bars.len(), 24);
        for &bar in bars {
            assert!(bar < 0.01, "silent input should produce near-zero bars");
        }
    }

    #[test]
    fn test_spectrum_analyzer_with_sine() {
        let mut analyzer = SpectrumAnalyzer::new(24, 0.85, 48000);
        // Generate a 440 Hz sine wave
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        let bars = analyzer.process(&samples);
        assert_eq!(bars.len(), 24);
        // At least one bar should have non-trivial energy
        let max_bar = bars.iter().cloned().fold(0.0_f64, f64::max);
        assert!(
            max_bar > 0.01,
            "440Hz sine should produce visible bars, got max={max_bar}"
        );
    }

    #[test]
    fn test_spectrum_analyzer_too_few_samples() {
        let mut analyzer = SpectrumAnalyzer::new(16, 0.85, 44100);
        let short = vec![0.5f32; 100]; // less than FFT_SIZE
        let bars = analyzer.process(&short);
        assert_eq!(bars.len(), 16);
        // Should not panic, just return decayed values
    }

    #[test]
    fn test_spectrum_analyzer_decay() {
        let mut analyzer = SpectrumAnalyzer::new(24, 0.5, 48000);
        // Feed a loud sine
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        analyzer.process(&samples);
        let bars_after_signal = analyzer.process(&samples).to_vec();

        // Now feed silence — bars should decay
        let silence = vec![0.0f32; FFT_SIZE];
        let bars_after_silence = analyzer.process(&silence).to_vec();

        let max_signal = bars_after_signal.iter().cloned().fold(0.0_f64, f64::max);
        let max_silence = bars_after_silence.iter().cloned().fold(0.0_f64, f64::max);
        assert!(
            max_silence < max_signal,
            "bars should decay after silence: signal={max_signal}, silence={max_silence}"
        );
    }

    #[test]
    fn test_map_to_bars_empty() {
        let bars = map_to_bars(&[], 24, 48000, 0);
        assert_eq!(bars.len(), 24);
    }

    #[test]
    fn test_reset() {
        let mut analyzer = SpectrumAnalyzer::new(16, 0.85, 48000);
        let samples: Vec<f32> = (0..FFT_SIZE)
            .map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        analyzer.process(&samples);
        analyzer.reset();
        for &bar in analyzer.process(&vec![0.0; 10]) {
            assert!(bar.abs() < 1e-10);
        }
    }
}
