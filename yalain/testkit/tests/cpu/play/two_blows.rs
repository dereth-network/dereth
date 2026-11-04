//! Skills that strike twice, a weapon known by its class, the hands: a basic attack with two blows
//! tells a result for each, each blow drawn apart, doing its own hand's weapon's damage and landing
//! at its own time; what is worn is not wielded; what a player wields is known by its class, and
//! says its combat mode and what its basic attacks are. Fixture: the test camp's beetle beside a
//! player, made-up weapon templates of a known damage and made-up skills with the install's hooks.

use auberean_protocol::types::PhysicsStory;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat::{function, next_swing_after, second_blow_seconds, two_blow_seconds};
use yalain_world::skill_use::implement as im;
use yalain_world::skills::{Hook, SkillDef, Usage};
use yalain_world::state::SkillState;

const RIGHT: u32 = 0x4700_7A01;
const LEFT: u32 = 0x4700_7A02;
const HAT: u32 = 0x4700_7A03;
const RIGHT_HAND: u32 = 64;
const OFF_HAND: u32 = 128;

/// A hook that strikes by a hand, as the install's are.
fn blow(flags: u32, anim_hook: u32) -> Hook {
    Hook {
        flags,
        chance: 1.0,
        radius: 2.0,
        anim_hook,
        ..Hook::default()
    }
}

/// A basic attack of the install's shape: walks, and allows `implement` in both hands.
fn basic(id: u32, implement: u32, hooks: Vec<Hook>) -> SkillDef {
    SkillDef {
        id,
        auto_attack: true,
        usage: Some(Usage {
            min_range: -1.0,
            max_range: 1.0,
            implements_right: implement.cast_signed(),
            implements_left: implement.cast_signed(),
            behavior_other: [0x4100_0077; 3],
            hooks,
            ui_flags: 0xCA1,
            ..Usage::default()
        }),
        ..SkillDef::default()
    }
}

/// A player beside the beetle with a skill it holds, two weapons of known damage made and placed
/// in its hands, the beetle full of health and standing still.
fn fighter(skill: SkillDef, implement: u32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    for (template, damage) in [(RIGHT, 100), (LEFT, 30)] {
        let mut p = Props::default();
        p.set("Weapon_Damage", Prop::Int(damage));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(template, 0x1067, p));
        h.server.catalog.implements.insert(template, implement);
    }
    let id = skill.id;
    h.server.skill_table.skills.insert(id, skill);
    let right = hold(&mut h, me, RIGHT, RIGHT_HAND);
    let left = hold(&mut h, me, LEFT, OFF_HAND);
    assert_ne!(right, left);
    let p = player_of(&mut h, me);
    p.play.skills.push(SkillState {
        skill: id,
        granted: 0,
        mask: 0,
        cost: 0,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    let at = p.position;
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(c) = t.creature.as_mut() {
            c.stats.movement.perception = 0.0;
            c.stats.scale = 0.0;
        }
        let mut place = at;
        place.origin[0] += 1.5;
        t.at = Some(place);
        t.max_health = 10_000_000;
        t.health = 10_000_000;
    }
    (h, b, me, beetle)
}

fn player_of(h: &mut Harness, me: u64) -> &mut yalain_world::world::Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

/// A thing made of a template, placed straight at a place on the player.
fn hold(h: &mut Harness, me: u64, template: u32, location: u32) -> u64 {
    let id = yalain_world::inventory::make_item(&mut h.server, template, 1, 0).expect("made");
    let p = player_of(h, me);
    let slot = u32::try_from(p.play.items.len()).unwrap_or(0);
    p.play.items.push(yalain_world::state::Held {
        id,
        template,
        slot,
        location,
        quantity: 1,
    });
    id
}

fn attack(h: &mut Harness, b: usize, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::START_ATTACK, &[Arg::LongInt(target)]),
    );
    from
}

fn stories(h: &Harness, b: usize, from: usize, of: u64) -> Vec<PhysicsStory> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == of => Some(e.action.clone()),
            _ => None,
        })
        .collect()
}

/// A skill of two blows, the hooks' flags in the order the skill names them.
fn two_blows(first: u32, second: u32) -> (Harness, usize, u64, u64) {
    let skill = basic(816, im::HANDBLADE, vec![blow(first, 1), blow(second, 2)]);
    fighter(skill, im::HANDBLADE)
}

/// A story of a skill that strikes twice has its opening hook and a result hook for each blow: the
/// second with the bit of a hook after the first, each with its index, its animation hook, and its
/// own health change when it hit.
/// Behaviour: server.weapons.two-blows
#[test]
fn a_skill_that_strikes_twice_tells_a_result_for_each_blow() {
    let (mut h, b, me, beetle) = two_blows(0x2002, 0x1002);
    let from = attack(&mut h, b, beetle);
    h.run(400);
    let told = stories(&h, b, from, me);
    assert!(told.len() >= 4, "{} swings", told.len());
    for s in &told {
        assert_eq!(s.skill, 816);
        assert_eq!(s.bits, 0x36);
        assert_eq!(s.hooks.len(), 3, "{:?}", s.hooks);
        assert_eq!(s.hooks[0].bits, 0x2);
        assert!([0xB81, 0x381].contains(&s.hooks[1].bits));
        assert!([0xF81, 0x781].contains(&s.hooks[2].bits));
        for (i, k) in s.hooks[1..].iter().enumerate() {
            assert_eq!(k.target, beetle);
            assert_eq!(k.numbers[1], i32::try_from(i + 1).unwrap());
            assert_eq!(k.numbers[2], i32::try_from(i).unwrap());
            assert_eq!(k.bits & 0x800 != 0, k.numbers[3] < 0);
        }
    }
}

/// Each blow is drawn apart from the other: one may hit as the other misses.
/// Behaviour: server.weapons.two-blows
#[test]
fn the_two_blows_are_drawn_apart() {
    let (mut h, b, me, beetle) = two_blows(0x2002, 0x1002);
    let from = attack(&mut h, b, beetle);
    h.run(8000);
    let told = stories(&h, b, from, me);
    let apart = told
        .iter()
        .filter(|s| (s.hooks[1].numbers[3] < 0) != (s.hooks[2].numbers[3] < 0))
        .count();
    let both = told
        .iter()
        .filter(|s| s.hooks[1].numbers[3] < 0 && s.hooks[2].numbers[3] < 0)
        .count();
    assert!(apart > 0, "of {} swings, none differed", told.len());
    assert!(both > 0, "of {} swings, none hit twice", told.len());
}

/// A blow does the damage of the weapon in the hand its hook names, whichever blow it is: a right
/// hand's first and a left hand's second, and the other way about.
/// Behaviour: server.weapons.two-blows
#[test]
fn each_blow_does_its_own_hands_weapons_damage() {
    for (first, second, first_most, second_most) in
        [(0x2002, 0x1002, 100, 30), (0x1002, 0x2002, 30, 100)]
    {
        let (mut h, b, me, beetle) = two_blows(first, second);
        let from = attack(&mut h, b, beetle);
        h.run(3000);
        let told = stories(&h, b, from, me);
        // A plain hit of a weapon with no variance: the same each time, a critical doubled.
        let plain = |i: usize| -> Vec<i32> {
            let mut v: Vec<i32> = told
                .iter()
                .filter(|s| s.hooks[i].numbers[0] == 0x4000_0008)
                .map(|s| -s.hooks[i].numbers[3])
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let (a, b) = (plain(1), plain(2));
        assert_eq!((a.len(), b.len()), (1, 1), "{a:?} {b:?}");
        assert_eq!(a[0] - b[0], first_most - second_most, "{a:?} {b:?}");
    }
}

/// A basic attack's hook that adds by rating adds at the player's rating in the skill, on top of the
/// weapon's damage, a critical doubling it with the rest.
/// Behaviour: server.weapons.hook-adds-by-rating
#[test]
fn a_basic_attacks_hook_adds_by_the_players_rating() {
    let plain_at = |points: Vec<(f32, f32)>, granted: u32| -> (i32, i32) {
        let mut one = blow(0x2002, 1);
        one.add_by_rating = points;
        let skill = basic(816, im::HANDBLADE, vec![one, blow(0x1002, 2)]);
        let (mut h, b, me, beetle) = fighter(skill, im::HANDBLADE);
        let p = player_of(&mut h, me);
        p.play.skills.last_mut().expect("the skill").granted = granted;
        h.server
            .skill_table
            .skills
            .get_mut(&816)
            .expect("the skill")
            .cap = 50;
        let from = attack(&mut h, b, beetle);
        h.run(6000);
        let told = stories(&h, b, from, me);
        let of = |code: i32| {
            told.iter()
                .find(|s| s.hooks[1].numbers[0] == code)
                .map_or(0, |s| -s.hooks[1].numbers[3])
        };
        (of(0x4000_0008), of(0x4000_0009))
    };
    let (none, none_crit) = plain_at(Vec::new(), 30);
    let (rated, rated_crit) = plain_at(vec![(1.0, 7.0), (50.0, 100.0), (100.0, 200.0)], 30);
    // 7 + 29 / 49 of 93: 62 by the line up to the rating of 50.
    assert_eq!(rated - none, 62, "{rated} {none}");
    assert_eq!(rated_crit - none_crit, 2 * 62, "doubled by a critical");
}

/// The target is hurt by each blow as that blow lands, the first at its skill's time and the second
/// at its own, over the time scale.
/// Behaviour: server.weapons.two-blows
#[test]
fn each_blow_hurts_the_target_as_it_lands() {
    let (mut h, b, me, beetle) = two_blows(0x2002, 0x1002);
    let full = 10_000_000;
    let from = attack(&mut h, b, beetle);
    assert!(
        h.run_until(4000, |h| stories(h, b, from, me)
            .iter()
            .any(|s| s.hooks[1].numbers[3] < 0 && s.hooks[2].numbers[3] < 0)),
        "a swing with two hits"
    );
    let told = h.now.0;
    let s = stories(&h, b, from, me).pop().expect("its story");
    let scale = f64::from(s.attacker_behavior.time_scale);
    let health = |h: &Harness| h.server.things.get(beetle).map_or(0, |t| t.health);
    let at = health(&h);
    assert!(at <= full);
    assert!(h.run_until(400, |h| health(h) < at), "the first lands");
    let first = h.now.0 - told;
    let lands = yalain_world::combat::hit_seconds(816, Some(im::HANDBLADE)) / scale;
    assert!(
        (first - lands).abs() < 0.12,
        "first {first:.3}, not {lands:.3}"
    );
    assert_eq!(health(&h), at + s.hooks[1].numbers[3], "only the first");
    let after = health(&h);
    assert!(h.run_until(400, |h| health(h) < after), "the second lands");
    let second = h.now.0 - told;
    let lands = second_blow_seconds(816) / scale;
    assert!(
        (second - lands).abs() < 0.12,
        "second {second:.3}, not {lands:.3}"
    );
    assert_eq!(
        health(&h),
        after + s.hooks[2].numbers[3],
        "the second's own"
    );
}

/// The next swing of a skill that strikes twice comes after its own period, over its time scale.
/// Behaviour: server.weapons.two-blows
#[test]
fn a_skill_that_strikes_twice_swings_at_its_own_pace() {
    let (mut h, b, me, beetle) = two_blows(0x2002, 0x1002);
    let from = attack(&mut h, b, beetle);
    let mut told = Vec::new();
    for _ in 0..1200 {
        h.run(1);
        let n = stories(&h, b, from, me).len();
        if n > told.len() {
            told.push(h.now.0);
        }
    }
    let scales: Vec<f32> = stories(&h, b, from, me)
        .iter()
        .map(|s| s.attacker_behavior.time_scale)
        .collect();
    assert!(told.len() >= 3);
    for (i, w) in told.windows(2).enumerate() {
        let due = next_swing_after(two_blow_seconds(816), scales[i]);
        let gap = w[1] - w[0];
        assert!(gap > due - 0.12 && gap < due + 0.12, "{gap} not {due}");
    }
}

/// A skill that strikes twice goes at its player's pace, and so does a skill that throws twice.
/// Behaviour: server.weapons.attack-speed-effects
#[test]
fn skills_that_strike_or_throw_twice_go_at_their_players_pace() {
    let fixtures = [
        (two_blows(0x2002, 0x1002), two_blow_seconds(816)),
        (two_throws(), yalain_world::combat::two_throws(1884).1),
    ];
    for ((mut h, b, me, beetle), period) in fixtures {
        super::attack_speed::speeded(&mut h, 27, 1.7, true);
        let from = attack(&mut h, b, beetle);
        let mut told = Vec::new();
        for _ in 0..600 {
            h.step();
            let swings = stories(&h, b, from, me);
            if swings.len() > told.len() {
                let s = swings.last().expect("told");
                told.push((h.now.0, s.attacker_behavior.time_scale));
            }
        }
        assert!(told.len() > 2, "{} swings", told.len());
        for (_, ts) in &told {
            assert!((1.72..=2.12).contains(ts), "{ts}");
        }
        for w in told.windows(2) {
            let (gap, due) = (w[1].0 - w[0].0, next_swing_after(period, w[0].1));
            assert!(
                (gap - due).abs() <= 0.052,
                "{gap:.3} where {due:.3} was due"
            );
        }
    }
}

/// What a player wears is not what it wields: a hat listed first leaves the bow its stand and its
/// shots, and the sword its damage.
/// Behaviour: server.weapons.worn-is-not-wielded
#[test]
fn what_is_worn_does_not_take_the_place_of_what_is_wielded() {
    const BOW: u32 = 0x4700_051B;
    for order in [[(HAT, 1), (BOW, 192)], [(BOW, 192), (HAT, 1)]] {
        let (mut h, b, me, beetle) = fighter(
            basic(816, im::HANDBLADE, vec![blow(0x2002, 1)]),
            im::POLEARM,
        );
        player_of(&mut h, me).play.items.clear();
        h.server
            .catalog
            .insert(yalain_testkit::test_template(HAT, 0x200, Props::default()));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(BOW, 0x1067, Props::default()));
        for (template, location) in order {
            hold(&mut h, me, template, location);
        }
        assert_eq!(
            yalain_world::skill_use::player_mode(&h.server, me),
            0x4100_0010,
            "the bow's mode"
        );
        if let Some(t) = h.server.things.get_mut(beetle) {
            if let Some(mut place) = t.at {
                place.origin[0] += 5.0;
                t.at = Some(place);
            }
        }
        let from = attack(&mut h, b, beetle);
        h.run(120);
        let swings = stories(&h, b, from, me);
        assert!(!swings.is_empty());
        for s in &swings {
            assert!(
                (199..=203).contains(&s.skill),
                "a bow's shot, not {}",
                s.skill
            );
        }
    }
}

/// A sword's damage is the sword's with a hat on.
/// Behaviour: server.weapons.worn-is-not-wielded
#[test]
fn a_swing_does_its_weapons_damage_whatever_is_worn_first() {
    let (mut h, b, me, beetle) = fighter(basic(816, im::SWORD, vec![blow(0x2002, 1)]), im::SWORD);
    let p = player_of(&mut h, me);
    let held = std::mem::take(&mut p.play.items);
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HAT, 0x200, Props::default()));
    hold(&mut h, me, HAT, 1);
    let p = player_of(&mut h, me);
    let right = held
        .iter()
        .find(|i| i.template == RIGHT)
        .copied()
        .expect("the sword");
    p.play.items.push(right);
    let me_now = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world");
    let special = yalain_world::combat::hitter(&h.server, me_now);
    assert_eq!(special.weapon.map(|w| w.most), Some(100), "a special's too");
    let from = attack(&mut h, b, beetle);
    h.run(2000);
    let told = stories(&h, b, from, me);
    let hits: Vec<i32> = told
        .iter()
        .filter(|s| s.hooks[1].numbers[0] == 0x4000_0008)
        .map(|s| -s.hooks[1].numbers[3])
        .collect();
    assert!(!hits.is_empty());
    assert!(
        hits.iter().any(|d| *d > 50),
        "the weapon's 100, not a bare fighter's: {hits:?}"
    );
}

/// A swing rolls the damage an aura holds on its weapon, not the weapon's own.
/// Behaviour: server.auras.damage-held-on-a-weapon
#[test]
fn a_swing_does_the_damage_an_aura_holds_on_its_weapon() {
    use yalain_world::effect_defs::{aura_target, Aura, EffectDef, Kind, Scale};
    const DAMAGE: u32 = 0x6F00_7791;
    const AURA: u32 = 0x6F00_7792;
    let (mut h, b, me, beetle) = fighter(basic(816, im::SWORD, vec![blow(0x2002, 1)]), im::SWORD);
    let p = player_of(&mut h, me);
    p.play.items.retain(|i| i.template == RIGHT);
    let right = p.play.items[0].id;
    h.server.things.get_mut(right).expect("the sword").wielded = Some((me, RIGHT_HAND));
    let like = h
        .server
        .effect_table
        .get(yalain_testkit::ESPIRIT_PULSE)
        .cloned()
        .expect("a pulse");
    let adds = EffectDef {
        state: DAMAGE,
        kind: Kind::Numeric {
            flags: 0,
            stats: vec![yalain_world::made::stat::DAMAGE],
            magnitude: Scale(vec![(1.0, 900.0), (50.0, 900.0)]),
        },
        ..like.clone()
    };
    let aura = EffectDef {
        state: AURA,
        kind: Kind::Aura(Aura {
            pulsed: vec![DAMAGE],
            targets: aura_target::WEAPON,
            range: 300.0,
        }),
        ..like
    };
    h.server.effect_table.effects.insert(DAMAGE, adds);
    h.server.effect_table.effects.insert(AURA, aura);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let clock = h.clock();
    yalain_world::effects::aura_on(&mut h.server, &mut h.net, world, AURA, 0, 50.0, clock);
    let from = attack(&mut h, b, beetle);
    h.run(2000);
    let hits: Vec<i32> = stories(&h, b, from, me)
        .iter()
        .filter(|s| s.hooks[1].numbers[0] == 0x4000_0008)
        .map(|s| -s.hooks[1].numbers[3])
        .collect();
    assert!(!hits.is_empty());
    assert!(
        hits.iter().all(|d| *d > 500),
        "1,000 less the beetle's armor: {hits:?}"
    );
}

/// A weapon known by its class fights as its kind: a bow that is no starting weapon stands and
/// shoots. A weapon of no known class is held as it was: a sword's run with the plain swings.
/// Behaviour: server.weapons.implement-by-class
#[test]
fn a_weapon_known_by_its_class_fights_as_its_kind() {
    let (mut h, b, me, beetle) = fighter(basic(816, im::SWORD, vec![blow(0x2002, 1)]), im::BOW);
    let p = player_of(&mut h, me);
    p.play.items.retain(|i| i.template == RIGHT);
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(mut place) = t.at {
            place.origin[0] += 5.0;
            t.at = Some(place);
        }
    }
    let from = attack(&mut h, b, beetle);
    h.run(120);
    let swings = stories(&h, b, from, me);
    assert!(!swings.is_empty());
    for s in &swings {
        assert!(
            (199..=203).contains(&s.skill),
            "a bow's shot, not {}",
            s.skill
        );
    }
    let (mut h, b, me, beetle) = fighter(basic(816, im::SWORD, vec![blow(0x2002, 1)]), im::BOW);
    h.server.catalog.implements.clear();
    let p = player_of(&mut h, me);
    p.play.items.retain(|i| i.template == RIGHT);
    let from = attack(&mut h, b, beetle);
    h.run(120);
    let swings = stories(&h, b, from, me);
    assert!(!swings.is_empty());
    for s in &swings {
        assert!(
            (122..=126).contains(&s.skill),
            "a plain swing, not {}",
            s.skill
        );
    }
}

/// A skill asks of both hands: with a shield in the left hand a skill that wants a second weapon
/// is not among the player's basic attacks, and with the weapon it wants it is.
/// Behaviour: server.weapons.implement-by-class
#[test]
fn a_basic_attack_asks_of_both_hands() {
    let (mut h, b, me, beetle) = fighter(
        basic(816, im::HANDBLADE, vec![blow(0x2002, 1), blow(0x1002, 2)]),
        im::HANDBLADE,
    );
    h.server.catalog.implements.insert(LEFT, im::SHIELD);
    let from = attack(&mut h, b, beetle);
    h.run(240);
    let swings = stories(&h, b, from, me);
    assert!(!swings.is_empty());
    assert!(
        swings.iter().all(|s| s.skill != 816),
        "the other hand is a shield"
    );
    let (mut h, b, me, beetle) = fighter(
        basic(816, im::HANDBLADE, vec![blow(0x2002, 1), blow(0x1002, 2)]),
        im::HANDBLADE,
    );
    let from = attack(&mut h, b, beetle);
    h.run(240);
    let swings = stories(&h, b, from, me);
    assert!(!swings.is_empty());
    assert!(swings.iter().all(|s| s.skill == 816), "both hands fit");
}

/// A player's combat mode is what it wields: handblades their own, a polearm its own, two
/// one-handed weapons together, one with a shield, one alone; a bow, a spear and a hammer one, a
/// stave and a drum another; alchemy and cestas theirs.
/// Behaviour: server.weapons.mode-by-implement
#[test]
fn a_players_combat_mode_comes_from_what_it_wields() {
    use yalain_world::skill_use::mode_of;
    for (right, left, mode) in [
        (im::HANDBLADE, Some(im::HANDBLADE), 0x4100_000F),
        (im::POLEARM, None, 0x4100_000C),
        (im::SWORD, Some(im::AXE), 0x4100_000E),
        (im::DAGGER, Some(im::SHIELD), 0x4100_000B),
        (im::SCYTHE, None, 0x4100_000A),
        (im::BOW, None, 0x4100_0010),
        (im::SPEAR, Some(im::SHIELD), 0x4100_0010),
        (im::HAMMER, None, 0x4100_0010),
        (im::STAVE, None, 0x4100_0011),
        (im::DRUM, None, 0x4100_0011),
        (im::ALCHEMY, None, 0x4100_0015),
        (im::CESTAS, None, 0x4100_0015),
        (im::FLAIL, None, 0x4100_0013),
    ] {
        assert_eq!(mode_of(Some(right), left), mode, "{right:#x} {left:?}");
    }
    assert_eq!(mode_of(None, None), 0x4100_000A, "empty hands");
}

/// A skill that throws from both hands: stands, its two hooks throwing, the left's first.
fn two_throws() -> (Harness, usize, u64, u64) {
    let throw = |flags: u32, anim_hook: u32| Hook {
        missile: 0x4700_00BE,
        missile_speed: 35.0,
        ..blow(flags, anim_hook)
    };
    let mut skill = basic(1884, im::ORB, vec![throw(0x1184, 1), throw(0x2184, 2)]);
    if let Some(u) = skill.usage.as_mut() {
        u.ui_flags = 0x14A0;
        u.max_range = 40.0;
    }
    fighter(skill, im::ORB)
}

/// A skill that throws twice names each hand's weapon, and its target tells each throw as it
/// arrives: the first with its animation hook 1, the second as a hook after the first with its
/// animation hook 2, each with its own weapon's damage, drawn apart.
/// Behaviour: server.weapons.two-throws
#[test]
fn a_skill_that_throws_twice_names_each_hands_weapon_and_tells_each_arrival() {
    let (mut h, b, me, beetle) = two_throws();
    let (right, left) = {
        let p = player_of(&mut h, me);
        let id = |t: u32| p.play.items.iter().find(|i| i.template == t).unwrap().id;
        (id(RIGHT), id(LEFT))
    };
    let from = attack(&mut h, b, beetle);
    h.run(4000);
    let told = stories(&h, b, from, me);
    assert!(told.len() >= 10, "{} swings", told.len());
    for s in &told {
        assert_eq!(s.skill, 1884);
        assert_eq!(s.hooks.len(), 3);
        assert_eq!(
            (
                s.hooks[1].bits,
                s.hooks[1].weapon,
                s.hooks[2].bits,
                s.hooks[2].weapon
            ),
            (0x323, left, 0x723, right)
        );
    }
    let arrivals = stories(&h, b, from, beetle);
    let of = |index: i32| -> Vec<&auberean_protocol::types::StoryHook> {
        arrivals
            .iter()
            .filter(|s| s.attacker == me)
            .flat_map(|s| s.hooks.iter())
            .filter(|k| k.numbers[2] == index)
            .collect()
    };
    let (first, second) = (of(0), of(1));
    assert!(
        first.len() >= 10 && second.len() >= 10,
        "{} {}",
        first.len(),
        second.len()
    );
    assert!(first
        .iter()
        .all(|k| k.bits & 0x400 == 0 && k.numbers[1] == 1));
    assert!(second
        .iter()
        .all(|k| k.bits & 0x400 != 0 && k.numbers[1] == 2));
    let plain = |v: &[&auberean_protocol::types::StoryHook]| -> Vec<i32> {
        let mut d: Vec<i32> = v
            .iter()
            .filter(|k| k.numbers[0] == 0x4000_0008)
            .map(|k| -k.numbers[3])
            .collect();
        d.sort_unstable();
        d.dedup();
        d
    };
    let (l, r) = (plain(&first), plain(&second));
    assert_eq!((l.len(), r.len()), (1, 1), "{l:?} {r:?}");
    assert_eq!(r[0] - l[0], 70, "the right hand's weapon does 70 more");
    let apart = first
        .iter()
        .zip(&second)
        .filter(|(a, b)| a.numbers[0] != b.numbers[0])
        .count();
    assert!(apart > 0, "drawn apart");
}

/// The second throw arrives after the first by its own windup, and the swings come at the skill's
/// own pace.
/// Behaviour: server.weapons.two-throws
#[test]
fn the_second_throw_arrives_after_the_first_and_swings_come_at_their_own_pace() {
    use yalain_world::combat::two_throws as measured;
    let (mut h, b, me, beetle) = two_throws();
    let from = attack(&mut h, b, beetle);
    let arrival = |h: &Harness, index: i32| {
        stories(h, b, from, beetle)
            .iter()
            .filter(|s| s.attacker == me)
            .any(|s| s.hooks.iter().any(|k| k.numbers[2] == index))
    };
    assert!(h.run_until(1000, |h| arrival(h, 0)), "the first arrives");
    let first = h.now.0;
    let told = stories(&h, b, from, me);
    let scale = f64::from(told[0].attacker_behavior.time_scale);
    assert!(h.run_until(400, |h| arrival(h, 1)), "the second arrives");
    let ([w0, w1], period) = measured(1884);
    let gap = h.now.0 - first;
    assert!(
        (gap - (w1 - w0) / scale).abs() < 0.12,
        "{gap:.3}, not {:.3}",
        (w1 - w0) / scale
    );
    assert!((period - 1.951).abs() < 1e-9);
}

/// A swing comes as soon as it is due, not at the world's next step: each gap is what its period
/// and time scale give, within the harness's own step.
/// Behaviour: server.weapons.swing-when-due
#[test]
fn a_swing_comes_when_it_is_due_not_at_the_worlds_next_step() {
    let (mut h, b, me, beetle) = two_blows(0x2002, 0x1002);
    let from = attack(&mut h, b, beetle);
    let mut at = Vec::new();
    for _ in 0..1500 {
        h.step();
        let told = stories(&h, b, from, me);
        if told.len() > at.len() {
            let scale = told.last().expect("told").attacker_behavior.time_scale;
            at.push((h.now.0, scale));
        }
    }
    assert!(at.len() > 10, "{} swings", at.len());
    for pair in at.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        let due = next_swing_after(two_blow_seconds(816), pair[0].1);
        assert!(
            (gap - due).abs() <= 0.051 + 0.001,
            "a gap of {gap:.3} where {due:.3} was due"
        );
    }
}
