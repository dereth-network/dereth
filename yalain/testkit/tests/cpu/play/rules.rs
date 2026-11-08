//! The rules of a fight and of experience, as numbers.

use yalain_testkit::invariants::newer;
use yalain_world::rules::{self, Kind, Ratings};
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

/// An attack lands half the time when its accuracy and its target's evasion are equal, whatever
/// they are.
/// Behaviour: server.combat.landing-chance
#[test]
fn an_attack_lands_half_the_time_at_equal_accuracy_and_evasion() {
    for r in [0.0, 8.0, 400.0, 1_234.5] {
        let p = rules::landing_chance(Ratings {
            accuracy: r,
            evasion: r,
        });
        assert!((p - 0.5).abs() < 1e-12, "at {r}: {p}");
    }
}

/// Near the middle, ten points of accuracy over evasion move the chance's log-odds by 0.043 (a
/// little over a point of chance), and the same ten points of evasion take as much away.
/// Behaviour: server.combat.landing-chance
#[test]
fn ten_points_of_accuracy_over_evasion_move_the_log_odds_by_0_043() {
    let logit = |p: f64| dereth_primitives::num::math::log(p / (1.0 - p));
    for base in [-60.0, 0.0, 60.0] {
        let at = |d: f64| {
            rules::landing_chance(Ratings {
                accuracy: 500.0 + base + d,
                evasion: 500.0,
            })
        };
        let step = logit(at(10.0)) - logit(at(0.0));
        assert!((0.042..=0.044).contains(&step), "from {base}: {step}");
        let less = logit(at(0.0))
            - logit(rules::landing_chance(Ratings {
                accuracy: 500.0 + base,
                evasion: 510.0,
            }));
        assert!((less - step).abs() < 1e-9, "evasion mirrors accuracy");
    }
    let middle = rules::landing_chance(Ratings {
        accuracy: 10.0,
        evasion: 0.0,
    }) - 0.5;
    assert!((0.0105..0.011).contains(&middle), "{middle}");
}

/// However far apart the ratings are, an attack lands at least one time in twenty and at most
/// nineteen in twenty.
/// Behaviour: server.combat.landing-chance
#[test]
fn an_attacks_chance_to_land_is_kept_within_5_and_95_percent() {
    let at = |accuracy: f64, evasion: f64| rules::landing_chance(Ratings { accuracy, evasion });
    assert!((at(2_000.0, 0.0) - 0.95).abs() < 1e-12);
    assert!((at(0.0, 2_000.0) - 0.05).abs() < 1e-12);
    // Inside the limits the curve is its own: 600 points over is 0.93, 700 is held at 0.95.
    assert!((at(600.0, 0.0) - 0.9296).abs() < 1e-3, "{}", at(600.0, 0.0));
    assert!((at(700.0, 0.0) - 0.95).abs() < 1e-12);
    assert!((at(0.0, 600.0) - 0.0704).abs() < 1e-3);
    assert!((at(0.0, 700.0) - 0.05).abs() < 1e-12);
    let rule = rules::LANDING;
    assert!((rule.slope, rule.least, rule.most) == (0.0043, 0.05, 0.95));
}

/// A creature's hit on a player at level 1 or 2 does its natural damage in full (2017's had no
/// armor yet), and on anything else the measured share armor lets through by the level it hits.
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
            let o = rules::roll_basic(&mut rng, rules::Kind::Melee, Ratings::default(), bonus);
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

/// A creature's skill is critical at what its class's threshold falls short of 2: none at 2 or more,
/// a fortieth at 1.975, a twentieth at 1.95, all at 1 or less; a creature's missile comes out
/// critical at the chance given among those that land, whatever its kind's measured share.
/// Behaviour: server.combat.creature-critical-by-skill
#[test]
fn a_creatures_critical_chance_is_what_its_skills_threshold_falls_short_of_two() {
    for (threshold, chance) in [
        (2.0, 0.0),
        (2.5, 0.0),
        (1.975, 0.025),
        (1.95, 0.05),
        (1.8, 0.2),
        (1.0, 1.0),
        (-1.0, 1.0),
    ] {
        let got = rules::skill_critical(threshold);
        assert!((got - chance).abs() < 1e-6, "{threshold}: {got}");
    }
    let mut rng = yalain_world::rng::Rng::new(13);
    let mut share = |critical: f64| {
        let (mut landed, mut crits) = (0, 0);
        for _ in 0..4_000 {
            let o = rules::roll_missile(&mut rng, Ratings::default(), false, critical);
            landed += u32::from(o.landed());
            crits += u32::from(o == rules::Outcome::Critical);
        }
        assert!(landed > 1_000, "{landed}");
        (crits, landed)
    };
    assert_eq!(share(0.0).0, 0, "a skill that sets none");
    let (crits, landed) = share(1.0);
    assert_eq!(crits, landed, "a skill that sets all of it");
}

/// A special's roll is critical at the chance it is given, its skill's with what its weapon adds:
/// never at none, and at that share of those that land otherwise.
/// Behaviour: server.specials.critical-by-skill-and-weapon
#[test]
fn a_special_is_critical_at_the_chance_its_skill_and_weapon_give() {
    let mut rng = yalain_world::rng::Rng::new(9);
    let mut share = |bonus: f64| {
        let (mut landed, mut critical) = (0.0, 0.0);
        for _ in 0..80_000 {
            let o = rules::roll_special(&mut rng, Kind::Missile, Ratings::default(), bonus);
            landed += f64::from(u8::from(o.landed()));
            critical += f64::from(u8::from(o == rules::Outcome::Critical));
        }
        assert!(landed > 10_000.0, "{landed}");
        critical / landed
    };
    assert!(share(0.0).abs() < f64::EPSILON);
    assert!((share(0.03) - 0.03).abs() < 0.006);
    assert!((share(0.2) - 0.2).abs() < 0.01, "a class that sets 1.8");
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
        pool: 0,
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

/// A damage pool's points join a hit's damage before its roll, the roll's share applying to them
/// too: the drum's 9 at a variance of a third with 10 points rolls 12 to 19, without a weapon the
/// base's roll takes them likewise. A landed blow takes the pool at its chance, all of it, and an
/// attack's later blows take none once one has; an empty pool draws nothing.
/// Behaviour: server.combat.damage-pool
#[test]
fn a_damage_pool_joins_a_hit_before_its_roll_and_is_taken_once_at_its_chance() {
    let mut rng = yalain_world::rng::Rng::new(5);
    let drum = |pool: i32| rules::Hitter {
        level: 1,
        weapon: Some(rules::WeaponDamage {
            most: 9,
            variance: 0.33,
        }),
        added: 0,
        critical: 0.0,
        pool,
    };
    let rolls: Vec<i32> = (0..2000)
        .map(|_| rules::player_hit(&mut rng, drum(10), 0, false))
        .collect();
    assert!(rolls.iter().all(|d| (12..=19).contains(d)), "{rolls:?}");
    assert!(rolls.contains(&12) && rolls.contains(&19));
    // Without a weapon: the base at level 1 is 6, rolled 0.7 to 1.3.
    let bare = rules::Hitter {
        weapon: None,
        ..drum(10)
    };
    for _ in 0..500 {
        let d = rules::player_hit(&mut rng, bare, 0, false);
        assert!((11..=21).contains(&d), "{d}");
    }
    // Taken at its chance, once.
    let mut taken = 0;
    let tries = 20_000;
    for _ in 0..tries {
        let mut pool = rules::DamagePool {
            points: 40,
            chance: 0.25,
        };
        let first = pool.take(&mut rng);
        assert!(first == 0 || first == 40);
        if first == 40 {
            taken += 1;
            assert_eq!(pool.take(&mut rng), 0, "once an attack");
            assert_eq!(pool.points, 0);
        }
    }
    let share = f64::from(taken) / f64::from(tries);
    assert!((share - 0.25).abs() < 0.015, "{share}");
    let mut sure = rules::DamagePool {
        points: 40,
        chance: 1.0,
    };
    assert_eq!((sure.take(&mut rng), sure.take(&mut rng)), (40, 0));
    // An empty pool, or one no blow may take, leaves the draws as they were.
    let (mut a, mut b) = (rng.clone(), rng.clone());
    for mut pool in [
        rules::DamagePool::default(),
        rules::DamagePool {
            points: 40,
            chance: 0.0,
        },
    ] {
        assert_eq!(pool.take(&mut a), 0);
    }
    assert_eq!(a.unit().to_bits(), b.unit().to_bits());
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
