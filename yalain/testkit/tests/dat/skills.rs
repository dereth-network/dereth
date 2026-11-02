//! The skills' definitions as the server reads them from the September 2013 data: how each used
//! skill is used.

use yalain_testkit::authored::Portal;
use yalain_world::missiles::attack_skill;
use yalain_world::rules::Kind;

/// The starting skills of a human read with their reach, waits, vigor, implements and hooks as the
/// data holds them: Deadeye shoots a missile from 2 to 80 meters with a bow, every 2 seconds, for
/// 32 vigor; Mind Lash and Riposte strike within a meter; Lifestone Recall powers up for 10
/// seconds and waits 30 to be used again.
/// Behaviour: server.skills.used-skills-read-from-their-definitions
#[test]
fn the_starting_skills_read_with_how_they_are_used() {
    let t = Portal::open().skills();
    let used = t.skills.values().filter(|d| d.usage.is_some()).count();
    assert!(used > 1_000, "most skills are used ones: {used}");

    let deadeye = t.skills[&207].usage.clone().expect("Deadeye is used");
    assert_eq!((deadeye.min_range, deadeye.max_range), (2.0, 80.0));
    assert_eq!(deadeye.recovery, 2.0);
    assert_eq!(deadeye.vigor_add, -32.0);
    assert_eq!(deadeye.implements_right, 4, "a bow");
    let shot = deadeye
        .hooks
        .iter()
        .find(|h| h.missile != 0)
        .expect("its missile");
    assert_eq!((shot.missile, shot.missile_speed), (0x4700_00BE, 35.0));
    assert_eq!((shot.add, shot.mult), (5.0, 1.0));
    assert_eq!(deadeye.target_effects, vec![0x7000_0725]);

    let lash = t.skills[&96].usage.clone().expect("Mind Lash is used");
    assert_eq!((lash.max_range, lash.recovery), (1.0, 5.0));
    let riposte = t.skills[&176].usage.clone().expect("Riposte is used");
    assert_eq!((riposte.max_range, riposte.recovery), (1.0, 1.0));

    let recall = t.skills[&835].usage.clone().expect("the recall is used");
    assert_eq!((recall.powerup, recall.reset), (10.0, 30.0));
    assert_eq!(recall.valid_targets, 1, "only its user");
}

/// A creature's attacks are its skills' definitions: `Strong` strikes with its own behavior and
/// adds 3 damage at rating 1 to 75 at 1,000 (`Stronger` 7), and its use bars it for a while; the
/// harming missile throws a missile, the plain bolt of magic is a magic attack that throws one; a
/// heal a creature uses on itself is no attack, nor is a rating.
/// Behaviour: server.creatures.attacks-from-skill-definitions
#[test]
fn creatures_attacks_read_from_their_skills_definitions() {
    let t = Portal::open().skills();
    let strong = attack_skill(&t, 1846, 1).expect("Strong is an attack");
    assert_eq!(strong.behavior, 0x4100_0077);
    assert_eq!(strong.kind, Kind::Melee);
    assert!(!strong.missile);
    assert!((strong.added - 3.0).abs() < 1e-3, "{}", strong.added);
    assert!(strong.barred_after);
    let top = attack_skill(&t, 1846, 1000).expect("Strong").added;
    assert!((top - 75.0).abs() < 1e-3, "{top}");
    let stronger = attack_skill(&t, 1847, 1).expect("Stronger").added;
    assert!((stronger - 7.0).abs() < 1e-3, "{stronger}");
    let harm = attack_skill(&t, 1860, 5).expect("the harming missile");
    assert!(harm.missile);
    assert_eq!(harm.kind, Kind::Missile);
    let bolt = attack_skill(&t, 1840, 1).expect("the plain bolt");
    assert!(bolt.missile);
    assert_eq!(bolt.kind, Kind::Magic);
    assert!(attack_skill(&t, 1818, 50).is_none(), "a heal on itself");
    assert!(attack_skill(&t, 2, 0).is_none(), "a rating");
}

/// The two summoning skills are used as the recall is, at half the user's vigor: Summon Lifestone
/// and Summon Portal power up for 10 seconds, only on their user, and name the effects that open
/// their portals; the portal's template is one that is not walked into.
/// Behaviour: server.skills.summoning-skills-read
#[test]
fn the_summoning_skills_read_as_recalls_at_half_vigor() {
    let p = Portal::open();
    let t = p.skills();
    for (skill, effect) in [(871, 0x7000_0728), (872, 0x7000_0729)] {
        let u = t.skills[&skill].usage.clone().expect("a summoning is used");
        assert_eq!(u.powerup, 10.0, "{skill}");
        assert_eq!(u.powerup_behavior, 0x4100_00BF, "{skill}");
        assert_eq!(u.vigor_mult, 0.5, "{skill}");
        assert_eq!(u.valid_targets, 1, "{skill}: only its user");
        assert_eq!(u.target_effects, vec![effect], "{skill}");
    }
    let portal = p.template(yalain_world::usable::portals::TEMPLATE);
    assert_eq!(portal.description.package, 0xF12);
}

/// Portal Recall, the cities' recalls and Allegiance Recall are used as the lifestone's recall is
/// (a 10 s power-up, only on their user, the whole of its vigor, the wait every skill shares of
/// 30 s): Portal Recall names its recall as the effect on its target, the others put theirs on
/// their user, the cities' with their protection.
/// Behaviour: server.skills.recall-skills-read
#[test]
fn the_recall_skills_read_as_the_lifestones() {
    let p = Portal::open();
    let t = p.skills();
    for (skill, target, user) in [
        (870, vec![0x7000_0727], vec![]),
        (925, vec![], vec![0x7000_1FC0, 0x7000_2014]),
        (926, vec![], vec![0x7000_1FC1, 0x7000_2014]),
        (927, vec![], vec![0x7000_1FC2, 0x7000_2014]),
        (1045, vec![], vec![0x6F00_0222, 0x7000_2014]),
        (999, vec![], vec![0x7000_1EC5]),
    ] {
        let u = t.skills[&skill].usage.clone().expect("a recall is used");
        assert_eq!(
            (
                u.powerup,
                u.powerup_behavior,
                u.vigor_mult,
                u.valid_targets,
                u.reset
            ),
            (10.0, 0x4100_00BF, 1.0, 1, 30.0),
            "{skill}"
        );
        assert_eq!(
            (u.target_effects, u.user_effects),
            (target, user),
            "{skill}"
        );
    }
}
