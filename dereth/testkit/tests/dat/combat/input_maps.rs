use super::*;

// ---------------------------------------------------------------------------------------------
// combat.input-map.*
//
// **Nothing here writes a key name or a map's contents down.** The contested controls are
// discovered from the shipped melee section, the quick bar's are discovered from the shipped
// quick-bar section, and the calibration control is the one the shipped combat section binds --
// so the population is the shipped data and the claim is what the client does with it.
//
// **The shipped conflict table is not read here.** That the three mode maps are declared not to
// clash with one another, and that the quick bar and the window commands are, is the shipped data's
// own statement and not a behaviour of this client; it stays behind the evidence handle, and what
// it justified -- which map really takes each contested key -- is driven below through the client's
// own input shell.
//
// **One narrowing, stated rather than hidden.** The peace-mode registration band is not pinned as
// a literal list of map ids: that would be a transcription of the client's own start-up table.
// What is asserted instead is every claim such a list would carry: the combat map is up throughout,
// the mode's map is in front of it, the quick bar is in front of the window commands, and the
// band comes back bit for bit when the player leaves combat.
// ---------------------------------------------------------------------------------------------

pub(super) fn exactly_one_combat_map_is_registered_and_it_follows_the_mode() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let at_rest = (
        shell.combat_input_mode() == mode::NONCOMBAT,
        maps::live_mode_maps(&shell).is_empty(),
        maps::none_at_start_up(),
        maps::band(&shell).contains(&maps::COMBAT_MAP),
        !shell.set_combat_input_maps(mode::NONCOMBAT),
    );

    let mut stations = Vec::new();
    for (m, want) in [
        (mode::MELEE, maps::MELEE_MAP),
        (mode::MISSILE, maps::MISSILE_MAP),
        (mode::MAGIC, maps::MAGIC_MAP),
        (mode::MELEE, maps::MELEE_MAP),
    ] {
        let changed = shell.set_combat_input_maps(m);
        stations.push((
            changed,
            maps::live_mode_maps(&shell) == vec![want],
            maps::band(&shell).first().copied() == Some(want),
            !shell.set_combat_input_maps(m),
        ));
    }

    let back = (
        shell.set_combat_input_maps(mode::NONCOMBAT),
        maps::live_mode_maps(&shell).is_empty(),
        maps::band(&shell).contains(&maps::COMBAT_MAP),
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.exactly-one-is-registered-and-it-follows-the-mode",
        move |_| {
            at_rest == (true, true, true, true, true)
                && stations.iter().all(|s| *s == (true, true, true, true))
                && back == (true, true, true)
        },
    );
}

pub(super) fn the_mode_decides_which_map_takes_the_contested_keys() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let contested = maps::contested_controls(&shell);
    // The key that belongs to combat itself: it must answer in every mode, or a silence below
    // would only mean the driver is dead.
    let calibration = maps::the_combat_maps_own_control(&shell);

    // Peace: every contested key reaches nothing, and the calibration key still answers.
    let peace = (
        contested
            .iter()
            .filter(|q| d.resolve(&mut shell, q).is_none())
            .count(),
        d.resolve(&mut shell, &calibration).is_some(),
    );

    // The nine readings: three modes over three maps, five controls each.
    let mut readings = Vec::new();
    for (m, want) in [
        (mode::MELEE, maps::MELEE_MAP),
        (mode::MISSILE, maps::MISSILE_MAP),
        (mode::MAGIC, maps::MAGIC_MAP),
    ] {
        shell.set_combat_input_maps(m);
        let mut per_map = [0usize; 3];
        for q in &contested {
            let (gm, _) = d
                .resolve(&mut shell, q)
                .unwrap_or_else(|| panic!("a contested key reached nothing in mode {m}"));
            if let Some(i) = maps::MODE_MAPS.iter().position(|x| *x == gm.0) {
                per_map[i] += 1;
            }
        }
        let want_row: [usize; 3] =
            std::array::from_fn(|i| usize::from(maps::MODE_MAPS[i] == want) * contested.len());
        readings.push((per_map, want_row));
        // And the calibration key still answers with a mode map up.
        assert!(
            d.resolve(&mut shell, &calibration).is_some(),
            "the combat key answers in {m}"
        );
    }
    let five = contested.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.the-mode-decides-which-map-takes-the-contested-keys",
        move |_| {
            five > 0
                && peace == (five, true)
                && readings.len() == 3
                && readings.iter().all(|(got, want)| got == want)
        },
    );
}

pub(super) fn every_shipped_quickslot_key_reaches_the_toolbar() {
    use dereth_input::combat::mode;
    use dereth_ui_screens::toolbar::shortcuts::dispatch;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let bindings = maps::shipped_bindings(&shell, maps::QUICKSLOT_MAP);
    assert!(
        bindings.len() > 20,
        "the shipped quick-bar section is a real one"
    );

    // The registration order the keys they share with the window commands turn on.
    let band = maps::band(&shell);
    let ordered = match (
        band.iter().position(|m| *m == maps::QUICKSLOT_MAP),
        band.iter().position(|m| *m == maps::UI_COMMANDS_MAP),
    ) {
        (Some(q), Some(u)) => q < u,
        _ => false,
    };

    // Peace and the two modes that bind no number key: every shipped quick-bar key reaches the
    // quick bar, resolves to the action it names, and the toolbar accepts it.
    let mut quiet_modes = Vec::new();
    for m in [mode::NONCOMBAT, mode::MELEE, mode::MISSILE] {
        shell.set_combat_input_maps(m);
        let mut reached = 0;
        for (want, q) in &bindings {
            if let Some((gm, ga)) = d.resolve(&mut shell, q) {
                if gm == maps::quick() {
                    assert_eq!(ga, *want, "the quick bar answered a different action");
                    assert!(
                        dispatch(ga.0, false).is_some(),
                        "the toolbar takes every action its own section names"
                    );
                    reached += 1;
                }
            }
        }
        quiet_modes.push(reached);
    }

    // Magic: the plain number keys go to the spell bar instead. Discovered, not counted here.
    shell.set_combat_input_maps(mode::MAGIC);
    let mut shadowed = 0usize;
    let mut reached_in_magic = 0usize;
    let mut every_shadow_is_a_spell = true;
    let mut every_modified_key_survives = true;
    for (want, q) in &bindings {
        match d.resolve(&mut shell, q) {
            Some((gm, ga)) if gm == maps::quick() => {
                assert_eq!(ga, *want);
                reached_in_magic += 1;
            }
            Some((gm, _)) => {
                shadowed += 1;
                every_shadow_is_a_spell &= gm.0 == maps::MAGIC_MAP;
                every_modified_key_survives &= q.meta_mode == 0;
            }
            None => every_shadow_is_a_spell = false,
        }
    }
    let total = bindings.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.every-shipped-quickslot-key-reaches-the-toolbar-except-the-nine-magic-takes",
        move |_| {
            ordered
                && quiet_modes == vec![total, total, total]
                && shadowed > 0
                && shadowed < total
                && reached_in_magic == total - shadowed
                && every_shadow_is_a_spell
                && every_modified_key_survives
        },
    );
}

pub(super) fn the_combat_map_itself_is_always_up_and_never_moves() {
    use dereth_input::combat::mode;

    let mut shell = maps::shell();
    let before = maps::band(&shell);
    let mut once_each_time = true;
    for m in [mode::MELEE, mode::MISSILE, mode::MAGIC, mode::NONCOMBAT] {
        shell.set_combat_input_maps(m);
        once_each_time &= maps::band(&shell)
            .iter()
            .filter(|x| **x == maps::COMBAT_MAP)
            .count()
            == 1;
    }
    let unchanged = maps::band(&shell) == before;

    // And the registration itself, driven directly rather than through the mode latch, with the
    // two arguments the client's own mode change passes it.
    let mut s = shell;
    s.register_combat_input_maps(mode::MAGIC, mode::NONCOMBAT);
    let magic_first = maps::band(&s).first().copied() == Some(maps::MAGIC_MAP);
    s.register_combat_input_maps(mode::MELEE, mode::MAGIC);
    let swapped = {
        let b = maps::band(&s);
        !b.contains(&maps::MAGIC_MAP) && b.first().copied() == Some(maps::MELEE_MAP)
    };
    s.register_combat_input_maps(mode::UNDEF, mode::MELEE);
    let none_at_all = maps::live_mode_maps(&s).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.input-map.the-combat-map-itself-is-always-up-and-never-moves",
        move |_| once_each_time && unchanged && magic_first && swapped && none_at_all,
    );
}

pub(super) fn a_running_client_swaps_the_keys_the_frame_after_the_mode_changes() {
    use dereth_client_model::combat::CombatMode;
    use dereth_input::combat::mode;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));

    // A client in the world is at peace and carries none of the three.
    let at_rest = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    // The shard says the mode changed -- the form that sends nothing back.
    maps::the_shard_sets_the_mode(&mut c, CombatMode::Magic);
    let same_frame = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    c.tick(1);
    let next_frame = maps::live_in(&mut c) == (mode::MAGIC, vec![maps::MAGIC_MAP]);

    // And back. A swap that only ever added would pass everything above.
    maps::the_shard_sets_the_mode(&mut c, CombatMode::NonCombat);
    c.tick(1);
    let back = maps::live_in(&mut c) == (mode::NONCOMBAT, Vec::new());

    c.assert_behaviour(
        "combat.input-map.a-running-client-swaps-the-keys-the-frame-after-the-mode-changes",
        move |_| at_rest && same_frame && next_frame && back,
    );
    c.shutdown();
}
