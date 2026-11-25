//! Qualities as the client packs them.

use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::Qualities;
use auberean_protocol::wire::{Field, Reader, Writer};

fn packed(q: &Qualities) -> Vec<u8> {
    let mut w = Writer::default();
    q.write(&mut w);
    w.finish()
}

/// Qualities pack with the header worked out from them, not the one they hold: a table holding an
/// entry is packed with its bit though the header left it out, and a bit naming an empty table is
/// dropped; the description's bit is the header's own.
///
/// Behaviour: protocol.qualities.packed-with-the-header-worked-out
#[test]
fn qualities_pack_with_the_header_worked_out_from_their_tables() {
    let q = Qualities {
        header: Qualities::BOOLS | Qualities::STRINGS,
        did: 0x8100_0530,
        ints: Table::from_inserted(SMALL, [(0x102, 5), (0x103, -6)]),
        floats: Table::from_inserted(SMALL, [(1, 2.5)]),
        ..Qualities::default()
    };
    let bytes = packed(&q);
    let header = u32::from_le_bytes(bytes[..4].try_into().expect("a header word"));
    assert_eq!(header, Qualities::INTS | Qualities::FLOATS);
    assert_eq!(header, q.pack_header());
    assert_eq!(bytes.len(), q.pack_size());

    let mut r = Reader::new(&bytes);
    let back = Qualities::read(&mut r).expect("the packed qualities read");
    r.finish().expect("every byte read");
    assert_eq!(back.header, header);
    assert_eq!((back.ints, back.floats), (q.ints.clone(), q.floats.clone()));

    let only_description = Qualities {
        header: Qualities::WEENIE | Qualities::INTS,
        ..Qualities::default()
    };
    let bytes = packed(&only_description);
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().expect("a header word")),
        Qualities::WEENIE
    );
}
