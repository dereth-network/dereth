//! The count-prefixed enchantment messages: an update (`0x02C4`) puts every entry in the registry;
//! a multiple removal (`0x02C5`) announces each spell and a multiple dispel (`0x02C8`) does not; the
//! single removal prints the expiry line and the single dispel does not; and a cooldown id takes
//! the early return with no line.
//! Fixture: `net::late_receivers`' socket-free replay App, with the recorded enchantment from
//! `fellowship-one-vassal` and production-encoded game events.

#![cfg(gpu)]

use crate::common::gpu_lock;
use crate::net::late_receivers::{chat, describe_player, recorded, settle, setup};

use dereth_client::app::App;
use dereth_client_runtime::dropped;
use dereth_protocol::{Message, Opcode};
use dereth_ui_screens::view::GameView;

/// `(helpful, harmful)` off the **view** the indicator strip reads, not off the tables.
fn counts(app: &App) -> (u32, u32) {
    app.hud().view(app.objects()).enchantment_counts()
}

/// The shard's own `Enchantment`, decoded out of the one `0x02C2` `fellowship-one-vassal` carries.
fn recorded_enchantment() -> dereth_protocol::types::qualities::Enchantment {
    let blob = recorded("fellowship-one-vassal", Opcode::MAGIC_UPDATE_ENCHANTMENT)
        .pop()
        .expect("fellowship carries one 0x02C2");
    let mut r = dereth_protocol::archive::Reader::new(&blob[16..]);
    dereth_protocol::qualities::MagicUpdateEnchantment::read(&mut r)
        .expect("the recorded body")
        .0
}

/// **`0x02C4 Magic_UpdateMultipleEnchantments` reaches the registry, and the strip sees it.**
///
/// The dispatcher unpacks a count-prefixed list of `Enchantment` — a `u32` count and that many 64-byte
/// entries, with no per-entry header. The registry applies each entry in a plain loop and the
/// dispatcher sends **one** enchantments-changed notice after the complete list.
///
/// **Falsified by** replacing `Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS` in the arm with an
/// unreachable opcode: the counts stay `(0, 0)` and `multi_enchantment_updates` stays 0.
#[test]
fn a_count_prefixed_update_puts_every_entry_in_the_registry() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("multi-update");
    describe_player(&mut app, &mut peer);

    assert_eq!(
        counts(&app),
        (0, 0),
        "the recorded 0x0013 carries an empty registry"
    );

    let one = recorded_enchantment();
    // The same 64 bytes with a different spell id: two entries, so `list.len()` and "one message"
    // cannot be confused with each other.
    let mut two = one;
    two.id = (one.id & 0xFFFF_0000) | 0x0019;
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicUpdateMultipleEnchantments(vec![one, two]),
    );
    settle(&mut app);

    let s = &app.interaction().stats;
    assert_eq!(s.multi_enchantment_updates, 1, "one message consumed");
    assert_eq!(
        s.multi_enchantments_applied, 2,
        "and both of its entries accepted"
    );
    // The buff/debuff lamp's input. The split is the **spell table's**, not this file's:
    // The total reads the spell table's bitfield bit 2 (the beneficial bit), and the portal
    // dat says spell `0x18` (the shard's own, out of the recorded `0x02C2`) is
    // beneficial and `0x19` is not. So `(1, 1)` is a stronger reading than `(2, 0)` would have
    // been: it says the totals went through the table for **each** entry rather than being
    // incremented twice by the list length.
    assert_eq!(
        counts(&app),
        (1, 1),
        "one helpful, one harmful -- one per entry"
    );
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("qualities")
            .enchantments
            .enchantments_in_effect()
            .len(),
        2,
        "and the effects pane's own source holds both"
    );
    assert!(!dropped::unreceived(
        Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS
    ));
}

/// Behaviour: magic.enchantments.a-multiple-removal-announces-and-a-dispel-does-not
///
/// **`0x02C5` announces and `0x02C8` is silent, and that is the only difference between them.**
///
/// Multiple removal calls the shared removal path with `notify = true`; multiple dispel reaches
/// the same path with `notify = false`. When notification is enabled, each removed spell produces
/// *"&lt;spell&gt; has expired."* on chat type 7.
///
/// **Falsified by** passing `true` for `notify` on the dispel arm: the second half then prints two
/// more lines and the assertion that the count did not move fails.
#[test]
fn a_multiple_removal_announces_and_a_multiple_dispel_does_not() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("multi-remove");
    describe_player(&mut app, &mut peer);

    let one = recorded_enchantment();
    let mut two = one;
    two.id = (one.id & 0xFFFF_0000) | 0x0019;
    let ids = vec![one.id, two.id];

    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicUpdateMultipleEnchantments(vec![one, two]),
    );
    settle(&mut app);
    assert_eq!(
        counts(&app),
        (1, 1),
        "two on the registry before the removal"
    );
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicRemoveMultipleEnchantments(ids.clone()),
    );
    settle(&mut app);

    let s = app.interaction().stats.clone();
    assert_eq!(s.multi_enchantment_removals, 1);
    assert_eq!(s.multi_enchantments_removed, 2);
    assert_eq!(
        s.enchantment_expiry_lines, 2,
        "one line per id, both spells in the table"
    );
    assert_eq!(counts(&app), (0, 0), "and the registry is empty again");

    let after_remove = chat(&mut app);
    let expired: Vec<&String> = after_remove[before..]
        .iter()
        .filter(|l| l.contains("has expired"))
        .collect();
    assert_eq!(
        expired.len(),
        2,
        "the two lines retail writes: {:?}",
        &after_remove[before..]
    );

    // Put them back and dispel instead. Same bytes on the wire, different opcode.
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicUpdateMultipleEnchantments(vec![one, two]),
    );
    settle(&mut app);
    assert_eq!(counts(&app), (1, 1));
    let before = chat(&mut app).len();

    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicDispelMultipleEnchantments(ids),
    );
    settle(&mut app);

    let s = app.interaction().stats.clone();
    assert_eq!(
        s.multi_enchantment_dispels, 1,
        "counted apart from the announcing arm"
    );
    assert_eq!(
        s.multi_enchantments_removed, 4,
        "two more ids out of the registry"
    );
    assert_eq!(
        s.enchantment_expiry_lines, 2,
        "and NOT one more line -- a dispel is silent"
    );
    assert_eq!(counts(&app), (0, 0));
    let after_dispel = chat(&mut app);
    assert!(
        after_dispel[before..]
            .iter()
            .all(|l| !l.contains("has expired")),
        "a dispel says nothing: {:?}",
        &after_dispel[before..]
    );

    assert!(!dropped::unreceived(
        Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS
    ));
    assert!(!dropped::unreceived(
        Opcode::MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS
    ));
}

/// **The single arms had the same hole and it is closed with the same call.**
///
/// Single removal reaches the shared removal path with `notify = true`, while single dispel reaches
/// it with `notify = false`.
///
/// **Falsified by** deleting the `notify_of_enchantment_removal` call from the `0x02C3` arm.
#[test]
fn the_single_removal_prints_the_expiry_line_and_the_single_dispel_does_not() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("single-remove");
    describe_player(&mut app, &mut peer);

    let one = recorded_enchantment();
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicUpdateEnchantment(one),
    );
    settle(&mut app);
    assert_eq!(counts(&app), (1, 0));

    let before = chat(&mut app).len();
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicRemoveEnchantment {
            layered_spell_id: one.id,
        },
    );
    settle(&mut app);
    assert_eq!(app.interaction().stats.enchantment_expiry_lines, 1);
    let lines = chat(&mut app);
    let line = lines[before..]
        .iter()
        .find(|l| l.contains("has expired"))
        .unwrap_or_else(|| panic!("no expiry line in {:?}", &lines[before..]));
    assert!(
        line.ends_with("has expired."),
        "the expiry suffix ends with a newline that the chat display trims: {line:?}"
    );

    // The same spell again, taken out silently.
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicUpdateEnchantment(one),
    );
    settle(&mut app);
    let before = chat(&mut app).len();
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicDispelEnchantment {
            layered_spell_id: one.id,
        },
    );
    settle(&mut app);
    assert_eq!(
        app.interaction().stats.enchantment_expiry_lines,
        1,
        "a dispel is silent because its removal path passes notify = false"
    );
    let lines = chat(&mut app);
    assert!(lines[before..].iter().all(|l| !l.contains("has expired")));
}

/// **A cooldown never announces itself.**
///
/// The removal-announcement path returns successfully with no line when
/// `(layered_id & 0xFFFF) >= 0x8000`. Cooldown display stores `cooldown_id + 0x8000` in the layered
/// id, so this is the cooldown gate and not an arbitrary range check.
///
/// **Falsified by** deleting the `>= 0x8000` early return: the line then goes out for an id no
/// spell table entry can name.
#[test]
fn a_cooldown_id_takes_the_early_return_with_no_line() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("cooldown");
    describe_player(&mut app, &mut peer);

    let before = chat(&mut app).len();
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicRemoveEnchantment {
            layered_spell_id: 0x0001_8005,
        },
    );
    settle(&mut app);
    assert_eq!(
        app.interaction().stats.enchantment_expiry_lines,
        0,
        "0x8005 & 0xFFFF >= 0x8000 -- a cooldown, not a spell"
    );
    let lines = chat(&mut app);
    assert!(lines[before..].iter().all(|l| !l.contains("has expired")));

    // **The positive control, in the same station.** An assertion that nothing happened passes
    // just as well when the arm has been deleted, which is how a station comes to measure its own
    // absence. `0x0005` is one below the gate and is in the table, so this half fails the moment
    // the arm goes away — and the two halves together are what make the *gate* the observable.
    peer.event(
        &mut app,
        &dereth_protocol::qualities::MagicRemoveEnchantment {
            layered_spell_id: 0x0001_0005,
        },
    );
    settle(&mut app);
    assert_eq!(
        app.interaction().stats.enchantment_expiry_lines,
        1,
        "0x0005 is below the gate: the same arm, the same absent registry entry, and a line"
    );
    let lines = chat(&mut app);
    assert!(
        lines[before..].iter().any(|l| l.contains("has expired")),
        "the removal announcement uses a spell-table lookup, not a registry lookup -- it prints \
         for an id the registry never held: {:?}",
        &lines[before..]
    );
}
