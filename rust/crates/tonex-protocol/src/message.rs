use alloc::vec::Vec;

use crate::{FrameError, decode_frame, encode_frame};

const MESSAGE_PREFIX: [u8; 2] = [0xb9, 0x03];
pub const PRESET_NAME_MARKER: [u8; 6] = [0xb9, 0x04, 0xb9, 0x02, 0xbc, 0x21];
pub const PRESET_NAME_CAPACITY: usize = 32;

pub const HELLO_REQUEST: &[u8] = &[
    0xb9, 0x03, 0x00, 0x82, 0x04, 0x00, 0x80, 0x0b, 0x01, 0xb9, 0x02, 0x02, 0x0b,
];
pub const STATE_REQUEST: &[u8] = &[
    0xb9, 0x03, 0x00, 0x82, 0x06, 0x00, 0x80, 0x0b, 0x03, 0xb9, 0x02, 0x81, 0x06, 0x03, 0x0b,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageType {
    Unknown(u16),
    StateUpdate,
    Hello,
    PresetDetails,
    PresetDetailsFull,
    ParameterChanged,
}

impl From<u16> for MessageType {
    fn from(value: u16) -> Self {
        match value {
            0x0306 => Self::StateUpdate,
            0x0304 => Self::PresetDetails,
            0x0303 => Self::PresetDetailsFull,
            0x0002 => Self::Hello,
            0x0309 => Self::ParameterChanged,
            unknown => Self::Unknown(unknown),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    pub message_type: MessageType,
    pub body_len: u16,
    pub unknown: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub header: MessageHeader,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageError {
    Frame(FrameError),
    TruncatedValue,
    InvalidPrefix,
    BodyLengthMismatch { declared: usize, actual: usize },
    InvalidPresetIndex(u8),
}

impl From<FrameError> for MessageError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}

/// Parses the variable-width unsigned integer used in TONEX messages.
///
/// The C reference treats both `0x81` and `0x82` as a two-byte little-endian
/// value and `0x80` as a one-byte value. Unprefixed values occupy one byte.
///
/// # Errors
///
/// Returns [`MessageError::TruncatedValue`] when the prefix is not followed
/// by all bytes required by the reference encoding.
pub fn parse_value(input: &[u8], cursor: &mut usize) -> Result<u16, MessageError> {
    let first = *input.get(*cursor).ok_or(MessageError::TruncatedValue)?;
    let (value, consumed) = match first {
        0x81 | 0x82 => {
            let low = *input.get(*cursor + 1).ok_or(MessageError::TruncatedValue)?;
            let high = *input.get(*cursor + 2).ok_or(MessageError::TruncatedValue)?;
            (u16::from_le_bytes([low, high]), 3)
        }
        0x80 => {
            let value = *input.get(*cursor + 1).ok_or(MessageError::TruncatedValue)?;
            (u16::from(value), 2)
        }
        value => (u16::from(value), 1),
    };
    *cursor += consumed;
    Ok(value)
}

/// Parses and validates one complete framed TONEX message.
///
/// # Errors
///
/// Returns [`MessageError`] if framing, the fixed prefix, header values, or
/// the declared body length are invalid.
pub fn parse_message(frame: &[u8]) -> Result<Message, MessageError> {
    let decoded = decode_frame(frame)?;
    parse_payload(&decoded)
}

/// Parses a CRC-verified, unframed TONEX payload.
///
/// # Errors
///
/// Returns [`MessageError`] if the fixed prefix, tagged header values, or
/// declared body length are invalid.
pub fn parse_payload(decoded: &[u8]) -> Result<Message, MessageError> {
    if !decoded.starts_with(&MESSAGE_PREFIX) {
        return Err(MessageError::InvalidPrefix);
    }

    let mut cursor = MESSAGE_PREFIX.len();
    let message_type = MessageType::from(parse_value(decoded, &mut cursor)?);
    let body_len = parse_value(decoded, &mut cursor)?;
    let unknown = parse_value(decoded, &mut cursor)?;
    let body = decoded.get(cursor..).ok_or(MessageError::TruncatedValue)?;
    if body.len() != usize::from(body_len) {
        return Err(MessageError::BodyLengthMismatch {
            declared: usize::from(body_len),
            actual: body.len(),
        });
    }

    Ok(Message {
        header: MessageHeader {
            message_type,
            body_len,
            unknown,
        },
        body: body.to_vec(),
    })
}

/// Builds the exact summary request used by the C reference for preset
/// indices `0..20`.
///
/// # Errors
///
/// Returns [`MessageError::InvalidPresetIndex`] for any value above 19.
pub fn encode_preset_details_request(preset_index: u8) -> Result<Vec<u8>, MessageError> {
    if preset_index >= 20 {
        return Err(MessageError::InvalidPresetIndex(preset_index));
    }
    Ok(encode_preset_details_request_unchecked(preset_index))
}

pub(crate) fn encode_preset_details_request_unchecked(preset_index: u8) -> Vec<u8> {
    let mut request = [
        0xb9, 0x03, 0x81, 0x00, 0x03, 0x82, 0x06, 0x00, 0x80, 0x0b, 0x03, 0xb9, 0x04, 0x0b, 0x01,
        0x00, 0x00,
    ];
    request[15] = preset_index;
    encode_frame(&request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_reference_value_forms() {
        let input = [0x05, 0x80, 0x06, 0x81, 0x34, 0x12, 0x82, 0x78, 0x56];
        let mut cursor = 0;
        assert_eq!(parse_value(&input, &mut cursor), Ok(5));
        assert_eq!(parse_value(&input, &mut cursor), Ok(6));
        assert_eq!(parse_value(&input, &mut cursor), Ok(0x1234));
        assert_eq!(parse_value(&input, &mut cursor), Ok(0x5678));
        assert_eq!(cursor, input.len());
    }

    #[test]
    fn rejects_truncated_prefixed_values() {
        let mut cursor = 0;
        assert_eq!(
            parse_value(&[0x82, 1], &mut cursor),
            Err(MessageError::TruncatedValue)
        );
    }

    #[test]
    fn parses_synthetic_hello_response() {
        // Type=2, body length=0, legacy unknown field=0x0b.
        let frame = encode_frame(&[0xb9, 0x03, 0x02, 0x00, 0x80, 0x0b]);
        assert_eq!(
            parse_message(&frame),
            Ok(Message {
                header: MessageHeader {
                    message_type: MessageType::Hello,
                    body_len: 0,
                    unknown: 0x0b,
                },
                body: Vec::new(),
            })
        );
    }

    #[test]
    fn validates_declared_body_size() {
        let frame = encode_frame(&[0xb9, 0x03, 0x02, 0x01, 0x80, 0x0b]);
        assert_eq!(
            parse_message(&frame),
            Err(MessageError::BodyLengthMismatch {
                declared: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn preset_request_matches_reference_payload_and_rejects_twenty() {
        let frame = encode_preset_details_request(7).expect("7 is a valid preset");
        let decoded = decode_frame(&frame).expect("generated frame must decode");
        assert_eq!(decoded[15], 7);
        assert_eq!(decoded[16], 0);
        assert_eq!(
            encode_preset_details_request(20),
            Err(MessageError::InvalidPresetIndex(20))
        );
    }
}
