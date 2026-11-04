//! The periodic save: every character in the world kept again once its last save is the
//! operator's interval old (five minutes unless given another), on the world's clock; a save at an
//! event counts; a crowd due at once is kept a character a tick. Fixture: new players in the world,
//! their level changed in the world and read back from the store.

use yalain_store::{MemoryStore, Store as _};
use yalain_testkit::Harness;
use yalain_world::{examine, quals, saves, world, ServerConfig};

/// The level a character's kept state holds, if any.
fn kept_level(h: &mut Harness, id: u64) -> Option<i32> {
    let st = h.server.store.world_state(id).expect("the store reads")?;
    quals::int(&st.qualities, examine::LEVEL)
}

/// Change a character's level in the world (not yet kept).
fn set_level(h: &mut Harness, id: u64, level: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    quals::set_int(&mut p.qualities, examine::LEVEL, level);
}

/// Step a second at a time until `id`'s kept level is `level`, until the clock reads `until` at
/// the latest; the world's time at the tick that kept it (each step runs the world at the clock's
/// time, then moves the clock on).
fn tick_that_kept(h: &mut Harness, id: u64, level: i32, until: f64) -> Option<f64> {
    while h.now.0 < until {
        let tick = h.now.0;
        h.step_by(1.0);
        if kept_level(h, id) == Some(level) {
            return Some(tick);
        }
    }
    None
}

/// The world connection of a character in the world.
fn world_of(h: &Harness, id: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(c, _)| *c)
        .expect("in the world")
}

/// A character in the world is kept five minutes after it entered and every five minutes after,
/// whatever it does or does not do: a change made in the world is in the store within a second of
/// its save coming due, and not before.
/// Behaviour: server.operator.characters-saved-every-five-minutes
#[test]
fn a_character_in_the_world_is_kept_every_five_minutes_on_the_world_clock() {
    let mut h = Harness::new();
    let (_, elara) = h.player_in_world("one", "Elara");
    h.step();
    let entered = h
        .server
        .saves
        .last(elara)
        .expect("the world counts its wait")
        .0;
    for (level, due) in [(7, 300.0), (8, 600.0)] {
        set_level(&mut h, elara, level);
        let tick = tick_that_kept(&mut h, elara, level, entered + due + 5.0)
            .unwrap_or_else(|| panic!("kept by {due} s"));
        let at = tick - entered;
        assert!(
            (due..due + 1.0).contains(&at),
            "kept {at} s after entering, due at {due} s"
        );
    }
}

/// A save at an event (here the one a craft or a butchery makes as it ends) counts: the periodic
/// save comes five minutes after it, not five minutes after the character entered.
/// Behaviour: server.operator.characters-saved-every-five-minutes
#[test]
fn a_save_at_an_event_puts_the_next_periodic_save_five_minutes_after_it() {
    let mut h = Harness::new();
    let (_, elara) = h.player_in_world("one", "Elara");
    h.step();
    let entered = h.server.saves.last(elara).expect("counted").0;
    while h.now.0 - entered < 100.0 {
        h.step_by(1.0);
    }
    let conn = world_of(&h, elara);
    world::keep_player(&mut h.server, conn);
    let event = h.server.saves.last(elara).expect("counted").0;
    assert!(event > entered, "the event's save counted");
    set_level(&mut h, elara, 9);
    assert_eq!(
        tick_that_kept(&mut h, elara, 9, entered + 305.0),
        None,
        "not kept at five minutes after entering"
    );
    let tick = tick_that_kept(&mut h, elara, 9, event + 305.0).expect("kept after the event");
    let at = tick - event;
    assert!(
        (300.0..301.0).contains(&at),
        "kept {at} s after the event's save"
    );
}

/// Characters whose saves come due together (here after the console's `save-world` kept all
/// three at once, at the operator's interval of a minute) are kept one a tick of the world, the
/// rest at the ticks after, so no tick stalls on the store; and `save-world` keeps everyone in
/// the world at once.
/// Behaviour: server.operator.saves-spread-one-a-tick
#[test]
fn characters_due_together_are_kept_one_a_tick_at_the_operators_interval() {
    let cfg = ServerConfig {
        save_seconds: 60,
        ..ServerConfig::default()
    };
    let mut h = Harness::with(cfg, MemoryStore::default(), yalain_testkit::test_rules());
    let ids = [
        h.player_in_world("one", "Elara").1,
        h.player_in_world("two", "Tamsin").1,
        h.player_in_world("three", "Corwin").1,
    ];
    h.step();
    for id in ids {
        set_level(&mut h, id, 5);
    }
    let clock = h.clock();
    assert_eq!(saves::save_world(&mut h.server, clock), Ok(3));
    for id in ids {
        assert_eq!(kept_level(&mut h, id), Some(5), "save-world keeps everyone");
        set_level(&mut h, id, 7);
    }
    let saved = h.now.0;
    while h.now.0 - saved < 60.0 {
        h.step_by(0.5);
    }
    let kept = |h: &mut Harness| {
        ids.iter()
            .filter(|id| kept_level(h, **id) == Some(7))
            .count()
    };
    assert_eq!(kept(&mut h), 0, "none before the interval");
    let mut by_tick = Vec::new();
    for _ in 0..4 {
        h.step_by(0.5);
        by_tick.push(kept(&mut h));
    }
    assert_eq!(
        by_tick,
        vec![1, 2, 3, 3],
        "one a tick, from the interval on"
    );
}

/// The time a character spends in the world is kept with it and goes on from there at its next
/// entry, and it is too new for the network until that time reaches what the operator asks, in the
/// world or out of it. With nothing asked no one is.
/// Behaviour: server.network.too-new-is-time-in-the-world
#[test]
fn time_in_the_world_is_kept_and_a_character_ages_past_the_networks_limit() {
    use yalain_world::saves::{save_world, too_new, too_new_in_world};
    use yalain_world::state::PlayState;
    let mut h = Harness::new();
    let (_, me) = h.player_in_world("one", "Elara");
    assert!(!too_new(&mut h.server, me), "nothing asked");
    h.server.cfg.min_character_age_seconds = 600;
    assert!(too_new(&mut h.server, me));
    assert!(too_new_in_world(&h.server).contains("elara"));
    h.advance(100.0);
    let clock = h.clock();
    save_world(&mut h.server, clock).expect("kept");
    let kept = |h: &mut Harness| {
        let w = h
            .server
            .store
            .world_state(me)
            .expect("reads")
            .expect("kept");
        PlayState::from_bytes(&w.play)
            .expect("reads")
            .played_seconds
    };
    let first = kept(&mut h);
    assert!((99..=102).contains(&first), "{first}");
    h.advance(600.0);
    h.step();
    assert!(!too_new(&mut h.server, me), "old enough");
    assert!(too_new_in_world(&h.server).is_empty());
    let clock = h.clock();
    save_world(&mut h.server, clock).expect("kept");
    let second = kept(&mut h);
    assert!(
        (699..=702).contains(&second),
        "from the same entry: {second}"
    );
}
