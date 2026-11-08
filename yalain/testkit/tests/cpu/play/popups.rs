//! Tips: the world's `[[popup]]` rows show an effect's description once per character, at its first
//! entry, its first level gained and the first glyph that comes into its hands; a character kept
//! does not see one again, and a world with no rows shows none. Fixture: made-up effects whose
//! descriptions are made-up strings, a made-up glyph package and a made-up item that is no glyph.

use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::popups::Moment;
use yalain_content::Props;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::popups::DISPLAY_MESSAGE;
use yalain_world::progress::{award, q, Award};

const WELCOME: u32 = 0x6F00_7001;
const LEVEL: u32 = 0x6F00_7002;
const GLYPH: u32 = 0x6F00_7003;
const WELCOME_TEXT: (u32, u32) = (0x6C45_9364, 0x2500_7001);
const LEVEL_TEXT: (u32, u32) = (0x6C45_9364, 0x2500_7002);
const GLYPH_TEXT: (u32, u32) = (0x6C45_9364, 0x2500_7003);

/// A glyph, its package one of the glyphs', and an item that is none.
const A_GLYPH: u32 = 0x4700_7101;
const GLYPH_PACKAGE: u32 = 0x2824;
const NOT_A_GLYPH: u32 = 0x4700_7102;

/// A server whose world gives every moment its tip (or none), with ten levels a thousand apart.
fn world(rows: bool) -> Harness {
    let mut h = Harness::new();
    if rows {
        for (m, e, text) in [
            (Moment::FirstEntry, WELCOME, WELCOME_TEXT),
            (Moment::LevelUp, LEVEL, LEVEL_TEXT),
            (Moment::Glyph, GLYPH, GLYPH_TEXT),
        ] {
            h.server.popups.by_moment.insert(m, e);
            h.server.effect_table.descriptions.insert(e, text);
        }
    }
    h.server.glyph_packages.insert(GLYPH_PACKAGE);
    for (t, package) in [(A_GLYPH, GLYPH_PACKAGE), (NOT_A_GLYPH, 0x1067)] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, package, Props::default()));
    }
    h.server.levels = yalain_world::levels::LevelTable {
        levels: (0..10)
            .map(|n| yalain_world::levels::Level {
                xp: n * 1_000,
                ..Default::default()
            })
            .collect(),
    };
    h
}

/// The tips a client was shown since `from`: each one's string, table and whether it is on top.
fn tips(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == DISPLAY_MESSAGE => {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                let topmost = match v.first()? {
                    Value::Int(t) => *t,
                    _ => return None,
                };
                let text = v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                    _ => None,
                })?;
                Some((text.0, text.1, topmost))
            }
            _ => None,
        })
        .collect()
}

fn shown(text: (u32, u32)) -> Vec<(u32, u32, u32)> {
    vec![(text.0, text.1, 0)]
}

/// A new character in the world: its client, its id, and where its first entry's messages start.
fn entered(h: &mut Harness) -> (usize, u64, usize) {
    let b = h.at_character_list("one");
    let me = h.create_character(b, "one", "Newcomer");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    h.run(10);
    (b, me, from)
}

/// The character leaves for the character list and enters again: where the new entry's messages
/// start.
fn again(h: &mut Harness, b: usize, me: u64) -> usize {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    h.run(10);
    from
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

fn take(h: &mut Harness, me: u64, template: u32) {
    let world = world_of(h, me);
    let clock = h.clock();
    yalain_world::quests::took(&mut h.server, &mut h.net, world, template, clock);
    h.run(2);
}

fn earn(h: &mut Harness, me: u64, xp: u64) -> i32 {
    let world = world_of(h, me);
    award(&mut h.server, &mut h.net, world, xp, Award::Step);
    h.run(2);
    let p = h.server.players.get(&world).expect("in the world");
    yalain_world::quals::int(&p.qualities, q::LEVEL).unwrap_or(0)
}

/// A new character's first entry shows the welcome tip, not on top, and a later entry shows none,
/// even one whose marks were lost.
/// Behaviour: server.tips.first-entry-once
#[test]
fn a_new_characters_first_entry_shows_its_tip_once() {
    let mut h = world(true);
    let (b, me, from) = entered(&mut h);
    assert_eq!(tips(&h, b, from), shown(WELCOME_TEXT));
    let from = again(&mut h, b, me);
    assert_eq!(tips(&h, b, from), [], "not at the next entry");
    // Only a first entry shows it, whatever the character's marks say.
    let world = world_of(&h, me);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .popups_shown = 0;
    let from = again(&mut h, b, me);
    assert_eq!(tips(&h, b, from), [], "nor at a later entry with no marks");
}

/// A character's first level gained shows the level tip; the next level shows none, nor does one
/// gained after the character entered again.
/// Behaviour: server.tips.first-level-once
#[test]
fn a_characters_first_level_shows_its_tip_once_across_entries() {
    let mut h = world(true);
    let (b, me, _) = entered(&mut h);
    let from = h.bots[b].inbox.len();
    assert_eq!(earn(&mut h, me, 1_000), 2);
    assert_eq!(tips(&h, b, from), shown(LEVEL_TEXT));
    let from = h.bots[b].inbox.len();
    assert_eq!(earn(&mut h, me, 1_000), 3);
    assert_eq!(tips(&h, b, from), [], "not at the next level");
    again(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    assert_eq!(earn(&mut h, me, 1_000), 4);
    assert_eq!(tips(&h, b, from), [], "nor after entering again");
}

/// A character kept before its first level was gained, entering again, is shown the level tip when
/// it gains it: the tip waits for the moment, and the mark is what a character keeps.
/// Behaviour: server.tips.first-level-once
#[test]
fn a_first_level_gained_after_entering_again_shows_its_tip() {
    let mut h = world(true);
    let (b, me, _) = entered(&mut h);
    again(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    assert_eq!(earn(&mut h, me, 1_000), 2);
    assert_eq!(tips(&h, b, from), shown(LEVEL_TEXT));
}

/// The first glyph that comes into a character's hands shows the glyph tip; an item that is no
/// glyph shows none, nor does a second glyph, before or after the character entered again.
/// Behaviour: server.tips.first-glyph-once
#[test]
fn a_characters_first_glyph_shows_its_tip_once_across_entries() {
    let mut h = world(true);
    let (b, me, _) = entered(&mut h);
    let from = h.bots[b].inbox.len();
    take(&mut h, me, NOT_A_GLYPH);
    assert_eq!(tips(&h, b, from), [], "no glyph");
    take(&mut h, me, A_GLYPH);
    assert_eq!(tips(&h, b, from), shown(GLYPH_TEXT));
    let from = h.bots[b].inbox.len();
    take(&mut h, me, A_GLYPH);
    assert_eq!(tips(&h, b, from), [], "not for the second");
    again(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    take(&mut h, me, A_GLYPH);
    assert_eq!(tips(&h, b, from), [], "nor after entering again");
}

/// A world with no tip rows shows none: not at a first entry, a first level or a first glyph; and
/// tips the world gains later are not shown for moments a character passed before.
/// Behaviour: server.tips.none-without-rows
#[test]
fn a_world_with_no_tip_rows_shows_none_and_a_passed_moment_none_later() {
    let mut h = world(false);
    let (b, me, from) = entered(&mut h);
    assert_eq!(earn(&mut h, me, 1_000), 2);
    take(&mut h, me, A_GLYPH);
    assert_eq!(tips(&h, b, from), []);
    let with = world(true);
    h.server.popups = with.server.popups.clone();
    h.server.effect_table.descriptions = with.server.effect_table.descriptions.clone();
    let from = h.bots[b].inbox.len();
    take(&mut h, me, A_GLYPH);
    assert_eq!(earn(&mut h, me, 1_000), 3);
    again(&mut h, b, me);
    assert_eq!(tips(&h, b, from), [], "passed before the tips came");
}

/// A character past its first level, its marks lost (one the operator made), is shown no level tip
/// at its next level: only the first level gained shows it.
/// Behaviour: server.tips.first-level-once
#[test]
fn a_character_past_its_first_level_is_shown_no_level_tip() {
    let mut h = world(true);
    let (b, me, _) = entered(&mut h);
    assert_eq!(earn(&mut h, me, 2_000), 3);
    let world = world_of(&h, me);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .popups_shown = 0;
    let from = h.bots[b].inbox.len();
    assert_eq!(earn(&mut h, me, 1_000), 4);
    assert_eq!(tips(&h, b, from), []);
}
