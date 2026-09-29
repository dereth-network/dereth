//! Vectors: empyrean/fixtures/vectors/commands/dev_*
//! DeveloperCommands' convert/enum parse/nudge/echoflags/listplayers/showstats text equals ACE
//! vectors; console lines run on the world thread; ciaetheria locations follow the colour.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_command::command_handler::CommandHandler;
use empyrean_command::command_manager;
use empyrean_command::handlers::admin_commands::{culture_compare, enum_try_parse};
use empyrean_command::handlers::developer_commands::{
    self as dc, convert, AetheriaColorEnum, SigilEnum, SurgeEnum,
};
use empyrean_common::vectors::{self, f64_of, i64_of, same_f64};
use empyrean_entity::enums::{
    AceEnum, ChatMessageType, EquipMask, MotionCommand, PlayScript, PositionType, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    Skill, Sound, WeenieType,
};
use empyrean_entity::Position;
use empyrean_testkit::TestServer;

/// The handler of ACE's method `method` in DeveloperCommands (the registry test's lookup, by name).
pub fn handler_of(method: &str) -> Option<&'static str> {
    let table: [(&str, &str); 121] = [
        ("HandleNudge", "developer_commands::handle_nudge"),
        ("HandleFixBusy", "developer_commands::handle_fix_busy"),
        ("EquipTest", "developer_commands::equip_test"),
        ("HandleNetStats", "developer_commands::handle_net_stats"),
        (
            "HandleShowCompatibleClothingBases",
            "developer_commands::handle_show_compatible_clothing_bases",
        ),
        ("HandleDebugEcho", "developer_commands::handle_debug_echo"),
        ("HandlePlaySound", "developer_commands::handle_play_sound"),
        ("HandlePlayEffect", "developer_commands::handle_play_effect"),
        ("ChatDump", "developer_commands::chat_dump"),
        ("Animation", "developer_commands::animation"),
        ("Movement", "developer_commands::movement"),
        ("MoveTo", "developer_commands::move_to"),
        ("BarberShop", "developer_commands::barber_shop"),
        (
            "HandleListPlayers",
            "developer_commands::handle_list_players",
        ),
        ("HandleSaveNow", "developer_commands::handle_save_now"),
        (
            "HandleLoadAllLandblocks",
            "developer_commands::handle_load_all_landblocks",
        ),
        (
            "HandlePropertyDump",
            "developer_commands::handle_property_dump",
        ),
        ("HandleWhoAmI", "developer_commands::handle_who_am_i"),
        (
            "HandleDebugEchoFlags",
            "developer_commands::handle_debug_echo_flags",
        ),
        ("HandleSetCoin", "developer_commands::handle_set_coin"),
        (
            "HandleDebugTeleportXYZ",
            "developer_commands::handle_debug_teleport_xyz",
        ),
        ("HandleTeleType", "developer_commands::handle_tele_type"),
        (
            "HandleListPositions",
            "developer_commands::handle_list_positions",
        ),
        (
            "HandleSetPosition",
            "developer_commands::handle_set_position",
        ),
        ("HandleDebugGPS", "developer_commands::handle_debug_gps"),
        ("HandleAddTitle", "developer_commands::handle_add_title"),
        (
            "HandleAddAllTitles",
            "developer_commands::handle_add_all_titles",
        ),
        ("HandleGrantXp", "developer_commands::handle_grant_xp"),
        (
            "HandleGrantLuminance",
            "developer_commands::handle_grant_luminance",
        ),
        (
            "HandleGrantItemXp",
            "developer_commands::handle_grant_item_xp",
        ),
        (
            "HandleSpendAllXp",
            "developer_commands::handle_spend_all_xp",
        ),
        ("SetVital", "developer_commands::set_vital"),
        ("HandleSetHealth", "developer_commands::handle_set_health"),
        ("HarmSelf", "developer_commands::harm_self"),
        ("HandleWeapons", "developer_commands::handle_weapons"),
        ("HandleInv", "developer_commands::handle_inv"),
        ("HandleSplits", "developer_commands::handle_splits"),
        ("HandleComps", "developer_commands::handle_comps"),
        ("HandleFood", "developer_commands::handle_food"),
        ("HandleCurrency", "developer_commands::handle_currency"),
        ("HandleCIRandom", "developer_commands::handle_ci_random"),
        (
            "HandleAddAllSpells",
            "developer_commands::handle_add_all_spells",
        ),
        ("GetSpellFormula", "developer_commands::get_spell_formula"),
        (
            "GetAllSpellFormula",
            "developer_commands::get_all_spell_formula",
        ),
        ("ReadDat", "developer_commands::read_dat"),
        ("HandleContract", "developer_commands::handle_contract"),
        (
            "HandleRequestTurnTo",
            "developer_commands::handle_request_turn_to",
        ),
        (
            "ToggleMovementDebug",
            "developer_commands::toggle_movement_debug",
        ),
        ("HandleVisible", "developer_commands::handle_visible"),
        ("HandleShowStats", "developer_commands::handle_show_stats"),
        ("HandleGiveMana", "developer_commands::handle_give_mana"),
        ("HandleDist", "developer_commands::handle_dist"),
        (
            "HandleTeleportDist",
            "developer_commands::handle_teleport_dist",
        ),
        ("HandleKnownObjs", "developer_commands::handle_known_objs"),
        (
            "HandleVisibleObjs",
            "developer_commands::handle_visible_objs",
        ),
        (
            "HandleKnownPlayers",
            "developer_commands::handle_known_players",
        ),
        (
            "HandleVisiblePlayers",
            "developer_commands::handle_visible_players",
        ),
        (
            "HandleVisibleTargets",
            "developer_commands::handle_visible_targets",
        ),
        (
            "HandleRetaliateTargets",
            "developer_commands::handle_retaliate_targets",
        ),
        (
            "HandleDestructionQueue",
            "developer_commands::handle_destruction_queue",
        ),
        ("HandleDebugEmote", "developer_commands::handle_debug_emote"),
        ("HandleMyLoc", "developer_commands::handle_my_loc"),
        (
            "HandleGetProperty",
            "developer_commands::handle_get_property",
        ),
        (
            "HandleSetProperty",
            "developer_commands::handle_set_property",
        ),
        (
            "HandleSetPurchaseTime",
            "developer_commands::handle_set_purchase_time",
        ),
        (
            "HandleDebugDamage",
            "developer_commands::handle_debug_damage",
        ),
        (
            "HandleEnableAetheria",
            "developer_commands::handle_enable_aetheria",
        ),
        ("HandleDebugChess", "developer_commands::handle_debug_chess"),
        ("HandleDebugBoard", "developer_commands::handle_debug_board"),
        (
            "HandleTeleDungeon",
            "developer_commands::handle_tele_dungeon",
        ),
        (
            "HandleDungeonName",
            "developer_commands::handle_dungeon_name",
        ),
        (
            "HandleClearPhysicsCaches",
            "developer_commands::handle_clear_physics_caches",
        ),
        ("HandleForceGC", "developer_commands::handle_force_gc"),
        ("HandleForceGC2", "developer_commands::handle_force_gc2"),
        (
            "HandleAuditObjectMaint",
            "developer_commands::handle_audit_object_maint",
        ),
        ("HandleLootGen", "developer_commands::handle_loot_gen"),
        ("HandleCILoot", "developer_commands::handle_ci_loot"),
        ("HandleMakeIOU", "developer_commands::handle_make_iou"),
        (
            "HandleTestDeathItems",
            "developer_commands::handle_test_death_items",
        ),
        (
            "HandleForceLogout",
            "developer_commands::handle_force_logout",
        ),
        (
            "HandleForceLogoff",
            "developer_commands::handle_force_logoff",
        ),
        (
            "HandleShowSession",
            "developer_commands::handle_show_session",
        ),
        (
            "HandleRequireComps",
            "developer_commands::handle_require_comps",
        ),
        ("HandleSafeComps", "developer_commands::handle_safe_comps"),
        (
            "HandleAddItemSpell",
            "developer_commands::handle_add_item_spell",
        ),
        (
            "HandleRemoveItemSpell",
            "developer_commands::handle_remove_item_spell",
        ),
        ("HandlePKTimer", "developer_commands::handle_pk_timer"),
        ("HandleFellowInfo", "developer_commands::handle_fellow_info"),
        ("HandleFellowDist", "developer_commands::handle_fellow_dist"),
        (
            "HandleGeneratorDump",
            "developer_commands::handle_generator_dump",
        ),
        (
            "HandlePurchaseHouse",
            "developer_commands::handle_purchase_house",
        ),
        (
            "HandleBarrierTest",
            "developer_commands::handle_barrier_test",
        ),
        ("HandleTargetLoc", "developer_commands::handle_target_loc"),
        (
            "HandleDamageHistory",
            "developer_commands::handle_damage_history",
        ),
        (
            "HandleRemoveVitae",
            "developer_commands::handle_remove_vitae",
        ),
        ("HandleFast", "developer_commands::handle_fast"),
        ("HandleSlow", "developer_commands::handle_slow"),
        ("HandleRip", "developer_commands::handle_rip"),
        ("HandleResistInfo", "developer_commands::handle_resist_info"),
        ("HandleDebugSpell", "developer_commands::handle_debug_spell"),
        ("HandleRecordCast", "developer_commands::handle_record_cast"),
        ("HandlePScript", "developer_commands::handle_p_script"),
        ("HandleGetInfo", "developer_commands::handle_get_info"),
        ("HandleTestAim", "developer_commands::handle_test_aim"),
        (
            "HandleReloadLandblocks",
            "developer_commands::handle_reload_landblocks",
        ),
        (
            "HandleShowVelocity",
            "developer_commands::handle_show_velocity",
        ),
        (
            "HandleBumpVelocity",
            "developer_commands::handle_bump_velocity",
        ),
        (
            "HandleCheckEthereal",
            "developer_commands::handle_check_ethereal",
        ),
        ("HandleFaction", "developer_commands::handle_faction"),
        ("HandleShowTier", "developer_commands::handle_show_tier"),
        ("HandleTierMobs", "developer_commands::handle_tier_mobs"),
        ("HandleDelevel", "developer_commands::handle_delevel"),
        (
            "HandleMonsterProj",
            "developer_commands::handle_monster_proj",
        ),
        (
            "HandleDebugSpellbook",
            "developer_commands::handle_debug_spellbook",
        ),
        ("HandleTryWield", "developer_commands::handle_try_wield"),
        (
            "HandleShowWieldedTreasure",
            "developer_commands::handle_show_wielded_treasure",
        ),
        ("HandleCIAetheria", "developer_commands::handle_ci_aetheria"),
        ("HandleVendorDump", "developer_commands::handle_vendor_dump"),
        ("HandleCastSpell", "developer_commands::handle_cast_spell"),
        (
            "HandleUseWithTarget",
            "developer_commands::handle_use_with_target",
        ),
        (
            "HandlePortalStorm",
            "developer_commands::handle_portal_storm",
        ),
    ];
    table.iter().find(|(m, _)| *m == method).map(|(_, h)| *h)
}

/// One `Convert` call's result as the vectors name it: a long, a bool or a double.
enum Converted {
    Long(i64),
    Bool(bool),
    Double(f64),
}

fn call(f: &str, s: &str) -> Result<Converted, &'static str> {
    use Converted::{Bool, Double, Long};
    Ok(match f {
        "ToInt16" => Long(i64::from(convert::to_int16(s)?)),
        "ToUInt16" => Long(i64::from(convert::to_uint16(s)?)),
        "ToInt32" => Long(i64::from(convert::to_int32(s)?)),
        "ToUInt32" => Long(i64::from(convert::to_uint32(s)?)),
        "ToInt64" => Long(convert::to_int64(s)?),
        "ToInt32Base10" => Long(i64::from(convert::to_int32_base(s, 10)?)),
        "ToInt32Base16" => Long(i64::from(convert::to_int32_base(s, 16)?)),
        "ToUInt32Base10" => Long(i64::from(convert::to_uint32_base(s, 10)?)),
        "ToUInt32Base16" => Long(i64::from(convert::to_uint32_base(s, 16)?)),
        "ToInt64Base10" => Long(convert::to_int64_base(s, 10)?),
        "ToInt64Base16" => Long(convert::to_int64_base(s, 16)?),
        "ToBoolean" => Bool(convert::to_boolean(s)?),
        "ToDouble" => Double(convert::to_double(s)?),
        other => panic!("unknown call {other}"),
    })
}

#[test]
fn convert_matches_net() {
    let file = vectors::load_named("commands", "dev_convert");
    let mut bad = Vec::new();
    for case in &file.cases {
        let f = case.input["f"].as_str().unwrap();
        let s = case.input["s"].as_str().unwrap();
        match (vectors::throws(&case.output), call(f, s)) {
            (Some(t), Err(e)) if e == t => {}
            (Some(t), Err(e)) => bad.push(format!("{f}({s:?}) threw {e}, .NET throws {t}")),
            (Some(t), Ok(_)) => bad.push(format!("{f}({s:?}) should throw {t}")),
            (None, Err(e)) => bad.push(format!(
                "{f}({s:?}) threw {e}, .NET returns {}",
                case.output["value"]
            )),
            (None, Ok(Converted::Long(v))) if Some(v) == i64_of(&case.output["value"]) => {}
            (None, Ok(Converted::Bool(v))) if Some(v) == case.output["value"].as_bool() => {}
            (None, Ok(Converted::Double(v)))
                if f64_of(&case.output["value"]).is_some_and(|e| same_f64(v, e)) => {}
            (None, Ok(_)) => bad.push(format!("{f}({s:?}): .NET returns {}", case.output["value"])),
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

type Parsed = Option<(i64, bool, String)>;

fn u32e<E: AceEnum>(s: &str, ic: bool, make: fn(u32) -> E) -> Parsed {
    enum_try_parse::<E>(s, ic, 0, i128::from(u32::MAX)).map(|v| {
        let e = make(u32::try_from(v).unwrap());
        (
            i64::try_from(v).unwrap(),
            e.name().is_some(),
            e.to_dotnet_string(),
        )
    })
}

fn u16e<E: AceEnum>(s: &str, ic: bool, make: fn(u16) -> E) -> Parsed {
    enum_try_parse::<E>(s, ic, 0, i128::from(u16::MAX)).map(|v| {
        let e = make(u16::try_from(v).unwrap());
        (
            i64::try_from(v).unwrap(),
            e.name().is_some(),
            e.to_dotnet_string(),
        )
    })
}

fn i32e<E: AceEnum>(s: &str, ic: bool, make: fn(i32) -> E) -> Parsed {
    enum_try_parse::<E>(s, ic, i128::from(i32::MIN), i128::from(i32::MAX)).map(|v| {
        let i = u32::try_from(v & u64::from(u32::MAX))
            .unwrap()
            .cast_signed();
        let e = make(i);
        (i64::from(i), e.name().is_some(), e.to_dotnet_string())
    })
}

/// `(value, defined, text)` of one of the developer handlers' `Enum.TryParse` calls.
fn parse_enum(name: &str, s: &str, ic: bool) -> Parsed {
    match name {
        "ChatMessageType" => u32e(s, ic, ChatMessageType),
        "Sound" => u32e(s, ic, Sound),
        "PlayScript" => u32e(s, ic, PlayScript),
        "MotionCommand" => u32e(s, ic, MotionCommand),
        "PositionType" => u16e(s, ic, PositionType),
        "WeenieType" => u32e(s, ic, WeenieType),
        "EquipMask" => u32e(s, ic, EquipMask),
        "AetheriaColor" => i32e(s, ic, AetheriaColorEnum),
        "Sigil" => i32e(s, ic, SigilEnum),
        "Surge" => i32e(s, ic, SurgeEnum),
        "PropertyInt" => u16e(s, ic, PropertyInt),
        "PropertyInt64" => u16e(s, ic, PropertyInt64),
        "PropertyBool" => u16e(s, ic, PropertyBool),
        "PropertyFloat" => u16e(s, ic, PropertyFloat),
        "PropertyString" => u16e(s, ic, PropertyString),
        "PropertyInstanceId" => u16e(s, ic, PropertyInstanceId),
        "PropertyDataId" => u16e(s, ic, PropertyDataId),
        other => panic!("unknown enum {other}"),
    }
}

#[test]
fn enum_try_parse_matches_net() {
    let file = vectors::load_named("commands", "dev_enum_try_parse");
    for case in &file.cases {
        let name = case.input["enum"].as_str().unwrap();
        let s = case.input["s"].as_str().unwrap();
        let ic = case.input["ignore_case"].as_bool().unwrap();
        let got = parse_enum(name, s, ic);
        let o = &case.output;
        assert!(vectors::throws(o).is_none(), "{name} {s:?} throws in .NET");
        if o["ok"].as_bool().unwrap() {
            let (value, defined, text) =
                got.unwrap_or_else(|| panic!("{name}.TryParse({s:?}, {ic}) should succeed"));
            assert_eq!(Some(value), i64_of(&o["value"]), "{name} {s:?}");
            assert_eq!(
                Some(defined),
                o["defined"].as_bool(),
                "{name} {s:?} defined"
            );
            assert_eq!(text, o["text"].as_str().unwrap(), "{name} {s:?} text");
        } else {
            assert!(
                got.is_none(),
                "{name}.TryParse({s:?}, {ic}) should fail, got {got:?}"
            );
        }
    }
    assert!(!file.cases.is_empty(), "the recorded cases are present");
}

#[test]
fn nudges_position_text_matches_ace() {
    let file = vectors::load_named("commands", "dev_position_format");
    for case in &file.cases {
        let i = &case.input;
        let f = |k: &str| vectors::f32_of(&i[k]).unwrap();
        let cell = u32::try_from(i["cell"].as_u64().unwrap()).unwrap();
        let pos = Position::from_components(
            cell,
            f("x"),
            f("y"),
            f("z"),
            f("rx"),
            f("ry"),
            f("rz"),
            f("rw"),
            false,
        );
        assert_eq!(
            dc::postion_as_landblocks_google_spreadsheet_format(&pos),
            case.output.as_str().unwrap(),
            "{i}"
        );
    }
}

/// `ItemType` values whose echo differs from ACE's recording because `CraftFletchingBase` takes
/// the retail client's 0x1000000 (V327, V327): 0x12345678 has bit 0x2000000, which ACE named
/// `CraftFletchingBase` and the client names nothing, so .NET's flags formatting falls back to
/// the number. `(parameters, the echo with the client's enum)`.
const RETAIL_RULED_ECHOES: &[(&[&str], &str)] = &[
    (&["type", "305419896"], "ItemType = 305419896 (305419896)"),
    (&["Type", "305419896"], "ItemType = 305419896 (305419896)"),
];

#[test]
fn echoflags_matches_ace() {
    let file = vectors::load_named("commands", "dev_echo_flags");
    let mut bad = Vec::new();
    let mut ruled = 0;
    for case in &file.cases {
        let parameters: Vec<String> = case.input["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .collect();
        let mut expected: Vec<&str> = case.output["console"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        if let Some(&(_, echo)) = RETAIL_RULED_ECHOES.iter().find(|r| {
            r.0.iter()
                .copied()
                .eq(parameters.iter().map(String::as_str))
        }) {
            assert_eq!(
                expected.len(),
                1,
                "{parameters:?}: one echo line in ACE's recording"
            );
            assert!(
                expected[0].contains("CraftFletchingBase"),
                "{parameters:?}: ACE named the bit"
            );
            expected = vec![echo];
            ruled += 1;
        }
        let got = dc::echo_flags_output(&parameters);
        if got.iter().map(String::as_str).collect::<Vec<_>>() != expected {
            bad.push(format!("{parameters:?}: {got:?} != {expected:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} mismatches:\n{}",
        bad.len(),
        bad.join("\n")
    );
    assert_eq!(
        ruled,
        RETAIL_RULED_ECHOES.len(),
        "every retail-ruled echo is in ACE's recording"
    );
}

/// The console lines one console command wrote.
fn console(ts: &mut TestServer, handler: CommandHandler, parameters: &[&str]) -> Vec<String> {
    let parameters: Vec<String> = parameters.iter().map(|p| (*p).to_owned()).collect();
    command_manager::start_console_capture();
    handler(&mut ts.world, None, &parameters);
    command_manager::take_console_output()
}

#[test]
fn listplayers_from_the_console_matches_ace() {
    let mut ts = TestServer::new();
    let file = vectors::load_named("commands", "dev_list_players");
    for case in &file.cases {
        let parameters: Vec<&str> = case.input["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let expected: Vec<&str> = case.output["console"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect();
        assert_eq!(
            console(&mut ts, dc::handle_list_players, &parameters),
            expected,
            "{parameters:?}"
        );
    }
}

#[test]
fn showstats_skill_order_matches_net() {
    let file = vectors::load_named("commands", "dev_skill_order");
    let case = &file.cases[0];
    let mut names: Vec<String> = case.input["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    // (the names are every distinct Skill value's ToString())
    let mut ours: Vec<String> = Vec::new();
    for s in Skill::MEMBERS {
        let n = s.to_dotnet_string();
        if !ours.contains(&n) {
            ours.push(n);
        }
    }
    assert_eq!(ours, names, "Skill names");
    names.sort_by(|a, b| culture_compare(a, b));
    let expected: Vec<&str> = case
        .output
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert_eq!(names, expected);
}

#[test]
fn console_commands_write_to_the_console() {
    let mut ts = TestServer::new();

    // getspellformula / getallspellformula: the usage line on a wrong count or a bad id
    assert_eq!(
        console(&mut ts, dc::get_spell_formula, &[]),
        ["getspellformula <accountname> <spellid>"]
    );
    assert_eq!(
        console(&mut ts, dc::get_spell_formula, &["acct", "x"]),
        ["getspellformula <accountname> <spellid>"]
    );
    assert_eq!(
        console(&mut ts, dc::get_all_spell_formula, &["a", "b"]),
        ["getallspellformula <accountname>"]
    );

    // the collector commands answer as ACE does
    assert_eq!(
        console(&mut ts, dc::handle_force_gc, &[]),
        [".NET Garbage Collection forced"]
    );
    assert_eq!(
        console(&mut ts, dc::handle_force_gc2, &[]),
        [".NET Garbage Collection forced with LOH Compact"]
    );
    assert_eq!(
        console(&mut ts, dc::handle_clear_physics_caches, &[]),
        ["Physics caches cleared"]
    );
    // from the console the audit only logs (its WriteOutputInfo needs a session)
    assert_eq!(
        console(&mut ts, dc::handle_audit_object_maint, &[]),
        Vec::<String>::new()
    );

    // netstats: NetworkStatistics.Summary
    let summary = ts.world.net.statistics().summary();
    assert_eq!(console(&mut ts, dc::handle_net_stats, &[]), [summary]);

    // echoflags on the console (CommandHandlerFlag.None)
    assert_eq!(
        console(&mut ts, dc::handle_debug_echo_flags, &["type", "3"]),
        ["ItemType = MeleeWeapon, Armor (3)"]
    );

    // show-wielded-treasure: a bad wcid, a weenie that is not there
    assert_eq!(
        console(&mut ts, dc::handle_show_wielded_treasure, &["x"]),
        ["Invalid wcid x"]
    );
    assert_eq!(
        console(&mut ts, dc::handle_show_wielded_treasure, &["999999"]),
        ["Couldn't find weenie 999999"]
    );
}

#[test]
fn developer_console_lines_run_on_the_world_thread() {
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    use empyrean_command::command_manager::WorldCommand;

    command_manager::initialize(None);
    let queued: Arc<Mutex<Vec<WorldCommand>>> = Arc::default();
    let q = Arc::clone(&queued);
    let submit = move |c: WorldCommand| {
        q.lock().unwrap().push(c);
        true
    };

    command_manager::start_console_capture();
    command_manager::command_thread(
        Cursor::new("listplayers developer\ngetspellformula acct\nlistcb 5\nforcegc\n"),
        &submit,
    );
    let _banner = command_manager::take_console_output();

    // nothing ran yet: the lookups and invokes wait for the world thread
    let commands: Vec<WorldCommand> = std::mem::take(&mut *queued.lock().unwrap());
    assert_eq!(commands.len(), 4);

    let mut ts = TestServer::new();
    command_manager::start_console_capture();
    for c in commands {
        c(&mut ts.world);
    }
    let out = command_manager::take_console_output();
    assert_eq!(
        out,
        [
            "Listing only Developers:\nTotal connected Players: 0\n",
            "getspellformula <accountname> <spellid>",
            // listcb (ConsoleInvoke): the fake dats hold no clothing table
            "There are 0 compatible clothingbase tables for setup 5",
            "",
            "",
            ".NET Garbage Collection forced",
        ]
    );
}

/// `ciaetheria` gives the new aetheria the sigil slot of its color (ACE's `ColorToMask`), so it
/// can be equipped: blue in the first sigil slot, yellow the second, red the third.
#[test]
fn ciaetheria_valid_locations_follow_the_color() {
    let _ = empyrean_testkit::TestServer::take_not_ported();
    assert_eq!(dc::aetheria_color_to_mask(0), EquipMask::SigilOne);
    assert_eq!(dc::aetheria_color_to_mask(1), EquipMask::SigilTwo);
    assert_eq!(dc::aetheria_color_to_mask(2), EquipMask::SigilThree);
    assert!(
        !empyrean_testkit::TestServer::take_not_ported().contains_key("ACE: Aetheria.ColorToMask")
    );
}
