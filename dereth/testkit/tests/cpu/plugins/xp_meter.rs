//! The XP Meter sample's monster statistics and session counts: over a recorded session's combat,
//! and in a scripted world for what the recording does not hold.

use dereth_client_net::client_session::SessionEvent;
use dereth_plugin_host::Package;
use dereth_protocol::combat::{
    AttackerNotification, DefenderNotification, EvasionAttackerNotification,
    EvasionDefenderNotification, VictimNotificationOther,
};
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER};

use super::inventory::SharedStorage;
use super::samples;

dereth_testkit::scenarios! {
    scenario_the_xp_meter_learns_what_each_monster_does_from_the_servers_combat_notices => the_xp_meter_learns_what_each_monster_does_from_the_servers_combat_notices ["plugins.samples.the-xp-meter-learns-what-each-monster-does-from-the-servers-combat-notices"],
    scenario_the_xp_meter_gives_a_kill_the_experience_and_the_corpse_that_follow_it => the_xp_meter_gives_a_kill_the_experience_and_the_corpse_that_follow_it ["plugins.samples.the-xp-meter-gives-a-kill-the-experience-and-the-corpse-that-follow-it"],
    scenario_the_xp_meter_counts_the_sessions_kills_deaths_and_luminance_and_keeps_the_monsters_for_the_character => the_xp_meter_counts_the_sessions_kills_deaths_and_luminance_and_keeps_the_monsters_for_the_character ["plugins.samples.the-xp-meter-counts-the-sessions-kills-deaths-and-luminance-and-keeps-the-monsters-for-the-character"],
    scenario_the_xp_meter_forgets_one_monster_and_keeps_the_others_for_the_character => the_xp_meter_forgets_one_monster_and_keeps_the_others_for_the_character ["plugins.samples.the-xp-meter-forgets-one-monster-and-keeps-the-others-for-the-character"],
}

const XP_METER: &str = "network.dereth.xpmeter";

/// The lines the XP Meter printed: the only plugin loaded.
fn said(run: &PluginRun) -> Vec<String> {
    run.printed()
}

/// The lines `/xp monsters` prints now.
fn monsters(run: &mut PluginRun) -> Vec<String> {
    let before = said(run).len();
    run.type_line("/xp monsters");
    run.advance(0.5);
    said(run).split_off(before)
}

fn no_errors(run: &PluginRun) {
    let console = run.console(XP_METER);
    assert!(
        !console.iter().any(|l| l.starts_with("error")),
        "{console:#?}"
    );
    assert_eq!(run.status(XP_METER), "running", "{console:#?}");
}

/// A recorded session in the Academy and beyond: swords and spells against five kinds of monster,
/// twelve of them killed. Each kind's line counts its kills, its experience, how often it evaded
/// the character and the character it, and how often it resisted the character's spells, from the
/// server's combat notices and its spell lines; a kill named only in a death line (one spell, no
/// swing) still counts. The window's row for a monster shows the rest.
fn the_xp_meter_learns_what_each_monster_does_from_the_servers_combat_notices() {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("xp-meter")));
    run.login("long-solo-play", Some("+Aldric"));
    run.advance(1050.0);
    no_errors(&run);
    let lines = monsters(&mut run);
    assert_eq!(
        lines,
        [
            // Two of the wasps fell to a spell and the third to the sword: 3 of 6 swings
            // evaded, 6 of 8 of its evaded, one of its two spells resisted.
            "Carpenter Wasp: 3 kills, 0 XP a kill, evades you 50%, you evade it 75%, resists you 50%, 5.9 s a kill.",
            // Two fell to spells with nothing before them, so one fight is timed.
            "Thieving Thrungus: 3 kills, 0 XP a kill, evades you 40%, you evade it 13%, resists you -, 11.9 s a kill.",
            // Four of seven Force Bolts resisted.
            "Young Olthoi: 3 kills, 0 XP a kill, evades you -, you evade it 75%, resists you 57%, 7.2 s a kill.",
            // The second gave 28 experience: 14 a kill. 5 of 23 swings evaded, 15 of 26 of its.
            "Sparring Golem: 2 kills, 14 XP a kill, evades you 22%, you evade it 58%, resists you 0%, 52.0 s a kill.",
            "Adolescent Olthoi: 1 kills, 0 XP a kill, evades you -, you evade it -, resists you 0%, 1.9 s a kill.",
        ]
    );
    // `/xp monsters` opened the window too, showing the monster killed most until one is picked.
    assert!(
        run.ui_dump()
            .contains(r#"label #name grow=1 text="Carpenter Wasp""#),
        "{}",
        run.ui_dump()
    );
    run.click_cell(XP_METER, "monsters.list", 4, 1);
    let dump = run.ui_dump();
    assert!(
        dump.contains("window network.dereth.xpmeter monsters open=true"),
        "{dump}"
    );
    for want in [
        // 37 damage in 18 hits; its corpse 2.1 s after its death line.
        r#"["Sparring Golem",2,"14","22%","58%","2","1","52.0 s"]"#,
        r#"label #name grow=1 text="Sparring Golem""#,
        r#"label #corpse text="2.1 s""#,
    ] {
        assert!(dump.contains(want), "{want:?} in:\n{dump}");
    }
    note_asserted(
        "plugins.samples.the-xp-meter-learns-what-each-monster-does-from-the-servers-combat-notices",
    );
}

const DRUDGE: u32 = 0x8000_0201;
const CORPSE: u32 = 0x8000_0202;
const WAND: u32 = 0x8000_0203;
const BOLT: u32 = 4001;

fn drudge() -> ScriptedObject {
    ScriptedObject {
        id: DRUDGE,
        name: "Drudge Skulker".into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::ATTACKABLE,
        place: Some((42.2, 33.85)),
        ..ScriptedObject::default()
    }
}

fn hit(who: &str, damage: u32, critical: bool) -> SessionEvent {
    ui_event(&AttackerNotification {
        defender_name: who.into(),
        damage_type: 1,
        percent: 0.2,
        damage,
        critical: u32::from(critical),
        ..AttackerNotification::default()
    })
}

fn hit_me(who: &str, damage: u32) -> SessionEvent {
    ui_event(&DefenderNotification {
        attacker_name: who.into(),
        damage_type: 4,
        percent: 0.1,
        damage,
        damage_location: 1,
        ..DefenderNotification::default()
    })
}

fn killed(text: &str) -> SessionEvent {
    ui_event(&VictimNotificationOther {
        message: text.into(),
    })
}

fn server_line(text: &str) -> SessionEvent {
    ui_event(&dereth_protocol::comms::CommunicationTextboxString {
        text: text.into(),
        text_type: 7,
    })
}

/// The character's total experience and luminance become `xp` and `luminance`, as the server's
/// updates set them.
fn earn(run: &mut PluginRun, xp: i64, luminance: i64) {
    let q = run
        .world_mut()
        .player_qualities_mut()
        .expect("the player's qualities");
    let int64s = q.int64s.get_or_insert_with(Default::default);
    int64s.insert(1, xp);
    int64s.insert(6, luminance);
    int64s.insert(7, 1_000_000);
    run.frame();
}

fn xp_meter(storage: &SharedStorage) -> PluginRun {
    let mut run = PluginRun::new()
        .with_creature_spells(&[(BOLT, "Frost Bolt I")])
        .with_storage(Box::new(storage.clone()));
    run.load(read_package(&samples().join("xp-meter")));
    // A plugin that casts Frost Bolt I at the drudge on /zap, as the player would, with the wand
    // in hand.
    run.load(Package {
        origin: "caster".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.caster\"\nname = \"Caster\"\nversion = \"1.0.0\"\napi = \"0.7\"\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                format!(
                    "commands.add('zap', 'cast', function() task.spawn(function() \
                     await(items.wield({WAND})) await(combat.set_mode('combat')) \
                     await(magic.cast({BOLT}, {DRUDGE})) end) end)\n"
                )
                .into_bytes(),
            ),
        ]
        .into(),
    });
    run.scripted_world(
        "Tester",
        vec![
            drudge(),
            ScriptedObject {
                id: WAND,
                name: "Wand".into(),
                obj_type: item_type::CASTER,
                container: Some(SCRIPTED_PLAYER),
                locations: Some(0x0100_0000),
                ..ScriptedObject::default()
            },
        ],
    );
    earn(&mut run, 1000, 0);
    run.advance(0.5);
    run
}

/// A drudge fought for ten seconds with swords and a spell: the experience that follows its death
/// line within three seconds is its kill's, and the experience a minute later (a quest's) is not;
/// its corpse two seconds after its death is its time to corpse; a critical hit and a plain one
/// are averaged apart, and its spell on the character is counted as its spell damage.
fn the_xp_meter_gives_a_kill_the_experience_and_the_corpse_that_follow_it() {
    let storage = SharedStorage::default();
    let mut run = xp_meter(&storage);
    run.server_says(&[hit("Drudge Skulker", 10, false)]);
    run.advance(2.0);
    run.type_line("/zap");
    run.advance(2.0);
    run.type_line("/zap");
    run.server_says(&[
        server_line("Drudge Skulker resists your spell"),
        server_line("You cast Imperil Other I on Drudge Skulker"),
    ]);
    run.server_says(&[
        hit("Drudge Skulker", 30, true),
        ui_event(&EvasionAttackerNotification {
            defender_name: "Drudge Skulker".into(),
        }),
        hit_me("Drudge Skulker", 6),
        ui_event(&EvasionDefenderNotification {
            attacker_name: "Drudge Skulker".into(),
        }),
        server_line("Drudge Skulker singes you for 12 points with Flame Bolt I."),
    ]);
    run.advance(6.0);
    run.server_says(&[killed("Drudge Skulker falls before your blow!")]);
    run.remove(DRUDGE);
    run.advance(1.0);
    earn(&mut run, 1150, 0);
    run.advance(1.0);
    run.spawn(vec![ScriptedObject {
        id: CORPSE,
        name: "Corpse of Drudge Skulker".into(),
        obj_type: item_type::CONTAINER,
        bitfield: bitfield::CORPSE | bitfield::OPENABLE,
        place: Some((42.2, 33.85)),
        ..ScriptedObject::default()
    }]);
    run.advance(60.0);
    earn(&mut run, 5150, 0);
    run.advance(1.0);
    no_errors(&run);
    assert_eq!(
        monsters(&mut run),
        ["Drudge Skulker: 1 kills, 150 XP a kill, evades you 33%, you evade it 50%, resists you 50%, 10.0 s a kill."]
    );
    run.click(XP_METER, "main.monsters");
    run.choose(XP_METER, "monsters.list", vec![1]);
    let dump = run.ui_dump();
    for want in [
        r#"label #normal text="10""#,
        r#"label #critical text="30""#,
        r#"label #taken text="6""#,
        r#"label #spell_taken text="12""#,
        r#"label #corpse text="2.0 s""#,
        r#"label #casts text="2.0""#,
    ] {
        assert!(dump.contains(want), "{want:?} in:\n{dump}");
    }
    note_asserted(
        "plugins.samples.the-xp-meter-gives-a-kill-the-experience-and-the-corpse-that-follow-it",
    );
}

/// The session's kills, deaths and luminance in the window and on `/xp`, each per hour since the
/// first of them; a reset starts them again but keeps the monsters, which the character finds
/// again in its next session, until `/xp forget`.
fn the_xp_meter_counts_the_sessions_kills_deaths_and_luminance_and_keeps_the_monsters_for_the_character(
) {
    let storage = SharedStorage::default();
    let mut run = xp_meter(&storage);
    run.server_says(&[
        hit("Drudge Skulker", 10, false),
        killed("Drudge Skulker falls before your blow!"),
    ]);
    run.advance(0.5);
    earn(&mut run, 1100, 500);
    run.advance(1799.0);
    run.die();
    run.advance(0.5);
    run.type_line("/xp");
    run.advance(0.5);
    let lines = said(&run);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("Kills: 1 (2 / hour), deaths: 1 (2 / hour), luminance: 500 (1,000 / hour).")
    );
    let dump = run.ui_dump();
    assert!(
        dump.contains("window network.dereth.xpmeter main open=true"),
        "/xp opens it:\n{dump}"
    );
    for want in [
        r#"label #kills text="1 (2 / hour)""#,
        r#"label #deaths text="1 (2 / hour)""#,
        r#"label #luminance text="500 (1,000 / hour)""#,
    ] {
        assert!(dump.contains(want), "{want:?} in:\n{dump}");
    }
    run.click(XP_METER, "main.reset");
    assert!(
        run.ui_dump()
            .contains(r#"label #kills text="0 (0 / hour)""#),
        "{}",
        run.ui_dump()
    );
    assert_eq!(monsters(&mut run).len(), 1);
    no_errors(&run);
    run.unload(XP_METER);

    // The next session: the drudge is remembered, and a second kill adds to it.
    let mut run = xp_meter(&storage);
    run.server_says(&[killed("Drudge Skulker falls before your blow!")]);
    run.advance(4.0);
    assert_eq!(
        monsters(&mut run),
        ["Drudge Skulker: 2 kills, 50 XP a kill, evades you 0%, you evade it -, resists you -, 0.0 s a kill."]
    );
    run.type_line("/xp forget");
    run.advance(0.5);
    assert_eq!(monsters(&mut run), ["No monsters fought yet."]);
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-xp-meter-counts-the-sessions-kills-deaths-and-luminance-and-keeps-the-monsters-for-the-character",
    );
}

/// Three monsters fought: the Monsters window's Forget forgets the one picked and shows the row
/// that took its place; `/xp forget` with a name forgets that one, its name matched as its row
/// shows it whatever the case, and says so when there is none; the one left is still there in the
/// character's next session.
fn the_xp_meter_forgets_one_monster_and_keeps_the_others_for_the_character() {
    let storage = SharedStorage::default();
    let mut run = xp_meter(&storage);
    for who in ["Drudge Skulker", "Drudge Skulker", "Mite", "Rat"] {
        run.server_says(&[
            hit(who, 5, false),
            killed(&format!("{who} falls before your blow!")),
        ]);
        run.advance(4.0);
    }
    assert_eq!(
        monsters(&mut run).len(),
        3,
        "the drudge, then the mite and the rat"
    );
    run.choose(XP_METER, "monsters.list", vec![2]);
    assert!(
        run.ui_dump().contains(r#"label #name grow=1 text="Mite""#),
        "{}",
        run.ui_dump()
    );
    run.click(XP_METER, "monsters.forget");
    let dump = run.ui_dump();
    assert!(
        dump.contains(r#"label #name grow=1 text="Rat""#),
        "the next row:\n{dump}"
    );
    assert!(!dump.contains(r#""Mite""#), "{dump}");
    run.type_line("/xp forget drudge skulker");
    run.type_line("/xp forget Nobody");
    run.advance(0.5);
    let lines = said(&run);
    assert_eq!(
        lines[lines.len() - 2..],
        [
            "XP Meter forgot Drudge Skulker.",
            "No monster named Nobody."
        ]
    );
    no_errors(&run);
    run.unload(XP_METER);

    let mut run = xp_meter(&storage);
    let left = monsters(&mut run);
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(left[0].starts_with("Rat: 1 kills,"), "{left:?}");
    run.click(XP_METER, "monsters.forget");
    assert!(
        run.ui_dump()
            .contains(r#"label #name grow=1 text="No monsters fought yet.""#),
        "{}",
        run.ui_dump()
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-xp-meter-forgets-one-monster-and-keeps-the-others-for-the-character",
    );
}
