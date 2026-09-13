//! A dependency-free unary gRPC client over cleartext HTTP/2 (h2c), enough
//! for netsimd's frontend: one request message, one response message, no
//! TLS, default flow-control windows, the request headers sent as HPACK
//! literals (no dynamic table, no Huffman coding), the response headers
//! kept raw. The protobuf helpers encode and decode the wire format
//! (varints, length-delimited fields) without a schema compiler.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

// ===== protobuf wire format =====

/// Appends a base-128 varint.
pub fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Reads a varint; the value and the bytes consumed.
pub fn read_varint(bytes: &[u8]) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err("truncated or overlong varint".to_string())
}

fn tag(field: u32, wire_type: u8, out: &mut Vec<u8>) {
    varint(u64::from(field) << 3 | u64::from(wire_type), out);
}

/// A varint field (wire type 0): integers, booleans, enums.
pub fn field_varint(field: u32, value: u64, out: &mut Vec<u8>) {
    tag(field, 0, out);
    varint(value, out);
}

/// A length-delimited field (wire type 2): bytes, strings, messages.
pub fn field_bytes(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    tag(field, 2, out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

/// One decoded field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value<'a> {
    /// Wire type 0.
    Varint(u64),
    /// Wire type 2.
    Bytes(&'a [u8]),
    /// Wire type 5.
    Fixed32(u32),
    /// Wire type 1.
    Fixed64(u64),
}

/// Every field of a message, in order (repeated fields appear repeatedly).
pub fn fields(mut bytes: &[u8]) -> Result<Vec<(u32, Value<'_>)>, String> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let (key, used) = read_varint(bytes)?;
        bytes = &bytes[used..];
        let field = u32::try_from(key >> 3).map_err(|_| "field number too large")?;
        let value = match key & 7 {
            0 => {
                let (value, used) = read_varint(bytes)?;
                bytes = &bytes[used..];
                Value::Varint(value)
            }
            1 => {
                let raw = bytes.get(..8).ok_or("truncated fixed64")?;
                bytes = &bytes[8..];
                Value::Fixed64(u64::from_le_bytes(raw.try_into().unwrap()))
            }
            2 => {
                let (length, used) = read_varint(bytes)?;
                let length = usize::try_from(length).map_err(|_| "length too large")?;
                let payload = bytes
                    .get(used..used + length)
                    .ok_or("truncated length-delimited field")?;
                bytes = &bytes[used + length..];
                Value::Bytes(payload)
            }
            5 => {
                let raw = bytes.get(..4).ok_or("truncated fixed32")?;
                bytes = &bytes[4..];
                Value::Fixed32(u32::from_le_bytes(raw.try_into().unwrap()))
            }
            other => return Err(format!("unsupported wire type {other} for field {field}")),
        };
        out.push((field, value));
    }
    Ok(out)
}

/// The first length-delimited value of `field` in a message.
pub fn message_field(message: &[u8], field: u32) -> Result<Option<&[u8]>, String> {
    Ok(fields(message)?
        .into_iter()
        .find_map(|(number, value)| match value {
            Value::Bytes(bytes) if number == field => Some(bytes),
            _ => None,
        }))
}

/// The first varint value of `field` in a message.
pub fn varint_field(message: &[u8], field: u32) -> Result<Option<u64>, String> {
    Ok(fields(message)?
        .into_iter()
        .find_map(|(number, value)| match value {
            Value::Varint(varint) if number == field => Some(varint),
            _ => None,
        }))
}

// ===== HTTP/2 framing and HPACK =====

const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const FRAME_DATA: u8 = 0x0;
const FRAME_HEADERS: u8 = 0x1;
const FRAME_RST_STREAM: u8 = 0x3;
const FRAME_SETTINGS: u8 = 0x4;
const FRAME_PING: u8 = 0x6;
const FRAME_GOAWAY: u8 = 0x7;
const FLAG_END_STREAM: u8 = 0x1;
const FLAG_ACK: u8 = 0x1;
const FLAG_END_HEADERS: u8 = 0x4;
const STREAM: u32 = 1;

/// One HTTP/2 frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The frame type.
    pub kind: u8,
    /// The flags byte.
    pub flags: u8,
    /// The stream identifier (31 bits).
    pub stream: u32,
    /// The payload.
    pub payload: Vec<u8>,
}

/// Encodes a frame: 24-bit length, type, flags, reserved bit + stream id.
pub fn encode_frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let length = payload.len();
    let mut out = Vec::with_capacity(9 + length);
    out.extend_from_slice(&[(length >> 16) as u8, (length >> 8) as u8, length as u8]);
    out.push(kind);
    out.push(flags);
    out.extend_from_slice(&(stream & 0x7fff_ffff).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Parses one complete frame from the front of `buffer`: the frame and the
/// bytes it took, or `None` while the buffer is still short of it.
pub fn parse_frame(buffer: &[u8]) -> Option<(Frame, usize)> {
    if buffer.len() < 9 {
        return None;
    }
    let length =
        usize::from(buffer[0]) << 16 | usize::from(buffer[1]) << 8 | usize::from(buffer[2]);
    if buffer.len() < 9 + length {
        return None;
    }
    let stream = u32::from_be_bytes([buffer[5], buffer[6], buffer[7], buffer[8]]) & 0x7fff_ffff;
    Some((
        Frame {
            kind: buffer[3],
            flags: buffer[4],
            stream,
            payload: buffer[9..9 + length].to_vec(),
        },
        9 + length,
    ))
}

/// An HPACK integer with an `n`-bit prefix (RFC 7541 §5.1), the prefix's
/// other bits taken from `first`.
pub fn hpack_int(first: u8, prefix_bits: u8, value: usize, out: &mut Vec<u8>) {
    let max = (1usize << prefix_bits) - 1;
    if value < max {
        out.push(first | value as u8);
        return;
    }
    out.push(first | max as u8);
    let mut rest = value - max;
    while rest >= 128 {
        out.push((rest % 128) as u8 | 0x80);
        rest /= 128;
    }
    out.push(rest as u8);
}

/// A header block of literal fields without indexing (RFC 7541 §6.2.2),
/// names and values as raw octets: valid for any HPACK decoder and free
/// of the dynamic table and Huffman coding.
pub fn hpack_literals(headers: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, value) in headers {
        out.push(0x00);
        hpack_int(0x00, 7, name.len(), &mut out);
        out.extend_from_slice(name.as_bytes());
        hpack_int(0x00, 7, value.len(), &mut out);
        out.extend_from_slice(value.as_bytes());
    }
    out
}

/// Wraps a protobuf message as one gRPC length-prefixed message
/// (uncompressed flag, big-endian length).
pub fn grpc_message(message: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8];
    out.extend_from_slice(&(message.len() as u32).to_be_bytes());
    out.extend_from_slice(message);
    out
}

/// Splits concatenated gRPC length-prefixed messages.
pub fn grpc_messages(mut data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    while !data.is_empty() {
        if data.len() < 5 {
            return Err(format!(
                "truncated gRPC message prefix ({} bytes)",
                data.len()
            ));
        }
        if data[0] != 0 {
            return Err("a compressed gRPC message, which this client does not decode".to_string());
        }
        let length = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        let message = data
            .get(5..5 + length)
            .ok_or_else(|| format!("truncated gRPC message ({length} bytes declared)"))?;
        out.push(message.to_vec());
        data = &data[5 + length..];
    }
    Ok(out)
}

/// RFC 7541 Appendix A: the static table, index 1 to 61.
const STATIC_TABLE: [(&str, &str); 61] = [
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

/// Reads an HPACK integer with an `n`-bit prefix; the value and the bytes
/// consumed.
pub fn hpack_read_int(bytes: &[u8], prefix_bits: u8) -> Option<(usize, usize)> {
    let max = (1usize << prefix_bits) - 1;
    let first = usize::from(*bytes.first()?) & max;
    if first < max {
        return Some((first, 1));
    }
    let mut value = max;
    let mut shift = 0u32;
    for (index, byte) in bytes.iter().enumerate().skip(1) {
        value = value.checked_add((usize::from(byte & 0x7f)).checked_shl(shift)?)?;
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
        shift = shift.checked_add(7)?;
    }
    None
}

/// A best-effort HPACK decoder for one connection's header blocks: indexed
/// fields from the static table and the entries this decoder added to its
/// dynamic table, literal fields with raw strings; a Huffman-coded string
/// is reported as `<huffman>` (its bytes skipped), which keeps the decoder
/// dependency-free and is enough to read `grpc-status`/`grpc-message` as
/// gRPC's C core sends them.
#[derive(Debug, Default)]
pub struct HpackDecoder {
    dynamic: Vec<(String, String)>,
}

impl HpackDecoder {
    fn lookup(&self, index: usize) -> Option<(String, String)> {
        if index == 0 {
            return None;
        }
        if index <= STATIC_TABLE.len() {
            let (name, value) = STATIC_TABLE[index - 1];
            return Some((name.to_string(), value.to_string()));
        }
        self.dynamic.get(index - STATIC_TABLE.len() - 1).cloned()
    }

    fn read_string(bytes: &[u8]) -> Option<(String, usize)> {
        let huffman = bytes.first()? & 0x80 != 0;
        let (length, used) = hpack_read_int(bytes, 7)?;
        let raw = bytes.get(used..used + length)?;
        let text = if huffman {
            "<huffman>".to_string()
        } else {
            String::from_utf8_lossy(raw).into_owned()
        };
        Some((text, used + length))
    }

    /// Decodes one header block, in order; decoding stops at the first
    /// representation it cannot follow.
    pub fn decode(&mut self, mut block: &[u8]) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        while let Some(&first) = block.first() {
            let (name_bits, add) = if first & 0x80 != 0 {
                let Some((index, used)) = hpack_read_int(block, 7) else {
                    break;
                };
                block = &block[used..];
                match self.lookup(index) {
                    Some(field) => fields.push(field),
                    None => break,
                }
                continue;
            } else if first & 0x40 != 0 {
                (6, true)
            } else if first & 0x20 != 0 {
                // A dynamic table size update: no field.
                let Some((_, used)) = hpack_read_int(block, 5) else {
                    break;
                };
                block = &block[used..];
                continue;
            } else {
                (4, false)
            };
            let Some((index, used)) = hpack_read_int(block, name_bits) else {
                break;
            };
            block = &block[used..];
            let name = if index == 0 {
                let Some((name, used)) = Self::read_string(block) else {
                    break;
                };
                block = &block[used..];
                name
            } else {
                match self.lookup(index) {
                    Some((name, _)) => name,
                    None => break,
                }
            };
            let Some((value, used)) = Self::read_string(block) else {
                break;
            };
            block = &block[used..];
            if add {
                self.dynamic.insert(0, (name.clone(), value.clone()));
            }
            fields.push((name, value));
        }
        fields
    }
}

/// The name gRPC gives a status code.
pub fn status_name(code: u32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "unknown",
    }
}

/// What one unary call yielded.
#[derive(Debug, Default)]
pub struct Unary {
    /// The response messages (one for a unary call that succeeded).
    pub messages: Vec<Vec<u8>>,
    /// The raw HPACK header blocks (headers, then trailers), for diagnostics.
    pub header_blocks: Vec<Vec<u8>>,
    /// An `RST_STREAM` error code, if the server reset the stream.
    pub reset: Option<u32>,
    /// A `GOAWAY` error code and debug data, if the server sent one.
    pub goaway: Option<(u32, String)>,
}

impl Unary {
    /// The decoded header fields (headers, then trailers).
    pub fn fields(&self) -> Vec<(String, String)> {
        let mut decoder = HpackDecoder::default();
        self.header_blocks
            .iter()
            .flat_map(|block| decoder.decode(block))
            .collect()
    }

    /// The call's `grpc-status` and `grpc-message`, when the trailers carry
    /// them in a form the decoder reads.
    pub fn status(&self) -> Option<(u32, String)> {
        let fields = self.fields();
        let code = fields
            .iter()
            .rev()
            .find(|(name, _)| name == "grpc-status")
            .and_then(|(_, value)| value.parse().ok())?;
        let message = fields
            .iter()
            .rev()
            .find(|(name, _)| name == "grpc-message")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        Some((code, message))
    }

    /// The printable bytes of the header blocks (literal header names and
    /// values such as `grpc-status`/`grpc-message` show through).
    pub fn headers_lossy(&self) -> String {
        self.header_blocks
            .iter()
            .map(|block| {
                block
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii_graphic() || *byte == b' ' {
                            char::from(*byte)
                        } else {
                            '.'
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// One unary gRPC call to `127.0.0.1:port`: `path` is
/// `/package.Service/Method`, `message` the encoded request. Ends at the
/// stream's end (trailers), a reset, a `GOAWAY`, or `timeout`.
pub fn unary(port: u16, path: &str, message: &[u8], timeout: Duration) -> Result<Unary, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("connecting to 127.0.0.1:{port}: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("setting the read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("setting the write timeout: {e}"))?;
    let authority = format!("127.0.0.1:{port}");
    let headers = hpack_literals(&[
        (":method", "POST"),
        (":scheme", "http"),
        (":path", path),
        (":authority", &authority),
        ("content-type", "application/grpc"),
        ("te", "trailers"),
        ("user-agent", "xtask-grpc/0.1"),
    ]);
    let mut request = Vec::new();
    request.extend_from_slice(PREFACE);
    request.extend_from_slice(&encode_frame(FRAME_SETTINGS, 0, 0, &[]));
    request.extend_from_slice(&encode_frame(
        FRAME_HEADERS,
        FLAG_END_HEADERS,
        STREAM,
        &headers,
    ));
    request.extend_from_slice(&encode_frame(
        FRAME_DATA,
        FLAG_END_STREAM,
        STREAM,
        &grpc_message(message),
    ));
    stream
        .write_all(&request)
        .map_err(|e| format!("sending the request: {e}"))?;

    let started = Instant::now();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16384];
    let mut data = Vec::new();
    let mut unary = Unary::default();
    loop {
        while let Some((frame, used)) = parse_frame(&buffer) {
            buffer.drain(..used);
            match frame.kind {
                FRAME_SETTINGS if frame.flags & FLAG_ACK == 0 => {
                    stream
                        .write_all(&encode_frame(FRAME_SETTINGS, FLAG_ACK, 0, &[]))
                        .map_err(|e| format!("acknowledging SETTINGS: {e}"))?;
                }
                FRAME_PING if frame.flags & FLAG_ACK == 0 => {
                    stream
                        .write_all(&encode_frame(FRAME_PING, FLAG_ACK, 0, &frame.payload))
                        .map_err(|e| format!("answering PING: {e}"))?;
                }
                FRAME_HEADERS if frame.stream == STREAM => {
                    unary.header_blocks.push(frame.payload.clone());
                    if frame.flags & FLAG_END_STREAM != 0 {
                        unary.messages = grpc_messages(&data)?;
                        return Ok(unary);
                    }
                }
                FRAME_DATA if frame.stream == STREAM => {
                    data.extend_from_slice(&frame.payload);
                    if frame.flags & FLAG_END_STREAM != 0 {
                        unary.messages = grpc_messages(&data)?;
                        return Ok(unary);
                    }
                }
                FRAME_RST_STREAM if frame.stream == STREAM => {
                    unary.reset = frame
                        .payload
                        .get(..4)
                        .map(|code| u32::from_be_bytes(code.try_into().unwrap()));
                    unary.messages = grpc_messages(&data).unwrap_or_default();
                    return Ok(unary);
                }
                FRAME_GOAWAY => {
                    let code = frame
                        .payload
                        .get(4..8)
                        .map_or(0, |code| u32::from_be_bytes(code.try_into().unwrap()));
                    let debug =
                        String::from_utf8_lossy(frame.payload.get(8..).unwrap_or(&[])).into_owned();
                    unary.goaway = Some((code, debug));
                    unary.messages = grpc_messages(&data).unwrap_or_default();
                    return Ok(unary);
                }
                _ => {}
            }
        }
        if started.elapsed() > timeout {
            return Err(format!(
                "no end of stream within {}s ({} data bytes, headers {:?})",
                timeout.as_secs(),
                data.len(),
                unary.headers_lossy()
            ));
        }
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(format!(
                    "the server closed the connection before the end of stream ({} data bytes, headers {:?})",
                    data.len(),
                    unary.headers_lossy()
                ));
            }
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(e) => return Err(format!("reading the response: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_fields_round_trip() {
        for value in [
            0u64,
            1,
            127,
            128,
            300,
            16_383,
            16_384,
            u32::MAX as u64,
            u64::MAX,
        ] {
            let mut out = Vec::new();
            varint(value, &mut out);
            assert_eq!(read_varint(&out).unwrap(), (value, out.len()), "{value}");
        }
        assert!(read_varint(&[0x80]).is_err());

        let mut inner = Vec::new();
        field_varint(1, 4, &mut inner);
        field_bytes(3, b"bleradar-beacon", &mut inner);
        let mut outer = Vec::new();
        field_bytes(6, &inner, &mut outer);
        field_varint(2, 300, &mut outer);
        let decoded = fields(&outer).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], (6, Value::Bytes(inner.as_slice())));
        assert_eq!(decoded[1], (2, Value::Varint(300)));
        assert_eq!(message_field(&outer, 6).unwrap(), Some(inner.as_slice()));
        assert_eq!(varint_field(&inner, 1).unwrap(), Some(4));
        assert_eq!(varint_field(&inner, 9).unwrap(), None);
        assert_eq!(
            message_field(&inner, 3).unwrap(),
            Some(b"bleradar-beacon".as_slice())
        );
        assert!(fields(&[0x0a, 0x05, 0x01]).is_err(), "truncated field");
        assert!(fields(&[0x0f]).is_err(), "wire type 7");
    }

    #[test]
    fn frames_round_trip_and_partial_buffers_wait() {
        let frame = encode_frame(FRAME_DATA, FLAG_END_STREAM, 1, b"hello");
        assert_eq!(frame.len(), 14);
        assert_eq!(&frame[..3], &[0, 0, 5]);
        let (parsed, used) = parse_frame(&frame).unwrap();
        assert_eq!(used, 14);
        assert_eq!(
            parsed,
            Frame {
                kind: FRAME_DATA,
                flags: FLAG_END_STREAM,
                stream: 1,
                payload: b"hello".to_vec()
            }
        );
        assert!(parse_frame(&frame[..13]).is_none());
        assert!(parse_frame(&[]).is_none());
        // The reserved bit is masked off the stream id.
        let mut reserved = encode_frame(FRAME_SETTINGS, 0, 0, &[]);
        reserved[5] |= 0x80;
        assert_eq!(parse_frame(&reserved).unwrap().0.stream, 0);
    }

    #[test]
    fn hpack_integers_and_literals_follow_rfc_7541() {
        // RFC 7541 C.1: 10 in a 5-bit prefix; 1337 in a 5-bit prefix; 42 in 8 bits.
        let mut out = Vec::new();
        hpack_int(0, 5, 10, &mut out);
        assert_eq!(out, [0x0a]);
        out.clear();
        hpack_int(0, 5, 1337, &mut out);
        assert_eq!(out, [0x1f, 0x9a, 0x0a]);
        out.clear();
        hpack_int(0, 8, 42, &mut out);
        assert_eq!(out, [0x2a]);

        // RFC 7541 C.2.2: a literal header field without indexing, new name.
        let block = hpack_literals(&[(":path", "/sample/path")]);
        assert_eq!(
            block,
            [
                0x00, 0x05, b':', b'p', b'a', b't', b'h', 0x0c, b'/', b's', b'a', b'm', b'p', b'l',
                b'e', b'/', b'p', b'a', b't', b'h'
            ]
        );
        let long_value = "x".repeat(200);
        let block = hpack_literals(&[("te", &long_value)]);
        assert_eq!(&block[..4], &[0x00, 0x02, b't', b'e']);
        assert_eq!(&block[4..6], &[0x7f, 200 - 127]);
        assert_eq!(block.len(), 6 + 200);
    }

    #[test]
    fn grpc_messages_are_framed_and_split() {
        let one = grpc_message(b"abc");
        assert_eq!(one, [0, 0, 0, 0, 3, b'a', b'b', b'c']);
        let mut two = one.clone();
        two.extend_from_slice(&grpc_message(b""));
        assert_eq!(
            grpc_messages(&two).unwrap(),
            vec![b"abc".to_vec(), Vec::new()]
        );
        assert!(grpc_messages(&one[..6]).is_err());
        assert!(grpc_messages(&[1, 0, 0, 0, 0]).is_err(), "compressed");
        assert!(grpc_messages(&[]).unwrap().is_empty());
    }

    #[test]
    fn hpack_blocks_decode_literals_indexes_and_the_dynamic_table() {
        // RFC 7541 C.2.1: literal with incremental indexing, new name.
        let mut decoder = HpackDecoder::default();
        let block = [
            0x40, 0x0a, b'c', b'u', b's', b't', b'o', b'm', b'-', b'k', b'e', b'y', 0x0d, b'c',
            b'u', b's', b't', b'o', b'm', b'-', b'h', b'e', b'a', b'd', b'e', b'r',
        ];
        assert_eq!(
            decoder.decode(&block),
            vec![("custom-key".to_string(), "custom-header".to_string())]
        );
        // The entry now sits at dynamic index 62; C.2.4: `:method: GET` is static index 2.
        assert_eq!(
            decoder.decode(&[0x82, 0xbe]),
            vec![
                (":method".to_string(), "GET".to_string()),
                ("custom-key".to_string(), "custom-header".to_string())
            ]
        );
        // C.2.2: literal without indexing, new name; C.2.3: never indexed.
        let mut fresh = HpackDecoder::default();
        assert_eq!(
            fresh.decode(&[
                0x00, 0x05, b':', b'p', b'a', b't', b'h', 0x03, b'a', b'b', b'c'
            ]),
            vec![(":path".to_string(), "abc".to_string())]
        );
        assert_eq!(
            fresh.decode(&[
                0x10, 0x08, b'p', b'a', b's', b's', b'w', b'o', b'r', b'd', 0x01, b'x'
            ]),
            vec![("password".to_string(), "x".to_string())]
        );
        // What gRPC's C core sent netsimd's trailers as: literals with
        // incremental indexing on static index 31 (content-type) and new names.
        let trailers = [
            0x5f, 0x10, b'a', b'p', b'p', b'l', b'i', b'c', b'a', b't', b'i', b'o', b'n', b'/',
            b'g', b'r', b'p', b'c', 0x40, 0x0b, b'g', b'r', b'p', b'c', b'-', b's', b't', b'a',
            b't', b'u', b's', 0x02, b'1', b'2', 0x40, 0x0c, b'g', b'r', b'p', b'c', b'-', b'm',
            b'e', b's', b's', b'a', b'g', b'e', 0x00,
        ];
        let unary = Unary {
            header_blocks: vec![trailers.to_vec()],
            ..Unary::default()
        };
        assert_eq!(unary.status(), Some((12, String::new())));
        assert_eq!(status_name(12), "UNIMPLEMENTED");
        // A Huffman-coded value is skipped, not decoded; a size update is no field.
        let mut huff = HpackDecoder::default();
        assert_eq!(
            huff.decode(&[0x3f, 0xe1, 0x1f, 0x00, 0x01, b'a', 0x82, 0xff, 0xff]),
            vec![("a".to_string(), "<huffman>".to_string())]
        );
        assert_eq!(hpack_read_int(&[0x1f, 0x9a, 0x0a], 5), Some((1337, 3)));
        assert_eq!(hpack_read_int(&[], 5), None);
    }

    #[test]
    fn header_blocks_render_lossy_for_diagnostics() {
        let unary = Unary {
            header_blocks: vec![b"\x00\x0bgrpc-status\x010".to_vec()],
            ..Unary::default()
        };
        assert_eq!(unary.headers_lossy(), "..grpc-status.0");
    }
}
