#![no_std]

mod mapping;

use tonex_application::AppCommand;
use tonex_domain::{EffectBlock, FootAction, FootButton, FootGesture, PresetIndex, Slot};
use tonex_parameters::{
    ParameterId, ParameterKind, TONEX_GLOBAL_BYPASS, TONEX_GLOBAL_CABSIM_BYPASS,
    TONEX_GLOBAL_MASTER_VOLUME, TONEX_PARAM_COMP_ENABLE, TONEX_PARAM_DELAY_ENABLE,
    TONEX_PARAM_MODEL_AMP_ENABLE, TONEX_PARAM_MODULATION_ENABLE, TONEX_PARAM_NOISE_GATE_ENABLE,
    TONEX_PARAM_REVERB_ENABLE, scale_midi_range, spec,
};

pub const CC_TAP_TEMPO: u8 = 10;
pub const CC_BYPASS: u8 = 12;
pub const CC_LOAD_SLOT_A: u8 = 120;
pub const CC_LOAD_SLOT_B: u8 = 121;
pub const CC_MASTER_VOLUME: u8 = 122;
pub const CC_SELECT_PRESET: u8 = 127;
pub const BOOLEAN_OFF: u8 = 0;
pub const BOOLEAN_TOGGLE: u8 = 64;
pub const BOOLEAN_ON: u8 = 127;
pub const FOOT_LONG_PRESS_MS: u64 = 650;
pub const FOOT_DEBOUNCE_MS: u64 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FootPressState {
    pressed_at_ms: u64,
    last_edge_ms: u64,
    pressed: bool,
    long_emitted: bool,
}

impl FootPressState {
    const NEW: Self = Self {
        pressed_at_ms: 0,
        last_edge_ms: 0,
        pressed: false,
        long_emitted: false,
    };
}

/// Turns momentary press/release edges into one short- or long-press gesture.
/// A long press fires once at the threshold and suppresses the short action on
/// release. This state machine is transport-independent and can be tested
/// without a Bluetooth radio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FootPressTracker {
    states: [FootPressState; 4],
}

impl Default for FootPressTracker {
    fn default() -> Self {
        Self {
            states: [FootPressState::NEW; 4],
        }
    }
}

impl FootPressTracker {
    pub fn edge(&mut self, button: FootButton, pressed: bool, now_ms: u64) -> Option<FootGesture> {
        let state = &mut self.states[button.index()];
        if state.pressed == pressed
            || (state.last_edge_ms != 0
                && now_ms.saturating_sub(state.last_edge_ms) < FOOT_DEBOUNCE_MS)
        {
            return None;
        }
        state.last_edge_ms = now_ms;
        if pressed {
            state.pressed = true;
            state.pressed_at_ms = now_ms;
            state.long_emitted = false;
            None
        } else {
            state.pressed = false;
            if state.long_emitted {
                None
            } else {
                Some(FootGesture::ShortPress)
            }
        }
    }

    pub fn poll_long(&mut self, now_ms: u64) -> Option<FootButton> {
        for (index, state) in self.states.iter_mut().enumerate() {
            if state.pressed
                && !state.long_emitted
                && now_ms.saturating_sub(state.pressed_at_ms) >= FOOT_LONG_PRESS_MS
            {
                state.long_emitted = true;
                return Some(match index {
                    0 => FootButton::One,
                    1 => FootButton::Two,
                    2 => FootButton::Three,
                    _ => FootButton::Four,
                });
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiChannel(u8);

impl MidiChannel {
    /// Creates a user-facing MIDI channel in `1..=16`.
    ///
    /// # Errors
    ///
    /// Returns [`MidiError::InvalidChannel`] outside that range.
    pub const fn new(value: u8) -> Result<Self, MidiError> {
        if value == 0 || value > 16 {
            Err(MidiError::InvalidChannel(value))
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    const fn wire(self) -> u8 {
        self.0 - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BooleanCommand {
    Off,
    Toggle,
    On,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiAction {
    SelectPreset(PresetIndex),
    PreviousPreset,
    NextPreset,
    PreviousAbBank,
    NextAbBank,
    LoadPresetToSlot {
        preset: PresetIndex,
        slot: Slot,
    },
    TapTempo,
    Bypass(BooleanCommand),
    MasterVolume(u8),
    /// A TONEX parameter CC retained for the parameter service. The mapping
    /// table is deliberately separate from Serial/BLE packet parsing.
    Parameter {
        controller: u8,
        value: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiError {
    Truncated,
    MissingStatus(u8),
    InvalidBleHeader,
    InvalidBleTimestamp(u8),
    UnsupportedStatus(u8),
    InvalidChannel(u8),
    InvalidDataByte(u8),
    InvalidPreset(u8),
    InvalidBoolean(u8),
    UnsupportedControlChange(u8),
    ParameterMappingRequired(u8),
}

/// Decodes one BLE-MIDI notification without allocation.
///
/// The callback is invoked once for every accepted Program Change or Control
/// Change action, including compact running-status values after a single
/// status byte.
///
/// # Errors
///
/// Rejects missing timestamp headers, truncated events, malformed data, and
/// unsupported MIDI statuses.
pub fn decode_ble_packet<F>(
    packet: &[u8],
    configured_channel: MidiChannel,
    mut emit: F,
) -> Result<(), MidiError>
where
    F: FnMut(MidiAction),
{
    if packet.len() < 3 || packet[0] & 0xc0 != 0x80 {
        return Err(MidiError::InvalidBleHeader);
    }
    let mut offset = 1;
    while offset < packet.len() {
        let timestamp = packet[offset];
        if timestamp & 0x80 == 0 {
            return Err(MidiError::InvalidBleTimestamp(timestamp));
        }
        offset += 1;
        let status = *packet.get(offset).ok_or(MidiError::Truncated)?;
        let data_count = match status & 0xf0 {
            0xc0 => 1,
            0xb0 => 2,
            _ => return Err(MidiError::UnsupportedStatus(status)),
        };
        offset += 1;

        loop {
            let end = offset.checked_add(data_count).ok_or(MidiError::Truncated)?;
            let data = packet.get(offset..end).ok_or(MidiError::Truncated)?;
            let message = [status, data[0], data.get(1).copied().unwrap_or_default()];
            if let Some(action) = parse_message(&message[..=data_count], configured_channel)? {
                emit(action);
            }
            offset = end;
            if offset >= packet.len() || packet[offset] & 0x80 != 0 {
                break;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MidiStreamDecoder {
    running_status: Option<u8>,
    data: [u8; 2],
    data_len: u8,
}

impl MidiStreamDecoder {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Consumes one byte from an arbitrarily chunked serial MIDI stream.
    ///
    /// System real-time bytes are ignored without disturbing running status.
    /// Program Change and Control Change running status is retained after each
    /// complete message.
    ///
    /// # Errors
    ///
    /// Rejects data without status, unsupported status classes, and invalid
    /// MIDI data bytes.
    pub fn push(
        &mut self,
        byte: u8,
        configured_channel: MidiChannel,
    ) -> Result<Option<MidiAction>, MidiError> {
        if byte >= 0xf8 {
            return Ok(None);
        }
        if byte & 0x80 != 0 {
            if matches!(byte & 0xf0, 0xb0 | 0xc0) {
                self.running_status = Some(byte);
                self.data_len = 0;
                return Ok(None);
            }
            self.reset();
            return Err(MidiError::UnsupportedStatus(byte));
        }

        let status = self.running_status.ok_or(MidiError::MissingStatus(byte))?;
        let required = if status & 0xf0 == 0xc0 { 1 } else { 2 };
        self.data[usize::from(self.data_len)] = byte;
        self.data_len += 1;
        if self.data_len < required {
            return Ok(None);
        }
        self.data_len = 0;
        let message = [status, self.data[0], self.data[1]];
        parse_message(&message[..=usize::from(required)], configured_channel)
    }
}

/// Converts a parsed MIDI action into the same typed command used by the
/// switches, touch UI, and Web adapter.
///
/// `current_value` reads the last synchronized value for toggle commands and
/// tempo-synchronized effect controls.
///
/// # Errors
///
/// Returns an error for ordinary parameter CCs until their source-derived
/// controller-to-parameter mapping is resolved.
pub fn resolve_action<F>(
    action: MidiAction,
    now_ms: u64,
    current_value: F,
) -> Result<AppCommand, MidiError>
where
    F: Fn(ParameterId) -> f32,
{
    match action {
        MidiAction::SelectPreset(preset) => Ok(AppCommand::SelectPreset(preset)),
        MidiAction::PreviousPreset => Ok(AppCommand::PreviousPreset),
        MidiAction::NextPreset => Ok(AppCommand::NextPreset),
        MidiAction::PreviousAbBank => Ok(AppCommand::PreviousAbBank),
        MidiAction::NextAbBank => Ok(AppCommand::NextAbBank),
        MidiAction::LoadPresetToSlot { preset, slot } => {
            Ok(AppCommand::SelectPresetInSlot { preset, slot })
        }
        MidiAction::TapTempo => Ok(AppCommand::TapTempo(now_ms)),
        MidiAction::Bypass(command) => {
            let current_bypass = current_value(TONEX_GLOBAL_BYPASS);
            let value = match command {
                BooleanCommand::Toggle if current_bypass == 0.0 => 1.0,
                BooleanCommand::On => 1.0,
                BooleanCommand::Off | BooleanCommand::Toggle => 0.0,
            };
            Ok(AppCommand::SetParameter {
                id: TONEX_GLOBAL_BYPASS,
                value,
            })
        }
        MidiAction::MasterVolume(value) => Ok(AppCommand::SetParameter {
            id: TONEX_GLOBAL_MASTER_VOLUME,
            value: scale_midi_range(TONEX_GLOBAL_MASTER_VOLUME, value),
        }),
        MidiAction::Parameter { controller, value } => {
            resolve_parameter(controller, value, current_value)
        }
    }
}

/// Resolves a Web-configured logical footswitch action to the same command
/// path used by the board UI and ordinary MIDI. `None` represents an inherited
/// or deliberately disabled binding.
#[must_use]
pub fn resolve_foot_action<F>(
    action: FootAction,
    now_ms: u64,
    current_value: F,
) -> Option<AppCommand>
where
    F: Fn(ParameterId) -> f32,
{
    match action {
        FootAction::Inherit | FootAction::Disabled => None,
        FootAction::SelectSlot(slot) => Some(AppCommand::SelectSlot(slot)),
        FootAction::PreviousPreset => Some(AppCommand::PreviousPreset),
        FootAction::NextPreset => Some(AppCommand::NextPreset),
        FootAction::PreviousAbBank => Some(AppCommand::PreviousAbBank),
        FootAction::NextAbBank => Some(AppCommand::NextAbBank),
        FootAction::SelectPreset(preset) => Some(AppCommand::SelectPreset(preset)),
        FootAction::TapTempo => Some(AppCommand::TapTempo(now_ms)),
        FootAction::ToggleBypass => Some(toggle_parameter(TONEX_GLOBAL_BYPASS, current_value)),
        FootAction::ToggleMute => Some(AppCommand::ToggleMute),
        FootAction::ToggleEffect(block) => {
            let id = match block {
                EffectBlock::Gate => TONEX_PARAM_NOISE_GATE_ENABLE,
                EffectBlock::Compressor => TONEX_PARAM_COMP_ENABLE,
                EffectBlock::Amp => TONEX_PARAM_MODEL_AMP_ENABLE,
                EffectBlock::Cabinet => TONEX_GLOBAL_CABSIM_BYPASS,
                EffectBlock::Modulation => TONEX_PARAM_MODULATION_ENABLE,
                EffectBlock::Delay => TONEX_PARAM_DELAY_ENABLE,
                EffectBlock::Reverb => TONEX_PARAM_REVERB_ENABLE,
            };
            Some(toggle_parameter(id, current_value))
        }
    }
}

fn toggle_parameter<F>(id: ParameterId, current_value: F) -> AppCommand
where
    F: Fn(ParameterId) -> f32,
{
    AppCommand::SetParameter {
        id,
        value: if current_value(id) == 0.0 { 1.0 } else { 0.0 },
    }
}

fn resolve_parameter<F>(
    controller: u8,
    midi_value: u8,
    current_value: F,
) -> Result<AppCommand, MidiError>
where
    F: Fn(ParameterId) -> f32,
{
    if controller == 99 {
        return Ok(parameter_command(
            tonex_parameters::TONEX_GLOBAL_BPM,
            f32::from(midi_value.max(40)),
        ));
    }
    if controller == 100 {
        return Ok(parameter_command(
            tonex_parameters::TONEX_GLOBAL_BPM,
            f32::from(midi_value) + 100.0,
        ));
    }
    if let Some((sync, free, synchronized)) = mapping::synchronized_parameter(controller) {
        let (id, value) = if current_value(sync) == 0.0 {
            (free, scale_midi_range(free, midi_value))
        } else {
            let definition = spec(synchronized);
            (
                synchronized,
                f32::from(midi_value).clamp(definition.minimum, definition.maximum),
            )
        };
        return Ok(parameter_command(id, value));
    }

    let id = mapping::fixed_parameter(controller)
        .ok_or(MidiError::ParameterMappingRequired(controller))?;
    let definition = spec(id);
    let value = match controller {
        7 | 94 => {
            if midi_value == 64 {
                1.0
            } else {
                0.0
            }
        }
        30 => {
            if midi_value == BOOLEAN_ON {
                1.0
            } else {
                0.0
            }
        }
        _ => match definition.kind {
            ParameterKind::Switch => match midi_value {
                BOOLEAN_TOGGLE if current_value(id) == 0.0 => 1.0,
                BOOLEAN_ON => 1.0,
                _ => 0.0,
            },
            ParameterKind::Select => {
                f32::from(midi_value).clamp(definition.minimum, definition.maximum)
            }
            ParameterKind::Range => scale_midi_range(id, midi_value),
        },
    };
    Ok(parameter_command(id, value))
}

const fn parameter_command(id: ParameterId, value: f32) -> AppCommand {
    AppCommand::SetParameter { id, value }
}

/// Parses one standard MIDI Program Change or Control Change message.
///
/// Messages for other channels are valid but produce `Ok(None)`. Running
/// status and BLE timestamp framing belong to their transport adapters.
///
/// # Errors
///
/// Rejects truncated messages, unsupported status bytes, invalid data bytes,
/// and TONEX ONE preset values above 19.
pub fn parse_message(
    bytes: &[u8],
    configured_channel: MidiChannel,
) -> Result<Option<MidiAction>, MidiError> {
    let status = *bytes.first().ok_or(MidiError::Truncated)?;
    let command = status & 0xf0;
    let channel = status & 0x0f;
    if channel != configured_channel.wire() {
        return Ok(None);
    }

    match command {
        0xc0 => {
            let preset = data_byte(bytes, 1)?;
            Ok(Some(MidiAction::SelectPreset(valid_preset(preset)?)))
        }
        0xb0 => {
            let controller = data_byte(bytes, 1)?;
            let value = data_byte(bytes, 2)?;
            Ok(Some(map_control_change(controller, value)?))
        }
        _ => Err(MidiError::UnsupportedStatus(status)),
    }
}

fn map_control_change(controller: u8, value: u8) -> Result<MidiAction, MidiError> {
    match controller {
        CC_TAP_TEMPO => Ok(MidiAction::TapTempo),
        CC_BYPASS => Ok(MidiAction::Bypass(boolean_command(value)?)),
        CC_LOAD_SLOT_A => Ok(MidiAction::LoadPresetToSlot {
            preset: valid_preset(value)?,
            slot: Slot::A,
        }),
        CC_LOAD_SLOT_B => Ok(MidiAction::LoadPresetToSlot {
            preset: valid_preset(value)?,
            slot: Slot::B,
        }),
        CC_MASTER_VOLUME => Ok(MidiAction::MasterVolume(value)),
        CC_SELECT_PRESET => Ok(MidiAction::SelectPreset(valid_preset(value)?)),
        86 => Ok(MidiAction::PreviousPreset),
        87 => Ok(MidiAction::NextPreset),
        89 => Ok(MidiAction::PreviousAbBank),
        90 => Ok(MidiAction::NextAbBank),
        parameter if is_reference_parameter_cc(parameter) => {
            Ok(MidiAction::Parameter { controller, value })
        }
        unsupported => Err(MidiError::UnsupportedControlChange(unsupported)),
    }
}

const fn is_reference_parameter_cc(controller: u8) -> bool {
    matches!(
        controller,
        1..=8 | 13..=56 | 59..=85 | 88 | 91..=95 | 99 | 100 | 102..=104 | 106..=119
    )
}

fn data_byte(bytes: &[u8], index: usize) -> Result<u8, MidiError> {
    let value = *bytes.get(index).ok_or(MidiError::Truncated)?;
    if value & 0x80 != 0 {
        Err(MidiError::InvalidDataByte(value))
    } else {
        Ok(value)
    }
}

fn valid_preset(value: u8) -> Result<PresetIndex, MidiError> {
    PresetIndex::new(value).map_err(|_| MidiError::InvalidPreset(value))
}

const fn boolean_command(value: u8) -> Result<BooleanCommand, MidiError> {
    match value {
        BOOLEAN_OFF => Ok(BooleanCommand::Off),
        BOOLEAN_TOGGLE => Ok(BooleanCommand::Toggle),
        BOOLEAN_ON => Ok(BooleanCommand::On),
        invalid => Err(MidiError::InvalidBoolean(invalid)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel_one() -> MidiChannel {
        MidiChannel::new(1).expect("channel one is valid")
    }

    #[test]
    fn program_change_is_zero_based_and_tonex_one_bounded() {
        assert_eq!(
            parse_message(&[0xc0, 19], channel_one()),
            Ok(Some(MidiAction::SelectPreset(
                PresetIndex::new(19).expect("19 is valid")
            )))
        );
        assert_eq!(
            parse_message(&[0xc0, 20], channel_one()),
            Err(MidiError::InvalidPreset(20))
        );
    }

    #[test]
    fn special_cc_mapping_matches_reference_controller() {
        assert_eq!(
            parse_message(&[0xb0, 10, 127], channel_one()),
            Ok(Some(MidiAction::TapTempo))
        );
        assert_eq!(
            parse_message(&[0xb0, 12, 64], channel_one()),
            Ok(Some(MidiAction::Bypass(BooleanCommand::Toggle)))
        );
        assert_eq!(
            parse_message(&[0xb0, 120, 7], channel_one()),
            Ok(Some(MidiAction::LoadPresetToSlot {
                preset: PresetIndex::new(7).expect("7 is valid"),
                slot: Slot::A,
            }))
        );
        assert_eq!(
            parse_message(&[0xb0, 121, 8], channel_one()),
            Ok(Some(MidiAction::LoadPresetToSlot {
                preset: PresetIndex::new(8).expect("8 is valid"),
                slot: Slot::B,
            }))
        );
    }

    #[test]
    fn messages_for_other_channels_are_ignored() {
        assert_eq!(parse_message(&[0xc1, 3], channel_one()), Ok(None));
    }

    #[test]
    fn malformed_data_is_never_accepted_as_status_or_preset() {
        assert_eq!(
            parse_message(&[0xb0, 10], channel_one()),
            Err(MidiError::Truncated)
        );
        assert_eq!(
            parse_message(&[0xb0, 10, 0xff], channel_one()),
            Err(MidiError::InvalidDataByte(0xff))
        );
        assert_eq!(
            parse_message(&[0x90, 1, 2], channel_one()),
            Err(MidiError::UnsupportedStatus(0x90))
        );
        assert_eq!(
            parse_message(&[0xb0, 9, 0], channel_one()),
            Err(MidiError::UnsupportedControlChange(9))
        );
    }

    #[test]
    fn special_actions_resolve_to_shared_application_commands() {
        let preset = PresetIndex::new(7).expect("valid preset");
        assert_eq!(
            resolve_action(
                MidiAction::LoadPresetToSlot {
                    preset,
                    slot: Slot::B,
                },
                5_000,
                |_| 0.0,
            ),
            Ok(AppCommand::SelectPresetInSlot {
                preset,
                slot: Slot::B,
            })
        );
        assert_eq!(
            resolve_action(MidiAction::TapTempo, 5_000, |_| 0.0),
            Ok(AppCommand::TapTempo(5_000))
        );
        assert_eq!(
            resolve_action(MidiAction::Bypass(BooleanCommand::Toggle), 5_000, |_| 0.0,),
            Ok(AppCommand::SetParameter {
                id: TONEX_GLOBAL_BYPASS,
                value: 1.0,
            })
        );
        assert_eq!(
            resolve_action(MidiAction::Bypass(BooleanCommand::Toggle), 5_000, |_| 1.0,),
            Ok(AppCommand::SetParameter {
                id: TONEX_GLOBAL_BYPASS,
                value: 0.0,
            })
        );
        assert_eq!(
            resolve_foot_action(FootAction::ToggleMute, 5_000, |_| 0.0),
            Some(AppCommand::ToggleMute)
        );
    }

    #[test]
    fn master_volume_uses_the_source_derived_parameter_range() {
        assert_eq!(
            resolve_action(MidiAction::MasterVolume(0), 0, |_| 0.0),
            Ok(AppCommand::SetParameter {
                id: TONEX_GLOBAL_MASTER_VOLUME,
                value: -40.0,
            })
        );
        assert_eq!(
            resolve_action(MidiAction::MasterVolume(127), 0, |_| 0.0),
            Ok(AppCommand::SetParameter {
                id: TONEX_GLOBAL_MASTER_VOLUME,
                value: 3.0,
            })
        );
    }

    #[test]
    fn every_reference_parameter_cc_has_an_explicit_resolution() {
        for controller in 0..=127 {
            if is_reference_parameter_cc(controller) {
                let action = MidiAction::Parameter {
                    controller,
                    value: 127,
                };
                let result = resolve_action(action, 0, |_| 0.0);
                assert!(
                    matches!(result, Ok(AppCommand::SetParameter { .. })),
                    "CC {controller} must resolve"
                );
            }
        }
    }

    #[test]
    fn tremolo_sync_cc_normalizes_the_legacy_fractional_switch_bug() {
        let command = resolve_action(
            MidiAction::Parameter {
                controller: 38,
                value: BOOLEAN_TOGGLE,
            },
            0,
            |_| 0.0,
        );
        assert_eq!(
            command,
            Ok(AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_MODULATION_TREMOLO_SYNC,
                value: 1.0,
            })
        );
    }

    #[test]
    fn synchronized_rate_cc_selects_free_or_note_division_parameter() {
        let free = resolve_action(
            MidiAction::Parameter {
                controller: 35,
                value: 127,
            },
            0,
            |_| 0.0,
        );
        assert_eq!(
            free,
            Ok(AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_MODULATION_CHORUS_RATE,
                value: tonex_parameters::spec(tonex_parameters::TONEX_PARAM_MODULATION_CHORUS_RATE)
                    .maximum,
            })
        );

        let synchronized = resolve_action(
            MidiAction::Parameter {
                controller: 35,
                value: 127,
            },
            0,
            |id| {
                if id == tonex_parameters::TONEX_PARAM_MODULATION_CHORUS_SYNC {
                    1.0
                } else {
                    0.0
                }
            },
        );
        assert_eq!(
            synchronized,
            Ok(AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_MODULATION_CHORUS_TS,
                value: tonex_parameters::spec(tonex_parameters::TONEX_PARAM_MODULATION_CHORUS_TS)
                    .maximum,
            })
        );
    }

    #[test]
    fn navigation_ccs_are_not_misclassified_as_parameters() {
        assert_eq!(
            parse_message(&[0xb0, 86, 0], channel_one()),
            Ok(Some(MidiAction::PreviousPreset))
        );
        assert_eq!(
            parse_message(&[0xb0, 87, 0], channel_one()),
            Ok(Some(MidiAction::NextPreset))
        );
        assert_eq!(
            parse_message(&[0xb0, 89, 0], channel_one()),
            Ok(Some(MidiAction::PreviousAbBank))
        );
        assert_eq!(
            parse_message(&[0xb0, 90, 0], channel_one()),
            Ok(Some(MidiAction::NextAbBank))
        );
    }

    #[test]
    fn serial_decoder_handles_partial_chunks_and_running_status() {
        let mut decoder = MidiStreamDecoder::default();
        assert_eq!(decoder.push(0xb0, channel_one()), Ok(None));
        assert_eq!(decoder.push(CC_TAP_TEMPO, channel_one()), Ok(None));
        assert_eq!(
            decoder.push(BOOLEAN_ON, channel_one()),
            Ok(Some(MidiAction::TapTempo))
        );
        assert_eq!(decoder.push(CC_SELECT_PRESET, channel_one()), Ok(None));
        assert_eq!(
            decoder.push(7, channel_one()),
            Ok(Some(MidiAction::SelectPreset(
                PresetIndex::new(7).expect("valid preset")
            )))
        );
    }

    #[test]
    fn serial_decoder_preserves_running_status_across_realtime_bytes() {
        let mut decoder = MidiStreamDecoder::default();
        assert_eq!(decoder.push(0xc0, channel_one()), Ok(None));
        assert_eq!(decoder.push(0xf8, channel_one()), Ok(None));
        assert_eq!(
            decoder.push(3, channel_one()),
            Ok(Some(MidiAction::SelectPreset(
                PresetIndex::new(3).expect("valid preset")
            )))
        );
        assert_eq!(decoder.push(0xfe, channel_one()), Ok(None));
        assert_eq!(
            decoder.push(4, channel_one()),
            Ok(Some(MidiAction::SelectPreset(
                PresetIndex::new(4).expect("valid preset")
            )))
        );
    }

    #[test]
    fn serial_decoder_rejects_or_resets_malformed_streams() {
        let mut decoder = MidiStreamDecoder::default();
        assert_eq!(
            decoder.push(1, channel_one()),
            Err(MidiError::MissingStatus(1))
        );
        assert_eq!(
            decoder.push(0x90, channel_one()),
            Err(MidiError::UnsupportedStatus(0x90))
        );
        assert_eq!(
            decoder.push(2, channel_one()),
            Err(MidiError::MissingStatus(2))
        );
    }

    #[test]
    fn ble_decoder_strips_timestamps_and_emits_compact_running_status() {
        let mut actions = [None; 3];
        let mut count = 0;
        decode_ble_packet(
            &[
                0x80,
                0x80,
                0xb0,
                CC_TAP_TEMPO,
                BOOLEAN_ON,
                CC_SELECT_PRESET,
                7,
                0x81,
                0xc0,
                4,
            ],
            channel_one(),
            |action| {
                actions[count] = Some(action);
                count += 1;
            },
        )
        .expect("valid BLE-MIDI packet");
        assert_eq!(
            actions,
            [
                Some(MidiAction::TapTempo),
                Some(MidiAction::SelectPreset(
                    PresetIndex::new(7).expect("valid preset")
                )),
                Some(MidiAction::SelectPreset(
                    PresetIndex::new(4).expect("valid preset")
                )),
            ]
        );
    }

    #[test]
    fn ble_decoder_rejects_bad_headers_timestamps_and_truncation() {
        assert_eq!(
            decode_ble_packet(&[0xc0, 10, 127], channel_one(), |_| {}),
            Err(MidiError::InvalidBleHeader)
        );
        assert_eq!(
            decode_ble_packet(&[0x80, 0x01, 0xc0, 3], channel_one(), |_| {}),
            Err(MidiError::InvalidBleTimestamp(0x01))
        );
        assert_eq!(
            decode_ble_packet(&[0x80, 0x80, 0xb0, 10], channel_one(), |_| {}),
            Err(MidiError::Truncated)
        );
    }

    #[test]
    fn foot_press_tracker_emits_short_only_on_release() {
        let mut tracker = FootPressTracker::default();
        assert_eq!(tracker.edge(FootButton::One, true, 100), None);
        assert_eq!(tracker.poll_long(700), None);
        assert_eq!(
            tracker.edge(FootButton::One, false, 720),
            Some(FootGesture::ShortPress)
        );
    }

    #[test]
    fn foot_press_tracker_emits_long_once_and_suppresses_short() {
        let mut tracker = FootPressTracker::default();
        assert_eq!(tracker.edge(FootButton::Four, true, 1_000), None);
        assert_eq!(tracker.poll_long(1_649), None);
        assert_eq!(tracker.poll_long(1_650), Some(FootButton::Four));
        assert_eq!(tracker.poll_long(2_000), None);
        assert_eq!(tracker.edge(FootButton::Four, false, 2_100), None);
    }

    #[test]
    fn foot_press_tracker_ignores_duplicate_and_bounced_edges() {
        let mut tracker = FootPressTracker::default();
        assert_eq!(tracker.edge(FootButton::Two, true, 100), None);
        assert_eq!(tracker.edge(FootButton::Two, true, 110), None);
        assert_eq!(tracker.edge(FootButton::Two, false, 120), None);
        assert_eq!(
            tracker.edge(FootButton::Two, false, 180),
            Some(FootGesture::ShortPress)
        );
    }
}
