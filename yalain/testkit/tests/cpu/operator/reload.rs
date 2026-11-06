//! The world database taken again while the server runs: what people say changes at once for a
//! player already in the world; a template the new database dropped stays known. Fixture: a person
//! beside a new player, the person's lines authored for the test.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::talk::{How, Line, Role, TalkTable, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, test_template, Harness};
use yalain_world::calls::{self, Node};
use yalain_world::graph::{Builder, Slot};
use yalain_world::reload::{table, Reloaded, WorldTables};

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;

const PERSON: u64 = 0x8130_0000_0000_0301;
const TEMPLATE: u32 = 0x4700_31E6;
const TABLE: u32 = 0x2500_2A34;
const BESIDE: [f32; 3] = [61.0, 241.0, -44.6];

fn person(origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(PERSON),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, TABLE)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// The person's line for a use.
fn talk(string: u32) -> TalkTable {
    TalkTable {
        lines: vec![Line {
            who: TEMPLATE,
            object: None,
            words: Words::Ref(string, TABLE),
            role: Some(Role::RefuseUse),
            how: How::Tell,
            when: When::Use,
            quest: None,
            update: None,
            phase: None,
            item: None,
            order: 0,
            measured: true,
            placed: "authored for a test".into(),
        }],
    }
}

/// The world as the server holds it, as a reload would hand it over.
pub(super) fn tables_of(h: &Harness) -> WorldTables {
    let s = &h.server;
    WorldTables {
        content: s.content.clone(),
        catalog: s.catalog.clone(),
        spawns: s.spawns.clone(),
        loot: s.loot.clone(),
        stores: s.stores.clone(),
        quests: s.quests.clone(),
        talk: s.talk.clone(),
        story: s.story.clone(),
        locks: s.locks.clone(),
        places_named: s.places_named.clone(),
        places_by_name: s.places_by_name.clone(),
        effects: s.effect_table.clone(),
        crafting: s.crafting.clone(),
        weapons: s.creature_weapons.clone(),
        weapon_rules: s.creature_weapon_rules.clone(),
        death_spawns: s.death_spawns.clone(),
        names: None,
        kinds: None,
        ground: None,
        files: None,
        patch: None,
    }
}

/// Reload the world database the server is handed.
pub(super) fn reload(h: &mut Harness, t: WorldTables) -> Reloaded {
    let clock = h.clock();
    h.server.reload(&mut h.net, clock, t)
}

/// A new player beside the person, its scene drawn.
fn world() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![person(BESIDE)],
        ..ContentPack::default()
    };
    h.server.talk = talk(0xA1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (h, b, me)
}

fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
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

/// Use the person; the string ids of the tells the player hears.
fn tells_on_use(h: &mut Harness, b: usize, me: u64) -> Vec<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(PERSON, me)]),
    );
    h.run(10);
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == HEAR_TELL => c.args.as_ref(),
            _ => None,
        })
        .filter_map(|a| calls::values(&a.0).ok())
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

/// A reload of a database whose person says something new: the player in the world hears the new
/// line on the next use, and the report names the lines as taken.
/// Behaviour: server.operator.reload-takes-what-changes-live
#[test]
fn a_reload_changes_what_people_say_at_once() {
    let (mut h, b, me) = world();
    assert_eq!(tells_on_use(&mut h, b, me), [0xA1], "the line before");

    let mut t = tables_of(&h);
    t.talk = talk(0xB2);
    let r = reload(&mut h, t);
    assert_eq!(r.applied, [table::TALK]);
    assert!(r.restart.is_empty());
    assert!(r
        .summary()
        .starts_with("reloaded: talk; needs a restart: nothing;"));

    assert_eq!(tells_on_use(&mut h, b, me), [0xB2], "the new line, at once");

    let same = tables_of(&h);
    let r = reload(&mut h, same);
    assert_eq!(
        r.applied,
        Vec::<&str>::new(),
        "a reload of the same database changes nothing"
    );
    assert!(r.details.is_empty());
}

/// A reload whose database no longer names a template keeps the template known (a thing of it may
/// be in the world), and takes a new one.
/// Behaviour: server.operator.reload-keeps-what-the-world-holds
#[test]
fn a_reload_keeps_every_template_the_world_may_hold() {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let held: Vec<u32> = h.server.catalog.templates.keys().copied().collect();
    let gone = held[0];

    let mut t = tables_of(&h);
    t.catalog.templates.remove(&gone);
    let new = test_template(0x4700_7777, 0x200, yalain_content::Props::default());
    t.catalog.insert(new.clone());
    let r: Reloaded = reload(&mut h, t);

    assert_eq!(r.applied, [table::TEMPLATES]);
    assert!(
        h.server.catalog.get(gone).is_some(),
        "the dropped template still known"
    );
    assert_eq!(
        h.server.catalog.get(new.id),
        Some(&new),
        "the new one taken"
    );
}

/// The conflict statuses a client was told since `from`.
fn statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == yalain_world::kingdoms::STATUS => {
                c.args.as_ref()
            }
            _ => None,
        })
        .filter_map(|a| calls::values(&a.0).ok())
        .filter_map(|v| match v.first() {
            Some(calls::Value::Int(s)) => Some(*s),
            _ => None,
        })
        .collect()
}

/// A reload whose database makes the player's landblock a free-for-all: the player is told the
/// new status at once, and a reload of the same areas tells it nothing.
/// Behaviour: server.operator.reload-tells-new-conflict-areas
#[test]
fn a_reload_that_changes_the_conflict_where_a_player_stands_tells_it_at_once() {
    use yalain_content::Conflict;
    let (mut h, b, _) = world();
    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.content.conflict.insert(0x0299, Conflict::FreeForAll);
    let r = reload(&mut h, t);
    h.run(5);
    assert_eq!(r.applied, [table::CONFLICT]);
    assert_eq!(statuses(&h, b, from), [Conflict::FreeForAll.status()]);

    let from = h.bots[b].inbox.len();
    let same = tables_of(&h);
    reload(&mut h, same);
    h.run(5);
    assert!(statuses(&h, b, from).is_empty());
}
