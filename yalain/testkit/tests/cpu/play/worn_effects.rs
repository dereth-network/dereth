//! What an item does to whoever wears or wields it: its effects come on as a record each just
//! before the equipping answer (one its wearer is never shown sends nothing), with "The <effect>
//! applies to you." for one a death does not end, and go just before the take-off's answer; a
//! weapon's likewise as it is wielded and put away; they stay through a death. Fixture: a player in
//! the test hall, a made-up chest piece and sword whose templates name effects modeled on 2017's
//! (a soulbound weapon's named bonus, an instrument's unseen appearance, a crafted regeneration
//! bonus).

use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, WORN_HEALTH_RATE_EFFECT};
use yalain_world::calls::Node;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::effects::{self, function as effect_fn, learned, WORN};
use yalain_world::equip::function as equip_fn;
use yalain_world::item_use::UsageEffect;
use yalain_world::session::function::SHOW_TEXT;
use yalain_world::state::EffectState;

const CHEST: u32 = 0x4700_7201;
const SWORD: u32 = 0x4700_7202;
/// A named bonus a death does not end (a soulbound weapon's: seen, internal flags 0x0100_0783).
const NAMED: u32 = 0x6F00_7201;
const NAMED_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_7201);
/// An appearance the wearer's client is never shown (an instrument's: internal flags 0x0140_0782).
const UNSEEN: u32 = 0x6F00_7202;
/// A bonus that watches effects put on (filed under category 8 besides the ordinary one).
const WATCHING: u32 = 0x6F00_7203;
const CHEST_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_7202);

fn effect(state: u32, internal_flags: u32, name: Option<(u32, u32)>) -> EffectDef {
    EffectDef {
        state,
        kind: Kind::Numeric {
            flags: 0x5,
            stats: vec![0x4_0125],
            magnitude: Scale(vec![(0.0, 5.0)]),
        },
        // A duration of its own, which a worn record does not take.
        duration: Scale(vec![(0.0, 60.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0x5,
        internal_flags,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 0,
        eq_class: state & 0xFFFF,
        name,
    }
}

/// A player in the world with the chest piece and the sword's templates and effects known; its
/// client, its character and its world connection.
fn wearer() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let mut chest = Props::default();
    chest
        .set("Item_ValidInventoryLocations", Prop::Word(2))
        .set("Item_PreferredInventoryLocation", Prop::Word(2));
    let mut t = yalain_testkit::test_template(CHEST, 0x180, chest);
    t.facts.name = Some(CHEST_NAME);
    h.server.catalog.insert(t);
    h.server.catalog.insert(yalain_testkit::test_template(
        SWORD,
        0x1067,
        Props::default(),
    ));
    h.server
        .catalog
        .implements
        .insert(SWORD, yalain_world::skill_use::implement::SWORD);
    for d in [
        effect(NAMED, 0x0100_0783, Some(NAMED_NAME)),
        effect(UNSEEN, 0x0140_0782, None),
        effect(WATCHING, 0x0940_0783, Some(NAMED_NAME)),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let at = |state: u32, spellcraft: f32| UsageEffect { state, spellcraft };
    h.server.worn_effects.insert(
        CHEST,
        vec![at(NAMED, 30.0), at(UNSEEN, 1.0), at(WATCHING, 12.0)],
    );
    h.server
        .worn_effects
        .insert(SWORD, vec![at(WORN_HEALTH_RATE_EFFECT, 25.0)]);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    (h, b, me, world)
}

fn use_it(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
    from
}

fn records(h: &Harness, world: ConnId) -> Vec<EffectState> {
    h.server.players[&world].play.effects.clone()
}

/// The functions of the calls since `from` that add or remove effects, show texts or answer an
/// equipping, in order.
fn sequence(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.functions(b, from)
        .into_iter()
        .filter(|f| {
            [
                effect_fn::ADD,
                effect_fn::REMOVE,
                SHOW_TEXT,
                equip_fn::EQUIP_DONE,
                equip_fn::UNEQUIP_DONE,
            ]
            .contains(f)
        })
        .collect()
}

/// A chest piece whose template names effects when worn, used from the pack: just before the
/// answer, each effect its wearer is shown comes on as a record of its own (cast by the item, from
/// it, at the template's spellcraft, held until it comes off, filed by its definition's flags),
/// the one a death does not end followed by "The <effect> applies to you." naming the chest; the
/// unseen one sends nothing and is not put on. Used again, its records are taken away just before
/// the take-off's answer.
/// Behaviour: server.equip.worn-effects-come-on-and-off
#[test]
fn a_worn_item_puts_its_effects_on_before_the_answer_and_takes_them_off_before_the_take_off() {
    let (mut h, b, me, world) = wearer();
    let chest = h.arm(b, me, CHEST);
    let on = records(&h, world);
    assert_eq!(on.len(), 2, "the two its wearer is shown: {on:?}");
    let named = on.iter().find(|e| e.state == NAMED).expect("the named one");
    assert_eq!(
        (named.flags, named.timeout, named.caster, named.item),
        (WORN, f32::MAX, chest, chest),
        "held until it comes off, cast by the item, from it"
    );
    assert!((named.spellcraft - 30.0).abs() < f32::EPSILON);
    assert_eq!(named.categories, 1, "an ordinary record");
    let watching = on.iter().find(|e| e.state == WATCHING).expect("the other");
    assert_eq!(watching.categories, 0x9, "filed as watching effects put on");
    assert!(
        !on.iter().any(|e| e.state == UNSEEN),
        "the unseen one is not"
    );
    let p = &h.server.players[&world];
    assert_eq!(
        effects::modifier(&h.server.effect_table, &p.play.effects, 0x4_0125).0,
        10.0,
        "both change their quality"
    );

    // Their order against the answer, made again from the start.
    let (mut h, b, me, world) = wearer();
    let from = h.bots[b].inbox.len();
    let chest = h.arm(b, me, CHEST);
    assert_eq!(
        sequence(&h, b, from),
        [
            effect_fn::ADD,
            SHOW_TEXT,
            effect_fn::ADD,
            equip_fn::EQUIP_DONE
        ],
        "the records, the named one's text, then the answer"
    );
    let texts = h.texts(b, from);
    assert_eq!(texts, [(2, learned::LINE)], "one text, of kind 2");
    let ids: Vec<u32> = records(&h, world).iter().map(|e| e.eid).collect();

    let from = use_it(&mut h, b, me, chest);
    assert_eq!(
        sequence(&h, b, from),
        [effect_fn::REMOVE, effect_fn::REMOVE, equip_fn::UNEQUIP_DONE],
        "taken away just before the answer"
    );
    let removed: Vec<u32> = h
        .calls(b, from)
        .iter()
        .filter(|(f, _)| *f == effect_fn::REMOVE)
        .filter_map(|(_, v)| match v.first() {
            Some(yalain_world::calls::Value::Int(e)) => Some(*e),
            _ => None,
        })
        .collect();
    assert_eq!(removed, ids, "each of its records");
    assert!(records(&h, world).is_empty(), "nothing left on");
}

/// A worn item taken off stops its effect's appearance right after that record's removal, before
/// the take-off's answer.
/// Behaviour: server.equip.worn-appearance-stops-with-its-record
#[test]
fn a_worn_items_appearance_stops_right_after_its_record_before_the_answer() {
    const NAMED_FX: u32 = 0x4100_7201;
    let (mut h, b, me, world) = wearer();
    if let Some(d) = h.server.effect_table.effects.get_mut(&NAMED) {
        d.fx = NAMED_FX;
    }
    let chest = h.arm(b, me, CHEST);
    let named = records(&h, world)
        .iter()
        .find(|e| e.state == NAMED)
        .map(|e| e.eid)
        .expect("the named record");
    let from = use_it(&mut h, b, me, chest);
    let msgs = h.sent_messages(b, from);
    let call = |m: &auberean_protocol::ServerMessage| match m {
        auberean_protocol::ServerMessage::PlayerCall(c) => Some(c.function.0),
        _ => None,
    };
    let named_stopped = |m: &auberean_protocol::ServerMessage| match m {
        auberean_protocol::ServerMessage::StopFx(e) => e.sender.id == me && e.action.0 == NAMED_FX,
        _ => false,
    };
    let stopped = msgs
        .iter()
        .position(named_stopped)
        .expect("the appearance stopped");
    let answer = msgs
        .iter()
        .position(|m| call(m) == Some(equip_fn::UNEQUIP_DONE))
        .expect("the answer");
    let removed = |m: &auberean_protocol::ServerMessage| match m {
        auberean_protocol::ServerMessage::PlayerCall(c) if c.function.0 == effect_fn::REMOVE => c
            .args
            .as_ref()
            .and_then(|a| yalain_world::calls::values(&a.0).ok())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    assert_eq!(
        stopped.checked_sub(1).map(|i| removed(&msgs[i])),
        Some(vec![yalain_world::calls::Value::Int(named)]),
        "right after its own record's removal"
    );
    assert!(stopped < answer, "before the answer");
}

/// The texts a client was shown since `from`.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<TextInfo> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
        })
        .collect()
}

/// A text's variable that holds another text: its string and table.
fn var(t: &TextInfo, id: u32) -> Option<(u32, u32)> {
    t.variables.iter().find_map(|v| match &v.value {
        VariableValue::Nested(n) if v.id == id => Some((n.string_id, n.table)),
        _ => None,
    })
}

/// The line a worn effect tells, naming the effect and the chest: "The <effect> applies to you."
/// where the effect's own table holds no line (2017: 182 of 182), and the line of its own table
/// where it holds one (an emblem's, 1 of 1).
/// Behaviour: server.equip.worn-effects-come-on-and-off
#[test]
fn a_worn_effect_tells_its_own_tables_line_or_else_that_it_applies() {
    let (mut h, b, me, _) = wearer();
    let from = h.bots[b].inbox.len();
    h.arm(b, me, CHEST);
    let said = shown(&h, b, from);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!((said[0].string_id, said[0].table), learned::APPLIES);
    assert_eq!(var(&said[0], learned::EFFECT_VAR), Some(NAMED_NAME));
    assert_eq!(var(&said[0], learned::GIVER_VAR), Some(CHEST_NAME));

    let (mut h, b, me, _) = wearer();
    h.server
        .effect_table
        .lines
        .insert(NAMED, (learned::LINE, NAMED_NAME.1));
    let from = h.bots[b].inbox.len();
    h.arm(b, me, CHEST);
    let said = shown(&h, b, from);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(
        (said[0].string_id, said[0].table),
        (learned::LINE, NAMED_NAME.1),
        "its own table's"
    );
    assert_eq!(var(&said[0], learned::GIVER_VAR), Some(CHEST_NAME));
}

/// A sword whose template names an effect when wielded: wielded, its record comes on before the
/// answer; a second sword wielded puts the first away, its record going before that answer; the
/// second put away by its use takes its own away.
/// Behaviour: server.equip.wielded-effects-come-on-and-off
#[test]
fn a_wielded_weapon_puts_its_effect_on_and_putting_it_away_takes_it_off() {
    let (mut h, b, me, world) = wearer();
    let from = h.bots[b].inbox.len();
    let first = h.arm(b, me, SWORD);
    assert_eq!(
        sequence(&h, b, from),
        [effect_fn::ADD, equip_fn::EQUIP_DONE],
        "a crafted bonus a death ends: no text"
    );
    let on = records(&h, world);
    assert_eq!(on.len(), 1);
    assert_eq!(
        (on[0].state, on[0].item, on[0].flags),
        (WORN_HEALTH_RATE_EFFECT, first, WORN)
    );
    let p = &h.server.players[&world];
    assert!(
        (effects::modifier(&h.server.effect_table, &p.play.effects, 0x3_010B).0 - 0.08).abs()
            < 1e-6,
        "its combat health rate counts"
    );

    let from = h.bots[b].inbox.len();
    let second = h.arm(b, me, SWORD);
    assert_eq!(
        sequence(&h, b, from),
        [
            effect_fn::REMOVE,
            equip_fn::UNEQUIP_DONE,
            effect_fn::ADD,
            equip_fn::EQUIP_DONE
        ],
        "the first's record off before its answer, the second's on before its own"
    );
    let on = records(&h, world);
    assert_eq!(on.iter().map(|e| e.item).collect::<Vec<_>>(), [second]);

    let from = use_it(&mut h, b, me, second);
    assert_eq!(
        sequence(&h, b, from),
        [effect_fn::REMOVE, equip_fn::UNEQUIP_DONE]
    );
    assert!(records(&h, world).is_empty());
}

/// An item's effects stay on through its wearer's death, even one a death ends on its own: a
/// resurrection takes off the buffs it ends and leaves the item's record.
/// Behaviour: server.equip.worn-effects-stay-through-a-death
#[test]
fn an_items_effects_stay_on_through_a_death_that_ends_a_buff_of_the_same_effect() {
    let (mut h, b, me, world) = wearer();
    let sword = h.arm(b, me, SWORD);
    let clock = h.clock();
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        WORN_HEALTH_RATE_EFFECT,
        effects::Cast {
            caster: me,
            spellcraft: 1.0,
            ..effects::Cast::default()
        },
        clock,
    );
    assert_eq!(records(&h, world).len(), 2, "the item's and a buff");
    effects::resurrected(&mut h.server, &mut h.net, world);
    let left = records(&h, world);
    assert_eq!(left.len(), 1, "the buff ended: {left:?}");
    assert_eq!((left[0].item, left[0].flags), (sword, WORN));
}

/// A weapon whose template names a quest's move for its wielder moves the quest as it is wielded,
/// before the answer, and holds no record of it; at the phase it moved to, wielding it again moves
/// nothing.
/// Behaviour: server.equip.worn-effect-moves-a-quest
#[test]
fn a_weapon_naming_a_quests_move_moves_the_quest_as_it_is_wielded() {
    use yalain_world::quests::{phase_of, update};
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x4000_7201;
    const ADVANCES: u32 = 0x6F00_7204;
    const BLADE: u32 = 0x4700_7203;
    let (mut h, b, me, world) = wearer();
    let mut q = super::kill_quests::quest(3);
    q.id = QUEST;
    h.server.quests.quests.push(q);
    h.server.catalog.insert(yalain_testkit::test_template(
        BLADE,
        0x1067,
        Props::default(),
    ));
    h.server
        .catalog
        .implements
        .insert(BLADE, yalain_world::skill_use::implement::SWORD);
    h.server.effect_table.effects.insert(
        ADVANCES,
        EffectDef {
            kind: Kind::Quest {
                quest: QUEST,
                update: update::ADVANCE | 2,
            },
            duration: Scale::default(),
            internal_flags: 0x4140_0512,
            ..effect(ADVANCES, 0, None)
        },
    );
    h.server.worn_effects.insert(
        BLADE,
        vec![UsageEffect {
            state: ADVANCES,
            spellcraft: 1.0,
        }],
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .quests
        .push(QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 1_000_000,
            updated: 1_000_000,
        });
    let phase = |h: &Harness| phase_of(&h.server.players[&world].play.quests, QUEST);
    let first = h.arm(b, me, BLADE);
    assert_eq!(phase(&h), 2);
    assert!(records(&h, world).iter().all(|e| e.item != first));
    use_it(&mut h, b, me, first);
    h.arm(b, me, BLADE);
    assert_eq!(phase(&h), 2);
}
