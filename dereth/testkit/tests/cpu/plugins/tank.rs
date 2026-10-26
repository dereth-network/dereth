//! The Tank sample against VTank: its `/vt` commands, Force Buff, pause, targeting, debuffs,
//! melee, ghosts, routes, looting and buffing, each run from `dereth/plugins/tank` in a scripted
//! world, as VTank's public documentation says VTank does each.

use dereth_client_net::client_session::SessionEvent;
use dereth_plugin_host::PluginFiles;
use dereth_primitives::ObjectId;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{read_package, ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER};

use super::{samples, SharedFiles};

dereth_testkit::scenarios! {
    scenario_the_tank_answers_vtanks_meta_and_profile_commands => the_tank_answers_vtanks_meta_and_profile_commands ["plugins.samples.the-tank-answers-vtanks-meta-and-profile-commands"],
    scenario_the_tank_force_buffs_every_buff_once_held_by_a_pause_and_ended_by_a_cancel => the_tank_force_buffs_every_buff_once_held_by_a_pause_and_ended_by_a_cancel ["plugins.samples.the-tank-force-buffs-every-buff-once-held-by-a-pause-and-ended-by-a-cancel"],
    scenario_the_tank_keeps_a_keep_up_to_rules_items_until_the_pack_holds_its_count => the_tank_keeps_a_keep_up_to_rules_items_until_the_pack_holds_its_count ["plugins.samples.the-tank-keeps-a-keep-up-to-rules-items-until-the-pack-holds-its-count"],
    scenario_the_tank_waits_at_a_checkpoint_until_the_server_places_it_there => the_tank_waits_at_a_checkpoint_until_the_server_places_it_there ["plugins.samples.the-tank-waits-at-a-checkpoint-until-the-server-places-it-there"],
    scenario_the_tank_uses_an_npc_until_it_speaks_and_a_use_npc_waypoints_npc_once => the_tank_uses_an_npc_until_it_speaks_and_a_use_npc_waypoints_npc_once ["plugins.samples.the-tank-uses-an-npc-until-it-speaks-and-a-use-npc-waypoints-npc-once"],
    scenario_the_tank_reads_the_unknown_scrolls_it_can_learn => the_tank_reads_the_unknown_scrolls_it_can_learn ["plugins.samples.the-tank-reads-the-unknown-scrolls-it-can-learn"],
    scenario_the_tanks_monster_expressions_read_species_health_shield_and_settings => the_tanks_monster_expressions_read_species_health_shield_and_settings ["plugins.samples.the-tanks-monster-expressions-read-species-health-shield-and-settings"],
    scenario_the_tank_picks_its_target_by_turning_or_range_and_the_players_selection => the_tank_picks_its_target_by_turning_or_range_and_the_players_selection ["plugins.samples.the-tank-picks-its-target-by-turning-or-range-and-the-players-selection"],
    scenario_the_tank_debuffs_each_monster_first_with_its_extra_vuln_and_recasts_before_they_end => the_tank_debuffs_each_monster_first_with_its_extra_vuln_and_recasts_before_they_end ["plugins.samples.the-tank-debuffs-each-monster-first-with-its-extra-vuln-and-recasts-before-they-end"],
    scenario_the_tank_times_an_item_buff_by_the_characters_age => the_tank_times_an_item_buff_by_the_characters_age ["plugins.samples.the-tank-times-an-item-buff-by-the-characters-age"],
    scenario_the_tank_pauses_and_resumes_its_run_where_it_was => the_tank_pauses_and_resumes_its_run_where_it_was ["plugins.samples.the-tank-pauses-and-resumes-its-run-where-it-was"],
    scenario_the_tank_tries_a_corpse_that_will_not_open_again_then_leaves_it => the_tank_tries_a_corpse_that_will_not_open_again_then_leaves_it ["plugins.samples.the-tank-tries-a-corpse-that-will-not-open-again-then-leaves-it"],
    scenario_the_tank_runs_a_reversed_route_backwards_and_stops_navigating_far_off_it => the_tank_runs_a_reversed_route_backwards_and_stops_navigating_far_off_it ["plugins.samples.the-tank-runs-a-reversed-route-backwards-and-stops-navigating-far-off-it"],
    scenario_the_tank_swings_at_the_power_its_weapon_recklessness_and_attack_bar_say => the_tank_swings_at_the_power_its_weapon_recklessness_and_attack_bar_say ["plugins.samples.the-tank-swings-at-the-power-its-weapon-recklessness-and-attack-bar-say"],
    scenario_the_tank_leaves_a_ghost_monster_alone_for_good => the_tank_leaves_a_ghost_monster_alone_for_good ["plugins.samples.the-tank-leaves-a-ghost-monster-alone-for-good"],
    scenario_the_tank_answers_vtanks_information_and_log_commands => the_tank_answers_vtanks_information_and_log_commands ["plugins.samples.the-tank-answers-vtanks-information-and-log-commands"],
    scenario_the_tank_leaves_out_excluded_buffs_scarabs_and_items_and_rebuffs_together => the_tank_leaves_out_excluded_buffs_scarabs_and_items_and_rebuffs_together ["plugins.samples.the-tank-leaves-out-excluded-buffs-scarabs-and-items-and-rebuffs-together"],
    scenario_the_tank_casts_random_protections_on_players_nearby => the_tank_casts_random_protections_on_players_nearby ["plugins.samples.the-tank-casts-random-protections-on-players-nearby"],
    scenario_the_tank_sets_an_option_in_every_profile => the_tank_sets_an_option_in_every_profile ["plugins.samples.the-tank-sets-an-option-in-every-profile"],
    scenario_the_tank_walks_round_walls_and_through_doors_to_a_monster_and_leaves_one_no_route_reaches => the_tank_walks_round_walls_and_through_doors_to_a_monster_and_leaves_one_no_route_reaches ["plugins.samples.the-tank-walks-round-walls-and-through-doors-to-a-monster-and-leaves-one-no-route-reaches"],
    scenario_the_tank_fights_a_monster_in_sight_before_one_behind_a_wall => the_tank_fights_a_monster_in_sight_before_one_behind_a_wall ["plugins.samples.the-tank-fights-a-monster-in-sight-before-one-behind-a-wall"],
    scenario_the_tank_walks_round_a_wall_to_a_monster_a_step_away_across_it_before_attacking => the_tank_walks_round_a_wall_to_a_monster_a_step_away_across_it_before_attacking ["plugins.samples.the-tank-walks-round-a-wall-to-a-monster-a-step-away-across-it-before-attacking"],
    scenario_the_tank_walks_round_someone_in_the_way_of_its_charge_before_attacking => the_tank_walks_round_someone_in_the_way_of_its_charge_before_attacking ["plugins.samples.the-tank-walks-round-someone-in-the-way-of-its-charge-before-attacking"],
    scenario_the_tank_chooses_its_next_target_itself_when_a_fallen_monsters_body_goes_mid_swing => the_tank_chooses_its_next_target_itself_when_a_fallen_monsters_body_goes_mid_swing ["plugins.samples.the-tank-chooses-its-next-target-itself-when-a-fallen-monsters-body-goes-mid-swing"],
    scenario_the_tank_walks_on_toward_a_monster_hidden_below_a_ramps_edge_until_it_sees_it => the_tank_walks_on_toward_a_monster_hidden_below_a_ramps_edge_until_it_sees_it ["plugins.samples.the-tank-walks-on-toward-a-monster-it-cannot-see-until-it-does"],
    scenario_the_tank_walks_a_route_leg_round_a_corner_along_the_clients_route => the_tank_walks_a_route_leg_round_a_corner_along_the_clients_route ["plugins.samples.the-tank-walks-a-route-leg-round-a-corner-along-the-clients-route"],
    scenario_the_tank_takes_up_a_route_set_again_mid_leg_from_its_nearest_waypoint => the_tank_takes_up_a_route_set_again_mid_leg_from_its_nearest_waypoint ["plugins.samples.the-tank-takes-up-a-route-set-again-mid-leg-from-its-nearest-waypoint"],
    scenario_the_tank_rings_a_crowd_and_casts_streaks_and_arcs_as_its_rows_and_settings_say => the_tank_rings_a_crowd_and_casts_streaks_and_arcs_as_its_rows_and_settings_say ["plugins.samples.the-tank-rings-a-crowd-and-casts-streaks-and-arcs-as-its-rows-and-settings-say"],
    scenario_the_tank_casts_void_curses_and_damage_over_time_and_drains_and_harms => the_tank_casts_void_curses_and_damage_over_time_and_drains_and_harms ["plugins.samples.the-tank-casts-void-curses-and-damage-over-time-and-drains-and-harms"],
    scenario_the_tank_debuffs_with_a_wand_before_melee_and_holds_a_shield_beside_its_sword => the_tank_debuffs_with_a_wand_before_melee_and_holds_a_shield_beside_its_sword ["plugins.samples.the-tank-debuffs-with-a-wand-before-melee-and-holds-a-shield-beside-its-sword"],
    scenario_the_tank_dispels_a_heavy_vulnerability_with_an_item_or_its_spell => the_tank_dispels_a_heavy_vulnerability_with_an_item_or_its_spell ["plugins.samples.the-tank-dispels-a-heavy-vulnerability-with-an-item-or-its-spell"],
    scenario_the_tank_takes_up_the_weapon_for_a_monsters_damage_and_a_striking_one_on_a_vulned_monster => the_tank_takes_up_the_weapon_for_a_monsters_damage_and_a_striking_one_on_a_vulned_monster ["plugins.samples.the-tank-takes-up-the-weapon-for-a-monsters-damage-and-a-striking-one-on-a-vulned-monster"],
    scenario_the_tank_throws_grenades_at_a_crowd_casts_fists_ahead_and_picks_a_random_damage_type => the_tank_throws_grenades_at_a_crowd_casts_fists_ahead_and_picks_a_random_damage_type ["plugins.samples.the-tank-throws-grenades-at-a-crowd-casts-fists-ahead-and-picks-a-random-damage-type"],
    scenario_the_tank_side_steps_its_casts_looks_again_after_turning_and_backs_off_to_buff => the_tank_side_steps_its_casts_looks_again_after_turning_and_backs_off_to_buff ["plugins.samples.the-tank-side-steps-its-casts-looks-again-after-turning-and-backs-off-to-buff"],
    scenario_a_metas_chat_capture_takes_the_colors_asked_and_its_persistent_seconds_outlast_a_stop => a_metas_chat_capture_takes_the_colors_asked_and_its_persistent_seconds_outlast_a_stop ["plugins.automation.a-metas-chat-capture-takes-the-colors-asked-and-its-persistent-seconds-outlast-a-stop"],
}

pub(crate) const TANK: &str = "network.dereth.tank";

/// A Tank run in a scripted world that allows its automation, its data folder `files`, its run
/// `run` (made by the caller to add its scripted spells), the world's objects `objects`.
pub(crate) fn tank_in(
    mut run: PluginRun,
    files: &SharedFiles,
    objects: Vec<ScriptedObject>,
) -> PluginRun {
    run = run.with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("tank")));
    run.scripted_world("Tester", objects);
    run.advance(0.5);
    run
}

/// A Tank in a scripted world of `objects`, its data folder shared with the test.
pub(crate) fn tank(objects: Vec<ScriptedObject>) -> (PluginRun, SharedFiles) {
    let files = SharedFiles::default();
    let run = tank_in(PluginRun::new(), &files, objects);
    (run, files)
}

/// A monster of the scripted world at map coordinates `(ns, ew)`.
pub(crate) fn monster(id: u32, name: &str, ns: f64, ew: f64) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::ATTACKABLE,
        place: Some((ns, ew)),
        ..ScriptedObject::default()
    }
}

/// An item in the character's pack.
pub(crate) fn held(id: u32, name: &str, obj_type: u32) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(SCRIPTED_PLAYER),
        ..ScriptedObject::default()
    }
}

/// A wand in the pack, which the Tank takes up to cast.
pub(crate) fn wand() -> ScriptedObject {
    ScriptedObject {
        locations: Some(0x0100_0000),
        ..held(0x8000_00B0, "Wand", item_type::CASTER)
    }
}

/// Whether a console line contains `s`.
pub(crate) fn logged(run: &PluginRun, s: &str) -> bool {
    run.console(TANK).iter().any(|l| l.contains(s))
}

/// How many messages the client sent that read `s`.
pub(crate) fn sent_count(run: &PluginRun, s: &str) -> usize {
    run.sent().iter().filter(|x| *x == s).count()
}

/// The Tank fights `monster` until it swings at it; the monster then falls and its corpse
/// `corpse` comes where it stood, with `contents` in it.
pub(crate) fn kill(
    run: &mut PluginRun,
    monster: u32,
    corpse: ScriptedObject,
    contents: Vec<ScriptedObject>,
) {
    let swing = format!("melee_attack {monster:#x}");
    for _ in 0..60 {
        if run.sent().contains(&swing) {
            break;
        }
        run.advance(0.5);
    }
    assert!(
        run.sent().contains(&swing),
        "a swing at the monster: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    run.remove(monster);
    let mut all = vec![corpse];
    all.extend(contents);
    run.spawn(all);
}

/// A corpse of `name` at map coordinates `(ns, ew)`: a container that opens.
pub(crate) fn corpse(id: u32, name: &str, ns: f64, ew: f64) -> ScriptedObject {
    ScriptedObject {
        id,
        name: format!("Corpse of {name}"),
        obj_type: item_type::CONTAINER,
        bitfield: bitfield::CORPSE | bitfield::OPENABLE | bitfield::STUCK,
        place: Some((ns, ew)),
        ..ScriptedObject::default()
    }
}

/// An item in container `container`.
pub(crate) fn inside(id: u32, name: &str, obj_type: u32, container: u32) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        container: Some(container),
        ..ScriptedObject::default()
    }
}

/// One rule of a meta: its condition kind and its data's cells, its action kind and its data's
/// cells, and its state; each cell a type tag and its value.
pub(crate) type Rule<'a> = (i32, Vec<String>, i32, Vec<String>, &'a str);

/// A VTank meta file's text, in its own layout: its rules.
pub(crate) fn met(rules: &[Rule<'_>]) -> String {
    let mut out: Vec<String> = [
        "1", "CondAct", "5", "CType", "AType", "CData", "AData", "State", "n", "n", "n", "n", "n",
    ]
    .map(str::to_owned)
    .to_vec();
    out.push(rules.len().to_string());
    for (c, cd, a, ad, state) in rules {
        out.extend(["i".to_owned(), c.to_string(), "i".to_owned(), a.to_string()]);
        out.extend(cd.iter().cloned());
        out.extend(ad.iter().cloned());
        out.extend(["s".to_owned(), (*state).to_owned()]);
    }
    out.join("\r\n") + "\r\n"
}

pub(crate) fn int(v: i64) -> Vec<String> {
    vec!["i".into(), v.to_string()]
}

pub(crate) fn text(v: &str) -> Vec<String> {
    vec!["s".into(), v.into()]
}

// The meta's condition and action kinds, by the numbers the file stores.
pub(crate) const NEVER: i32 = 0;
pub(crate) const ALWAYS: i32 = 1;
const DIED: i32 = 8;
pub(crate) const NONE: i32 = 0;
const SET_STATE: i32 = 1;
pub(crate) const CHAT: i32 = 2;
const WALK: i32 = 4;

/// VTank's commands a meta types, answered by the Tank: the meta's state set, a death faked, an
/// expression run and its variable listed (the meta functions weighing a spell against the
/// character's skill and reading the macro's own settings), a line echoed, an option read back (one
/// the Tank has not got too), another meta loaded in place of the running one and saved under
/// another name, the loot profile switched and saved, the settings saved as a profile and as one
/// of the character's own and that read back, and the meta functions listed.
fn the_tank_answers_vtanks_meta_and_profile_commands() {
    let files = SharedFiles::default();
    let mut put = files.clone();
    let a = met(&[
        (NEVER, int(0), NONE, int(0), "Default"),
        (DIED, int(0), SET_STATE, text("Dead"), "Hunt"),
    ]);
    let b = met(&[(ALWAYS, int(0), CHAT, text("/vt echo b runs"), "Default")]);
    put.write(TANK, "metas/a.met", a.as_bytes())
        .expect("in memory");
    put.write(TANK, "metas/b.met", b.as_bytes())
        .expect("in memory");
    let mut run = tank_in(PluginRun::new(), &files, Vec::new());
    run.set_skill("Creature Enchantment", 250);
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank meta a.met",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(1.0);
    for line in [
        "/tank vt setmetastate Hunt",
        "/tank vt fakedeath",
        "/tank vt mexec $x=2+3",
        "/tank vt dumpmetavars",
        "/tank vt echo plain words",
        "/tank vt opt set WhoYouGonnaCall true",
        "/tank vt opt get WhoYouGonnaCall",
        "/tank vt opt get AttackPower",
        // Might of the Lugians is pitched at 300: not within the hunting threshold of 250; Strength
        // Self I, at 1, within the buffing one.
        "/tank vt mexec getcancastspell_hunt[2087]",
        "/tank vt mexec getcancastspell_buff[1]",
        "/tank vt mexec vtgetsetting[AttackPower]",
    ] {
        run.type_line(line);
        run.advance(0.5);
    }
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l == s);
    assert!(said("[Tank] meta state Hunt"), "{printed:#?}");
    assert!(
        logged(&run, "meta: Default -> Hunt"),
        "{:#?}",
        run.console(TANK)
    );
    assert!(
        logged(&run, "meta: Hunt -> Dead"),
        "the faked death moves the meta on: {:#?}",
        run.console(TANK)
    );
    assert!(said("[Tank] mexec: 5"), "{printed:#?}");
    assert!(said("[Tank] x = 5"), "{printed:#?}");
    assert!(said("plain words"), "{printed:#?}");
    assert!(
        said("[Tank] WhoYouGonnaCall = true"),
        "kept, though the Tank has no such option: {printed:#?}"
    );
    assert!(said("[Tank] AttackPower = 60"), "{printed:#?}");
    assert!(
        said("[Tank] mexec: False") && said("[Tank] mexec: True") && said("[Tank] mexec: 60"),
        "the skill against the thresholds, and the macro's own setting: {printed:#?}"
    );
    // Another meta in place of the one running; saved as another file.
    run.type_line("/tank vt meta load b");
    run.advance(1.0);
    assert!(
        run.printed().iter().any(|l| l == "b runs"),
        "the new meta runs: {:#?}",
        run.printed()
    );
    for line in [
        "/tank vt meta save c",
        "/tank vt loot load basic",
        "/tank vt loot save mine",
        "/tank vt settings save one",
        "/tank vt settings savechar two",
        "/tank vt settings loadchar two",
        "/tank vt listmetafunctions",
    ] {
        run.type_line(line);
        run.advance(0.5);
    }
    let read = |path: &str| {
        files
            .clone()
            .read(TANK, path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    assert_eq!(read("metas/c.met"), Some(b.clone()), "the meta saved as c");
    assert_eq!(
        read("profiles/mine.utl"),
        std::fs::read_to_string(samples().join("tank/profiles/basic.utl")).ok(),
        "the loot profile saved as mine"
    );
    assert!(read("one.usd").is_some_and(|t| t.contains("Settings")));
    assert!(read("characters/Tester/two.usd").is_some_and(|t| t.contains("Settings")));
    let printed = run.printed();
    assert!(
        printed.iter().any(|l| l == "[Tank] loot profile basic.utl"),
        "{printed:#?}"
    );
    assert!(
        printed
            .iter()
            .any(|l| l.starts_with("[Tank] settings characters/Tester/two.usd: ")),
        "{printed:#?}"
    );
    assert!(
        printed
            .iter()
            .any(|l| l.starts_with("[Tank] ") && l.ends_with(" meta functions:")),
        "{printed:#?}"
    );
    note_asserted("plugins.samples.the-tank-answers-vtanks-meta-and-profile-commands");
}

/// VTank's Force Buff: every buff is cast again at once, whatever time it has left, each once; a
/// pause holds the sequence and the resumed run carries it on without casting again what it has
/// cast; a cancel ends it; and with the macro stopped, a run for the buffs alone casts them and
/// stops.
fn the_tank_force_buffs_every_buff_once_held_by_a_pause_and_ended_by_a_cancel() {
    let (mut run, _) = tank(vec![wand()]);
    run.casts_land(true);
    let buffs = [
        "cast 2087 0x50000001",
        "cast 1133 0x50000001",
        "cast 2215 0x50000001",
    ];
    let counts = |run: &PluginRun| buffs.map(|b| sent_count(run, b));
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank start");
    run.advance(4.0);
    assert_eq!(counts(&run), [1, 1, 1], "{:?}", run.sent());
    // Each is up: a force buff casts each again, once.
    run.type_line("/tank forcebuff");
    run.advance(4.0);
    assert_eq!(counts(&run), [2, 2, 2], "{:#?}", run.console(TANK));
    assert!(logged(&run, "force buff: done, 3 buffs cast"));
    // Paused after its first cast, it waits; resumed, it casts the other two only.
    run.type_line("/tank forcebuff");
    for _ in 0..40 {
        if counts(&run)[0] == 3 {
            break;
        }
        run.frame();
    }
    run.type_line("/tank pause");
    let held = counts(&run);
    run.advance(4.0);
    assert_eq!(counts(&run), held, "nothing cast while paused");
    run.type_line("/tank start");
    run.advance(4.0);
    assert_eq!(
        counts(&run),
        [3, 3, 3],
        "resumed where it left off: {:#?}",
        run.console(TANK)
    );
    // Cancelled while paused: nothing is cast once resumed.
    run.type_line("/tank pause");
    run.type_line("/tank forcebuff");
    run.type_line("/tank cancelforcebuff");
    run.type_line("/tank start");
    run.advance(4.0);
    assert_eq!(counts(&run), [3, 3, 3], "{:#?}", run.console(TANK));
    // With the macro stopped: a run for the buffs alone, which stops when they are cast.
    run.type_line("/tank stop");
    run.advance(1.0);
    run.type_line("/tank forcebuff");
    run.advance(5.0);
    assert_eq!(counts(&run), [4, 4, 4], "{:#?}", run.console(TANK));
    assert!(
        logged(&run, "stopped: the force buff is done"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-force-buffs-every-buff-once-held-by-a-pause-and-ended-by-a-cancel",
    );
}

/// A loot profile's keep-up-to rule: the Tank takes the rule's items from a corpse until the pack
/// holds the rule's count, counting what it held before, and leaves the rest.
fn the_tank_keeps_a_keep_up_to_rules_items_until_the_pack_holds_its_count() {
    const MONSTER: u32 = 0x8000_0080;
    const CORPSE: u32 = 0x8000_0081;
    // Any gem (class 11): keep up to 2.
    let profile = "UTL\r\n1\r\n1\r\nGems\r\n\r\n0;10;7\r\n2\r\n4\r\n11\r\n";
    let files = SharedFiles::default();
    files
        .clone()
        .write(TANK, "profiles/gems.utl", profile.as_bytes())
        .expect("in memory");
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![
            monster(MONSTER, "Drudge Skulker", 42.2, 33.8001),
            held(0x8000_00E0, "Opal", item_type::GEM),
        ],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank profile gems.utl",
        "/tank start",
    ] {
        run.type_line(line);
    }
    let gems = [0x8000_0083, 0x8000_0084, 0x8000_0085];
    kill(
        &mut run,
        MONSTER,
        corpse(CORPSE, "Drudge Skulker", 42.2, 33.8001),
        gems.iter()
            .map(|id| inside(*id, "Black Garnet", item_type::GEM, CORPSE))
            .collect(),
    );
    run.advance(15.0);
    let taken = gems
        .iter()
        .filter(|g| run.container_of(**g) == Some(SCRIPTED_PLAYER))
        .count();
    assert_eq!(
        taken,
        1,
        "one gem taken, to the count of two: {:#?}",
        run.console(TANK)
    );
    assert!(
        logged(&run, "left Black Garnet: 2 held of Gems's 2"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-keeps-a-keep-up-to-rules-items-until-the-pack-holds-its-count",
    );
}

/// The server's word on where the character stands (`Movement_PositionEvent`, `0xF748`), at map
/// coordinates `(ns, ew)`.
fn server_places_player(ns: f64, ew: f64) -> SessionEvent {
    use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
    use dereth_protocol::types::space::{Origin, Vec3};
    let p = dereth_plugin_host::places::from_coords(ns, ew, 0.0).expect("on the map");
    ui_event(&MovementPositionEvent {
        id: ObjectId(SCRIPTED_PLAYER),
        position: PositionPack {
            origin: Origin {
                objcell_id: p.cell.0,
                origin: Vec3 {
                    x: p.frame.origin.x,
                    y: p.frame.origin.y,
                    z: p.frame.origin.z,
                },
            },
            ..PositionPack::default()
        },
    })
}

/// A checkpoint waypoint (VTank's): the Tank walks to it and waits there, stepping forward now
/// and then, until the server places the character on it; then the route goes on.
fn the_tank_waits_at_a_checkpoint_until_the_server_places_it_there() {
    let (mut run, _) = tank(Vec::new());
    run.type_line("/tank route new once");
    run.type_line("/tank route add");
    run.place_player(42.2, 33.81);
    run.type_line("/tank route checkpoint");
    run.place_player(42.2, 33.8);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(15.0);
    assert!(
        !logged(&run, "route: finished"),
        "it waits for the server: {:#?}",
        run.console(TANK)
    );
    assert!(
        run.moves().iter().any(|m| m == "hold forward"),
        "it steps forward while it waits: {:?}",
        run.moves()
    );
    run.server_says(&[server_places_player(42.2, 33.81)]);
    run.advance(2.0);
    assert!(
        logged(&run, "route: the server has the character at checkpoint 2")
            && logged(&run, "route: finished"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-waits-at-a-checkpoint-until-the-server-places-it-there",
    );
}

/// An NPC Talk waypoint (VTank's) uses the NPC again and again until the NPC tells the character
/// something; a Use Portal/NPC waypoint naming an NPC uses it once and goes on, waiting for no
/// teleport.
fn the_tank_uses_an_npc_until_it_speaks_and_a_use_npc_waypoints_npc_once() {
    use dereth_protocol::comms::CommunicationHearDirectSpeech;
    const NPC: u32 = 0x8000_0070;
    let crier = ScriptedObject {
        id: NPC,
        name: "Town Crier".into(),
        obj_type: item_type::CREATURE,
        // An NPC stands where it is: used, it is not picked up.
        bitfield: bitfield::STUCK,
        place: Some((42.2, 33.81)),
        ..ScriptedObject::default()
    };
    let (mut run, _) = tank(vec![crier]);
    run.type_line("/tank route new once");
    run.type_line("/tank route add");
    run.select(NPC);
    run.type_line("/tank route talk");
    run.type_line("/tank route portal");
    run.select(SCRIPTED_PLAYER);
    run.type_line("/tank route points");
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Tank] Route: once, 3 waypoints: point, use_npc, portal"),
        "{:#?}",
        run.printed()
    );
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank start",
    ] {
        run.type_line(line);
    }
    let used = format!("use {NPC:#x}");
    run.advance(20.0);
    let before = sent_count(&run, &used);
    assert!(
        before >= 2 && !logged(&run, "route: finished"),
        "used again and again until it speaks: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    run.server_says(&[ui_event(&CommunicationHearDirectSpeech {
        message: "Hear ye!".into(),
        sender_name: "Town Crier".into(),
        sender_id: ObjectId(NPC),
        target_id: ObjectId(SCRIPTED_PLAYER),
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })]);
    run.advance(5.0);
    assert!(
        logged(&run, "route: Town Crier told the character something")
            && logged(&run, "route: finished"),
        "{:#?}",
        run.console(TANK)
    );
    // The NPC Talk waypoint's last use, then the Use Portal/NPC waypoint's one.
    assert!(sent_count(&run, &used) <= before + 2, "{:?}", run.sent());
    assert!(
        !logged(&run, "did not take the character"),
        "no teleport waited for"
    );
    note_asserted(
        "plugins.samples.the-tank-uses-an-npc-until-it-speaks-and-a-use-npc-waypoints-npc-once",
    );
}

/// ReadUnknownScrolls: of a corpse's scrolls the profile does not keep, the Tank takes and reads
/// one of a spell the character does not know and can learn (its skill within 15 of the spell's
/// difficulty), after the corpse is looted; one of a spell it knows it leaves. Switched off, it
/// reads none.
fn the_tank_reads_the_unknown_scrolls_it_can_learn() {
    const MONSTER: u32 = 0x8000_0080;
    const CORPSE: u32 = 0x8000_0081;
    const UNKNOWN: u32 = 0x8000_0086;
    const KNOWN: u32 = 0x8000_0087;
    let scroll = |id: u32, name: &str, spell: u32| ScriptedObject {
        dids: vec![(28, spell)],
        ..inside(id, name, item_type::WRITABLE, CORPSE)
    };
    let hunt = |read: bool| {
        let files = SharedFiles::default();
        let mut run = tank_in(
            PluginRun::new().with_spells(&[(3000, "Fire Vulnerability Other I", 2)], false),
            &files,
            vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001)],
        );
        run.set_skill("Life Magic", 50);
        for line in [
            "/tank set EnableNav false",
            "/tank set EnableBuffing false",
            &format!("/tank set ReadUnknownScrolls {read}"),
            "/tank profile basic.utl",
            "/tank start",
        ] {
            run.type_line(line);
        }
        kill(
            &mut run,
            MONSTER,
            corpse(CORPSE, "Drudge Skulker", 42.2, 33.8001),
            vec![
                scroll(UNKNOWN, "Scroll of Fire Vulnerability Other I", 3000),
                scroll(KNOWN, "Scroll of Imperil Other I", 23),
            ],
        );
        run.advance(15.0);
        run
    };
    let run = hunt(true);
    let sent = run.sent();
    let read = sent.iter().position(|s| *s == format!("use {UNKNOWN:#x}"));
    let looted = run
        .console(TANK)
        .iter()
        .position(|l| l.contains("looted Corpse of Drudge Skulker"));
    assert!(
        read.is_some() && run.container_of(UNKNOWN) == Some(SCRIPTED_PLAYER),
        "the unknown scroll taken and read: {sent:?} (console {:#?})",
        run.console(TANK)
    );
    assert!(looted.is_some(), "{:#?}", run.console(TANK));
    assert!(
        logged(
            &run,
            "read Scroll of Fire Vulnerability Other I (Fire Vulnerability Other I)"
        ),
        "{:#?}",
        run.console(TANK)
    );
    assert!(
        !sent.contains(&format!("use {KNOWN:#x}")) && run.container_of(KNOWN) == Some(CORPSE),
        "the known spell's scroll is left: {sent:?}"
    );
    let off = hunt(false);
    assert!(
        !off.sent().contains(&format!("use {UNKNOWN:#x}")),
        "switched off, none is read: {:?}",
        off.sent()
    );
    note_asserted("plugins.samples.the-tank-reads-the-unknown-scrolls-it-can-learn");
}

/// The monster list's expressions read VTank's variables: a species by its name and a maximum
/// health from the monster's appraisal make a drudge the first fought; a monster of a species, one
/// wielding a shield and one a setting rules out are left alone.
fn the_tanks_monster_expressions_read_species_health_shield_and_settings() {
    // Ids in the order a search finds them; monsters of one priority the character turns equally
    // to face are fought in that order.
    const MOSSWART: u32 = 0x8000_008D;
    const BOG: u32 = 0x8000_008E;
    const RAT: u32 = 0x8000_008F;
    const BEARER: u32 = 0x8000_0091;
    const SHIELD: u32 = 0x8000_0092;
    const PROWLER: u32 = 0x8000_0095;
    let (mut run, _) = tank(vec![
        monster(MOSSWART, "Mosswart", 42.2, 33.8001),
        // Of the mosswarts' species (4).
        ScriptedObject {
            ints: vec![(2, 4)],
            ..monster(BOG, "Bog Thing", 42.2, 33.8002)
        },
        monster(RAT, "Rat", 42.2, 33.8003),
        monster(BEARER, "Shield Bearer", 42.2, 33.8004),
        ScriptedObject {
            id: SHIELD,
            name: "Buckler".into(),
            obj_type: item_type::ARMOR,
            ..ScriptedObject::default()
        },
        // A drudge (species 3) of 150 health.
        ScriptedObject {
            ints: vec![(2, 3)],
            health: Some((150, 150)),
            ..monster(PROWLER, "Drudge Prowler", 42.2, 33.8005)
        },
    ]);
    // The bearer wields its shield, as the server describes a creature's gear.
    if let Some(w) = run.world_mut().tables.weenies.get_mut(ObjectId(SHIELD)) {
        w.pwd.wielder_id = Some(ObjectId(BEARER));
    }
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank monster priority 3 species==Drudge&&maxhp>=100",
        "/tank monster priority -1 species==Mosswart",
        "/tank monster priority -1 hasshield==true",
        "/tank monster priority -1 setting_AttackPower>50&&name==Rat",
    ] {
        run.type_line(line);
    }
    // Each looked at first: what only an appraisal tells is asked for and comes.
    for id in [MOSSWART, BOG, RAT, BEARER, PROWLER] {
        run.select(id);
        run.type_line("/tank vt testmonster");
        run.advance(0.3);
    }
    run.type_line("/tank start");
    run.advance(12.0);
    let console = run.console(TANK);
    let first = console
        .iter()
        .find(|l| l.contains(" target "))
        .cloned()
        .unwrap_or_default();
    assert!(
        first.contains("target Drudge Prowler"),
        "the drudge first, by its species and health: {console:#?}"
    );
    let sent = run.sent();
    let swung = |id: u32| sent.contains(&format!("melee_attack {id:#x}"));
    assert!(swung(MOSSWART), "{sent:?}");
    assert!(
        !swung(BOG) && !swung(BEARER) && !swung(RAT),
        "the mosswart by species, the shield bearer and the rat left alone: {sent:?}"
    );
    note_asserted(
        "plugins.samples.the-tanks-monster-expressions-read-species-health-shield-and-settings",
    );
}

/// TargetSelectMethod: of two monsters of one priority, by range the nearer is fought first, by
/// turning the one the character faces; and with TargetLock, the monster the player selects.
fn the_tank_picks_its_target_by_turning_or_range_and_the_players_selection() {
    const AHEAD: u32 = 0x8000_0090;
    const BESIDE: u32 = 0x8000_0091;
    let first_target = |lines: &[&str], select: bool| {
        // One 6 m north (ahead of the character, which faces north), one 3 m east.
        let (mut run, _) = tank(vec![
            monster(AHEAD, "Drudge Ahead", 42.2 + 6.0 / 240.0, 33.8),
            monster(BESIDE, "Drudge Beside", 42.2, 33.8 + 3.0 / 240.0),
        ]);
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank set EnableBuffing false");
        for line in lines {
            run.type_line(line);
        }
        if select {
            run.select(BESIDE);
        }
        run.type_line("/tank start");
        run.advance(3.0);
        let console = run.console(TANK);
        console
            .iter()
            .find(|l| l.contains("target Drudge"))
            .cloned()
            .unwrap_or_else(|| panic!("no target: {console:#?}"))
    };
    assert!(first_target(&["/tank set TargetSelectMethod 1"], false).contains("Drudge Beside"));
    assert!(first_target(&["/tank set TargetSelectMethod 2"], false).contains("Drudge Ahead"));
    assert!(
        first_target(&[], false).contains("Drudge Ahead"),
        "both: by turning near by"
    );
    assert!(
        first_target(
            &[
                "/tank set TargetSelectMethod 2",
                "/tank set TargetLock true"
            ],
            true
        )
        .contains("Drudge Beside"),
        "the selected monster first"
    );
    note_asserted(
        "plugins.samples.the-tank-picks-its-target-by-turning-or-range-and-the-players-selection",
    );
}

/// DebuffEachFirst: with Priority, every monster of the target's priority in reach is debuffed
/// before the first attack spell; a row's extra vulnerability (VTank's Ex. Vuln) is cast with its
/// debuffs; and a debuff is cast again DebuffPrecastSeconds before its spell's duration runs out.
fn the_tank_debuffs_each_monster_first_with_its_extra_vuln_and_recasts_before_they_end() {
    const DRUDGE: u32 = 0x8000_0090;
    const MITE: u32 = 0x8000_0091;
    let fight = |lines: &[&str], secs: f64| {
        let files = SharedFiles::default();
        let mut run = tank_in(
            PluginRun::new().with_spells(&[(3001, "Fire Vulnerability Other I", 2)], true),
            &files,
            vec![
                monster(DRUDGE, "Drudge", 42.2, 33.8001),
                monster(MITE, "Mite", 42.2, 33.8002),
                wand(),
            ],
        );
        for line in [
            "/tank set EnableNav false",
            "/tank set EnableBuffing false",
            "/tank set Weapon Wand",
            "/tank monster imperil on <DEFAULT>",
        ] {
            run.type_line(line);
        }
        for line in lines {
            run.type_line(line);
        }
        run.type_line("/tank start");
        run.advance(secs);
        run
    };
    let run = fight(
        &[
            "/tank set DebuffEachFirst 2",
            "/tank monster imperil on Drudge",
            "/tank monster exvuln fire Drudge",
        ],
        8.0,
    );
    let console = run.console(TANK);
    let at = |s: &str| console.iter().position(|l| l.ends_with(s));
    let first_bolt = run.sent().iter().position(|s| s == "cast 27");
    let imperils = run.sent().iter().filter(|s| *s == "cast 23").count();
    let bolts_after =
        first_bolt.map(|b| run.sent()[..b].iter().filter(|s| *s == "cast 23").count());
    assert!(
        at("Imperil Other I on Drudge").is_some() && at("Imperil Other I on Mite").is_some(),
        "both imperiled: {console:#?}"
    );
    assert_eq!(
        bolts_after,
        Some(2),
        "both imperiled before the first bolt ({imperils} imperils): {:?}",
        run.sent()
    );
    assert!(
        at("Fire Vulnerability Other I on Drudge").is_some(),
        "the extra vuln: {console:#?}"
    );
    // A debuff of a half-hour spell, cast again so long before it ends that it is due at once.
    let run = fight(&["/tank set DebuffPrecastSeconds 1797"], 12.0);
    let recast = run
        .console(TANK)
        .iter()
        .filter(|l| l.contains("Imperil Other I on "))
        .count();
    assert!(recast >= 2, "{:#?}", run.console(TANK));
    note_asserted(
        "plugins.samples.the-tank-debuffs-each-monster-first-with-its-extra-vuln-and-recasts-before-they-end",
    );
}

/// The character's age (its integer property 125, seconds played).
fn set_age(run: &mut PluginRun, age: i32) {
    if let Some(q) = run.world_mut().player_qualities_mut() {
        q.ints.get_or_insert_with(Default::default).insert(125, age);
    }
    run.frame();
}

/// An item buff's time runs as the character plays (its age), not by the clock: the Tank casts it
/// again once the character has played the spell's duration less RebuffTimeRemainingSeconds
/// since, and not before.
fn the_tank_times_an_item_buff_by_the_characters_age() {
    const ARMOR: u32 = 0x8000_00C0;
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_item_spells(&[(51, "Impenetrability I")]),
        &files,
        vec![
            ScriptedObject {
                locations: Some(0x0000_0200),
                bitfield: bitfield::ATTACKABLE,
                worn: true,
                ..held(ARMOR, "Breastplate", item_type::ARMOR)
            },
            wand(),
        ],
    );
    run.casts_land(true);
    set_age(&mut run, 1000);
    for line in [
        "/tank set EnableNav false",
        "/tank set BuffSpells Nothing",
        "/tank start",
    ] {
        run.type_line(line);
    }
    let cast = format!("cast 51 {ARMOR:#x}");
    run.advance(10.0);
    assert_eq!(sent_count(&run, &cast), 1, "{:#?}", run.console(TANK));
    // Played 1,000 s of its 1,800: not yet.
    set_age(&mut run, 2000);
    run.advance(5.0);
    assert_eq!(sent_count(&run, &cast), 1, "{:#?}", run.console(TANK));
    // Played 1,600 s: within the 300 s it is recast before it ends.
    set_age(&mut run, 2600);
    run.advance(8.0);
    assert_eq!(sent_count(&run, &cast), 2, "{:#?}", run.console(TANK));
    note_asserted("plugins.samples.the-tank-times-an-item-buff-by-the-characters-age");
}

/// The number of attacks the run's tally counts, as `/tank status` tells it.
fn attacks(run: &mut PluginRun) -> u32 {
    run.type_line("/tank status");
    let line = run
        .printed()
        .iter()
        .rev()
        .find(|l| l.starts_with("[Tank] "))
        .cloned()
        .unwrap_or_default();
    line.split(", ")
        .find_map(|part| part.strip_suffix(" attacks"))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no attacks in {line:?}"))
}

/// VTank's Run Macro switch is a pause: paused, the Tank does nothing and the plugin bar says so;
/// started again, it resumes the same run (its tally kept) rather than starting a new one.
fn the_tank_pauses_and_resumes_its_run_where_it_was() {
    const MONSTER: u32 = 0x8000_0080;
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_repeating_attacks(),
        &files,
        vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001)],
    );
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set EnableBuffing false");
    run.type_line("/tank start");
    run.advance(6.0);
    assert!(attacks(&mut run) > 0, "{:#?}", run.console(TANK));
    run.type_line("/tank pause");
    run.advance(1.5);
    let paused = attacks(&mut run);
    run.advance(5.0);
    assert_eq!(attacks(&mut run), paused, "nothing while paused");
    assert!(
        run.bar()
            .iter()
            .any(|l| l.starts_with("network.dereth.tank	paused	")),
        "{:?}",
        run.bar()
    );
    run.type_line("/tank start");
    run.advance(4.0);
    assert!(
        attacks(&mut run) > paused,
        "it fights on, its tally kept: {:#?}",
        run.console(TANK)
    );
    let started = run
        .console(TANK)
        .iter()
        .filter(|l| l.contains(" started: "))
        .count();
    assert_eq!(started, 1, "the same run: {:#?}", run.console(TANK));
    assert!(logged(&run, "resumed: by you"));
    note_asserted("plugins.samples.the-tank-pauses-and-resumes-its-run-where-it-was");
}

/// A kill's corpse that does not open is tried again CorpseOpenTimeoutSeconds later, and after
/// BlacklistCorpseOpenAttemptCount tries left alone.
fn the_tank_tries_a_corpse_that_will_not_open_again_then_leaves_it() {
    const MONSTER: u32 = 0x8000_0080;
    const CORPSE: u32 = 0x8000_0081;
    let (mut run, _) = tank(vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001)]);
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank set CorpseOpenTimeoutSeconds 3",
        "/tank set BlacklistCorpseOpenAttemptCount 3",
        "/tank profile basic.utl",
        "/tank start",
    ] {
        run.type_line(line);
    }
    // A corpse the server shows nothing of when it is used.
    kill(
        &mut run,
        MONSTER,
        ScriptedObject {
            obj_type: item_type::MISC,
            ..corpse(CORPSE, "Drudge Skulker", 42.2, 33.8001)
        },
        Vec::new(),
    );
    run.advance(25.0);
    let used = sent_count(&run, &format!("use {CORPSE:#x}"));
    assert_eq!(
        used,
        3,
        "{:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    assert!(
        logged(
            &run,
            "Corpse of Drudge Skulker would not open (3 tries): left for 600 s"
        ),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-tries-a-corpse-that-will-not-open-again-then-leaves-it",
    );
}

/// VTank's reverse route runs the route's points the other way round: from the first point the
/// Tank goes to the last. And a character further off its route than NavFarStopRange has
/// navigation turned off.
fn the_tank_runs_a_reversed_route_backwards_and_stops_navigating_far_off_it() {
    let route = |run: &mut PluginRun| {
        // The start; a point 24 m north; one 24 m east.
        run.type_line("/tank route new circular");
        run.type_line("/tank route add 42.2N 33.8E");
        run.type_line("/tank route add 42.3N 33.8E");
        run.type_line("/tank route add 42.2N 33.9E");
        run.type_line("/tank set EnableCombat false");
        run.type_line("/tank set EnableBuffing false");
    };
    let (mut run, _) = tank(Vec::new());
    route(&mut run);
    run.type_line("/tank vt reverseroute true");
    run.type_line("/tank start");
    run.advance(5.0);
    let (ns, ew, _) = run.player_place().expect("placed");
    assert!(
        ew > 33.83 && (ns - 42.2).abs() < 0.01,
        "toward the last point (east), not the second (north): {ns} {ew} (console {:#?})",
        run.console(TANK)
    );
    assert!(logged(&run, "route: starting at waypoint 1 of 3, reversed"));
    // Far off the route (the route 24 m away, NavFarStopRange 10 m): navigation off.
    let (mut run, _) = tank(Vec::new());
    route(&mut run);
    run.place_player(42.2, 33.7);
    run.type_line("/tank set NavFarStopRange 0.04");
    run.type_line("/tank start");
    run.advance(5.0);
    let (_, ew, _) = run.player_place().expect("placed");
    assert!((ew - 33.7).abs() < 0.001, "it did not walk: {ew}");
    assert!(
        logged(&run, "further than NavFarStopRange: navigation off"),
        "{:#?}",
        run.console(TANK)
    );
    run.type_line("/tank settings");
    run.advance(0.5);
    assert!(run
        .printed()
        .iter()
        .any(|l| l == "[Tank] EnableNav = false"));
    note_asserted(
        "plugins.samples.the-tank-runs-a-reversed-route-backwards-and-stops-navigating-far-off-it",
    );
}

/// The power of each melee swing: with AutoAttackPower, a sword that both thrusts and slashes is
/// swung under the thrust mark at a monster set to pierce and over it at one set to slash; with
/// UseRecklessness and Recklessness trained, the power stays within Recklessness's band; VTank's
/// /vt setattackbar sets it; and its /vt equipitemsfor takes a step at a time toward wielding the
/// monster's weapon.
fn the_tank_swings_at_the_power_its_weapon_recklessness_and_attack_bar_say() {
    use dereth_client_model::Request;
    const MONSTER: u32 = 0x8000_0080;
    let power = |lines: &[&str], skill: bool| {
        let (mut run, _) = tank(vec![
            monster(MONSTER, "Drudge Skulker", 42.2, 33.8001),
            ScriptedObject {
                // A thrust and a slash.
                ints: vec![(47, 0x6)],
                locations: Some(0x0010_0000),
                worn: true,
                ..held(0x8000_00D0, "Sword", item_type::MELEE_WEAPON)
            },
        ]);
        if skill {
            run.set_skill("Recklessness", 100);
        }
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank set EnableBuffing false");
        for line in lines {
            run.type_line(line);
        }
        run.type_line("/tank start");
        run.advance(6.0);
        run.requests()
            .iter()
            .find_map(|r| match r {
                Request::TargetedMeleeAttack(m) => Some(m.power_level),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no swing: {:#?}", run.console(TANK)))
    };
    let pierce = power(&["/tank monster damage pierce Drudge Skulker"], false);
    assert!(pierce < 0.33, "thrust: {pierce}");
    let slash = power(&["/tank monster damage slash Drudge Skulker"], false);
    assert!((slash - 0.6).abs() < 0.01, "slash, at AttackPower: {slash}");
    let reckless = power(
        &[
            "/tank set AttackPower 100",
            "/tank set UseRecklessness true",
        ],
        true,
    );
    assert!((reckless - 0.8).abs() < 0.01, "{reckless}");
    let bar = power(
        &[
            "/tank set AutoAttackPower false",
            "/tank vt setattackbar 0.45",
        ],
        false,
    );
    assert!((bar - 0.45).abs() < 0.01, "{bar}");
    // VTank's /vt equipitemsfor: a step at a time toward the monster's weapon.
    const AXE: u32 = 0x8000_00D1;
    let (mut run, _) = tank(vec![ScriptedObject {
        locations: Some(0x0010_0000),
        ..held(AXE, "Axe", item_type::MELEE_WEAPON)
    }]);
    run.type_line("/tank monster weapon Axe Drudge Skulker");
    run.type_line("/tank vt equipitemsfor Drudge Skulker");
    run.advance(1.0);
    run.type_line("/tank vt equipitemsfor Drudge Skulker");
    run.advance(1.0);
    assert!(
        run.sent().contains(&format!("wield {AXE:#x} 0x100000")),
        "{:?}",
        run.sent()
    );
    let printed = run.printed();
    assert!(
        printed.iter().any(|l| l == "[Tank] wielding Axe")
            && printed.iter().any(|l| l == "[Tank] Axe is wielded"),
        "{printed:#?}"
    );
    note_asserted(
        "plugins.samples.the-tank-swings-at-the-power-its-weapon-recklessness-and-attack-bar-say",
    );
}

/// A ghost (a monster the client shows that the server does not have) is left alone for good:
/// one whose health does not change in GhostDeleteHPTrackerSeconds of fighting it (with the
/// health tracker on), one whose casts never begin GhostMonsterSpellAttemptCount times running,
/// and one the player marks with VTank's /vt deletemonster.
fn the_tank_leaves_a_ghost_monster_alone_for_good() {
    const MONSTER: u32 = 0x8000_0080;
    let swing = format!("melee_attack {MONSTER:#x}");
    // The health tracker: no change in 5 s of fighting.
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_repeating_attacks(),
        &files,
        vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001)],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank set DeleteGhostMonstersByHPTracker true",
        "/tank set GhostDeleteHPTrackerSeconds 5",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(9.0);
    assert!(
        logged(
            &run,
            "Drudge Skulker is a ghost (its health has not changed in 5 s of fighting)"
        ),
        "{:#?}",
        run.console(TANK)
    );
    let swings = attacks(&mut run);
    run.advance(40.0);
    assert_eq!(attacks(&mut run), swings, "left alone for good");
    // Casts that never begin (the server finds no such target).
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_cast_failures(vec![0x0403; 4]),
        &files,
        vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001), wand()],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank set Weapon Wand",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(15.0);
    assert!(
        logged(
            &run,
            "Drudge Skulker is a ghost (4 casts at it never began)"
        ),
        "{:#?}",
        run.console(TANK)
    );
    // Marked by the player.
    let (mut run, _) = tank(vec![monster(MONSTER, "Drudge Skulker", 42.2, 33.8001)]);
    run.select(MONSTER);
    run.type_line("/tank vt deletemonster");
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set EnableBuffing false");
    run.type_line("/tank start");
    run.advance(5.0);
    assert!(!run.sent().contains(&swing), "{:?}", run.sent());
    assert!(run
        .printed()
        .iter()
        .any(|l| l == "[Tank] Drudge Skulker is left alone until the client lets it go"));
    note_asserted("plugins.samples.the-tank-leaves-a-ghost-monster-alone-for-good");
}

/// VTank's information commands: the selected item against the loot profile, its properties, the
/// selected monster against the monster list, a spell against the casting thresholds, pets,
/// the monster variables, the game's tables written to the data folder, and the log's types
/// switched off and on.
fn the_tank_answers_vtanks_information_and_log_commands() {
    const GEM: u32 = 0x8000_00E0;
    const MONSTER: u32 = 0x8000_0080;
    let (mut run, files) = tank(vec![
        ScriptedObject {
            value: 3000,
            ints: vec![(19, 3000)],
            ..held(GEM, "Black Garnet", item_type::GEM)
        },
        monster(MONSTER, "Drudge Skulker", 42.2, 33.85),
    ]);
    run.type_line("/tank profile basic.utl");
    run.select(GEM);
    for line in [
        "/tank vt testitem",
        "/tank vt propertydump",
        "/tank vt testspell 2087",
        "/tank vt testpet",
        "/tank vt listmonstervariables",
        "/tank vt dumpspecies",
        "/tank vt dumpspells",
    ] {
        run.type_line(line);
        run.advance(1.0);
    }
    run.select(MONSTER);
    run.type_line("/tank vt testmonster");
    run.advance(0.5);
    let printed = run.printed();
    let said = |s: &str| printed.iter().any(|l| l.contains(s));
    assert!(
        said("[Tank] Black Garnet: rule Valuables, kept"),
        "{printed:#?}"
    );
    assert!(said("int 19 = 3000"), "{printed:#?}");
    assert!(
        said("[Tank] Might of the Lugians (2087): known; hunting"),
        "{printed:#?}"
    );
    assert!(said("[Tank] no pet now: SummonPets is off"), "{printed:#?}");
    assert!(said("hasshield"), "{printed:#?}");
    assert!(
        said("[Tank] Drudge Skulker: row <DEFAULT>, priority 1, attack, damage auto"),
        "{printed:#?}"
    );
    let read = |path: &str| {
        files
            .clone()
            .read(TANK, path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    };
    assert!(read("dumps/species.txt").contains("3\tDrudge"));
    assert!(read("dumps/spells.txt").contains("2087\tMight of the Lugians"));
    // The log's Misc lines off, then on again.
    run.type_line("/tank vt log Misc off");
    run.type_line("/tank vt setattackbar 0.5");
    run.advance(0.2);
    assert!(
        !logged(&run, "attack bar at 50%"),
        "{:#?}",
        run.console(TANK)
    );
    assert!(run
        .printed()
        .iter()
        .any(|l| l.starts_with("[Tank] log: ") && l.contains("Misc off") && l.contains("Loot on")));
    run.type_line("/tank vt log Misc on");
    run.type_line("/tank vt setattackbar 0.4");
    run.advance(0.2);
    assert!(
        logged(&run, "attack bar at 40%"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted("plugins.samples.the-tank-answers-vtanks-information-and-log-commands");
}

/// The buffs left out: a buff the player excluded is never cast, an excluded scarab leaves out its
/// level's spells (Might of the Lugians, level 7, gives way to Strength Self I), an item listed as
/// never buffed is not; and after a buff is cast, the others are recast BuffCastRecast_Seconds
/// earlier for a while, so buffs running low are cast together.
fn the_tank_leaves_out_excluded_buffs_scarabs_and_items_and_rebuffs_together() {
    const ARMOR: u32 = 0x8000_00C0;
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_item_spells(&[(51, "Impenetrability I")]),
        &files,
        vec![
            ScriptedObject {
                locations: Some(0x0000_0200),
                bitfield: bitfield::ATTACKABLE,
                worn: true,
                ..held(ARMOR, "Breastplate", item_type::ARMOR)
            },
            wand(),
        ],
    );
    run.casts_land(true);
    for line in [
        "/tank set EnableNav false",
        "/tank set ExcludedBuffs Armor Self",
        "/tank set BlacklistedSpellComps Platinum Scarab",
        "/tank set NoBuffItems Breastplate",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(6.0);
    let sent = run.sent();
    let cast = |s: &str| sent.iter().filter(|x| *x == s).count();
    assert_eq!(cast("cast 1 0x50000001"), 1, "Strength Self I: {sent:?}");
    assert_eq!(cast("cast 2087 0x50000001"), 0, "not Might of the Lugians");
    assert_eq!(cast("cast 1133 0x50000001"), 0, "not Armor Self");
    assert_eq!(cast("cast 2215 0x50000001"), 1, "Blade Protection Self");
    assert_eq!(
        cast(&format!("cast 51 {ARMOR:#x}")),
        0,
        "not the breastplate"
    );
    // Armor Self let back in: it is cast, and for a while the others are due 1,600 s earlier, so
    // the two with their half hour still on are cast again with it.
    run.type_line("/tank set BuffCastRecast_Seconds 1600");
    run.type_line("/tank set BuffCastRecastReset_Seconds 3");
    run.type_line("/tank set ExcludedBuffs none");
    run.advance(6.0);
    let sent = run.sent();
    let cast = |s: &str| sent.iter().filter(|x| *x == s).count();
    assert_eq!(cast("cast 1133 0x50000001"), 1, "{:#?}", run.console(TANK));
    assert_eq!(cast("cast 1 0x50000001"), 2, "{:#?}", run.console(TANK));
    assert_eq!(cast("cast 2215 0x50000001"), 2, "{:#?}", run.console(TANK));
    note_asserted(
        "plugins.samples.the-tank-leaves-out-excluded-buffs-scarabs-and-items-and-rebuffs-together",
    );
}

/// RandomHelperBuffs: with nothing to fight, every RandomHelperIntervalSeconds the Tank casts a
/// protection it knows on a player nearby; switched off, none.
fn the_tank_casts_random_protections_on_players_nearby() {
    const BOB: u32 = 0x5000_0002;
    let help = |on: bool| {
        let files = SharedFiles::default();
        let mut run = tank_in(
            PluginRun::new().with_spells(&[(3002, "Blade Protection Other I", 2)], true),
            &files,
            vec![
                ScriptedObject {
                    id: BOB,
                    name: "Bob".into(),
                    obj_type: item_type::CREATURE,
                    bitfield: bitfield::PLAYER,
                    place: Some((42.2, 33.8002)),
                    ..ScriptedObject::default()
                },
                wand(),
            ],
        );
        run.casts_land(true);
        for line in [
            "/tank set EnableNav false",
            "/tank set BuffSpells Nothing",
            &format!("/tank set RandomHelperBuffs {on}"),
            "/tank set RandomHelperIntervalSeconds 5",
            "/tank start",
        ] {
            run.type_line(line);
        }
        run.advance(13.0);
        run
    };
    let run = help(true);
    let casts = run
        .sent()
        .iter()
        .filter(|s| s.starts_with("cast 3002"))
        .count();
    assert!(
        casts >= 2,
        "{:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    assert!(
        logged(&run, "helper buff: Blade Protection Other I on Bob"),
        "{:#?}",
        run.console(TANK)
    );
    let run = help(false);
    assert!(!run.sent().iter().any(|s| s.starts_with("cast 3002")));
    note_asserted("plugins.samples.the-tank-casts-random-protections-on-players-nearby");
}

/// VTank's /vt opt setinall sets an option now and in each settings profile in the data folder,
/// the rest of each profile as it was; and a follow route, which the Tank does not walk, is
/// refused with why, not an error.
fn the_tank_sets_an_option_in_every_profile() {
    let usd = [
        "2",
        "Settings",
        "2",
        "Setting",
        "Value",
        "y",
        "n",
        "1", //
        "s",
        "AttackDistance",
        "d",
        "0.05", //
        "MyMonsters",
        "3",
        "MonsterName",
        "AttackPriority",
        "Attack",
        "y",
        "n",
        "n",
        "1", //
        "s",
        "Drudge Skulker",
        "i",
        "4",
        "b",
        "True",
        "",
    ]
    .join("\r\n");
    let files = SharedFiles::default();
    let mut put = files.clone();
    put.write(TANK, "p1.usd", usd.as_bytes())
        .expect("in memory");
    put.write(TANK, "p2.usd", usd.as_bytes())
        .expect("in memory");
    let mut run = tank_in(PluginRun::new(), &files, Vec::new());
    run.type_line("/tank vt opt setinall AttackPower 75");
    run.advance(0.5);
    assert!(
        run.printed()
            .iter()
            .any(|l| l == "[Tank] AttackPower = 75, and in 2 settings profiles"),
        "{:#?}",
        run.printed()
    );
    for name in ["p1.usd", "p2.usd"] {
        let text = put
            .read(TANK, name)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .expect("kept");
        assert!(
            text.contains("s\r\nAttackPower\r\ni\r\n75\r\n") && text.contains("Drudge Skulker"),
            "{name}: {text}"
        );
    }
    run.type_line("/tank import p1.usd");
    run.type_line("/tank monsters");
    run.advance(0.5);
    assert!(
        run.printed()
            .iter()
            .any(|l| l.contains("2 settings and 1 monster rows read")),
        "the profile reads, its monster list kept: {:#?}",
        run.printed()
    );
    note_asserted("plugins.samples.the-tank-sets-an-option-in-every-profile");
}

/// A meta's chat capture (VTank's ChatMessageCapture) with a color list matches only lines of those
/// chat types and keeps the matched line's type in `capturecolor`; its "seconds in state (P)"
/// condition counts from when the state was entered, a stop and start between, where the plain
/// one counts from the start.
fn a_metas_chat_capture_takes_the_colors_asked_and_its_persistent_seconds_outlast_a_stop() {
    use dereth_plugin_host::Package;
    use dereth_protocol::comms::CommunicationTextboxString;
    let main = r#"
local meta = require("dereth.meta")
local m = meta.new({ start = "Wait", states = {
	Wait = {
		{ meta.chat_capture("^(?<who>\\w+) waves", "3;5"), meta.lua(function(ctx)
			print("captured", ctx.meta.vars.capturegroup_who, ctx.meta.vars.capturecolor)
			ctx.meta:set_state("Timed")
		end) },
	},
	Timed = {
		{ meta.seconds_in_state_ge(3), meta.lua(function(ctx) print("plain at 3") ctx.meta:set_state("Done") end) },
		{ meta.seconds_in_state_persist_ge(3), meta.lua(function(ctx) print("persistent at 3") ctx.meta:set_state("Done") end) },
	},
	Done = {},
} })
on("context.begin", function() m:start() end)
commands.add("restart", "stop and start the meta", function()
	m:stop()
	m:start()
end)
"#;
    let mut run = PluginRun::new();
    run.load(Package {
        origin: "capture".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.capture\"\nname = \"Capture\"\nversion = \"1.0.0\"\napi = \"0.7\"\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    });
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    let line = |text: &str, ty: u32| {
        ui_event(&CommunicationTextboxString {
            text: text.into(),
            text_type: ty,
        })
    };
    // Of type 2 (speech): not one of the colors asked.
    run.server_says(&[line("Alba waves", 2)]);
    run.advance(4.5);
    assert!(
        run.console("test.capture").is_empty(),
        "{:?}",
        run.console("test.capture")
    );
    // Of type 5, five seconds after the meta began: the state it enters counts from its entry.
    run.server_says(&[line("Bob waves", 5)]);
    run.advance(1.0);
    assert_eq!(run.console("test.capture"), ["captured\tBob\t5"]);
    // Stopped and started a second and a half into its state: the persistent count reaches 3 s
    // first.
    run.advance(0.5);
    run.type_line("/restart");
    run.advance(2.5);
    assert_eq!(
        run.console("test.capture"),
        ["captured\tBob\t5", "persistent at 3"],
        "{:?}",
        run.console("test.capture")
    );
    note_asserted(
        "plugins.automation.a-metas-chat-capture-takes-the-colors-asked-and-its-persistent-seconds-outlast-a-stop",
    );
}

/// A place `east` and `north` metres from where the scripted character starts, as map coordinates.
fn off(east: f64, north: f64) -> (f64, f64) {
    (42.2 + north / 240.0, 33.8 + east / 240.0)
}

/// A monster of the scripted world `east` and `north` metres from the character's start.
fn monster_at(id: u32, name: &str, east: f64, north: f64) -> ScriptedObject {
    let (ns, ew) = off(east, north);
    monster(id, name, ns, ew)
}

/// How many console lines contain `s`.
fn logged_count(run: &PluginRun, s: &str) -> usize {
    run.console(TANK).iter().filter(|l| l.contains(s)).count()
}

/// Run the Tank `secs` seconds after `lines`, with navigation on and buffing off.
fn hunt(run: &mut PluginRun, lines: &[&str], secs: f64) {
    run.type_line("/tank set EnableBuffing false");
    for line in lines {
        run.type_line(line);
    }
    run.type_line("/tank start");
    run.advance(secs);
}

/// The owner's ask for the Tank's approach: a monster behind a wall is walked to along the
/// client's route round the wall's end, and one behind a closed door in a wall's gap along the
/// route through the door, which the Tank opens on the way; each is then fought. A monster walled
/// in, which no route reaches, is left alone, and a monster that can be reached is fought after it.
fn the_tank_walks_round_walls_and_through_doors_to_a_monster_and_leaves_one_no_route_reaches() {
    const MONSTER: u32 = 0x8000_0080;
    const DOOR: u32 = 0x8000_0081;
    const OTHER: u32 = 0x8000_0082;
    let swing = format!("melee_attack {MONSTER:#x}");
    // Round a wall: eight metres of wall six metres north, the monster twelve metres north.
    let (mut run, _) = tank(vec![monster_at(MONSTER, "Drudge Skulker", 0.0, 12.0)]);
    run.wall(off(-4.0, 6.0), off(4.0, 6.0));
    hunt(&mut run, &[], 15.0);
    assert!(
        logged(&run, "route to Drudge Skulker: 4 corners"),
        "{:#?}",
        run.console(TANK)
    );
    assert!(
        run.sent().contains(&swing),
        "the monster behind the wall reached and fought: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    // Through a door: the wall has a gap the door stands in.
    let door = ScriptedObject {
        id: DOOR,
        name: "Door".into(),
        obj_type: item_type::MISC,
        place: Some(off(0.0, 6.0)),
        door: Some(dereth_testkit::plugins::ScriptedDoor { locked: false }),
        ..ScriptedObject::default()
    };
    let (mut run, _) = tank(vec![monster_at(MONSTER, "Drudge Skulker", 0.0, 12.0), door]);
    run.wall(off(-6.0, 6.0), off(-1.2, 6.0));
    run.wall(off(1.2, 6.0), off(6.0, 6.0));
    hunt(&mut run, &[], 15.0);
    let sent = run.sent();
    let used = sent.iter().position(|s| *s == format!("use {DOOR:#x}"));
    let swung = sent.iter().position(|s| *s == swing);
    assert!(
        used.is_some() && swung.is_some() && used < swung,
        "the door opened on the way, then the monster fought: {sent:?} (console {:#?})",
        run.console(TANK)
    );
    assert!(logged(&run, "opening Door on the way to Drudge Skulker"));
    // Walled in: no route reaches it; it is left alone, and the one that comes is fought.
    let (mut run, _) = tank(vec![monster_at(MONSTER, "Drudge Skulker", 0.0, 12.0)]);
    run.wall(off(-2.0, 10.0), off(2.0, 10.0));
    run.wall(off(2.0, 10.0), off(2.0, 14.0));
    run.wall(off(2.0, 14.0), off(-2.0, 14.0));
    run.wall(off(-2.0, 14.0), off(-2.0, 10.0));
    hunt(&mut run, &[], 4.0);
    assert!(
        logged(
            &run,
            "leaving Drudge Skulker alone for 30 s (cannot reach it: no_route)"
        ),
        "{:#?}",
        run.console(TANK)
    );
    run.spawn(vec![monster_at(OTHER, "Drudge Lurker", 5.0, 0.0)]);
    run.advance(6.0);
    assert!(
        run.sent().contains(&format!("melee_attack {OTHER:#x}")) && !run.sent().contains(&swing),
        "{:?}",
        run.sent()
    );
    note_asserted(
        "plugins.samples.the-tank-walks-round-walls-and-through-doors-to-a-monster-and-leaves-one-no-route-reaches",
    );
}

/// The owner's L-shaped hallway: a route of two waypoints, the second round the corner, out of
/// sight of the first. The Tank walks the leg along the client's route from the start, up the
/// hallway and round the corner, and reaches the waypoint never stuck; with VTank's straight legs
/// (NavStraightLegs) it runs straight at it, into the corner's wall, and is stuck there first.
fn the_tank_walks_a_route_leg_round_a_corner_along_the_clients_route() {
    // A hallway two metres wide, ten metres north and then ten metres east; the first waypoint at
    // its foot, the second at the end of its arm, east round the corner.
    let hallway = |run: &mut PluginRun| {
        run.wall(off(-1.0, -2.0), off(-1.0, 11.0));
        run.wall(off(-1.0, 11.0), off(12.0, 11.0));
        run.wall(off(1.0, -2.0), off(1.0, 9.0));
        run.wall(off(1.0, 9.0), off(12.0, 9.0));
        let (a, b) = (off(0.0, 0.0), off(10.0, 10.0));
        run.type_line("/tank route new once");
        run.type_line(&format!("/tank route add {:.6}N {:.6}E", a.0, a.1));
        run.type_line(&format!("/tank route add {:.6}N {:.6}E", b.0, b.1));
        run.type_line("/tank set EnableCombat false");
        run.type_line("/tank set EnableBuffing false");
    };
    let end = off(10.0, 10.0);
    let near_end = |run: &PluginRun| {
        let (ns, ew, _) = run.player_place().expect("placed");
        dereth_primitives::num::math::hypot((ns - end.0) * 240.0, (ew - end.1) * 240.0) < 2.5
    };
    let (mut run, _) = tank(Vec::new());
    hallway(&mut run);
    run.type_line("/tank start");
    run.advance(15.0);
    assert!(
        near_end(&run),
        "round the corner at the second waypoint: {:?} (console {:#?})",
        run.player_place(),
        run.console(TANK)
    );
    assert!(
        logged(
            &run,
            "waypoint 2 is out of sight; walking the client's route to it"
        ),
        "{:#?}",
        run.console(TANK)
    );
    assert!(logged(&run, "route: finished"), "{:#?}", run.console(TANK));
    assert!(
        !logged(&run, "stuck") && !logged(&run, "round to waypoint"),
        "never stuck: {:#?}",
        run.console(TANK)
    );
    // VTank's straight legs: straight at the waypoint, into the wall, stuck there first and
    // only then round it.
    let (mut run, _) = tank(Vec::new());
    hallway(&mut run);
    run.type_line("/tank set NavStraightLegs true");
    run.type_line("/tank start");
    run.advance(15.0);
    assert!(
        logged(&run, "travelling round to waypoint 2"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-walks-a-route-leg-round-a-corner-along-the-clients-route",
    );
}

/// A route set again while one of its legs is walked (a meta entering the state it is in anew
/// hands the Tank the same route) is taken up from its nearest waypoint once the walk ends, and
/// runs on to its end.
fn the_tank_takes_up_a_route_set_again_mid_leg_from_its_nearest_waypoint() {
    // The L-shaped hallway, and a meta whose one state walks a route of two waypoints: at the
    // hallway's foot, and round its corner.
    let mut lines: Vec<String> = ["[None]", "1", "uTank2 NAV 1.2", "4", "2"]
        .map(String::from)
        .to_vec();
    for (ns, ew) in [off(0.0, 0.0), off(10.0, 10.0)] {
        lines.extend([
            "0".into(),
            format!("{ew:.6}"),
            format!("{ns:.6}"),
            "0".into(),
            "0".into(),
        ]);
    }
    let route = lines.join("\r\n");
    // The route's bytes, the line break the file puts after them counted in.
    let walk = vec!["ba".to_owned(), (route.len() + 2).to_string(), route];
    let files = SharedFiles::default();
    let mut put = files.clone();
    put.write(
        TANK,
        "metas/walk.met",
        met(&[(ALWAYS, int(0), WALK, walk, "Default")]).as_bytes(),
    )
    .expect("in memory");
    let mut run = tank_in(PluginRun::new(), &files, Vec::new());
    run.wall(off(-1.0, -2.0), off(-1.0, 11.0));
    run.wall(off(-1.0, 11.0), off(12.0, 11.0));
    run.wall(off(1.0, -2.0), off(1.0, 9.0));
    run.wall(off(1.0, 9.0), off(12.0, 9.0));
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank meta walk.met",
        "/tank start",
    ] {
        run.type_line(line);
    }
    // Under way round the corner, the meta enters its state again.
    let rounding = "waypoint 2 is out of sight; walking the client's route to it";
    for _ in 0..100 {
        if logged(&run, rounding) {
            break;
        }
        run.advance(0.1);
    }
    assert!(logged(&run, rounding), "{:#?}", run.console(TANK));
    run.type_line("/tank vt setmetastate Default");
    run.advance(15.0);
    assert_eq!(
        logged_count(&run, "route from the meta: once, 2 waypoints"),
        2,
        "premise: the meta handed the route over again mid-leg: {:#?}",
        run.console(TANK)
    );
    assert!(
        !logged(&run, "error:"),
        "no error in the step: {:#?}",
        run.console(TANK)
    );
    assert!(
        logged(&run, "route: starting at waypoint 2 of 2"),
        "taken up again from the waypoint nearest: {:#?}",
        run.console(TANK)
    );
    assert!(logged(&run, "route: finished"), "{:#?}", run.console(TANK));
    let end = off(10.0, 10.0);
    let (ns, ew, _) = run.player_place().expect("placed");
    assert!(
        dereth_primitives::num::math::hypot((ns - end.0) * 240.0, (ew - end.1) * 240.0) < 2.5,
        "at the second waypoint: {:?}",
        run.player_place()
    );
    note_asserted(
        "plugins.samples.the-tank-takes-up-a-route-set-again-mid-leg-from-its-nearest-waypoint",
    );
}

/// A monster within melee distance but across a wall is walked to round the wall's end before it
/// is attacked: the server's charge would go the long way round and give up.
fn the_tank_walks_round_a_wall_to_a_monster_a_step_away_across_it_before_attacking() {
    const MONSTER: u32 = 0x8000_0080;
    let swing = format!("melee_attack {MONSTER:#x}");
    // The monster 2.6 m north, within melee distance; a wall between, 1.3 m north, 8 m long.
    let (mut run, _) = tank(vec![monster_at(MONSTER, "Drudge Skulker", 0.0, 2.6)]);
    run.wall(off(-4.0, 1.3), off(4.0, 1.3));
    hunt(&mut run, &[], 15.0);
    assert!(
        logged(&run, "route to Drudge Skulker:"),
        "walked to along the client's route: {:#?}",
        run.console(TANK)
    );
    // Where the swing is made from: past the wall's end, where the monster is in sight.
    let (ns, ew, _) = run.player_place().expect("placed");
    let (north, east) = ((ns - 42.2) * 240.0, (ew - 33.8) * 240.0);
    assert!(
        east.abs() > 4.0 || north > 1.3,
        "round the wall's end: {east:.1} m east, {north:.1} m north"
    );
    assert!(
        run.sent().contains(&swing),
        "and the monster fought there: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-walks-round-a-wall-to-a-monster-a-step-away-across-it-before-attacking",
    );
}

/// The Training Academy's hive ramp: a monster just beyond the attack distance, hidden below the
/// ramp's edge, which cuts the line of sight but not the walk. The Tank walks on toward it, past
/// the edge, until it is in sight, and fights it; it does not stand at the attack distance
/// waiting for a line of sight that never comes.
fn the_tank_walks_on_toward_a_monster_hidden_below_a_ramps_edge_until_it_sees_it() {
    const MONSTER: u32 = 0x8000_0080;
    let (mut run, _) = tank(vec![monster_at(MONSTER, "Young Olthoi", 0.0, 7.4)]);
    run.screen(off(-6.0, 4.0), off(6.0, 4.0));
    hunt(&mut run, &[], 15.0);
    assert!(
        run.sent().contains(&format!("melee_attack {MONSTER:#x}")),
        "walked on into sight of it and fought it: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    let (ns, _, _) = run.player_place().expect("placed");
    assert!(
        (ns - 42.2) * 240.0 > 4.0,
        "past the ramp's edge: {:.1} m north",
        (ns - 42.2) * 240.0
    );
    note_asserted("plugins.samples.the-tank-walks-on-toward-a-monster-it-cannot-see-until-it-does");
}

/// A townsperson of the scripted world (a creature no one may attack) `east` and `north` metres
/// from the character's start.
fn person_at(id: u32, name: &str, east: f64, north: f64) -> ScriptedObject {
    ScriptedObject {
        bitfield: 0,
        ..monster_at(id, name, east, north)
    }
}

/// Where the character stood when it first swung at `monster`, in metres east and north of its
/// start, the Tank run on for at most `secs` seconds.
fn first_swing_from(run: &mut PluginRun, monster: u32, secs: f64) -> Option<(f64, f64)> {
    let swing = format!("melee_attack {monster:#x}");
    let mut left = secs;
    while left > 0.0 {
        if run.sent().contains(&swing) {
            let (ns, ew, _) = run.player_place()?;
            return Some(((ew - 33.8) * 240.0, (ns - 42.2) * 240.0));
        }
        run.advance(0.1);
        left -= 0.1;
    }
    None
}

/// The Training Academy's Sparring Golem: a monster in sight within the attack distance, with the
/// Training Master standing across the straight line to it. From there a melee attack is a charge
/// straight at the monster, which his body stops short (the server charges on into him until the
/// attack gives up as out of reach), so the Tank walks round him first and swings only once nobody
/// stands in the way. Someone standing off the line does not hold the swing up.
fn the_tank_walks_round_someone_in_the_way_of_its_charge_before_attacking() {
    const GOLEM: u32 = 0x8000_0080;
    const MASTER: u32 = 0x8000_0081;
    // The golem 5 m north, in sight and within the attack distance; the Master halfway, on the line.
    let (mut run, _) = tank(vec![
        monster_at(GOLEM, "Sparring Golem", 0.0, 5.0),
        person_at(MASTER, "Training Master", 0.0, 2.5),
    ]);
    hunt(&mut run, &[], 0.1);
    let (east, north) = first_swing_from(&mut run, GOLEM, 15.0).unwrap_or_else(|| {
        panic!(
            "the golem fought: {:?} (console {:#?})",
            run.sent(),
            run.console(TANK)
        )
    });
    assert!(
        north > 2.5 || east.abs() > 1.0,
        "the first swing made past the Master, not charged through him: from {east:.1} m east, \
         {north:.1} m north (console {:#?})",
        run.console(TANK)
    );
    assert!(
        run.moves().iter().any(|m| m.starts_with("travel ")),
        "walked round him by the client's travel, which goes round people standing on the way, \
         not straight into him: {:?}",
        run.moves()
    );
    // The Master three metres east of the line: the swing is made at once, from the start.
    let (mut run, _) = tank(vec![
        monster_at(GOLEM, "Sparring Golem", 0.0, 5.0),
        person_at(MASTER, "Training Master", 3.0, 2.5),
    ]);
    hunt(&mut run, &[], 0.1);
    let (east, north) = first_swing_from(&mut run, GOLEM, 15.0).expect("the golem fought");
    assert!(
        north < 0.5 && east.abs() < 0.5,
        "swung at once from where it stood: {east:.1} m east, {north:.1} m north"
    );
    note_asserted(
        "plugins.samples.the-tank-walks-round-someone-in-the-way-of-its-charge-before-attacking",
    );
}

/// A kill in the Training Academy's hive: the Tank's swing at a monster that has just fallen is
/// still charging when its body goes, and the client's automatic targeting selects the next
/// monster, twelve metres off. The swing is dropped rather than sent there (the server would
/// charge across the room, "You charged too far!"); the Tank chooses its next target itself and
/// walks within reach of it before swinging.
fn the_tank_chooses_its_next_target_itself_when_a_fallen_monsters_body_goes_mid_swing() {
    const FALLEN: u32 = 0x8000_0080;
    const NEXT: u32 = 0x8000_0082;
    const CORPSE: u32 = 0x8000_0090;
    let (mut run, _) = tank(vec![
        monster_at(FALLEN, "Young Olthoi", 0.0, 1.5),
        monster_at(NEXT, "Young Olthoi Soldier", 12.0, 0.0),
    ]);
    hunt(&mut run, &[], 0.1);
    // The first swing ends the attack sequence (the monster fell); the Tank swings again, and the
    // client charges that swing.
    let fallen = format!("melee_attack {FALLEN:#x}");
    let mut charging = false;
    for _ in 0..300 {
        run.advance(1.0 / 30.0);
        let building = run.world_mut().combat.build_in_progress;
        if sent_count(&run, &fallen) == 1 && building {
            charging = true;
            break;
        }
    }
    assert!(
        charging,
        "premise: a second swing at the fallen monster charging: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    // Its body goes, its corpse comes, and the client selects the next monster.
    let (ns, ew) = off(0.0, 1.5);
    run.remove(FALLEN);
    run.spawn(vec![corpse(CORPSE, "Young Olthoi", ns, ew)]);
    run.select(NEXT);
    let (east, north) = first_swing_from(&mut run, NEXT, 15.0).unwrap_or_else(|| {
        panic!(
            "the next monster fought: {:?} (console {:#?})",
            run.sent(),
            run.console(TANK)
        )
    });
    assert_eq!(sent_count(&run, &fallen), 1, "{:?}", run.sent());
    let off_it = dereth_primitives::num::math::hypot(12.0 - east, north);
    assert!(
        off_it <= 8.0,
        "swung at from within the attack distance, not charged at from where the other fell: \
         {off_it:.1} m off it (console {:#?})",
        run.console(TANK)
    );
    assert!(
        logged(&run, "target_gone"),
        "the swing dropped: {:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-chooses-its-next-target-itself-when-a-fallen-monsters-body-goes-mid-swing",
    );
}

/// VTank's wall detection: of two monsters of one priority, the nearer behind a wall (a bolt would
/// hit the wall), the Tank fights the one in a straight line of sight first.
fn the_tank_fights_a_monster_in_sight_before_one_behind_a_wall() {
    const NEAR: u32 = 0x8000_0080;
    const FAR: u32 = 0x8000_0081;
    let (mut run, _) = tank(vec![
        monster_at(NEAR, "Drudge Near", 0.0, 5.0),
        monster_at(FAR, "Drudge Far", 6.0, 0.0),
        wand(),
    ]);
    run.wall(off(-3.0, 3.0), off(3.0, 3.0));
    hunt(
        &mut run,
        &[
            "/tank set EnableNav false",
            "/tank set Weapon Wand",
            "/tank set TargetSelectMethod 1",
        ],
        6.0,
    );
    let console = run.console(TANK);
    let first = console
        .iter()
        .find(|l| l.contains(" target "))
        .cloned()
        .unwrap_or_default();
    assert!(
        first.contains("target Drudge Far"),
        "the monster in sight first, though the other is nearer: {console:#?}"
    );
    assert!(
        logged(&run, "Flame Bolt I at Drudge Far") && !logged(&run, "at Drudge Near"),
        "{console:#?}"
    );
    note_asserted("plugins.samples.the-tank-fights-a-monster-in-sight-before-one-behind-a-wall");
}

/// The fire ring, arc and streak of the scripted world, by id.
const RING: u32 = 4001;
const ARC: u32 = 4002;
const STREAK: u32 = 4003;

/// A caster Tank knowing a fire ring, arc and streak, with monsters `monsters`.
fn mage(monsters: Vec<ScriptedObject>) -> PluginRun {
    let files = SharedFiles::default();
    let mut all = monsters;
    all.push(wand());
    let mut run = tank_in(
        PluginRun::new()
            .with_spells(
                &[
                    (RING, "Cassius' Ring of Fire", 1),
                    (ARC, "Flame Arc I", 1),
                    (STREAK, "Flame Streak I", 1),
                ],
                true,
            )
            .with_categories(&[(RING, 226)]),
        &files,
        all,
    );
    run.type_line("/tank set EnableNav false");
    run.type_line("/tank set Weapon Wand");
    run.type_line("/tank monster damage fire <DEFAULT>");
    run
}

/// VTank's rings, arcs and streaks: a row marked for rings casts the ring once MinimumRingTargets
/// monsters are within RingDistance, and a bolt with fewer; one marked for rings that does not
/// attack rings with a single monster near; a row marked for streaks casts the streak; with
/// UseArcs the arc is cast at monsters ArcRange away or more, the bolt at nearer ones.
fn the_tank_rings_a_crowd_and_casts_streaks_and_arcs_as_its_rows_and_settings_say() {
    let crowd = || {
        (0..4)
            .map(|i| monster_at(0x8000_0080 + i, "Drudge", 1.0 + f64::from(i), 1.0))
            .collect::<Vec<_>>()
    };
    let mut run = mage(crowd());
    hunt(&mut run, &["/tank monster ring on <DEFAULT>"], 4.0);
    assert!(
        run.sent().contains(&format!("cast {RING}")) && logged(&run, "ring: Cassius' Ring of Fire"),
        "four near: a ring: {:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    let mut run = mage(crowd());
    hunt(
        &mut run,
        &[
            "/tank monster ring on <DEFAULT>",
            "/tank set MinimumRingTargets 5",
        ],
        4.0,
    );
    assert!(
        !run.sent().contains(&format!("cast {RING}")) && logged(&run, "Flame Bolt I at Drudge"),
        "four of five: the bolt: {:#?}",
        run.console(TANK)
    );
    // Rings, not attacks: one monster in ring range is enough.
    let mut run = mage(vec![monster_at(0x8000_0080, "Drudge", 2.0, 2.0)]);
    hunt(
        &mut run,
        &[
            "/tank monster ring on <DEFAULT>",
            "/tank monster attack off <DEFAULT>",
        ],
        4.0,
    );
    assert!(
        run.sent().contains(&format!("cast {RING}")),
        "{:#?}",
        run.console(TANK)
    );
    let mut run = mage(vec![monster_at(0x8000_0080, "Drudge", 2.0, 2.0)]);
    hunt(&mut run, &["/tank monster streak on <DEFAULT>"], 4.0);
    assert!(
        logged(&run, "Flame Streak I at Drudge") && !logged(&run, "Flame Bolt I at"),
        "{:#?}",
        run.console(TANK)
    );
    // Arcs at range: five metres off, with arcs from four metres, then from ten.
    let mut run = mage(vec![monster_at(0x8000_0080, "Drudge", 0.0, 5.0)]);
    hunt(
        &mut run,
        &["/tank set UseArcs true", "/tank set ArcRange 0.0167"],
        4.0,
    );
    assert!(
        logged(&run, "Flame Arc I at Drudge") && !logged(&run, "Flame Bolt I at"),
        "{:#?}",
        run.console(TANK)
    );
    let mut run = mage(vec![monster_at(0x8000_0080, "Drudge", 0.0, 5.0)]);
    hunt(
        &mut run,
        &["/tank set UseArcs true", "/tank set ArcRange 0.0417"],
        4.0,
    );
    assert!(
        logged(&run, "Flame Bolt I at Drudge") && !logged(&run, "Flame Arc I at"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-rings-a-crowd-and-casts-streaks-and-arcs-as-its-rows-and-settings-say",
    );
}

/// The void curses and damage over time, VTank's FC, Cp, DC and Cs: a curse is recast
/// DebuffPrecastSeconds before it runs out as any debuff is, while damage over time is recast only
/// once it has run its course. Drain Auto drains the monster's health, and its mana when the
/// character's runs low; Harm casts the harm spell.
fn the_tank_casts_void_curses_and_damage_over_time_and_drains_and_harms() {
    const DOT: u32 = 5387;
    const CURSE: u32 = 5371;
    let caster = |spells: &[(u32, &str, u32)]| {
        let files = SharedFiles::default();
        let mut run = tank_in(
            PluginRun::new().with_spells(spells, true),
            &files,
            vec![monster_at(0x8000_0080, "Drudge", 0.0, 2.0), wand()],
        );
        run.type_line("/tank set EnableNav false");
        run.type_line("/tank set Weapon Wand");
        run
    };
    let mut run = caster(&[(DOT, "Corrosion I", 5), (CURSE, "Festering Curse I", 5)]);
    hunt(
        &mut run,
        &[
            "/tank monster attack off <DEFAULT>",
            "/tank monster corrosion on <DEFAULT>",
            "/tank monster festering on <DEFAULT>",
            // Every debuff here lasts half an hour: this is due again at once.
            "/tank set DebuffPrecastSeconds 1797",
        ],
        12.0,
    );
    assert!(
        logged_count(&run, "Festering Curse I on Drudge") >= 2,
        "the curse recast before it runs out: {:#?}",
        run.console(TANK)
    );
    assert_eq!(
        logged_count(&run, "Corrosion I on Drudge"),
        1,
        "damage over time runs its course: {:#?}",
        run.console(TANK)
    );
    let mut run = caster(&[
        (1237, "Drain Health Other I", 2),
        (1260, "Drain Mana Other I", 2),
    ]);
    hunt(&mut run, &["/tank monster damage drain <DEFAULT>"], 3.0);
    assert!(
        logged(&run, "Drain Health Other I at Drudge"),
        "{:#?}",
        run.console(TANK)
    );
    // Under its top-off mark, above the in-fight one: drained from the monster, not restored.
    run.set_vitals((100, 100), (100, 100), (40, 100));
    run.advance(3.0);
    assert!(
        logged(&run, "Drain Mana Other I at Drudge"),
        "mana low: drained from the monster: {:#?}",
        run.console(TANK)
    );
    let mut run = caster(&[(7, "Harm Other I", 2)]);
    hunt(&mut run, &["/tank monster damage harm <DEFAULT>"], 3.0);
    assert!(
        logged(&run, "Harm Other I at Drudge"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-casts-void-curses-and-damage-over-time-and-drains-and-harms",
    );
}

/// A sword, a wand and a buckler in the pack; the sword wielded.
fn armed() -> Vec<ScriptedObject> {
    vec![
        ScriptedObject {
            locations: Some(0x0010_0000),
            worn: true,
            ..held(0x8000_00D0, "Sword", item_type::MELEE_WEAPON)
        },
        wand(),
        ScriptedObject {
            locations: Some(0x0020_0000),
            ..held(0x8000_00D2, "Buckler", item_type::ARMOR)
        },
    ]
}

/// VTank's SwitchWandsToDebuff: a monster its row fights with a sword is debuffed with a wand
/// first, then the sword is taken up again; and the row's offhand, a shield, is taken up beside
/// the sword. Without the setting, the sword fights at once, undebuffed.
fn the_tank_debuffs_with_a_wand_before_melee_and_holds_a_shield_beside_its_sword() {
    const MONSTER: u32 = 0x8000_0080;
    let fight = |switch: bool| {
        let mut objects = armed();
        objects.push(monster_at(MONSTER, "Drudge", 0.0, 1.0));
        let (mut run, _) = tank(objects);
        hunt(
            &mut run,
            &[
                "/tank set EnableNav false",
                "/tank set Weapon Sword",
                &format!("/tank set SwitchWandsToDebuff {switch}"),
                "/tank monster imperil on <DEFAULT>",
                "/tank monster offhand Buckler <DEFAULT>",
            ],
            10.0,
        );
        run
    };
    let run = fight(true);
    let sent = run.sent();
    let at = |s: &str| sent.iter().position(|x| x == s);
    let wand_up = at("wield 0x800000b0 0x1000000");
    let imperil = at("cast 23");
    let sword_back = sent
        .iter()
        .enumerate()
        .skip(imperil.unwrap_or(sent.len()))
        .find(|(_, s)| s.starts_with("wield 0x800000d0"))
        .map(|(i, _)| i);
    let shield = at("wield 0x800000d2 0x200000");
    let swing = at(&format!("melee_attack {MONSTER:#x}"));
    assert!(
        wand_up < imperil
            && imperil < sword_back
            && sword_back < swing
            && shield.is_some()
            && shield < swing
            && wand_up.is_some(),
        "the wand to debuff, then the sword and the shield to fight: {sent:?} (console {:#?})",
        run.console(TANK)
    );
    let run = fight(false);
    assert!(
        !run.sent().contains(&"cast 23".to_owned())
            && run.sent().contains(&format!("melee_attack {MONSTER:#x}")),
        "{:?}",
        run.sent()
    );
    note_asserted(
        "plugins.samples.the-tank-debuffs-with-a-wand-before-melee-and-holds-a-shield-beside-its-sword",
    );
}

/// VTank's dispelling: with a vulnerability of level 7 on the character, UseDispelItems uses a Gem
/// of Stillness from the pack, and CastDispelSelf casts Eradicate Life Magic Self; a level 6
/// vulnerability is left.
fn the_tank_dispels_a_heavy_vulnerability_with_an_item_or_its_spell() {
    use dereth_protocol::qualities::MagicUpdateEnchantment;
    use dereth_protocol::types::qualities::Enchantment;
    const GEM: u32 = 0x8000_00E0;
    let vulned = |level7: bool, lines: &[&str], gem: bool| {
        let files = SharedFiles::default();
        let mut objects = vec![wand()];
        if gem {
            objects.push(held(GEM, "Gem of Stillness", item_type::GEM));
        }
        let mut run = tank_in(
            PluginRun::new().with_spells(
                &[
                    (2000, "Fire Vulnerability Other VII", 2),
                    (2001, "Fire Vulnerability Other VI", 2),
                    (3194, "Eradicate Life Magic Self", 2),
                ],
                true,
            ),
            &files,
            objects,
        );
        run.server_says(&[ui_event(&MagicUpdateEnchantment(Enchantment {
            id: if level7 { 2000 } else { 2001 },
            category_word: 2000,
            duration: 300.0,
            // An additive lowering, as a vulnerability's.
            smod: dereth_protocol::types::qualities::StatMod {
                kind: 0x0000_8001,
                key: 1,
                value: -10.0,
            },
            ..Default::default()
        }))]);
        hunt(&mut run, lines, 4.0);
        run
    };
    let run = vulned(true, &["/tank set UseDispelItems true"], true);
    assert!(
        run.sent().contains(&format!("use {GEM:#x}")),
        "{:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    assert!(logged(
        &run,
        "dispelling Fire Vulnerability Other VII with Gem of Stillness"
    ));
    let run = vulned(true, &["/tank set CastDispelSelf true"], false);
    assert!(
        run.sent().contains(&"cast 3194 0x50000001".to_owned()),
        "{:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    let run = vulned(
        false,
        &[
            "/tank set UseDispelItems true",
            "/tank set CastDispelSelf true",
        ],
        true,
    );
    assert!(
        !run.sent().contains(&format!("use {GEM:#x}"))
            && !run.sent().iter().any(|s| s.starts_with("cast 3194")),
        "a level 6 vulnerability is left: {:?}",
        run.sent()
    );
    note_asserted(
        "plugins.samples.the-tank-dispels-a-heavy-vulnerability-with-an-item-or-its-spell",
    );
}

/// VTank's automatic weapon choice: for a row whose weapon is auto, of the weapons the Weapons
/// setting names the one striking with the row's damage type is taken up, a rending one first; on
/// a monster made vulnerable, a critical-strike one first.
fn the_tank_takes_up_the_weapon_for_a_monsters_damage_and_a_striking_one_on_a_vulned_monster() {
    const MONSTER: u32 = 0x8000_0080;
    let sword = |id: u32, name: &str, damage: i32, imbued: i32| ScriptedObject {
        ints: vec![(45, damage), (179, imbued)],
        locations: Some(0x0010_0000),
        ..held(id, name, item_type::MELEE_WEAPON)
    };
    let fight = |lines: &[&str]| {
        let files = SharedFiles::default();
        let mut run = tank_in(
            PluginRun::new().with_spells(&[(3001, "Fire Vulnerability Other I", 2)], true),
            &files,
            vec![
                sword(0x8000_00D1, "Frost Sword", 0x8, 0),
                sword(0x8000_00D2, "Strike Sword", 0x10, 0x1),
                sword(0x8000_00D3, "Fire Sword", 0x10, 0x200),
                wand(),
                monster_at(MONSTER, "Drudge", 0.0, 1.0),
            ],
        );
        hunt(
            &mut run,
            &[
                &[
                    "/tank set EnableNav false",
                    "/tank set Weapons Frost Sword, Strike Sword, Fire Sword",
                    "/tank monster damage fire <DEFAULT>",
                    "/tank monster weapon auto <DEFAULT>",
                ][..],
                lines,
            ]
            .concat(),
            10.0,
        );
        run
    };
    let run = fight(&[]);
    assert!(
        logged(&run, "taking up Fire Sword") && !logged(&run, "taking up Strike Sword"),
        "the fire-rending sword: {:#?}",
        run.console(TANK)
    );
    let run = fight(&[
        "/tank set SwitchWandsToDebuff true",
        "/tank monster vuln on <DEFAULT>",
    ]);
    assert!(
        logged(&run, "Fire Vulnerability Other I on Drudge")
            && logged(&run, "taking up Strike Sword"),
        "vulned: the critical-strike sword: {:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-takes-up-the-weapon-for-a-monsters-damage-and-a-striking-one-on-a-vulned-monster",
    );
}

/// VTank's grenades, Tusker Fists and Random damage type: a fighter whose row is marked for rings
/// throws a grenade the Consumables list names at a crowd, and swings with fewer about; a row set
/// to fists faces the monster and casts Tusker Fists ahead; a row set to random fights with a
/// damage type of those the character has a spell of.
fn the_tank_throws_grenades_at_a_crowd_casts_fists_ahead_and_picks_a_random_damage_type() {
    const BOMB: u32 = 0x8000_00F0;
    let crowd = || {
        (0..4)
            .map(|i| monster_at(0x8000_0080 + i, "Drudge", 1.0 + f64::from(i), 1.0))
            .collect::<Vec<_>>()
    };
    let thrower = |lines: &[&str]| {
        let mut objects = crowd();
        objects.push(ScriptedObject {
            locations: Some(0x0040_0000),
            stack: Some(10),
            ..held(BOMB, "Fire Bomb", item_type::MISSILE_WEAPON)
        });
        let (mut run, _) = tank(objects);
        hunt(
            &mut run,
            &[
                &[
                    "/tank set EnableNav false",
                    "/tank set Consumables Fire Bomb",
                    "/tank monster ring on <DEFAULT>",
                    "/tank monster damage fire <DEFAULT>",
                ][..],
                lines,
            ]
            .concat(),
            6.0,
        );
        run
    };
    // The grenade taken up to throw (the throw itself is the client's missile attack).
    let run = thrower(&[]);
    assert!(
        run.sent().contains(&format!("wield {BOMB:#x} 0x400000"))
            && logged(&run, "throwing Fire Bomb at Drudge"),
        "{:?} (console {:#?})",
        run.sent(),
        run.console(TANK)
    );
    let run = thrower(&["/tank set MinimumRingTargets 5"]);
    assert!(
        !logged(&run, "throwing Fire Bomb")
            && !run.sent().contains(&format!("wield {BOMB:#x} 0x400000"))
            && run.sent().iter().any(|s| s.starts_with("melee_attack")),
        "{:#?}",
        run.console(TANK)
    );
    // Fists: four metres east of the character, which faces north.
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new().with_spells(&[(2934, "Tusker Fists", 1)], true),
        &files,
        vec![monster_at(0x8000_0080, "Drudge", 4.0, 0.0), wand()],
    );
    hunt(
        &mut run,
        &[
            "/tank set EnableNav false",
            "/tank set Weapon Wand",
            "/tank monster damage fists <DEFAULT>",
        ],
        4.0,
    );
    let moves = run.moves();
    assert!(
        moves.iter().any(|m| m == "face 90") && run.sent().contains(&"cast 2934".to_owned()),
        "faced, then the fists: {moves:?} {:?}",
        run.sent()
    );
    // Random, knowing fire's bolt alone.
    let files = SharedFiles::default();
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![monster_at(0x8000_0080, "Drudge", 0.0, 2.0), wand()],
    );
    hunt(
        &mut run,
        &[
            "/tank set EnableNav false",
            "/tank set Weapon Wand",
            "/tank monster damage random <DEFAULT>",
        ],
        4.0,
    );
    assert!(
        logged(&run, "Drudge: fire, at random") && logged(&run, "Flame Bolt I at Drudge"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted(
        "plugins.samples.the-tank-throws-grenades-at-a-crowd-casts-fists-ahead-and-picks-a-random-damage-type",
    );
}

/// VTank's DoJiggle, breakable turn and fast-cast buffs: with DoJiggle the caster side-steps after
/// its casts; with the breakable turn, having turned to a monster off to the side it fights one of
/// higher priority that came meanwhile instead; with fast-cast buffs it moves backward while it
/// buffs. Each off, it does none of these.
fn the_tank_side_steps_its_casts_looks_again_after_turning_and_backs_off_to_buff() {
    const A: u32 = 0x8000_0080;
    const BOSS: u32 = 0x8000_0081;
    let jiggle = |on: bool| {
        let (mut run, _) = tank(vec![monster_at(A, "Drudge", 0.0, 5.0), wand()]);
        hunt(
            &mut run,
            &[
                "/tank set EnableNav false",
                "/tank set Weapon Wand",
                &format!("/tank set DoJiggle {on}"),
            ],
            5.0,
        );
        run.moves()
            .iter()
            .any(|m| m == "hold strafe_left" || m == "hold strafe_right")
    };
    assert!(jiggle(true), "side-steps");
    assert!(!jiggle(false));
    // The breakable turn: a monster off to the side; a boss comes while the Tank turns to it.
    let turn = |on: bool| {
        let (mut run, _) = tank(vec![monster_at(A, "Drudge", 5.0, 0.0), wand()]);
        for line in [
            "/tank set EnableNav false",
            "/tank set EnableBuffing false",
            "/tank set Weapon Wand",
            "/tank monster priority 3 Drudge Boss",
            &format!("/tank set BreakableTurn {on}"),
            "/tank start",
        ] {
            run.type_line(line);
        }
        for _ in 0..90 {
            if logged(&run, "target Drudge (") {
                break;
            }
            run.advance(1.0 / 30.0);
        }
        run.spawn(vec![monster_at(BOSS, "Drudge Boss", 0.0, -5.0)]);
        run.advance(4.0);
        run
    };
    let at_first = |run: &PluginRun| {
        run.console(TANK)
            .iter()
            .any(|l| l.ends_with("Flame Bolt I at Drudge"))
    };
    let run = turn(true);
    assert!(
        logged(&run, "turned to Drudge; Drudge Boss comes first")
            && !at_first(&run)
            && logged(&run, "Flame Bolt I at Drudge Boss"),
        "{:#?}",
        run.console(TANK)
    );
    let run = turn(false);
    assert!(
        at_first(&run),
        "without it, the cast at the first goes: {:#?}",
        run.console(TANK)
    );
    // Fast-cast buffs: backward while buffing.
    let backs = |on: bool| {
        let (mut run, _) = tank(vec![wand()]);
        run.casts_land(true);
        run.type_line("/tank set EnableNav false");
        run.type_line(&format!("/tank set FastCastBuffs {on}"));
        run.type_line("/tank start");
        run.advance(4.0);
        run.moves().iter().any(|m| m == "hold backward")
    };
    assert!(backs(true), "backward while buffing");
    assert!(!backs(false));
    note_asserted(
        "plugins.samples.the-tank-side-steps-its-casts-looks-again-after-turning-and-backs-off-to-buff",
    );
}
