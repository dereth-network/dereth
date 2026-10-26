//! The Tank sample's following, fellows, summoning and locks against VTank: follow routes and
//! following around corners, helping fellows and a Healer's Heart, running a fellowship on tells,
//! the Tank's own fellow over the clients on one machine (in the manner of VI2's fellows), summoning
//! pets, picking a locked door's lock, and a wand's own restoring spell, each run from
//! `dereth/plugins/tank` in a scripted world.

use dereth_plugin_host::PluginFiles;
use dereth_primitives::ObjectId;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    read_package, ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER, START_EW, START_NS,
};

use super::tank::{held, logged, met, monster, sent_count, tank_in, text, wand, ALWAYS, TANK};
use super::{samples, SharedFiles};

dereth_testkit::scenarios! {
    scenario_the_tank_follows_its_leader_straight_or_around_corners_by_a_follow_route => the_tank_follows_its_leader_straight_or_around_corners_by_a_follow_route ["plugins.samples.the-tank-follows-its-leader-straight-or-around-corners-by-a-follow-route"],
    scenario_the_tank_heals_restams_and_gives_mana_to_fellows_within_reach_and_heals_with_a_healers_heart => the_tank_heals_restams_and_gives_mana_to_fellows_within_reach_and_heals_with_a_healers_heart ["plugins.samples.the-tank-heals-restams-and-gives-mana-to-fellows-within-reach-and-heals-with-a-healers-heart"],
    scenario_the_tank_recruits_on_xp_tells_and_answers_vtanks_fellowship_commands => the_tank_recruits_on_xp_tells_and_answers_vtanks_fellowship_commands ["plugins.samples.the-tank-recruits-on-xp-tells-and-answers-vtanks-fellowship-commands"],
    scenario_tanks_in_one_fellow_on_a_machine_heal_each_other_and_share_their_debuffs => tanks_in_one_fellow_on_a_machine_heal_each_other_and_share_their_debuffs ["plugins.samples.tanks-in-one-fellow-on-a-machine-heal-each-other-and-share-their-debuffs"],
    scenario_the_tank_summons_a_pet_of_its_mastery_and_the_monsters_element_once_enough_gather => the_tank_summons_a_pet_of_its_mastery_and_the_monsters_element_once_enough_gather ["plugins.samples.the-tank-summons-a-pet-of-its-mastery-and-the-monsters-element-once-enough-gather"],
    scenario_the_tank_picks_a_locked_doors_lock_when_its_skill_beats_it => the_tank_picks_a_locked_doors_lock_when_its_skill_beats_it ["plugins.samples.the-tank-picks-a-locked-doors-lock-when-its-skill-beats-it"],
    scenario_the_tank_recharges_with_its_wands_own_spell_keeping_the_wand_in_hand => the_tank_recharges_with_its_wands_own_spell_keeping_the_wand_in_hand ["plugins.samples.the-tank-recharges-with-its-wands-own-spell-keeping-the-wand-in-hand"],
}

// How an item is used: where it must be (the low half of its use flags) and what it may be used
// on (the high half), as the game's own flags say it.
const SELF: u32 = 0x02;
const WIELDED: u32 = 0x04;
const CONTAINED: u32 = 0x08;
const REMOTE: u32 = 0x20;
const NEVER_WALK: u32 = 0x40;

/// An item's use flags: used where `source` says, on what `target` says.
const fn used(source: u32, target: u32) -> u32 {
    source | (target << 16)
}

/// The character the followers and helpers of these scenarios follow and help.
const BOB: u32 = 0x5000_0002;

/// A character of the scripted world, `east` and `north` metres from where the Tank's starts.
fn character(id: u32, name: &str, north: f64, east: f64) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((START_NS + north / 240.0, START_EW + east / 240.0)),
        ..ScriptedObject::default()
    }
}

/// A data folder holding `files` (path and text).
fn data(files: &[(&str, &str)]) -> SharedFiles {
    let shared = SharedFiles::default();
    for (path, text) in files {
        shared
            .clone()
            .write(TANK, path, text.as_bytes())
            .expect("in memory");
    }
    shared
}

/// Metres from the Tank's character to the scripted place `north`, `east` metres from its start.
fn metres_from(run: &PluginRun, north: f64, east: f64) -> f64 {
    let (ns, ew, _) = run.player_place().expect("placed");
    let dn = (ns - START_NS) * 240.0 - north;
    let de = (ew - START_EW) * 240.0 - east;
    (dn * dn + de * de).sqrt()
}

/// The macro's loop ran without an error.
fn no_errors(run: &PluginRun) {
    let console = run.console(TANK);
    assert!(
        !console.iter().any(|l| l.contains(" error: ")),
        "{console:#?}"
    );
}

/// The settings that leave only what a scenario tests.
const QUIET: [&str; 3] = [
    "/tank set EnableCombat false",
    "/tank set EnableLooting false",
    "/tank set EnableBuffing false",
];

/// A follow route (VTank's, loaded from a file, made with `/tank follow` from the selection, and
/// embedded in a meta) follows its leader: straight at it, held within reach; with
/// FollowAroundCorners along the leader's own way round a wall a straight walk would stop at; and
/// saved, it is VTank's follow route file.
fn the_tank_follows_its_leader_straight_or_around_corners_by_a_follow_route() {
    let nav = format!("uTank2 NAV 1.2\r\n3\r\nBob\r\n{BOB}\r\n");
    let files = data(&[("routes/bob.nav", &nav)]);
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![character(BOB, "Bob", 0.0, 10.0)],
    );
    for line in QUIET {
        run.type_line(line);
    }
    run.type_line("/tank route bob.nav");
    run.advance(0.5);
    assert!(
        logged(&run, "route bob.nav: following Bob"),
        "{:#?}",
        run.console(TANK)
    );
    run.type_line("/tank start");
    run.advance(8.0);
    assert!(
        metres_from(&run, 0.0, 10.0) <= 3.0,
        "straight to the leader: {:.1} m off",
        metres_from(&run, 0.0, 10.0)
    );
    // The leader walks off north; the follower keeps with it.
    for step in 1..=10 {
        run.move_object(
            BOB,
            START_NS + f64::from(step) * 2.0 / 240.0,
            START_EW + 10.0 / 240.0,
        );
        run.advance(0.5);
    }
    run.advance(6.0);
    assert!(
        metres_from(&run, 20.0, 10.0) <= 3.0,
        "kept with the leader: {:.1} m off",
        metres_from(&run, 20.0, 10.0)
    );
    run.type_line("/tank stop");

    // Around corners: the leader goes north, then east, round a wall the straight way crosses.
    let around = |corners: bool| -> (f64, Vec<String>) {
        let mut run = tank_in(
            PluginRun::new(),
            &data(&[]),
            vec![character(BOB, "Bob", 2.0, 0.0)],
        );
        run.wall(
            (START_NS + 8.0 / 240.0, START_EW + 2.0 / 240.0),
            (START_NS + 2.0 / 240.0, START_EW + 8.0 / 240.0),
        );
        for line in QUIET {
            run.type_line(line);
        }
        run.type_line(&format!("/tank set FollowAroundCorners {corners}"));
        run.select(BOB);
        run.type_line("/tank follow");
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank start");
        run.advance(1.0);
        for step in 1..=14 {
            run.move_object(BOB, START_NS + (2.0 + f64::from(step)) / 240.0, START_EW);
            run.advance(0.4);
        }
        for step in 1..=16 {
            run.move_object(
                BOB,
                START_NS + 16.0 / 240.0,
                START_EW + f64::from(step) / 240.0,
            );
            run.advance(0.4);
        }
        run.type_line("/tank set EnableNav true");
        run.advance(30.0);
        no_errors(&run);
        (metres_from(&run, 16.0, 16.0), run.console(TANK))
    };
    let (straight, _) = around(false);
    let (cornered, log) = around(true);
    assert!(
        cornered <= 3.0 && straight > 6.0,
        "around corners reaches the leader ({cornered:.1} m off), straight stops at the wall \
         ({straight:.1} m off): {log:#?}"
    );

    // The follow route made from the selection saves as VTank's follow route file.
    let files = data(&[]);
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![character(BOB, "Bob", 0.0, 10.0)],
    );
    run.select(BOB);
    run.type_line("/tank follow");
    run.type_line("/tank route save trail");
    run.advance(0.5);
    let saved = files
        .clone()
        .read(TANK, "routes/trail.nav")
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    assert_eq!(
        saved.as_deref(),
        Some(nav.as_str()),
        "{:#?}",
        run.console(TANK)
    );

    // A meta's embedded follow route is the macro's route.
    let body = format!("Bob\r\n1\r\nuTank2 NAV 1.2\r\n3\r\nBob\r\n{BOB}");
    // The byte count covers the line end after the route, as a meta file counts it.
    let walk = vec!["ba".to_owned(), (body.len() + 2).to_string(), body];
    let meta = met(&[(ALWAYS, vec!["i".into(), "0".into()], 4, walk, "Default")]);
    let files = data(&[("metas/follow.met", &meta)]);
    // The leader is an administrator, its name shown with a plus: the route's name still finds it.
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![character(BOB, "+Bob", 0.0, 12.0)],
    );
    for line in QUIET {
        run.type_line(line);
    }
    run.type_line("/tank meta follow.met");
    run.type_line("/tank start");
    run.advance(10.0);
    assert!(
        logged(&run, "route from the meta: following Bob") && metres_from(&run, 0.0, 12.0) <= 3.0,
        "{:.1} m off: {:#?}",
        metres_from(&run, 0.0, 12.0),
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-follows-its-leader-straight-or-around-corners-by-a-follow-route",
    );
}

/// A fellow of the scripted fellowship: its id, its name, and its health, stamina and mana, each
/// `(current, maximum)`.
type ScriptedFellow<'a> = (u32, &'a str, [(u32, u32); 3]);

/// The scripted fellowship: the Tank's character and `fellows`.
fn fellowship(run: &mut PluginRun, fellows: &[ScriptedFellow<'_>]) {
    use dereth_protocol::social::{Fellow, Fellowship, FellowshipFullUpdate, FellowshipLocks};
    let mut entries = vec![(
        SCRIPTED_PLAYER,
        Fellow {
            name: "Tester".into(),
            level: 10,
            ..Fellow::default()
        },
    )];
    for (id, name, [h, s, m]) in fellows {
        entries.push((
            *id,
            Fellow {
                name: (*name).into(),
                level: 10,
                current_health: h.0,
                max_health: h.1,
                current_stamina: s.0,
                max_stamina: s.1,
                current_mana: m.0,
                max_mana: m.1,
                ..Fellow::default()
            },
        ));
    }
    run.server_says(&[ui_event(&FellowshipFullUpdate(Fellowship {
        members: dereth_protocol::archive::PackedHash {
            table_size: 16,
            entries,
        },
        name: "Hunters".into(),
        leader: ObjectId(SCRIPTED_PLAYER),
        share_xp: 1,
        even_xp_split: 0,
        open_fellow: 1,
        locked: 0,
        fellows_departed: dereth_protocol::archive::PackedHash::default(),
        locks: FellowshipLocks::default(),
    }))]);
}

/// The helper spells of these scenarios: Heal Other, Revitalize Other and Mana Boost Other.
const HELPER_SPELLS: &[(u32, &str, u32)] = &[
    (5, "Heal Other I", 2),
    (1183, "Revitalize Other I", 2),
    (1207, "Mana Boost Other I", 2),
];

/// VTank's DoHelp: a fellow under a helper mark and within its distance is healed, given stamina
/// and given mana, each by the Other spell of it; one beyond the distance is not; with
/// UseHealersHeart a Healer's Heart in hand is used on the fellow, its own heal cast, the heart
/// kept in hand.
fn the_tank_heals_restams_and_gives_mana_to_fellows_within_reach_and_heals_with_a_healers_heart() {
    const FAR: u32 = 0x5000_0003;
    const HEART: u32 = 0x8000_0B10;
    let mut run = tank_in(
        PluginRun::new().with_spells(HELPER_SPELLS, true),
        &data(&[]),
        vec![
            character(BOB, "Bob", 0.0, 6.0),
            character(FAR, "Far", 0.0, 80.0),
            wand(),
        ],
    );
    run.set_vitals((100, 100), (100, 100), (100, 100));
    for line in QUIET {
        run.type_line(line);
    }
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set DoHelp true");
    fellowship(
        &mut run,
        &[
            (BOB, "Bob", [(20, 100), (100, 100), (100, 100)]),
            (FAR, "Far", [(10, 100), (100, 100), (100, 100)]),
        ],
    );
    // The first heal is beyond the spell's reach, as the server says: the helper walks closer.
    run.fail_casts([0x0550]);
    run.type_line("/tank start");
    run.advance(6.0);
    assert!(
        sent_count(&run, "cast 5") >= 2
            && logged(
                &run,
                "help: Heal Other I on Bob (health 20/100): failed (0x0550)"
            )
            && metres_from(&run, 0.0, 6.0) <= 5.5
            && !logged(&run, " on Far "),
        "the near fellow healed (walked closer after one out of reach), the far one not: {:?} \
         {:#?}",
        run.sent(),
        run.console(TANK)
    );
    // Above the health mark (60%), the fellow is given stamina, not healed.
    let heals = sent_count(&run, "cast 5");
    fellowship(
        &mut run,
        &[(BOB, "Bob", [(70, 100), (10, 100), (100, 100)])],
    );
    run.advance(6.0);
    assert!(
        sent_count(&run, "cast 1183") >= 1
            && logged(&run, "help: Revitalize Other I on Bob (stamina 10/100)")
            && sent_count(&run, "cast 5") == heals,
        "stamina: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    fellowship(
        &mut run,
        &[(BOB, "Bob", [(100, 100), (100, 100), (10, 100)])],
    );
    run.advance(6.0);
    assert!(
        sent_count(&run, "cast 1207") >= 1
            && logged(&run, "help: Mana Boost Other I on Bob (mana 10/100)"),
        "mana: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    run.type_line("/tank stop");

    // A Healer's Heart in hand heals instead, kept in hand.
    let mut run = tank_in(
        PluginRun::new().with_spells(HELPER_SPELLS, true),
        &data(&[]),
        vec![
            character(BOB, "Bob", 0.0, 6.0),
            ScriptedObject {
                worn: true,
                locations: Some(0x0100_0000),
                spell_id: Some(1166),
                useability: Some(used(WIELDED, REMOTE | NEVER_WALK)),
                target_type: Some(item_type::CREATURE),
                ..held(HEART, "The Healer's Heart", item_type::CASTER)
            },
        ],
    );
    for line in QUIET {
        run.type_line(line);
    }
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set DoHelp true");
    run.type_line("/tank set UseHealersHeart true");
    fellowship(
        &mut run,
        &[(BOB, "Bob", [(20, 100), (100, 100), (100, 100)])],
    );
    run.type_line("/tank start");
    run.advance(6.0);
    let used = format!("use_with_target {HEART:#x} {BOB:#x}");
    assert!(
        sent_count(&run, &used) >= 1
            && run.sent().iter().all(|s| !s.starts_with("cast 5 "))
            && !run
                .sent()
                .iter()
                .any(|s| s.starts_with(&format!("put_item_in_container {HEART:#x}"))),
        "the heart's own heal, the heart kept: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-heals-restams-and-gives-mana-to-fellows-within-reach-and-heals-with-a-healers-heart",
    );
}

/// A tell to the character from `name` (object `id`) saying `said`.
fn tell(run: &mut PluginRun, name: &str, id: u32, said: &str) {
    use dereth_protocol::comms::CommunicationHearDirectSpeech;
    run.server_says(&[ui_event(&CommunicationHearDirectSpeech {
        message: said.into(),
        sender_name: name.into(),
        sender_id: ObjectId(id),
        target_id: ObjectId(SCRIPTED_PLAYER),
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })]);
}

/// VTank's AutoFellowManagement, as the fellowship's leader: "xp" from a player standing near is
/// answered with VTank's line and the player recruited; one too far is told so and given another
/// chance; "help" lists the commands; with the option off, nothing is answered.
fn the_tank_recruits_on_xp_tells_and_answers_vtanks_fellowship_commands() {
    const FAR: u32 = 0x5000_0003;
    let mut run = tank_in(
        PluginRun::new(),
        &data(&[]),
        vec![
            character(BOB, "Bob", 0.0, 5.0),
            character(FAR, "Far", 0.0, 90.0),
        ],
    );
    fellowship(&mut run, &[]);
    tell(&mut run, "Bob", BOB, "xp");
    run.advance(5.0);
    assert!(
        !run.sent().iter().any(|s| s.starts_with("tell ")),
        "off: nothing answered: {:?}",
        run.sent()
    );
    run.type_line("/tank set AutoFellowManagement true");
    tell(&mut run, "Bob", BOB, "xp");
    run.advance(5.0);
    let recruit = format!("fellowship_recruit {BOB:#x}");
    assert!(
        run.sent().iter().any(|s| s
            == "tell Bob: [VT Fellow Manager] I will recruit you in a moment. Please stand close to me. -v-")
            && sent_count(&run, &recruit) == 1,
        "{:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    tell(&mut run, "Far", FAR, "xp");
    run.advance(5.0);
    assert!(
        run.sent().iter().any(|s| s
            == "tell Far: [VT Fellow Manager] You are too far away. I will wait 20 seconds and give you one more chance. -v-")
            && sent_count(&run, &format!("fellowship_recruit {FAR:#x}")) == 0,
        "{:?}",
        run.sent()
    );
    run.move_object(FAR, START_NS, START_EW + 5.0 / 240.0);
    run.advance(25.0);
    assert_eq!(
        sent_count(&run, &format!("fellowship_recruit {FAR:#x}")),
        1,
        "the second chance taken: {:?}",
        run.sent()
    );
    tell(&mut run, "Bob", BOB, "help");
    run.advance(2.0);
    assert!(
        run.sent().iter().any(|s| s
            == "tell Bob: [VT Fellow Manager] Available commands: xp, line, remove, leader, startvote, vote, help -v-"),
        "{:?}",
        run.sent()
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-recruits-on-xp-tells-and-answers-vtanks-fellowship-commands",
    );
}

/// Two clients' Tanks in one fellow of their own (PeerFellow) on the machine's bus, no
/// fellowship between them: one heals the other, hurt, from what the other tells it; a
/// vulnerability one has put on a monster the other does not cast again, and the other fights the
/// monster made vulnerable before a nearer one; a Tank in no fellow of its own hears none of it.
fn tanks_in_one_fellow_on_a_machine_heal_each_other_and_share_their_debuffs() {
    use dereth_plugin_host::LocalBus;
    const A: u32 = 0x5000_0011;
    const B: u32 = 0x5000_0012;
    const NEAR: u32 = 0x8000_0C01;
    const VULNED: u32 = 0x8000_0C02;
    let spells: Vec<(u32, &str, u32)> = HELPER_SPELLS
        .iter()
        .copied()
        .chain([(1030, "Fire Vulnerability Other I", 4)])
        .collect();
    // Two clients on one bus, each seeing the other's character 5 m west and two monsters east.
    let pair = |fellow: &str| {
        let bus = LocalBus::new();
        let run_of = |me: u32, other: u32| {
            let mut run = tank_in(
                PluginRun::new()
                    .with_spells(&spells, true)
                    .with_peers(Box::new(bus.endpoint()))
                    .with_player_id(me),
                &data(&[]),
                vec![
                    monster(NEAR, "Drudge Skulker", START_NS, START_EW + 4.0 / 240.0),
                    monster(VULNED, "Drudge Ravener", START_NS, START_EW + 7.0 / 240.0),
                    character(other, "Tester", 0.0, -5.0),
                    ScriptedObject {
                        container: Some(me),
                        ..wand()
                    },
                ],
            );
            run.set_vitals((100, 100), (100, 100), (100, 100));
            for line in [
                "/tank set EnableLooting false",
                "/tank set EnableBuffing false",
                "/tank set EnableNav false",
                "/tank set EnableCombat false",
            ] {
                run.type_line(line);
            }
            run.type_line(&format!("/tank set PeerFellow {fellow}"));
            run
        };
        (run_of(A, B), run_of(B, A))
    };
    let both = |a: &mut PluginRun, b: &mut PluginRun, secs: f64| {
        let mut t = 0.0;
        while t < secs {
            a.advance(0.25);
            b.advance(0.25);
            t += 0.25;
        }
    };

    // Helping from what the fellow tells.
    for (fellow, healed) in [("hunters", true), ("", false)] {
        let (mut a, mut b) = pair(fellow);
        a.set_vitals((20, 100), (100, 100), (100, 100));
        b.type_line("/tank set DoHelp true");
        a.type_line("/tank start");
        b.type_line("/tank start");
        both(&mut a, &mut b, 6.0);
        assert_eq!(
            sent_count(&b, "cast 5") >= 1
                && logged(&b, "help: Heal Other I on Tester (health 20/100)"),
            healed,
            "in a fellow ({fellow:?}) B heals A from what A's Tank tells: {:?} {:#?}",
            b.sent(),
            b.console(TANK)
        );
    }

    // Debuffs: A vulns the monster the player selects; B, its row vulning too, casts no vuln on it
    // and fights it before the nearer one.
    let (mut a, mut b) = pair("hunters");
    for run in [&mut a, &mut b] {
        for line in [
            "/tank set EnableCombat true",
            "/tank monster damage fire <DEFAULT>",
            "/tank monster vuln on <DEFAULT>",
            "/tank monster weapon Wand <DEFAULT>",
        ] {
            run.type_line(line);
        }
    }
    a.type_line("/tank monster attack off <DEFAULT>");
    a.type_line("/tank set TargetLock true");
    a.select(VULNED);
    a.type_line("/tank start");
    both(&mut a, &mut b, 4.0);
    assert!(
        sent_count(&a, "cast 1030") >= 1
            && logged(&a, "Fire Vulnerability Other I on Drudge Ravener"),
        "A vulns the selected monster: {:?} {:#?}",
        a.sent(),
        a.console(TANK)
    );
    b.type_line("/tank start");
    both(&mut b, &mut a, 6.0);
    let first_target = b
        .console(TANK)
        .into_iter()
        .find(|l| l.contains(" target "))
        .unwrap_or_default();
    assert!(
        !logged(&b, "Fire Vulnerability Other I on Drudge Ravener")
            && first_target.contains("target Drudge Ravener")
            && sent_count(&b, "cast 27") >= 1,
        "B does not vuln it again, and bolts it first: {:?} {:#?}",
        b.sent(),
        b.console(TANK)
    );
    no_errors(&b);
    note_asserted(
        "plugins.samples.tanks-in-one-fellow-on-a-machine-heal-each-other-and-share-their-debuffs",
    );
}

/// VTank's SummonPets: with no pet of the character's out and enough monsters its rows summon for in
/// pet range, the summoning device of the character's mastery whose element is the monster's row's
/// (PetDmg) is used; a device of another mastery is never used; with too few monsters, or a pet
/// out, none is.
fn the_tank_summons_a_pet_of_its_mastery_and_the_monsters_element_once_enough_gather() {
    const FIRE: u32 = 0x8000_0D01;
    const FROST: u32 = 0x8000_0D02;
    // Before the frost device in the pack, so a choice blind to mastery would take it.
    const OTHER_MASTERY: u32 = 0x8000_0D00;
    let essence = |id: u32, name: &str, effects: u32, mastery: i32| ScriptedObject {
        effects: Some(effects),
        ints: vec![(362, mastery), (367, 50)],
        structure: Some(50),
        ..held(id, name, item_type::MISC)
    };
    let mut run = tank_in(
        PluginRun::new(),
        &data(&[]),
        vec![
            essence(OTHER_MASTERY, "Frost Skeleton Essence", 0x80, 2),
            essence(FIRE, "Fire Elemental Essence", 0x20, 1),
            essence(FROST, "Frost Elemental Essence", 0x80, 1),
            monster(
                0x8000_0D10,
                "Drudge Skulker",
                START_NS,
                START_EW + 4.0 / 240.0,
            ),
        ],
    );
    run.set_skill("Summoning", 200);
    run.world_mut()
        .player_qualities_mut()
        .expect("qualities")
        .ints
        .get_or_insert_with(Default::default)
        .insert(362, 1);
    for line in [
        "/tank set EnableLooting false",
        "/tank set EnableBuffing false",
        "/tank set EnableNav false",
        "/tank set SummonPets true",
        "/tank set PetMonsterDensity 2",
        "/tank monster petdmg cold <DEFAULT>",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(4.0);
    let used = |run: &PluginRun, id: u32| sent_count(run, &format!("use {id:#x}"));
    assert_eq!(
        used(&run, FROST) + used(&run, FIRE),
        0,
        "one monster, two wanted: {:?}",
        run.sent()
    );
    run.type_line("/tank vt testpet");
    run.advance(1.0);
    assert!(
        run.printed()
            .iter()
            .any(|l| l.starts_with("[Tank] no pet now: 1 monsters to summon for")),
        "{:#?}",
        run.printed()
    );
    run.spawn(vec![monster(
        0x8000_0D11,
        "Drudge Skulker",
        START_NS,
        START_EW + 5.0 / 240.0,
    )]);
    run.advance(4.0);
    assert!(
        used(&run, FROST) == 1 && used(&run, FIRE) == 0 && used(&run, OTHER_MASTERY) == 0,
        "the frost device of the character's mastery: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    // A pet out: no other summoned.
    run.spawn(vec![ScriptedObject {
        id: 0x8000_0D20,
        name: "Tester's Frost Elemental".into(),
        obj_type: item_type::CREATURE,
        place: Some((START_NS, START_EW + 1.0 / 240.0)),
        ..ScriptedObject::default()
    }]);
    run.advance(15.0);
    assert_eq!(used(&run, FROST) + used(&run, FIRE), 1, "{:?}", run.sent());
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-summons-a-pet-of-its-mastery-and-the-monsters-element-once-enough-gather",
    );
}

/// VTank's lockpicks for doors: a locked door across the route's leg is looked at and its lock
/// picked with a lockpick when the character's lockpicking beats the lock by
/// DoorLockpickDiffExcessThreshold, then opened and walked through; one whose lock is beyond the
/// character is left, said.
fn the_tank_picks_a_locked_doors_lock_when_its_skill_beats_it() {
    const DOOR: u32 = 0x8000_0E01;
    const PICK: u32 = 0x8000_0E02;
    let route = "uTank2 NAV 1.2\r\n4\r\n1\r\n0\r\n33.8\r\n42.25\r\n0\r\n0\r\n";
    let world = || {
        vec![
            ScriptedObject {
                id: DOOR,
                name: "Door".into(),
                obj_type: item_type::MISC,
                place: Some((START_NS + 6.0 / 240.0, START_EW)),
                door: Some(dereth_testkit::plugins::ScriptedDoor { locked: true }),
                ints: vec![(173, 120)],
                ..ScriptedObject::default()
            },
            ScriptedObject {
                bitfield: bitfield::LOCKPICK,
                useability: Some(used(CONTAINED, REMOTE)),
                target_type: Some(0x280),
                ints: vec![(88, 10)],
                ..held(PICK, "Plain Lockpick", item_type::KEY)
            },
        ]
    };
    let picked = |skill: i32| {
        let files = data(&[("routes/door.nav", route)]);
        let mut run = tank_in(PluginRun::new(), &files, world());
        run.set_skill("Lockpick", skill);
        for line in QUIET {
            run.type_line(line);
        }
        run.type_line("/tank set DoorLockpickDiffExcessThreshold 5");
        run.type_line("/tank route door.nav");
        run.type_line("/tank start");
        run.advance(25.0);
        no_errors(&run);
        run
    };
    let run = picked(150);
    let pick = format!("use_with_target {PICK:#x} {DOOR:#x}");
    assert!(
        sent_count(&run, &pick) >= 1
            && logged(&run, "picked the lock of Door")
            && logged(&run, "route: finished"),
        "picked, opened and walked through: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    let run = picked(100);
    assert!(
        sent_count(&run, &pick) == 0 && logged(&run, "is beyond picking (110)"),
        "beyond the character: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    note_asserted("plugins.samples.the-tank-picks-a-locked-doors-lock-when-its-skill-beats-it");
}

/// VTank's wand recharge spells: a caster whose own spell is Stamina to Mana is used, in hand, on
/// the character when its mana runs low, before the character's own spell; the caster is never put
/// in the pack for it. A caster whose magic needs more Arcane Lore than the character has is not
/// used (its own spell is cast instead), and one whose use gave nothing back is left a while.
fn the_tank_recharges_with_its_wands_own_spell_keeping_the_wand_in_hand() {
    const STEIN: u32 = 0x8000_0F01;
    let run_with = |lore: i32| {
        let mut run = tank_in(
            PluginRun::new().with_spells(&[(1679, "Stamina to Mana Self IV", 2)], false),
            &data(&[]),
            vec![ScriptedObject {
                worn: true,
                locations: Some(0x0100_0000),
                spell_id: Some(1679),
                useability: Some(used(WIELDED, CONTAINED | SELF)),
                target_type: Some(item_type::CREATURE),
                ints: vec![(109, 50)],
                ..held(STEIN, "Casting Stein", item_type::CASTER)
            }],
        );
        run.set_skill("Arcane Lore", lore);
        for line in QUIET {
            run.type_line(line);
        }
        run.type_line("/tank set EnableNav false");
        run.set_vitals((100, 100), (100, 100), (10, 100));
        run.type_line("/tank start");
        run.advance(8.0);
        no_errors(&run);
        run
    };
    let used = format!("use_with_target {STEIN:#x} {SCRIPTED_PLAYER:#x}");
    let run = run_with(80);
    assert!(
        sent_count(&run, &used) == 1
            && !run
                .sent()
                .iter()
                .any(|s| s.starts_with(&format!("put_item_in_container {STEIN:#x}")))
            && logged(
                &run,
                "mana: Casting Stein's Stamina to Mana Self IV did nothing; left 120 s"
            ),
        "the stein's own spell, the stein kept, once while it gives nothing: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    let run = run_with(20);
    assert!(
        sent_count(&run, &used) == 0
            && sent_count(&run, &format!("cast 1676 {SCRIPTED_PLAYER:#x}")) >= 1,
        "its magic beyond the character, its own spell instead: {:?} {:#?}",
        run.sent(),
        run.console(TANK)
    );
    let _ = (read_package, samples, text);
    note_asserted(
        "plugins.samples.the-tank-recharges-with-its-wands-own-spell-keeping-the-wand-in-hand",
    );
}
