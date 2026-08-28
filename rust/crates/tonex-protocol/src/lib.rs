#![no_std]

extern crate alloc;

use alloc::vec::Vec;

mod message;
mod parameter_write;
mod session;
mod state;

pub use message::{
    HELLO_REQUEST, Message, MessageError, MessageHeader, MessageType, PRESET_NAME_CAPACITY,
    PRESET_NAME_MARKER, STATE_REQUEST, encode_preset_details_request, parse_message, parse_payload,
    parse_value,
};
pub use parameter_write::{ParameterWriteError, encode_master_volume, encode_parameter_change};
pub use session::{Session, SessionAction, SessionError, SessionState};
pub use state::{
    GlobalWriteError, ParameterChange, PresetColor, PresetName, StateDocument, StateError,
    StateSnapshot, extract_preset_name, parse_master_volume, parse_parameter_change,
    parse_preset_colors, parse_preset_parameters, parse_state,
};

pub const FRAME_FLAG: u8 = 0x7e;
pub const ESCAPE: u8 = 0x7d;
pub const ESCAPE_XOR: u8 = 0x20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    MissingBoundary,
    TooShort,
    InvalidEscape,
    CrcMismatch { received: u16, calculated: u16 },
    FrameTooLarge,
}

/// CRC-16/X-25 used by the reference C implementation.
#[must_use]
pub fn crc16_x25(data: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in data {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[must_use]
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let crc = crc16_x25(payload);
    let mut output = Vec::with_capacity(payload.len().saturating_mul(2).saturating_add(6));
    output.push(FRAME_FLAG);
    for byte in payload
        .iter()
        .copied()
        .chain(crc.to_le_bytes().iter().copied())
    {
        push_escaped(&mut output, byte);
    }
    output.push(FRAME_FLAG);
    output
}

/// Removes boundaries and escaping and verifies the wire CRC.
///
/// # Errors
///
/// Returns [`FrameError`] for invalid boundaries, length, escaping, or CRC.
pub fn decode_frame(frame: &[u8]) -> Result<Vec<u8>, FrameError> {
    if frame.first() != Some(&FRAME_FLAG) || frame.last() != Some(&FRAME_FLAG) {
        return Err(FrameError::MissingBoundary);
    }
    if frame.len() < 4 {
        return Err(FrameError::TooShort);
    }

    let mut decoded = Vec::with_capacity(frame.len() - 2);
    let mut index = 1;
    while index < frame.len() - 1 {
        match frame[index] {
            ESCAPE => {
                if index + 1 >= frame.len() - 1 {
                    return Err(FrameError::InvalidEscape);
                }
                decoded.push(frame[index + 1] ^ ESCAPE_XOR);
                index += 2;
            }
            FRAME_FLAG => break,
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    if decoded.len() < 2 {
        return Err(FrameError::TooShort);
    }
    let crc_index = decoded.len() - 2;
    let received = u16::from_le_bytes([decoded[crc_index], decoded[crc_index + 1]]);
    decoded.truncate(crc_index);
    let calculated = crc16_x25(&decoded);
    if received != calculated {
        return Err(FrameError::CrcMismatch {
            received,
            calculated,
        });
    }
    Ok(decoded)
}

fn push_escaped(output: &mut Vec<u8>, byte: u8) {
    if matches!(byte, FRAME_FLAG | ESCAPE) {
        output.push(ESCAPE);
        output.push(byte ^ ESCAPE_XOR);
    } else {
        output.push(byte);
    }
}

#[derive(Debug)]
pub struct StreamDecoder {
    encoded: Vec<u8>,
    max_encoded_len: usize,
    in_frame: bool,
}

impl StreamDecoder {
    #[must_use]
    pub fn new(max_encoded_len: usize) -> Self {
        Self {
            encoded: Vec::with_capacity(max_encoded_len),
            max_encoded_len,
            in_frame: false,
        }
    }

    /// Feeds an arbitrary USB chunk and returns every complete frame it contains.
    ///
    /// Noise before a boundary is ignored. An oversized partial frame is dropped,
    /// and extraction resumes at the next frame boundary.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Result<Vec<u8>, FrameError>> {
        let mut frames = Vec::new();
        for byte in chunk {
            if *byte == FRAME_FLAG {
                if self.in_frame && self.encoded.len() > 1 {
                    self.encoded.push(FRAME_FLAG);
                    frames.push(decode_frame(&self.encoded));
                    self.encoded.clear();
                    self.encoded.push(FRAME_FLAG);
                } else {
                    self.encoded.clear();
                    self.encoded.push(FRAME_FLAG);
                    self.in_frame = true;
                }
                continue;
            }

            if !self.in_frame {
                continue;
            }
            if self.encoded.len() >= self.max_encoded_len {
                self.encoded.clear();
                self.in_frame = false;
                frames.push(Err(FrameError::FrameTooLarge));
                continue;
            }
            self.encoded.push(*byte);
        }
        frames
    }

    pub fn reset(&mut self) {
        self.encoded.clear();
        self.in_frame = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn crc_matches_standard_check_vector() {
        assert_eq!(crc16_x25(b"123456789"), 0x906e);
    }

    #[test]
    fn frame_round_trip_escapes_reserved_bytes() {
        let payload = [0xb9, 0x03, FRAME_FLAG, ESCAPE, 0x00];
        let frame = encode_frame(&payload);
        assert!(frame.windows(2).any(|pair| pair == [ESCAPE, 0x5e]));
        assert!(frame.windows(2).any(|pair| pair == [ESCAPE, 0x5d]));
        assert_eq!(decode_frame(&frame), Ok(payload.to_vec()));
    }

    #[test]
    fn rejects_damaged_crc() {
        let mut frame = encode_frame(&[1, 2, 3]);
        frame[2] ^= 0x01;
        assert!(matches!(
            decode_frame(&frame),
            Err(FrameError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn rejects_trailing_escape() {
        assert_eq!(
            decode_frame(&[FRAME_FLAG, ESCAPE, FRAME_FLAG]),
            Err(FrameError::TooShort)
        );
        assert_eq!(
            decode_frame(&[FRAME_FLAG, 1, ESCAPE, FRAME_FLAG]),
            Err(FrameError::InvalidEscape)
        );
    }

    #[test]
    fn stream_decoder_handles_partial_and_consecutive_frames() {
        let first = encode_frame(&[1, 2, 3]);
        let second = encode_frame(&[4, 5]);
        let split = first.len() / 2;
        let mut decoder = StreamDecoder::new(128);

        assert!(decoder.push(&first[..split]).is_empty());
        let mut tail = first[split..].to_vec();
        tail.extend_from_slice(&second);
        assert_eq!(decoder.push(&tail), vec![Ok(vec![1, 2, 3]), Ok(vec![4, 5])]);
    }

    #[test]
    fn stream_decoder_ignores_noise_and_recovers_after_oversize() {
        let valid = encode_frame(&[9]);
        let mut decoder = StreamDecoder::new(5);
        let mut input = vec![1, 2, FRAME_FLAG, 3, 4, 5, 6, 7, 8, FRAME_FLAG];
        input.extend_from_slice(&valid);
        let results = decoder.push(&input);
        assert!(results.contains(&Err(FrameError::FrameTooLarge)));
        assert!(results.contains(&Ok(vec![9])));
    }

    #[test]
    fn deterministic_fuzz_corpus_never_panics_and_valid_frames_round_trip() {
        let mut random = 0x6d2b_79f5_u32;
        let mut decoder = StreamDecoder::new(1_024);

        for iteration in 0..4_096 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            let length = usize::try_from(random % 513).expect("bounded length fits usize");
            let mut bytes = vec![0_u8; length];
            for byte in &mut bytes {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                *byte = random.to_le_bytes()[0];
            }
            if bytes.len() >= 2 && iteration % 2 == 0 {
                bytes[0] = FRAME_FLAG;
                let last = bytes.len() - 1;
                bytes[last] = FRAME_FLAG;
            }

            let _ = decode_frame(&bytes);
            let _ = parse_message(&bytes);
            let _ = parse_payload(&bytes);
            let _ = parse_state(&bytes);
            let _ = StateDocument::new(&bytes);
            let _ = extract_preset_name(&bytes);
            let _ = parse_master_volume(&bytes);
            let _ = parse_parameter_change(&bytes);
            let _ = parse_preset_parameters(&bytes);

            for chunk in bytes.chunks(17) {
                let _ = decoder.push(chunk);
            }
            decoder.reset();

            let encoded = encode_frame(&bytes);
            assert_eq!(decode_frame(&encoded), Ok(bytes));
        }
    }
}
