//! Asheron's Call 2 messages: stored whole, filed under their type.
//!
//! AC2 shares AC1's transport, so its sessions are framed, checksummed and reassembled exactly as
//! AC1's; only the application layer differs, and there is no AC2 codec yet. An AC2 message is
//! therefore not decoded: its row keeps the whole blob (`raw`), its first dword (`opcode`) and
//! its type (`mtype`), with status `ac2`.
//!
//! **`mtype` is the full first dword**, `0x0001_xxxx`, not the low u16. The AC2 type number is
//! `mtype & 0xFFFF`; keeping the `0x0001` half keeps AC2 types apart from AC1 types of the same
//! low value (AC2 `0x10086` against AC1 `0x0086`) in every query that groups or joins on `mtype`,
//! including the join to AC1's `opcode` table, which then names nothing. A message whose first
//! dword is not of the AC2 form is filed the same way, under that dword.
//!
//! [`decode`] is the seam for a future AC2 codec crate: it receives every message of an AC2
//! session and returns the same [`Decoded`] the AC1 decoder does, so a codec plugs in here
//! (status `ok`/`error`, `codec`, `fields`, object ids) without touching the transport or the
//! index.

use crate::decode::{Decoded, Status};
use crate::flows::Dir;

/// File one reassembled AC2 blob travelling `dir`.
#[must_use]
pub fn decode(dir: Dir, blob: &[u8]) -> Decoded {
    let _ = dir;
    let opcode = blob
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    Decoded {
        opcode: opcode.unwrap_or(0),
        sub_opcode: None,
        mtype: opcode.unwrap_or(0),
        order_iid: None,
        order_stamp: None,
        status: if opcode.is_some() {
            Status::Ac2
        } else {
            Status::Short
        },
        codec: None,
        failure: None,
        padding: 0,
        fields: None,
        guids: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ac2_message_is_filed_whole_under_its_full_type_dword() {
        let blob = [
            0x86, 0x00, 0x01, 0x00, 0xB0, 0xF7, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8,
        ];
        let d = decode(Dir::S2c, &blob);
        assert_eq!(
            (d.opcode, d.sub_opcode, d.mtype),
            (0x0001_0086, None, 0x0001_0086)
        );
        assert_eq!(d.status, Status::Ac2);
        assert_eq!((d.codec, d.fields, d.guids.len()), (None, None, 0));
        assert_eq!(decode(Dir::C2s, &[0x8E, 0]).status, Status::Short);
    }
}
