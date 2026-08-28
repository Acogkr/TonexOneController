use alloc::vec::Vec;
use tonex_parameters::{ParameterError, ParameterId, TONEX_GLOBAL_MASTER_VOLUME, validate};

use crate::encode_frame;

const WRITE_HEADER: [u8; 11] = [
    0xb9, 0x03, 0x81, 0x09, 0x03, 0x82, 0x0a, 0x00, 0x80, 0x0b, 0x03,
];
const FLOAT_MARKER: u8 = 0x88;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterWriteError {
    InvalidValue(ParameterError),
    GlobalRequiresStateWrite(ParameterId),
}

impl From<ParameterError> for ParameterWriteError {
    fn from(value: ParameterError) -> Self {
        Self::InvalidValue(value)
    }
}

/// Encodes the TONEX ONE editor-era single-parameter command.
///
/// # Errors
///
/// Rejects invalid values and global parameters, which use distinct TONEX ONE
/// commands or a complete state write.
pub fn encode_parameter_change(
    id: ParameterId,
    value: f32,
) -> Result<Vec<u8>, ParameterWriteError> {
    let checked = validate(id, value)?;
    if checked.id().is_global() {
        return Err(ParameterWriteError::GlobalRequiresStateWrite(id));
    }

    let mut payload = Vec::with_capacity(21);
    payload.extend_from_slice(&WRITE_HEADER);
    payload.extend_from_slice(&[0xb9, 0x04, 0x02, 0x00]);
    payload.push(checked.id().get().to_le_bytes()[0]);
    payload.push(FLOAT_MARKER);
    payload.extend_from_slice(&checked.value().to_le_bytes());
    Ok(encode_frame(&payload))
}

/// Encodes master volume after converting the public `-40..=3 dB` value to
/// the pedal's `0..=10` wire scale used by the reference firmware.
///
/// # Errors
///
/// Rejects non-finite or out-of-range dB values.
pub fn encode_master_volume(value_db: f32) -> Result<Vec<u8>, ParameterWriteError> {
    let checked = validate(TONEX_GLOBAL_MASTER_VOLUME, value_db)?;
    let wire_value = ((checked.value() + 40.0) / 43.0) * 10.0;

    let mut payload = Vec::with_capacity(21);
    payload.extend_from_slice(&WRITE_HEADER);
    payload.extend_from_slice(&[0xb9, 0x04, 0x03, 0x00, 0x00, FLOAT_MARKER]);
    payload.extend_from_slice(&wire_value.to_le_bytes());
    Ok(encode_frame(&payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_frame;
    use tonex_parameters::{TONEX_GLOBAL_BPM, TONEX_PARAM_EQ_BASS};

    #[test]
    fn parameter_packet_matches_reference_layout() {
        let frame = encode_parameter_change(TONEX_PARAM_EQ_BASS, 7.25).expect("valid value");
        let payload = decode_frame(&frame).expect("encoder produces a valid frame");
        assert_eq!(&payload[..11], &WRITE_HEADER);
        assert_eq!(
            &payload[11..17],
            &[0xb9, 0x04, 0x02, 0x00, 11, FLOAT_MARKER]
        );
        assert_eq!(&payload[17..21], &7.25_f32.to_le_bytes());
    }

    #[test]
    fn normal_parameter_writer_rejects_globals() {
        assert_eq!(
            encode_parameter_change(TONEX_GLOBAL_BPM, 120.0),
            Err(ParameterWriteError::GlobalRequiresStateWrite(
                TONEX_GLOBAL_BPM
            ))
        );
    }

    #[test]
    fn master_volume_uses_distinct_command_and_wire_scale() {
        let frame = encode_master_volume(3.0).expect("upper bound is valid");
        let payload = decode_frame(&frame).expect("encoder produces a valid frame");
        assert_eq!(&payload[..11], &WRITE_HEADER);
        assert_eq!(
            &payload[11..17],
            &[0xb9, 0x04, 0x03, 0x00, 0x00, FLOAT_MARKER]
        );
        assert_eq!(&payload[17..21], &10.0_f32.to_le_bytes());
    }
}
