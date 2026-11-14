//! GStreamer 1.26 D3D12 IPC wire-format interoperability, without GStreamer linkage.
// Protocol reference: GStreamer 1.26.11 sys/d3d12/gstd3d12ipc.{h,cpp}.
// This implementation exchanges metadata/OS handles only; video pixels are shared textures.
use std::io::{self, Read, Write};

#[cfg(all(windows, feature = "composition"))]
pub mod composition;
#[cfg(all(windows, feature = "gpu"))]
pub mod gpu;
#[cfg(all(windows, feature = "wgpu"))]
pub mod receiver;
#[cfg(all(windows, feature = "ui"))]
pub mod view;

const MAGIC: u32 = 0xD3D1210C;
const HEADER: usize = 9;
const MAX_PAYLOAD: usize = 65536;

#[derive(Debug, PartialEq)]
pub enum Packet {
    Config {
        pid: u32,
        adapter_luid: i64,
        fence_handle: u64,
        caps: String,
    },
    Frame {
        /// Absolute system presentation time; never interpret this as media seek position.
        pts_ns: u64,
        pitch: u32,
        offsets: [u32; 4],
        texture_handle: u64,
        fence_value: u64,
        caps: Option<String>,
    },
    Eos,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid media IPC packet")
}

pub fn read_packet(reader: &mut impl Read) -> io::Result<Packet> {
    let mut header = [0u8; HEADER];
    reader.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header[1..5].try_into().unwrap()) as usize;
    if u32::from_le_bytes(header[5..9].try_into().unwrap()) != MAGIC || length > MAX_PAYLOAD {
        return Err(invalid());
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    parse_payload(header[0], &payload)
}

fn number<const N: usize>(bytes: &[u8], offset: usize) -> io::Result<[u8; N]> {
    bytes
        .get(offset..offset + N)
        .ok_or_else(invalid)?
        .try_into()
        .map_err(|_| invalid())
}

fn caps(bytes: &[u8]) -> io::Result<String> {
    let bytes = bytes.strip_suffix(&[0]).ok_or_else(invalid)?;
    if bytes.contains(&0) {
        return Err(invalid());
    }
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| invalid())
}

fn parse_payload(kind: u8, bytes: &[u8]) -> io::Result<Packet> {
    match kind {
        1 => Ok(Packet::Config {
            pid: u32::from_le_bytes(number(bytes, 0)?),
            adapter_luid: i64::from_le_bytes(number(bytes, 4)?),
            fence_handle: u64::from_le_bytes(number(bytes, 12)?),
            caps: caps(bytes.get(20..).ok_or_else(invalid)?)?,
        }),
        3 => {
            let changed_caps = match bytes.get(44) {
                Some(0) if bytes.len() == 45 => None,
                Some(1) => Some(caps(bytes.get(45..).ok_or_else(invalid)?)?),
                _ => return Err(invalid()),
            };
            Ok(Packet::Frame {
                pts_ns: u64::from_le_bytes(number(bytes, 0)?),
                pitch: u32::from_le_bytes(number(bytes, 8)?),
                offsets: [12, 16, 20, 24]
                    .map(|offset| number(bytes, offset).map(u32::from_le_bytes))
                    .into_iter()
                    .collect::<io::Result<Vec<_>>>()?
                    .try_into()
                    .unwrap(),
                texture_handle: u64::from_le_bytes(number(bytes, 28)?),
                fence_value: u64::from_le_bytes(number(bytes, 36)?),
                caps: changed_caps,
            })
        }
        6 if bytes.is_empty() => Ok(Packet::Eos),
        _ => Err(invalid()),
    }
}

fn send(writer: &mut impl Write, kind: u8, payload: &[u8]) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    bytes.push(kind);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(payload);
    writer.write_all(&bytes)?;
    writer.flush()
}

pub fn request_frame(writer: &mut impl Write) -> io::Result<()> {
    send(writer, 2, &[])
}
pub fn frame_received(writer: &mut impl Write) -> io::Result<()> {
    send(writer, 4, &[])
}
pub fn release_frame(writer: &mut impl Write, server_handle: u64) -> io::Result<()> {
    send(writer, 5, &server_handle.to_le_bytes())
}
pub fn finish(writer: &mut impl Write) -> io::Result<()> {
    send(writer, 7, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_packed_64_bit_windows_layout() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&42u32.to_le_bytes());
        payload.extend_from_slice(&(-1i64).to_le_bytes());
        payload.extend_from_slice(&123u64.to_le_bytes());
        payload.extend_from_slice(b"video/x-raw,format=RGBA\0");
        let mut wire = Vec::new();
        send(&mut wire, 1, &payload).unwrap();
        assert_eq!(
            read_packet(&mut wire.as_slice()).unwrap(),
            Packet::Config {
                pid: 42,
                adapter_luid: -1,
                fence_handle: 123,
                caps: "video/x-raw,format=RGBA".into()
            }
        );
    }

    #[test]
    fn metadata_never_allocates_from_unbounded_or_truncated_headers() {
        for length in [65537u32, u32::MAX] {
            let mut wire = vec![1];
            wire.extend_from_slice(&length.to_le_bytes());
            wire.extend_from_slice(&MAGIC.to_le_bytes());
            assert!(read_packet(&mut wire.as_slice()).is_err());
        }
        assert!(parse_payload(1, &[0u8; 20]).is_err());
        assert!(parse_payload(3, &[0u8; 44]).is_err());
        assert!(parse_payload(3, &[2u8; 45]).is_err());
    }

    #[test]
    fn frame_offsets_and_release_handshake_match_wire_format() {
        let mut payload = vec![0u8; 45];
        payload[0..8].copy_from_slice(&999u64.to_le_bytes());
        payload[8..12].copy_from_slice(&1280u32.to_le_bytes());
        payload[28..36].copy_from_slice(&0x123456789u64.to_le_bytes());
        payload[36..44].copy_from_slice(&12u64.to_le_bytes());
        assert!(matches!(
            parse_payload(3, &payload).unwrap(),
            Packet::Frame {
                pts_ns: 999,
                pitch: 1280,
                texture_handle: 0x123456789,
                fence_value: 12,
                ..
            }
        ));
        let mut wire = Vec::new();
        release_frame(&mut wire, 0x123456789).unwrap();
        assert_eq!(wire[0], 5);
        assert_eq!(&wire[9..], &0x123456789u64.to_le_bytes());
    }
}
