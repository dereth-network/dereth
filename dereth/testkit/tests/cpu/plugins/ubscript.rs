//! Scripts written for UtilityBelt's scripting, run by the host's library of UtilityBelt's script
//! API: the spec's `ubscript` conformance cases, each a script folder (or a package naming
//! `ubscript = true`) in a scripted world.

use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::conformance::assert_case;

dereth_testkit::scenarios! {
    scenario_a_utilitybelt_script_runs_from_its_folder_and_reads_the_game => a_utilitybelt_script_runs_from_its_folder_and_reads_the_game ["plugins.ubscript.a-utilitybelt-script-runs-from-its-folder-and-reads-the-game"],
    scenario_a_scripts_actions_go_through_utilitybelts_queue => a_scripts_actions_go_through_utilitybelts_queue ["plugins.ubscript.a-scripts-actions-go-through-utilitybelts-queue"],
    scenario_a_script_hears_utilitybelts_events => a_script_hears_utilitybelts_events ["plugins.ubscript.a-script-hears-utilitybelts-events"],
    scenario_a_scripts_imgui_hud_is_a_declared_window => a_scripts_imgui_hud_is_a_declared_window ["plugins.ubscript.a-scripts-imgui-hud-is-a-declared-window"],
    scenario_a_script_reaches_utilitybelts_other_modules => a_script_reaches_utilitybelts_other_modules ["plugins.ubscript.a-script-reaches-utilitybelts-other-modules"],
    scenario_what_cannot_be_done_here_is_refused_plainly => what_cannot_be_done_here_is_refused_plainly ["plugins.ubscript.what-cannot-be-done-here-is-refused-plainly"],
    scenario_a_named_script_active_before_the_world_logs_in_and_hears_its_end => a_named_script_active_before_the_world_logs_in_and_hears_its_end ["plugins.ubscript.a-named-script-active-before-the-world-logs-in-and-hears-its-end"],
    scenario_a_scripts_character_reads_its_fellowship_vitals_and_own_properties => a_scripts_character_reads_its_fellowship_vitals_and_own_properties ["plugins.ubscript.a-scripts-character-reads-its-fellowship-vitals-and-own-properties"],
}

fn a_utilitybelt_script_runs_from_its_folder_and_reads_the_game() {
    assert_case("ubscript", "basics");
    note_asserted("plugins.ubscript.a-utilitybelt-script-runs-from-its-folder-and-reads-the-game");
}

fn a_scripts_actions_go_through_utilitybelts_queue() {
    assert_case("ubscript", "actions");
    note_asserted("plugins.ubscript.a-scripts-actions-go-through-utilitybelts-queue");
}

fn a_script_hears_utilitybelts_events() {
    assert_case("ubscript", "events");
    note_asserted("plugins.ubscript.a-script-hears-utilitybelts-events");
}

fn a_scripts_imgui_hud_is_a_declared_window() {
    assert_case("ubscript", "huds");
    note_asserted("plugins.ubscript.a-scripts-imgui-hud-is-a-declared-window");
}

fn a_script_reaches_utilitybelts_other_modules() {
    assert_case("ubscript", "modules");
    note_asserted("plugins.ubscript.a-script-reaches-utilitybelts-other-modules");
}

fn what_cannot_be_done_here_is_refused_plainly() {
    assert_case("ubscript", "refusals");
    note_asserted("plugins.ubscript.what-cannot-be-done-here-is-refused-plainly");
}

fn a_named_script_active_before_the_world_logs_in_and_hears_its_end() {
    assert_case("ubscript", "lifecycle");
    note_asserted(
        "plugins.ubscript.a-named-script-active-before-the-world-logs-in-and-hears-its-end",
    );
}

fn a_scripts_character_reads_its_fellowship_vitals_and_own_properties() {
    use dereth_plugin_host::Package;
    use dereth_protocol::social::{Fellow, Fellowship, FellowshipFullUpdate, FellowshipLocks};
    use dereth_testkit::plugins::PluginRun;
    let index = r#"
-- The character as UtilityBelt's API reads it: its fellowship and fellows, its vitals, its own
-- properties by typed id, and the inventory counted by name.
game.Character.Fellowship.OnJoin.Add(function()
	print("joined")
end)
sleep(1000)
local f = game.Character.Fellowship
print("fellowship", f.Exists, f.Name, f.LeaderId == game.CharacterId, f.ShareExp, f.IsOpen, f.Members.Count)
local friend = f.Members[0x50000099]
print("fellow", friend.Name, friend.Level, friend.CurrentHealth, friend.MaxHealth, friend.MaxMana)
local me = game.Character.Weenie
local health = me.Vitals[VitalId.Health]
print("vitals", health.Current, health.Max, me.Vitals[VitalId.Mana].Current, game.Character.Vitae)
print("augs", me.Value(IntId.AugmentationIncreasedCarryingCapacity), me.IntValues[IntId.AugmentationBonusXp], me.HasValue(IntId.AugmentationBonusSalvage))
print("inventory", game.Character.GetInventoryCount("Pyreal"), #game.Character.Inventory, game.Character.GetFirstInventory(ObjectClass.Money).Name)
print("allegiance", game.Character.Allegiance.Exists, game.Character.Trade.IsOpen, game.World.Vendor.IsOpen)
"#;
    let package = Package {
        origin: "Character".into(),
        files: [("index.lua".to_owned(), index.as_bytes().to_vec())].into(),
    };
    let mut run = PluginRun::new();
    run.load(package);
    let mut coins = dereth_testkit::plugins::ScriptedObject {
        id: 0x8000_0071,
        name: "Pyreal".into(),
        obj_type: dereth_rules::weenie::item_type::MONEY,
        container: Some(dereth_testkit::plugins::SCRIPTED_PLAYER),
        ..dereth_testkit::plugins::ScriptedObject::default()
    };
    coins.stack = Some(250);
    run.scripted_world("Tester", vec![coins]);
    run.set_vitals((80, 100), (120, 120), (40, 90));
    run.advance(0.5);
    let me = dereth_testkit::plugins::SCRIPTED_PLAYER;
    let fellow = |name: &str, level: u32, health: u32| Fellow {
        name: name.into(),
        level,
        max_health: 100,
        max_stamina: 120,
        max_mana: 80,
        current_health: health,
        current_stamina: 120,
        current_mana: 80,
        ..Fellow::default()
    };
    run.server_says(&[dereth_testkit::plugins::ui_event(&FellowshipFullUpdate(
        Fellowship {
            members: dereth_protocol::archive::PackedHash {
                table_size: 16,
                entries: vec![
                    (me, fellow("Tester", 10, 100)),
                    (0x5000_0099, fellow("Friend", 12, 40)),
                ],
            },
            name: "Hunters".into(),
            leader: dereth_primitives::ObjectId(me),
            share_xp: 1,
            even_xp_split: 0,
            open_fellow: 1,
            locked: 0,
            fellows_departed: dereth_protocol::archive::PackedHash::default(),
            locks: FellowshipLocks::default(),
        },
    ))]);
    {
        use dereth_client_model::{StatKey, StatType, StatValue};
        let w = run.world_mut();
        if let Some(q) = w.player_qualities_mut() {
            q.set(StatKey::new(StatType::Int, 230), StatValue::Int(1));
            q.set(StatKey::new(StatType::Int, 234), StatValue::Int(3));
        }
    }
    run.advance(1.5);
    assert_eq!(
        run.console("ubscript.character"),
        [
            "joined",
            "fellowship true Hunters true true true 2",
            "fellow Friend 12 40 100 80",
            "vitals 80 100 40 1",
            "augs 1 3 false",
            "inventory 250 1 Pyreal",
            "allegiance false false false",
        ]
    );
    note_asserted(
        "plugins.ubscript.a-scripts-character-reads-its-fellowship-vitals-and-own-properties",
    );
}
