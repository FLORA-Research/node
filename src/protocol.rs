//! FLORA RS-485 v1 framing. Kept allocation-free for embedded use.

pub const PROTOCOL_VERSION: &str = "flora-rs485-v1";
pub const SOF: u8 = 0xAA;
pub const EOF: u8 = 0x55;
pub const BROADCAST_ADDR: u8 = 0x00;
pub const AGGREGATOR_ADDR: u8 = 0x01;
pub const FIRST_NODE_ADDR: u8 = 0x02;
pub const RESERVED_ADDR: u8 = 0xFF;
pub const MAX_PAYLOAD_LEN: usize = 64;
pub const FRAME_OVERHEAD: usize = 9;
pub const MAX_FRAME_LEN: usize = FRAME_OVERHEAD + MAX_PAYLOAD_LEN;

pub const CMD_POLL: u8 = 0x01;
pub const CMD_SET_VALVE: u8 = 0x02;
pub const CMD_SET_THRESHOLD: u8 = 0x03;
pub const CMD_CALIBRATE: u8 = 0x04;
pub const CMD_PING: u8 = 0x05;
pub const CMD_SET_ADDRESS: u8 = 0x06;
pub const CMD_GET_INFO: u8 = 0x07;
pub const CMD_ACK: u8 = 0x80;
pub const CMD_NACK: u8 = 0x81;
pub const CMD_DATA: u8 = 0x82;
pub const CMD_INFO: u8 = 0x83;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub addr: u8,
    pub cmd: u8,
    pub sequence: u16,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooShort,
    InvalidStart,
    InvalidAddress,
    PayloadTooLong,
    LengthMismatch,
    InvalidEnd,
    Checksum,
    OutputTooSmall,
}

/// Encodes one frame into `out`; the caller owns the buffer and payload memory.
pub fn encode(frame: &Frame<'_>, out: &mut [u8]) -> Result<usize, Error> {
    if frame.addr == RESERVED_ADDR {
        return Err(Error::InvalidAddress);
    }
    if frame.payload.len() > MAX_PAYLOAD_LEN {
        return Err(Error::PayloadTooLong);
    }

    let frame_len = FRAME_OVERHEAD + frame.payload.len();
    if out.len() < frame_len {
        return Err(Error::OutputTooSmall);
    }

    out[0] = SOF;
    out[1] = frame.addr;
    out[2] = frame.cmd;
    out[3] = (frame.sequence >> 8) as u8;
    out[4] = frame.sequence as u8;
    out[5] = frame.payload.len() as u8;
    out[6..6 + frame.payload.len()].copy_from_slice(frame.payload);

    let checksum_at = 6 + frame.payload.len();
    let checksum = crc16_ccitt_false(&out[1..checksum_at]);
    out[checksum_at] = (checksum >> 8) as u8;
    out[checksum_at + 1] = checksum as u8;
    out[checksum_at + 2] = EOF;
    Ok(frame_len)
}

/// Decodes exactly one complete frame without allocating or copying its payload.
pub fn decode(bytes: &[u8]) -> Result<Frame<'_>, Error> {
    if bytes.len() < FRAME_OVERHEAD {
        return Err(Error::TooShort);
    }
    if bytes[0] != SOF {
        return Err(Error::InvalidStart);
    }

    let payload_len = bytes[5] as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(Error::PayloadTooLong);
    }
    if bytes.len() != FRAME_OVERHEAD + payload_len {
        return Err(Error::LengthMismatch);
    }
    if bytes[bytes.len() - 1] != EOF {
        return Err(Error::InvalidEnd);
    }
    if bytes[1] == RESERVED_ADDR {
        return Err(Error::InvalidAddress);
    }

    let checksum_at = 6 + payload_len;
    let received = u16::from_be_bytes([bytes[checksum_at], bytes[checksum_at + 1]]);
    let expected = crc16_ccitt_false(&bytes[1..checksum_at]);
    if received != expected {
        return Err(Error::Checksum);
    }

    Ok(Frame {
        addr: bytes[1],
        cmd: bytes[2],
        sequence: u16::from_be_bytes([bytes[3], bytes[4]]),
        payload: &bytes[6..checksum_at],
    })
}

/// CRC-16/CCITT-FALSE: poly=0x1021, init=0xFFFF, refin=false,
/// refout=false, xorout=0x0000.
pub fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::vec::Vec;

    #[derive(Deserialize)]
    struct VectorSet {
        protocol_version: std::string::String,
        crc_check_ascii_123456789: std::string::String,
        vectors: Vec<GoldenVector>,
        invalid_vectors: Vec<InvalidVector>,
    }

    #[derive(Deserialize)]
    struct GoldenVector {
        name: std::string::String,
        addr: u8,
        cmd: u8,
        sequence: u16,
        payload_hex: std::string::String,
        frame_hex: std::string::String,
    }

    #[derive(Deserialize)]
    struct InvalidVector {
        name: std::string::String,
        frame_hex: std::string::String,
        error: std::string::String,
    }

    fn hex_nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("invalid hexadecimal fixture"),
        }
    }

    fn decode_hex(hex: &str) -> Vec<u8> {
        let bytes = hex.as_bytes();
        assert_eq!(bytes.len() % 2, 0, "odd-length hexadecimal fixture");
        bytes
            .chunks_exact(2)
            .map(|pair| (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]))
            .collect()
    }

    fn error_name(error: Error) -> &'static str {
        match error {
            Error::TooShort => "TooShort",
            Error::InvalidStart => "InvalidStart",
            Error::InvalidAddress => "InvalidAddress",
            Error::PayloadTooLong => "PayloadTooLong",
            Error::LengthMismatch => "LengthMismatch",
            Error::InvalidEnd => "InvalidEnd",
            Error::Checksum => "Checksum",
            Error::OutputTooSmall => "OutputTooSmall",
        }
    }

    #[test]
    fn matches_every_cloud_golden_vector() {
        let vectors: VectorSet =
            serde_json::from_str(include_str!("../protocol_vectors.json")).unwrap();
        assert_eq!(vectors.protocol_version, PROTOCOL_VERSION);
        assert_eq!(vectors.crc_check_ascii_123456789, "29b1");
        assert_eq!(crc16_ccitt_false(b"123456789"), 0x29B1);

        for vector in vectors.vectors {
            let payload = decode_hex(&vector.payload_hex);
            let expected = decode_hex(&vector.frame_hex);
            let frame = Frame {
                addr: vector.addr,
                cmd: vector.cmd,
                sequence: vector.sequence,
                payload: &payload,
            };
            let mut encoded = [0u8; MAX_FRAME_LEN];
            let encoded_len = encode(&frame, &mut encoded).unwrap();
            assert_eq!(&encoded[..encoded_len], expected, "{} encode", vector.name);

            let decoded = decode(&expected).unwrap();
            assert_eq!(decoded.addr, vector.addr, "{} address", vector.name);
            assert_eq!(decoded.cmd, vector.cmd, "{} command", vector.name);
            assert_eq!(
                decoded.sequence, vector.sequence,
                "{} sequence",
                vector.name
            );
            assert_eq!(decoded.payload, payload, "{} payload", vector.name);
        }
    }

    #[test]
    fn rejects_every_invalid_cloud_vector() {
        let vectors: VectorSet =
            serde_json::from_str(include_str!("../protocol_vectors.json")).unwrap();
        for vector in vectors.invalid_vectors {
            let frame = decode_hex(&vector.frame_hex);
            let error = decode(&frame).unwrap_err();
            assert_eq!(error_name(error), vector.error, "{}", vector.name);
        }
    }

    #[test]
    fn reports_too_small_output_and_oversized_encode_payload() {
        let frame = Frame {
            addr: FIRST_NODE_ADDR,
            cmd: CMD_POLL,
            sequence: 1,
            payload: &[],
        };
        assert_eq!(
            encode(&frame, &mut [0u8; FRAME_OVERHEAD - 1]),
            Err(Error::OutputTooSmall)
        );

        let payload = [0u8; MAX_PAYLOAD_LEN + 1];
        let frame = Frame {
            payload: &payload,
            ..frame
        };
        let mut output = [0u8; MAX_FRAME_LEN];
        assert_eq!(encode(&frame, &mut output), Err(Error::PayloadTooLong));
    }
}
