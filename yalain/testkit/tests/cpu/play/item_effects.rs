//! Skills aimed at an item: what they may be aimed at, what the effects do while they run, and that
//! they end with their time, however the item moves.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, STEP};
use yalain_world::combat::weapon_damage;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::profile::{self, item_line, name_look};
use yalain_world::skill_use::{function, implement, object_kind, refusal, target_bit};
use yalain_world::skills::{Held, Hook, SkillDef, Usage};

const SHIELD: u32 = 0x4700_7A01;
const SWORD: u32 = 0x4700_7A02;
const RING: u32 = 0x4700_7A03;
/// A skill on a shield of the user's, in its hands or its pack, and its effects.
const ON_SHIELD: u32 = 7001;
const SHIELD_UP: u32 = 0x7000_7A01;
const ATTUNES: u32 = 0x7000_7A02;
/// A skill on a weapon in its user's hands, and its effects.
const ON_WEAPON: u32 = 7002;
const WEAPON_UP: u32 = 0x7000_7A03;
const FX: u32 = 0x4100_029A;
/// The behavior of a use on an item.
const BEHAVIOR: u32 = 0x4100_009D;
const LASTS: f32 = 1800.0;
/// A skill whose effect lasts a minute and does not attune.
const BRIEF: u32 = 7003;
const BRIEF_UP: u32 = 0x7000_7A04;

fn numeric(stat: u32, magnitude: Vec<(f32, f32)>) -> EffectDef {
    EffectDef {
        state: 0,
        kind: Kind::Numeric {
            flags: 166,
            stats: vec![stat],
            magnitude: Scale(magnitude),
        },
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

fn chain(state: u32, seconds: f32, class: u32, list: Vec<EffectDef>) -> EffectDef {
    EffectDef {
        state,
        kind: Kind::Chained(list),
        duration: Scale(vec![(0.0, seconds)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: FX,
        external_flags: 0,
        internal_flags: 0x0140_0713,
        enum_value: 0,
        eq_class: class,
        name: Some((0xA860_7CEC, 0x2500_06EC)),
    }
}

fn skill(id: u32, valid_targets: u32, kinds: Vec<u32>, effects: Vec<u32>) -> SkillDef {
    SkillDef {
        id,
        active: true,
        cap: 150,
        usage: Some(Usage {
            max_range: 40.0,
            min_range: -1.0,
            recovery: 1.0,
            implements_right: -1,
            implements_left: -1,
            valid_targets,
            valid_object_kinds: kinds,
            behavior_other: [BEHAVIOR; 3],
            behavior_self: [BEHAVIOR; 2],
            target_effects: effects,
            hooks: vec![Hook {
                flags: 0x2018,
                mult: 1.0,
                chance: 1.0,
                anim_hook: 1,
                damages: true,
                ..Hook::default()
            }],
            ui_flags: 0x12C1,
            ..Usage::default()
        }),
        ..SkillDef::default()
    }
}

/// A world knowing a shield, a sword, a ring and the skills; a player in it, rated 30 in each skill.
fn world() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    let hand = |location: u32| {
        let mut p = Props::default();
        p.set("Item_ValidInventoryLocations", Prop::Word(location))
            .set("Item_PreferredInventoryLocation", Prop::Word(location))
            .set("Weapon_Damage", Prop::Int(92))
            .set("Weapon_Variance", Prop::Float(0.27))
            .set("Weapon_Speed", Prop::Int(7))
            .set("Weapon_VigorCost", Prop::Int(6));
        p
    };
    let mut shield = hand(128);
    shield.set("Item_ArmorLevel", Prop::Int(5));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SHIELD, 0x1068, shield));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SWORD, 0x1067, hand(64)));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(RING, 0x180, Props::default()));
    h.server
        .catalog
        .implements
        .insert(SHIELD, implement::SHIELD);
    h.server.catalog.implements.insert(SWORD, implement::SWORD);
    let up = |state: u32, seconds: f32, class: u32| {
        chain(
            state,
            seconds,
            class,
            vec![
                numeric(
                    yalain_world::made::stat::ARMOR,
                    vec![(1.0, 1.0), (25.0, 25.0), (50.0, 100.0), (100.0, 200.0)],
                ),
                numeric(
                    yalain_world::made::stat::DAMAGE,
                    vec![(1.0, 5.0), (50.0, 100.0), (100.0, 150.0)],
                ),
            ],
        )
    };
    let attunes = EffectDef {
        kind: Kind::Other,
        fx: 0,
        internal_flags: 0x0140_0712,
        name: None,
        ..chain(ATTUNES, LASTS, 114, Vec::new())
    };
    for d in [
        up(SHIELD_UP, LASTS, 82),
        attunes,
        up(WEAPON_UP, LASTS, 122),
        up(BRIEF_UP, 60.0, 31),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    for d in [
        skill(
            ON_SHIELD,
            target_bit::OWN_ITEM | target_bit::OF_KINDS,
            vec![object_kind::SHIELD],
            vec![SHIELD_UP, ATTUNES],
        ),
        skill(
            ON_WEAPON,
            target_bit::ITEM_IN_HAND | target_bit::OF_KINDS,
            vec![object_kind::WEAPON],
            vec![WEAPON_UP, ATTUNES],
        ),
        skill(
            BRIEF,
            target_bit::ITEM_IN_HAND | target_bit::OF_KINDS,
            vec![object_kind::WEAPON],
            vec![BRIEF_UP],
        ),
    ] {
        h.server.skill_table.skills.insert(d.id, d);
    }
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    rate(&mut h, me, 30);
    (h, b, me)
}

/// A player's rating in the skills.
fn rate(h: &mut Harness, me: u64, granted: u32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    p.play
        .skills
        .retain(|x| ![ON_SHIELD, ON_WEAPON, BRIEF].contains(&x.skill));
    for skill in [ON_SHIELD, ON_WEAPON, BRIEF] {
        p.play.skills.push(
            Held {
                skill,
                granted,
                xp: 0,
                mask: 3,
                cost: 2,
                children: 0,
                dependents: 0,
            }
            .into(),
        );
    }
}

/// Ask to use a skill, then run through its use.
fn cast(h: &mut Harness, b: usize, skill: u32, target: u64) -> usize {
    let from = asked(h, b, skill, target);
    h.run(steps(3.0));
    from
}

fn asked(h: &mut Harness, b: usize, skill: u32, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(1), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / STEP).round() as usize;
    n
}

/// Why a use was refused, as its answer says.
fn refusal_of(h: &Harness, b: usize, from: usize) -> Option<u32> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == function::REFUSED => {
            let v = yalain_world::calls::values(&c.args.as_ref()?.0).ok()?;
            v.iter().rev().find_map(|x| match x {
                yalain_world::calls::Value::Int(e) => Some(*e),
                _ => None,
            })
        }
        _ => None,
    })
}

fn fx_played(h: &Harness, b: usize, from: usize, item: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == item && e.action.fx == FX))
        .count()
}

fn most(h: &Harness, item: u64) -> i32 {
    weapon_damage(&h.server, item).expect("a weapon").most
}

/// Used on the shield in its hand, named or by naming its user: the story names the shield with a
/// landed hook, the item plays the behavior and then the look, and the shield's damage is up by the
/// effect's amount at the user's rating; while an attuning effect runs, it is attuned.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-lands-on-it
#[test]
fn a_skill_aimed_at_a_shield_lands_on_it_named_or_by_naming_its_user() {
    let (mut h, b, me) = world();
    let shield = h.arm(b, me, SHIELD);
    assert_eq!(most(&h, shield), 92);
    let from = asked(&mut h, b, ON_SHIELD, me);
    h.run(2);
    let using = h.server.players.values().find(|p| p.character == me);
    assert_eq!(
        using.and_then(|p| p.using.as_ref()).map(|x| x.turned),
        Some(false),
        "it does not turn toward what it holds"
    );
    h.run(steps(3.0));
    assert_eq!(refusal_of(&h, b, from), None);
    let story = h
        .world_messages(b, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) => Some(e.action),
            _ => None,
        })
        .expect("the story");
    let hook = &story.hooks[1];
    assert_eq!(hook.target, shield);
    assert_eq!(hook.bits, 897, "a landed hook");
    assert_eq!(hook.numbers[0].cast_unsigned(), 0x4000_0008);
    assert_eq!(hook.numbers[1], 1, "the animation's hook");
    assert_eq!(fx_played(&h, b, from, shield), 1);
    assert_eq!(most(&h, shield), 92 + 61, "at rating 30");
    let t = h.server.things.get(shield).expect("the shield");
    assert!(yalain_world::inventory::attuned_now(&h.server, t));
    assert_eq!(t.timed.len(), 2);
}

/// With a polearm in hand a skill's effect reaches the item later than with a sword: the look and
/// the effect wait on its longer play, and neither is lost.
/// Behaviour: server.skills.a-skill-on-an-item-lands-with-what-is-held
#[test]
fn a_skills_effect_reaches_a_polearm_later_than_a_sword() {
    const POLEARM: u32 = 0x4700_7A07;
    let (mut h, b, me) = world();
    let sword = h.arm(b, me, SWORD);
    asked(&mut h, b, ON_WEAPON, sword);
    h.run(steps(1.2));
    let on_sword = h.server.things.get(sword).expect("it").timed.len();
    assert_eq!(on_sword, 2, "a sword's by now");

    let (mut h, b, me) = world();
    let mut p = Props::default();
    p.set("Item_ValidInventoryLocations", Prop::Word(64))
        .set("Item_PreferredInventoryLocation", Prop::Word(64))
        .set("Weapon_Damage", Prop::Int(92))
        .set("Weapon_Variance", Prop::Float(0.27))
        .set("Weapon_Speed", Prop::Int(7))
        .set("Weapon_VigorCost", Prop::Int(6));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(POLEARM, 0x1067, p));
    h.server
        .catalog
        .implements
        .insert(POLEARM, implement::POLEARM);
    for u in h
        .server
        .skill_table
        .skills
        .values_mut()
        .filter_map(|d| d.usage.as_mut())
    {
        u.implements_right = -1;
    }
    let pole = h.arm(b, me, POLEARM);
    let from = asked(&mut h, b, ON_WEAPON, pole);
    h.run(steps(1.2));
    let timed = |h: &Harness| h.server.things.get(pole).expect("it").timed.len();
    assert_eq!(timed(&h), 0, "not yet");
    assert_eq!(fx_played(&h, b, from, pole), 0);
    h.run(steps(2.0));
    assert_eq!(refusal_of(&h, b, from), None);
    assert_eq!(timed(&h), 2, "landed, though the use is over");
    assert_eq!(fx_played(&h, b, from, pole), 1);
    h.server.things.get_mut(pole).expect("it").timed.clear();
    let from = asked(&mut h, b, ON_WEAPON, pole);
    h.run(steps(1.0));
    h.server.things.get_mut(pole).expect("it").wielded = None;
    h.server.things.get_mut(pole).expect("it").container = None;
    h.run(steps(3.0));
    assert_eq!(timed(&h), 0, "gone from its hands before it lands");
    assert_eq!(fx_played(&h, b, from, pole), 0);
}

/// Its examination while it runs: the rating and the damage drawn as enchanted, the effect among
/// its enchantments with its spellcraft and the countdown to its end; the attuning effect unlisted.
/// A weapon given a rating shows it, on its own line.
/// Behaviour: server.examination.an-item-shows-the-effects-skills-put-on-it
#[test]
fn an_items_profile_shows_what_a_skill_put_on_it() {
    let (mut h, b, me) = world();
    let shield = h.arm(b, me, SHIELD);
    let sword = h.arm(b, me, SWORD);
    h.server.things.get_mut(shield).expect("it").own.made = Some(yalain_world::made::Made {
        damage: Some((92, 0.27)),
        armor: Some(36),
        delay: Some(7),
        vigor: Some(6),
        ..yalain_world::made::Made::default()
    });
    cast(&mut h, b, ON_SHIELD, shield);
    h.run(steps(2.0));
    cast(&mut h, b, ON_WEAPON, sword);
    let t = h.server.things.get(shield).expect("the shield").clone();
    let p = yalain_world::requests::item_view(&h.server, &t);
    assert!(p.damage_changed && p.rating_changed);
    assert_eq!(p.weapon.map(|w| w.0), Some(153));
    assert_eq!(p.shield, Some(76), "36 and 40 at rating 30");
    let enchantments: Vec<_> = p
        .effects
        .iter()
        .filter(|e| e.when == profile::When::Enchantment)
        .collect();
    assert_eq!(enchantments.len(), 1, "the attuning effect is not listed");
    assert_eq!(enchantments[0].spellcraft, Some(30));
    let until = t
        .timed
        .iter()
        .find(|e| e.state == SHIELD_UP)
        .expect("on it")
        .until;
    assert_eq!(enchantments[0].ends, Some(until));
    let bytes = profile::item(0x3FF, &p).packed(&[]);
    assert!(
        bytes.windows(8).any(|w| w == until.to_bits().to_le_bytes()),
        "the countdown"
    );
    let t = h.server.things.get(sword).expect("the sword").clone();
    let p = yalain_world::requests::item_view(&h.server, &t);
    assert_eq!(p.rating_given, Some(40), "at rating 30");
    let drawn = profile::item(0x3FF, &p);
    let bytes = drawn.packed(&[]);
    let has = |id: u32| bytes.windows(4).any(|w| w == id.to_le_bytes());
    assert!(has(item_line::SHIELD_RATING));
    assert!(drawn.nodes.iter().any(|n| matches!(
        n,
        profile::Node::Int { look, value: 40, .. } if *look == name_look::ENCHANTED
    )));
}

/// The raise ends with its time, the look stops, and nothing is left on the item; used again while
/// it runs, it does not stack, the look does not play again, and the time starts over.
/// Behaviour: server.skills.an-effect-on-an-item-ends-and-does-not-stack
#[test]
fn an_effect_on_an_item_does_not_stack_and_ends_with_its_time() {
    let (mut h, b, me) = world();
    let sword = h.arm(b, me, SWORD);
    cast(&mut h, b, BRIEF, sword);
    assert_eq!(most(&h, sword), 153);
    let first = h.server.things.get(sword).expect("it").timed[0].until;
    for _ in 0..3 {
        let from = cast(&mut h, b, BRIEF, sword);
        assert_eq!(fx_played(&h, b, from, sword), 0, "it is still showing");
        assert_eq!(most(&h, sword), 153, "no more than one");
    }
    let t = h.server.things.get(sword).expect("it");
    assert_eq!(t.timed.len(), 1);
    assert!(t.timed[0].until > first, "its time started over");
    // A weaker one changes nothing.
    let until = t.timed[0].until;
    rate(&mut h, me, 10);
    cast(&mut h, b, BRIEF, sword);
    let t = h.server.things.get(sword).expect("it");
    assert_eq!((t.timed.len(), t.timed[0].until), (1, until));
    assert_eq!(most(&h, sword), 153);
    let from = h.bots[b].inbox.len();
    h.run(steps(61.0));
    assert_eq!(most(&h, sword), 92, "over");
    assert!(h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::StopFx(e) if e.sender.id == sword && e.action.0 == FX)
    }));
    assert!(h.server.things.get(sword).expect("it").timed.is_empty());
    let from = cast(&mut h, b, BRIEF, sword);
    assert_eq!(fx_played(&h, b, from, sword), 1, "the first again");
}

/// A skill aimed where it may not go is refused, and nothing is put on anything: at a creature, at
/// a ring, at a weapon for a skill on shields, at another player's item, at a weapon in the pack
/// for a skill on what is in hand; naming its user with nothing fit in hand. A shield in the pack is
/// taken by a skill on the user's items.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-refused
#[test]
fn a_skill_aimed_at_an_item_is_refused_elsewhere() {
    let (mut h, b, me) = world();
    let (o, other) = h.player_in_world("two", "Bran");
    let theirs = h.arm(o, other, SWORD);
    let ring = yalain_world::inventory::make_item(&mut h.server, RING, 1, 0).expect("made");
    let mine = |h: &mut Harness, id: u64, template: u32| {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == me)
            .expect("in the world");
        let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
        p.play.items.push(yalain_world::state::Held {
            id,
            template,
            slot,
            location: 0,
            quantity: 1,
        });
        if let Some(t) = h.server.things.get_mut(id) {
            t.container = Some((me, slot));
        }
    };
    mine(&mut h, ring, RING);
    let sword = yalain_world::inventory::make_item(&mut h.server, SWORD, 1, 0).expect("made");
    mine(&mut h, sword, SWORD);
    let shield = yalain_world::inventory::make_item(&mut h.server, SHIELD, 1, 0).expect("made");
    mine(&mut h, shield, SHIELD);
    let in_hand = h.arm(b, me, SHIELD);
    for (skill, target, why) in [
        (ON_WEAPON, in_hand, refusal::NOT_THIS_ITEM),
        (ON_SHIELD, 0x40EE_A700_0000_0001, refusal::NOT_THIS_ITEM),
        (ON_SHIELD, ring, refusal::NOT_THIS_ITEM),
        (ON_SHIELD, sword, refusal::NOT_THIS_ITEM),
        (ON_SHIELD, other, refusal::NOT_THIS_ITEM),
        (ON_WEAPON, theirs, refusal::NOT_ANOTHERS_ITEM),
        (ON_WEAPON, sword, refusal::NOT_ITEM_IN_PACK),
        (ON_WEAPON, me, refusal::NOT_THIS_ITEM),
    ] {
        let from = cast(&mut h, b, skill, target);
        assert_eq!(refusal_of(&h, b, from), Some(why), "{skill} on {target:#x}");
    }
    for id in [ring, sword, shield, theirs] {
        assert!(h.server.things.get(id).expect("it").timed.is_empty());
    }
    let from = cast(&mut h, b, ON_SHIELD, shield);
    assert_eq!(refusal_of(&h, b, from), None);
    assert_eq!(h.server.things.get(shield).expect("it").timed.len(), 2);
}

/// An item its user no longer holds when the use comes to it takes nothing.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-refused
#[test]
fn an_item_gone_from_its_users_hands_before_the_effect_lands_takes_nothing() {
    let (mut h, b, me) = world();
    let sword = h.arm(b, me, SWORD);
    asked(&mut h, b, BRIEF, sword);
    h.run(steps(0.3));
    h.server.things.get_mut(sword).expect("it").wielded = None;
    h.server.things.get_mut(sword).expect("it").container = None;
    h.run(steps(3.0));
    assert!(h.server.things.get(sword).expect("it").timed.is_empty());
}

/// Used on an item a blow binds, a use on it binds nothing: it is no blow.
/// Behaviour: server.skills.a-skill-aimed-at-an-item-lands-on-it
#[test]
fn a_use_on_an_item_binds_nothing() {
    let (mut h, b, me) = world();
    let sword = h.arm(b, me, SWORD);
    h.server.things.get_mut(sword).expect("it").binds_on_use = true;
    for _ in 0..100 {
        cast(&mut h, b, BRIEF, sword);
    }
    let t = h.server.things.get(sword).expect("it");
    assert!(t.binds_on_use && t.own.bound_to == 0);
}

/// A stack under an effect is apart from one that is not: new pieces take a stack of their own, and
/// moved onto it another stack stays apart, where two plain ones join. The count stays what was given.
/// Behaviour: server.skills.an-effect-on-an-item-ends-and-does-not-stack
#[test]
fn a_stack_under_an_effect_takes_no_plain_pieces() {
    use yalain_world::inventory::{give_stacked, move_graph, Move};
    const GEMS: u32 = 0x4700_7A04;
    let (mut h, b, me) = world();
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("in the world");
    let mut props = Props::default();
    props.set("Item_MaxStackSize", Prop::Int(100));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEMS, 0x1067, props));
    let stacks = |h: &Harness| -> Vec<(u64, u32, i32)> {
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.template == GEMS)
            .map(|i| (i.id, i.quantity, i32::try_from(i.slot).unwrap_or(0)))
            .collect()
    };
    give_stacked(&mut h.server, &mut h.net, world, GEMS, 3);
    give_stacked(&mut h.server, &mut h.net, world, GEMS, 2);
    assert_eq!(stacks(&h).len(), 1, "plain pieces join");
    let stack = stacks(&h)[0].0;
    let until = h.server.game_now + 100.0;
    h.server.things.get_mut(stack).expect("it").timed = vec![yalain_world::things::TimedEffect {
        state: BRIEF_UP,
        spellcraft: 30.0,
        until,
    }];
    give_stacked(&mut h.server, &mut h.net, world, GEMS, 2);
    assert_eq!(
        stacks(&h).iter().map(|x| x.1).collect::<Vec<_>>(),
        [5, 2],
        "new pieces apart"
    );
    let onto = |h: &mut Harness, (item, n, from_slot): (u64, u32, i32), slot: i32| {
        let m = Move {
            item,
            quantity: n,
            from: me,
            from_slot,
            target: me,
            target_slot: slot,
            ..Move::default()
        };
        h.ask(
            b,
            yalain_world::pack::function::REORGANIZE,
            &[move_graph(&m)],
            5,
        );
    };
    h.run(steps(3.0));
    // Each way round: the pack's order changes which slot a move onto a stack names.
    for _ in 0..2 {
        let v = stacks(&h);
        onto(&mut h, v[1], v[0].2);
        h.run(steps(3.0));
        assert_eq!(stacks(&h).len(), 2, "moved onto it, apart");
    }
    assert_eq!(stacks(&h).iter().map(|x| x.1).sum::<u32>(), 7);
    h.server.things.get_mut(stack).expect("it").timed.clear();
    for _ in 0..2 {
        if stacks(&h).len() == 2 {
            let v = stacks(&h);
            onto(&mut h, v[1], v[0].2);
            h.run(steps(3.0));
        }
    }
    assert_eq!(stacks(&h).len(), 1, "the same moves join plain ones");
    assert_eq!(stacks(&h)[0].1, 7);
}
