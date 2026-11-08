//! Pets: a Ranger's Lasher summoned by Nature's Aid (told of, then beside its owner), a summon past
//! the class's pets failing, the orders (a mode echoed, a name in the written form, death, attack
//! and return, too many too quickly), the pet keeping near its owner and taking on what its owner
//! attacks, a creature it strikes fighting it back, and its end with its owner (dying, leaving the
//! world) and through a portal, and its owner's kingdom it carries. Fixture: Nature's Aid, its pet
//! effect and the Lasher as the data defines them, a player with a sword in hand who trained the
//! skill, and the test world's beetles.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::{Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::kingdoms::{self, kingdom};
use yalain_world::pets::{self, function};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skills::{SkillDef, Usage};
use yalain_world::state::SkillState;
use yalain_world::world::Player;

const NATURES_AID: u32 = 227;
const PET_EFFECT: u32 = 0x7000_0805;
const LASHER: u32 = 0x4700_0846;
const LASHER_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_05D8);
const ICON: u32 = 0x4100_47E6;
const SWORD: u32 = 0x4700_051C;
const SKILL_INFO: u32 = 0x8214_000D;
const USE_SKILL: u32 = 0x8204_000B;
const ATTACK_STATE: u32 = 0x8204_001B;
const USE_ENDED: u32 = 0x8204_0019;
const USE_REFUSED: u32 = 0x8204_001A;

/// The Lasher's physics type, movement and placement ethereality (low and high words).
const LASHER_NUMBERS: [i32; 6] = [0x80_0000 | 0x20, 0, 0x0220_0010, 0, 0x42A0_0090, 0];

/// Nature's Aid, its pet effect and the Lasher, as the data defines them.
pub(super) fn with_natures_aid(h: &mut Harness) {
    let usage = Usage {
        min_range: -1.0,
        max_range: 1.0,
        recovery: 30.0,
        min_use: 0.5,
        powerup: 1.0,
        powerup_behavior: 0x4100_000C,
        vigor_add: -1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00ED; 3],
        behavior_self: [0x4100_00ED; 2],
        user_effects: vec![PET_EFFECT],
        ui_flags: 0x32C0,
        ..Usage::default()
    };
    h.server.skill_table.skills.insert(
        NATURES_AID,
        SkillDef {
            id: NATURES_AID,
            icon: ICON,
            cap: 150,
            table: 0x6F00_000B,
            flags: 0x41,
            active: true,
            usage: Some(usage),
            ..SkillDef::default()
        },
    );
    h.server.effect_table.effects.insert(
        PET_EFFECT,
        EffectDef {
            state: PET_EFFECT,
            kind: Kind::Pet {
                class: 16,
                lifetime: Scale::default(),
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 177,
            internal_flags: 0x4140_8513,
            enum_value: 538,
            eq_class: 0,
            name: None,
        },
    );
    let mut props = Props::default();
    props
        .set("GameplayStatistics_MaxHealth", Prop::Int(560))
        .set("GameplayStatistics_MaxVigor", Prop::Int(560))
        .set("AICombat_NaturalDamage", Prop::Float(45.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(40.0))
        .set("AI_HomesickRadius", Prop::Float(60.0))
        .set("Physics_VelScale", Prop::Float(2.0));
    let mut lasher = test_template(LASHER, 0x1169, props);
    lasher.description.numbers = LASHER_NUMBERS;
    lasher.description.name = Some(LASHER_NAME);
    lasher.description.scale = Some(0.7);
    h.server.catalog.insert(lasher);
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

/// A Ranger in the world with the scene drawn, a sword in hand, vigor to spare and Nature's Aid
/// trained; returns the client and the player.
fn ranger(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    let p = player(h, me);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    p.play.skills.push(SkillState {
        skill: NATURES_AID,
        granted: 0,
        mask: 0x1,
        cost: 2,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    (b, me)
}

/// Use Nature's Aid and let the use end; the messages since the request begin at the index
/// returned.
fn summon(h: &mut Harness, b: usize, use_id: u32) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            USE_SKILL,
            &[Arg::Int(use_id), Arg::Int(NATURES_AID), Arg::LongInt(0)],
        ),
    );
    h.run(120);
    from
}

/// The pet a call names (its 64-bit value).
fn pet_of(v: &[Value]) -> Option<u64> {
    v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    })
}

/// A ready Ranger with its Lasher out; returns the client, the player and the pet.
fn with_lasher(h: &mut Harness) -> (usize, u64, u64) {
    with_natures_aid(h);
    let (b, me) = ranger(h, "one", "Kestrel");
    let from = summon(h, b, 1);
    let pet = h
        .calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| pet_of(v))
        .expect("a pet added");
    (b, me, pet)
}

fn order(h: &mut Harness, b: usize, f: u32, args: &[Arg]) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(f, args));
    h.run(10);
    from
}

/// Nature's Aid ends with its pet: the player is told of a new pet (the Lasher's name, the skill's
/// icon, no time left, class 16, flags 1096) before the skill's information, and the Lasher
/// appears two meters in front of it after, naming it as its summoner.
/// Behaviour: server.pets.summoned
#[test]
fn natures_aid_tells_the_ranger_of_its_lasher_which_then_appears_beside_it() {
    let mut h = Harness::new();
    with_natures_aid(&mut h);
    let (b, me) = ranger(&mut h, "one", "Kestrel");
    let from = summon(&mut h, b, 1);
    let calls = h.calls(b, from);
    let add = calls
        .iter()
        .position(|(f, _)| *f == function::ADD)
        .expect("the pet added");
    let info = calls
        .iter()
        .position(|(f, _)| *f == SKILL_INFO)
        .expect("the skill's information");
    assert!(add < info, "the pet first: {:?}", h.functions(b, from));
    let v = &calls[add].1;
    let pet = pet_of(v).expect("its id");
    assert!(v.contains(&Value::Int(ICON)), "the skill's icon");
    assert!(v.iter().any(|x| matches!(x.root(), Some(Node::TextInfo(t))
        if (t.string_id, t.table) == LASHER_NAME)));
    let data = v
        .iter()
        .find_map(|x| match x.root() {
            Some(Node::Script { package, slots }) if *package == pets::PET_DATA => Some(slots),
            _ => None,
        })
        .expect("its data");
    let words: Vec<u32> = data.iter().map(|s| s.1).collect();
    assert_eq!(
        f64::from_bits(u64::from(words[1]) << 32 | u64::from(words[0])),
        -1.0
    );
    assert_eq!(&words[2..], &[16, 1096]);
    let msgs = h.world_messages(b, from);
    let made = msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(c) if c.id == pet => Some(c),
            _ => None,
        })
        .expect("the pet appears");
    assert_eq!(made.weenie.entity_did, LASHER);
    assert_eq!(made.weenie.objects[6], me, "its summoner");
    assert!(
        (made.physics.velocity_scale - 3.5).abs() < 1e-4,
        "1.75 times its kind's 2"
    );
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let mine = player(&mut h, me).position;
    let d = yalain_world::presence::distance(&at, &mine).expect("near");
    assert!((d - pets::APPEARS_AWAY).abs() < 0.01, "{d}");
    // In front of it, along its heading.
    let h8 = yalain_world::world::heading_of_rotation(mine.rotation);
    let a = (f32::from(h8) * 360.0 / 255.0).to_radians();
    let (dx, dy) = (at.origin[0] - mine.origin[0], at.origin[1] - mine.origin[1]);
    let ahead =
        dx * dereth_primitives::num::math::sinf(a) + dy * dereth_primitives::num::math::cosf(a);
    assert!(ahead > 1.9, "in front: {ahead}");
}

/// A summoning skill used while its user has as many live pets of its class as it allows is
/// refused as too many pets, in one tick: attacking, the use over, the refusal (the skill and its
/// reason), not attacking; no pet is told of and none appears.
/// Behaviour: server.pets.too-many-of-a-class
#[test]
fn a_second_lasher_is_refused_as_too_many_pets() {
    let mut h = Harness::new();
    let (b, _, first) = with_lasher(&mut h);
    h.run(700);
    let from = summon(&mut h, b, 2);
    let calls = h.calls(b, from);
    let fs: Vec<u32> = calls.iter().map(|(f, _)| *f).collect();
    assert_eq!(
        fs[..4],
        [ATTACK_STATE, USE_ENDED, USE_REFUSED, ATTACK_STATE],
        "{fs:?}"
    );
    assert!(calls[2].1.contains(&Value::Int(pets::TOO_MANY_PETS)));
    assert!(!fs.contains(&function::ADD));
    let owner = h.server.pets.by_id[&first].owner;
    assert_eq!(h.server.pets.of(owner), vec![first]);
    // Its first pet gone, the skill summons again.
    order(&mut h, b, function::DIE, &[Arg::LongInt(first)]);
    let from = summon(&mut h, b, 3);
    assert!(h.functions(b, from).contains(&function::ADD));
}

/// A pet's mode is echoed back, pet and mode, and kept.
/// Behaviour: server.pets.mode-echoed
#[test]
fn a_pets_mode_is_echoed_back_and_kept() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let from = order(
        &mut h,
        b,
        function::MODE,
        &[Arg::Int(0x4000_0003), Arg::LongInt(pet)],
    );
    let calls = h.calls(b, from);
    let echo = calls
        .iter()
        .find(|(f, _)| *f == function::UPDATE_MODE)
        .expect("echoed");
    assert_eq!(pet_of(&echo.1), Some(pet));
    assert!(echo.1.contains(&Value::Int(0x4000_0003)));
    assert_eq!(h.server.pets.by_id[&pet].mode, 0x4000_0003);
}

/// The literal text of a name a call carries: the nested reference's literal.
fn literal_names(v: &[Value]) -> Vec<String> {
    v.iter()
        .filter_map(|x| match x.root() {
            Some(Node::TextInfo(t)) => Some(t),
            _ => None,
        })
        .flat_map(|t| t.variables.iter())
        .filter_map(|var| match &var.value {
            auberean_script::native_object::VariableValue::Nested(n) => {
                n.literal.as_ref().map(|l| String::from_utf16_lossy(l))
            }
            _ => None,
        })
        .collect()
}

/// A pet renamed is named in the written form as "Name (kind)": its owner told, everyone who sees
/// it shown the name; the same name again tells only its owner.
/// Behaviour: server.pets.renamed
#[test]
fn a_pet_renamed_is_named_in_the_written_form_and_the_same_name_shows_nothing_new() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let name = |n: &str| {
        let mut g = yalain_world::graph::Builder::new();
        #[allow(clippy::cast_possible_truncation)]
        g.native(
            auberean_script::native_object::native_type::WP_STRING as u16,
            auberean_script::native_object::NativeObject::Text(n.encode_utf16().collect()),
        );
        Arg::Object(g.graph())
    };
    let from = order(
        &mut h,
        b,
        function::NAME,
        &[name("karl marx ii"), Arg::LongInt(pet)],
    );
    let calls = h.calls(b, from);
    let update = calls
        .iter()
        .find(|(f, _)| *f == function::UPDATE_NAME)
        .expect("named");
    assert_eq!(literal_names(&update.1), vec!["Karl Marx Ii".to_string()]);
    let shown = |h: &Harness, from: usize| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::UpdateStringInfoVisual(q)
                if q.object.id == pet && q.quality == pets::NAME_QUALITY)
        })
    };
    assert!(shown(&h, from), "shown as the pet's own name");
    let from = order(
        &mut h,
        b,
        function::NAME,
        &[name("Karl Marx II"), Arg::LongInt(pet)],
    );
    assert!(h.functions(b, from).contains(&function::UPDATE_NAME));
    assert!(!shown(&h, from), "the same name: nothing shown");
}

/// A name past the longest, or holding a digit or a mark, is refused with the client's own words and
/// changes nothing; the longest a name may be, an accented letter and a space are taken, and shown.
/// Behaviour: server.pets.a-name-can-be-refused
#[test]
fn a_pet_name_too_long_or_not_allowed_is_refused_and_the_pet_keeps_its_name() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let name = |n: &str| {
        let mut g = yalain_world::graph::Builder::new();
        #[allow(clippy::cast_possible_truncation)]
        g.native(
            auberean_script::native_object::native_type::WP_STRING as u16,
            auberean_script::native_object::NativeObject::Text(n.encode_utf16().collect()),
        );
        Arg::Object(g.graph())
    };
    let shown = |h: &Harness, from: usize| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::UpdateStringInfoVisual(q)
                if q.object.id == pet && q.quality == pets::NAME_QUALITY)
        })
    };
    let ok = "q".repeat(26);
    let long = "q".repeat(27);
    for (typed, refused) in [
        (ok.as_str(), None),
        ("bärbel", None),
        ("pet eins", None),
        (long.as_str(), Some(pets::text::NAME_TOO_LONG)),
        ("pet1", Some(pets::text::NAME_NOT_ALLOWED)),
        ("2", Some(pets::text::NAME_NOT_ALLOWED)),
        ("<b>x", Some(pets::text::NAME_NOT_ALLOWED)),
        ("   ", Some(pets::text::NAME_NOT_ALLOWED)),
    ] {
        let from = order(&mut h, b, function::NAME, &[name(typed), Arg::LongInt(pet)]);
        let answered = h.functions(b, from).contains(&function::UPDATE_NAME);
        let told = h.texts(b, from);
        match refused {
            None => {
                assert!(answered, "{typed:?} taken");
                assert!(shown(&h, from), "{typed:?} shown");
                assert!(told.is_empty(), "{typed:?} refused: {told:?}");
            }
            Some(why) => {
                assert!(!answered, "{typed:?} not taken");
                assert!(!shown(&h, from), "{typed:?} not shown");
                assert_eq!(told, vec![(1, why.0)], "{typed:?}");
            }
        }
    }
    assert_eq!(
        h.server.pets.by_id[&pet].name.as_deref(),
        Some("Pet Eins"),
        "the last name it took"
    );
}

/// A pet ordered to die (0: all) is taken off its owner's list and falls as a creature does, in
/// one tick; its corpse comes after, and its body leaves the world.
/// Behaviour: server.pets.ordered-to-die
#[test]
fn a_pet_ordered_to_die_falls_and_is_taken_off_its_owners_list() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let from = order(&mut h, b, function::DIE, &[Arg::LongInt(0)]);
    let msgs = h.world_messages(b, from);
    let fell = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::UpdateBoolVisual(q) if q.object.id == pet && q.quality == 279 && q.value == 1))
        .expect("dead");
    let removed = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == function::REMOVE))
        .expect("taken off the list");
    assert!(removed < fell, "off the list first");
    assert!(h.server.pets.by_id.is_empty());
    h.run(40);
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .any(|t| t.corpse.as_ref().is_some_and(|c| c.originator == pet));
    assert!(corpse, "its corpse a second and a half later");
    h.run(100);
    assert!(h.server.things.get(pet).is_none(), "its body is gone");
}

/// A beetle of the test world beside the Ranger, one that never attacks first.
fn beetle(h: &mut Harness) -> u64 {
    h.add_creatures(1);
    h.run(10);
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a beetle");
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.attacks_first = false;
    }
    id
}

fn conn_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("in the world")
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.target)
}

/// A pet keeps to supporting its owner: it takes on what its owner hurts (not before the owner's
/// first hit takes health) and strikes it, and the creature it strikes fights the pet.
/// Behaviour: server.pets.supports-its-owner
#[test]
fn a_pet_takes_on_what_its_owner_attacks_and_the_creature_fights_the_pet() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    // The owner beside the beetle, so its hits reach.
    let at = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("placed");
    let conn = conn_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin = [at.origin[0] - 1.0, at.origin[1], at.origin[2]];
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
    );
    let struck = h.run_until(400, |h| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet
                && e.action.hooks.iter().any(|k| k.target == beetle))
        })
    });
    assert!(
        struck,
        "the pet strikes its owner's target: {:?} {:?}",
        target_of(&h, pet),
        h.server
            .players
            .values()
            .map(|p| p.attacking.is_some())
            .collect::<Vec<_>>()
    );
    assert_eq!(target_of(&h, pet), Some(beetle));
    let hurt_first = h.world_messages(b, from).iter().position(|m| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
            && e.action.hooks.iter().any(|k| k.target == beetle && k.numbers[3] < 0))
    });
    let pet_first = h
        .world_messages(b, from)
        .iter()
        .position(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet));
    assert!(
        hurt_first.expect("the owner hurts it") < pet_first.expect("the pet strikes it"),
        "the owner's hit first"
    );
    assert!(
        h.run_until(100, |h| target_of(h, beetle) == Some(pet)),
        "the beetle fights the pet"
    );
}

/// A creature's strike at a pet comes when it is due, not at the world's next step: each gap is the
/// strike's own, within the harness's step.
/// Behaviour: server.creatures.attacks-when-due
#[test]
fn a_creatures_strike_at_a_pet_comes_when_it_is_due_not_at_the_worlds_next_step() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let at = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("placed");
    // The owner just beyond what the beetle notices, so it stays on the pet.
    let conn = conn_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position = at;
        p.position.origin[0] += 2.0;
    }
    if let Some(t) = h.server.things.get_mut(pet) {
        t.at = Some(at);
        t.max_health = 10_000_000;
        t.health = 10_000_000;
    }
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.mode = yalain_world::things::Mode::Chasing;
        c.target = Some(pet);
        c.next_attack = now;
        c.stats.movement.perception = 1.0;
    }
    let from = h.bots[b].inbox.len();
    let mut seen: Vec<(f64, u32)> = Vec::new();
    let mut told = 0;
    for _ in 0..1500 {
        h.step();
        assert_eq!(target_of(&h, beetle), Some(pet), "it stays on the pet");
        let strikes: Vec<u32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e)
                    if e.action.attacker == beetle
                        && e.action.hooks.iter().any(|k| k.target == pet) =>
                {
                    Some(e.action.skill)
                }
                _ => None,
            })
            .collect();
        if strikes.len() > told {
            seen.push((h.now.0, *strikes.last().expect("told")));
            told = strikes.len();
        }
    }
    assert!(seen.len() > 10, "{} strikes", seen.len());
    for pair in seen.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        let due = yalain_world::combat::strike_gap(pair[0].1);
        assert!(
            (gap - due).abs() <= 0.051 + 0.001,
            "a gap of {gap:.3} where {due:.3} was due"
        );
    }
}

/// A pet ordered to attack goes after the target, with no answer; a target more than 32 m from
/// the owner is refused with its text and no pet goes.
/// Behaviour: server.pets.attack-order
#[test]
fn an_attack_order_sends_the_pet_unless_the_target_is_too_far_from_its_owner() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let at = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("placed");
    // The owner 40 m off.
    let conn = conn_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin = [at.origin[0] - 40.0, at.origin[1], at.origin[2]];
    }
    let from = order(&mut h, b, function::ATTACK, &[Arg::LongInt(beetle)]);
    assert!(h
        .texts(b, from)
        .iter()
        .any(|(k, id)| (*k, *id) == (1, pets::text::TOO_FAR.0)));
    assert_eq!(target_of(&h, pet), None);
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin = [at.origin[0] - 30.0, at.origin[1], at.origin[2]];
    }
    let from = order(&mut h, b, function::ATTACK, &[Arg::LongInt(beetle)]);
    assert!(h.texts(b, from).is_empty(), "no answer");
    assert_eq!(target_of(&h, pet), Some(beetle));
}

/// Orders run up a debt of a second each: from none, six in one moment are carried out and the
/// seventh is refused with the seconds to wait (the debt past five).
/// Behaviour: server.pets.too-many-orders
#[test]
fn the_seventh_order_in_a_moment_is_refused_with_the_wait() {
    let mut h = Harness::new();
    let (b, _, _) = with_lasher(&mut h);
    let from = h.bots[b].inbox.len();
    for _ in 0..7 {
        h.bots[b].send(true, queue::WEENIE, &server_call(function::RETURN, &[]));
    }
    h.run(3);
    let refusals: Vec<f64> = h
        .calls(b, from)
        .iter()
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) if t.string_id == pets::text::TOO_MANY.0 => {
                    t.variables.iter().find_map(|var| match var.value {
                        auberean_script::native_object::VariableValue::Double(bits, _) => {
                            Some(f64::from_bits(bits))
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
        })
        .collect();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!((refusals[0] - 2.0).abs() < 1e-6, "{refusals:?}");
}

/// A pet called back drops its fight and is at peace until it is with its owner again.
/// Behaviour: server.pets.called-back
#[test]
fn a_pet_called_back_drops_its_fight() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    order(&mut h, b, function::ATTACK, &[Arg::LongInt(beetle)]);
    assert_eq!(target_of(&h, pet), Some(beetle));
    let from = order(&mut h, b, function::RETURN, &[]);
    assert_eq!(target_of(&h, pet), None);
    assert!(h.world_messages(b, from).iter().any(|m| matches!(m,
        ServerMessage::DoMode(e) if e.sender.id == pet && e.action.0 == 0x4000_0001)));
}

/// A pet that supports its owner does not fight back what strikes it; in a mode that defends
/// itself it does; struck down, it is taken off its owner's list.
/// Behaviour: server.pets.defends-itself-by-mode
#[test]
fn a_pet_fights_back_only_in_a_mode_that_defends_itself() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let strike = |h: &mut Harness| {
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_attack = 0.0;
        }
        let clock = h.clock();
        yalain_world::combat::thing_strikes_thing(
            &mut h.server,
            &mut h.net,
            beetle,
            pet,
            None,
            clock,
        );
        // Its blow reaches the pet.
        h.run(20);
    };
    let landed = |h: &Harness| {
        h.server
            .things
            .get(pet)
            .is_some_and(|t| t.health < t.max_health)
    };
    while !landed(&h) {
        strike(&mut h);
    }
    assert_eq!(target_of(&h, pet), None, "supporting: no fighting back");
    order(
        &mut h,
        b,
        function::MODE,
        &[Arg::Int(0x4000_0006), Arg::LongInt(pet)],
    );
    let before = h.server.things.get(pet).map_or(0, |t| t.health);
    while h.server.things.get(pet).is_some_and(|t| t.health == before) {
        strike(&mut h);
    }
    assert_eq!(
        target_of(&h, pet),
        Some(beetle),
        "self-defending: it fights back"
    );
    if let Some(t) = h.server.things.get_mut(pet) {
        t.health = 1;
    }
    let from = h.bots[b].inbox.len();
    while h.server.pets.is_pet(pet) {
        strike(&mut h);
    }
    h.run(2);
    assert!(h.functions(b, from).contains(&function::REMOVE));
}

/// A creature's strike takes a pet's health as the blow reaches it, not with its story.
/// Behaviour: server.pets.strikes-land-late
#[test]
fn a_creatures_strike_hurts_a_pet_when_its_blow_lands() {
    let mut h = Harness::new();
    let (_, _, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let whole = h.server.things.get(pet).map_or(0, |t| t.health);
    let mut landed_after = None;
    for _ in 0..200 {
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_attack = 0.0;
        }
        let clock = h.clock();
        yalain_world::combat::thing_strikes_thing(
            &mut h.server,
            &mut h.net,
            beetle,
            pet,
            None,
            clock,
        );
        assert_eq!(
            h.server.things.get(pet).map_or(0, |t| t.health),
            whole,
            "not in the story's tick"
        );
        let at = h.now.0;
        for _ in 0..30 {
            h.step();
            if h.server.things.get(pet).is_some_and(|t| t.health < whole) {
                landed_after = Some(h.now.0 - at);
                break;
            }
        }
        if landed_after.is_some() {
            break;
        }
    }
    // 0.696 s over a time scale of 1.296 give or take a tenth.
    let after = landed_after.expect("a strike landed");
    assert!((0.45..0.65).contains(&after), "{after}");
}

/// The pet is ordered at a beetle: steps until its first story there is seen, and returns when.
fn pet_strike_seen(h: &mut Harness, b: usize, pet: u64, beetle: u64) -> f64 {
    let from = h.bots[b].inbox.len();
    let told = |h: &Harness| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet
                && e.action.hooks.iter().any(|k| k.target == beetle))
        })
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::ATTACK, &[Arg::LongInt(beetle)]),
    );
    assert!(h.run_until(400, told), "the pet strikes");
    h.now.0
}

/// A pet's strike takes a creature's health as its blow reaches it, not with its story.
/// Behaviour: server.pets.strikes-land-late
#[test]
fn a_pets_strike_hurts_a_creature_when_its_blow_lands() {
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let whole = h.server.things.get(beetle).map_or(0, |t| t.health);
    let mut told = pet_strike_seen(&mut h, b, pet, beetle);
    assert_eq!(
        h.server.things.get(beetle).map_or(0, |t| t.health),
        whole,
        "not with its story"
    );
    let mut seen = h.bots[b].inbox.len();
    let mut after = None;
    for _ in 0..2000 {
        h.step();
        let another = h.world_messages(b, seen).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet
                && e.action.hooks.iter().any(|k| k.target == beetle))
        });
        seen = h.bots[b].inbox.len();
        if h.server
            .things
            .get(beetle)
            .is_some_and(|t| t.health < whole)
        {
            after = Some(h.now.0 - told);
            break;
        }
        if another {
            told = h.now.0;
        }
    }
    // 0.717 s over a time scale of 1.296 give or take a tenth, a step or two of the wire aside.
    let after = after.expect("a strike landed");
    assert!((0.35..0.65).contains(&after), "{after}");
}

/// A pet's strike carries the basic-attack flag by its skill, whichever pet swings it.
/// Behaviour: server.pets.strike-flag-by-skill
#[test]
fn a_pets_strike_carries_the_basic_flag_by_its_skill() {
    for (skill, bits) in [
        (490, 0x36),
        (491, 0x36),
        (1098, 0x36),
        (848, 0x16),
        (865, 0x16),
    ] {
        let mut h = Harness::new();
        let (b, _, pet) = with_lasher(&mut h);
        let beetle = beetle(&mut h);
        h.server.skill_table.skills.insert(
            skill,
            yalain_world::skills::SkillDef {
                id: skill,
                usage: Some(yalain_world::skills::Usage {
                    behavior_other: [0x4100_0077; 3],
                    max_range: 2.0,
                    hooks: vec![yalain_world::skills::Hook {
                        flags: 0x2002,
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        if let Some(c) = h
            .server
            .things
            .get_mut(pet)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.skills = vec![(skill, 1)];
        }
        let from = h.bots[b].inbox.len();
        pet_strike_seen(&mut h, b, pet, beetle);
        let told = h
            .world_messages(b, from)
            .iter()
            .find_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == pet => Some(e.action.clone()),
                _ => None,
            })
            .expect("its story");
        assert_eq!((told.skill, told.bits), (skill, bits));
    }
}

/// A pet striking a creature that sends harm back loses what it told.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_pet_striking_a_holder_of_what_sends_harm_back_loses_it() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale, Share, Watcher};
    use yalain_world::effects::{self, Cast, On};
    const WATCHER: u32 = 0x6F00_7321;
    let mut h = Harness::new();
    let (b, _, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    h.server.effect_table.effects.insert(
        WATCHER,
        EffectDef {
            state: WATCHER,
            kind: Kind::Watcher(Box::new(Watcher {
                sends_back: Share {
                    chance: 1.0,
                    amount: Scale(vec![(0.0, 0.01)]),
                    spread: 0.0,
                },
                against_a_player: 1.0,
                shares: true,
                watches: true,
                ..Watcher::default()
            })),
            duration: Scale(vec![(0.0, 100_000.0)]),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0x1100_0003,
            enum_value: 0,
            eq_class: 43,
            name: None,
        },
    );
    let clock = h.clock();
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Creature(beetle),
        WATCHER,
        Cast::default(),
        clock,
    );
    // It never strikes the pet itself: all the pet loses is what comes back.
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    let keep = h.server.things.get(pet).map_or(0, |t| t.health);
    let from = h.bots[b].inbox.len();
    pet_strike_seen(&mut h, b, pet, beetle);
    let lost = h.run_until(2000, |h| {
        h.server.things.get(pet).map_or(0, |t| t.health) < keep
    });
    assert!(lost, "it loses what came back");
    let told = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet
            && e.action.hooks.iter().any(|k| k.target == beetle && k.bits & 0x1000 != 0 && k.numbers[4] < 0))
    });
    assert!(told, "it is told");
}

/// A creature dead by the time a pet's blow reaches it is not hurt by it.
/// Behaviour: server.pets.strikes-land-late
#[test]
fn a_pets_strike_reaching_a_creature_dead_by_then_does_nothing() {
    let mut h = Harness::new();
    let (_, me, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, me, 1_000_000, clock);
    let hurt = h.server.things.get(beetle).map(|t| t.last_hurt);
    assert!(hurt.is_some());
    h.step();
    let clock = h.clock();
    yalain_world::combat::thing_strike_lands(
        &mut h.server,
        &mut h.net,
        (pet, 0),
        beetle,
        yalain_world::effects::Harm::plain(5),
        clock,
    );
    assert_eq!(h.server.things.get(beetle).map(|t| t.last_hurt), hurt);
}

/// An owner's death takes its pets in its death tick: each taken off the list.
/// Behaviour: server.pets.die-with-their-owner
#[test]
fn an_owners_death_takes_its_pets() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let from = h.bots[b].inbox.len();
    let conn = conn_of(&h, me);
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, conn, None, clock);
    assert!(!h.server.pets.is_pet(pet));
    h.run(3);
    let calls = h.calls(b, from);
    let death = calls
        .iter()
        .position(|(f, _)| *f == yalain_world::death::player_fn::DEATH_MESSAGE)
        .expect("the owner's death");
    let removed = calls
        .iter()
        .position(|(f, _)| *f == function::REMOVE)
        .expect("the pet taken off");
    assert!(death < removed);
}

/// An owner leaving the world takes its pets out of it, and another player sees the pet go.
/// Behaviour: server.pets.leave-with-their-owner
#[test]
fn an_owner_leaving_the_world_takes_its_pets_out_of_it() {
    let mut h = Harness::new();
    let (_, me, pet) = with_lasher(&mut h);
    let (c, _) = h.player_in_world("two", "Watcher");
    h.run(10);
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character != me && p.known.contains_key(&pet)));
    let from = h.bots[c].inbox.len();
    let conn = conn_of(&h, me);
    yalain_world::world::leave(&mut h.server, &mut h.net, conn);
    h.run(3);
    assert!(!h.server.pets.is_pet(pet) && h.server.things.get(pet).is_none());
    assert!(h
        .world_messages(c, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == pet)));
}

/// An owner who logs off lingers in the world, and its pet with it, until it leaves; then the pet
/// is forgotten by those who saw it in the owner's leaving instant, the owner told nothing of it.
/// Behaviour: server.pets.linger-with-their-owner
#[test]
fn an_owner_logging_off_keeps_its_pet_until_it_leaves() {
    use yalain_world::world::log_off;
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let (c, _) = h.player_in_world("two", "Watcher");
    h.run(10);
    let (from_b, from_c) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    let asked = h.now.0;
    h.bots[b].send(
        false,
        auberean_protocol::queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    while h.now.0 < asked + log_off::LEAVES_AFTER - 0.3 {
        h.step();
    }
    assert!(h.server.pets.is_pet(pet), "the pet is still in the world");
    while h.now.0 < asked + log_off::LEAVES_AFTER + 0.5 {
        h.step();
    }
    assert!(!h.server.pets.is_pet(pet) && h.server.things.get(pet).is_none());
    let seen = h.world_messages(c, from_c);
    let owner_left = seen
        .iter()
        .position(|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == me))
        .expect("the owner leaves");
    let pet_forgotten = seen
        .iter()
        .position(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == pet))
        .expect("the pet is forgotten");
    assert!(
        pet_forgotten < owner_left,
        "in its leaving instant, before it"
    );
    assert!(
        !h.calls(b, from_b)
            .iter()
            .any(|(f, _)| *f == function::REMOVE),
        "the owner is told nothing of its pet"
    );
}

/// A pet not fighting plans its way to its owner every three seconds, stopping three meters short
/// (its velocity scale told as it sets off), and one more than 100 m away is put beside its owner.
/// Behaviour: server.pets.follow
#[test]
fn a_pet_follows_its_owner_and_is_put_beside_it_when_far() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let conn = conn_of(&h, me);
    let start = h.server.players[&conn].position;
    let to =
        auberean_primitives::Vec3::new(start.origin[0] + 20.0, start.origin[1], start.origin[2]);
    let stamp = h.server.players[&conn].teleport_stamp;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(start.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp),
    );
    h.run(200);
    let owner_at = h.server.players[&conn].position;
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("near");
    assert!((2.0..4.0).contains(&d), "three meters short: {d}");
    assert!(h.world_messages(b, from).iter().any(|m| matches!(m,
        ServerMessage::VelocityScale(e) if e.sender.id == pet && (e.action.0 - 3.5).abs() < 1e-4)));
    // Far off: put beside its owner at its next plan.
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin[1] -= 140.0;
        p.position.cell = start.cell;
    }
    h.run(80);
    let owner_at = h.server.players[&conn].position;
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("near");
    assert!(d < 5.0, "{d}");
}

/// A pet goes with its owner through a portal: out of portal space, the pet is beside its owner
/// at the destination.
/// Behaviour: server.pets.through-portals
#[test]
fn a_pet_goes_with_its_owner_through_a_portal() {
    const PORTAL: u64 = 0x4002_9900_0000_0056;
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let conn = conn_of(&h, me);
    let here = h.server.players[&conn].position;
    let to = yalain_content::Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = yalain_content::ContentPack {
        objects: vec![yalain_content::ContentObject {
            object: Some(PORTAL),
            template: 0x4700_1154,
            description: yalain_content::Description {
                name_text: None,
                package: 0x1A00,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: here.cell,
            origin: here.origin,
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: yalain_content::Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(to),
            facts: yalain_content::Facts::default(),
            walks: None,
            gate: None,
        }],
        ..yalain_content::ContentPack::default()
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::requests::function::USE,
            &[yalain_testkit::play::use_of(PORTAL, me)],
        ),
    );
    h.run(60);
    let owner_at = h.server.players[&conn].position;
    assert_eq!(owner_at.cell, 0x7000_0001);
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("beside it");
    assert!(d < 3.0, "{d}");
}

/// A pet setting off from standing toward its owner is told its speed once, and again on arrival
/// (its kind's own, told even when unchanged); its plans while it runs tell nothing.
/// Behaviour: server.pets.speed-told-setting-off-and-arriving
#[test]
fn a_pets_speed_is_told_as_it_sets_off_and_as_it_arrives_and_not_between() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let conn = conn_of(&h, me);
    h.run(200);
    let start = h.server.players[&conn].position;
    let to =
        auberean_primitives::Vec3::new(start.origin[0] + 60.0, start.origin[1], start.origin[2]);
    let stamp = h.server.players[&conn].teleport_stamp;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(start.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp),
    );
    h.run(400);
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let owner_at = h.server.players[&conn].position;
    let d = yalain_world::presence::distance(&at, &owner_at).expect("near");
    assert!(d < 4.0, "arrived: {d}");
    let told: Vec<f32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::VelocityScale(e) if e.sender.id == pet => Some(e.action.0),
            _ => None,
        })
        .collect();
    assert_eq!(told.len(), 2, "setting off and arriving: {told:?}");
    assert!((told[1] - 3.5).abs() < 1e-4, "its kind's own on arrival");
}

/// The pet's own messages since `from`: (a mode it went into, or a velocity scale told).
fn modes_and_speeds(h: &Harness, b: usize, from: usize, pet: u64) -> Vec<(bool, f32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoMode(e) if e.sender.id == pet => Some((true, 0.0)),
            ServerMessage::VelocityScale(e) if e.sender.id == pet => Some((false, e.action.0)),
            _ => None,
        })
        .collect()
}

/// A pet that goes into a fight while still on its way to its owner is told its kind's own speed
/// right after its fighting mode; one that goes into a fight standing is told nothing.
/// Behaviour: server.pets.speed-told-entering-a-fight-mid-move
#[test]
fn a_pet_going_into_a_fight_on_its_way_is_told_its_own_speed_after_its_mode() {
    // Standing beside its owner: nothing told.
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    h.run(200);
    let beetle_id = beetle(&mut h);
    let conn = conn_of(&h, me);
    let at = h
        .server
        .things
        .get(beetle_id)
        .and_then(|t| t.at)
        .expect("placed");
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin = [at.origin[0] - 10.0, at.origin[1], at.origin[2]];
    }
    if let Some(t) = h.server.things.get_mut(pet) {
        let mut near = at;
        near.origin[0] -= 11.0;
        t.at = Some(near);
    }
    let from = order(&mut h, b, function::ATTACK, &[Arg::LongInt(beetle_id)]);
    h.run(20);
    let seen = modes_and_speeds(&h, b, from, pet);
    assert!(
        seen.first().is_some_and(|(mode, _)| *mode),
        "its mode: {seen:?}"
    );
    assert!(!seen.iter().any(|(mode, _)| !mode), "no speed: {seen:?}");

    // On its way: its own speed right after its mode.
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let conn = conn_of(&h, me);
    h.run(200);
    let start = h.server.players[&conn].position;
    let to =
        auberean_primitives::Vec3::new(start.origin[0] + 60.0, start.origin[1], start.origin[2]);
    let stamp = h.server.players[&conn].teleport_stamp;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(start.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp),
    );
    assert!(
        h.run_until(100, |h| !modes_and_speeds(h, b, from, pet).is_empty()),
        "it sets off"
    );
    let beetle_id = beetle(&mut h);
    let owner_at = h.server.players[&conn].position;
    if let Some(t) = h.server.things.get_mut(beetle_id) {
        let mut near = owner_at;
        near.origin[0] += 5.0;
        t.at = Some(near);
    }
    let from = order(&mut h, b, function::ATTACK, &[Arg::LongInt(beetle_id)]);
    h.run(20);
    let seen = modes_and_speeds(&h, b, from, pet);
    let mode = seen
        .iter()
        .position(|(m, _)| *m)
        .expect("its fighting mode");
    let told = seen.get(mode + 1).copied();
    let own = h
        .server
        .things
        .get(pet)
        .and_then(|t| h.server.catalog.get(t.template))
        .and_then(|t| t.props.float(pets::KIND_VELOCITY))
        .unwrap_or(1.0)
        * pets::VELOCITY_FACTOR;
    assert!(
        told.is_some_and(|(m, v)| !m && (v - own).abs() < 1e-4),
        "its kind's own speed ({own}) right after: {seen:?}"
    );
}

/// The pet a client was told of since `from`.
fn added_pet(h: &Harness, b: usize, from: usize) -> u64 {
    h.calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| pet_of(v))
        .expect("a pet added")
}

/// The weenie numbers of the pet's creation its owner's client was told of: its kingdom, its
/// physics type and its movement ethereality, `None` for a number left out.
fn kingdom_numbers(h: &Harness, b: usize, from: usize, pet: u64) -> [Option<i32>; 3] {
    let w = h
        .world_messages(b, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(c) if c.id == pet => Some(c.weenie),
            _ => None,
        })
        .expect("the pet appears");
    let bits = auberean_protocol::types::WeenieDesc::NUMBER_BITS;
    [2, 5, 7].map(|i| (w.mask & bits[i] != 0).then_some(w.numbers[i]))
}

/// A summoned pet is of its owner's kingdom (an owner in none: the neutral one): its creation
/// carries the kingdom, its physics type the kingdom's bit, and its movement ethereality its
/// owner's for where it stands with the pet's own bit. An owner whom kingdoms may fight anywhere
/// passes it on even while protected, which a pet never is.
/// Behaviour: server.pets.of-the-owners-kingdom
#[test]
fn a_lasher_appears_of_its_owners_kingdom_and_passes_through_what_its_owner_would() {
    let mut h = Harness::new();
    with_natures_aid(&mut h);
    let (b, me) = ranger(&mut h, "one", "Kestrel");
    kingdoms::set_kingdom(&mut player(&mut h, me).qualities, kingdom::ORDER);
    let from = summon(&mut h, b, 1);
    let pet = added_pet(&h, b, from);
    assert_eq!(
        kingdom_numbers(&h, b, from, pet),
        [Some(2), Some(0x1080_0020), Some(0x7E20_0010)],
        "the Order's, at peace"
    );
    let (c, wren) = ranger(&mut h, "two", "Wren");
    let from = summon(&mut h, c, 1);
    let pet = added_pet(&h, c, from);
    assert_eq!(
        kingdom_numbers(&h, c, from, pet),
        [
            Some(kingdom::NEUTRAL.cast_signed()),
            Some(0x0480_0020),
            Some(0x7E20_0010)
        ],
        "an owner in no kingdom: the neutral one"
    );
    let (d, tor) = ranger(&mut h, "three", "Tor");
    {
        let p = player(&mut h, tor);
        kingdoms::set_kingdom(&mut p.qualities, kingdom::DOMINION);
        kingdoms::set_permissions(&mut p.qualities, kingdoms::permission::ANYWHERE, 0);
    }
    let from = summon(&mut h, d, 1);
    let pet = added_pet(&h, d, from);
    assert_eq!(
        kingdom_numbers(&h, d, from, pet)[2],
        Some(0x6220_0010),
        "the Dominion's anywhere"
    );
    // Its owner protected (as after a recall): the pet passes through as before.
    player(&mut h, tor).protected = Some(yalain_world::effects::Protected {
        eid: 1,
        until: f64::INFINITY,
        ended: (0, 0),
    });
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    assert_eq!(
        pets::of_owners_kingdom(&h.server, tor, at.cell),
        Some((kingdom::DOMINION, 0x6220_0010)),
        "its owner's protection aside"
    );
    assert_ne!(wren, tor);
}

/// A pet that comes into a landblock of another conflict status takes its owner's movement
/// ethereality for it, with its own bit, and every client that knows it is told; within the same
/// landblock nothing is told.
/// Behaviour: server.pets.ethereality-where-it-stands
#[test]
fn a_pet_put_beside_its_owner_in_a_controlled_landblock_passes_through_its_kingdoms_there() {
    const NEXT_LANDBLOCK: u16 = 0x029A;
    let mut h = Harness::new();
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK, yalain_content::Conflict::Controlled);
    with_natures_aid(&mut h);
    let (b, me) = ranger(&mut h, "one", "Kestrel");
    kingdoms::set_kingdom(&mut player(&mut h, me).qualities, kingdom::ORDER);
    let from = summon(&mut h, b, 1);
    let pet = added_pet(&h, b, from);
    let ethereality = |h: &Harness, from: usize| -> Vec<i32> {
        h.world_messages(b, from)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateIntVisual(q)
                    if q.object.id == pet && q.quality == kingdoms::quality::ETHEREALITY =>
                {
                    Some(q.value)
                }
                _ => None,
            })
            .collect()
    };
    h.run(200);
    assert!(ethereality(&h, from).is_empty(), "nothing at the start");
    let conn = conn_of(&h, me);
    let from = h.bots[b].inbox.len();
    if let Some(p) = h.server.players.get_mut(&conn) {
        // More than 100 m away: the pet is put beside it at its next plan.
        p.position.cell = u32::from(NEXT_LANDBLOCK) << 16 | 0x0040;
        p.position.origin = [150.0, 150.0, 0.0];
    }
    h.run(80);
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    assert_eq!(at.cell >> 16, u32::from(NEXT_LANDBLOCK), "beside its owner");
    // The Order's in a controlled area passes through the Order and the neutral kingdom.
    assert_eq!(ethereality(&h, from), [0x5620_0010]);
}

/// A pet's physics type as it dies is its own, its owner's kingdom's bit and all, with the dead
/// bit.
/// Behaviour: server.pets.dies-of-its-kingdom
#[test]
fn a_dead_lasher_keeps_its_kingdom_in_its_dead_physics_type() {
    let mut h = Harness::new();
    with_natures_aid(&mut h);
    let (b, me) = ranger(&mut h, "one", "Kestrel");
    kingdoms::set_kingdom(&mut player(&mut h, me).qualities, kingdom::ORDER);
    let from = summon(&mut h, b, 1);
    let pet = added_pet(&h, b, from);
    let from = order(&mut h, b, function::DIE, &[Arg::LongInt(0)]);
    h.run(40);
    let physics: Vec<i32> = h
        .world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(q)
                if q.object.id == pet && q.quality == kingdoms::quality::PHYSICS_TYPE =>
            {
                Some(q.value)
            }
            _ => None,
        })
        .collect();
    assert_eq!(physics, [0x1080_0030]);
}

/// Another player's pet goes as its owner: while the owner may not be harmed, a pet's order at it is
/// refused and its owner's own blow is too, and it takes nothing; where the two may fight, it can be
/// fought.
/// Behaviour: server.pets.another-players-pet-goes-as-its-owner
#[test]
fn another_players_pet_takes_no_harm_while_its_owner_may_not_be_harmed() {
    let mut h = Harness::new();
    with_natures_aid(&mut h);
    let (a, me) = ranger(&mut h, "one", "Kestrel");
    let (b, _) = ranger(&mut h, "two", "Merrow");
    let pet_of_client = |h: &mut Harness, c: usize| {
        let from = summon(h, c, 1);
        h.calls(c, from)
            .iter()
            .find(|(f, _)| *f == function::ADD)
            .and_then(|(_, v)| pet_of(v))
            .expect("a pet added")
    };
    let mine = pet_of_client(&mut h, a);
    let theirs = pet_of_client(&mut h, b);
    let health = |h: &Harness, id: u64| h.server.things.get(id).map_or(0, |t| t.health);
    let refused = |h: &Harness, from: usize| {
        h.functions(a, from)
            .contains(&yalain_world::combat::function::BASIC_ATTACKS_FAILED)
    };
    let whole = health(&h, theirs);
    let from = order(&mut h, a, function::ATTACK, &[Arg::LongInt(theirs)]);
    assert!(refused(&h, from), "the order refused");
    h.run(400);
    assert_eq!(health(&h, theirs), whole, "its pet leaves it alone");
    let c = h.server.things.get(mine).and_then(|t| t.creature.as_ref());
    assert_ne!(
        c.and_then(|c| c.target),
        Some(theirs),
        "and is not after it"
    );
    let start = yalain_world::combat::function::START_ATTACK;
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(start, &[Arg::LongInt(theirs)]),
    );
    h.run(4);
    assert!(refused(&h, from), "its owner's own blow refused at once");
    assert!(
        h.server.players[&world].attacking.is_none(),
        "no attack begun"
    );
    h.run(400);
    assert_eq!(health(&h, theirs), whole, "nor does its owner touch it");
    // Its owner's blows stop as soon as the two may no longer fight.
    for p in h.server.players.values_mut() {
        p.conflict = 4;
    }
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(start, &[Arg::LongInt(theirs)]),
    );
    h.run(4);
    assert!(
        h.server.players[&world].attacking.is_some(),
        "an attack begun"
    );
    for p in h.server.players.values_mut() {
        p.conflict = 1;
    }
    let from = h.bots[a].inbox.len();
    h.run(40);
    assert!(refused(&h, from), "its next blow refused");
    assert!(
        h.server.players[&world].attacking.is_none(),
        "the attack ended"
    );
    if let Some(t) = h.server.things.get_mut(theirs) {
        t.health = whole;
    }
    // Where all may fight, it is fought.
    for p in h.server.players.values_mut() {
        p.conflict = 4;
    }
    let from = order(&mut h, a, function::ATTACK, &[Arg::LongInt(theirs)]);
    assert!(!refused(&h, from), "the order taken");
    h.run(400);
    assert!(health(&h, theirs) < whole, "and it is hurt");
    // Peace again: the pet already after it stops.
    for p in h.server.players.values_mut() {
        p.conflict = 1;
    }
    h.run(100);
    let c = h.server.things.get(mine).and_then(|t| t.creature.as_ref());
    assert_ne!(
        c.and_then(|c| c.target),
        Some(theirs),
        "peace ends the fight"
    );
}

/// A creature a pet kills is its owner's kill, and so is its corpse: the owner may open it and
/// another may not.
/// Behaviour: server.loot.a-pets-kill-is-its-owners
#[test]
fn a_corpse_a_pet_made_opens_to_its_owner_and_not_to_another() {
    let mut h = Harness::new();
    let (b, me, pet) = with_lasher(&mut h);
    let beetle = beetle(&mut h);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 1;
        t.max_health = 1;
    }
    let (_, oid) = h.player_in_world("two", "Bystander");
    pet_strike_seen(&mut h, b, pet, beetle);
    let there = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.corpse.as_ref().is_some_and(|c| c.originator == beetle))
            .map(|t| t.id)
    };
    assert!(h.run_until(2000, |h| there(h).is_some()), "a corpse");
    let corpse = there(&h).expect("a corpse");
    assert_eq!(
        h.server
            .things
            .get(corpse)
            .and_then(|t| t.corpse.as_ref())
            .map(|c| c.killer),
        Some(me),
        "the owner's kill"
    );
    let at = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.at)
        .expect("placed");
    for p in h.server.players.values_mut() {
        p.position = at;
    }
    let now = h.now.0;
    assert!(!yalain_world::death::may_loot(&h.server, oid, corpse, now));
    assert!(yalain_world::death::may_loot(&h.server, me, corpse, now));
}
