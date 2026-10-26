//! The Trackers sample in a scripted world: what the combat lines, the character's experience and
//! luminance, its worn items' appraisals, its pack, the corpses and the players around it come to
//! in the plugin's window, its commands and its arrow.

use dereth_client_net::client_session::SessionEvent;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER};

use super::samples;

dereth_testkit::scenarios! {
    scenario_the_trackers_count_each_creatures_damage_by_element_and_the_share_of_attacks_that_landed => the_trackers_count_each_creatures_damage_by_element_and_the_share_of_attacks_that_landed ["plugins.samples.the-trackers-count-each-creatures-damage-by-element-and-the-share-of-attacks-that-landed"],
    scenario_the_trackers_show_experience_and_luminance_per_hour_and_over_the_last_five_minutes => the_trackers_show_experience_and_luminance_per_hour_and_over_the_last_five_minutes ["plugins.samples.the-trackers-show-experience-and-luminance-per-hour-and-over-the-last-five-minutes"],
    scenario_the_trackers_tell_how_long_each_worn_items_mana_lasts_and_warn_before_it_runs_out => the_trackers_tell_how_long_each_worn_items_mana_lasts_and_warn_before_it_runs_out ["plugins.samples.the-trackers-tell-how-long-each-worn-items-mana-lasts-and-warn-before-it-runs-out"],
    scenario_the_trackers_count_the_consumables_used_and_warn_when_one_runs_low => the_trackers_count_the_consumables_used_and_warn_when_one_runs_low ["plugins.samples.the-trackers-count-the-consumables-used-and-warn-when-one-runs-low"],
    scenario_the_trackers_remember_your_corpse_and_point_the_arrow_at_it => the_trackers_remember_your_corpse_and_point_the_arrow_at_it ["plugins.samples.the-trackers-remember-your-corpse-and-point-the-arrow-at-it"],
    scenario_the_trackers_keep_each_player_seen_and_where_it_was_last => the_trackers_keep_each_player_seen_and_where_it_was_last ["plugins.samples.the-trackers-keep-each-player-seen-and-where-it-was-last"],
}

const TRACKERS: &str = "network.dereth.trackers";

/// A run with the Trackers loaded, asking for nothing, in a scripted world holding `objects`.
fn trackers(objects: Vec<ScriptedObject>) -> PluginRun {
    let mut run = PluginRun::new();
    run.load(read_package(&samples().join("trackers")));
    run.scripted_world("Tester", objects);
    run.advance(0.5);
    run
}

/// A line of `text_type` the server sends as it is.
fn server_line(text: &str, text_type: u32) -> SessionEvent {
    ui_event(&dereth_protocol::comms::CommunicationTextboxString {
        text: text.into(),
        text_type,
    })
}

/// The kind of the server's magic lines.
const MAGIC: u32 = 7;

fn printed_with(run: &PluginRun, text: &str) -> bool {
    run.printed().iter().any(|l| l.contains(text))
}

/// Melee hits each way, a spell each way, an evasion each way, a resist each way, a critical hit
/// and the server's word on a death followed by the corpse: the window's totals, its creature
/// rows with the share of attacks that landed and the elements each creature hits with, and the
/// lifetime that a reset of the session keeps.
fn the_trackers_count_each_creatures_damage_by_element_and_the_share_of_attacks_that_landed() {
    use dereth_client_model::combat::damage_type;
    use dereth_protocol::combat::{
        AttackerNotification, DefenderNotification, EvasionAttackerNotification,
        EvasionDefenderNotification, VictimNotificationOther,
    };
    let mut run = trackers(Vec::new());
    let hit = |who: &str, kind: u32, damage: u32, critical: bool| {
        ui_event(&AttackerNotification {
            defender_name: who.into(),
            damage_type: kind,
            percent: 0.3,
            damage,
            critical: u32::from(critical),
            ..AttackerNotification::default()
        })
    };
    let hit_me = |who: &str, kind: u32, damage: u32| {
        ui_event(&DefenderNotification {
            attacker_name: who.into(),
            damage_type: kind,
            percent: 0.3,
            damage,
            damage_location: 1,
            ..DefenderNotification::default()
        })
    };
    run.server_says(&[
        // You: three blows land on the drudge (one a critical hit), one it evades; one spell on
        // the wasp lands and one it resists.
        hit("Drudge Skulker", damage_type::SLASH, 10, false),
        hit("Drudge Skulker", damage_type::SLASH, 20, true),
        hit("Drudge Skulker", damage_type::PIERCE, 10, false),
        ui_event(&EvasionAttackerNotification {
            defender_name: "Drudge Skulker".into(),
        }),
        server_line(
            "You scorch Carpenter Wasp for 30 points with Flame Bolt I.",
            MAGIC,
        ),
        server_line("Carpenter Wasp resists your spell", MAGIC),
        // Against you: the drudge hits twice and you evade it twice; the wasp's spell lands
        // once and you resist it once.
        hit_me("Drudge Skulker", damage_type::BLUDGEON, 6),
        hit_me("Drudge Skulker", damage_type::BLUDGEON, 4),
        ui_event(&EvasionDefenderNotification {
            attacker_name: "Drudge Skulker".into(),
        }),
        ui_event(&EvasionDefenderNotification {
            attacker_name: "Drudge Skulker".into(),
        }),
        server_line(
            "Carpenter Wasp singes you for 12 points with Flame Bolt I.",
            MAGIC,
        ),
        server_line("You resist the spell cast by Carpenter Wasp", MAGIC),
    ]);
    // The server tells its killer of the drudge's death; the line reaches the chat with the next
    // batch, as the client's scroll gives it.
    run.server_says(&[ui_event(&VictimNotificationOther {
        message: "Drudge Skulker falls before your blow!".into(),
    })]);
    run.server_says(&[]);
    run.spawn(vec![ScriptedObject {
        id: 0x8000_0101,
        name: "Corpse of Drudge Skulker".into(),
        obj_type: item_type::CONTAINER,
        bitfield: bitfield::CORPSE | bitfield::OPENABLE,
        place: Some((42.21, 33.81)),
        ..ScriptedObject::default()
    }]);
    run.advance(1.5);
    let dump = run.ui_dump();
    for want in [
        // 40 + 30 dealt, 6 + 4 + 12 taken; one of four landed blows critical.
        "Dealt 70 (25% critical), taken 22; 1 killing blows, 1 kills.",
        // Four landed of six: the drudge evaded one, the wasp resisted one.
        "Your attacks: 6, 67% landed (1 evaded, 1 resisted).",
        // Two of four melee attacks evaded; one of two spells resisted.
        "You evaded 50% of 4 melee and missile attacks; resisted 50% of 2 spells.",
        "Taken by element: Fire 55%, Bludgeoning 45%",
        "Dealt by element: Fire 43%, Slashing 43%, Piercing 14%",
    ] {
        assert!(dump.contains(want), "{want:?} in:\n{dump}");
    }
    // The creatures' rows: the drudge's blows landed 3 of 4, it was evaded 2 of 4, it fell to
    // your blow and hits with bludgeoning; the wasp's spells were resisted 1 of 2 and it hits
    // with fire.
    assert!(
        dump.contains(r#"["Drudge Skulker",40,10,"75%","50%","-",1,"Bludgeoning 100%"]"#)
            && dump.contains(r#"["Carpenter Wasp",30,12,"50%","-","50%",0,"Fire 100%"]"#),
        "the creature rows:\n{dump}"
    );
    // A reset starts the session over; the lifetime keeps what was counted.
    run.type_line("/track reset");
    run.advance(1.5);
    let dump = run.ui_dump();
    assert!(
        dump.contains("Dealt 0 (- critical), taken 0; 0 killing blows, 0 kills."),
        "the session starts over:\n{dump}"
    );
    assert!(
        dump.contains("Lifetime: dealt 70, taken 22, 1 killing blows."),
        "the lifetime is kept:\n{dump}"
    );
    note_asserted("plugins.samples.the-trackers-count-each-creatures-damage-by-element-and-the-share-of-attacks-that-landed");
}

/// The character's own experience and luminance (64-bit integer qualities 1, 6), as the server
/// updates them.
fn int64(sequence: u8, property_id: u32, value: i64) -> SessionEvent {
    use dereth_protocol::qualities::{PrivateUpdate, QualitiesPrivateUpdateInt64};
    ui_event(&QualitiesPrivateUpdateInt64(PrivateUpdate {
        sequence,
        property_id,
        value,
    }))
}

/// Experience gained a minute in and again later, luminance once: the session's totals, the rate
/// per hour since the first gain, and the rate over the last five minutes, which leaves out the
/// first gain once it is older.
fn the_trackers_show_experience_and_luminance_per_hour_and_over_the_last_five_minutes() {
    let mut run = trackers(Vec::new());
    run.server_says(&[int64(1, 1, 10_000), int64(2, 6, 0), int64(3, 7, 1_000_000)]);
    run.advance(60.0);
    // The first gain: the session's clock starts here.
    run.server_says(&[int64(4, 1, 10_600), int64(5, 6, 500)]);
    run.advance(340.0);
    // The second gain, 340 s after the first.
    run.server_says(&[int64(6, 1, 11_200)]);
    run.advance(250.0);
    // 590 s since the first gain: 1,200 in all is 7,322 an hour; in the last five minutes only
    // the second gain's 600, 7,200 an hour. Luminance: 500 in 590 s, 3,051 an hour; none in the
    // last five minutes.
    let dump = run.ui_dump();
    for want in [
        "Experience this session: 1,200",
        "Per hour: 7,3",
        "over the last 5 minutes: 7,200",
        "Luminance this session: 500",
        "Per hour: 3,05",
        "over the last 5 minutes: 0",
        // No experience table in the scripted world: the time to the next level is unknown.
        "Next level in: -",
    ] {
        assert!(dump.contains(want), "{want:?} in:\n{dump}");
    }
    run.type_line("/track xp");
    assert!(
        printed_with(&run, "[Track] Experience 1,200, 7,3")
            && printed_with(&run, "(7,200 over the last 5 minutes); luminance 500, 3,05"),
        "{:?}",
        run.printed()
    );
    note_asserted(
        "plugins.samples.the-trackers-show-experience-and-luminance-per-hour-and-over-the-last-five-minutes",
    );
}

/// Worn items appraised a few at a time: the robe's spells show on the character and its mana
/// burns half a point a second, the hat has spells that do not show, the ring has no spells, the
/// boots are not worn. The mana list says what each has and how long it lasts; past the warning
/// mark the chat says so once; the server's low-on-mana line is noted.
fn the_trackers_tell_how_long_each_worn_items_mana_lasts_and_warn_before_it_runs_out() {
    const ROBE: u32 = 0x8000_0201;
    const HAT: u32 = 0x8000_0202;
    const RING: u32 = 0x8000_0203;
    const BOOTS: u32 = 0x8000_0204;
    const ROBE_SPELL: u32 = 1;
    let worn =
        |id: u32, name: &str, ints: Vec<(u32, i32)>, rate: f64, spells: Vec<u32>| ScriptedObject {
            id,
            name: name.into(),
            obj_type: item_type::CLOTHING,
            worn: true,
            ints,
            floats: vec![(5, rate)],
            spells,
            ..ScriptedObject::default()
        };
    let mut run = trackers(vec![
        worn(
            ROBE,
            "Robe",
            vec![(107, 1_800), (108, 2_000)],
            -0.5,
            vec![ROBE_SPELL],
        ),
        worn(HAT, "Hat", vec![(107, 50), (108, 500)], -0.1, vec![2]),
        worn(RING, "Ring", Vec::new(), 0.0, Vec::new()),
        ScriptedObject {
            container: Some(SCRIPTED_PLAYER),
            worn: false,
            ..worn(BOOTS, "Boots", vec![(107, 10), (108, 100)], -1.0, vec![2])
        },
    ]);
    // The robe's spell on the character.
    run.server_says(&[ui_event(
        &dereth_protocol::qualities::MagicUpdateEnchantment(
            dereth_protocol::types::qualities::Enchantment {
                id: ROBE_SPELL,
                category_word: 1,
                duration: -1.0,
                caster: dereth_primitives::ObjectId(ROBE),
                // An additive, beneficial raise of an attribute, as the robe's spell gives.
                smod: dereth_protocol::types::qualities::StatMod {
                    kind: 0x0200_8001,
                    key: 1,
                    value: 10.0,
                },
                ..Default::default()
            },
        ),
    )]);
    // Warned of under 59 minutes: the hat at once, the robe a minute into its hour.
    run.type_line("/track warn 59");
    run.advance(8.0);
    let rows = list_rows(&run, "mana");
    // The hat burns out first (500 s), then the robe (3,600 s, less what has burned); the ring
    // burns nothing; the boots are not worn.
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(
        rows[0].starts_with(r#"["Hat","#) && rows[0].ends_with(r#","inactive"]"#),
        "the hat first, its spell not showing: {rows:?}"
    );
    assert!(
        rows[1].starts_with(r#"["Robe","1"#)
            && rows[1].contains(r#""59m "#)
            && rows[1].ends_with(r#","active"]"#),
        "the robe, an hour of mana less what it burned, its spell on the character: {rows:?}"
    );
    assert!(
        rows[2].starts_with(r#"["Ring","-","-","none"]"#),
        "{rows:?}"
    );
    // The hat is warned of at once, the robe once it burns past the mark (and before it is
    // appraised again).
    assert!(
        printed_with(&run, "[Track] Hat has "),
        "{:?}",
        run.printed()
    );
    assert!(
        !printed_with(&run, "[Track] Robe has "),
        "{:?}",
        run.printed()
    );
    run.advance(120.0);
    let robe: Vec<_> = run
        .printed()
        .into_iter()
        .filter(|l| l.starts_with("[Track] Robe has "))
        .collect();
    assert_eq!(robe.len(), 1, "the robe warned of once: {robe:?}");
    assert!(robe[0].ends_with("of mana left."), "{robe:?}");
    // The server's own word on the hat.
    run.server_says(&[server_line("Your Hat is low on Mana.", MAGIC)]);
    run.advance(1.5);
    assert!(
        run.ui_dump().contains("Last notice: Hat is low on mana."),
        "{}",
        run.ui_dump()
    );
    run.type_line("/track mana");
    assert!(
        printed_with(&run, "[Track] Robe: ") && printed_with(&run, "left (active)."),
        "{:?}",
        run.printed()
    );
    note_asserted(
        "plugins.samples.the-trackers-tell-how-long-each-worn-items-mana-lasts-and-warn-before-it-runs-out",
    );
}

/// The rows of a list in the Trackers' window, each as the dump writes it.
fn list_rows(run: &PluginRun, list: &str) -> Vec<String> {
    let dump = run.ui_dump();
    let line = dump
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("list #{list} ")))
        .unwrap_or_else(|| panic!("no list {list} in:\n{dump}"))
        .to_owned();
    let rows = line.split_once("rows=").map_or("", |(_, r)| r);
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut quoted = false;
    for (i, c) in rows.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => {
                depth += 1;
                if depth == 2 {
                    start = i;
                }
            }
            ']' if !quoted => {
                if depth == 2 {
                    out.push(rows[start..=i].to_owned());
                }
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    out
}

/// Scarabs, kits, salvage and a trade note in the pack: the start of the session is counted, a
/// stack used down is a negative change with its rate, a new bag is a positive one, and the
/// scarabs the player asked to be warned of are warned of once, under the count.
fn the_trackers_count_the_consumables_used_and_warn_when_one_runs_low() {
    const SCARABS: u32 = 0x8000_0301;
    const KIT: u32 = 0x8000_0302;
    const NOTE: u32 = 0x8000_0303;
    const SALVAGE: u32 = 0x8000_0304;
    let held =
        |id: u32, name: &str, obj_type: u32, stack: Option<u16>, value: u32| ScriptedObject {
            id,
            name: name.into(),
            obj_type,
            container: Some(SCRIPTED_PLAYER),
            stack,
            value,
            ..ScriptedObject::default()
        };
    let mut run = trackers(vec![
        held(
            SCARABS,
            "Lead Scarab",
            item_type::SPELL_COMPONENTS,
            Some(12),
            120,
        ),
        ScriptedObject {
            bitfield: bitfield::HEALER,
            ..held(KIT, "Healing Kit", item_type::MISC, None, 50)
        },
        held(
            NOTE,
            "Trade Note (250,000)",
            item_type::PROMISSORY_NOTE,
            None,
            250_000,
        ),
    ]);
    run.type_line("/track low lead scarab 10");
    run.advance(3.0);
    // Four scarabs used; a bag of salvage looted.
    if let Some(w) = run
        .world_mut()
        .tables
        .weenies
        .get_mut(dereth_primitives::ObjectId(SCARABS))
    {
        w.pwd.stack_size = Some(8);
        w.pwd.value = Some(80);
    }
    run.spawn(vec![held(
        SALVAGE,
        "Salvaged Steel",
        item_type::TINKERING_MATERIAL,
        None,
        1_000,
    )]);
    run.advance(1_800.0 - 3.0);
    let rows = list_rows(&run, "items");
    for want in [
        r#"["Lead Scarab",12,8,"-4","-8"]"#,
        r#"["Healing Kit",1,1,"+0","+0"]"#,
        r#"["Trade Note (250,000)",1,1,"+0","+0"]"#,
        r#"["Salvaged Steel",0,1,"+1","+2"]"#,
    ] {
        assert!(rows.iter().any(|r| r == want), "{want} among {rows:?}");
    }
    // Worth: the salvage's 1,000 less the scarabs' 40, in half an hour.
    assert!(
        run.ui_dump().contains("Worth: 960 pyreals (1,9"),
        "{}",
        run.ui_dump()
    );
    let low: Vec<_> = run
        .printed()
        .into_iter()
        .filter(|l| l.starts_with("[Track] Low on"))
        .collect();
    assert_eq!(
        low,
        ["[Track] Low on lead scarab: 8 left (warned under 10)."],
        "warned once"
    );
    note_asserted(
        "plugins.samples.the-trackers-count-the-consumables-used-and-warn-when-one-runs-low",
    );
}

/// The character dies where it stands and its corpse lies there; carried off to the lifestone,
/// the corpse out of view, `/track corpse` points the arrow at it, and does so again after the
/// plugin is loaded afresh, from what it keeps with the character. A creature fallen after your
/// blows is your kill; another player's corpse is a player's.
fn the_trackers_remember_your_corpse_and_point_the_arrow_at_it() {
    const MY_CORPSE: u32 = 0x8000_0401;
    const ITS_CORPSE: u32 = 0x8000_0402;
    const BOB: u32 = 0x5000_0099;
    const BOBS_CORPSE: u32 = 0x8000_0403;
    let corpse = |id: u32, of: &str, ns: f64| ScriptedObject {
        id,
        name: format!("Corpse of {of}"),
        obj_type: item_type::CONTAINER,
        bitfield: bitfield::CORPSE | bitfield::OPENABLE,
        place: Some((ns, 33.8)),
        ..ScriptedObject::default()
    };
    let mut run = trackers(vec![ScriptedObject {
        id: BOB,
        name: "Bob".into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((42.25, 33.85)),
        ..ScriptedObject::default()
    }]);
    run.server_says(&[server_line(
        "You scorch Drudge Skulker for 30 points with Flame Bolt I.",
        MAGIC,
    )]);
    run.spawn(vec![
        corpse(ITS_CORPSE, "Drudge Skulker", 42.21),
        corpse(BOBS_CORPSE, "Bob", 42.22),
    ]);
    run.advance(1.0);
    run.die();
    run.spawn(vec![corpse(MY_CORPSE, "Tester", 42.2)]);
    run.advance(1.5);
    assert!(
        printed_with(&run, "[Track] Your corpse lies at 42.2N, 33.8E."),
        "{:?} {:?}",
        run.printed(),
        run.console(TRACKERS)
    );
    let rows = list_rows(&run, "corpses");
    assert!(
        rows.iter().any(|r| r.starts_with(r#"["Tester","yours""#))
            && rows
                .iter()
                .any(|r| r.starts_with(r#"["Drudge Skulker","your kill""#))
            && rows.iter().any(|r| r.starts_with(r#"["Bob","player""#)),
        "{rows:?}"
    );
    // Off to the lifestone, 0.25 map units (60 m) south; the corpse out of view.
    run.teleport(41.95, 33.8);
    run.remove(MY_CORPSE);
    run.advance(1.0);
    run.type_line("/track corpse");
    run.advance(0.5);
    assert!(
        printed_with(&run, "[Track] Your corpse: 42.2N, 33.8E, 60 m away."),
        "{:?}",
        run.printed()
    );
    let overlays = run.overlays();
    assert!(
        overlays.contains(&format!("arrow {TRACKERS}"))
            && overlays.contains("label=\"Your corpse\"")
            && overlays.contains("distance=60.0"),
        "the arrow at the corpse:\n{overlays}"
    );
    // Loaded afresh: the corpse is remembered with the character.
    run.unload(TRACKERS);
    run.load(read_package(&samples().join("trackers")));
    run.advance(0.5);
    assert!(
        !run.overlays().contains("Your corpse"),
        "the arrow went with the plugin"
    );
    run.type_line("/track corpse");
    run.advance(0.5);
    assert!(
        run.overlays().contains("label=\"Your corpse\""),
        "the arrow again:\n{}",
        run.overlays()
    );
    note_asserted("plugins.samples.the-trackers-remember-your-corpse-and-point-the-arrow-at-it");
}

/// Two players come into view; one walks on and then goes out of view. Each is kept with when
/// and where it was last seen: the one still here as here, the one gone with how long ago and
/// the place it was last seen.
fn the_trackers_keep_each_player_seen_and_where_it_was_last() {
    const ALICE: u32 = 0x5000_00A1;
    const BOB: u32 = 0x5000_00A2;
    let player = |id: u32, name: &str, ns: f64| ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((ns, 33.8)),
        ..ScriptedObject::default()
    };
    let mut run = trackers(vec![player(ALICE, "Alice", 42.25)]);
    run.spawn(vec![player(BOB, "Bob", 42.3)]);
    run.advance(1.0);
    // Bob walks north 0.1 (24 m), then is gone.
    run.move_object(BOB, 42.4, 33.8);
    run.advance(1.0);
    run.remove(BOB);
    run.advance(120.0);
    let rows = list_rows(&run, "players");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows[0].starts_with(r#"["Alice","here","42.2N, 33.8E""#)
            || rows[0].starts_with(r#"["Alice","here","42.3N, 33.8E""#),
        "Alice here, first: {rows:?}"
    );
    assert!(
        rows[1].starts_with(r#"["Bob","2m 0"#) && rows[1].contains(r#"ago","42.4N, 33.8E","-"]"#),
        "Bob gone two minutes, last seen where he walked to: {rows:?}"
    );
    note_asserted("plugins.samples.the-trackers-keep-each-player-seen-and-where-it-was-last");
}
