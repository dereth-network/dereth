//! Little-endian fields within admitted packet sections.

pub(crate) fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let s = buf.get(at..at + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub(crate) fn read_u16(buf: &[u8], at: usize) -> Option<u16> {
    let s = buf.get(at..at + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

pub(crate) fn take_u32(buf: &[u8], at: &mut usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let value = read_u32(buf, *at)?;
    *at = end;
    Some(value)
}
