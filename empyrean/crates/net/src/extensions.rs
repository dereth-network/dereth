// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Extensions.cs
//
// ACE's `BinaryWriter`/`BinaryReader` extension methods, over a plain byte vector (writer) or a
// stream position (reader). They are shared by the message builders.

use dereth_primitives::text::cp1252;
use empyrean_common::dotnet::CsCast;

// ACE: Extensions.CalculatePadMultiple
#[must_use]
pub const fn calculate_pad_multiple(length: u32, multiple: u32) -> u32 {
    multiple
        .wrapping_mul(length.wrapping_add(multiple).wrapping_sub(1) / multiple)
        .wrapping_sub(length)
}

// ACE: Extensions.WriteString16L
/// `ushort` length in UTF-16 units, the Windows-1252 bytes, zero padding to a multiple of 4
/// (counting the length word). A character 1252 cannot represent becomes `?`, as .NET's
/// best-fit-free `Encoding.GetEncoding(1252)` does. A `null` string is written as empty.
pub fn write_string16l(writer: &mut Vec<u8>, data: Option<&str>) {
    let data = data.unwrap_or("");
    // **The client's long form:** a length of 65,535 or more is the
    // 0xFFFF escape and then a dword, as the client reads it; ACE's `(ushort)data.Length` wrapped.
    let text = cp1252::encode_lossy(data);
    let length = u32::try_from(text.len()).unwrap_or(u32::MAX);
    let prefix = if length >= 0xFFFF {
        writer.extend_from_slice(&0xFFFFu16.to_le_bytes());
        writer.extend_from_slice(&length.to_le_bytes());
        6
    } else {
        let len: u16 = length.cs_cast();
        writer.extend_from_slice(&len.to_le_bytes());
        2
    };
    writer.extend(text);
    let pad = calculate_pad_multiple(prefix + length, 4);
    self::pad(writer, pad);
}

// ACE: Extensions.WritePackedDword
pub fn write_packed_dword(writer: &mut Vec<u8>, value: u32) {
    if value <= 32767 {
        // `Convert.ToUInt16(value)`: in range here, so the plain narrowing.
        let network_value: u16 = value.cs_cast();
        writer.extend_from_slice(&network_value.to_le_bytes());
    } else {
        let packed_value = (value << 16) | ((value >> 16) | 0x8000);
        writer.extend_from_slice(&packed_value.to_le_bytes());
    }
}

// ACE: Extensions.WritePackedDwordOfKnownType
/// ACE's known-type pack: its 2-byte form reaches 0x7FFF, and it subtracts the type only from a
/// value sharing a bit with it. The server does not use it: it packs known-type DataIDs as the
/// retail client does, the 2-byte form only below 0x4000, and sends an id the retail packer
/// refuses as none, logged. Kept as ACE's port.
pub fn write_packed_dword_of_known_type(writer: &mut Vec<u8>, mut value: u32, r#type: u32) {
    if (value & r#type) > 0 {
        value = value.wrapping_sub(r#type);
    }
    write_packed_dword(writer, value);
}

// ACE: Extensions.WriteUInt16BE
pub fn write_u16_be(writer: &mut Vec<u8>, value: u16) {
    writer.extend_from_slice(&value.to_be_bytes());
}

// ACE: Extensions.Pad
pub fn pad(writer: &mut Vec<u8>, pad: u32) {
    writer.resize(writer.len() + pad as usize, 0);
}

// ACE: Extensions.Align
/// Writer form: pads the stream's length to a multiple of 4.
pub fn align(writer: &mut Vec<u8>) {
    // `(uint)writer.BaseStream.Length`, from a `long`.
    let pad = calculate_pad_multiple(i64::try_from(writer.len()).unwrap_or(i64::MAX).cs_cast(), 4);
    self::pad(writer, pad);
}

/// Reader form of ACE `Extensions.Align`: how many bytes to skip from `position` to reach a
/// multiple of 4.
#[must_use]
pub const fn align_reader_skip(position: u32) -> u32 {
    calculate_pad_multiple(position, 4)
}

// ACE: Extensions.WritePosition
/// Overwrites the `uint` at `position`, leaving the writer's end where it was.
pub fn write_position(writer: &mut [u8], value: u32, position: usize) {
    writer[position..position + 4].copy_from_slice(&value.to_le_bytes());
}

// ACE: Extensions.WriteGuid
pub fn write_guid(writer: &mut Vec<u8>, guid_full: u32) {
    writer.extend_from_slice(&guid_full.to_le_bytes());
}

// ACE: Extensions.ReadGuid
#[must_use]
pub fn read_guid(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
}

// ACE: Extensions.BuildPacketString
/// The hex dump ACE's packet log writes: a column ruler, rows of 16 bytes, and a text column in
/// which control characters print as spaces.
#[must_use]
pub fn build_packet_string(bytes: &[u8], start_position: usize, bytes_to_output: usize) -> String {
    use std::fmt::Write as _;
    let columns = 16usize;
    let mut tw = String::new();
    tw.push_str("   x  ");
    for i in 0..columns {
        let _ = write!(tw, "{i:>3}");
    }
    tw.push_str("  |Text\n");
    tw.push_str("   0  ");
    let mut column = 0usize;
    let mut row = 0usize;
    let mut ascii_line = String::new();
    for &b in bytes.iter().skip(start_position).take(bytes_to_output) {
        if column >= columns {
            row += 1;
            column = 0;
            let _ = writeln!(tw, "  |{ascii_line}");
            ascii_line.clear();
            let _ = write!(tw, "{:>4}  ", row * columns);
        }
        let _ = write!(tw, " {b:02X}");
        let c = char::from(b);
        ascii_line.push(if c.is_control() { ' ' } else { c });
        column += 1;
    }
    tw.push_str(&" ".repeat((columns - column) * 3));
    let _ = writeln!(tw, "  |{ascii_line}");
    tw
}
