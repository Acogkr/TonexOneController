use alloc::vec::Vec;
use tonex_domain::{PRESET_NAME_CAPACITY, PresetIndex, Slot};
use tonex_parameters::{
    PRESET_PARAMETER_COUNT, ParameterError, ParameterId, TONEX_GLOBAL_BPM, TONEX_GLOBAL_BYPASS,
    TONEX_GLOBAL_CABSIM_BYPASS, TONEX_GLOBAL_INPUT_TRIM, TONEX_GLOBAL_MASTER_VOLUME,
    TONEX_GLOBAL_TEMPO_SOURCE, TONEX_GLOBAL_TUNING_REFERENCE, validate,
};

const INPUT_TRIM_OFFSET: usize = 15;
// TONEX ONE uses Double mode for slots A/B and Stomp mode for slot C.
const STOMP_MODE_OFFSET: usize = 19;
const CAB_BYPASS_OFFSET: usize = 20;
const PRESET_COLORS_OFFSET: usize = 22;
const PRESET_COLORS_HEADER: [u8; 2] = [0xba, 0x14];
const PRESET_COLOR_HEADER: [u8; 2] = [0xb9, 0x03];
const END_BPM: usize = 4;
const END_TEMPO_SOURCE: usize = 6;
const END_DIRECT_MONITOR: usize = 7;
const END_TUNING_REFERENCE: usize = 9;
const END_CURRENT_SLOT: usize = 11;
const END_BYPASS_MODE: usize = 12;
const END_SLOT_C_PRESET: usize = 14;
const END_SLOT_B_PRESET: usize = 16;
const END_SLOT_A_PRESET: usize = 18;
const PARAMETER_CHANGE_MARKER: [u8; 3] = [0xb9, 0x04, 0x03];
const FLOAT_MARKER: u8 = 0x88;
const PRESET_PARAMETERS_MARKER: [u8; 4] = [0xba, 0x03, 0xba, 0x6d];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateSnapshot {
    pub slot_a: PresetIndex,
    pub slot_b: PresetIndex,
    pub slot_c: PresetIndex,
    pub current_slot: Slot,
    pub bpm: f32,
    pub input_trim_db: f32,
    pub cabinet_bypass: bool,
    pub tempo_source: u8,
    pub tuning_reference_hz: u16,
    pub bypass: bool,
}

/// Raw RGB colour assigned to one preset by TONEX ONE.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PresetColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl PresetColor {
    #[must_use]
    pub const fn rgb888(self) -> u32 {
        (self.red as u32) << 16 | (self.green as u32) << 8 | self.blue as u32
    }
}

impl StateSnapshot {
    #[must_use]
    pub const fn current_preset(self) -> PresetIndex {
        match self.current_slot {
            Slot::A => self.slot_a,
            Slot::B => self.slot_b,
            Slot::C => self.slot_c,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    BodyTooShort { actual: usize, minimum: usize },
    InvalidPreset(u8),
    InvalidSlot(u8),
    NonFiniteFloat,
    MarkerNotFound,
    TruncatedParameterChange,
    InvalidFloatMarker(u8),
    InvalidMasterVolumeIndex(u16),
    TruncatedPresetParameters,
    InvalidPresetParameterMarker { index: u16, marker: u8 },
    BodyTooLong(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateDocument {
    body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlobalWriteError {
    InvalidValue(ParameterError),
    MasterVolumeUsesDedicatedCommand,
    NotGlobal(ParameterId),
}

impl StateDocument {
    /// Copies a validated state body while preserving every unknown field.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] when the stable fields cannot be parsed.
    pub fn new(body: &[u8]) -> Result<Self, StateError> {
        parse_state(body)?;
        Ok(Self {
            body: body.to_vec(),
        })
    }

    /// Returns the currently validated typed view.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] only if internal state was corrupted.
    pub fn snapshot(&self) -> Result<StateSnapshot, StateError> {
        parse_state(&self.body)
    }

    /// Assigns a preset to a slot and optionally selects that slot.
    pub fn assign_preset(&mut self, preset: PresetIndex, slot: Slot, select: bool) {
        let len = self.body.len();
        self.body[len - END_DIRECT_MONITOR] = 1;
        self.body[len - END_BYPASS_MODE] = 0;
        let preset_offset = match slot {
            Slot::A => END_SLOT_A_PRESET,
            Slot::B => END_SLOT_B_PRESET,
            Slot::C => END_SLOT_C_PRESET,
        };
        self.body[len - preset_offset] = preset.get();
        if select {
            self.select_slot(slot);
        }
    }

    /// Selects a slot without changing its assigned preset.
    pub fn select_slot(&mut self, slot: Slot) {
        let len = self.body.len();
        self.body[STOMP_MODE_OFFSET] = u8::from(matches!(slot, Slot::C));
        self.body[len - END_DIRECT_MONITOR] = 1;
        self.body[len - END_BYPASS_MODE] = 0;
        self.body[len - END_CURRENT_SLOT] = match slot {
            Slot::A => 0,
            Slot::B => 1,
            Slot::C => 2,
        };
    }

    /// Validates and updates one global field in the preserved state body.
    ///
    /// # Errors
    ///
    /// Rejects invalid values, preset parameters, and master volume, which
    /// has a dedicated TONEX ONE command.
    pub fn set_global(&mut self, id: ParameterId, value: f32) -> Result<(), GlobalWriteError> {
        let checked = validate(id, value).map_err(GlobalWriteError::InvalidValue)?;
        if !id.is_global() {
            return Err(GlobalWriteError::NotGlobal(id));
        }
        if id == TONEX_GLOBAL_MASTER_VOLUME {
            return Err(GlobalWriteError::MasterVolumeUsesDedicatedCommand);
        }
        let len = self.body.len();
        if id == TONEX_GLOBAL_BPM {
            self.body[len - END_BPM..len - END_BPM + 4]
                .copy_from_slice(&checked.value().to_le_bytes());
        } else if id == TONEX_GLOBAL_INPUT_TRIM {
            self.body[INPUT_TRIM_OFFSET..INPUT_TRIM_OFFSET + 4]
                .copy_from_slice(&checked.value().to_le_bytes());
        } else if id == TONEX_GLOBAL_CABSIM_BYPASS {
            self.body[CAB_BYPASS_OFFSET] = u8::from(checked.value() != 0.0);
        } else if id == TONEX_GLOBAL_TEMPO_SOURCE {
            self.body[len - END_TEMPO_SOURCE] = u8::from(checked.value() != 0.0);
        } else if id == TONEX_GLOBAL_TUNING_REFERENCE {
            let frequency = integral_u16(checked.value());
            self.body[len - END_TUNING_REFERENCE..len - END_TUNING_REFERENCE + 2]
                .copy_from_slice(&frequency.to_le_bytes());
        } else if id == TONEX_GLOBAL_BYPASS {
            self.body[len - END_BYPASS_MODE] = u8::from(checked.value() != 0.0);
        }
        Ok(())
    }

    /// Encodes the complete state-write command used by TONEX ONE.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::BodyTooLong`] if the body exceeds the protocol's
    /// 16-bit length field.
    pub fn encode_write(&self) -> Result<Vec<u8>, StateError> {
        let length =
            u16::try_from(self.body.len()).map_err(|_| StateError::BodyTooLong(self.body.len()))?;
        let [low, high] = length.to_le_bytes();
        let mut payload = Vec::with_capacity(self.body.len() + 11);
        payload.extend_from_slice(&[
            0xb9, 0x03, 0x81, 0x06, 0x03, 0x82, low, high, 0x80, 0x0b, 0x03,
        ]);
        payload.extend_from_slice(&self.body);
        Ok(crate::encode_frame(&payload))
    }

    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

fn integral_u16(value: f32) -> u16 {
    let mut converted = 0_u16;
    while f32::from(converted) < value {
        converted += 1;
    }
    converted
}

/// Parses the stable global and slot fields used by the C reference.
///
/// Preset colours are parsed separately because some older device firmware
/// omits the optional colour list while retaining the stable state fields.
///
/// # Errors
///
/// Returns [`StateError`] for short bodies, invalid slot/preset indices, or
/// non-finite floating-point values.
pub fn parse_state(body: &[u8]) -> Result<StateSnapshot, StateError> {
    let minimum = CAB_BYPASS_OFFSET + 1;
    if body.len() < minimum {
        return Err(StateError::BodyTooShort {
            actual: body.len(),
            minimum,
        });
    }

    let slot_a = preset_at_end(body, END_SLOT_A_PRESET)?;
    let slot_b = preset_at_end(body, END_SLOT_B_PRESET)?;
    let slot_c = preset_at_end(body, END_SLOT_C_PRESET)?;
    let current_slot = match byte_at_end(body, END_CURRENT_SLOT)? {
        0 => Slot::A,
        1 => Slot::B,
        2 => Slot::C,
        invalid => return Err(StateError::InvalidSlot(invalid)),
    };
    let bpm = f32_at_end(body, END_BPM)?;
    let input_trim_db = f32::from_le_bytes(
        body.get(INPUT_TRIM_OFFSET..INPUT_TRIM_OFFSET + 4)
            .ok_or(StateError::BodyTooShort {
                actual: body.len(),
                minimum: INPUT_TRIM_OFFSET + 4,
            })?
            .try_into()
            .map_err(|_| StateError::BodyTooShort {
                actual: body.len(),
                minimum: INPUT_TRIM_OFFSET + 4,
            })?,
    );
    if !bpm.is_finite() || !input_trim_db.is_finite() {
        return Err(StateError::NonFiniteFloat);
    }

    let tuning_index =
        body.len()
            .checked_sub(END_TUNING_REFERENCE)
            .ok_or(StateError::BodyTooShort {
                actual: body.len(),
                minimum: END_TUNING_REFERENCE,
            })?;
    let tuning_reference_hz = u16::from_le_bytes(
        body.get(tuning_index..tuning_index + 2)
            .ok_or(StateError::BodyTooShort {
                actual: body.len(),
                minimum: tuning_index + 2,
            })?
            .try_into()
            .map_err(|_| StateError::BodyTooShort {
                actual: body.len(),
                minimum: tuning_index + 2,
            })?,
    );

    Ok(StateSnapshot {
        slot_a,
        slot_b,
        slot_c,
        current_slot,
        bpm,
        input_trim_db,
        cabinet_bypass: body[CAB_BYPASS_OFFSET] != 0,
        tempo_source: byte_at_end(body, END_TEMPO_SOURCE)?,
        tuning_reference_hz,
        bypass: byte_at_end(body, END_BYPASS_MODE)? != 0,
    })
}

/// Parses the optional 20-entry preset-colour list used by TONEX ONE.
///
/// The format and offsets match the physical-device reference controller:
/// `ba 14`, followed by 20 `b9 03` RGB tuples encoded as variable-width
/// unsigned integers. A missing or malformed optional list returns `None`
/// without invalidating the rest of the device state.
#[must_use]
pub fn parse_preset_colors(body: &[u8]) -> Option<[PresetColor; tonex_domain::PRESET_COUNT]> {
    let mut cursor = PRESET_COLORS_OFFSET;
    if body.get(cursor..cursor + 2)? != PRESET_COLORS_HEADER {
        return None;
    }
    cursor += 2;
    let mut colors = [PresetColor::default(); tonex_domain::PRESET_COUNT];
    for color in &mut colors {
        if body.get(cursor..cursor + 2)? != PRESET_COLOR_HEADER {
            return None;
        }
        cursor += 2;
        let red = u8::try_from(crate::parse_value(body, &mut cursor).ok()?).ok()?;
        let green = u8::try_from(crate::parse_value(body, &mut cursor).ok()?).ok()?;
        let blue = u8::try_from(crate::parse_value(body, &mut cursor).ok()?).ok()?;
        *color = PresetColor { red, green, blue };
    }
    Some(colors)
}

fn byte_at_end(body: &[u8], offset: usize) -> Result<u8, StateError> {
    body.len()
        .checked_sub(offset)
        .and_then(|index| body.get(index))
        .copied()
        .ok_or(StateError::BodyTooShort {
            actual: body.len(),
            minimum: offset,
        })
}

fn preset_at_end(body: &[u8], offset: usize) -> Result<PresetIndex, StateError> {
    let value = byte_at_end(body, offset)?;
    PresetIndex::new(value).map_err(|_| StateError::InvalidPreset(value))
}

fn f32_at_end(body: &[u8], offset: usize) -> Result<f32, StateError> {
    let index = body
        .len()
        .checked_sub(offset)
        .ok_or(StateError::BodyTooShort {
            actual: body.len(),
            minimum: offset,
        })?;
    let bytes: [u8; 4] = body
        .get(index..index + 4)
        .ok_or(StateError::BodyTooShort {
            actual: body.len(),
            minimum: index + 4,
        })?
        .try_into()
        .map_err(|_| StateError::BodyTooShort {
            actual: body.len(),
            minimum: index + 4,
        })?;
    Ok(f32::from_le_bytes(bytes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetName {
    bytes: [u8; PRESET_NAME_CAPACITY],
    len: usize,
}

impl PresetName {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Extracts the fixed-width preset name following the legacy marker.
///
/// The returned value remains bytes because the wire encoding has not been
/// proven to always be UTF-8. NUL and `0xff` padding are removed.
///
/// # Errors
///
/// Returns [`StateError::MarkerNotFound`] if the marker or the complete
/// 32-byte name field is absent.
pub fn extract_preset_name(payload: &[u8]) -> Result<PresetName, StateError> {
    let marker = payload
        .windows(crate::PRESET_NAME_MARKER.len())
        .position(|window| window == crate::PRESET_NAME_MARKER)
        .ok_or(StateError::MarkerNotFound)?;
    let start = marker + crate::PRESET_NAME_MARKER.len();
    let source = payload
        .get(start..start + PRESET_NAME_CAPACITY)
        .ok_or(StateError::MarkerNotFound)?;
    let len = source
        .iter()
        .position(|byte| matches!(byte, 0x00 | 0xff))
        .unwrap_or(PRESET_NAME_CAPACITY);
    let mut bytes = [0; PRESET_NAME_CAPACITY];
    bytes[..len].copy_from_slice(&source[..len]);
    Ok(PresetName { bytes, len })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterChange {
    pub index: u16,
    pub value: f32,
}

/// Parses the parameter-change record consumed by the C implementation.
///
/// # Errors
///
/// Returns [`StateError`] if the marker is absent, the record is truncated,
/// the float marker is invalid, or the value is not finite.
pub fn parse_parameter_change(payload: &[u8]) -> Result<ParameterChange, StateError> {
    let marker = payload
        .windows(PARAMETER_CHANGE_MARKER.len())
        .position(|window| window == PARAMETER_CHANGE_MARKER)
        .ok_or(StateError::MarkerNotFound)?;
    let start = marker + PARAMETER_CHANGE_MARKER.len();
    let record = payload
        .get(start..start + 7)
        .ok_or(StateError::TruncatedParameterChange)?;
    if record[2] != FLOAT_MARKER {
        return Err(StateError::InvalidFloatMarker(record[2]));
    }
    let value = f32::from_le_bytes(
        record[3..7]
            .try_into()
            .map_err(|_| StateError::TruncatedParameterChange)?,
    );
    if !value.is_finite() {
        return Err(StateError::NonFiniteFloat);
    }
    Ok(ParameterChange {
        index: u16::from_le_bytes([record[0], record[1]]),
        value,
    })
}

/// Parses and converts the TONEX ONE master-volume response to `-40..=3 dB`.
///
/// # Errors
///
/// Returns [`StateError`] for a malformed parameter-change record or a
/// nonzero index, which is not a master-volume response.
pub fn parse_master_volume(payload: &[u8]) -> Result<f32, StateError> {
    let change = parse_parameter_change(payload)?;
    if change.index != 0 {
        return Err(StateError::InvalidMasterVolumeIndex(change.index));
    }
    Ok(((change.value / 10.0) * 43.0) - 40.0)
}

/// Extracts all 109 preset-scoped values from a full preset-details response.
///
/// # Errors
///
/// Returns [`StateError`] when the block marker is absent, the payload is
/// truncated, a float tag is wrong, or any value is non-finite.
pub fn parse_preset_parameters(
    payload: &[u8],
) -> Result<[f32; PRESET_PARAMETER_COUNT], StateError> {
    let marker = payload
        .windows(PRESET_PARAMETERS_MARKER.len())
        .position(|window| window == PRESET_PARAMETERS_MARKER)
        .ok_or(StateError::MarkerNotFound)?;
    let mut cursor = marker + PRESET_PARAMETERS_MARKER.len();
    let mut values = [0.0; PRESET_PARAMETER_COUNT];

    for (index, value) in values.iter_mut().enumerate() {
        let record = payload
            .get(cursor..cursor + 5)
            .ok_or(StateError::TruncatedPresetParameters)?;
        if record[0] != FLOAT_MARKER {
            return Err(StateError::InvalidPresetParameterMarker {
                index: u16::try_from(index).unwrap_or(u16::MAX),
                marker: record[0],
            });
        }
        *value = f32::from_le_bytes(
            record[1..5]
                .try_into()
                .map_err(|_| StateError::TruncatedPresetParameters)?,
        );
        if !value.is_finite() {
            return Err(StateError::NonFiniteFloat);
        }
        cursor += 5;
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn valid_state() -> [u8; 64] {
        let mut body = [0_u8; 64];
        body[INPUT_TRIM_OFFSET..INPUT_TRIM_OFFSET + 4].copy_from_slice(&(-3.5_f32).to_le_bytes());
        body[CAB_BYPASS_OFFSET] = 1;
        body[64 - END_SLOT_A_PRESET] = 4;
        body[64 - END_SLOT_B_PRESET] = 5;
        body[64 - END_SLOT_C_PRESET] = 6;
        body[64 - END_CURRENT_SLOT] = 1;
        body[64 - END_BPM..64 - END_BPM + 4].copy_from_slice(&120.0_f32.to_le_bytes());
        body[64 - END_TEMPO_SOURCE] = 1;
        body[64 - END_TUNING_REFERENCE..64 - END_TUNING_REFERENCE + 2]
            .copy_from_slice(&440_u16.to_le_bytes());
        body[64 - END_BYPASS_MODE] = 0;
        body
    }

    #[test]
    fn parses_validated_state_snapshot() {
        let state = parse_state(&valid_state()).expect("fixture is valid");
        assert_eq!(state.slot_a.get(), 4);
        assert_eq!(state.current_slot, Slot::B);
        assert_eq!(state.current_preset().get(), 5);
        assert!((state.bpm - 120.0).abs() < f32::EPSILON);
        assert!((state.input_trim_db + 3.5).abs() < f32::EPSILON);
        assert_eq!(state.tuning_reference_hz, 440);
        assert!(state.cabinet_bypass);
        assert!(!state.bypass);
    }

    #[test]
    fn parses_all_twenty_reference_preset_colours() {
        let mut body = vec![0_u8; 170];
        let mut cursor = PRESET_COLORS_OFFSET;
        body[cursor..cursor + 2].copy_from_slice(&PRESET_COLORS_HEADER);
        cursor += 2;
        for index in 0..tonex_domain::PRESET_COUNT {
            body[cursor..cursor + 2].copy_from_slice(&PRESET_COLOR_HEADER);
            cursor += 2;
            let red = u8::try_from(index * 10).expect("fixture red fits");
            body[cursor..cursor + 5].copy_from_slice(&[0x80, red, 0x80, 63, 0]);
            cursor += 5;
        }
        let colors = parse_preset_colors(&body).expect("valid colour list");
        assert_eq!(
            colors[0],
            PresetColor {
                red: 0,
                green: 63,
                blue: 0
            }
        );
        assert_eq!(
            colors[19],
            PresetColor {
                red: 190,
                green: 63,
                blue: 0
            }
        );

        body[PRESET_COLORS_OFFSET] = 0;
        assert_eq!(parse_preset_colors(&body), None);
    }

    #[test]
    fn rejects_invalid_slot_and_preset() {
        let mut body = valid_state();
        body[64 - END_CURRENT_SLOT] = 3;
        assert_eq!(parse_state(&body), Err(StateError::InvalidSlot(3)));

        body = valid_state();
        body[64 - END_SLOT_A_PRESET] = 20;
        assert_eq!(parse_state(&body), Err(StateError::InvalidPreset(20)));
    }

    #[test]
    fn extracts_name_without_assuming_utf8() {
        let mut payload = [0_u8; 64];
        payload[3..9].copy_from_slice(&crate::PRESET_NAME_MARKER);
        payload[9..13].copy_from_slice(&[b'T', b'O', 0xff, b'X']);
        let name = extract_preset_name(&payload).expect("name marker exists");
        assert_eq!(name.as_bytes(), b"TO");
    }

    #[test]
    fn parses_parameter_change_with_bounds_checks() {
        let value = 7.25_f32;
        let mut payload = [0_u8; 16];
        payload[2..5].copy_from_slice(&PARAMETER_CHANGE_MARKER);
        payload[5..7].copy_from_slice(&42_u16.to_le_bytes());
        payload[7] = FLOAT_MARKER;
        payload[8..12].copy_from_slice(&value.to_le_bytes());
        assert_eq!(
            parse_parameter_change(&payload),
            Ok(ParameterChange { index: 42, value })
        );

        payload[7] = 0;
        assert_eq!(
            parse_parameter_change(&payload),
            Err(StateError::InvalidFloatMarker(0))
        );
    }

    #[test]
    fn master_volume_conversion_matches_reference_firmware() {
        let mut payload = [0_u8; 16];
        payload[2..5].copy_from_slice(&PARAMETER_CHANGE_MARKER);
        payload[5..7].copy_from_slice(&0_u16.to_le_bytes());
        payload[7] = FLOAT_MARKER;
        payload[8..12].copy_from_slice(&10.0_f32.to_le_bytes());
        assert!(
            (parse_master_volume(&payload).expect("valid response") - 3.0).abs() < f32::EPSILON
        );

        payload[5..7].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            parse_master_volume(&payload),
            Err(StateError::InvalidMasterVolumeIndex(1))
        );
    }

    #[test]
    fn parses_complete_preset_parameter_block() {
        let mut payload = PRESET_PARAMETERS_MARKER.to_vec();
        for index in 0..PRESET_PARAMETER_COUNT {
            payload.push(FLOAT_MARKER);
            let index = u16::try_from(index).expect("fixture index fits u16");
            payload.extend_from_slice(&(f32::from(index) / 10.0).to_le_bytes());
        }
        let values = parse_preset_parameters(&payload).expect("complete block");
        assert!((values[42] - 4.2).abs() < f32::EPSILON * 4.0);

        payload[PRESET_PARAMETERS_MARKER.len() + 5] = 0;
        assert_eq!(
            parse_preset_parameters(&payload),
            Err(StateError::InvalidPresetParameterMarker {
                index: 1,
                marker: 0
            })
        );
    }

    #[test]
    fn state_document_preserves_unknown_bytes_and_encodes_slot_assignment() {
        let original = valid_state();
        let mut document = StateDocument::new(&original).expect("valid state");
        let preset = PresetIndex::new(9).expect("valid preset");
        document.assign_preset(preset, Slot::C, true);
        let snapshot = document.snapshot().expect("mutation remains valid");
        assert_eq!(snapshot.slot_c, preset);
        assert_eq!(snapshot.current_slot, Slot::C);
        assert_eq!(document.body()[STOMP_MODE_OFFSET], 1);
        assert!(!snapshot.bypass);
        assert_eq!(document.body()[0], original[0]);

        document.select_slot(Slot::B);
        let snapshot = document
            .snapshot()
            .expect("double-mode mutation remains valid");
        assert_eq!(snapshot.current_slot, Slot::B);
        assert_eq!(document.body()[STOMP_MODE_OFFSET], 0);

        let decoded = crate::decode_frame(&document.encode_write().expect("length fits"))
            .expect("valid framed command");
        assert_eq!(
            &decoded[..11],
            &[0xb9, 0x03, 0x81, 0x06, 0x03, 0x82, 64, 0, 0x80, 0x0b, 0x03]
        );
        assert_eq!(&decoded[11..], document.body());
    }

    #[test]
    fn state_document_updates_all_state_backed_globals() {
        let mut document = StateDocument::new(&valid_state()).expect("valid state");
        document
            .set_global(TONEX_GLOBAL_BPM, 96.5)
            .expect("BPM is state-backed");
        document
            .set_global(TONEX_GLOBAL_INPUT_TRIM, -2.0)
            .expect("trim is state-backed");
        document
            .set_global(TONEX_GLOBAL_BYPASS, 1.0)
            .expect("bypass is state-backed");
        let state = document.snapshot().expect("mutated state stays valid");
        assert!((state.bpm - 96.5).abs() < f32::EPSILON);
        assert!((state.input_trim_db + 2.0).abs() < f32::EPSILON);
        assert!(state.bypass);
        assert_eq!(
            document.set_global(TONEX_GLOBAL_MASTER_VOLUME, 0.0),
            Err(GlobalWriteError::MasterVolumeUsesDedicatedCommand)
        );
    }
}
