//! The Inventory sample: Mag-Tools' inventory tools. A log of each character's items kept for the
//! world and searched from another character, the line of what an appraised item is, partial
//! stacks merged, items put in the packs their rules name, a set of gear put on again, trading by
//! a vendor's own profile, and the best suit built from every character's gear; each run from
//! `dereth/plugins/inventory` in a scripted world.

use std::cell::RefCell;
use std::rc::Rc;

use dereth_plugin_host::{MemoryStorage, Package, PluginFiles, PluginStorage};
use dereth_rules::weenie::item_type;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    read_package, PluginRun, ScriptedObject, ScriptedVendor, SCRIPTED_PLAYER, START_EW, START_NS,
};

use super::tank::{held, inside};
use super::tank_items::{rule, utl};
use super::{samples, SharedFiles};

dereth_testkit::scenarios! {
    scenario_the_inventory_logs_each_character_and_finds_items_across_them => the_inventory_logs_each_character_and_finds_items_across_them ["plugins.samples.the-inventory-logs-each-character-and-finds-items-across-them"],
    scenario_the_inventory_stacks_packs_puts_gear_on_again_and_trades_by_a_vendors_profile => the_inventory_stacks_packs_puts_gear_on_again_and_trades_by_a_vendors_profile ["plugins.samples.the-inventory-stacks-packs-puts-gear-on-again-and-trades-by-a-vendors-profile"],
    scenario_the_suit_builder_finds_the_best_suit_across_characters_with_its_locks => the_suit_builder_finds_the_best_suit_across_characters_with_its_locks ["plugins.samples.the-suit-builder-finds-the-best-suit-across-characters-with-its-locks"],
}

pub(crate) const INVENTORY: &str = "network.dereth.inventory";

/// Stores kept between runs: a run given a clone opens what another saved.
#[derive(Debug, Default, Clone)]
pub(crate) struct SharedStorage(Rc<RefCell<MemoryStorage>>);

impl PluginStorage for SharedStorage {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        self.0.borrow_mut().load(plugin, scope)
    }
    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        self.0.borrow_mut().save(plugin, scope, text);
    }
}

/// The lines the plugin printed to the chat, its prefix taken off.
pub(crate) fn said(run: &PluginRun) -> Vec<String> {
    run.printed()
        .iter()
        .filter_map(|l| l.strip_prefix("[Inventory] ").map(str::to_owned))
        .collect()
}

pub(crate) fn no_errors(run: &PluginRun) {
    let console = run.console(INVENTORY);
    assert!(
        !console.iter().any(|l| l.starts_with("error")),
        "{console:#?}"
    );
    assert_eq!(run.status(INVENTORY), "running", "{console:#?}");
}

/// An Inventory run of `character` holding `objects`, its stores in `storage`.
pub(crate) fn inventory_run(
    run: PluginRun,
    storage: &SharedStorage,
    character: &str,
    objects: Vec<ScriptedObject>,
) -> PluginRun {
    let mut run = run.with_storage(Box::new(storage.clone()));
    run.load(read_package(&samples().join("inventory")));
    run.scripted_world(character, objects);
    run
}

/// One character's items logged, its log kept for the world, and searched from another
/// character's client: by a word of the name (both characters' rings), by a spell's words (only
/// the ring with Epic Strength) and by its set's name (the breastplate, worn); a logged item's
/// line names its spell, its rating and its value; and an item the player has selected says what
/// it is when its appraisal comes.
fn the_inventory_logs_each_character_and_finds_items_across_them() {
    const GOLD: u32 = 0x8000_0E01;
    const PLATE: u32 = 0x8000_0E02;
    const MOTES: u32 = 0x8000_0E03;
    const SILVER: u32 = 0x8000_0E11;
    let storage = SharedStorage::default();
    let spells = [(4001, "Epic Strength", 4)];
    let mut alice = inventory_run(
        PluginRun::new().with_spells(&spells, false),
        &storage,
        "Alice",
        vec![
            ScriptedObject {
                value: 5000,
                ints: vec![(370, 2)],
                spells: vec![4001],
                ..held(GOLD, "Gold Ring", item_type::JEWELRY)
            },
            ScriptedObject {
                locations: Some(0x0000_0200),
                worn: true,
                ints: vec![(28, 500), (265, 16)],
                ..held(PLATE, "Breastplate", item_type::ARMOR)
            },
            ScriptedObject {
                stack: Some(5),
                max_stack: Some(100),
                ..held(MOTES, "Pyreal Mote", item_type::MISC)
            },
        ],
    );
    alice.advance(10.0);
    assert!(
        alice
            .console(INVENTORY)
            .iter()
            .any(|l| l == "logged Alice: 3 items"),
        "{:#?}",
        alice.console(INVENTORY)
    );
    no_errors(&alice);
    alice.unload(INVENTORY);

    let mut bob = inventory_run(
        PluginRun::new().with_spells(&spells, false),
        &storage,
        "Bob",
        vec![ScriptedObject {
            value: 3000,
            ints: vec![(372, 1)],
            ..held(SILVER, "Silver Ring", item_type::JEWELRY)
        }],
    );
    // Selected before its appraisal comes: its line is said when it does.
    bob.select(SILVER);
    bob.advance(6.0);
    let lines = said(&bob);
    assert!(
        lines
            .iter()
            .any(|l| l == "Silver Ring: Crit 1; Value 3,000"),
        "{lines:#?}"
    );
    for (find, expect) in [
        (
            "/inv find ring",
            &[
                "2 found",
                "Alice: Gold Ring (main pack)",
                "Bob: Silver Ring (main pack)",
            ][..],
        ),
        (
            "/inv find epic strength",
            &["1 found", "Alice: Gold Ring (main pack)"][..],
        ),
        (
            "/inv find defender",
            &["1 found", "Alice: Breastplate (worn)"][..],
        ),
        (
            "/inv find mote",
            &["1 found", "Alice: Pyreal Mote x5 (main pack)"][..],
        ),
    ] {
        let from = said(&bob).len();
        bob.type_line(find);
        bob.advance(0.5);
        let got = said(&bob)[from..].to_vec();
        assert_eq!(got, expect, "{find}");
    }
    let from = said(&bob).len();
    bob.type_line("/inv info gold ring");
    bob.advance(0.5);
    assert_eq!(
        said(&bob)[from..],
        ["Alice: Gold Ring: Epic Strength; Dmg 2; Value 5,000"]
    );
    let from = said(&bob).len();
    bob.type_line("/inv info breastplate");
    bob.advance(0.5);
    assert_eq!(
        said(&bob)[from..],
        ["Alice: Breastplate: AL 500; Set: Defender's"]
    );
    no_errors(&bob);
    note_asserted("plugins.samples.the-inventory-logs-each-character-and-finds-items-across-them");
}

/// The everyday tools: `/inv stack` merges the two partial stacks of tapers into one; with a pack
/// rule for tapers, `/inv pack` puts them in the side pack; a set of gear kept while worn is put
/// on again after a piece came off; and with AutoTrade on, opening a vendor sells what the
/// vendor's own profile sells (the dagger) and buys what its Keep # rule keeps up to its count.
fn the_inventory_stacks_packs_puts_gear_on_again_and_trades_by_a_vendors_profile() {
    const TAPER_A: u32 = 0x8000_0E21;
    const TAPER_B: u32 = 0x8000_0E22;
    const SIDE: u32 = 0x8000_0E23;
    const PLATE: u32 = 0x8000_0E24;
    const HELM: u32 = 0x8000_0E25;
    const DAGGER: u32 = 0x8000_0E26;
    const VENDOR: u32 = 0x8000_0E30;
    const STOCK_TAPER: u32 = 0x8000_0E31;
    let files = SharedFiles::default();
    files
        .clone()
        .write(
            INVENTORY,
            "vendors/Tester the Shopkeeper.utl",
            utl(
                &[
                    rule("Junk", 3, None, &[(7, &["1"])]),
                    rule("Tapers", 10, Some(50), &[(1, &["Prismatic Taper", "1"])]),
                ],
                "",
            )
            .as_bytes(),
        )
        .expect("in memory");
    // A plugin of the test's own, taking a piece of gear off when typed to.
    let hands = Package {
        origin: "hands".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.hands\"\nname = \"Hands\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\n[permissions]\n\"act.interactive\" = \"Takes gear off when typed to.\"\n"
                    .to_vec(),
            ),
            (
                "main.lua".to_owned(),
                format!(
                    "commands.add(\"takeoff\", \"\", function() task.spawn(function() print(pawait(items.unwield({HELM}))) end) end)\n"
                )
                .into_bytes(),
            ),
        ]
        .into(),
    };
    let storage = SharedStorage::default();
    let mut run = PluginRun::new().with_files(Box::new(files.clone()));
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(hands);
    let taper = |id: u32, n: u16| ScriptedObject {
        stack: Some(n),
        max_stack: Some(100),
        value: 5,
        ..held(id, "Prismatic Taper", item_type::SPELL_COMPONENTS)
    };
    let mut run = inventory_run(
        run,
        &storage,
        "Alice",
        vec![
            taper(TAPER_A, 10),
            taper(TAPER_B, 20),
            held(SIDE, "Backpack", item_type::CONTAINER),
            ScriptedObject {
                locations: Some(0x0000_0200),
                worn: true,
                ..held(PLATE, "Breastplate", item_type::ARMOR)
            },
            ScriptedObject {
                locations: Some(0x0000_0001),
                worn: true,
                ..held(HELM, "Helm", item_type::ARMOR)
            },
            ScriptedObject {
                value: 50,
                ..held(DAGGER, "Rusty Dagger", item_type::MELEE_WEAPON)
            },
            ScriptedObject {
                id: VENDOR,
                name: "Tester the Shopkeeper".into(),
                obj_type: item_type::CREATURE,
                place: Some((START_NS, START_EW + 0.5 / 240.0)),
                vendor: Some(ScriptedVendor {
                    buys: item_type::MELEE_WEAPON,
                }),
                ..ScriptedObject::default()
            },
            ScriptedObject {
                value: 5,
                max_stack: Some(1000),
                ..inside(
                    STOCK_TAPER,
                    "Prismatic Taper",
                    item_type::SPELL_COMPONENTS,
                    VENDOR,
                )
            },
        ],
    );
    run.advance(1.0);
    let tapers = |run: &PluginRun| {
        run.held_items()
            .into_iter()
            .filter(|(_, name, _)| name == "Prismatic Taper")
            .map(|(id, _, n)| (id, n))
            .collect::<Vec<_>>()
    };
    run.type_line("/inv stack");
    run.advance(3.0);
    assert_eq!(
        tapers(&run),
        [(TAPER_B, 30)],
        "{:#?}",
        run.console(INVENTORY)
    );
    run.type_line("/inv set PackRules Taper=2");
    run.type_line("/inv pack");
    run.advance(3.0);
    assert_eq!(
        run.container_of(TAPER_B),
        Some(SIDE),
        "{:#?}",
        run.console(INVENTORY)
    );
    // Gear: kept while worn, a piece taken off, put on again.
    run.type_line("/inv gear save fight");
    run.type_line("/takeoff");
    run.advance(2.0);
    assert_eq!(run.container_of(HELM), Some(SCRIPTED_PLAYER));
    run.type_line("/inv gear wear fight");
    run.advance(3.0);
    let lines = said(&run);
    assert!(
        lines.iter().any(|l| l == "set fight: 2 items"),
        "{lines:#?}"
    );
    assert!(
        lines.iter().any(|l| l == "set fight: 1 put on"),
        "{lines:#?}"
    );
    assert!(
        run.sent()
            .iter()
            .any(|l| l == &format!("wield {HELM:#x} 0x1")),
        "{:?}",
        run.sent()
    );
    // Trading by the vendor's own profile, as its wares show.
    run.set_coins(1000);
    run.type_line("/inv set AutoTrade on");
    run.player_uses(VENDOR);
    run.advance(6.0);
    assert!(
        run.sent()
            .contains(&format!("vendor_sell {VENDOR:#x} {DAGGER:#x}")),
        "the dagger sold, nothing else: {:?}",
        run.sent()
    );
    assert!(!run.held_items().iter().any(|(id, _, _)| *id == DAGGER));
    let held_tapers: u32 = tapers(&run).iter().map(|(_, n)| n).sum();
    assert_eq!(held_tapers, 50, "{:#?}", run.console(INVENTORY));
    let lines = said(&run);
    assert!(
        lines.iter().any(|l| l.starts_with(
            "traded with Tester the Shopkeeper: sold 1 items, bought 20 Prismatic Taper"
        )),
        "{lines:#?}"
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-inventory-stacks-packs-puts-gear-on-again-and-trades-by-a-vendors-profile",
    );
}

/// The suit builder, as SuitBuilder builds one: from two characters' logged gear, the best suit
/// for the second by the spells and the set asked for. Three pieces of the set (the helm and
/// breastplate of the first character, the gauntlets of the second) with the rings' Legendary
/// Strength and Minor Invulnerability beat the hauberk's Major Invulnerability and its armor
/// (it covers the breastplate's places and the upper arms, so not both); the bracelet attuned to
/// the first character, and the leggings that take a higher level than the second has, are left
/// out. With the hauberk locked, it is in the suit; the pieces the second character holds are put
/// on, and the rest said to be held by the first.
fn the_suit_builder_finds_the_best_suit_across_characters_with_its_locks() {
    const HELM: u32 = 0x8000_0F01;
    const PLATE: u32 = 0x8000_0F02;
    const HAUBERK: u32 = 0x8000_0F03;
    const GOLD: u32 = 0x8000_0F04;
    const SILVER: u32 = 0x8000_0F05;
    const BRACELET: u32 = 0x8000_0F06;
    const GAUNTLETS: u32 = 0x8000_0F11;
    const LEGGINGS: u32 = 0x8000_0F12;
    let spells = [
        (4101, "Epic Strength", 4),
        (4102, "Major Invulnerability", 4),
        (4103, "Legendary Strength", 4),
        (4104, "Minor Invulnerability", 4),
        (4105, "Epic Invulnerability", 4),
    ];
    let gear = |id: u32,
                name: &str,
                obj_type: u32,
                locations: u32,
                ints: &[(u32, i32)],
                spells: &[u32]| {
        ScriptedObject {
            locations: Some(locations),
            ints: ints.to_vec(),
            spells: spells.to_vec(),
            ..held(id, name, obj_type)
        }
    };
    let storage = SharedStorage::default();
    let mut alice = inventory_run(
        PluginRun::new().with_spells(&spells, false),
        &storage,
        "Alice",
        vec![
            gear(
                HELM,
                "Defender Helm",
                item_type::ARMOR,
                0x1,
                &[(28, 500), (265, 16)],
                &[4101],
            ),
            gear(
                PLATE,
                "Defender Breastplate",
                item_type::ARMOR,
                0x600,
                &[(28, 500), (265, 16), (370, 1)],
                &[],
            ),
            gear(
                HAUBERK,
                "Plain Hauberk",
                item_type::ARMOR,
                0xE00,
                &[(28, 900)],
                &[4102],
            ),
            gear(
                GOLD,
                "Gold Ring",
                item_type::JEWELRY,
                0xC_0000,
                &[(374, 2)],
                &[4103],
            ),
            gear(
                SILVER,
                "Silver Ring",
                item_type::JEWELRY,
                0xC_0000,
                &[],
                &[4104],
            ),
            gear(
                BRACELET,
                "Bound Bracelet",
                item_type::JEWELRY,
                0x3_0000,
                &[(114, 1)],
                &[4105],
            ),
        ],
    );
    alice.advance(12.0);
    no_errors(&alice);
    alice.unload(INVENTORY);

    let mut bob = inventory_run(
        PluginRun::new().with_spells(&spells, false),
        &storage,
        "Bob",
        vec![
            gear(
                GAUNTLETS,
                "Defender Gauntlets",
                item_type::ARMOR,
                0x20,
                &[(28, 400), (265, 16)],
                &[],
            ),
            gear(
                LEGGINGS,
                "Heavy Leggings",
                item_type::ARMOR,
                0x6000,
                &[(28, 600), (158, 7), (160, 200)],
                &[],
            ),
        ],
    );
    bob.set_player_int(25, 100);
    bob.advance(10.0);
    let build = |run: &mut PluginRun| {
        let from = said(run).len();
        run.type_line("/inv suit");
        run.advance(3.0);
        said(run)[from..].to_vec()
    };
    bob.type_line("/inv set SuitSpells Strength, Invulnerability");
    bob.type_line("/inv set SuitSet Defender");
    let lines = build(&mut bob);
    assert!(
        lines[0].starts_with("6 pieces from 2 characters; "),
        "{lines:#?}"
    );
    assert_eq!(
        lines[1..],
        [
            "best: 3 of Defender's; Minor Invulnerability, Legendary Strength; ratings 3; armor 1900",
            "  Head: Defender Helm (Alice)",
            "  Chest, Abdomen: Defender Breastplate (Alice)",
            "  Hands: Defender Gauntlets (Bob)",
            "  Left finger: Gold Ring (Alice)",
            "  Right finger: Silver Ring (Alice)",
        ],
        "{lines:#?}"
    );
    // The hauberk locked into every suit.
    let from = said(&bob).len();
    bob.type_line("/inv suit lock hauberk");
    bob.advance(0.5);
    assert_eq!(
        said(&bob)[from..],
        ["locked Plain Hauberk (Alice) into every suit"]
    );
    let lines = build(&mut bob);
    assert_eq!(
        lines[1..3],
        [
            "best: 2 of Defender's; Major Invulnerability, Legendary Strength; ratings 2; armor 3600",
            "  Chest, Abdomen, Upper arms: Plain Hauberk (Alice)",
        ],
        "{lines:#?}"
    );
    let from = said(&bob).len();
    bob.type_line("/inv suit wear");
    bob.advance(3.0);
    let lines = said(&bob)[from..].to_vec();
    assert_eq!(
        lines,
        ["suit 1: 1 put on; held by others: Plain Hauberk (Alice), Defender Helm (Alice), Gold Ring (Alice), Silver Ring (Alice)"],
    );
    assert!(
        bob.sent()
            .iter()
            .any(|l| l.starts_with(&format!("wield {GAUNTLETS:#x}"))),
        "{:?}",
        bob.sent()
    );
    no_errors(&bob);
    note_asserted(
        "plugins.samples.the-suit-builder-finds-the-best-suit-across-characters-with-its-locks",
    );
}
