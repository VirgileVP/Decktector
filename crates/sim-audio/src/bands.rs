//! Three-band split of the audio signal, so a waveform column can be
//! coloured the way a player colours it: bass blue, mids amber, highs
//! bright.
//!
//! Biquads rather than an FFT: the split feeds a 150-column-per-second
//! waveform and a beat detector, neither of which needs frequency
//! resolution finer than "is this a kick, a stab or a hat". Two crossovers
//! and four filters do that without pulling in a transform library.

/// Crossovers, in Hz. Below the first is kick-and-bass territory, above the
/// second is hats and air; the band between carries most of the musical
/// content a DJ reads as "the track".
const LOW_CROSSOVER_HZ: f32 = 200.0;
const HIGH_CROSSOVER_HZ: f32 = 2_000.0;
/// Butterworth Q: the flattest passband, which is what matters when the
/// output is an energy measurement rather than something anyone listens to.
const Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// One direct-form-I biquad section.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// Coefficients from the RBJ audio EQ cookbook, normalized by `a0`.
    fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn low_pass(frequency_hz: f32, sample_rate: f32) -> Self {
        let (cos, alpha) = Self::shape(frequency_hz, sample_rate);
        Self::new(
            (1.0 - cos) / 2.0,
            1.0 - cos,
            (1.0 - cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    fn high_pass(frequency_hz: f32, sample_rate: f32) -> Self {
        let (cos, alpha) = Self::shape(frequency_hz, sample_rate);
        Self::new(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    /// `(cos w0, alpha)` — the two terms the cookbook's low- and high-pass
    /// forms are written in.
    fn shape(frequency_hz: f32, sample_rate: f32) -> (f32, f32) {
        let w0 = 2.0 * std::f32::consts::PI * frequency_hz / sample_rate;
        let (sin, cos) = w0.sin_cos();
        (cos, sin / (2.0 * Q))
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Energy split of one sample into low, mid and high.
pub type BandSample = [f32; 3];

/// Splits a stream of samples, one at a time, keeping its own filter state.
pub struct BandSplitter {
    low: Biquad,
    mid_high_pass: Biquad,
    mid_low_pass: Biquad,
    high: Biquad,
}

impl BandSplitter {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        Self {
            low: Biquad::low_pass(LOW_CROSSOVER_HZ, rate),
            mid_high_pass: Biquad::high_pass(LOW_CROSSOVER_HZ, rate),
            mid_low_pass: Biquad::low_pass(HIGH_CROSSOVER_HZ, rate),
            high: Biquad::high_pass(HIGH_CROSSOVER_HZ, rate),
        }
    }

    pub fn process(&mut self, sample: f32) -> BandSample {
        let mid = self
            .mid_low_pass
            .process(self.mid_high_pass.process(sample));
        [self.low.process(sample), mid, self.high.process(sample)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 44_100;

    /// Root-mean-square of one band over a sine of `frequency_hz`, after
    /// letting the filters settle.
    fn band_rms(frequency_hz: f32, band: usize) -> f32 {
        let mut splitter = BandSplitter::new(SAMPLE_RATE);
        let step = 2.0 * std::f32::consts::PI * frequency_hz / SAMPLE_RATE as f32;
        let total = SAMPLE_RATE as usize;
        let settle = total / 2;
        let mut sum = 0.0;
        for i in 0..total {
            let value = splitter.process((i as f32 * step).sin())[band];
            if i >= settle {
                sum += value * value;
            }
        }
        (sum / (total - settle) as f32).sqrt()
    }

    #[test]
    fn each_band_answers_loudest_to_its_own_range() {
        let bass = [band_rms(60.0, 0), band_rms(60.0, 1), band_rms(60.0, 2)];
        let mid = [band_rms(800.0, 0), band_rms(800.0, 1), band_rms(800.0, 2)];
        let high = [
            band_rms(8_000.0, 0),
            band_rms(8_000.0, 1),
            band_rms(8_000.0, 2),
        ];

        assert!(bass[0] > bass[1] && bass[0] > bass[2], "{bass:?}");
        assert!(mid[1] > mid[0] && mid[1] > mid[2], "{mid:?}");
        assert!(high[2] > high[0] && high[2] > high[1], "{high:?}");
    }
}
