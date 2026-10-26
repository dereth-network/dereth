//! The Tank sample's looting, items and trading against VTank: salvage and the profile's combining
//! ranges, every loot rule's action, others' and rare corpses, stacking and cramming, buff
//! profiles and buff items, the recharge refinements, and selling and buying at a vendor on its
//! trip there and back, each run from `dereth/plugins/tank` in a scripted world.

use dereth_plugin_host::PluginFiles;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    PluginRun, ScriptedObject, ScriptedVendor, SCRIPTED_PLAYER, START_EW, START_NS,
};

use super::tank::{
    corpse, held, inside, kill, logged, met, monster, sent_count, tank_in, text, wand, ALWAYS,
    CHAT, TANK,
};
use super::SharedFiles;

dereth_testkit::scenarios! {
    scenario_the_tank_sells_its_loot_and_buys_its_buy_list_on_its_vendor_trip => the_tank_sells_its_loot_and_buys_its_buy_list_on_its_vendor_trip ["plugins.samples.the-tank-sells-its-loot-and-buys-its-buy-list-on-its-vendor-trip"],
    scenario_the_tank_goes_to_its_vendor_when_its_pack_is_full_or_a_meta_says_so => the_tank_goes_to_its_vendor_when_its_pack_is_full_or_a_meta_says_so ["plugins.samples.the-tank-goes-to-its-vendor-when-its-pack-is-full-or-a-meta-says-so"],
    scenario_the_tank_walks_to_its_vendor_from_where_it_was_set_however_far_it_stands_from_there => the_tank_walks_to_its_vendor_from_where_it_was_set_however_far_it_stands_from_there ["plugins.samples.the-tank-walks-to-its-vendor-from-where-it-was-set-however-far-it-stands-from-there"],
    scenario_the_tank_trades_by_a_vendors_own_profile_at_a_routes_vendor_waypoint => the_tank_trades_by_a_vendors_own_profile_at_a_routes_vendor_waypoint ["plugins.samples.the-tank-trades-by-a-vendors-own-profile-at-a-routes-vendor-waypoint"],
    scenario_the_tank_salvages_what_its_profile_salvages_and_combines_bags_by_its_ranges => the_tank_salvages_what_its_profile_salvages_and_combines_bags_by_its_ranges ["plugins.samples.the-tank-salvages-what-its-profile-salvages-and-combines-bags-by-its-ranges"],
    scenario_the_tank_takes_what_sell_read_and_user_rules_match => the_tank_takes_what_sell_read_and_user_rules_match ["plugins.samples.the-tank-takes-what-sell-read-and-user-rules-match"],
    scenario_the_tank_loots_fellows_and_others_corpses_and_only_rare_ones_when_told => the_tank_loots_fellows_and_others_corpses_and_only_rare_ones_when_told ["plugins.samples.the-tank-loots-fellows-and-others-corpses-and-only-rare-ones-when-told"],
    scenario_the_tank_stacks_partial_stacks_and_crams_its_main_pack_into_its_side_pack => the_tank_stacks_partial_stacks_and_crams_its_main_pack_into_its_side_pack ["plugins.samples.the-tank-stacks-partial-stacks-and-crams-its-main-pack-into-its-side-pack"],
    scenario_the_tank_casts_the_protections_and_banes_its_buff_profiles_name_and_uses_buff_items => the_tank_casts_the_protections_and_banes_its_buff_profiles_name_and_uses_buff_items ["plugins.samples.the-tank-casts-the-protections-and-banes-its-buff-profiles-name-and-uses-buff-items"],
    scenario_the_tank_uses_kits_in_peace_and_keeps_recharging_after_switching => the_tank_uses_kits_in_peace_and_keeps_recharging_after_switching ["plugins.samples.the-tank-uses-kits-in-peace-and-keeps-recharging-after-switching"],
}

/// One rule of a loot profile in VTank's file layout: its name, its action (1 keep, 2 salvage, 3
/// sell, 4 read, 5 to 9 the player's, 10 keep up to `count`), and its requirements, each a type
/// and its body's lines.
pub(super) fn rule(name: &str, action: i32, count: Option<i32>, reqs: &[(i32, &[&str])]) -> String {
    let mut out = format!("{name}\r\n\r\n0;{action}");
    for (ty, _) in reqs {
        out.push_str(&format!(";{ty}"));
    }
    out.push_str("\r\n");
    if let Some(n) = count {
        out.push_str(&format!("{n}\r\n"));
    }
    for (_, lines) in reqs {
        let body: String = lines.iter().map(|l| format!("{l}\r\n")).collect();
        out.push_str(&format!("{}\r\n{body}", body.len()));
    }
    out
}

/// A loot profile of `rules`, then `tail` (its extra blocks).
pub(super) fn utl(rules: &[String], tail: &str) -> String {
    format!("UTL\r\n1\r\n{}\r\n{}{tail}", rules.len(), rules.concat())
}

/// The vendor of these scenarios, 20 metres east of where the character starts, buying weapons
/// and gems.
const VENDOR_EAST: f64 = 20.0;
pub(super) const VENDOR: u32 = 0x8000_0300;
const VENDOR_NAME: &str = "Tester the Shopkeeper";
const TAPER: u32 = 0x8000_0301;
const BREAD: u32 = 0x8000_0302;

pub(super) fn vendor() -> Vec<ScriptedObject> {
    vec![
        ScriptedObject {
            id: VENDOR,
            name: VENDOR_NAME.into(),
            obj_type: item_type::CREATURE,
            place: Some((START_NS, START_EW + VENDOR_EAST / 240.0)),
            vendor: Some(ScriptedVendor {
                buys: item_type::MELEE_WEAPON | item_type::GEM,
            }),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            value: 5,
            max_stack: Some(1000),
            ..inside(
                TAPER,
                "Prismatic Taper",
                item_type::SPELL_COMPONENTS,
                VENDOR,
            )
        },
        ScriptedObject {
            value: 10,
            max_stack: Some(100),
            ..inside(BREAD, "Bread", item_type::FOOD, VENDOR)
        },
    ]
}

/// A data folder holding `files` (path and text).
pub(super) fn data(files: &[(&str, &str)]) -> SharedFiles {
    let shared = SharedFiles::default();
    for (path, text) in files {
        shared
            .clone()
            .write(super::tank::TANK, path, text.as_bytes())
            .expect("in memory");
    }
    shared
}

/// Run until the Tank's console has `line`, at most `secs` seconds.
fn until(run: &mut PluginRun, line: &str, secs: f64) {
    let mut t = 0.0;
    while t < secs && !logged(run, line) {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(logged(run, line), "{line:?}: {:#?}", run.console(TANK));
}

/// The macro's loop ran without an error.
fn no_errors(run: &PluginRun) {
    let console = run.console(TANK);
    assert!(
        !console.iter().any(|l| l.contains(" error: ")),
        "{console:#?}"
    );
}

/// How many of `name` the pack holds, stacks counted.
fn held_count(run: &PluginRun, name: &str) -> u32 {
    run.held_items()
        .iter()
        .filter(|(_, n, _)| n == name)
        .map(|(_, _, s)| *s)
        .sum()
}

/// The loot profile of the vendor scenarios: melee weapons (class 1) sold, gems (class 11) kept.
pub(super) fn selling_profile() -> String {
    utl(
        &[
            rule("Junk", 3, None, &[(7, &["1"])]),
            rule("Gems", 1, None, &[(7, &["11"])]),
        ],
        "",
    )
}

/// The pack of the vendor scenarios: two daggers to sell, the weapon the settings name (a melee
/// weapon too, never sold), an opal the profile keeps, and ten tapers.
const DAGGERS: [u32; 2] = [0x8000_0310, 0x8000_0311];
const SWORD: u32 = 0x8000_0312;
const OPAL: u32 = 0x8000_0313;
const MY_TAPERS: u32 = 0x8000_0314;

pub(super) fn pack() -> Vec<ScriptedObject> {
    vec![
        ScriptedObject {
            value: 50,
            ..held(DAGGERS[0], "Rusty Dagger", item_type::MELEE_WEAPON)
        },
        ScriptedObject {
            value: 50,
            ..held(DAGGERS[1], "Rusty Dagger", item_type::MELEE_WEAPON)
        },
        ScriptedObject {
            value: 500,
            ..held(SWORD, "Long Sword", item_type::MELEE_WEAPON)
        },
        ScriptedObject {
            value: 300,
            ..held(OPAL, "Opal", item_type::GEM)
        },
        ScriptedObject {
            value: 50,
            stack: Some(10),
            max_stack: Some(1000),
            ..held(MY_TAPERS, "Prismatic Taper", item_type::SPELL_COMPONENTS)
        },
    ]
}

/// The vendor trip on the player's `/tank vendor`: the Tank travels to where the player stood when
/// the vendor was set (the vendor 80 metres off, out of reach of a walk to it), sells what the loot
/// profile's Sell rules match (not the weapon the settings name, nor what a Keep rule keeps), buys
/// the buy list up to its counts (counting the ten tapers it holds), for the pyreals it has,
/// travels back where it left, and the run made for the trip stops.
fn the_tank_sells_its_loot_and_buys_its_buy_list_on_its_vendor_trip() {
    const NOTE: u32 = 0x8000_0303;
    let files = data(&[("profiles/sell.utl", &selling_profile())]);
    let mut objects = vendor();
    objects[0].place = Some((START_NS, START_EW + 80.0 / 240.0));
    objects.push(ScriptedObject {
        value: 50,
        max_stack: Some(250),
        ..inside(
            NOTE,
            "Trade Note (250,000)",
            item_type::PROMISSORY_NOTE,
            VENDOR,
        )
    });
    objects.extend(pack());
    let mut run = tank_in(PluginRun::new(), &files, objects);
    run.set_coins(1000);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set Weapon Long Sword",
        "/tank profile sell.utl",
        "/tank buy 50 Prismatic Taper",
        // A name with a comma in it: the list's entries are kept apart all the same.
        "/tank buy 2 Trade Note (250,000)",
        "/tank buy 3 Bread",
    ] {
        run.type_line(line);
    }
    // Set beside the vendor, then back where the character started.
    run.place_player(START_NS, START_EW + 77.0 / 240.0);
    run.select(VENDOR);
    run.type_line("/tank vendor set");
    run.place_player(START_NS, START_EW);
    run.type_line("/tank vendor");
    until(&mut run, "stopped: the vendor trip is done", 90.0);
    assert!(logged(
        &run,
        "vendor trip to Tester the Shopkeeper (by a command), 80 m away"
    ));
    assert!(logged(&run, "travelling to Tester the Shopkeeper"));
    assert!(logged(&run, "travelling back from Tester the Shopkeeper"));
    let sent = run.sent();
    assert!(
        sent.contains(&format!(
            "vendor_sell {VENDOR:#x} {:#x} {:#x}",
            DAGGERS[0], DAGGERS[1]
        )),
        "the daggers sold, not the sword or the opal: {sent:?}"
    );
    assert!(
        sent.contains(&format!(
            "vendor_buy {VENDOR:#x} {TAPER:#x}x40 {BREAD:#x}x3 {NOTE:#x}x2"
        )),
        "forty tapers to fifty, three loaves, two notes: {sent:?}"
    );
    assert_eq!(run.coins(), 1000 + 100 - 40 * 5 - 2 * 50 - 3 * 10);
    assert_eq!(held_count(&run, "Prismatic Taper"), 50);
    assert_eq!(run.container_of(SWORD), Some(SCRIPTED_PLAYER));
    assert_eq!(run.container_of(OPAL), Some(SCRIPTED_PLAYER));
    assert!(
        logged(
            &run,
            "vendor: at Tester the Shopkeeper: sold 2 (Rusty Dagger, Rusty Dagger); bought 40 Prismatic Taper, 3 Bread, 2 Trade Note (250,000); pyreals 1000 -> 770"
        ),
        "{:#?}",
        run.console(TANK)
    );
    assert!(logged(&run, "vendor trip: back from Tester the Shopkeeper"));
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-sells-its-loot-and-buys-its-buy-list-on-its-vendor-trip",
    );
}

/// The vendor set from fifty meters off (the player stood across the square from it): the trip
/// travels to where the player stood, walks the fifty meters to the vendor from there, and trades.
fn the_tank_walks_to_its_vendor_from_where_it_was_set_however_far_it_stands_from_there() {
    let files = data(&[("profiles/sell.utl", &selling_profile())]);
    let mut objects = vendor();
    objects[0].place = Some((START_NS, START_EW + 50.0 / 240.0));
    objects.extend(pack());
    let mut run = tank_in(PluginRun::new(), &files, objects);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set Weapon Long Sword",
        "/tank profile sell.utl",
    ] {
        run.type_line(line);
    }
    run.select(VENDOR);
    run.type_line("/tank vendor set");
    run.place_player(START_NS + 300.0 / 240.0, START_EW);
    run.type_line("/tank vendor");
    until(&mut run, "stopped: the vendor trip is done", 240.0);
    assert!(
        logged(
            &run,
            "vendor: at Tester the Shopkeeper: sold 2 (Rusty Dagger, Rusty Dagger)"
        ),
        "{:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-walks-to-its-vendor-from-where-it-was-set-however-far-it-stands-from-there");
}

/// OnFullPack vendor: a full pack sends the running Tank to its vendor and back, and it hunts on;
/// a pack still full after the vendor stops it. A meta's chat action typing `/tank vendor` sends it
/// too.
fn the_tank_goes_to_its_vendor_when_its_pack_is_full_or_a_meta_says_so() {
    let go = met(&[(
        ALWAYS,
        super::tank::int(0),
        CHAT,
        text("/tank vendor"),
        "Default",
    )]);
    let files = data(&[
        ("profiles/sell.utl", &selling_profile()),
        ("metas/go.met", &go),
    ]);
    let mut objects = vendor();
    objects.extend(pack());
    let mut run = tank_in(PluginRun::new(), &files, objects);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set EnableNav false",
        "/tank set Weapon Long Sword",
        "/tank profile sell.utl",
        // The pack holds 102; five items leave 97 free, a full pack under 98.
        "/tank set MinFreeSlots 98",
        "/tank set OnFullPack vendor",
    ] {
        run.type_line(line);
    }
    run.select(VENDOR);
    run.type_line("/tank vendor set");
    run.type_line("/tank start");
    until(
        &mut run,
        "vendor trip: back from Tester the Shopkeeper",
        60.0,
    );
    assert!(logged(
        &run,
        "vendor trip to Tester the Shopkeeper (the pack is full)"
    ));
    assert!(logged(&run, "sold 2 (Rusty Dagger, Rusty Dagger)"));
    run.advance(3.0);
    assert!(
        !logged(&run, "stopped:"),
        "it hunts on: {:#?}",
        run.console(TANK)
    );
    // Full again, with nothing it may sell: back from the vendor still full, it stops.
    run.type_line("/tank set VendorSell false");
    run.spawn(vec![
        ScriptedObject {
            value: 50,
            ..held(0x8000_0320, "Rusty Dagger", item_type::MELEE_WEAPON)
        },
        ScriptedObject {
            value: 50,
            ..held(0x8000_0321, "Rusty Dagger", item_type::MELEE_WEAPON)
        },
    ]);
    until(
        &mut run,
        "stopped: the pack is still full after the vendor",
        60.0,
    );
    // A meta's `/tank vendor` sends it.
    run.type_line("/tank set OnFullPack keep");
    run.type_line("/tank meta go.met");
    run.type_line("/tank start");
    until(
        &mut run,
        "vendor trip to Tester the Shopkeeper (by a command)",
        20.0,
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-goes-to-its-vendor-when-its-pack-is-full-or-a-meta-says-so",
    );
}

/// A route's vendor waypoint: the Tank opens the vendor and trades by the vendor's own profile
/// (`vendors/<its name>.utl`, as Mag-Tools reads one): its Sell rules sold, its Keep # rule
/// bought up to its count, its Keep rule bought once when the pack has none.
fn the_tank_trades_by_a_vendors_own_profile_at_a_routes_vendor_waypoint() {
    let shop = utl(
        &[
            rule("S: Daggers", 3, None, &[(7, &["1"])]),
            rule("B: Tapers", 10, Some(25), &[(1, &["Prismatic Taper", "1"])]),
            rule("B: Bread", 1, None, &[(1, &["^Bread$", "1"])]),
        ],
        "",
    );
    let files = data(&[("vendors/Tester the Shopkeeper.utl", &shop)]);
    let mut objects = vendor();
    objects.extend(pack());
    let mut run = tank_in(PluginRun::new(), &files, objects);
    run.set_coins(100);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set Weapon Long Sword",
        "/tank route new once",
    ] {
        run.type_line(line);
    }
    run.select(VENDOR);
    run.type_line("/tank route vendor");
    run.type_line("/tank start");
    until(&mut run, "route: finished", 60.0);
    let sent = run.sent();
    assert!(
        sent.contains(&format!(
            "vendor_sell {VENDOR:#x} {:#x} {:#x}",
            DAGGERS[0], DAGGERS[1]
        )),
        "{sent:?}"
    );
    assert!(
        sent.contains(&format!(
            "vendor_buy {VENDOR:#x} {TAPER:#x}x15 {BREAD:#x}x1"
        )),
        "{sent:?}"
    );
    assert_eq!(run.coins(), 100 + 100 - 15 * 5 - 10);
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-trades-by-a-vendors-own-profile-at-a-routes-vendor-waypoint",
    );
}

/// The steel salvage bags of the salvage scenario: its units, its workmanship.
fn bags(run: &PluginRun) -> Vec<(u32, f64)> {
    let mut out: Vec<(u32, f64)> = run
        .salvage_bags()
        .into_iter()
        .map(|(_, units, work)| (units, (work * 10.0).round() / 10.0))
        .collect();
    out.sort_by_key(|a| a.0);
    out
}

/// VTank's auto-salvager and CombineSalvage: what the loot profile's Salvage rule takes is
/// salvaged once the corpse is done (an inscribed one kept), and the steel bags are combined as
/// the profile's ranges say: bags of one range together while they fill no more than a bag, a bag
/// of another range left alone.
fn the_tank_salvages_what_its_profile_salvages_and_combines_bags_by_its_ranges() {
    const MONSTER: u32 = 0x8000_0400;
    const CORPSE: u32 = 0x8000_0401;
    const INSCRIBED: u32 = 0x8000_0412;
    const STEEL: u32 = 64;
    let profile = utl(
        &[rule("Weapons", 2, None, &[(7, &["1"])])],
        "SalvageCombine\r\n19\r\n1\r\n1-6,7-10\r\n0\r\n",
    );
    let files = data(&[("profiles/salvage.utl", &profile)]);
    let bag = |id: u32, units: u16, work: f32| ScriptedObject {
        material: Some(STEEL),
        structure: Some(units),
        workmanship: Some(work),
        ..held(id, "Salvage", item_type::TINKERING_MATERIAL)
    };
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![
            monster(MONSTER, "Drudge Skulker", START_NS, START_EW + 0.0001),
            held(0x8000_0420, "Ust", item_type::TINKERING_TOOL),
            bag(0x8000_0421, 30, 4.0),
            bag(0x8000_0422, 40, 5.0),
            bag(0x8000_0423, 50, 2.0),
            bag(0x8000_0424, 10, 8.0),
        ],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank profile salvage.utl",
        "/tank start",
    ] {
        run.type_line(line);
    }
    let weapon = |id: u32, name: &str, work: f32| ScriptedObject {
        material: Some(STEEL),
        workmanship: Some(work),
        value: 100,
        ..inside(id, name, item_type::MELEE_WEAPON, CORPSE)
    };
    kill(
        &mut run,
        MONSTER,
        corpse(CORPSE, "Drudge Skulker", START_NS, START_EW + 0.0001),
        vec![
            weapon(0x8000_0410, "Rusty Dagger", 5.0),
            weapon(0x8000_0411, "Rusty Dagger", 6.0),
            ScriptedObject {
                strings: vec![(7, "Mine".into())],
                ..weapon(INSCRIBED, "Inscribed Dagger", 7.0)
            },
        ],
    );
    until(&mut run, "salvaged Rusty Dagger, Rusty Dagger", 30.0);
    run.advance(20.0);
    assert_eq!(
        run.container_of(INSCRIBED),
        Some(SCRIPTED_PLAYER),
        "the inscribed dagger taken, kept: {:#?}",
        run.console(TANK)
    );
    // The salvage made a bag of 11 units at 5.5; range 1-6 then holds 11, 30, 40 and 50, combined
    // smallest first while they fill no more than a bag (11 and 30, then 40 and 41); range 7-10
    // holds the bag at 8 alone.
    let b = bags(&run);
    assert_eq!(
        b.iter().map(|x| x.0).collect::<Vec<_>>(),
        [10, 50, 81],
        "{b:?}: {:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-salvages-what-its-profile-salvages-and-combines-bags-by-its-ranges",
    );
}

/// Every rule's action takes its item, as the plugin reads VTank's loot actions: a Sell rule's to
/// sell later, a Read rule's read once the corpse is done, a User rule's for the player; an item
/// no rule matches is left.
fn the_tank_takes_what_sell_read_and_user_rules_match() {
    const MONSTER: u32 = 0x8000_0500;
    const CORPSE: u32 = 0x8000_0501;
    const DAGGER: u32 = 0x8000_0510;
    const BOOK: u32 = 0x8000_0511;
    const RING: u32 = 0x8000_0512;
    const GEM: u32 = 0x8000_0513;
    let profile = utl(
        &[
            rule("Sell", 3, None, &[(7, &["1"])]),
            rule("Read", 4, None, &[(7, &["33"])]),
            rule("Mine", 5, None, &[(7, &["4"])]),
        ],
        "",
    );
    let files = data(&[("profiles/rules.utl", &profile)]);
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![monster(
            MONSTER,
            "Drudge Skulker",
            START_NS,
            START_EW + 0.0001,
        )],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank profile rules.utl",
        "/tank start",
    ] {
        run.type_line(line);
    }
    kill(
        &mut run,
        MONSTER,
        corpse(CORPSE, "Drudge Skulker", START_NS, START_EW + 0.0001),
        vec![
            inside(DAGGER, "Rusty Dagger", item_type::MELEE_WEAPON, CORPSE),
            inside(BOOK, "Old Book", item_type::WRITABLE, CORPSE),
            inside(RING, "Silver Ring", item_type::JEWELRY, CORPSE),
            inside(GEM, "Opal", item_type::GEM, CORPSE),
        ],
    );
    until(
        &mut run,
        "looted Corpse of Drudge Skulker: kept 3 of 4",
        20.0,
    );
    run.advance(3.0);
    for id in [DAGGER, BOOK, RING] {
        assert_eq!(
            run.container_of(id),
            Some(SCRIPTED_PLAYER),
            "{id:#x}: {:#?}",
            run.console(TANK)
        );
    }
    assert_eq!(run.container_of(GEM), Some(CORPSE), "no rule, left");
    assert_eq!(sent_count(&run, &format!("use {BOOK:#x}")), 1);
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-takes-what-sell-read-and-user-rules-match");
}

/// Others' corpses (VTank's LootFellowCorpses, LootAllCorpses, LootOnlyRareCorpses), each told
/// apart by its appraisal ("Killed by …"): a fellow's kill looted with LootFellowCorpses, a
/// stranger's left; with LootAllCorpses and LootOnlyRareCorpses only the one that generated a
/// rare; without the rare-only, the other stranger's too.
fn the_tank_loots_fellows_and_others_corpses_and_only_rare_ones_when_told() {
    use dereth_protocol::social::{Fellow, Fellowship, FellowshipFullUpdate, FellowshipLocks};
    let profile = utl(&[rule("Gems", 1, None, &[(7, &["11"])])], "");
    let files = data(&[("profiles/gems.utl", &profile)]);
    let body = |id: u32, east: f64, killed: &str| ScriptedObject {
        strings: vec![(16, killed.into())],
        ..corpse(id, "Drudge Skulker", START_NS, START_EW + east / 240.0)
    };
    const FELLOWS: u32 = 0x8000_0600;
    const STRANGERS: u32 = 0x8000_0601;
    const RARE: u32 = 0x8000_0602;
    let gem = |id: u32, c: u32| inside(id, "Opal", item_type::GEM, c);
    let mut run = tank_in(
        PluginRun::new(),
        &files,
        vec![
            body(FELLOWS, 3.0, "Killed by Friend."),
            body(STRANGERS, 5.0, "Killed by Stranger."),
            body(
                RARE,
                7.0,
                "Killed by Stranger. This corpse generated a rare item!",
            ),
            gem(0x8000_0610, FELLOWS),
            gem(0x8000_0611, STRANGERS),
            gem(0x8000_0612, RARE),
        ],
    );
    let fellow = |name: &str| Fellow {
        name: name.into(),
        level: 10,
        ..Fellow::default()
    };
    run.server_says(&[dereth_testkit::plugins::ui_event(&FellowshipFullUpdate(
        Fellowship {
            members: dereth_protocol::archive::PackedHash {
                table_size: 16,
                entries: vec![
                    (SCRIPTED_PLAYER, fellow("Tester")),
                    (0x5000_0099, fellow("Friend")),
                ],
            },
            name: "Hunters".into(),
            leader: dereth_primitives::ObjectId(SCRIPTED_PLAYER),
            share_xp: 1,
            even_xp_split: 0,
            open_fellow: 1,
            locked: 0,
            fellows_departed: dereth_protocol::archive::PackedHash::default(),
            locks: FellowshipLocks::default(),
        },
    ))]);
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set LootFellowCorpses true",
        "/tank profile gems.utl",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(15.0);
    let taken = |run: &PluginRun| {
        [0x8000_0610, 0x8000_0611, 0x8000_0612]
            .map(|g| run.container_of(g) == Some(SCRIPTED_PLAYER))
    };
    assert_eq!(
        taken(&run),
        [true, false, false],
        "the fellow's kill only: {:#?}",
        run.console(TANK)
    );
    run.type_line("/tank set LootAllCorpses true");
    run.type_line("/tank set LootOnlyRareCorpses true");
    run.advance(15.0);
    assert_eq!(
        taken(&run),
        [true, false, true],
        "the rare one only: {:#?}",
        run.console(TANK)
    );
    run.type_line("/tank set LootOnlyRareCorpses false");
    run.advance(15.0);
    assert_eq!(taken(&run), [true, true, true], "{:#?}", run.console(TANK));
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-loots-fellows-and-others-corpses-and-only-rare-ones-when-told",
    );
}

/// AutoStack and AutoCram: stacks are merged toward the bottom-most of their kind, leaving the one
/// partial stack top-most; then the main pack's items are moved into the side pack.
fn the_tank_stacks_partial_stacks_and_crams_its_main_pack_into_its_side_pack() {
    const SIDE: u32 = 0x8000_0700;
    const BREAD_ID: u32 = 0x8000_0710;
    let taper = |id: u32, n: u16| ScriptedObject {
        stack: Some(n),
        max_stack: Some(100),
        value: u32::from(n),
        ..held(id, "Prismatic Taper", item_type::SPELL_COMPONENTS)
    };
    let mut run = tank_in(
        PluginRun::new(),
        &SharedFiles::default(),
        vec![
            held(SIDE, "Pack", item_type::CONTAINER),
            taper(0x8000_0701, 30),
            taper(0x8000_0702, 100),
            taper(0x8000_0703, 20),
            held(BREAD_ID, "Bread", item_type::FOOD),
        ],
    );
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set AutoStack true",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(10.0);
    let stacks = |run: &PluginRun| {
        run.held_items()
            .iter()
            .filter(|(_, n, _)| n == "Prismatic Taper")
            .map(|(_, _, s)| *s)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        stacks(&run),
        [50, 100],
        "the partial stack top-most: {:#?}",
        run.console(TANK)
    );
    run.type_line("/tank set AutoCram true");
    run.advance(15.0);
    assert_eq!(
        run.container_of(BREAD_ID),
        Some(SIDE),
        "{:#?}",
        run.console(TANK)
    );
    assert_eq!(run.container_of(0x8000_0703), Some(SIDE));
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-stacks-partial-stacks-and-crams-its-main-pack-into-its-side-pack",
    );
}

/// VTank's buff profiles: with Custom protections "SF" only the blade and fire protections are
/// cast; with banes "B" only the bludgeon bane on the armor; a buff added to the set (a skill not
/// trained) is cast; and a buff item the Consumables name is used for a buff the character does
/// not cast.
fn the_tank_casts_the_protections_and_banes_its_buff_profiles_name_and_uses_buff_items() {
    const ARMOR: u32 = 0x8000_0800;
    const BEER: u32 = 0x8000_0801;
    let mut run = tank_in(
        PluginRun::new()
            .with_spells(
                &[
                    (60, "Fire Protection Self I", 2),
                    (61, "Acid Protection Self I", 2),
                    (62, "Leadership Mastery Self I", 4),
                ],
                true,
            )
            .with_spells(&[(63, "Coordination Self I", 4)], false)
            .with_item_spells(&[(64, "Bludgeon Bane I"), (65, "Flame Bane I")]),
        &SharedFiles::default(),
        vec![
            ScriptedObject {
                locations: Some(0x0000_0200),
                bitfield: bitfield::ATTACKABLE,
                worn: true,
                ..held(ARMOR, "Breastplate", item_type::ARMOR)
            },
            wand(),
            ScriptedObject {
                spells: vec![63],
                ..held(BEER, "Bottle of Beer", item_type::FOOD)
            },
        ],
    );
    run.casts_land(true);
    for line in [
        "/tank set EnableNav false",
        "/tank set BuffProfile_Prots Custom",
        "/tank set BuffProfile-Prots SF",
        "/tank set BuffProfile_Banes B",
        "/tank set AddedBuffs Leadership Mastery Self",
        "/tank set Consumables Bottle of Beer",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(12.0);
    let me = format!("{SCRIPTED_PLAYER:#x}");
    let cast = |run: &PluginRun, s: String| sent_count(run, &s);
    assert_eq!(
        cast(&run, format!("cast 2215 {me}")),
        1,
        "blade: {:#?}",
        run.console(TANK)
    );
    assert_eq!(cast(&run, format!("cast 60 {me}")), 1, "fire");
    assert_eq!(cast(&run, format!("cast 61 {me}")), 0, "not acid");
    assert_eq!(cast(&run, format!("cast 62 {me}")), 1, "the added buff");
    assert_eq!(
        cast(&run, format!("cast 64 {ARMOR:#x}")),
        1,
        "the bludgeon bane: {:#?} {:?}",
        run.console(TANK),
        run.sent()
    );
    assert_eq!(
        cast(&run, format!("cast 65 {ARMOR:#x}")),
        0,
        "not the flame bane"
    );
    assert!(
        sent_count(&run, &format!("use {BEER:#x}")) >= 1,
        "the beer for Coordination: {:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-casts-the-protections-and-banes-its-buff-profiles-name-and-uses-buff-items",
    );
}

/// The recharge refinements: with GoToPeaceModeToUseKits the Tank drops from its fighting stance
/// to peace mode before a healing kit; and after switching to magic stance for a restoring spell,
/// the recharge boost has it recharge again above its usual mark.
fn the_tank_uses_kits_in_peace_and_keeps_recharging_after_switching() {
    const MONSTER: u32 = 0x8000_0900;
    const KIT: u32 = 0x8000_0901;
    let kit = ScriptedObject {
        bitfield: bitfield::HEALER,
        ..held(KIT, "Healing Kit", item_type::MISC)
    };
    let mut run = tank_in(
        PluginRun::new(),
        &SharedFiles::default(),
        vec![
            monster(MONSTER, "Drudge Skulker", START_NS, START_EW + 0.0001),
            kit,
            wand(),
        ],
    );
    run.set_vitals((100, 100), (100, 100), (100, 100));
    for line in [
        "/tank set EnableNav false",
        "/tank set EnableBuffing false",
        "/tank set GoToPeaceModeToUseKits true",
        "/tank set RechargeBoostTimeSeconds 60",
        "/tank set RechargeBoostAmount 50",
        "/tank start",
    ] {
        run.type_line(line);
    }
    run.advance(3.0);
    // Health low, fighting: to peace mode, then the kit (used on the character itself, it goes
    // out as a plain use).
    let before = run.sent().len();
    run.set_vitals((20, 100), (100, 100), (100, 100));
    run.advance(3.0);
    let after: Vec<String> = run.sent()[before..].to_vec();
    let kit_at = after
        .iter()
        .position(|s| *s == format!("use {KIT:#x}"))
        .unwrap_or_else(|| panic!("the kit: {after:?}: {:#?}", run.console(TANK)));
    assert!(
        after[..kit_at].iter().any(|s| s == "ChangeCombatMode"),
        "peace mode before the kit: {after:?}: {:#?}",
        run.console(TANK)
    );
    // Nothing about, stamina low: Revitalize, switching to magic stance, which starts the boost;
    // at 70%, above the top-off mark of 60%, it is cast again while the boost lasts.
    run.remove(MONSTER);
    run.set_vitals((100, 100), (10, 100), (100, 100));
    run.advance(3.0);
    let revitalize = |run: &PluginRun| sent_count(run, "cast 1177 0x50000001");
    assert!(revitalize(&run) >= 1, "{:#?}", run.console(TANK));
    run.set_vitals((100, 100), (70, 100), (100, 100));
    let boosted = revitalize(&run);
    run.advance(3.0);
    assert!(
        revitalize(&run) > boosted,
        "recharged again under the boosted mark: {:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-uses-kits-in-peace-and-keeps-recharging-after-switching",
    );
}
