//! The Tank sample's crafting against VTank: missile ammunition made from bundles of heads and
//! shafts for the launcher in hand, the best the pack makes, a failed combine left alone, carried
//! ammunition readied in a fight, nothing made without the skill, the game's craft dialog answered
//! by its chance, each tier made from its least buffed skill otherwise, ammunition named as retail
//! names it, the world's recipe answers taken over the names, and a healing kit made in its two
//! combines. Each runs from `dereth/plugins/tank` in a
//! scripted world, the test answering each combine as the server would.

use dereth_rules::weenie::item_type;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{PluginRun, ScriptedObject, SCRIPTED_PLAYER, START_EW, START_NS};

use super::tank::{held, logged, monster, tank_in, TANK};
use super::SharedFiles;

dereth_testkit::scenarios! {
    scenario_the_tank_crafts_the_best_ammo_its_heads_and_shafts_make_and_readies_it => the_tank_crafts_the_best_ammo_its_heads_and_shafts_make_and_readies_it ["plugins.samples.the-tank-crafts-the-best-ammo-its-heads-and-shafts-make-and-readies-it"],
    scenario_the_tank_leaves_a_failed_combine_and_tries_the_next_when_it_has_no_ammo => the_tank_leaves_a_failed_combine_and_tries_the_next_when_it_has_no_ammo ["plugins.samples.the-tank-leaves-a-failed-combine-and-tries-the-next-when-it-has-no-ammo"],
    scenario_the_tank_readies_the_ammo_it_carries_in_a_fight_rather_than_crafting_better => the_tank_readies_the_ammo_it_carries_in_a_fight_rather_than_crafting_better ["plugins.samples.the-tank-readies-the-ammo-it-carries-in-a-fight-rather-than-crafting-better"],
    scenario_the_tank_crafts_nothing_without_the_skill_and_says_why => the_tank_crafts_nothing_without_the_skill_and_says_why ["plugins.samples.the-tank-crafts-nothing-without-the-skill-and-says-why"],
    scenario_the_tank_answers_the_craft_dialog_yes_at_its_chance_and_no_below => the_tank_answers_the_craft_dialog_yes_at_its_chance_and_no_below ["plugins.samples.the-tank-answers-the-craft-dialog-yes-at-its-chance-and-no-below"],
    scenario_the_tank_makes_each_tier_from_its_least_buffed_skill_when_no_chance_is_told => the_tank_makes_each_tier_from_its_least_buffed_skill_when_no_chance_is_told ["plugins.samples.the-tank-makes-each-tier-from-its-least-buffed-skill-when-no-chance-is-told"],
    scenario_the_tank_names_ammunition_as_retail_does_and_keeps_wrapped_heads_off_plain_shafts => the_tank_names_ammunition_as_retail_does_and_keeps_wrapped_heads_off_plain_shafts ["plugins.samples.the-tank-names-ammunition-as-retail-does-and-keeps-wrapped-heads-off-plain-shafts"],
    scenario_the_tank_takes_the_worlds_recipe_answers_over_the_names_of_its_heads => the_tank_takes_the_worlds_recipe_answers_over_the_names_of_its_heads ["plugins.samples.the-tank-takes-the-worlds-recipe-answers-over-the-names-of-its-heads"],
    scenario_the_tank_makes_a_healing_kit_in_two_combines_when_its_consumables_want_one => the_tank_makes_a_healing_kit_in_two_combines_when_its_consumables_want_one ["plugins.samples.the-tank-makes-a-healing-kit-in-two-combines-when-its-consumables-want-one"],
}

// Use flags: used from the pack, on something in the pack (the low half where it must be, the high
// half what it is used on).
const CONTAINED: u32 = 0x08;
const IN_THE_PACK: u32 = CONTAINED | (CONTAINED << 16);

const BOW: u32 = 0x8000_0A00;
const MISSILE_SLOT: u32 = 0x0040_0000;
/// The ammunition slot: bit 23 of the equip mask.
const AMMO_SLOT: u32 = 1 << 23;

/// A longbow in hand.
fn bow() -> ScriptedObject {
    launcher("Longbow")
}

/// The launcher `name` in hand.
fn launcher(name: &str) -> ScriptedObject {
    ScriptedObject {
        worn: true,
        locations: Some(MISSILE_SLOT),
        ..held(BOW, name, item_type::MISSILE_WEAPON)
    }
}

/// A bundle of heads (or herbs) in the pack, used on something else there.
fn used_on(id: u32, name: &str) -> ScriptedObject {
    ScriptedObject {
        useability: Some(IN_THE_PACK),
        target_type: Some(item_type::MISC),
        stack: Some(1),
        ..held(id, name, item_type::MISC)
    }
}

/// A bundle of shafts (or another ingredient a combine is used on) in the pack.
fn ingredient(id: u32, name: &str) -> ScriptedObject {
    ScriptedObject {
        stack: Some(1),
        ..held(id, name, item_type::MISC)
    }
}

/// Loose ammunition in the pack, `count` of it.
fn ammo(id: u32, name: &str, count: u16) -> ScriptedObject {
    ScriptedObject {
        locations: Some(AMMO_SLOT),
        stack: Some(count),
        max_stack: Some(1000),
        ..held(id, name, item_type::MISSILE_WEAPON)
    }
}

/// The player option the game's craft success dialog is, by its name in the option table.
fn craft_dialog_ordinal() -> usize {
    use dereth_client_model::player::options::Options;
    (0..64)
        .find(|o| Options::name(*o) == "UseCraftSuccessDialog")
        .expect("the craft success dialog option")
}

/// Whether the character's craft dialog option is on.
fn craft_dialog(run: &mut PluginRun) -> bool {
    run.world_mut()
        .player_system
        .options
        .get(craft_dialog_ordinal())
}

/// The character's craft dialog option set, as the player sets it.
fn set_craft_dialog(run: &mut PluginRun, on: bool) {
    run.world_mut()
        .player_system
        .options
        .set(craft_dialog_ordinal(), on);
    run.frame();
}

/// A Tank in a scripted world of `objects` with Fletching and Healing trained (buffed Fletching
/// 400, above every tier's default skill but Greater and Deadly Prismatic; Healing 300, above the
/// healing kit's), the craft dialog off, walking nowhere and buffing nothing, `lines` typed before
/// it starts.
fn crafter(objects: Vec<ScriptedObject>, lines: &[&str]) -> PluginRun {
    crafter_at(objects, lines, 400)
}

/// [`crafter`] with buffed Fletching `fletching`.
fn crafter_at(objects: Vec<ScriptedObject>, lines: &[&str], fletching: i32) -> PluginRun {
    crafter_with(objects, lines, fletching, 300)
}

/// [`crafter`] with buffed Fletching `fletching` and Healing `healing`.
fn crafter_with(
    objects: Vec<ScriptedObject>,
    lines: &[&str],
    fletching: i32,
    healing: i32,
) -> PluginRun {
    let mut run = tank_in(PluginRun::new(), &SharedFiles::default(), objects);
    run.set_skill("Fletching", fletching);
    run.set_skill("Healing", healing);
    set_craft_dialog(&mut run, false);
    run.set_vitals((100, 100), (100, 100), (100, 100));
    for line in ["/tank set EnableNav false", "/tank set EnableBuffing false"]
        .iter()
        .chain(lines)
    {
        run.type_line(line);
    }
    run
}

/// The combine of `item` on `target` the client sent.
fn combine(item: u32, target: u32) -> String {
    format!("use_with_target {item:#x} {target:#x}")
}

/// Run until the client has sent `what`, at most `secs` seconds.
fn until_sent(run: &mut PluginRun, what: &str, secs: f64) {
    let mut t = 0.0;
    while t < secs && !run.sent().iter().any(|s| s == what) {
        run.advance(0.25);
        t += 0.25;
    }
    assert!(
        run.sent().iter().any(|s| s == what),
        "{what}: sent {:?}: {:#?}",
        run.sent(),
        run.console(TANK)
    );
}

/// Run until the client has sent a wield of `item`, at most `secs` seconds.
fn until_wielded(run: &mut PluginRun, item: u32, secs: f64) {
    let wield = format!("wield {item:#x} ");
    let mut t = 0.0;
    while t < secs && !run.sent().iter().any(|s| s.starts_with(&wield)) {
        run.advance(0.25);
        t += 0.25;
    }
    assert!(
        run.sent().iter().any(|s| s.starts_with(&wield)),
        "a wield of {item:#x}: sent {:?}: {:#?}",
        run.sent(),
        run.console(TANK)
    );
}

fn combines_sent(run: &PluginRun) -> Vec<String> {
    run.sent()
        .iter()
        .filter(|s| s.starts_with("use_with_target "))
        .cloned()
        .collect()
}

/// The macro's loop ran without an error.
fn no_errors(run: &PluginRun) {
    let console = run.console(TANK);
    assert!(
        !console.iter().any(|l| l.contains(" error: ")),
        "{console:#?}"
    );
}

/// Behaviour: plugins.samples.the-tank-crafts-the-best-ammo-its-heads-and-shafts-make-and-readies-it
fn the_tank_crafts_the_best_ammo_its_heads_and_shafts_make_and_readies_it() {
    const PLAIN: u32 = 0x8000_0A01;
    const DEADLY_FIRE: u32 = 0x8000_0A02;
    const RAIDER: u32 = 0x8000_0A03;
    const ARROWSHAFTS: u32 = 0x8000_0A04;
    const QUARRELSHAFTS: u32 = 0x8000_0A05;
    const MADE: u32 = 0x8000_0A06;
    let mut run = crafter(
        vec![
            bow(),
            used_on(PLAIN, "Bundle of Arrowheads"),
            used_on(DEADLY_FIRE, "Bundle of Deadly Fire Arrowheads"),
            used_on(RAIDER, "Bundle of Raider Lightning Arrowheads"),
            ingredient(QUARRELSHAFTS, "Bundle of Quarrelshafts"),
            ingredient(ARROWSHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    // No ammunition for the bow: the best heads the bow's shafts take, the special ones left out
    // with UseSpecialAmmo at 0, and never the crossbow's shafts.
    until_sent(&mut run, &combine(DEADLY_FIRE, ARROWSHAFTS), 10.0);
    assert_eq!(
        combines_sent(&run),
        vec![combine(DEADLY_FIRE, ARROWSHAFTS)],
        "{:#?}",
        run.console(TANK)
    );
    // The server makes the arrows: they are readied.
    run.remove(DEADLY_FIRE);
    run.spawn(vec![ammo(MADE, "Deadly Fire Arrow", 100)]);
    until_wielded(&mut run, MADE, 10.0);
    assert!(
        logged(
            &run,
            "made ammunition from Bundle of Deadly Fire Arrowheads"
        ),
        "{:#?}",
        run.console(TANK)
    );
    run.advance(3.0);
    assert_eq!(
        combines_sent(&run).len(),
        1,
        "one combine: {:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-crafts-the-best-ammo-its-heads-and-shafts-make-and-readies-it",
    );
}

/// Behaviour: plugins.samples.the-tank-leaves-a-failed-combine-and-tries-the-next-when-it-has-no-ammo
fn the_tank_leaves_a_failed_combine_and_tries_the_next_when_it_has_no_ammo() {
    const PLAIN: u32 = 0x8000_0B01;
    const DEADLY: u32 = 0x8000_0B02;
    const SHAFTS: u32 = 0x8000_0B03;
    const MORE_SHAFTS: u32 = 0x8000_0B04;
    const MADE: u32 = 0x8000_0B05;
    let mut run = crafter(
        vec![
            bow(),
            used_on(PLAIN, "Bundle of Arrowheads"),
            used_on(DEADLY, "Bundle of Deadly Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
            ingredient(MORE_SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    until_sent(&mut run, &combine(DEADLY, SHAFTS), 10.0);
    // It fails: both bundles are gone and nothing is made. With no ammunition at all, the next
    // heads go on the other shafts.
    run.remove(DEADLY);
    run.remove(SHAFTS);
    until_sent(&mut run, &combine(PLAIN, MORE_SHAFTS), 20.0);
    assert!(
        logged(
            &run,
            "the combine of Bundle of Deadly Arrowheads and Bundle of Arrowshafts failed; it is not tried again this run"
        ),
        "{:#?}",
        run.console(TANK)
    );
    run.remove(PLAIN);
    run.spawn(vec![ammo(MADE, "Arrow", 100)]);
    until_wielded(&mut run, MADE, 10.0);
    assert_eq!(combines_sent(&run).len(), 2, "{:#?}", run.console(TANK));
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-leaves-a-failed-combine-and-tries-the-next-when-it-has-no-ammo",
    );
}

/// Behaviour: plugins.samples.the-tank-readies-the-ammo-it-carries-in-a-fight-rather-than-crafting-better
fn the_tank_readies_the_ammo_it_carries_in_a_fight_rather_than_crafting_better() {
    const MONSTER: u32 = 0x8000_0C00;
    const ARROWS: u32 = 0x8000_0C01;
    const DEADLY: u32 = 0x8000_0C02;
    const SHAFTS: u32 = 0x8000_0C03;
    let mut run = crafter(
        vec![
            monster(MONSTER, "Drudge Skulker", START_NS, START_EW + 0.0001),
            bow(),
            ammo(ARROWS, "Arrow", 50),
            used_on(DEADLY, "Bundle of Deadly Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    // A monster is at hand: the arrows carried go on at once, and the better ones wait.
    until_wielded(&mut run, ARROWS, 10.0);
    run.advance(5.0);
    assert!(combines_sent(&run).is_empty(), "{:#?}", run.console(TANK));
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-readies-the-ammo-it-carries-in-a-fight-rather-than-crafting-better");
}

/// Behaviour: plugins.samples.the-tank-crafts-nothing-without-the-skill-and-says-why
fn the_tank_crafts_nothing_without_the_skill_and_says_why() {
    const HEADS: u32 = 0x8000_0D01;
    const SHAFTS: u32 = 0x8000_0D02;
    let mut run = crafter(
        vec![
            bow(),
            used_on(HEADS, "Bundle of Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true"],
    );
    run.untrain_skill("Fletching");
    run.type_line("/tank start");
    run.advance(5.0);
    assert!(combines_sent(&run).is_empty(), "{:#?}", run.console(TANK));
    assert!(
        logged(&run, "crafting needs Fletching trained"),
        "{:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-crafts-nothing-without-the-skill-and-says-why");
}

/// Behaviour: plugins.samples.the-tank-answers-the-craft-dialog-yes-at-its-chance-and-no-below
fn the_tank_answers_the_craft_dialog_yes_at_its_chance_and_no_below() {
    const DEADLY: u32 = 0x8000_0D11;
    const PLAIN: u32 = 0x8000_0D12;
    const SHAFTS: u32 = 0x8000_0D13;
    const MADE: u32 = 0x8000_0D14;
    let chance = |n: u32| format!("You determine that you have a {n} percent chance to succeed.");
    let mut run = crafter(
        vec![
            bow(),
            used_on(DEADLY, "Bundle of Deadly Arrowheads"),
            used_on(PLAIN, "Bundle of Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true"],
    );
    // The player's craft dialog is on: the server says each combine's chance.
    set_craft_dialog(&mut run, true);
    run.type_line("/tank start");
    // The best heads first; at a 50 percent chance, under CraftMinChance's 90, the answer is No and
    // the combine is left.
    until_sent(&mut run, &combine(DEADLY, SHAFTS), 10.0);
    run.ask_craft(0x91, &chance(50));
    until_sent(&mut run, "confirm 5 0x91 no", 5.0);
    assert!(
        logged(&run, "declined a combine at a 50% chance"),
        "{:#?}",
        run.console(TANK)
    );
    // With no ammunition, the next heads: at 95 percent, Yes.
    until_sent(&mut run, &combine(PLAIN, SHAFTS), 20.0);
    run.ask_craft(0x92, &chance(95));
    until_sent(&mut run, "confirm 5 0x92 yes", 5.0);
    run.remove(PLAIN);
    run.spawn(vec![ammo(MADE, "Arrow", 100)]);
    until_wielded(&mut run, MADE, 10.0);
    assert_eq!(
        combines_sent(&run),
        vec![combine(DEADLY, SHAFTS), combine(PLAIN, SHAFTS)],
        "{:#?}",
        run.console(TANK)
    );
    // The Tank answered; it changed no setting of the player's.
    assert!(
        craft_dialog(&mut run),
        "the dialog is as the player left it"
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-answers-the-craft-dialog-yes-at-its-chance-and-no-below",
    );
}

/// Behaviour: plugins.samples.the-tank-takes-the-worlds-recipe-answers-over-the-names-of-its-heads
fn the_tank_takes_the_worlds_recipe_answers_over_the_names_of_its_heads() {
    use dereth_protocol::admin::{RecipeAnswerStatus, RecipeMatch};
    const DEADLY: u32 = 0x8000_0D21;
    const PLAIN: u32 = 0x8000_0D22;
    const SHAFTS: u32 = 0x8000_0D23;
    const MADE: u32 = 0x8000_0D24;
    let mut run = crafter(
        vec![
            bow(),
            used_on(DEADLY, "Bundle of Deadly Arrowheads"),
            used_on(PLAIN, "Bundle of Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true"],
    );
    run.offer_recipes();
    run.type_line("/tank start");
    // The Tank asks the world what its heads and shafts make.
    let mut t = 0.0;
    while t < 10.0 && run.recipe_queries().is_empty() {
        run.advance(0.25);
        t += 0.25;
    }
    let queries = run.recipe_queries();
    assert_eq!(queries.len(), 1, "{:#?}", run.console(TANK));
    let mut asked = queries[0].1.clone();
    asked.sort_unstable();
    assert_eq!(asked, vec![DEADLY, PLAIN, SHAFTS]);
    // The world says the deadly heads would fail half the time: the plain ones are made, which
    // the names alone would have ranked below.
    let m = |source: u32, name: &str, chance: f32| RecipeMatch {
        source,
        target: SHAFTS,
        recipe: 1,
        skill: 37,
        difficulty: 100,
        chance,
        makes_wcid: 300,
        makes_name: name.into(),
        makes_count: 100,
        fails: RecipeMatch::FAIL_SOURCE | RecipeMatch::FAIL_TARGET,
    };
    run.answer_recipes(
        queries[0].0,
        RecipeAnswerStatus::Answered,
        vec![m(DEADLY, "Deadly Arrow", 0.5), m(PLAIN, "Arrow", 0.99)],
    );
    until_sent(&mut run, &combine(PLAIN, SHAFTS), 10.0);
    run.remove(PLAIN);
    run.spawn(vec![ammo(MADE, "Arrow", 100)]);
    until_wielded(&mut run, MADE, 10.0);
    assert_eq!(
        combines_sent(&run),
        vec![combine(PLAIN, SHAFTS)],
        "{:#?}",
        run.console(TANK)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-takes-the-worlds-recipe-answers-over-the-names-of-its-heads",
    );
}

/// Behaviour: plugins.samples.the-tank-makes-a-healing-kit-in-two-combines-when-its-consumables-want-one
fn the_tank_makes_a_healing_kit_in_two_combines_when_its_consumables_want_one() {
    const HYSSOP: u32 = 0x8000_0E01;
    const MANDRAKE: u32 = 0x8000_0E02;
    const BANDAGES: u32 = 0x8000_0E03;
    const COMBINED: u32 = 0x8000_0E04;
    const KIT: u32 = 0x8000_0E05;
    let mut run = crafter(
        vec![
            used_on(HYSSOP, "Treated Hyssop"),
            ingredient(MANDRAKE, "Treated Mandrake"),
            ingredient(BANDAGES, "Soft Bandages"),
        ],
        &[
            "/tank set EnableCombat false",
            "/tank set AutoCraftItems true",
            "/tank set Consumables Plentiful Healing Kit",
            "/tank start",
        ],
    );
    // No kit carried and the Consumables list one: the herbs together first.
    until_sent(&mut run, &combine(HYSSOP, MANDRAKE), 20.0);
    run.remove(HYSSOP);
    run.remove(MANDRAKE);
    run.spawn(vec![used_on(COMBINED, "Combined Hyssop and Mandrake")]);
    // Then the herbs on the bandages.
    until_sent(&mut run, &combine(COMBINED, BANDAGES), 20.0);
    run.remove(COMBINED);
    run.remove(BANDAGES);
    run.spawn(vec![ScriptedObject {
        container: Some(SCRIPTED_PLAYER),
        ..held(KIT, "Plentiful Healing Kit", item_type::MISC)
    }]);
    run.advance(10.0);
    assert!(
        logged(&run, "made Plentiful Healing Kit"),
        "{:#?}",
        run.console(TANK)
    );
    assert_eq!(combines_sent(&run).len(), 2, "{:#?}", run.console(TANK));
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-makes-a-healing-kit-in-two-combines-when-its-consumables-want-one");
}

/// Behaviour: plugins.samples.the-tank-makes-each-tier-from-its-least-buffed-skill-when-no-chance-is-told
fn the_tank_makes_each_tier_from_its_least_buffed_skill_when_no_chance_is_told() {
    const HEADS: u32 = 0x8000_0D41;
    const SHAFTS: u32 = 0x8000_0D42;
    // Each tier below, at and above its least buffed Fletching, the craft dialog off: from the
    // retail game's difficulties (plain, Prismatic, Greater Prismatic, wrapped Blunt) and the
    // public ones (Deadly Fire bundles, wrapped Greater), each plus 74.
    for (heads, shafts, skill) in [
        ("Bundle of Arrowheads", "Bundle of Arrowshafts", 79),
        (
            "Bundle of Prismatic Arrowheads",
            "Bundle of Arrowshafts",
            324,
        ),
        (
            "Wrapped Bundle of Greater Prismatic Arrowheads",
            "Wrapped Bundle of Arrowshafts",
            449,
        ),
        (
            "Wrapped Bundle of Blunt Arrowheads",
            "Wrapped Bundle of Arrowshafts",
            114,
        ),
        (
            "Bundle of Deadly Fire Arrowheads",
            "Bundle of Arrowshafts",
            299,
        ),
        (
            "Wrapped Bundle of Greater Arrowheads",
            "Wrapped Bundle of Arrowshafts",
            224,
        ),
    ] {
        for (fletching, made) in [(skill - 1, false), (skill, true), (skill + 40, true)] {
            let mut run = crafter_at(
                vec![bow(), used_on(HEADS, heads), ingredient(SHAFTS, shafts)],
                &["/tank set CraftAmmo true", "/tank start"],
                fletching,
            );
            run.advance(5.0);
            assert_eq!(
                !combines_sent(&run).is_empty(),
                made,
                "{heads} at buffed Fletching {fletching}: {:#?}",
                run.console(TANK)
            );
            assert!(!craft_dialog(&mut run), "the Tank changes no setting");
            no_errors(&run);
        }
    }
    // The player's own skill for a tier over the Tank's: plain heads from 200, not 79.
    let mut run = crafter_at(
        vec![
            bow(),
            used_on(HEADS, "Bundle of Arrowheads"),
            ingredient(SHAFTS, "Bundle of Arrowshafts"),
        ],
        &[
            "/tank set CraftAmmo true",
            "/tank set CraftSkills Plain = 200",
            "/tank start",
        ],
        150,
    );
    run.advance(5.0);
    assert!(combines_sent(&run).is_empty(), "{:#?}", run.console(TANK));
    // The healing kit's herbs from buffed Healing 174 (difficulty 100), the kit from 274 (200).
    const HYSSOP: u32 = 0x8000_0D43;
    const MANDRAKE: u32 = 0x8000_0D44;
    const BANDAGES: u32 = 0x8000_0D45;
    const COMBINED: u32 = 0x8000_0D46;
    for (first, used, on, healing, made) in [
        (HYSSOP, "Treated Hyssop", "Treated Mandrake", 173, false),
        (HYSSOP, "Treated Hyssop", "Treated Mandrake", 174, true),
        (
            COMBINED,
            "Combined Hyssop and Mandrake",
            "Soft Bandages",
            273,
            false,
        ),
        (
            COMBINED,
            "Combined Hyssop and Mandrake",
            "Soft Bandages",
            274,
            true,
        ),
    ] {
        let target = if first == HYSSOP { MANDRAKE } else { BANDAGES };
        let mut run = crafter_with(
            vec![used_on(first, used), ingredient(target, on)],
            &[
                "/tank set EnableCombat false",
                "/tank set AutoCraftItems true",
                "/tank set Consumables Plentiful Healing Kit",
                "/tank start",
            ],
            400,
            healing,
        );
        run.advance(10.0);
        assert_eq!(
            combines_sent(&run) == vec![combine(first, target)],
            made,
            "{used} at buffed Healing {healing}: {:#?}",
            run.console(TANK)
        );
        no_errors(&run);
    }
    // A tier no data gives a skill for is left, and the log says how to set one.
    let mut run = crafter(
        vec![
            bow(),
            used_on(HEADS, "Wrapped Bundle of Chorizite Arrowheads"),
            ingredient(SHAFTS, "Wrapped Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    run.advance(5.0);
    assert!(combines_sent(&run).is_empty(), "{:#?}", run.console(TANK));
    assert!(
        logged(&run, "no Fletching skill is set for Chorizite (wrapped)"),
        "{:#?}",
        run.console(TANK)
    );
    note_asserted("plugins.samples.the-tank-makes-each-tier-from-its-least-buffed-skill-when-no-chance-is-told");
}

/// Behaviour: plugins.samples.the-tank-names-ammunition-as-retail-does-and-keeps-wrapped-heads-off-plain-shafts
fn the_tank_names_ammunition_as_retail_does_and_keeps_wrapped_heads_off_plain_shafts() {
    const HEADS: u32 = 0x8000_0D51;
    const SHAFTS: u32 = 0x8000_0D52;
    const PLAIN_SHAFTS: u32 = 0x8000_0D53;
    // Broad heads make Broadhead ammunition, and some heads on quarrelshafts make a Bolt: the Ammo
    // setting naming them finds their heads.
    for (launcher_name, heads, shafts, ammo) in [
        (
            "Atlatl",
            "Bundle of Greater Deadly Broad Arrowheads",
            "Bundle of Atlatl Dart Shafts",
            "Greater Deadly Broadhead Atlatl Dart",
        ),
        (
            "Heavy Crossbow",
            "Wrapped Bundle of Burning Sands Arrowheads",
            "Wrapped Bundle of Quarrelshafts",
            "Burning Sands Bolt",
        ),
        (
            "Heavy Crossbow",
            "Wrapped Bundle of Olthoi Acid Arrowheads",
            "Wrapped Bundle of Quarrelshafts",
            "Olthoi Acid Bolt",
        ),
    ] {
        let set = format!("/tank set Ammo {ammo}");
        let mut run = crafter(
            vec![
                launcher(launcher_name),
                used_on(HEADS, heads),
                ingredient(SHAFTS, shafts),
            ],
            &["/tank set CraftAmmo true", &set, "/tank start"],
        );
        until_sent(&mut run, &combine(HEADS, SHAFTS), 10.0);
        no_errors(&run);
    }
    // Wrapped heads on a plain bundle of shafts is no recipe: they wait for wrapped shafts.
    let mut run = crafter(
        vec![
            bow(),
            used_on(HEADS, "Wrapped Bundle of Blunt Arrowheads"),
            ingredient(PLAIN_SHAFTS, "Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    run.advance(5.0);
    assert!(combines_sent(&run).is_empty(), "{:#?}", run.console(TANK));
    no_errors(&run);
    // With both, on the wrapped shafts.
    let mut run = crafter(
        vec![
            bow(),
            used_on(HEADS, "Wrapped Bundle of Blunt Arrowheads"),
            ingredient(PLAIN_SHAFTS, "Bundle of Arrowshafts"),
            ingredient(SHAFTS, "Wrapped Bundle of Arrowshafts"),
        ],
        &["/tank set CraftAmmo true", "/tank start"],
    );
    until_sent(&mut run, &combine(HEADS, SHAFTS), 10.0);
    assert_eq!(combines_sent(&run), vec![combine(HEADS, SHAFTS)]);
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-names-ammunition-as-retail-does-and-keeps-wrapped-heads-off-plain-shafts");
}
