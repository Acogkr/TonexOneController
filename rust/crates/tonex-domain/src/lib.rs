#![no_std]

pub const PRESET_COUNT: usize = 20;
const PRESET_COUNT_U8: u8 = 20;
pub const PRESET_NAME_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidPresetIndex(pub u8);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PresetIndex(u8);

impl PresetIndex {
    pub const FIRST: Self = Self(0);

    /// Creates a validated TONEX ONE preset index.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPresetIndex`] when `value` is outside `0..20`.
    pub const fn new(value: u8) -> Result<Self, InvalidPresetIndex> {
        if (value as usize) < PRESET_COUNT {
            Ok(Self(value))
        } else {
            Err(InvalidPresetIndex(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn next_wrapping(self) -> Self {
        Self((self.0 + 1) % PRESET_COUNT_U8)
    }

    #[must_use]
    pub const fn previous_wrapping(self) -> Self {
        if self.0 == 0 {
            Self(PRESET_COUNT_U8 - 1)
        } else {
            Self(self.0 - 1)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    A,
    B,
    C,
}

pub const FOOT_BUTTON_COUNT: usize = 4;
pub const FOOT_GESTURE_COUNT: usize = 2;
pub const FOOT_BINDING_COUNT: usize = FOOT_BUTTON_COUNT * FOOT_GESTURE_COUNT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootButton {
    One,
    Two,
    Three,
    Four,
}

impl FootButton {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::One => 0,
            Self::Two => 1,
            Self::Three => 2,
            Self::Four => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootGesture {
    ShortPress,
    LongPress,
}

impl FootGesture {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::ShortPress => 0,
            Self::LongPress => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectBlock {
    Gate,
    Compressor,
    Amp,
    Cabinet,
    Modulation,
    Delay,
    Reverb,
}

/// A logical controller action. Bluetooth MIDI bytes are normalized into
/// buttons first, so changing a Chocolate preset never leaks raw CC values
/// into the TONEX protocol layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootAction {
    /// Only valid in a preset override; use the global binding.
    Inherit,
    Disabled,
    SelectSlot(Slot),
    PreviousPreset,
    NextPreset,
    PreviousAbBank,
    NextAbBank,
    SelectPreset(PresetIndex),
    TapTempo,
    ToggleBypass,
    /// Temporarily attenuate TONEX ONE master volume and restore it later.
    ToggleMute,
    ToggleEffect(EffectBlock),
}

impl FootAction {
    #[must_use]
    pub const fn encode(self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::SelectSlot(Slot::A) => 1,
            Self::SelectSlot(Slot::B) => 2,
            Self::SelectSlot(Slot::C) => 3,
            Self::PreviousPreset => 4,
            Self::NextPreset => 5,
            Self::TapTempo => 6,
            Self::ToggleBypass => 7,
            Self::ToggleEffect(EffectBlock::Gate) => 8,
            Self::ToggleEffect(EffectBlock::Compressor) => 9,
            Self::ToggleEffect(EffectBlock::Amp) => 10,
            Self::ToggleEffect(EffectBlock::Cabinet) => 11,
            Self::ToggleEffect(EffectBlock::Modulation) => 12,
            Self::ToggleEffect(EffectBlock::Delay) => 13,
            Self::ToggleEffect(EffectBlock::Reverb) => 14,
            Self::ToggleMute => 15,
            Self::PreviousAbBank => 16,
            Self::NextAbBank => 17,
            Self::SelectPreset(preset) => 32 + preset.get(),
            Self::Inherit => u8::MAX,
        }
    }

    #[must_use]
    pub const fn decode(encoded: u8) -> Option<Self> {
        Some(match encoded {
            0 => Self::Disabled,
            1 => Self::SelectSlot(Slot::A),
            2 => Self::SelectSlot(Slot::B),
            3 => Self::SelectSlot(Slot::C),
            4 => Self::PreviousPreset,
            5 => Self::NextPreset,
            6 => Self::TapTempo,
            7 => Self::ToggleBypass,
            8 => Self::ToggleEffect(EffectBlock::Gate),
            9 => Self::ToggleEffect(EffectBlock::Compressor),
            10 => Self::ToggleEffect(EffectBlock::Amp),
            11 => Self::ToggleEffect(EffectBlock::Cabinet),
            12 => Self::ToggleEffect(EffectBlock::Modulation),
            13 => Self::ToggleEffect(EffectBlock::Delay),
            14 => Self::ToggleEffect(EffectBlock::Reverb),
            15 => Self::ToggleMute,
            16 => Self::PreviousAbBank,
            17 => Self::NextAbBank,
            32..=51 => match PresetIndex::new(encoded - 32) {
                Ok(preset) => Self::SelectPreset(preset),
                Err(_) => return None,
            },
            u8::MAX => Self::Inherit,
            _ => return None,
        })
    }
}

#[must_use]
pub const fn foot_binding_index(button: FootButton, gesture: FootGesture) -> usize {
    button.index() * FOOT_GESTURE_COUNT + gesture.index()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Synchronizing,
    Ready,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncState {
    NotStarted,
    InProgress { completed: u8 },
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TapTempo {
    last_tap_ms: Option<u64>,
    bpm: Option<f32>,
}

impl TapTempo {
    pub const MIN_BPM: f32 = 40.0;
    pub const MAX_BPM: f32 = 240.0;
    const MAX_INTERVAL_MS: u64 = 1_500;
    const MIN_INTERVAL_MS: u64 = 250;

    #[must_use]
    pub const fn bpm(&self) -> Option<f32> {
        self.bpm
    }

    /// Records a monotonic millisecond timestamp.
    ///
    /// The first tap, or a tap more than 1500 ms after the previous one,
    /// establishes a new origin and returns no BPM. Faster-than-250 ms input
    /// is clamped to 240 BPM, matching the reference firmware.
    pub fn tap(&mut self, now_ms: u64) -> Option<f32> {
        let previous = self.last_tap_ms.replace(now_ms)?;
        let interval = now_ms.saturating_sub(previous);
        if interval > Self::MAX_INTERVAL_MS {
            return None;
        }
        let clamped = interval.max(Self::MIN_INTERVAL_MS);
        let clamped = u16::try_from(clamped).ok()?;
        let bpm = 60_000.0 / f32::from(clamped);
        self.bpm = Some(bpm);
        Some(bpm)
    }

    pub fn reset(&mut self) {
        self.last_tap_ms = None;
        self.bpm = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_index_rejects_out_of_range_values() {
        assert_eq!(PresetIndex::new(19).expect("19 is valid").get(), 19);
        assert_eq!(PresetIndex::new(20), Err(InvalidPresetIndex(20)));
        assert_eq!(PresetIndex::new(u8::MAX), Err(InvalidPresetIndex(u8::MAX)));
    }

    #[test]
    fn preset_navigation_wraps_at_both_edges() {
        assert_eq!(PresetIndex::FIRST.previous_wrapping().get(), 19);
        assert_eq!(
            PresetIndex::new(19)
                .expect("19 is valid")
                .next_wrapping()
                .get(),
            0
        );
    }

    #[test]
    fn tap_tempo_matches_reference_range_and_timeout() {
        let mut tap = TapTempo::default();
        assert_eq!(tap.tap(1_000), None);
        assert_eq!(tap.tap(1_500), Some(120.0));
        assert_eq!(tap.tap(1_600), Some(TapTempo::MAX_BPM));
        assert_eq!(tap.tap(3_101), None);
        assert_eq!(tap.tap(4_601), Some(TapTempo::MIN_BPM));
    }

    #[test]
    fn tap_tempo_handles_non_monotonic_clock_without_dividing_by_zero() {
        let mut tap = TapTempo::default();
        assert_eq!(tap.tap(1_000), None);
        assert_eq!(tap.tap(900), Some(TapTempo::MAX_BPM));
        tap.reset();
        assert_eq!(tap.bpm(), None);
    }
}
