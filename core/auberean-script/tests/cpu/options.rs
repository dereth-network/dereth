//! The player's options profile: the chat options it answers.

use auberean_script::options::{GameplayOptions, Table};

/// A profile no packed form filled answers the chat options a new profile holds: the four chat
/// windows' filters, every text kind small, the kinds' colors, the devoted windows' channels (say,
/// general, the magic and missile combat kinds) and the kinds that pop the chat up; a profile
/// whose packed form carried a table answers from it, a key it lacks as 0 (a devoted window's
/// channel as the say channel).
///
/// Behaviour: script.natives.the-chat-options-answer-a-new-profiles-defaults
#[test]
fn the_chat_options_answer_a_new_profiles_defaults() {
    let new = GameplayOptions::default();
    assert_eq!(new.filter(1), 0x60_0000 | 0x17F);
    assert_eq!(new.filter(4), 0x0002_0000);
    assert_eq!(new.filter(5), 0);
    assert_eq!(new.chat_font_size(0x10), 0);
    assert_eq!(new.chat_font_color(0x2), 1);
    assert_eq!(new.chat_font_color(0x100), 9);
    assert_eq!(new.chat_font_color(0x2_0000), 8);
    assert_eq!(new.devoted_channel(2), 0x8_0000);
    assert_eq!(new.devoted_channel(9), 0x10);
    assert_eq!(new.chat_popup_flag(0x100), 1);
    assert_eq!(new.chat_popup_flag(0x10), 0);

    let read = GameplayOptions {
        filters: Some(Table {
            header: 0x0200_0001,
            entries: vec![(2, 0x10)],
        }),
        devoted_windows: Some(Table {
            header: 0x0200_0000,
            entries: Vec::new(),
        }),
        font_colors: Some(Table {
            header: 0x0200_0001,
            entries: vec![(0x2, 3)],
        }),
        popup_flags: Table {
            header: 0x0200_0001,
            entries: vec![(0x10, 1)],
        },
        ..GameplayOptions::default()
    };
    assert_eq!(read.filter(2), 0x10);
    assert_eq!(read.filter(1), 0);
    assert_eq!(read.devoted_channel(1), 0x10);
    assert_eq!(read.chat_font_color(0x2), 3);
    assert_eq!(read.chat_font_color(0x100), 0);
    assert_eq!(read.chat_popup_flag(0x10), 1);
    assert_eq!(read.chat_popup_flag(0x100), 0);
}

/// A profile's aliases are a table of 47 buckets keyed by the wide strings' hash: a new alias goes
/// to the head of its bucket's chain, a name held takes its new text in place, and the table packs
/// bucket by bucket; removing and flushing keep the count in the header.
///
/// Behaviour: script.natives.a-profiles-aliases-pack-in-their-tables-order
#[test]
fn a_profiles_aliases_go_to_the_head_of_their_bucket_and_pack_in_bucket_order() {
    use auberean_script::wire::In;
    let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let mut p = GameplayOptions {
        bits: auberean_script::options::VERSIONED,
        ..GameplayOptions::default()
    };
    // ";a" hashes to 1041 (bucket 7), "A0" to 1088 (bucket 7), ";b" to 1042 (bucket 8), ";zz" to
    // 17178 (bucket 23).
    for (k, v) in [
        (";zz", "3"),
        (";b", "2"),
        (";a", "1"),
        ("A0", "0"),
        (";b", "two"),
    ] {
        assert!(p.set_alias(&w(k), &w(v)));
    }
    let order: Vec<String> = p
        .aliases
        .entries
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                String::from_utf16_lossy(k),
                String::from_utf16_lossy(v)
            )
        })
        .collect();
    assert_eq!(order, ["A0=0", ";a=1", ";b=two", ";zz=3"]);
    assert_eq!(p.aliases.header, 0x0200_0004);
    let back = GameplayOptions::read(&mut In::new(&p.pack())).expect("reads back");
    assert_eq!(back.aliases, p.aliases);
    assert_eq!(p.alias(&w(";b")), Some(&w("two")[..]));
    assert_eq!(p.alias(&w(";B")), None, "names compare exactly");
    assert_eq!(
        String::from_utf16_lossy(&p.alias_list()).lines().next(),
        Some("alias [A0] == [0]")
    );
    assert!(p.remove_alias(&w(";a")) && !p.remove_alias(&w(";a")));
    assert_eq!(p.aliases.header, 0x0200_0003);
    p.flush_aliases();
    assert_eq!(p.aliases.header, 0x0200_0000);
}
