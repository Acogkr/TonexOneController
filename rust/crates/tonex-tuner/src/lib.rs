#![no_std]

//! Allocation-free pitch analysis for a future verified audio input adapter.
//!
//! This crate deliberately does not select an ADC or USB audio source. The
//! product firmware may only feed it real samples after that hardware boundary
//! has been verified. Keeping detection independent makes synthetic accuracy
//! tests possible without presenting fabricated tuner data to the player.

const MAX_LAG: usize = 1_200;
const MAX_FRAME_SAMPLES: usize = 4_096;
const MIN_FRAME_SAMPLES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerConfig {
    pub sample_rate_hz: u16,
    pub minimum_frequency_hz: f32,
    pub maximum_frequency_hz: f32,
    pub yin_threshold: f32,
    pub minimum_rms: f32,
    pub reference_hz: f32,
}

impl Default for TunerConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16_000,
            minimum_frequency_hz: 40.0,
            maximum_frequency_hz: 1_200.0,
            yin_threshold: 0.15,
            minimum_rms: 0.005,
            reference_hz: 440.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PitchClass {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerReading {
    pub frequency_hz: f32,
    pub target_frequency_hz: f32,
    pub cents: f32,
    pub midi_note: u8,
    pub pitch_class: PitchClass,
    pub octave: i8,
    pub confidence: f32,
    pub rms: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetectError {
    InvalidConfig,
    InsufficientSamples,
    WeakSignal,
    UnstablePitch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothingConfig {
    /// Weight of the newest frame while the MIDI note remains unchanged.
    pub new_frame_weight: f32,
    /// Number of weak/unstable frames that may retain the last reading.
    pub hold_frames: u8,
}

impl Default for SmoothingConfig {
    fn default() -> Self {
        Self {
            new_frame_weight: 0.35,
            hold_frames: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackedPitch {
    Stable(TunerReading),
    /// A short dropout is displaying the last stable reading temporarily.
    Holding(TunerReading),
    WeakSignal,
    UnstablePitch,
    Fault(DetectError),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PitchSmoother {
    reading: Option<TunerReading>,
    missed_frames: u8,
}

impl PitchSmoother {
    /// Smooths repeated readings without delaying a genuine note change.
    ///
    /// Same-note frames use an exponential moving average. A different MIDI
    /// note replaces the previous state immediately, while a bounded number
    /// of weak or unstable frames may hold the last display value.
    #[must_use]
    pub fn update(
        &mut self,
        detected: Result<TunerReading, DetectError>,
        config: SmoothingConfig,
    ) -> TrackedPitch {
        if !config.new_frame_weight.is_finite()
            || !(0.0..=1.0).contains(&config.new_frame_weight)
            || config.new_frame_weight == 0.0
        {
            self.reset();
            return TrackedPitch::Fault(DetectError::InvalidConfig);
        }
        match detected {
            Ok(next) => {
                let reading = self.reading.map_or(next, |previous| {
                    if previous.midi_note == next.midi_note {
                        smooth_reading(previous, next, config.new_frame_weight)
                    } else {
                        next
                    }
                });
                self.reading = Some(reading);
                self.missed_frames = 0;
                TrackedPitch::Stable(reading)
            }
            Err(error @ (DetectError::WeakSignal | DetectError::UnstablePitch)) => {
                self.missed_frames = self.missed_frames.saturating_add(1);
                if self.missed_frames <= config.hold_frames
                    && let Some(reading) = self.reading
                {
                    return TrackedPitch::Holding(reading);
                }
                self.reading = None;
                match error {
                    DetectError::WeakSignal => TrackedPitch::WeakSignal,
                    DetectError::UnstablePitch => TrackedPitch::UnstablePitch,
                    DetectError::InvalidConfig | DetectError::InsufficientSamples => unreachable!(),
                }
            }
            Err(error @ (DetectError::InvalidConfig | DetectError::InsufficientSamples)) => {
                self.reset();
                TrackedPitch::Fault(error)
            }
        }
    }

    pub fn reset(&mut self) {
        self.reading = None;
        self.missed_frames = 0;
    }
}

fn smooth_reading(previous: TunerReading, next: TunerReading, weight: f32) -> TunerReading {
    let blend = |old: f32, new: f32| old + (new - old) * weight;
    TunerReading {
        frequency_hz: blend(previous.frequency_hz, next.frequency_hz),
        target_frequency_hz: next.target_frequency_hz,
        cents: blend(previous.cents, next.cents),
        midi_note: next.midi_note,
        pitch_class: next.pitch_class,
        octave: next.octave,
        confidence: blend(previous.confidence, next.confidence),
        rms: blend(previous.rms, next.rms),
    }
}

#[derive(Debug)]
pub struct YinDetector {
    normalized_difference: [f32; MAX_LAG + 1],
}

impl Default for YinDetector {
    fn default() -> Self {
        Self {
            normalized_difference: [0.0; MAX_LAG + 1],
        }
    }
}

impl YinDetector {
    /// Detects one stable fundamental from a mono, normalized `-1.0..=1.0`
    /// sample frame.
    ///
    /// # Errors
    ///
    /// Returns a typed error for invalid bounds, a short frame, a weak input,
    /// or a frame without a sufficiently periodic candidate.
    pub fn detect(
        &mut self,
        samples: &[f32],
        config: TunerConfig,
    ) -> Result<TunerReading, DetectError> {
        let (minimum_lag, maximum_lag) = validate(samples, config)?;
        let rms = signal_rms(samples);
        if rms < config.minimum_rms {
            return Err(DetectError::WeakSignal);
        }

        self.normalized_difference[0] = 1.0;
        let mut running_sum = 0.0;
        for lag in 1..=maximum_lag {
            let difference = squared_difference(samples, lag);
            running_sum += difference;
            self.normalized_difference[lag] = if running_sum > f32::EPSILON {
                difference * bounded_usize_as_f32(lag) / running_sum
            } else {
                1.0
            };
        }

        let candidate = select_candidate(
            &self.normalized_difference,
            minimum_lag,
            maximum_lag,
            config.yin_threshold,
        )
        .ok_or(DetectError::UnstablePitch)?;
        let period = interpolated_period(&self.normalized_difference, candidate, maximum_lag);
        let frequency_hz = f32::from(config.sample_rate_hz) / period;
        let confidence = (1.0 - self.normalized_difference[candidate]).clamp(0.0, 1.0);
        reading_from_frequency(frequency_hz, config.reference_hz, confidence, rms)
    }
}

fn validate(samples: &[f32], config: TunerConfig) -> Result<(usize, usize), DetectError> {
    if !(MIN_FRAME_SAMPLES..=MAX_FRAME_SAMPLES).contains(&samples.len())
        || !config.minimum_frequency_hz.is_finite()
        || !config.maximum_frequency_hz.is_finite()
        || !config.reference_hz.is_finite()
        || config.sample_rate_hz == 0
        || config.minimum_frequency_hz <= 0.0
        || config.maximum_frequency_hz <= config.minimum_frequency_hz
        || config.reference_hz <= 0.0
        || !(0.0..1.0).contains(&config.yin_threshold)
        || config.minimum_rms < 0.0
    {
        return Err(DetectError::InvalidConfig);
    }
    let sample_rate = f32::from(config.sample_rate_hz);
    let mut minimum_lag = 2;
    while minimum_lag < MAX_LAG
        && sample_rate / bounded_usize_as_f32(minimum_lag) > config.maximum_frequency_hz
    {
        minimum_lag += 1;
    }
    let mut maximum_lag = minimum_lag + 1;
    while maximum_lag < MAX_LAG
        && sample_rate / bounded_usize_as_f32(maximum_lag + 1) >= config.minimum_frequency_hz
    {
        maximum_lag += 1;
    }
    let maximum_lag = maximum_lag.min(samples.len() / 2).min(MAX_LAG);
    if minimum_lag + 2 >= maximum_lag {
        return Err(DetectError::InsufficientSamples);
    }
    Ok((minimum_lag, maximum_lag))
}

fn signal_rms(samples: &[f32]) -> f32 {
    let sample_count = bounded_usize_as_f32(samples.len());
    let mean = samples.iter().copied().sum::<f32>() / sample_count;
    let energy = samples
        .iter()
        .map(|sample| {
            let centered = sample - mean;
            centered * centered
        })
        .sum::<f32>();
    libm::sqrtf(energy / sample_count)
}

fn squared_difference(samples: &[f32], lag: usize) -> f32 {
    samples
        .iter()
        .zip(&samples[lag..])
        .map(|(left, right)| {
            let delta = left - right;
            delta * delta
        })
        .sum()
}

fn select_candidate(
    difference: &[f32; MAX_LAG + 1],
    minimum_lag: usize,
    maximum_lag: usize,
    threshold: f32,
) -> Option<usize> {
    let mut lag = minimum_lag;
    while lag < maximum_lag {
        if difference[lag] < threshold {
            while lag < maximum_lag && difference[lag + 1] < difference[lag] {
                lag += 1;
            }
            return Some(lag);
        }
        lag += 1;
    }
    let candidate = (minimum_lag..=maximum_lag)
        .min_by(|left, right| difference[*left].total_cmp(&difference[*right]))?;
    (difference[candidate] < 0.35).then_some(candidate)
}

fn interpolated_period(difference: &[f32; MAX_LAG + 1], lag: usize, maximum_lag: usize) -> f32 {
    if lag <= 1 || lag >= maximum_lag {
        return bounded_usize_as_f32(lag);
    }
    let left = difference[lag - 1];
    let center = difference[lag];
    let right = difference[lag + 1];
    let denominator = left - 2.0 * center + right;
    if denominator.abs() <= f32::EPSILON {
        bounded_usize_as_f32(lag)
    } else {
        bounded_usize_as_f32(lag) + (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
    }
}

fn reading_from_frequency(
    frequency_hz: f32,
    reference_hz: f32,
    confidence: f32,
    rms: f32,
) -> Result<TunerReading, DetectError> {
    if !frequency_hz.is_finite() || frequency_hz <= 0.0 {
        return Err(DetectError::UnstablePitch);
    }
    let midi_note = (0_u8..=127)
        .min_by(|left, right| {
            cents_from_target(frequency_hz, reference_hz, *left)
                .abs()
                .total_cmp(&cents_from_target(frequency_hz, reference_hz, *right).abs())
        })
        .ok_or(DetectError::UnstablePitch)?;
    let target_frequency_hz = reference_hz * libm::powf(2.0, (f32::from(midi_note) - 69.0) / 12.0);
    let cents = 1_200.0 * libm::log2f(frequency_hz / target_frequency_hz);
    Ok(TunerReading {
        frequency_hz,
        target_frequency_hz,
        cents,
        midi_note,
        pitch_class: pitch_class(midi_note),
        octave: i8::try_from(midi_note / 12).expect("MIDI octave fits i8") - 1,
        confidence,
        rms,
    })
}

fn cents_from_target(frequency_hz: f32, reference_hz: f32, midi_note: u8) -> f32 {
    let target = reference_hz * libm::powf(2.0, (f32::from(midi_note) - 69.0) / 12.0);
    1_200.0 * libm::log2f(frequency_hz / target)
}

fn bounded_usize_as_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("tuner bounds fit u16"))
}

const fn pitch_class(midi_note: u8) -> PitchClass {
    match midi_note % 12 {
        0 => PitchClass::C,
        1 => PitchClass::CSharp,
        2 => PitchClass::D,
        3 => PitchClass::DSharp,
        4 => PitchClass::E,
        5 => PitchClass::F,
        6 => PitchClass::FSharp,
        7 => PitchClass::G,
        8 => PitchClass::GSharp,
        9 => PitchClass::A,
        10 => PitchClass::ASharp,
        _ => PitchClass::B,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec;
    use std::vec::Vec;

    const SAMPLE_RATE: u16 = 16_000;
    const FRAME_SAMPLES: usize = 2_048;

    fn sine(frequency: f32, amplitude: f32) -> Vec<f32> {
        (0..FRAME_SAMPLES)
            .map(|index| {
                amplitude
                    * libm::sinf(
                        2.0 * core::f32::consts::PI * frequency * bounded_usize_as_f32(index)
                            / f32::from(SAMPLE_RATE),
                    )
            })
            .collect()
    }

    fn detect(samples: &[f32]) -> TunerReading {
        YinDetector::default()
            .detect(samples, TunerConfig::default())
            .expect("synthetic pitch is detected")
    }

    #[test]
    fn standard_guitar_and_a4_notes_are_accurate() {
        for (frequency, midi) in [
            (82.41, 40),
            (110.00, 45),
            (146.83, 50),
            (196.00, 55),
            (246.94, 59),
            (329.63, 64),
            (440.00, 69),
        ] {
            let reading = detect(&sine(frequency, 0.7));
            assert_eq!(reading.midi_note, midi, "frequency {frequency}");
            assert!(reading.cents.abs() < 1.0, "{reading:?}");
            assert!(reading.confidence > 0.9, "{reading:?}");
        }
    }

    #[test]
    fn reports_flat_and_sharp_offsets() {
        for cents in [-17.0_f32, 23.0] {
            let frequency = 440.0 * libm::powf(2.0, cents / 1_200.0);
            let reading = detect(&sine(frequency, 0.65));
            assert!((reading.cents - cents).abs() < 1.0, "{reading:?}");
        }
    }

    #[test]
    fn tolerates_harmonics_and_deterministic_noise() {
        let mut samples = sine(110.0, 0.65);
        let harmonic = sine(220.0, 0.30);
        let mut random = 0x1234_5678_u32;
        for (sample, harmonic) in samples.iter_mut().zip(harmonic) {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise_word = u16::try_from(random >> 16).expect("upper word fits u16");
            let noise = (f32::from(noise_word) / 65_535.0 - 0.5) * 0.08;
            *sample += harmonic + noise;
        }
        let reading = detect(&samples);
        assert_eq!(reading.midi_note, 45);
        assert!(reading.cents.abs() < 2.0, "{reading:?}");
        assert!(reading.confidence > 0.75, "{reading:?}");
    }

    #[test]
    fn weak_or_non_periodic_input_is_not_presented_as_a_note() {
        let mut detector = YinDetector::default();
        assert_eq!(
            detector.detect(&vec![0.0; FRAME_SAMPLES], TunerConfig::default()),
            Err(DetectError::WeakSignal)
        );

        let mut random = 0x8765_4321_u32;
        let noise: Vec<f32> = (0..FRAME_SAMPLES)
            .map(|_| {
                random = random.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let noise_word = u16::try_from(random >> 16).expect("upper word fits u16");
                (f32::from(noise_word) / 65_535.0 - 0.5) * 0.7
            })
            .collect();
        assert_eq!(
            detector.detect(&noise, TunerConfig::default()),
            Err(DetectError::UnstablePitch)
        );
    }

    #[test]
    fn low_bass_and_fast_note_changes_are_independent() {
        let mut detector = YinDetector::default();
        let low_b = detector
            .detect(&sine(61.74, 0.7), TunerConfig::default())
            .expect("B1 is detected");
        let high_e = detector
            .detect(&sine(329.63, 0.7), TunerConfig::default())
            .expect("E4 is detected after B1");
        assert_eq!((low_b.pitch_class, low_b.octave), (PitchClass::B, 1));
        assert_eq!((high_e.pitch_class, high_e.octave), (PitchClass::E, 4));
    }

    #[test]
    fn smoother_reduces_same_note_jitter_without_delaying_note_changes() {
        let mut smoother = PitchSmoother::default();
        let config = SmoothingConfig {
            new_frame_weight: 0.25,
            hold_frames: 1,
        };
        let sharp_frequency = 440.0 * libm::powf(2.0, 10.0 / 1_200.0);
        let flat_frequency = 440.0 * libm::powf(2.0, -10.0 / 1_200.0);
        let sharp =
            reading_from_frequency(sharp_frequency, 440.0, 0.95, 0.4).expect("sharp A4 reading");
        let flat =
            reading_from_frequency(flat_frequency, 440.0, 0.90, 0.3).expect("flat A4 reading");
        assert!(matches!(
            smoother.update(Ok(sharp), config),
            TrackedPitch::Stable(reading) if (reading.cents - 10.0).abs() < 0.01
        ));
        assert!(matches!(
            smoother.update(Ok(flat), config),
            TrackedPitch::Stable(reading) if (reading.cents - 5.0).abs() < 0.02
        ));

        let e4 = reading_from_frequency(329.63, 440.0, 0.97, 0.5).expect("E4 reading");
        assert!(matches!(
            smoother.update(Ok(e4), config),
            TrackedPitch::Stable(reading)
                if reading.midi_note == 64 && (reading.frequency_hz - e4.frequency_hz).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn smoother_holds_only_the_configured_number_of_dropouts() {
        let mut smoother = PitchSmoother::default();
        let config = SmoothingConfig {
            new_frame_weight: 0.35,
            hold_frames: 1,
        };
        let a4 = reading_from_frequency(440.0, 440.0, 0.98, 0.5).expect("A4 reading");
        assert!(matches!(
            smoother.update(Ok(a4), config),
            TrackedPitch::Stable(_)
        ));
        assert!(matches!(
            smoother.update(Err(DetectError::WeakSignal), config),
            TrackedPitch::Holding(reading) if reading.midi_note == 69
        ));
        assert_eq!(
            smoother.update(Err(DetectError::WeakSignal), config),
            TrackedPitch::WeakSignal
        );
        assert_eq!(
            smoother.update(Err(DetectError::UnstablePitch), config),
            TrackedPitch::UnstablePitch
        );
    }

    #[test]
    fn invalid_smoothing_configuration_clears_held_state() {
        let mut smoother = PitchSmoother::default();
        let a4 = reading_from_frequency(440.0, 440.0, 0.98, 0.5).expect("A4 reading");
        let _ = smoother.update(Ok(a4), SmoothingConfig::default());
        assert_eq!(
            smoother.update(
                Ok(a4),
                SmoothingConfig {
                    new_frame_weight: 0.0,
                    hold_frames: 2,
                },
            ),
            TrackedPitch::Fault(DetectError::InvalidConfig)
        );
        assert_eq!(
            smoother.update(Err(DetectError::WeakSignal), SmoothingConfig::default()),
            TrackedPitch::WeakSignal
        );
    }
}
