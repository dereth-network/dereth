//! The rules of a fight and of experience, as numbers.

use yalain_testkit::invariants::newer;
use yalain_world::rules::{self, Direction, Kind};
use yalain_world::xp::kill_award;

/// A solo kill by a killer under level 50 pays the creature's death experience in full from ten
/// levels below the killer to nine above, 15% less a level further below (at least 1%), 20% less a
/// level further above, rounded down.
/// Behaviour: server.experience.solo-kill-award
#[test]
fn a_solo_kill_pays_the_death_experience_weighed_by_the_level_difference() {
    let at = |creature: i32, killer: i32| kill_award(100, creature, killer);
    assert_eq!(at(20, 20), 100);
    assert_eq!(at(10, 20), 100, "ten below: in full");
    assert_eq!(at(29, 20), 100, "nine above: in full");
    assert_eq!(at(9, 20), 85);
    assert_eq!(at(4, 20), 10);
    assert_eq!(at(3, 20), 1, "never under 1%");
    assert_eq!(at(1, 60), 1);
    assert_eq!(at(30, 20), 80);
    assert_eq!(at(31, 20), 60);
    assert_eq!(kill_award(11, 2, 1), 11);
    assert_eq!(kill_award(7, 1, 30), 0, "1% of 7 rounds down to nothing");
}

/// From level 50 what is below a killer pays in full only to four levels below it, then 20% less
/// a level (at least 1%); at level 49 it still pays in full to ten below.
/// Behaviour: server.experience.from-level-50-the-fall-comes-sooner
#[test]
fn from_level_50_a_kill_pays_in_full_only_to_four_levels_below_the_killer() {
    let at = |creature: i32, killer: i32| kill_award(100, creature, killer);
    assert_eq!(at(46, 50), 100, "four below: in full");
    assert_eq!(at(45, 50), 80);
    assert_eq!(at(44, 50), 60);
    assert_eq!(at(43, 50), 40);
    assert_eq!(at(42, 50), 20);
    assert_eq!(at(41, 50), 1, "nine below: never under 1%");
    assert_eq!(at(49, 57), 20, "the same at a higher level");
    assert_eq!(at(41, 49), 100, "at level 49, eight below: in full");
    assert_eq!(at(39, 49), 100, "at level 49, ten below: in full");
    assert_eq!(at(38, 49), 85, "at level 49, eleven below: as before");
}

/// From level 50 what is above a killer falls a level sooner: in full to eight above it, then 20%
/// less a level (at least 1%); at level 49 it still pays in full to nine above.
/// Behaviour: server.experience.from-level-50-the-upper-side-falls-a-level-sooner
#[test]
fn from_level_50_what_is_above_a_killer_falls_a_level_sooner() {
    let at = |creature: i32, killer: i32| kill_award(100, creature, killer);
    assert_eq!(at(58, 50), 100, "eight above: in full");
    assert_eq!(at(59, 50), 80, "nine above");
    assert_eq!(at(60, 50), 60, "ten above");
    assert_eq!(at(63, 50), 1, "thirteen above: never under 1%");
    assert_eq!(at(67, 57), 60, "the same at a higher level");
    assert_eq!(at(58, 49), 100, "at level 49, nine above: in full");
    assert_eq!(at(59, 49), 80, "at level 49, ten above: 80");
}

/// Whether an attack lands follows the measured fits, by direction, kind, level difference and the
/// player's own level: at equal levels a level 1 player's bow lands about half the time and a level
/// 40 player's three times in four; a creature's melee attack lands on a player of any level
/// alike, a little more than half the time at equal levels; a higher attacker lands more often.
/// Behaviour: server.combat.landing-chance
#[test]
fn attacks_land_with_the_measured_chance_for_their_direction_level_difference_and_player_level() {
    let pc =
        |kind, me: i32, it: i32| rules::landing_chance(Direction::PlayerOnCreature, kind, me, it);
    let cp =
        |kind, it: i32, me: i32| rules::landing_chance(Direction::CreatureOnPlayer, kind, it, me);
    let low = pc(Kind::Missile, 1, 1);
    assert!((low - 0.473).abs() < 0.001, "a level 1 bow: {low}");
    let high = pc(Kind::Missile, 40, 40);
    assert!((high - 0.742).abs() < 0.001, "a level 40 bow: {high}");
    assert!(
        (pc(Kind::Magic, 1, 1) - 0.533).abs() < 0.001,
        "a level 1 stave"
    );
    // Below the lowest level 2017 shows for a kind, the player's own term holds there.
    let sword = pc(Kind::Melee, 1, 1);
    assert!((sword - 0.527).abs() < 0.001, "a level 1 sword: {sword}");
    assert!(
        (pc(Kind::Melee, 5, 5) - sword).abs() < 1e-9,
        "melee measured from level 11 up"
    );
    let c = cp(Kind::Melee, 5, 5);
    assert!((c - 0.545).abs() < 0.001, "a creature's melee: {c}");
    assert!(
        (cp(Kind::Melee, 40, 40) - c).abs() < 1e-9,
        "the same on a level 40 player"
    );
    assert!(pc(Kind::Missile, 15, 5) > pc(Kind::Missile, 15, 15));
    assert!(cp(Kind::Melee, 15, 5) > c);
}

/// A creature's hit on a player at level 1 or 2 does its natural damage in full (2017's had no
/// armor yet), and from level 3 the measured share the player's armor lets through.
/// Behaviour: server.combat.creature-damage-through-armor
#[test]
fn a_creatures_hit_does_its_natural_damage_in_full_to_a_player_of_level_one_or_two() {
    assert!((rules::armor_lets_through(1) - 1.0).abs() < f32::EPSILON);
    assert!((rules::armor_lets_through(2) - 1.0).abs() < f32::EPSILON);
    assert!((rules::armor_lets_through(4) - 0.42).abs() < f32::EPSILON);
    assert!((rules::armor_lets_through(8) - 0.26).abs() < f32::EPSILON);
    let mut rng = yalain_world::rng::Rng::new(7);
    let hits: Vec<i32> = (0..200)
        .map(|_| rules::creature_damage(&mut rng, Kind::Melee, 5.0, 0.5, 1, false))
        .collect();
    assert!(hits.iter().all(|d| (3..=8).contains(d)), "{hits:?}");
    assert!(hits.iter().any(|d| *d >= 6), "a digger's 5 x U(0.5, 1.5)");
}

/// A skill's kind of attack is its name's family: a bow's strikes up close are missile attacks, a
/// stave's are magic.
/// Behaviour: server.combat.landing-chance
#[test]
fn an_attack_skills_kind_is_its_names_family() {
    assert_eq!(rules::kind_of_name("Hum_Me_AutoAttack1"), Some(Kind::Melee));
    assert_eq!(
        rules::kind_of_name("Hum_Mi_AutoAttackMelee1"),
        Some(Kind::Missile)
    );
    assert_eq!(rules::kind_of_name("Hum_Ma_AutoAttack1"), Some(Kind::Magic));
    assert_eq!(rules::kind_of_name("Mn_Mi_Strong"), Some(Kind::Missile));
    assert_eq!(rules::kind_of_name("Lifestone_Recall"), None);
}

/// What a creature's skill adds goes into its hit before the player's armor: a level 1 player
/// (no armor) takes a digger's natural 5 plus `Strong`'s 3 in full.
/// Behaviour: server.creatures.skills-add-damage
#[test]
fn a_creatures_skill_adds_its_damage_before_the_players_armor() {
    let mut rng = yalain_world::rng::Rng::new(3);
    let n = rules::Natural {
        damage: 5.0,
        variance: 0.0,
        added: 3.0,
        factor: 1.0,
    };
    assert_eq!(rules::creature_hit(&mut rng, Kind::Melee, n, 1, false), 8);
    assert_eq!(
        rules::creature_hit(&mut rng, Kind::Melee, n, 4, false),
        3,
        "8 x 0.42 at level 4"
    );
    assert_eq!(rules::creature_hit(&mut rng, Kind::Melee, n, 1, true), 12);
}

/// What a weapon adds to the chance of a critical goes on top of the measured one, in the share of
/// the attacks that land: nothing else about the attack moves.
/// Behaviour: server.combat.critical-by-weapon
#[test]
fn a_weapons_critical_bonus_adds_to_the_chance_among_hits_that_land() {
    let mut rng = yalain_world::rng::Rng::new(5);
    let base = rules::fit(rules::Direction::PlayerOnCreature, rules::Kind::Melee).critical;
    let mut share = |bonus: f64| {
        let (mut landed, mut critical) = (0.0, 0.0);
        for _ in 0..80_000 {
            let o = rules::roll_basic(&mut rng, rules::Kind::Melee, 20, 20, bonus);
            landed += f64::from(u8::from(o.landed()));
            critical += f64::from(u8::from(o == rules::Outcome::Critical));
        }
        (critical / landed, landed)
    };
    let (plain, n0) = share(0.0);
    let (with, n1) = share(0.08);
    assert!((plain - base).abs() < 0.006, "{plain}");
    assert!((with - base - 0.08).abs() < 0.006, "{with}");
    assert!((n0 - n1).abs() / n0 < 0.02, "{n0} {n1}");
}

/// A special is critical only by what its weapon adds: never with a weapon that adds nothing, and at
/// that share with one that adds.
/// Behaviour: server.specials.critical-by-weapon-only
#[test]
fn a_special_is_critical_only_by_its_weapons_bonus() {
    let mut rng = yalain_world::rng::Rng::new(9);
    let mut share = |bonus: f64| {
        let (mut landed, mut critical) = (0.0, 0.0);
        for _ in 0..80_000 {
            let o = rules::roll_special(&mut rng, Kind::Missile, 20, 20, bonus);
            landed += f64::from(u8::from(o.landed()));
            critical += f64::from(u8::from(o == rules::Outcome::Critical));
        }
        assert!(landed > 10_000.0, "{landed}");
        critical / landed
    };
    assert!(share(0.0).abs() < f64::EPSILON);
    assert!((share(0.03) - 0.03).abs() < 0.006);
}

/// A player's hit with a weapon is a whole number drawn evenly across the weapon's damage (the
/// starting drum's 9 at a variance of a third: 6 to 9), with what its effects add, less the creature's
/// armor level, at least 1; a critical doubles it.
/// Behaviour: server.combat.player-hits-by-weapon
#[test]
fn a_players_hit_is_its_weapons_damage_less_the_creatures_armor() {
    let mut rng = yalain_world::rng::Rng::new(11);
    let drum = rules::WeaponDamage {
        most: 9,
        variance: 0.34,
    };
    assert_eq!(drum.least(), 5, "9 less 0.34 of it, rounded down");
    let sword = rules::WeaponDamage {
        most: 167,
        variance: 0.25,
    };
    assert_eq!(sword.least(), 125);
    let me = |added: i32| rules::Hitter {
        level: 1,
        weapon: Some(rules::WeaponDamage {
            most: 9,
            variance: 0.33,
        }),
        added,
        critical: 0.0,
    };
    let rolls: Vec<i32> = (0..2000)
        .map(|_| rules::player_hit(&mut rng, me(0), 0, false))
        .collect();
    for d in 6..=9 {
        assert!(rolls.contains(&d), "{d} rolled");
    }
    assert!(rolls.iter().all(|d| (6..=9).contains(d)));
    for _ in 0..200 {
        let d = rules::player_hit(&mut rng, me(5), 2, false);
        assert!(
            (9..=12).contains(&d),
            "+5 from an effect, 2 armor levels: {d}"
        );
        let c = rules::player_hit(&mut rng, me(0), 2, true);
        assert!(
            (8..=14).contains(&c) && c % 2 == 0,
            "doubled after armor: {c}"
        );
        assert_eq!(
            rules::player_hit(&mut rng, me(0), 40, false),
            1,
            "at least 1"
        );
    }
}

/// The invariant checker orders stamps as the client does: the larger of two is newer while they are
/// less than half the circle apart and the smaller from there on, exactly half included.
/// Behaviour: server.replay.the-checker-orders-stamps-as-the-client-does
#[test]
fn the_checker_orders_stamps_as_the_client_does() {
    assert!(newer(8, 7));
    assert!(!newer(7, 7));
    assert!(newer(0, 0xFFFF), "across the wrap");
    assert!(!newer(0xFFFF, 0));
    assert!(newer(0x7FFF, 0), "one short of half");
    assert!(newer(0, 0x8001), "one past half");
    assert!(newer(0, 0x8000), "exactly half: the smaller");
    assert!(!newer(0x8000, 0));
    assert!(newer(0x1234, 0x9234));
    assert!(!newer(0x9234, 0x1234));
}
