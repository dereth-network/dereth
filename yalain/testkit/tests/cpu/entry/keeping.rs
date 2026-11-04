//! What a character keeps from one session to the next and is sent as it enters again: the weapon
//! in its hand (listed in the session start's inventory tables, made in its hand), and when it
//! last used each skill (the time it was away not counting toward the skill's wait), every quest it
//! holds with what is left of each one's wait to be taken again (but the hidden ones), a timed
//! quest's time standing still while it is away (one with no time left as it left failed).
//! Fixture: a new human in the test hall with the starting bow, beside the test camp's beetle.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::types::PhysicsDesc;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{QuestDef, QuestTable};
use yalain_store::Store;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::quests::{status, CANCEL_WAIT};
use yalain_world::session::function;
use yalain_world::skill_use::{self, refusal};
use yalain_world::state::QuestState;

const BOW: u32 = 0x4700_051B;
const DEADEYE: u32 = 207;
/// Deadeye's recovery in the test that keeps its last use, in seconds.
const RECOVERY: f32 = 60.0;
/// The places a bow takes (both hands), and where it is (the primary hand).
const BOTH_HANDS: u32 = 192;
const PRIMARY_HAND: u32 = 64;
/// The bow's own mode: strung.
const BOW_MODE: u32 = 0x4100_0010;

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The session start's values since `from`, in packing order.
fn session_start(h: &Harness, b: usize, from: usize) -> Vec<Value> {
    calls_of(h, b, from)
        .into_iter()
        .find(|(f, _)| *f == function::SESSION_START)
        .map(|(_, v)| v)
        .expect("a session start")
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// Run the world for so many seconds, a second a step.
fn wait_coarse(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(1.0);
    }
}

/// The script objects of one of the session start's graphs: their packages and slot values.
fn objects(v: &Value) -> Vec<(u16, Vec<u32>)> {
    match v {
        Value::Graph(g) => g
            .iter()
            .filter_map(|(_, n)| match n {
                Node::Script { package, slots } => {
                    Some((*package, slots.iter().map(|(_, x)| *x).collect()))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The root of one of the session start's graphs: a native's type and bytes.
fn root_native(v: &Value) -> (u16, Vec<u8>) {
    match v.root() {
        Some(Node::Native { ty, bytes }) => (*ty, bytes.clone()),
        other => panic!("not a native root: {other:?}"),
    }
}

/// A bow wielded when the character logs off is in its hand when it enters again: the session
/// start lists it in the inventory tables (the places filled, the bow by where it is and by
/// itself: the hands it takes, the primary hand, a bow's implement, an empty look) and not among
/// the pack's contents; the bow is made with the player as its container and its wielder, hung
/// from the player's hand, strung.
/// Behaviour: server.world.a-wielded-weapon-is-kept-across-sessions
#[test]
fn a_bow_wielded_at_logoff_is_in_hand_again_at_the_next_entry() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let bow = h.arm(b, me, BOW);
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);

    let start = session_start(&h, b, from);
    assert_eq!(start[6], Value::Int(BOTH_HANDS), "the places filled");
    let (ty, bytes) = root_native(&start[7]);
    assert_eq!(ty, 17, "by where it is: a table of words");
    assert_eq!(bytes[..4], [1, 0, 0, 0], "one entry, eleven buckets");
    assert_eq!(bytes[4..8], PRIMARY_HAND.to_le_bytes(), "keyed by the hand");
    let (ty, bytes) = root_native(&start[8]);
    assert_eq!(ty, 0x40, "by item: a table of 64-bit keys");
    assert_eq!(bytes[4..12], bow.to_le_bytes(), "keyed by the bow");
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (bow as u32, (bow >> 32) as u32);
    for v in &start[7..=8] {
        let profiles: Vec<Vec<u32>> = objects(v)
            .into_iter()
            .filter(|(p, _)| *p == 0x1F9)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(profiles.len(), 1, "one profile");
        assert_eq!(
            profiles[0][1..],
            [BOTH_HANDS, PRIMARY_HAND, skill_use::implement::BOW, lo, hi],
            "the bow's places, its hand, its implement"
        );
        let nodes = match v {
            Value::Graph(g) => g.clone(),
            _ => unreachable!(),
        };
        let look = |ty: u16| {
            nodes.iter().find_map(|(_, n)| match n {
                Node::Native { ty: t, bytes } if *t == ty => Some(bytes.clone()),
                _ => None,
            })
        };
        assert_eq!(
            look(112),
            Some([1.0f32.to_le_bytes(); 3].concat()),
            "scale 1"
        );
        assert_eq!(look(15), Some(vec![0, 0, 0, 2]), "no appearance");
        assert_eq!(look(113), Some(vec![0; 4]), "an empty visual description");
    }
    let contents = match start[11].root() {
        Some(Node::Native { bytes, .. }) => bytes.clone(),
        other => panic!("not a list: {other:?}"),
    };
    assert!(
        !contents.windows(8).any(|w| w == bow.to_le_bytes()),
        "not in the pack's contents"
    );

    let msgs = h.world_messages(b, from);
    let instance = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == me => Some(o.physics.instance_stamp),
            _ => None,
        })
        .expect("the player made");
    let o = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == bow => Some(o.clone()),
            _ => None,
        })
        .expect("the bow made");
    assert_eq!(o.weenie.objects[..2], [me, me], "contained and wielded");
    assert_ne!(
        o.physics.bits & PhysicsDesc::PARENT,
        0,
        "hung from the player"
    );
    assert_eq!(
        (
            o.physics.parent.parent,
            o.physics.parent.location,
            o.physics.parent.stamp
        ),
        (me, 2, instance),
        "from the hand a bow is held in"
    );
    assert_eq!(o.physics.mode, BOW_MODE, "strung");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == bow && i.location == BOTH_HANDS),
        "still wielded"
    );
}

/// Ask to use a skill on a target, as the client packs it.
fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill_use::function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

/// A skill used before a logoff comes back in the next session start with when it was last used
/// and its mask saying so; the time the character was away does not count: the last use moves on
/// by it, and a skill still waiting its recovery at the logoff is still waiting at the entry.
/// Behaviour: server.skills.last-use-kept-across-sessions
#[test]
fn a_skills_last_use_comes_back_and_its_wait_does_not_run_while_away() {
    let mut h = Harness::new();
    // A recovery longer than the character's linger in the world after its logoff.
    if let Some(u) = h
        .server
        .skill_table
        .skills
        .get_mut(&DEADEYE)
        .and_then(|d| d.usage.as_mut())
    {
        u.recovery = RECOVERY;
    }
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    h.arm(b, me, BOW);
    use_skill(&mut h, b, 1, DEADEYE, beetle);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    assert!(
        h.run_until(400, |h| h.server.players[&world]
            .last_used
            .contains_key(&DEADEYE)),
        "used"
    );
    let used = h.server.players[&world].last_used[&DEADEYE];
    log_off(&mut h, b, me);
    let kept = h
        .server
        .store
        .world_state(me)
        .ok()
        .flatten()
        .and_then(|st| yalain_world::state::PlayState::from_bytes(&st.play).ok())
        .expect("kept");
    assert_eq!(kept.last_used, [(DEADEYE, used)], "its last use kept");
    let left = kept.kept_at;
    wait_coarse(&mut h, 100.0);
    let from = h.bots[b].inbox.len();
    let asked = h.server.game_now;
    h.enter_world(b, "one", me);
    let entered = h.server.game_now;
    let start = session_start(&h, b, from);
    let deadeye = objects(&start[4])
        .into_iter()
        .find(|(p, s)| *p == 0x231 && s.get(14) == Some(&DEADEYE))
        .map(|(_, s)| s)
        .expect("Deadeye's information");
    let time = f64::from_bits(u64::from(deadeye[1]) << 32 | u64::from(deadeye[0]));
    // The entry came between the request and the welcome's arrival.
    assert!(asked - left > 99.0, "away a while");
    assert!(
        time - used >= asked - left && time - used <= entered - left,
        "the last use moved on by the time away: {time} against {used} + {asked} - {left}"
    );
    assert_ne!(
        deadeye[7] & skill_use::HAS_LAST_USED,
        0,
        "the mask says it was used"
    );
    scene_drawn(&mut h, b);
    let in_world = h.server.game_now - time;
    assert!(
        f64::from(RECOVERY) - in_world > 1.0,
        "less of Deadeye's recovery gone in the world than it needs: {in_world}"
    );
    let from = use_skill(&mut h, b, 2, DEADEYE, beetle);
    h.run(4);
    assert!(
        calls_of(&h, b, from).contains(&(
            skill_use::function::REFUSED,
            vec![
                Value::Int(DEADEYE),
                Value::Long(beetle),
                Value::Int(refusal::NOT_RESET)
            ]
        )),
        "still waiting its recovery"
    );
}

/// A chest piece worn when the character logs off is worn again at the next entry: the character
/// list draws the character wearing it, the session start lists it on the chest with the
/// appearance it gives the wearer as its look, the item is made contained and worn by the player
/// and hung from nothing, the player is made wearing it, and a player who comes into view later
/// sees it worn.
/// Behaviour: server.world.worn-armor-kept-across-sessions
#[test]
fn a_chest_piece_worn_at_logoff_is_worn_again_and_seen_at_the_next_entry() {
    use yalain_content::{Prop, Props};
    const CHEST: u32 = 0x4700_7101;
    const CHEST_LOOK: u32 = 0x2000_7101;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let (race, sex) = {
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == me)
            .unwrap();
        (p.race, p.sex)
    };
    let mut props = Props::default();
    props
        .set("Item_ValidInventoryLocations", Prop::Word(2))
        .set("Item_PreferredInventoryLocation", Prop::Word(2))
        .set("Item_WornAppearanceRace1", Prop::Word(race))
        .set("Item_WornAppearanceSex1", Prop::Word(sex))
        .set("Item_WornAppearanceFile1", Prop::Word(CHEST_LOOK));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CHEST, 0x180, props));
    let chest = h.arm(b, me, CHEST);
    let wears = |v: &auberean_protocol::types::VisualDesc| {
        v.global
            .appearance
            .get(CHEST_LOOK)
            .is_some_and(|t| t.entries == [(0x10, 1.0)])
    };
    let before = h.bots[b].inbox.len();
    log_off(&mut h, b, me);
    let listed = h.bots[b].inbox[before..]
        .iter()
        .find_map(|r| match r.message() {
            Some(ServerMessage::CharacterList(l)) => Some(l),
            _ => None,
        })
        .expect("the character list");
    let entry = listed.characters.iter().find(|c| c.id == me).unwrap();
    assert!(wears(&entry.visual), "listed wearing it");

    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let start = session_start(&h, b, from);
    assert_eq!(start[6], Value::Int(2), "the chest filled");
    for v in &start[7..=8] {
        let profiles: Vec<Vec<u32>> = objects(v)
            .into_iter()
            .filter(|(p, _)| *p == 0x1F9)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(profiles.len(), 1, "one profile");
        assert_eq!(profiles[0][1..4], [2, 2, 0], "the chest, no implement");
        let nodes = match v {
            Value::Graph(g) => g.clone(),
            _ => unreachable!(),
        };
        let native = |ty: u16| {
            nodes.iter().find_map(|(_, n)| match n {
                Node::Native { ty: t, bytes } if *t == ty => Some(bytes.clone()),
                _ => None,
            })
        };
        let app = native(15).expect("an appearance");
        assert_eq!(app[..4], [1, 0, 0, 2], "one appearance");
        assert_eq!(app[4..8], CHEST_LOOK.to_le_bytes(), "the chest's");
        let vd = native(113).expect("a visual description");
        assert_eq!(vd[..4], 0x100u32.to_le_bytes(), "whole-object values only");
    }
    let msgs = h.world_messages(b, from);
    let o = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == chest => Some(o.clone()),
            _ => None,
        })
        .expect("the chest made");
    assert_eq!(o.weenie.objects[..2], [me, me], "contained and worn");
    assert_eq!(
        o.physics.bits, 0x82,
        "hung from nothing: a mode and sliders"
    );
    let player = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == me => Some(o.visual.clone()),
            _ => None,
        })
        .expect("the player made");
    assert!(wears(&player), "made wearing it");

    let fw = 0;
    let (w, _) = h.player_in_world("two", "Bran");
    h.run(10);
    let seen = h
        .world_messages(w, fw)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == me => Some(o.visual.clone()),
            _ => None,
        })
        .expect("seen");
    assert!(wears(&seen), "seen wearing it");
}

/// An item made during a session is made with instance stamp 0; each time its holder enters again
/// it is made with one more than it had, and every message about it that session carries that.
/// Behaviour: server.world.item-instance-stamp-counts-entries
#[test]
fn an_items_instance_stamp_is_one_more_at_each_entry() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let world = *h.server.players.keys().next().expect("in the world");
    let from = h.bots[b].inbox.len();
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.run(5);
    let made = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .find_map(|m| match m {
                ServerMessage::CreateObject(o) if o.id == item => Some(o.physics.instance_stamp),
                _ => None,
            })
            .expect("made")
    };
    assert_eq!(made(&h, from), 0, "made in the session: 0");
    for n in 1..=2 {
        log_off(&mut h, b, me);
        let from = h.bots[b].inbox.len();
        h.enter_world(b, "one", me);
        assert_eq!(made(&h, from), n, "entry {n}");
        assert_eq!(
            h.server.things.get(item).map(|t| t.instance),
            Some(n),
            "kept for the session"
        );
    }
}

/// A quest for the entry's list: its wait to be taken again an hour.
fn hour_quest(id: u32) -> QuestDef {
    QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (1..=3).map(|n| (n, 0x2500_17EE)).collect(),
        challenge: 5,
        duration: 0.0,
        timed_phase: 0,
        retry: 3600.0,
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
    }
}

/// The records of the session start's quest list.
fn quest_list(start: &[Value]) -> Vec<QuestInfo> {
    let (_, bytes) = root_native(&start[2]);
    let mut r = In::new(&bytes);
    let n = r.u32().expect("a count");
    (0..n)
        .map(|_| QuestInfo::unpack(&mut r).expect("a quest record"))
        .collect()
}

/// A character enters with every quest it holds in its session start's list, under way, done,
/// failed or cancelled; one no longer under way names what is left of its wait to be taken again
/// (an hour for a done or failed quest, five minutes for a cancelled one), nothing once it is over.
/// Behaviour: server.quests.every-quest-held-listed-at-entry
#[test]
fn every_quest_held_is_listed_at_entry_with_what_is_left_of_its_wait() {
    const UNDER_WAY: u32 = 0x4000_0101;
    const DONE_LONG_AGO: u32 = 0x4000_0102;
    const DONE_LATELY: u32 = 0x4000_0103;
    const FAILED: u32 = 0x4000_0104;
    const CANCELLED: u32 = 0x4000_0105;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.quests = QuestTable {
        quests: [UNDER_WAY, DONE_LONG_AGO, DONE_LATELY, FAILED, CANCELLED]
            .into_iter()
            .map(hour_quest)
            .collect(),
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let now = h.unix;
    let held = |quest, status, phase, ago: i64| QuestState {
        quest,
        status,
        phase,
        started: now - ago - 100,
        updated: now - ago,
    };
    for p in h.server.players.values_mut() {
        p.play.quests = vec![
            held(UNDER_WAY, status::UNDERWAY, 2, 50),
            held(DONE_LONG_AGO, status::COMPLETED, 0, 10_000),
            held(DONE_LATELY, status::COMPLETED, 0, 1000),
            held(FAILED, status::FAILED, 0, 600),
            held(CANCELLED, status::CANCELLED, 0, 100),
        ];
    }
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    let before = h.unix;
    h.enter_world(b, "one", me);
    let after = h.unix;
    let list = quest_list(&session_start(&h, b, from));
    let record = |id: u32| {
        list.iter()
            .find(|q| q.id == id)
            .unwrap_or_else(|| panic!("{id:08X} listed"))
    };
    assert_eq!(list.len(), 5, "every quest held");
    let wait = |q: &QuestInfo| f64::from_bits(q.seconds_until_retry);
    // What is left of a wait, between the entry's first and last second.
    #[allow(clippy::cast_precision_loss)]
    let left = |whole: f64, ago: i64| {
        (
            (whole - (after - now + ago) as f64 - 1.0).max(0.0),
            (whole - (before - now + ago) as f64 + 1.0).max(0.0),
        )
    };
    for (id, st, whole, ago) in [
        (DONE_LATELY, status::COMPLETED, 3600.0, 1000),
        (FAILED, status::FAILED, 3600.0, 600),
        (CANCELLED, status::CANCELLED, CANCEL_WAIT, 100),
    ] {
        let q = record(id);
        assert_eq!((q.status, q.phase), (st, 0), "{id:08X}");
        let (lo, hi) = left(whole, ago);
        assert!(
            wait(q) >= lo && wait(q) <= hi && wait(q) > 0.0,
            "{id:08X}: {} left of its wait, not {lo}..{hi}",
            wait(q)
        );
    }
    let under = record(UNDER_WAY);
    assert_eq!((under.status, under.phase), (status::UNDERWAY, 2));
    assert_eq!(wait(under), 0.0, "under way: no wait");
    let long = record(DONE_LONG_AGO);
    assert_eq!(long.status, status::COMPLETED);
    assert_eq!(wait(long), 0.0, "its wait over");
}

/// A worn item's effect stays on across a logout, the same record (its id, its spellcraft, from
/// the item) at the next entry; a made item keeps its own bonus with it.
/// Behaviour: server.world.worn-effects-kept-across-sessions
#[test]
fn a_worn_items_effect_and_its_own_bonus_are_kept_across_a_logout() {
    use yalain_content::{Prop, Props};
    use yalain_testkit::WORN_HEALTH_RATE_EFFECT;
    use yalain_world::effects::WORN;
    const CHEST: u32 = 0x4700_7201;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let mut props = Props::default();
    props
        .set("Item_ValidInventoryLocations", Prop::Word(2))
        .set("Item_PreferredInventoryLocation", Prop::Word(2));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CHEST, 0x180, props));
    let chest = h.arm(b, me, CHEST);
    // Its own bonus, as a recipe gives it, worn by taking it off and on again.
    if let Some(t) = h.server.things.get_mut(chest) {
        t.bonuses.push((WORN_HEALTH_RATE_EFFECT, 17.0));
    }
    for _ in 0..2 {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                yalain_world::requests::function::USE,
                &[yalain_testkit::play::use_of(chest, me)],
            ),
        );
        h.run(10);
    }
    let on = |h: &Harness| -> Vec<(u32, u32, f32, u64, u32)> {
        h.server
            .players
            .values()
            .find(|p| p.character == me)
            .map(|p| {
                p.play
                    .effects
                    .iter()
                    .map(|e| (e.eid, e.state, e.spellcraft, e.item, e.flags))
                    .collect()
            })
            .unwrap_or_default()
    };
    let before = on(&h);
    assert_eq!(before.len(), 1, "the bonus on: {before:?}");
    assert_eq!(
        (before[0].1, before[0].2, before[0].3, before[0].4),
        (WORN_HEALTH_RATE_EFFECT, 17.0, chest, WORN)
    );
    log_off(&mut h, b, me);
    h.enter_world(b, "one", me);
    h.run(10);
    assert_eq!(on(&h), before, "the same record at the next entry");
    assert_eq!(
        h.server.things.get(chest).map(|t| t.bonuses.clone()),
        Some(vec![(WORN_HEALTH_RATE_EFFECT, 17.0)]),
        "the item's own bonus kept with it"
    );
}

/// An item that binds when used in combat keeps the mark across a logout, and one not marked
/// stays unmarked.
/// Behaviour: server.items.binds-on-use-kept
#[test]
fn an_items_bind_on_use_mark_is_kept_across_a_logout() {
    const CHEST: u32 = 0x4700_7202;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.catalog.insert(yalain_testkit::test_template(
        CHEST,
        0x180,
        yalain_content::Props::default(),
    ));
    let marked = h.arm(b, me, CHEST);
    let plain = h.arm(b, me, CHEST);
    if let Some(t) = h.server.things.get_mut(marked) {
        t.binds_on_use = true;
    }
    log_off(&mut h, b, me);
    h.enter_world(b, "one", me);
    h.run(10);
    let mark = |id: u64| h.server.things.get(id).map(|t| t.binds_on_use);
    assert_eq!((mark(marked), mark(plain)), (Some(true), Some(false)));
}

/// A hidden quest a character holds is never in its session start's list, under way or done; the
/// shown ones beside it are.
/// Behaviour: server.quests.hidden-never-listed
#[test]
fn a_hidden_quest_is_never_listed_at_entry() {
    const SHOWN: u32 = 0x4000_0263;
    const HIDDEN_DONE: u32 = 0x4000_0269;
    const HIDDEN_UNDER_WAY: u32 = 0x8000_00EF;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.quests = QuestTable {
        quests: vec![
            hour_quest(SHOWN),
            QuestDef {
                hidden: true,
                ..hour_quest(HIDDEN_DONE)
            },
            QuestDef {
                hidden: true,
                ..hour_quest(HIDDEN_UNDER_WAY)
            },
        ],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let now = h.unix;
    let held = |quest, status, phase| QuestState {
        quest,
        status,
        phase,
        started: now - 100,
        updated: now - 50,
    };
    for p in h.server.players.values_mut() {
        p.play.quests = vec![
            held(SHOWN, status::COMPLETED, 0),
            held(HIDDEN_DONE, status::COMPLETED, 0),
            held(HIDDEN_UNDER_WAY, status::UNDERWAY, 1),
        ];
    }
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let list = quest_list(&session_start(&h, b, from));
    assert_eq!(
        list.iter().map(|q| q.id).collect::<Vec<_>>(),
        vec![SHOWN],
        "the shown quest alone"
    );
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.play.quests.len(), 3, "the hidden ones still held");
}

/// A quest with no time left when its character left (a server that stopped before its time told
/// it) enters failed: listed failed, with what is left of its wait from when its time ran out.
/// Behaviour: server.quests.ran-out-before-leaving
#[test]
fn a_quest_with_no_time_left_when_its_player_left_is_listed_failed() {
    const TIMED: u32 = 0x4000_0112;
    const STILL_TIMED: u32 = 0x4000_0113;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let timed = |id| QuestDef {
        duration: 600.0,
        timed_phase: 0,
        ..hour_quest(id)
    };
    h.server.quests = QuestTable {
        quests: vec![timed(TIMED), timed(STILL_TIMED)],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let now = h.unix;
    let given = |quest, ago: i64| QuestState {
        quest,
        status: status::UNDERWAY,
        phase: 1,
        started: now - ago,
        updated: now - ago,
    };
    for p in h.server.players.values_mut() {
        p.play.quests = vec![given(TIMED, 1000), given(STILL_TIMED, 100)];
    }
    log_off(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    let before = h.unix;
    h.enter_world(b, "one", me);
    let after = h.unix;
    let list = quest_list(&session_start(&h, b, from));
    let record = |id: u32| list.iter().find(|q| q.id == id).expect("listed");
    let ran_out = record(TIMED);
    assert_eq!((ran_out.status, ran_out.phase), (status::FAILED, 0));
    // Its wait (an hour) counted from when its time ran out, 400 s before the log off.
    #[allow(clippy::cast_precision_loss)]
    let (lo, hi) = (
        3600.0 - (after - now + 400) as f64 - 1.0,
        3600.0 - (before - now + 400) as f64 + 1.0,
    );
    let wait = f64::from_bits(ran_out.seconds_until_retry);
    assert!(wait >= lo && wait <= hi, "{wait} left, not {lo}..{hi}");
    let still = record(STILL_TIMED);
    assert_eq!((still.status, still.phase), (status::UNDERWAY, 1));
}

/// A timed quest's time stands still while its player is away, however long: it enters under way
/// with what it had left, and then runs out only after that much more time in the world. When it
/// was given moves on by the time away, for a done quest too.
/// Behaviour: server.quests.time-runs-only-in-the-world
#[test]
fn a_timed_quests_time_stands_still_while_its_player_is_away() {
    const TIMED: u32 = 0x4000_0112;
    const DONE: u32 = 0x4000_0113;
    const AWAY: f64 = 1000.0;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                duration: 600.0,
                timed_phase: 0,
                ..hour_quest(TIMED)
            },
            hour_quest(DONE),
        ],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let given = h.unix - 100;
    for p in h.server.players.values_mut() {
        p.play.quests = vec![
            QuestState {
                quest: TIMED,
                status: status::UNDERWAY,
                phase: 1,
                started: given,
                updated: given,
            },
            QuestState {
                quest: DONE,
                status: status::COMPLETED,
                phase: 0,
                started: given,
                updated: given,
            },
        ];
    }
    let leaving = h.unix;
    log_off(&mut h, b, me);
    let left = h.unix;
    wait_coarse(&mut h, AWAY);
    let from = h.bots[b].inbox.len();
    let entering = h.unix;
    h.enter_world(b, "one", me);
    let entered = h.unix;
    let list = quest_list(&session_start(&h, b, from));
    let record = |id: u32| list.iter().find(|q| q.id == id).expect("listed");
    let timed = record(TIMED);
    assert_eq!((timed.status, timed.phase), (status::UNDERWAY, 1));
    let time_left = f64::from_bits(timed.seconds_remaining);
    #[allow(clippy::cast_precision_loss)]
    let played = (left - leaving + entered - entering) as f64;
    assert!(
        time_left >= 500.0 - played - 2.0 && time_left <= 500.0 + 2.0,
        "{time_left} left, about 500 less the {played} s logging off and on"
    );
    // When each was given, as the list says, moved on alike by the time away: not the done one's
    // completion.
    let (a, d) = (timed, record(DONE));
    let bestowed = |q: &QuestInfo| f64::from_bits(q.bestowed);
    assert_eq!(
        bestowed(a),
        bestowed(d),
        "given at one time, and moved alike"
    );
    let moved = bestowed(d) - f64::from_bits(d.done);
    assert!(
        (moved - AWAY).abs() <= played + 2.0,
        "given {moved} s after it was done: the time away"
    );
    // It runs out only after what it has left, in the world.
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let under_way = |h: &Harness| {
        h.server.players[&world]
            .play
            .quests
            .iter()
            .any(|q| q.quest == TIMED && q.status == status::UNDERWAY)
    };
    wait_coarse(&mut h, time_left - 5.0);
    assert!(under_way(&h), "still has a little time");
    let from = h.bots[b].inbox.len();
    wait_coarse(&mut h, 10.0);
    assert!(!under_way(&h), "its time ran out");
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == yalain_world::quests::UPDATE_QUEST),
        "and it was told"
    );
}

/// A wait to be taken again runs while its player is away, a done, a failed and a cancelled
/// quest's alike; when it was done stays where it was.
/// Behaviour: server.quests.waits-run-while-away
#[test]
fn a_wait_to_be_taken_again_runs_while_its_player_is_away() {
    const DONE: u32 = 0x4000_0112;
    const FAILED: u32 = 0x4000_0113;
    const CANCELLED: u32 = 0x4000_0114;
    const OVER: u32 = 0x4000_0115;
    const AWAY: f64 = 1000.0;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.quests = QuestTable {
        quests: [DONE, FAILED, CANCELLED, OVER]
            .into_iter()
            .map(|q| QuestDef {
                duration: 600.0,
                timed_phase: 0,
                ..hour_quest(q)
            })
            .collect(),
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let now = h.unix;
    let held = |quest, status, ago: i64| QuestState {
        quest,
        status,
        phase: 0,
        started: now - ago - 100,
        updated: now - ago,
    };
    for p in h.server.players.values_mut() {
        p.play.quests = vec![
            held(DONE, status::COMPLETED, 100),
            held(FAILED, status::FAILED, 100),
            held(CANCELLED, status::CANCELLED, 100),
            held(OVER, status::COMPLETED, 3000),
        ];
    }
    log_off(&mut h, b, me);
    wait_coarse(&mut h, AWAY);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let entered = h.unix;
    let list = quest_list(&session_start(&h, b, from));
    let wait = |id: u32| {
        f64::from_bits(
            list.iter()
                .find(|q| q.id == id)
                .expect("listed")
                .seconds_until_retry,
        )
    };
    #[allow(clippy::cast_precision_loss)]
    let since = (entered - now + 100) as f64;
    assert!(since > AWAY, "away a while");
    for q in [DONE, FAILED] {
        let left = wait(q);
        assert!(
            (left - (3600.0 - since)).abs() <= 2.0,
            "{q:08X}: {left} s of its hour left, {since} s on"
        );
    }
    assert_eq!(wait(CANCELLED), 0.0, "its five minutes went by while away");
    assert_eq!(wait(OVER), 0.0, "its hour went by while away");
}

/// A quest under way in a save made before its time stood still keeps what it had left when it was
/// kept: the save's own form is read as it stands, it is neither lost nor given time back.
/// Behaviour: server.quests.a-saved-quest-keeps-what-it-had-left
#[test]
fn a_quest_under_way_in_a_save_keeps_the_time_it_had_left_when_it_was_kept() {
    const TIMED: u32 = 0x4000_0112;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            duration: 600.0,
            timed_phase: 0,
            ..hour_quest(TIMED)
        }],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    log_off(&mut h, b, me);
    // As a save holds it: given 3000 s before it was kept, of which 2800 s were not played, and
    // kept 5000 s ago. It has 400 s left by this reckoning, none by the calendar.
    wait_coarse(&mut h, 2.0);
    let kept_at = h.server.game_now - 5000.0;
    let mut st = h.server.store.world_state(me).ok().flatten().expect("kept");
    let mut play = yalain_world::state::PlayState::from_bytes(&st.play).expect("it reads");
    play.kept_at = kept_at;
    let given = h.unix - 5000 - 200;
    play.quests = vec![QuestState {
        quest: TIMED,
        status: status::UNDERWAY,
        phase: 1,
        started: given,
        updated: given,
    }];
    st.play = play.to_bytes();
    h.server.store.set_world_state(me, &st).expect("kept");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let list = quest_list(&session_start(&h, b, from));
    let timed = list.iter().find(|q| q.id == TIMED).expect("listed");
    assert_eq!(
        (timed.status, timed.phase),
        (status::UNDERWAY, 1),
        "not lost"
    );
    let left = f64::from_bits(timed.seconds_remaining);
    assert!((left - 400.0).abs() <= 5.0, "{left} s left, about 400");
}

/// Time away moves on when a quest under way came to its phase, so a later phase's time stands still
/// too; when a quest no longer under way ended stays put, its wait running on the calendar.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn time_away_moves_when_a_phase_came_and_not_when_a_quest_ended() {
    let held = |status, updated| QuestState {
        quest: 0x4000_0112,
        status,
        phase: 2,
        started: 1000,
        updated,
    };
    let mut quests = [held(status::UNDERWAY, 2000), held(status::COMPLETED, 2000)];
    yalain_world::quests::time_stood_still(&mut quests, 500, 10_000);
    assert_eq!(quests[0].updated, 2500, "moved on by the time away");
    assert_eq!(quests[1].updated, 2000, "stays put");
    let mut late = [held(status::UNDERWAY, 9800)];
    yalain_world::quests::time_stood_still(&mut late, 500, 10_000);
    assert_eq!(late[0].updated, 10_000, "never past now");
}

/// At entry, a quest is failed for having no time left only where its time runs at its phase and has
/// run out; one past the phase its duration runs in, however long ago it was given, stays under way.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn a_quest_at_a_phase_its_time_does_not_run_in_is_not_failed_at_entry() {
    const IN_FIRST: u32 = 0x4000_0112;
    const IN_SECOND: u32 = 0x4000_0113;
    let timed = |id, phase| QuestDef {
        duration: 600.0,
        timed_phase: phase,
        ..hour_quest(id)
    };
    let table = QuestTable {
        quests: vec![timed(IN_FIRST, 1), timed(IN_SECOND, 2)],
        steps: Vec::new(),
        gives: Vec::new(),
    };
    let at = |quest, phase, updated| QuestState {
        quest,
        status: status::UNDERWAY,
        phase,
        started: 1000,
        updated,
    };
    // A first phase's time counts from the giving, whatever moved it since.
    let mut held = [
        at(IN_FIRST, 2, 1000),
        at(IN_FIRST, 1, 9900),
        at(IN_SECOND, 1, 1000),
        at(IN_SECOND, 2, 9900),
        at(IN_SECOND, 2, 1000),
    ];
    yalain_world::quests::ran_out_before_leaving(&table, &mut held, 10_000);
    let statuses: Vec<u32> = held.iter().map(|q| q.status).collect();
    assert_eq!(
        statuses,
        [
            status::UNDERWAY,
            status::FAILED,
            status::UNDERWAY,
            status::UNDERWAY,
            status::FAILED
        ]
    );
    assert_eq!(held[4].updated, 1600, "as of when it ran out");
}

/// A quest given while its character was out of the world counts from when the character enters,
/// not from later; one with a time already ahead of now keeps it.
/// Behaviour: server.quests.given-out-of-the-world-counts-from-entry
#[test]
fn time_away_moves_a_quests_time_on_but_never_past_now() {
    let held = |started| QuestState {
        quest: 0x4000_0112,
        status: status::UNDERWAY,
        phase: 1,
        started,
        updated: started,
    };
    let mut quests = [held(1000), held(9000), held(20_000)];
    yalain_world::quests::time_stood_still(&mut quests, 500, 10_000);
    assert_eq!(quests[0].started, 1500, "moved on by the time away");
    assert_eq!(quests[1].started, 9500, "moved on by the time away");
    let mut late = [held(9800), held(20_000), held(i64::MAX - 1)];
    yalain_world::quests::time_stood_still(&mut late, 500, 10_000);
    assert_eq!(late[0].started, 10_000, "never past now");
    assert_eq!(late[1].started, 20_000, "one ahead of now stays");
    assert_eq!(late[2].started, i64::MAX - 1, "and does not overflow");
    assert_eq!(quests[2].started, 20_000, "one ahead of now stays");
}
