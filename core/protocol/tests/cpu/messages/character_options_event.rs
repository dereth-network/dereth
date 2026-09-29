//! Behaviour: none (checks data formats, fixture conformance or host contracts)
//! All five recorded 0x01A1 blobs round-trip byte for byte with literal offsets; the recorded
//! option flags are those the pack header can produce; the options words are the right way round.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_protocol::actions::{pack_action, unpack_action};
use dereth_protocol::login::{player_module_flags, CharacterCharacterOptionsEvent, PlayerModule};
use dereth_protocol::{Message, Reader};

fn hex(parts: &[&str]) -> Vec<u8> {
    let s: String = parts.concat();
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex digit"))
        .collect()
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `first-login-walk-jump` blob 212, t=138.74 s, stamp 25. **All eight spell bars empty**, and the
/// one of the five whose gameplay-options bag is 129 bytes -- so it is the only blob here that
/// exercises `PlayerModule`'s trailing align-to-4 with a non-zero pad (3 bytes).
const FIRST_LOGIN_WALK_JUMP: &[&str] = &[
    "b1f7000019000000a1010000600600004aa5c45000000000000000000000000000000000000000000000000000000000",
    "00000000ff3f0000008794020200000000018c0000108c000010110000008b00001000008b00001000008b0000100000",
    "8b00001000008b00001000008b00001000008b00001000008b00001000008b00001000008b00001000008b0000100000",
    "8b00001000008b00001000018a0000108a000010008b00001000008b00001000008b00001000008b0000100000000000",
];

/// `early-inventory-and-casting` blob 1459, t=267.75 s, stamp 288. Bar 0 carries seven favourite
/// spells. `short-second-connection` blob 96 (stamp 7) has a **byte-identical body** and so is
/// rebuilt from this one rather than pasted twice; the test asserts that they really do agree.
const EARLY_INVENTORY_AND_CASTING: &[&str] = &[
    "b1f7000020010000a1010000600600004aa5c45007000000180000008d050000060000001b000000d504000019000000",
    "0700000000000000000000000000000000000000000000000000000000000000ff3f0000008794020200000000018c00",
    "00108c000010110000008b00001000008b00001000008b00001000008b00001000008b00001000008b00001000028800",
    "001088000010620200008900001089000010780000008b00001000008b00001000008b00001000008b00001000008b00",
    "001000008b00001000008b00001000008b00001000008b00001000008b00001000008b0000100000",
];

/// `long-solo-play` blob 4426, t=583.58 s, stamp 985. The largest gameplay-options bag in the
/// corpus (252 bytes) -- the session where the player moved most of the windows.
const LONG_SOLO_PLAY_A: &[&str] = &[
    "b1f70000d9030000a1010000600600004aa5c450070000001800000023000000e70500001b0000000600000019000000",
    "1200000000000000000000000000000000000000000000000000000000000000ff3f0000008794020200000000018c00",
    "00108c000010110000008b00001000008b00001000008b00001000008b00001000008b00001000008b00001000048600",
    "0010860000101c0000008700001087000010070100008800001088000010620200008900001089000010780000008b00",
    "001000008b00001000048600001086000010000000008700001087000010f401000088000010880000109a0100008900",
    "001089000010640000008b00001000028600001086000010e80100008700001087000010810000008b00001000008b00",
    "001000008b00001000008b00001000018a0000108a000010008b00001000008b00001000008b00001000008b00001000",
    "00000000",
];

/// `long-solo-play` blob 8920, t=1088.42 s, stamp 2178. The same character eight minutes later with
/// an eighth favourite spell on bar 0, which is the change that produced the message.
const LONG_SOLO_PLAY_B: &[&str] = &[
    "b1f7000082080000a1010000600600004aa5c450080000001800000023000000e70500001b0000005600000006000000",
    "190000001200000000000000000000000000000000000000000000000000000000000000ff3f00000087940202000000",
    "00018c0000108c000010110000008b00001000008b00001000008b00001000008b00001000008b00001000008b000010",
    "000486000010860000101c00000087000010870000100701000088000010880000106202000089000010890000107800",
    "00008b00001000008b00001000048600001086000010000000008700001087000010f401000088000010880000109a01",
    "00008900001089000010640000008b00001000028600001086000010e80100008700001087000010810000008b000010",
    "00008b00001000008b00001000008b00001000018a0000108a000010008b00001000008b00001000008b00001000008b",
    "0000100000000000",
];

/// Every captured blob, with the stamp the recording carried.
fn corpus() -> Vec<(&'static str, u32, Vec<u8>)> {
    vec![
        ("first-login-walk-jump/212", 25, hex(FIRST_LOGIN_WALK_JUMP)),
        (
            "early-inventory-and-casting/1459",
            288,
            hex(EARLY_INVENTORY_AND_CASTING),
        ),
        ("short-second-connection/96", 7, {
            let mut b = hex(EARLY_INVENTORY_AND_CASTING);
            b[4..8].copy_from_slice(&7u32.to_le_bytes());
            b
        }),
        ("long-solo-play/4426", 985, hex(LONG_SOLO_PLAY_A)),
        ("long-solo-play/8920", 2178, hex(LONG_SOLO_PLAY_B)),
    ]
}

/// The five recorded blobs decode, and re-encode **byte for byte including the stamp**.
///
/// The offsets are literals: `0xF7B1` at 0, the stamp at 4, the sub-type `0x01A1` at 8, the body
/// at 12. The character-options event writes the sub-type dword and then packs the player module;
/// the `OrderedActionHeader` is the eight bytes before it.
#[test]
fn every_recorded_options_event_round_trips_byte_for_byte() {
    let all = corpus();
    assert_eq!(
        all.len(),
        5,
        "the corpus contains exactly five 0x01A1 blobs"
    );
    for (name, stamp, blob) in &all {
        assert_eq!(le32(blob, 0), 0xF7B1, "{name}: the game-action envelope");
        assert_eq!(le32(blob, 4), *stamp, "{name}: the action-order stamp");
        assert_eq!(
            le32(blob, 8),
            0x01A1,
            "{name}: Character_CharacterOptionsEvent"
        );

        let action = unpack_action(blob).expect("the corpus blob frames");
        assert_eq!(action.sub_type.0, 0x01A1);
        let mut r = action.body;
        let m = CharacterCharacterOptionsEvent::read(&mut r).expect("the corpus blob decodes");
        r.expect_exhausted()
            .expect("the cursor lands on the end of the record");

        let back = pack_action(*stamp, &m).expect("re-encodes");
        assert_eq!(
            back, *blob,
            "{name}: the re-encoded blob is not the captured one"
        );
    }
    // Two of the five carry the same module under different stamps, which is what makes the
    // "one message per change" reading of this opcode wrong: it is a whole-module snapshot.
    assert_eq!(
        all[1].2[12..],
        all[2].2[12..],
        "early-inventory-and-casting and short-second-connection carry the same module"
    );
}

/// What the five blobs actually **exercise**, stated as a fraction rather than implied.
///
/// `PlayerModule` has ten fields. All five recorded blobs carry `option_flags = 0x0660`, so six
/// are exercised (`option_flags`, `options`, `spell_bars`, `spell_filters`, `options2`,
/// `gameplay_options`) and four are not (`shortcuts`, `desired_comps`, `timestamp_format`,
/// `generic_qualities`). `timestamp_format` is unreachable from this direction by construction --
/// The player module's pack has no branch that writes it -- so the reachable gap is three.
///
/// The gate values are literals, not the symbols the codec
/// reads them through.
#[test]
fn the_recorded_option_flags_are_the_ones_setpackheader_can_produce() {
    // The pack header: `| 0x400`, then `| 0x60`, both unconditional.
    const ALWAYS: u32 = 0x0460;
    // The four it sets from a pointer.
    const OPTIONAL: u32 = 0x0001 | 0x0008 | 0x0100 | 0x0200;
    // The two spell-bar shapes and the timestamp string, which it never sets.
    const NEVER: u32 = 0x0004 | 0x0010 | 0x0080;

    for (name, _, blob) in corpus() {
        let flags = le32(&blob, 12);
        assert_eq!(
            flags, 0x0660,
            "{name}: the corpus is unanimous on the gate word"
        );
        assert_eq!(
            flags & ALWAYS,
            ALWAYS,
            "{name}: 0x0460 is unconditional in the pack header"
        );
        assert_eq!(
            flags & NEVER,
            0,
            "{name}: the pack header can never set these"
        );
        assert_eq!(
            flags & !(ALWAYS | OPTIONAL),
            0,
            "{name}: an undocumented gate bit"
        );
    }

    // And the symbols agree with those literals. Without this pair the constants are
    // self-consistent and a transcription error in any of them is unfalsifiable.
    assert_eq!(player_module_flags::SHORTCUT, 0x0001);
    assert_eq!(player_module_flags::MULTI_SPELL_LIST, 0x0004);
    assert_eq!(player_module_flags::DESIRED_COMPS, 0x0008);
    assert_eq!(player_module_flags::EXTENDED_MULTI_SPELL_LISTS, 0x0010);
    assert_eq!(player_module_flags::SPELLBOOK_FILTERS, 0x0020);
    assert_eq!(player_module_flags::CHARACTER_OPTIONS_2, 0x0040);
    assert_eq!(player_module_flags::TIMESTAMP_FORMAT, 0x0080);
    assert_eq!(player_module_flags::GENERIC_QUALITIES_DATA, 0x0100);
    assert_eq!(player_module_flags::GAMEPLAY_OPTIONS, 0x0200);
    assert_eq!(player_module_flags::SPELL_LISTS_8, 0x0400);
}

/// The fixed part of a client-packed `PlayerModule`, addressed by **literal blob offsets**.
///
/// `first-login-walk-jump` is the one blob whose eight spell bars are all empty, so every field
/// after them has a constant offset: 12 body start, +0 `option_flags`, +4 `options_`, +8 the eight
/// `PackableList<unsigned long>` counts (four bytes each, all zero), +40 `spell_filters_`, +44
/// `options2_`, +48 the `PackObjPropertyCollection`. The pack-size calculation starts from 0x10 =
/// 16 bytes for exactly those four dwords.
#[test]
fn the_fixed_part_of_a_client_packed_module_is_at_these_offsets() {
    let blob = hex(FIRST_LOGIN_WALK_JUMP);
    assert_eq!(blob.len(), 192);
    assert_eq!(le32(&blob, 12), 0x0660, "option_flags");
    assert_eq!(
        le32(&blob, 12 + 4),
        0x50C4_A54A,
        "options_ (CharacterOptions1)"
    );
    for bar in 0..8 {
        assert_eq!(le32(&blob, 12 + 8 + bar * 4), 0, "spell bar {bar} is empty");
    }
    assert_eq!(le32(&blob, 12 + 40), 0x3FFF, "spell_filters_");
    assert_eq!(
        le32(&blob, 12 + 44),
        0x0294_8700,
        "options2_ (CharacterOptions2)"
    );
    // The bag's own versioned-archive header.
    assert_eq!(
        le32(&blob, 12 + 48),
        2,
        "the gameplay-options bag begins here"
    );
    // 12 + 48 + 129 = 189, so the player-module writer emits three zero bytes of padding. The rule is
    // `(-blob_offset) & 3` computed from the **blob's** start, which is why the origin matters.
    assert_eq!(
        &blob[189..192],
        &[0, 0, 0],
        "packed player module trailing pad"
    );

    let mut r = Reader::with_origin(&blob[12..], 12);
    let m = PlayerModule::read(&mut r).expect("decodes");
    assert_eq!(
        m.spell_bars.len(),
        8,
        "the pack header always sets 0x0400, so eight bars"
    );
    assert!(m.shortcuts.is_none() && m.desired_comps.is_none());
    assert!(m.timestamp_format.is_none() && m.generic_qualities.is_none());
    assert!(m.gameplay_options.is_some());
}

/// Two option bits read out of the recorded `options_` / `options2_`, as literals.
///
/// Both are tabulated in `docs/networking/messages/03-qualities-and-updates.md` §6; they are
/// re-checked here only because this codec is what carries them, and a swapped pair of dwords
/// would be invisible to a round trip. Oracle:
/// the side-by-side-vitals getter reads `(byte)(options_ >> 0x15) & 1` and its setter
/// writes `options_ | 0x200000`; the UI-lock getter is `options2_` bit 24.
#[test]
fn the_recorded_options_words_are_the_right_way_round() {
    const SIDE_BY_SIDE_VITALS: u32 = 0x0020_0000; // options_  bit 21
    const LOCK_UI: u32 = 0x0100_0000; // options2_ bit 24
                                      // `HearGeneralChat | HearTradeChat | HearLFGChat | LeadMissileTargets | ShowHelm | ShowCloak`
                                      // is what `UnPack` defaults `options2_` to, and every recorded blob still carries all six.
    const DEFAULT_OPTIONS2: u32 = 0x0094_8700;

    for (name, _, blob) in corpus() {
        let options = le32(&blob, 12 + 4);
        let mut r = Reader::with_origin(&blob[12..], 12);
        let m = PlayerModule::read(&mut r).expect("decodes");
        assert_eq!(
            m.options, options,
            "{name}: options_ is the second dword of the module"
        );
        assert_eq!(
            m.options & SIDE_BY_SIDE_VITALS,
            0,
            "{name}: this player used stacked vitals"
        );
        assert_eq!(
            m.options2 & LOCK_UI,
            0,
            "{name}: this player did not lock the UI"
        );
        assert_eq!(
            m.options2 & DEFAULT_OPTIONS2,
            DEFAULT_OPTIONS2,
            "{name}: the six default chat/display bits are still set"
        );
        // Bit 25 of the second options word, hear-PK-deaths, is set in all five: the
        // default word carries it, and so every recorded blob is exactly the default.
        assert_eq!(m.options2, 0x0294_8700, "{name}");
    }
    assert_eq!(
        PlayerModule::DEFAULT_OPTIONS2,
        DEFAULT_OPTIONS2 | 0x0200_0000
    );
    assert_eq!(PlayerModule::DEFAULT_SPELL_FILTERS, 0x3FFF);
}
