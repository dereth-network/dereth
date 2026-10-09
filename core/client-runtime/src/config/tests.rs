use super::*;

/// Behaviour: none (the object identity budget distinguishes connected and offline startup).
#[test]
fn connected_clients_work_out_the_object_identity_in_the_background_unless_told_not_to() {
    let mut cfg = Config::default();
    assert_eq!(cfg.scene_config().object_identity_budget, None);
    assert!(cfg.scene_config().object_identity_cache);
    cfg.account = "account".into();
    cfg.host = "127.0.0.1".into();
    assert_eq!(
        cfg.scene_config().object_identity_budget,
        Some(crate::app::IDENTITY_BACKGROUND_BUDGET)
    );
    cfg.object_identity_ms = Some(0);
    assert_eq!(cfg.scene_config().object_identity_budget, None);
    cfg.object_identity_ms = Some(5);
    cfg.connect = false;
    assert_eq!(
        cfg.scene_config().object_identity_budget,
        Some(std::time::Duration::from_millis(5))
    );
    let mut parsed = Config::default();
    parsed
        .parse_args(&["--object-identity-ms".to_string(), "2".to_string()])
        .expect("the switch parses");
    assert_eq!(parsed.object_identity_ms, Some(2));
}

/// Behaviour: none (scene streaming budgets distinguish connected and offline startup).
#[test]
fn connected_scenes_stream_unless_the_budget_is_explicitly_disabled() {
    let mut cfg = Config::default();
    assert_eq!(cfg.scene_config().stream_budget, None);
    cfg.account = "account".into();
    cfg.host = "127.0.0.1".into();
    assert_eq!(
        cfg.scene_config().stream_budget,
        Some(std::time::Duration::from_millis(4))
    );
    cfg.stream_budget_ms = Some(0);
    assert_eq!(cfg.scene_config().stream_budget, None);
    cfg.stream_budget_ms = Some(9);
    assert_eq!(
        cfg.scene_config().stream_budget,
        Some(std::time::Duration::from_millis(9))
    );
    cfg.connect = false;
    assert_eq!(
        cfg.scene_config().stream_budget,
        Some(std::time::Duration::from_millis(9))
    );
    cfg.stream_budget_ms = None;
    assert_eq!(cfg.scene_config().stream_budget, None);
}

/// Behaviour: none (scene startup carries profile values and the selected terrain policy).
#[test]
fn scene_startup_keeps_profile_values_and_each_terrain_mode() {
    let mut cfg = Config {
        landblock: 0xA9B4,
        start_cell: Some(0xA9B4_0100),
        land_radius: 3,
        scenery_radius: 2,
        time_of_day: Some(0.25),
        ..Config::default()
    };
    cfg.render.field_of_view = 75.0;
    for (mode, merge, splat) in [
        (crate::render_prefs::TerrainBlending::Cpu, false, false),
        (crate::render_prefs::TerrainBlending::Gpu, true, false),
        (crate::render_prefs::TerrainBlending::Splat, true, true),
    ] {
        cfg.terrain_blending = mode;
        let scene = cfg.scene_config();
        assert_eq!(scene.landblock, 0xA9B4);
        assert_eq!(
            scene.start_cell,
            Some(dereth_primitives::CellId(0xA9B4_0100))
        );
        assert_eq!((scene.land_radius, scene.scenery_radius), (3, 2));
        assert_eq!(scene.time_of_day, Some(0.25));
        assert_eq!(scene.render, cfg.render);
        assert_eq!(scene.camera, cfg.camera);
        assert_eq!(scene.mouse_look, cfg.mouse_look);
        assert_eq!(
            (scene.gpu_terrain_merge, scene.terrain_splat),
            (merge, splat)
        );
    }
}

fn parse(args: &[&str]) -> Result<Config, ConfigError> {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    Config::from_args_and_prefs_with(&argv, &Preferences::default())
}

/// `[Render] Ground` and `[Render] Sky` choose a style each, read in any of their spellings
/// (the older `software`, `hardware` and `later` included); absent, or anything else, is the
/// world's own.
#[test]
fn the_ground_and_sky_preferences_choose_a_style_and_default_to_the_worlds_own() {
    use crate::render_prefs::RegionStyle;
    let styles = |text: &str| {
        let s = Config::from_args_and_prefs_with(&[], &Preferences::parse(text))
            .expect("parses")
            .scene_config();
        (s.render.ground, s.render.sky)
    };
    assert_eq!(
        styles("[Render]\nGround=PaletteShift\nSky=Legacy Hardware\n"),
        (
            Some(RegionStyle::LegacySoftware),
            Some(RegionStyle::LegacyHardware)
        )
    );
    assert_eq!(
        styles("[Render]\nGround=LegacyBlend\nSky=modern\n"),
        (Some(RegionStyle::LegacyHardware), Some(RegionStyle::Late))
    );
    assert_eq!(
        styles("[Render]\nGround=LATER\n"),
        (Some(RegionStyle::Late), None)
    );
    assert_eq!(
        styles("[Render]\nGround=software\n"),
        (Some(RegionStyle::LegacySoftware), None)
    );
    assert_eq!(styles("[Render]\nGround=hardware\n"), (None, None));
    assert_eq!(styles("[Render]\nGround=tod\nSky=World\n"), (None, None));
    assert_eq!(styles(""), (None, None));
}

/// `[Render] Objects` chooses the objects' look, and `--object-visuals` wins over it; either
/// older style is the one older look.
#[test]
fn the_object_visuals_come_from_the_switch_before_the_preference() {
    use crate::render_prefs::RegionStyle;
    let objects = |args: &[&str], text: &str| {
        let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let c = Config::from_args_and_prefs_with(&argv, &Preferences::parse(text)).expect("parses");
        (c.scene_config().render.objects, c.object_visuals)
    };
    assert_eq!(objects(&[], ""), (None, None));
    assert_eq!(
        objects(&[], "[Render]\nObjects=Legacy\n"),
        (Some(RegionStyle::LegacyHardware), None)
    );
    assert_eq!(
        objects(&[], "[Render]\nObjects=PaletteShift\n"),
        (Some(RegionStyle::LegacyHardware), None)
    );
    assert_eq!(
        objects(
            &["--object-visuals", "modern"],
            "[Render]\nObjects=Legacy\n"
        ),
        (Some(RegionStyle::Late), Some(Some(RegionStyle::Late)))
    );
    assert_eq!(
        objects(&["--object-visuals", "world"], "[Render]\nObjects=Legacy\n"),
        (None, Some(None))
    );
    let argv = vec!["--object-visuals".to_string(), "sideways".to_string()];
    assert!(Config::from_args_and_prefs_with(&argv, &Preferences::parse("")).is_err());
}

/// Behaviour: none (tooling: the switch only says where the older data files are)
#[test]
fn the_classic_dat_dir_switch_names_where_the_older_files_are_and_is_optional() {
    let c = parse(&["--dat-dir", "eor", "--classic-dat-dir", "feb2005"]).expect("parses");
    assert_eq!(c.dat_dir, PathBuf::from("eor"));
    assert_eq!(c.classic_dat_dir, Some(PathBuf::from("feb2005")));
    assert_eq!(parse(&[]).expect("parses").classic_dat_dir, None);
    let c = parse(&["--overlay-dat-dir", "worlds/one"]).expect("parses");
    assert_eq!(c.overlay_dat_dir, Some(PathBuf::from("worlds/one")));
    let c = parse(&["--world-base", "Modern"]).expect("parses");
    assert_eq!(c.world_base, Some(dereth_primitives::ContainerEra::Modern));
    let c = parse(&["--world-base", "classic"]).expect("parses");
    assert_eq!(c.world_base, Some(dereth_primitives::ContainerEra::Classic));
    assert!(parse(&["--world-base", "eor"]).is_err());
    for retired in ["--world-dat-dir", "--legacy-dat-dir"] {
        assert!(parse(&[retired, "x"]).is_err(), "{retired}");
    }
}

/// Behaviour: login.logon-version.a-world-that-wants-another-logon-version-is-sent-it
#[test]
fn the_logon_version_switch_replaces_1802_and_refuses_what_cannot_be_sent() {
    assert_eq!(parse(&[]).expect("parses").logon_version, "1802");
    assert_eq!(
        parse(&["--logon-version", "c118"])
            .expect("parses")
            .logon_version,
        "c118"
    );
    for bad in ["", " ", "c 118", "c118\u{e9}"] {
        assert!(parse(&["--logon-version", bad]).is_err(), "{bad:?}");
    }
}

/// Behaviour: world.rules.a-world-profile-sets-its-clients-rules-over-the-end-of-retails
#[test]
fn the_world_profile_switch_takes_a_compiled_profile_and_refuses_any_other() {
    let none = parse(&[]).expect("parses");
    assert!(none.world_rules.is_end_of_retail());
    let c = parse(&["--world-profile", "classicace-customdm"]).expect("parses");
    assert_eq!(c.world_rules.burden_strength_bonus, 40);
    assert_eq!(c.world_rules.run_scale, 1.5);
    assert_eq!(
        c.world_rules.profile.as_deref(),
        Some("classicace-customdm")
    );
    let i = parse(&["--world-profile", "classicace-infiltration"]).expect("parses");
    assert_eq!(i.world_rules.burden_strength_bonus, 0);
    assert!(!i.world_rules.recklessness_marker);
    let e = parse(&["--world-profile", "nowhere"]).unwrap_err().detail;
    assert!(
        e.contains("classicace-customdm"),
        "names the known ones: {e}"
    );
}

/// `--set-at` and `--capture-at` collect a frame and what to do there, in order.
#[test]
fn a_run_can_set_a_preference_and_take_a_picture_part_way_through() {
    let c = parse(&[
        "--capture-at",
        "4:before.png",
        "--set-at",
        "5:Render.Ground=PaletteShift",
        "--capture-at",
        "9:after.png",
    ])
    .expect("parses");
    assert_eq!(c.set_at, [(5, "Render.Ground=PaletteShift".to_string())]);
    assert_eq!(
        c.capture_at,
        [
            (4, PathBuf::from("before.png")),
            (9, PathBuf::from("after.png"))
        ]
    );
    assert!(parse(&["--set-at", "five:Render.Ground=PaletteShift"]).is_err());
    assert!(parse(&["--set-at", "5:Render.Ground"]).is_err());
    assert!(parse(&["--capture-at", "5:"]).is_err());
}

/// `--action-at` collects a frame and an action named as a key map names it; a name no action
/// has is refused.
#[test]
fn a_run_can_press_an_action_part_way_through() {
    let c = parse(&["--action-at", "7:ToggleAllegiancePanel"]).expect("parses");
    assert_eq!(c.action_at, [(7, 0x1000_000E)]);
    assert!(parse(&["--action-at", "7:NoSuchAction"]).is_err());
    assert!(parse(&["--action-at", "seven:ToggleAllegiancePanel"]).is_err());
}

/// `--era` names the era the server's world plays; an unknown name is refused.
#[test]
fn the_era_switch_names_the_servers_era() {
    let c = parse(&["--era", "Infiltration"]).expect("parses");
    assert_eq!(c.era, Some(dereth_primitives::EraId::Infiltration));
    assert_eq!(parse(&[]).expect("parses").era, None);
    assert!(parse(&["--era", "tod"]).is_err());
}

/// `--era-features` names the systems the server's world has; a system this client does not
/// know is skipped, and a malformed list is refused.
#[test]
fn the_era_features_switch_names_the_servers_systems() {
    let c = parse(&[
        "--era-features",
        "trade=false,aetheria=true,spell_credits=true",
    ])
    .expect("parses");
    assert_eq!(c.era_features.get("trade"), Some(false));
    assert_eq!(c.era_features.get("aetheria"), Some(true));
    assert_eq!(c.era_features.get("chess"), None);
    assert!(parse(&[]).expect("parses").era_features.is_empty());
    assert!(parse(&["--era-features", "trade"]).is_err());
}

/// `--era-features` also takes the shared bitfield, as the launcher passes it: every system of
/// its table is set, and a newer table's systems past this client's are skipped.
///
/// Behaviour: presentation.era.the-worlds-systems-are-read-from-the-shared-bitfield
#[test]
fn the_era_features_switch_takes_the_bitfield() {
    use dereth_primitives::{EraFeatures, EraId};
    let c = parse(&["--era-features", "1:0002de"]).expect("parses");
    let f = c.era_features.apply(EraFeatures::NONE);
    assert!(f.aetheria && !f.trade && f.housing && f.chess && !f.ratings);
    assert_eq!(c.era_features.iter().count(), EraFeatures::COUNT);
    let newer = parse(&["--era-features", "99:ffff5fff"]).expect("parses");
    assert_eq!(
        newer.era_features.apply(EraFeatures::NONE),
        EraId::Eor.features()
    );
    assert!(parse(&["--era-features", "1:zz"]).is_err());
    assert!(parse(&["--era-features", "0:ffff5f"]).is_err());
}

/// `--cast` names the spell a scripted world entry casts; it takes a spell id.
#[test]
fn the_cast_switch_names_the_spell_a_scripted_entry_casts() {
    assert_eq!(parse(&["--cast", "35"]).expect("parses").cast, Some(35));
    assert_eq!(parse(&[]).expect("parses").cast, None);
    assert!(parse(&["--cast", "blood"]).is_err());
}

/// `--say` lines and `--use` targets are kept in the order given.
#[test]
fn the_say_switch_keeps_each_line_in_order() {
    let c = parse(&["--say", "@level 2", "--say", "hello there"]).expect("parses");
    assert_eq!(c.say, ["@level 2", "hello there"]);
    assert!(parse(&[]).expect("parses").say.is_empty());
    let argv: Vec<String> = ["--headless", "--say", "@level 2"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let at = Config::from_args_and_prefs_at(&argv, Path::new(""))
        .expect("parses")
        .say;
    assert_eq!(
        at,
        ["@level 2"],
        "the command line is read twice, the line kept once"
    );
    let c = parse(&["--use", "0x7A9B0000", "--use", "closest", "--use", "logout"]).expect("parses");
    assert_eq!(c.use_targets, ["0x7A9B0000", "closest", "logout"]);
    assert!(parse(&["--use", "door"]).is_err());
}

// Oracle: the complete supported switch table and its per-switch behavior.
#[test]
fn the_documented_switches_parse_to_the_documented_values() {
    let c = parse(&[
        "-a",
        "Ac01",
        "-h",
        "127.0.0.1",
        "-p",
        "19000",
        "-q",
        "9000",
        "-language",
        "English",
        "-usemem",
        "-u",
        "Aren",
        "-r",
        "Aldis",
        "-z",
        "tkt",
        "-vgpassword",
        "pw",
    ])
    .expect("parses");
    // "-a / -account -- the value is ... lower-cased in place with _strlwr."
    assert_eq!(c.account, "ac01");
    assert_eq!(c.host, "127.0.0.1");
    assert_eq!(c.port, 19000);
    assert_eq!(c.client_port, 9000);
    assert_eq!(c.retail.language, "English");
    assert!(c.retail.use_memory_manager);
    assert_eq!(c.start_char, "Aren");
    assert_eq!(c.create_char, "Aldis");
    assert_eq!(c.retail.zone_ticket, "tkt");
    assert_eq!(c.vg_password, "pw");
}

#[test]
fn the_defaults_are_the_constructed_ones() {
    let c = parse(&[]).expect("an empty command line is legal at this layer");
    assert_eq!(c.port, 7304, "the client default port is 0x1C88");
    assert_eq!(c.client_port, 0);
    assert!(c.retail.read_only_dat_files, "read-only dats default on");
    assert!(c.windowed, "windowed defaults on");
    assert_eq!(
        (c.width, c.height),
        (800, 600),
        "device initialization defaults to 800 by 600"
    );
    assert_eq!(
        c.display.resolution, 0x0400_0300,
        "the compiled-in default is 1024x768"
    );
    assert!(!c.display.full_screen, "the client starts in a window");
    // The login path owns the account/host requirement; the parser only records values.
    assert!(c.require_account_and_host().is_err());
}

// "-rodat -- the handler is read_only_dat_files = (value is the empty string). Bare -rodat
// turns read-only dats **on**; -rodat <anything> (the launcher passes `off`) turns them
// **off**. The text of the value is never examined, so `-rodat on` also *disables* read-only
// mode."
#[test]
fn rodat_is_backwards_and_stays_backwards() {
    assert!(parse(&["-rodat"]).unwrap().retail.read_only_dat_files);
    assert!(
        !parse(&["-rodat", "off"])
            .unwrap()
            .retail
            .read_only_dat_files
    );
    assert!(!parse(&["-rodat", "on"]).unwrap().retail.read_only_dat_files);
    // Value optional: a following switch is re-processed rather than consumed.
    let c = parse(&["-rodat", "-usemem"]).unwrap();
    assert!(c.retail.read_only_dat_files);
    assert!(c.retail.use_memory_manager);
}

/// The renderer is selected by the switch over the preference over the default.
#[test]
fn the_renderer_is_selected_by_the_switch_over_the_preference_over_the_default() {
    use dereth_client_contract::RendererChoice as Backend;

    // Neither: the choice is left open, which `App::device_presentation` reads as
    // the default.
    assert_eq!(parse(&[]).unwrap().renderer, None);

    // The switch, in both spellings the parser accepts, and case-insensitively.
    assert_eq!(
        parse(&["--renderer", "d3d12"]).unwrap().renderer,
        Some(Backend::D3d12)
    );
    assert_eq!(
        parse(&["--renderer", "vulkan"]).unwrap().renderer,
        Some(Backend::Vulkan)
    );
    assert_eq!(
        parse(&["--RENDERER", "D3D12"]).unwrap().renderer,
        Some(Backend::D3d12)
    );

    // A name that is not a backend is a parse error, not a silent default.
    let e = parse(&["--renderer", "opengl"]).expect_err("opengl is not a backend");
    assert!(
        e.detail.contains("--renderer wants vulkan, d3d12 or wgpu"),
        "{}",
        e.detail
    );

    // The preference, in the `[Render]` section and with no section at all.
    let p = Preferences::parse(
        "[Render]
Renderer=d3d12
",
    );
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!(c.renderer, Some(Backend::D3d12));
    let p = Preferences::parse(
        "Renderer=d3d12
",
    );
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!(c.renderer, Some(Backend::D3d12));

    // The switch wins over the preference: `from_args_and_prefs_with` applies the profile and
    // then the command line, which is the order the startup path uses.
    let p = Preferences::parse(
        "[Render]
Renderer=d3d12
",
    );
    let argv = vec!["--renderer".to_string(), "vulkan".to_string()];
    let c = Config::from_args_and_prefs_with(&argv, &p).unwrap();
    assert_eq!(c.renderer, Some(Backend::Vulkan));
    // Each source is kept apart too, for the options page's renderer choice.
    assert_eq!(c.renderer_preference, Some(Backend::D3d12));
    assert_eq!(c.renderer_argument, Some(Backend::Vulkan));
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!(c.renderer_argument, None);

    // A preference naming nothing this client knows leaves the choice open rather than
    // failing the start-up, as an unparsable numeric preference leaves its default standing.
    let p = Preferences::parse(
        "[Render]
Renderer=glide
",
    );
    assert_eq!(
        Config::from_args_and_prefs_with(&[], &p).unwrap().renderer,
        None
    );
}

/// The log's three switches and its two preferences, with the command line winning.
#[test]
fn the_log_is_configured_by_switch_and_by_preference() {
    let c = parse(&[]).unwrap();
    assert_eq!(
        (c.log_filter.as_deref(), c.log_file, c.log_spans),
        (None, false, false)
    );

    let c = parse(&[
        "--log",
        "info,dereth_client_runtime::frame=trace",
        "--log-file",
        "--log-spans",
    ])
    .unwrap();
    assert_eq!(
        c.log_filter.as_deref(),
        Some("info,dereth_client_runtime::frame=trace")
    );
    assert!(c.log_file && c.log_spans);
    // `--log` takes a value; with none it is a parse error like any other required value.
    assert!(parse(&["--log"]).is_err());

    let p = Preferences::parse("[Log]\nLevel=debug\nFile=True\n");
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!((c.log_filter.as_deref(), c.log_file), (Some("debug"), true));
    let argv = vec!["--log".to_string(), "warn".to_string()];
    let c = Config::from_args_and_prefs_with(&argv, &p).unwrap();
    assert_eq!(c.log_filter.as_deref(), Some("warn"));
    // An empty preference is no preference.
    let p = Preferences::parse("[Log]\nLevel=\n");
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!(c.log_filter, None);
}

/// Behaviour: none (tooling: what the crash log may carry of the command line).
#[test]
fn a_logged_command_line_hides_the_account_password_and_tickets() {
    let argv = [
        "dereth-client",
        "-a",
        "acct",
        "-v",
        "secret",
        "-h",
        "host:9000",
        "--vgpassword",
        "again",
        "/z",
        "ticket",
        "-glsticketdirect",
        "gls",
        "-ACCOUNT",
        "loud",
        "-A",
        "upper",
        "-glsticket",
        "--headless",
        "-u",
        "-v",
    ];
    assert_eq!(
        argv_for_log(&argv),
        [
            "dereth-client",
            "-a",
            "***",
            "-v",
            "***",
            "-h",
            "host:9000",
            "--vgpassword",
            "***",
            "/z",
            "***",
            "-glsticketdirect",
            "***",
            "-ACCOUNT",
            "***",
            "-A",
            "***",
            "-glsticket",
            "--headless",
            "-u",
            "-v",
        ]
    );
}

// Client initialization sets the command characters to "-/", so both prefixes are accepted.
#[test]
fn both_command_characters_are_accepted() {
    assert_eq!(parse(&["/host", "example"]).unwrap().host, "example");
    assert_eq!(parse(&["-host", "example"]).unwrap().host, "example");
}

// "Exactly one leading command character is stripped, so --rodat is looked up as the long name
// -rodat and fails."
#[test]
fn exactly_one_command_character_is_stripped() {
    let e = parse(&["--rodat"]).expect_err("--rodat is not a name");
    assert!(
        e.detail.contains("Unrecognized command line argument"),
        "{}",
        e.detail
    );
    // ... and every command-line error surfaces as corestrings 205.
    assert_eq!(
        e.to_string(),
        "You can only run the game from the launcher program."
    );
}

// "otherwise long-name lookup (case **insensitive**)"; short names are case sensitive.
#[test]
fn long_names_are_case_insensitive_and_short_names_are_not() {
    assert_eq!(parse(&["-HOST", "h"]).unwrap().host, "h");
    assert_eq!(parse(&["-a", "X"]).unwrap().account, "x");
    // 'A' is not a registered short name, and "A" is not a long name either.
    assert!(parse(&["-A", "X"]).is_err());
}

// "2 | **requires** a value; the next token is consumed as the value even if it starts with a
// command character. An empty token errors with `%S requires a value`"
#[test]
fn a_required_value_swallows_the_next_token_whatever_it_is() {
    let c = parse(&["-h", "-usemem"]).unwrap();
    assert_eq!(c.host, "-usemem");
    assert!(
        !c.retail.use_memory_manager,
        "the switch was eaten as -host's value"
    );
    let e = parse(&["-h"]).expect_err("no value at all");
    assert!(e.detail.contains("requires a value"), "{}", e.detail);
}

// "-outport -- after assignment the handler range-checks 1 <= client_port <= 65535 ...
// -port is **not** range-checked despite its description."
#[test]
fn outport_is_range_checked_and_port_is_not() {
    assert!(parse(&["-q", "70000"]).is_err());
    assert!(parse(&["-q", "0"]).is_err());
    assert_eq!(parse(&["-p", "70000"]).unwrap().port, 70000);
}

// "-debug -- strtoul(value, 0, 0) (so 0x... hex and leading-zero octal are accepted)"
#[test]
fn debug_takes_a_strtoul_base_zero_mask() {
    assert_eq!(parse(&["-debug", "0x1F"]).unwrap().retail.debug_flags, 0x1F);
    assert_eq!(parse(&["-debug", "31"]).unwrap().retail.debug_flags, 31);
    assert_eq!(parse(&["-debug", "037"]).unwrap().retail.debug_flags, 0o37);
}

#[test]
fn a_stray_bare_token_is_an_error_but_an_empty_one_is_skipped() {
    assert!(parse(&["stray"]).is_err());
    assert!(parse(&[""]).is_ok());
}

// The rebuild-only spelling, which the retail parser could never have resolved.
#[test]
fn the_rebuild_switches_use_two_command_characters() {
    let c = parse(&["--headless", "--frames", "3", "--capture", "out.png"]).unwrap();
    assert!(c.headless);
    assert_eq!(c.frames, Some(3));
    assert_eq!(c.capture, Some(PathBuf::from("out.png")));
    // The single-dash spelling is not a retail name, so it is rejected like any unknown switch.
    assert!(parse(&["-headless"]).is_err());
}

/// The title the owner asked for, and the one case that is not `Dereth | <account>`.
///
/// Client divergence CD-006.
///
/// Behaviour: presentation.window.the-title-names-the-account-as-it-was-typed
#[test]
fn the_window_title_carries_the_account_as_typed() {
    // `-a` is lower-cased for the wire and kept as typed for the title. Both, from one token.
    let c = parse(&["-a", "Tester"]).unwrap();
    assert_eq!(c.account, "tester", "the wire spelling is still _strlwr's");
    assert_eq!(c.window_title(), "Dereth | Tester");
    // No account: the offline slices, which run with no login at all.
    assert_eq!(parse(&[]).unwrap().window_title(), "Dereth");
    // An account of blanks is not an account. `-a` takes the next token whatever it is.
    assert_eq!(parse(&["-a", "   "]).unwrap().window_title(), "Dereth");
}

/// `--no-console` parses like any other arity-`None` rebuild switch, and records itself.
#[test]
fn no_console_is_a_rebuild_switch() {
    assert!(parse(&[]).unwrap().console, "a console by default");
    assert!(!parse(&["--no-console"]).unwrap().console);
    // It takes no value, so the token after it is still a switch in its own right.
    let c = parse(&["--no-console", "--frames", "2"]).unwrap();
    assert!(!c.console);
    assert_eq!(c.frames, Some(2));
    // One command character makes it the long name `-no-console`, which no table has.
    assert!(parse(&["-no-console"]).is_err());
}

/// The pre-parse peek and the parser agree, which is the property that matters: `main` acts on
/// the first and everything downstream reads the second.
#[test]
fn the_console_peek_matches_the_parser() {
    for argv in [
        vec!["--no-console"],
        vec!["--NO-CONSOLE"],
        vec!["//no-console"],
        vec!["-/no-console"],
        vec!["--frames", "2", "--no-console", "--headless"],
    ] {
        let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
        assert!(no_console_in_argv(&owned), "peek missed {argv:?}");
        assert!(!parse(&argv).unwrap().console, "parser missed {argv:?}");
    }
    for argv in [
        vec![],
        vec!["--headless"],
        // A *value* that happens to spell the switch belongs to `--capture`, and the peek does
        // not consume values -- but it also never sees this token as a switch, because a
        // capture path is not one. The parser is the check that the two agree.
        vec!["--capture", "no-console"],
    ] {
        let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
        assert!(
            !no_console_in_argv(&owned),
            "peek false-positive on {argv:?}"
        );
        assert!(
            parse(&argv).unwrap().console,
            "parser disagrees on {argv:?}"
        );
    }
}

// Oracle: the preference parser's multi-separator rule:
// "more than 2 parts -> the value is the remaining parts concatenated **without** the `=`
// separators, so a value containing `=` is corrupted on load."
#[test]
fn a_value_containing_an_equals_sign_is_corrupted_on_load() {
    let p = Preferences::parse("[Net]\nBindInterface=a=b=c\nComputeUniquePort=True\nBare\n");
    assert_eq!(p.get("Net.BindInterface"), Some("abc"));
    assert_eq!(p.bool("Net.ComputeUniquePort"), Some(true));
    // "1 part -> value \"\""
    assert_eq!(p.get("Net.Bare"), Some(""));
    // Names are stored lower-cased, so lookups are case-insensitive.
    assert_eq!(p.get("net.bindinterface"), Some("abc"));
}

#[test]
fn a_missing_preference_file_still_starts() {
    let p = Preferences::load(Path::new("no-such-file-anywhere.ini"));
    assert!(!p.loaded_ok);
    let c = Config::from_args_and_prefs_with(&[], &p).expect("starts anyway");
    assert_eq!(
        c.display.resolution, 0x0400_0300,
        "the compiled-in default survives"
    );
}

// Oracle: display-preference initialization and loading.
// Resolution's stored value is the display-mode word (w<<16)|h.
#[test]
fn the_display_preferences_reach_the_presentation() {
    let p = Preferences::parse(
        "[Display]\nResolution=83887104\nFullScreen=False\nSyncToRefresh=True\nRefreshRate=60\n",
    );
    let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
    assert_eq!(c.display.resolution, 1280 << 16 | 1024);
    assert!(!c.display.full_screen);
    assert!(c.display.sync_to_refresh);
    assert_eq!(c.display.refresh_rate, 60);

    let pres = c
        .load_display_preferences(None, true)
        .expect("1280x1024 is >= 800x600");
    assert_eq!((pres.width, pres.height), (1280, 1024));
    assert!(!pres.full_screen);
    assert!(pres.compatibility.fs_sync_to_display_refresh);
    assert_eq!(
        pres.compatibility.fs_bits_per_pixel, 32,
        "the default-constructed presentation's value"
    );

    // "if (w < 800 || h < 600) return false // caller then fails"
    let mut small = c.clone();
    small.display.resolution = 640 << 16 | 480;
    assert!(small.load_display_preferences(None, true).is_none());

    // A forced resolution overrides the width and height, and a disallowed full-screen mode
    // forces windowed.
    let mut fs = c.clone();
    fs.display.full_screen = true;
    let pres = fs
        .load_display_preferences(Some((800, 600)), false)
        .unwrap();
    assert_eq!((pres.width, pres.height), (800, 600));
    assert!(!pres.full_screen);
}

#[test]
fn the_prefs_switch_replaces_the_path() {
    let c = parse(&["-prefs", "x/y.ini"]).unwrap();
    assert_eq!(c.preferences_file, PathBuf::from("x/y.ini"));
    assert!(c.preferences_named);
}

/// Behaviour: presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file
///
/// The doubled spelling names the same file as the retail one, and neither is needed for a
/// windowed run to use the default.
#[test]
fn either_spelling_of_the_prefs_switch_names_the_settings_file() {
    let c = parse(&["--prefs", "x/y.ini"]).unwrap();
    assert_eq!(c.preferences_file, PathBuf::from("x/y.ini"));
    assert!(c.preferences_named);
    assert!(!parse(&[]).unwrap().preferences_named);
    assert!(
        parse(&["--prefs"]).is_err(),
        "the doubled spelling wants a value too"
    );
}

/// Behaviour: presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file
///
/// A headless run that names no file is given no settings file at all -- nothing is read from
/// the default, and the empty path is what every writer takes as "no file" -- while a
/// windowed run keeps the default, and a headless run that names one keeps and reads it.
#[test]
fn a_headless_run_that_names_no_settings_file_reads_and_keeps_none() {
    let dir = std::env::temp_dir().join(format!(
        "dereth-headless-prefs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let default = dir.join("UserPreferences.ini");
    std::fs::write(&default, "[Render]\r\nRenderer=d3d12\r\n").unwrap();
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

    let headless = Config::from_args_and_prefs_at(&args(&["--headless"]), &default).unwrap();
    assert_eq!(headless.preferences_file, PathBuf::new());
    assert_eq!(headless.renderer, None, "the default file was not read");

    let windowed = Config::from_args_and_prefs_at(&[], &default).unwrap();
    assert_eq!(windowed.preferences_file, default);
    assert_eq!(
        windowed.renderer,
        Some(dereth_client_contract::RendererChoice::D3d12)
    );

    let named = dir.join("named.ini");
    std::fs::write(&named, "[Render]\r\nRenderer=wgpu\r\n").unwrap();
    let path = named.to_str().unwrap();
    let asked =
        Config::from_args_and_prefs_at(&args(&["--headless", "--prefs", path]), &default).unwrap();
    assert_eq!(asked.preferences_file, named);
    assert_eq!(
        asked.renderer,
        Some(dereth_client_contract::RendererChoice::Wgpu)
    );
    // A settings folder named outright (in the environment) is asked for too: the headless
    // run reads its file and keeps it to write back.
    let folder =
        Config::from_args_and_prefs_named_at(&args(&["--headless"]), &default, true).unwrap();
    assert_eq!(folder.preferences_file, default);
    assert_eq!(
        folder.renderer,
        Some(dereth_client_contract::RendererChoice::D3d12)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The files that hang off the settings directory all take it from the *parent of the
/// preferences file*, which is the settings directory. (The keymap file is the front end's;
/// its own test asserts the same join.)
/// This asserts the join, so that moving the default moves all of them together and `-prefs`
/// still moves all of them together.
#[test]
fn everything_the_client_saves_hangs_off_the_preferences_files_directory() {
    let prefs = PathBuf::from("root/dir").join(PREFERENCES_FILE_NAME);
    let dir = prefs.parent().expect("a parent");
    assert_eq!(dir, std::path::Path::new("root/dir"));

    // The journal, with no prefix on it.
    let journal = dereth_client_contract::journal::JournalIdentity {
        directory: dir.to_path_buf(),
        world: "Frostfell".into(),
        character: "Tester".into(),
    };
    assert_eq!(journal.client_path().parent(), Some(dir));
    assert_eq!(
        journal.client_path().file_name().and_then(|n| n.to_str()),
        Some("Journal-Frostfell-Tester.txt")
    );

    // The gameplay screen-layout path.
    let layout =
        dereth_client_contract::persist::ScreenLayout::default_path(&dir.to_string_lossy());
    assert!(layout.starts_with("root/dir"), "{layout}");
}
