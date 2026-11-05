//! Item ids across restarts: an id a kept item holds is never given to anything new, whatever range
//! a run was given, and an item that comes back from the store never takes another's place.

use yalain_net::ConnId;
use yalain_store::{MemoryStore, Store};
use yalain_testkit::{test_rules, Harness, TEST_ITEM};
use yalain_world::state::PlayState;
use yalain_world::{inventory, ServerConfig};

fn run(store: MemoryStore, seed: u64) -> Harness {
    let cfg = ServerConfig {
        seed,
        ..ServerConfig::default()
    };
    let mut h = Harness::with(cfg, store, test_rules());
    h.server.catalog = yalain_testkit::test_catalog();
    h
}

fn world_of(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

fn held(h: &mut Harness, id: u64) -> Vec<u64> {
    let st = h
        .server
        .store
        .world_state(id)
        .expect("reads")
        .expect("kept");
    let play = PlayState::from_bytes(&st.play).expect("reads");
    play.items.iter().map(|x| x.id).collect()
}

/// A run high in the id ranges gives two players items; the next run, lower, lets one of them back
/// in and makes new things: none takes an id the other, away, still holds.
/// Behaviour: server.inventory.item-ids-never-given-twice
#[test]
fn a_restart_never_gives_a_new_thing_an_id_a_character_away_still_holds() {
    let mut h = run(MemoryStore::default(), 0x00FF_FFF0);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Torvald");
    let _ = (a, b);
    let (wa, wb) = (world_of(&h, ida), world_of(&h, idb));
    inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given");
    for _ in 0..50 {
        inventory::give(&mut h.server, &mut h.net, wb, TEST_ITEM, 1).expect("given");
    }
    h.run(5);
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    let away = held(&mut h, idb);
    let store = std::mem::take(&mut h.server.store);

    let mut h = run(store, 1);
    let (_, ida2) = {
        let b = h.at_character_list("one");
        h.enter_world(b, "one", ida);
        (b, ida)
    };
    let wa = world_of(&h, ida2);
    let made: Vec<u64> = (0..200)
        .map(|_| inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given"))
        .collect();
    let taken: Vec<&u64> = made.iter().filter(|x| away.contains(x)).collect();
    assert!(
        taken.is_empty(),
        "{} of {} new ids are ids a character away holds",
        taken.len(),
        made.len()
    );
}

/// A character kept by a build before these blocks, holding an id high in the old ranges, comes back:
/// new ids stay in the blocks, above every old range, and are not raised to follow its item.
/// Behaviour: server.inventory.item-ids-never-given-twice
#[test]
fn an_item_from_the_old_id_ranges_does_not_move_new_ids() {
    const OLD: u64 = 0x8130_FFF0_1000_0005;
    let mut h = run(MemoryStore::default(), 1);
    let (_, ida) = h.player_in_world("one", "Elara");
    let wa = world_of(&h, ida);
    inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given");
    h.run(5);
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    let mut st = h
        .server
        .store
        .world_state(ida)
        .expect("reads")
        .expect("kept");
    let mut play = PlayState::from_bytes(&st.play).expect("reads");
    play.items
        .iter_mut()
        .find(|x| x.template == TEST_ITEM)
        .expect("held")
        .id = OLD;
    st.play = play.to_bytes();
    h.server.store.set_world_state(ida, &st).expect("kept");
    let store = std::mem::take(&mut h.server.store);
    let mut h = run(store, 2);
    let b = h.at_character_list("one");
    h.enter_world(b, "one", ida);
    let wa = world_of(&h, ida);
    assert!(h.server.players[&wa].play.items.iter().any(|x| x.id == OLD));
    let made = inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given");
    assert!(made >= yalain_world::things::THING_IDS, "{made:#x}");
    assert_eq!(
        made & 0xFFFF_0000_0000_0000,
        yalain_world::things::THING_IDS
    );
}

/// New things of a run, past a block's first half: the next block is set aside in the store before
/// the first runs out, so a run started after a crash takes up above everything given.
/// Behaviour: server.inventory.item-ids-never-given-twice
#[test]
fn the_next_block_of_ids_is_kept_before_the_first_runs_out() {
    use yalain_world::things::ID_BLOCK;
    let mut h = run(MemoryStore::default(), 1);
    let mut top = 0;
    for _ in 0..=ID_BLOCK / 2 {
        top = h.server.things.new_id();
    }
    h.run(2);
    assert!(!h.server.things.low(), "another block set aside");
    // It crashes: nothing else of this run is kept, and a new run starts.
    let store = std::mem::take(&mut h.server.store);
    let mut h = run(store, 1);
    let next = h.server.things.new_id();
    assert!(
        next > top + ID_BLOCK / 2,
        "{next:#x} is above all the last run gave or could give ({top:#x})"
    );
}

/// Two characters kept holding one id (saves made before ids were set aside) both enter: each has
/// its own thing, kept under its own id, and one leaving takes nothing from the other.
/// Behaviour: server.inventory.a-thing-never-takes-anothers-place
#[test]
fn two_characters_kept_with_one_id_each_keep_their_own_item() {
    let mut h = run(MemoryStore::default(), 1);
    let (_, ida) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Torvald");
    let (wa, wb) = (world_of(&h, ida), world_of(&h, idb));
    let item = inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given");
    inventory::give(&mut h.server, &mut h.net, wb, TEST_ITEM, 1).expect("given");
    h.run(5);
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    // The second holds the first's id.
    let mut st = h
        .server
        .store
        .world_state(idb)
        .expect("reads")
        .expect("kept");
    let mut play = PlayState::from_bytes(&st.play).expect("reads");
    let last = play
        .items
        .iter_mut()
        .find(|x| x.template == TEST_ITEM)
        .expect("held");
    last.id = item;
    st.play = play.to_bytes();
    h.server.store.set_world_state(idb, &st).expect("kept");
    let store = std::mem::take(&mut h.server.store);

    let mut h = run(store, 2);
    for (account, id) in [("one", ida), ("two", idb)] {
        let b = h.at_character_list(account);
        h.enter_world(b, account, id);
    }
    let (wa, wb) = (world_of(&h, ida), world_of(&h, idb));
    let of = |h: &Harness, w: ConnId| -> Vec<u64> {
        let items = &h.server.players[&w].play.items;
        items
            .iter()
            .filter(|x| x.template == TEST_ITEM)
            .map(|x| x.id)
            .collect()
    };
    let (a, b) = (of(&h, wa), of(&h, wb));
    assert_eq!(a, vec![item], "the first in keeps its id");
    assert_eq!(b.len(), 1);
    assert_ne!(b[0], item, "the second's item has an id of its own");
    assert!(h.server.things.get(b[0]).is_some());
    yalain_world::world::leave(&mut h.server, &mut h.net, wa);
    assert!(h.server.things.get(b[0]).is_some(), "still in the world");
    h.run(5);
    assert!(held(&mut h, idb).contains(&b[0]), "kept under its new id");
}

/// A thing given the id of one the world has does not take its place.
/// Behaviour: server.inventory.a-thing-never-takes-anothers-place
#[test]
fn a_thing_with_an_id_the_world_has_is_left_out() {
    let mut h = run(MemoryStore::default(), 1);
    let first = inventory::make_item(&mut h.server, TEST_ITEM, 7, 0).expect("made");
    let template = h
        .server
        .catalog
        .get(TEST_ITEM)
        .cloned()
        .expect("in the catalog");
    let other = yalain_world::things::Thing::of(first, &template, yalain_world::things::Kind::Item);
    assert!(!h.server.things.insert(other));
    assert_eq!(h.server.things.get(first).expect("there").quantity, 7);
}

/// A character's first pass back in the world saves it (its craft day is over): what its items carry
/// of their own is in that save, though they are not yet back in the world when it is taken.
/// Behaviour: server.inventory.a-save-at-entry-keeps-what-items-carry
#[test]
fn a_save_made_as_a_character_enters_keeps_what_its_items_carry() {
    let mut h = run(MemoryStore::default(), 1);
    let (_, id) = h.player_in_world("one", "Elara");
    let w = world_of(&h, id);
    let item = inventory::give_made(&mut h.server, &mut h.net, w, TEST_ITEM, 1).expect("given");
    h.run(5);
    yalain_world::shutdown::save_everyone(&mut h.server, &mut h.net);
    let carried = |h: &mut Harness| {
        let st = h
            .server
            .store
            .world_state(id)
            .expect("reads")
            .expect("kept");
        let play = PlayState::from_bytes(&st.play).expect("reads");
        play.item_states
            .iter()
            .find(|k| k.id == item)
            .and_then(|k| k.crafter.clone())
    };
    assert!(carried(&mut h).is_some(), "kept with its maker");
    // Away past the end of its craft day, then back: the entry's pass saves it.
    let mut st = h
        .server
        .store
        .world_state(id)
        .expect("reads")
        .expect("kept");
    let mut play = PlayState::from_bytes(&st.play).expect("reads");
    play.craft_reset = 1.0;
    st.play = play.to_bytes();
    h.server.store.set_world_state(id, &st).expect("kept");
    let store = std::mem::take(&mut h.server.store);
    let mut h = run(store, 2);
    let w = yalain_net::ConnId(9_999);
    let clock = h.clock();
    yalain_world::world::enter(&mut h.server, &mut h.net, w, w, id, clock).expect("enters");
    // What the store has after the pass that entered it, had the server died here.
    yalain_world::saves::write_out(&mut h.server).expect("kept");
    assert!(carried(&mut h).is_some(), "still kept with its maker");
}
