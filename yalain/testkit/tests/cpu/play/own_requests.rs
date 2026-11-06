//! A player's own small requests: using another player (refused, the other told nothing), using an
//! item of the pack that has no use (a mote refused, a dye plant and a lodestone only answered),
//! setting and removing an alias (kept with the options, nothing answered), inscribing an item (a
//! weapon takes it, anything else is refused with a text), listing the skills it trained, and
//! cancelling a quest. Fixture: new players in the test world, made-up item templates and a made-up quest.

use std::collections::HashMap;

use auberean_protocol::queue;
use auberean_script::args::Arg;
use auberean_script::native_object::NativeObject;
use yalain_content::quests::QuestDef;
use yalain_content::Props;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, Harness};
use yalain_world::calls::{Node, Value};
use yalain_world::personal::{function, text};
use yalain_world::skills::{SkillDef, SkillTable};
use yalain_world::state::{QuestState, SkillState};

const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const MOTE: u32 = 0x4700_06C8;
const DYE_PLANT: u32 = 0x4700_0FFD;
const SWORD: u32 = 0x4700_0746;
const MOTE_PACKAGE: u32 = 0x2201;
const DYE_PACKAGE: u32 = 0x2202;
const SWORD_PACKAGE: u32 = 0x1067;

/// A use of an object, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// The use answers since `from`: (status, control flags, distance).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, f32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == USE_DONE)
        .filter_map(|(_, v)| match v.first()?.root()? {
            Node::Script { slots, .. } => {
                Some((slots[11].1, slots[17].1, f32::from_bits(slots[8].1)))
            }
            _ => None,
        })
        .collect()
}

fn string(s: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    b.native(
        u16::try_from(auberean_script::native_object::native_type::WP_STRING)
            .expect("a native type"),
        NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// A player in the test world with made-up items: its client, id and connection.
fn player(h: &mut Harness, name: &str) -> (usize, u64, ConnId) {
    let (b, me) = h.player_in_world(name, name);
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    (b, me, world)
}

fn world_of_items() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    for (t, pkg) in [
        (MOTE, MOTE_PACKAGE),
        (DYE_PLANT, DYE_PACKAGE),
        (SWORD, SWORD_PACKAGE),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, pkg, Props::default()));
    }
    h.server.held_answers.insert(MOTE_PACKAGE, (10_001, 316));
    h.server.held_answers.insert(DYE_PACKAGE, (0, 444));
    h.server.inscribable_packages.insert(SWORD_PACKAGE);
    h
}

fn give(h: &mut Harness, world: ConnId, template: u32) -> u64 {
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1);
    h.run(3);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .expect("given")
        .id
}

/// Using another player is refused at once, "cannot use", with the distance between them; the
/// other is told nothing.
/// Behaviour: server.social.using-another-player-is-refused
#[test]
fn using_another_player_is_refused_and_the_other_is_told_nothing() {
    let mut h = world_of_items();
    let (a, me, _) = player(&mut h, "one");
    let (b, you, _) = player(&mut h, "two");
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(true, queue::WEENIE, &server_call(USE, &[use_of(you, me)]));
    h.run(5);
    let got = answers(&h, a, fa);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].0, got[0].1), (10_001, 828));
    assert!(got[0].2 >= 0.0, "the distance between them");
    assert!(answers(&h, b, fb).is_empty(), "the other told nothing");
}

/// A mote used from the pack is refused "cannot use" with the use's own flags, and a dye plant is
/// answered with nothing done; neither is wielded.
/// Behaviour: server.inventory.items-with-no-use-only-answer
#[test]
fn a_mote_used_from_the_pack_is_refused_and_a_dye_plant_only_answered() {
    let mut h = world_of_items();
    let (a, me, world) = player(&mut h, "one");
    let mote = give(&mut h, world, MOTE);
    let plant = give(&mut h, world, DYE_PLANT);
    for (item, want) in [(mote, (10_001, 316)), (plant, (0, 444))] {
        let fa = h.bots[a].inbox.len();
        h.bots[a].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
        h.run(5);
        let got = answers(&h, a, fa);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0, got[0].1), want);
    }
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.id == mote || i.id == plant)
            .all(|i| i.location == 0),
        "neither wielded"
    );
}

/// An alias set is kept with the player's options, and removed again, with nothing answered.
/// Behaviour: server.social.aliases-kept-with-the-options
#[test]
fn an_alias_set_is_kept_with_the_options_and_removed_again_unanswered() {
    let mut h = world_of_items();
    let (a, _, world) = player(&mut h, "one");
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::SET_ALIAS,
        &[Arg::Int(1), string("/loadui"), string(";ll")],
        3,
    );
    assert!(h.calls(a, from).is_empty(), "nothing answered");
    assert_eq!(
        h.server.players[&world].options.alias_list(),
        [(";ll".to_string(), "/loadui".to_string())]
    );
    h.ask(
        a,
        function::SET_ALIAS,
        &[Arg::Int(0), string(""), string(";ll")],
        3,
    );
    assert!(h.server.players[&world].options.alias_list().is_empty());
}

/// A weapon takes an inscription and the player is told so; anything else is refused with a text.
/// Behaviour: server.inventory.inscribing-an-item
#[test]
fn a_weapon_takes_an_inscription_and_a_mote_is_refused() {
    let mut h = world_of_items();
    let (a, _, world) = player(&mut h, "one");
    let sword = give(&mut h, world, SWORD);
    let mote = give(&mut h, world, MOTE);
    let words = |s: &str| yalain_world::chat::text_arg(yalain_world::profile::literal(s));
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::INSCRIBE,
        &[words("Sharp"), Arg::LongInt(sword)],
        3,
    );
    assert_eq!(h.texts(a, from), [(text::SHOWN_AS, text::INSCRIBED.0)]);
    assert_eq!(
        h.server
            .things
            .get(sword)
            .and_then(|t| t.inscription.clone())
            .map(|(t, _)| t),
        Some("Sharp".to_string())
    );
    let from = h.bots[a].inbox.len();
    h.ask(a, function::INSCRIBE, &[words("No"), Arg::LongInt(mote)], 3);
    assert_eq!(
        h.texts(a, from),
        [(text::SHOWN_AS, text::CANNOT_INSCRIBE.0)]
    );
    assert!(h
        .server
        .things
        .get(mote)
        .and_then(|t| t.inscription.clone())
        .is_none());
}

/// Words past the longest an item takes are refused in the client's own words, and what it held stays;
/// the longest itself is taken whole.
/// Behaviour: server.inventory.an-inscription-past-its-longest-is-refused
#[test]
fn an_inscription_past_its_longest_is_refused_and_the_old_one_stays() {
    let mut h = world_of_items();
    let (a, _, world) = player(&mut h, "one");
    let sword = give(&mut h, world, SWORD);
    let words = |s: &str| yalain_world::chat::text_arg(yalain_world::profile::literal(s));
    let held = |h: &Harness| {
        h.server
            .things
            .get(sword)
            .and_then(|t| t.inscription.clone())
            .map(|(t, _)| t)
    };
    let longest = "x".repeat(yalain_world::personal::MAX_INSCRIPTION);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        function::INSCRIBE,
        &[words(&longest), Arg::LongInt(sword)],
        3,
    );
    assert_eq!(h.texts(a, from), [(text::SHOWN_AS, text::INSCRIBED.0)]);
    assert_eq!(held(&h).as_deref(), Some(longest.as_str()));
    let from = h.bots[a].inbox.len();
    let past = "y".repeat(yalain_world::personal::MAX_INSCRIPTION + 1);
    h.ask(
        a,
        function::INSCRIBE,
        &[words(&past), Arg::LongInt(sword)],
        3,
    );
    // "Your inscription is too long."
    assert_eq!(text::TOO_LONG, (0x72F8_3432, 0x2500_000B));
    assert_eq!(h.texts(a, from), [(text::SHOWN_AS, text::TOO_LONG.0)]);
    assert_eq!(held(&h).as_deref(), Some(longest.as_str()));
    // A template may name a longest of its own.
    let mut own = h.server.catalog.get(SWORD).expect("the sword").clone();
    own.props
        .set("Item_MaxInscriptionLength", yalain_content::Prop::Int(128));
    h.server.catalog.insert(own);
    let from = h.bots[a].inbox.len();
    let past = "z".repeat(129);
    h.ask(
        a,
        function::INSCRIBE,
        &[words(&past), Arg::LongInt(sword)],
        3,
    );
    assert_eq!(h.texts(a, from), [(text::SHOWN_AS, text::TOO_LONG.0)]);
    let from = h.bots[a].inbox.len();
    let fits = "z".repeat(128);
    h.ask(
        a,
        function::INSCRIBE,
        &[words(&fits), Arg::LongInt(sword)],
        3,
    );
    assert_eq!(h.texts(a, from), [(text::SHOWN_AS, text::INSCRIBED.0)]);
}

/// An item the player does not hold is refused with the same text, and takes no words: a client
/// naming another player's item, or one lying in the world.
/// Behaviour: server.inventory.inscribing-only-what-the-player-holds
#[test]
fn an_item_the_player_does_not_hold_takes_no_inscription() {
    let mut h = world_of_items();
    let (_, _, owner) = player(&mut h, "one");
    let (b, _, _) = player(&mut h, "two");
    let sword = give(&mut h, owner, SWORD);
    let lying = yalain_world::inventory::make_item(&mut h.server, SWORD, 1, 0).expect("made");
    h.server.things.get_mut(lying).expect("made").at = Some(h.server.players[&owner].position);
    let words = |s: &str| yalain_world::chat::text_arg(yalain_world::profile::literal(s));
    for item in [sword, lying] {
        let from = h.bots[b].inbox.len();
        h.ask(
            b,
            function::INSCRIBE,
            &[words("Mine now"), Arg::LongInt(item)],
            3,
        );
        assert_eq!(
            h.texts(b, from),
            [(text::SHOWN_AS, text::CANNOT_INSCRIBE.0)]
        );
        assert!(h
            .server
            .things
            .get(item)
            .and_then(|t| t.inscription.clone())
            .is_none());
    }
}

/// An inscription stays with its item across a logout, with who wrote it.
/// Behaviour: server.inventory.inscription-kept-across-sessions
#[test]
fn an_inscription_is_kept_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::{ClientMessage, ServerMessage};
    let mut h = world_of_items();
    let (a, me, world) = player(&mut h, "one");
    let sword = give(&mut h, world, SWORD);
    let words = yalain_world::chat::text_arg(yalain_world::profile::literal("Sharp"));
    h.ask(a, function::INSCRIBE, &[words, Arg::LongInt(sword)], 3);
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.enter_world(a, "one", me);
    let (said, by) = h
        .server
        .things
        .get(sword)
        .and_then(|t| t.inscription.clone())
        .expect("still inscribed");
    assert_eq!(said, "Sharp");
    assert!(
        by.starts_with("One ["),
        "its writer's name in the world: {by}"
    );
}

/// Asked, the server lists one line for each skill the player trained, none for those it was given.
/// Behaviour: server.skills.list-trained-skills
#[test]
fn the_trained_skills_list_has_a_line_for_each_trained_skill_only() {
    let mut h = world_of_items();
    let mut skills = HashMap::new();
    for id in [15, 16] {
        skills.insert(
            id,
            SkillDef {
                id,
                name: (0xA860_7CEC, 0x2500_085B),
                cost: 2,
                ..SkillDef::default()
            },
        );
    }
    h.server.skill_table = SkillTable {
        skills,
        tables: HashMap::new(),
    };
    let (a, _, world) = player(&mut h, "one");
    let p = h.server.players.get_mut(&world).expect("in");
    for (skill, granted) in [(15, 0), (16, 1)] {
        p.play.skills.push(SkillState {
            skill,
            granted,
            mask: 1,
            cost: 2,
            children: 0,
            dependents: 0,
            spent: 40,
        });
    }
    let starting = h
        .server
        .start
        .skills
        .get(&p.race)
        .cloned()
        .unwrap_or_default();
    let trained = yalain_world::skills::held(&starting, &p.play.skills)
        .iter()
        .filter(|s| s.granted == 0)
        .count();
    let from = h.bots[a].inbox.len();
    h.ask(a, function::LIST_SKILLS, &[], 3);
    let lists: Vec<Vec<Value>> = h
        .calls(a, from)
        .into_iter()
        .filter(|(f, _)| *f == function::LIST_SKILLS_DONE)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(lists.len(), 1, "one answer");
    let Some(Value::Graph(g)) = lists[0].first() else {
        panic!("a list");
    };
    let lines = g
        .iter()
        .filter(|(_, n)| matches!(n, Node::TextInfo(t) if t.string_id == text::SKILL_LINE.0))
        .count();
    assert_eq!(lines, trained, "a line for each trained skill");
}

/// A quest under way, cancelled: one quest update of the cancel kind, and the quest no longer under
/// way.
/// Behaviour: server.quests.cancel-a-quest
#[test]
fn a_quest_cancelled_is_updated_as_cancelled_and_no_longer_under_way() {
    use yalain_world::quests::{self, status, update};
    let mut h = world_of_items();
    let quest = 0x8000_02E5;
    h.server.quests.quests.push(QuestDef {
        id: quest,
        name: (1, 0x2500_14B4),
        description: (2, 0x2500_14B4),
        icon: 0x4100_034B,
        play_fx: false,
        journals: vec![(3, 0x2500_14B4)],
        challenge: 1,
        duration: 0.0,
        timed_phase: 0,
        retry: 86_400.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    });
    let (a, _, world) = player(&mut h, "one");
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(QuestState {
            quest,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    let from = h.bots[a].inbox.len();
    h.ask(a, quests::CANCEL_QUEST, &[Arg::Int(quest)], 3);
    let updates: Vec<Vec<Value>> = h
        .calls(a, from)
        .into_iter()
        .filter(|(f, _)| *f == quests::UPDATE_QUEST)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(updates.len(), 1);
    assert!(
        updates[0].contains(&Value::Int(update::CANCEL)),
        "the cancel kind"
    );
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, quest),
        0
    );
    assert!(
        h.server.players[&world]
            .play
            .quests
            .iter()
            .any(|q| q.quest == quest && q.status == status::CANCELLED),
        "kept as cancelled"
    );
}

/// A lodestone used from the pack is only answered, made with the flags of a use at once, and is
/// neither wielded nor spent.
/// Behaviour: server.inventory.lodestones-only-answer
#[test]
fn a_lodestone_used_from_the_pack_is_only_answered() {
    const LODESTONE: u32 = 0x4700_0C1D;
    const LODESTONE_PACKAGE: u32 = 0x2203;
    let mut h = world_of_items();
    let (_, status, flags) = yalain_world::places::HELD_ANSWERED
        .iter()
        .find(|(base, _, _)| *base == "Lodestone")
        .copied()
        .expect("lodestones are answered");
    h.server.catalog.insert(yalain_testkit::test_template(
        LODESTONE,
        LODESTONE_PACKAGE,
        Props::default(),
    ));
    h.server
        .held_answers
        .insert(LODESTONE_PACKAGE, (status, flags));
    let (a, me, world) = player(&mut h, "one");
    let stone = give(&mut h, world, LODESTONE);
    let fa = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &server_call(USE, &[use_of(stone, me)]));
    h.run(5);
    let got = answers(&h, a, fa);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].0, got[0].1), (0, 444));
    let held: Vec<u32> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.id == stone)
        .map(|i| i.location)
        .collect();
    assert_eq!(held, [0], "kept in the pack, not wielded");
}

/// A character's aliases are kept up to a bound: past it an alias is not taken and nothing already
/// kept changes; one may still be replaced by one that fits, or removed.
/// Behaviour: server.social.options-kept-within-bounds
#[test]
fn an_alias_past_what_a_character_keeps_changes_nothing() {
    use yalain_world::options::{Options, MOST_ALIAS_BYTES, MOST_ALIAS_UNITS};
    let mut o = Options::default();
    let long = "x".repeat(MOST_ALIAS_UNITS);
    assert!(o.set_alias("mine", "/say hello", true));
    assert!(o.set_alias("long", &long, true));
    assert!(!o.set_alias("toolong", &format!("{long}x"), true));
    assert!(!o.set_alias(&format!("{long}x"), "t", true));
    let two = o.alias_list();
    assert_eq!(two.len(), 2);
    let mut n = 0;
    while n < 1000 && o.set_alias(&format!("a{n}"), &long, true) {
        n += 1;
    }
    assert!(n > 60, "two thousand bytes each: {n}");
    let kept = o.aliases.as_ref().map_or(0, Vec::len);
    assert!(kept <= MOST_ALIAS_BYTES, "{kept}");
    assert!(kept > MOST_ALIAS_BYTES - 3 * MOST_ALIAS_UNITS, "{kept}");
    let full = o.alias_list();
    assert!(!o.set_alias("another", &long, true));
    assert_eq!(o.alias_list(), full, "nothing changed");
    assert!(o.set_alias("mine", "/say bye", true), "replaced");
    assert!(o.set_alias("a0", "", false), "removed");
    assert!(o
        .alias_list()
        .contains(&("mine".to_owned(), "/say bye".to_owned())));
    assert!(o.set_alias("another", &long, true), "room again");
}

/// Options kept before there were bounds: as a character enters, every alias of a size a player uses
/// stays, in order, wherever it stood among the rest; what is left is within the bounds, and is what
/// is kept and told from then on. A character within the bounds keeps its options to the byte.
/// Behaviour: server.social.options-kept-within-bounds
#[test]
fn a_character_with_options_past_the_bounds_enters_with_the_aliases_a_player_uses() {
    use auberean_protocol::login::CharacterRef;
    use auberean_script::wire::Out;
    use yalain_world::options::{LeftOut, Options, MOST_ALIAS_BYTES, MOST_PART_BYTES};
    let flood = "x".repeat(30_000);
    let mut names: Vec<(String, String)> = vec![("first".into(), "/say one".into())];
    for i in 0..4 {
        names.push((format!("big{i}"), flood.clone()));
    }
    names.push(("second".into(), "/say two".into()));
    let mut w = Out::new();
    w.u32(u32::try_from(names.len()).expect("few"));
    for (a, t) in &names {
        w.wide(&a.encode_utf16().collect::<Vec<_>>());
        w.wide(&t.encode_utf16().collect::<Vec<_>>());
    }
    let mut big_shortcut = Out::new();
    big_shortcut.u32(0x4000_0003);
    big_shortcut.wide(&vec![0x78; MOST_PART_BYTES]);
    let mut big_window = Vec::new();
    big_window.extend(1u32.to_le_bytes());
    big_window.extend(0x1234u32.to_le_bytes());
    big_window.extend(200u32.to_le_bytes());
    big_window.extend(vec![0u8; 200 * 24]);
    let row = Options {
        aliases: Some(w.buf),
        shortcuts: vec![big_shortcut.buf, 0u32.to_le_bytes().to_vec()],
        window_places: Some(big_window),
        ..Options::default()
    };
    assert_eq!(row.alias_list().len(), 6, "the row as it stood reads");
    let mut trimmed = row.clone();
    assert_eq!(
        trimmed.keep_within_bounds(),
        LeftOut {
            aliases: 4,
            parts: 2
        }
    );
    let mut same = trimmed.clone();
    assert_eq!(same.keep_within_bounds(), LeftOut::default());
    assert_eq!(same, trimmed, "within the bounds it is left to the byte");

    let mut h = world_of_items();
    let (a, me, world) = player(&mut h, "one");
    h.server.players.get_mut(&world).expect("in").options = row;
    h.bots[a].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(CharacterRef { character: me }),
    );
    for _ in 0..25 {
        h.step_by(1.0);
    }
    assert!(!h.server.players.contains_key(&world), "gone");
    let kept = yalain_world::saves::state_of(&mut h.server, me)
        .expect("reads")
        .expect("kept");
    assert!(kept.options.len() > MOST_ALIAS_BYTES, "kept as it stood");
    let back = h.at_character_list("one");
    h.enter_world(back, "one", me);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world again");
    assert_eq!(
        p.options.alias_list(),
        vec![
            ("first".to_owned(), "/say one".to_owned()),
            ("second".to_owned(), "/say two".to_owned())
        ]
    );
    assert_eq!(p.options.shortcuts[0], 0u32.to_le_bytes().to_vec());
    assert_eq!(p.options.window_places, None);
    let w = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    yalain_world::world::keep_player(&mut h.server, w);
    h.run(2);
    let kept = yalain_world::saves::state_of(&mut h.server, me)
        .expect("reads")
        .expect("kept");
    assert!(
        kept.options.len() < MOST_ALIAS_BYTES,
        "kept within them now"
    );
}

/// A call that sets a part of the options, past what a client's own use comes to, is not kept.
/// Behaviour: server.social.options-kept-within-bounds
#[test]
fn a_shortcut_past_what_a_character_keeps_is_not_kept() {
    use yalain_world::options::MOST_PART_BYTES;
    let mut h = world_of_items();
    let (b, _, world) = player(&mut h, "one");
    let text = |units: usize| -> Vec<u8> {
        let mut bytes = 0x4000_0003_u32.to_le_bytes().to_vec();
        bytes.extend(u16::try_from(units).expect("short").to_le_bytes());
        bytes.extend(std::iter::repeat_n(0x78_u8, units * 2));
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        bytes
    };
    let set = |h: &mut Harness, b: usize, bytes: Vec<u8>| {
        let packed = auberean_script::native_object::NativeObject::Packed { ty: 172, bytes };
        let mut g = yalain_world::graph::Builder::new();
        g.native(172, packed);
        h.bots[b].send(
            true,
            auberean_protocol::queue::WEENIE,
            &yalain_testkit::play::server_call(
                yalain_world::session::request::SHORTCUT,
                &[Arg::Object(g.graph()), Arg::Int(5)],
            ),
        );
        h.run(3);
    };
    let small = text(100);
    set(&mut h, b, small.clone());
    assert_eq!(h.server.players[&world].options.shortcuts[5], small);
    set(&mut h, b, text(MOST_PART_BYTES));
    assert_eq!(h.server.players[&world].options.shortcuts[5], small);
}
