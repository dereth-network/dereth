//! The Tank sample's errands of its own: buffs asked of a buffbot (another player's character that
//! buffs whoever tells it a word) when they run low, and the corpse a death leaves gone back to
//! and taken again, each run from `dereth/plugins/tank` in a scripted world on the run's own clock.

use dereth_primitives::ObjectId;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER, START_EW, START_NS,
};

use super::tank::{logged, tank, TANK};

dereth_testkit::scenarios! {
    scenario_the_tank_asks_its_buffbot_for_buffs_when_they_run_low_and_waits_for_them => the_tank_asks_its_buffbot_for_buffs_when_they_run_low_and_waits_for_them ["plugins.samples.the-tank-asks-its-buffbot-for-buffs-when-they-run-low-and-waits-for-them"],
    scenario_the_tank_gives_up_on_a_buffbot_that_is_absent_or_silent_and_carries_on => the_tank_gives_up_on_a_buffbot_that_is_absent_or_silent_and_carries_on ["plugins.samples.the-tank-gives-up-on-a-buffbot-that-is-absent-or-silent-and-carries-on"],
    scenario_the_tank_renews_its_buffbots_buffs_on_a_vendor_trip_when_they_would_run_low_before_the_next => the_tank_renews_its_buffbots_buffs_on_a_vendor_trip_when_they_would_run_low_before_the_next ["plugins.samples.the-tank-renews-its-buffbots-buffs-on-a-vendor-trip-when-they-would-run-low-before-the-next"],
    scenario_the_tank_goes_back_to_its_corpse_after_a_death_and_takes_everything_again => the_tank_goes_back_to_its_corpse_after_a_death_and_takes_everything_again ["plugins.samples.the-tank-goes-back-to-its-corpse-after-a-death-and-takes-everything-again"],
}

const BUD: u32 = 0x5000_0010;
/// The buffbot stands this far east of where the character starts, meters.
const BUD_EAST: f64 = 80.0;

/// The buffbot's character.
fn buffbot_at(east: f64) -> ScriptedObject {
    ScriptedObject {
        id: BUD,
        name: "Bud".into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((START_NS, START_EW + east / 240.0)),
        ..ScriptedObject::default()
    }
}

/// A tell to the character from the buffbot.
fn bud_says(run: &mut PluginRun, said: &str) {
    use dereth_protocol::comms::CommunicationHearDirectSpeech;
    run.server_says(&[ui_event(&CommunicationHearDirectSpeech {
        message: said.into(),
        sender_name: "Bud".into(),
        sender_id: ObjectId(BUD),
        target_id: ObjectId(SCRIPTED_PLAYER),
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })]);
}

/// The buffbot's buff `spell` (a scripted spell, its own category) lands on the character,
/// lasting `secs`.
fn buff_lands(run: &mut PluginRun, spell: u32, secs: f64) {
    run.server_says(&[ui_event(
        &dereth_protocol::qualities::MagicUpdateEnchantment(
            dereth_protocol::types::qualities::Enchantment {
                id: spell,
                category_word: spell,
                duration: secs,
                caster: ObjectId(BUD),
                smod: dereth_protocol::types::qualities::StatMod {
                    kind: 0x0200_8001,
                    key: 1,
                    value: 10.0,
                },
                ..Default::default()
            },
        ),
    )]);
}

/// Run until the Tank's console has `line`, at most `secs` seconds of the run's clock.
fn until(run: &mut PluginRun, line: &str, secs: f64) {
    let mut t = 0.0;
    while t < secs && !logged(run, line) {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(logged(run, line), "{line:?}: {:#?}", run.console(TANK));
}

fn at(run: &PluginRun, s: &str) -> usize {
    run.console(TANK)
        .iter()
        .position(|l| l.contains(s))
        .unwrap_or_else(|| panic!("{s:?} not in {:#?}", run.console(TANK)))
}

fn no_errors(run: &PluginRun) {
    let console = run.console(TANK);
    assert!(
        !console.iter().any(|l| l.contains(" error: ")),
        "{console:#?}"
    );
}

/// A Tank whose buffbot is Bud, set from the selection beside it, standing `east` meters off; the
/// macro neither fights nor walks a route.
fn with_buffbot(east: f64) -> PluginRun {
    let (mut run, _) = tank(vec![buffbot_at(east)]);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set EnableLooting false",
        "/tank set BuffbotMinutes 10",
    ] {
        run.type_line(line);
    }
    run.place_player(START_NS, START_EW + (east - 3.0) / 240.0);
    run.select(BUD);
    run.type_line("/tank buffbot set");
    run.place_player(START_NS, START_EW);
    run
}

/// With no buffs from its buffbot yet, the running Tank goes to it (a journey: it stands 80 m
/// off), tells it the request, waits while it is told its place in line, keeps the buffs that land
/// until "Done", and goes back; a buff of those running under BuffbotMinutes sends it again. Dead
/// while it waits there, it asks again where it comes back to life, and stays there after.
fn the_tank_asks_its_buffbot_for_buffs_when_they_run_low_and_waits_for_them() {
    let mut run = with_buffbot(BUD_EAST);
    run.type_line("/tank set StopMacroOnDeath false");
    run.type_line("/tank start");
    until(
        &mut run,
        "buffbot trip to Bud (no buffs from Bud yet), 80 m away",
        5.0,
    );
    until(&mut run, "buffbot: asked Bud for buffs", 60.0);
    assert!(
        run.sent().contains(&"tell Bud: buffs".to_owned()),
        "{:?}",
        run.sent()
    );
    let (ns, ew, _) = run.player_place().expect("placed");
    let (dn, de) = (ns - START_NS, ew - (START_EW + BUD_EAST / 240.0));
    let from_bud = (dn * dn + de * de).sqrt() * 240.0;
    assert!(from_bud <= 5.0, "beside the buffbot: {from_bud:.1} m");
    bud_says(&mut run, "I am busy; you are number 2 in line.");
    until(&mut run, "buffbot: number 2 in line", 5.0);
    // Its buffs land, then it says it is done.
    bud_says(&mut run, "Buffing you now: buffs, 2 spells.");
    buff_lands(&mut run, 2, 1800.0);
    buff_lands(&mut run, 1133, 1800.0);
    run.advance(1.0);
    bud_says(&mut run, "Done: 2 buffs cast on you.");
    until(&mut run, "buffbot trip: buffed by Bud", 60.0);
    assert!(logged(&run, "buffbot: buffed by Bud; 2 buffs landed"));
    assert!(
        at(&run, "buffbot: number 2 in line") < at(&run, "buffbot: Done: 2 buffs cast on you.")
    );
    let (ns, ew, _) = run.player_place().expect("placed");
    let (dn, de) = (ns - START_NS, ew - START_EW);
    let from_start = (dn * dn + de * de).sqrt() * 240.0;
    assert!(from_start <= 10.0, "back where it left: {from_start:.1} m");
    // Not due while the buffs last.
    run.advance(5.0);
    assert_eq!(
        run.console(TANK)
            .iter()
            .filter(|l| l.contains("buffbot trip to"))
            .count(),
        1,
        "{:#?}",
        run.console(TANK)
    );
    // One of them runs low: off again.
    buff_lands(&mut run, 1133, 200.0);
    until(
        &mut run,
        "buffbot trip to Bud (Armor Self I has 3 min left)",
        5.0,
    );
    // Dead while it waits beside the buffbot: what landed went with the death, so it asks again
    // where it comes back to life, and stays there when it is done.
    let asks = |run: &PluginRun| {
        run.sent()
            .iter()
            .filter(|s| *s == "tell Bud: buffs")
            .count()
    };
    let mut t = 0.0;
    while t < 60.0 && asks(&run) < 2 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(asks(&run), 2, "{:#?}", run.console(TANK));
    run.die();
    run.teleport(START_NS, START_EW + (BUD_EAST - 6.0) / 240.0);
    let mut t = 0.0;
    while t < 30.0 && asks(&run) < 3 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(
        asks(&run),
        3,
        "asked again after the death: {:#?}",
        run.console(TANK)
    );
    let back_trips = |run: &PluginRun| {
        run.console(TANK)
            .iter()
            .filter(|l| l.contains("travelling back from Bud"))
            .count()
    };
    let before = back_trips(&run);
    buff_lands(&mut run, 2, 1800.0);
    bud_says(&mut run, "Done: 1 buffs cast on you.");
    until(&mut run, "buffbot trip: buffed by Bud", 10.0);
    run.advance(5.0);
    assert_eq!(back_trips(&run), before, "no way back after the death");
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-asks-its-buffbot-for-buffs-when-they-run-low-and-waits-for-them",
    );
}

/// A buffbot that does not answer is asked three times, half a minute apart, then given up on, and
/// one that is not where it was is waited for a minute: either way the Tank goes back and carries
/// on, and asks again only after BuffbotRetryMinutes.
fn the_tank_gives_up_on_a_buffbot_that_is_absent_or_silent_and_carries_on() {
    let mut run = with_buffbot(20.0);
    run.type_line("/tank set BuffbotRetryMinutes 5");
    run.type_line("/tank start");
    until(
        &mut run,
        "buffbot: Bud did not answer; 0 buffs landed",
        120.0,
    );
    assert_eq!(
        run.sent()
            .iter()
            .filter(|s| *s == "tell Bud: buffs")
            .count(),
        3,
        "asked three times: {:?}",
        run.sent()
    );
    until(&mut run, "buffbot trip: Bud did not answer", 30.0);
    // Not again for five minutes.
    run.advance(60.0);
    assert_eq!(
        run.console(TANK)
            .iter()
            .filter(|l| l.contains("buffbot trip to"))
            .count(),
        1,
        "{:#?}",
        run.console(TANK)
    );
    // Gone from its place: waited for at the place, then given up on.
    run.remove(BUD);
    run.advance(250.0);
    until(&mut run, "buffbot: Bud is not where it was", 90.0);
    until(&mut run, "buffbot trip: Bud is not where it was", 30.0);
    assert_eq!(run.status(TANK), "running");
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-gives-up-on-a-buffbot-that-is-absent-or-silent-and-carries-on",
    );
}

/// On a vendor trip, buffs from the buffbot with under twice BuffbotMinutes left are renewed while
/// the character is in town: from the vendor it goes on to the buffbot, and from there back to
/// where the vendor trip set out (300 m off). With longer left, the vendor trip goes back as it is;
/// and a buffbot trip under way when the vendor trip comes up goes on from the vendor.
fn the_tank_renews_its_buffbots_buffs_on_a_vendor_trip_when_they_would_run_low_before_the_next() {
    use super::tank_items::{data, pack, selling_profile, vendor, VENDOR};
    let files = data(&[("profiles/sell.utl", &selling_profile())]);
    let mut objects = vendor();
    objects.extend(pack());
    objects.push(buffbot_at(BUD_EAST));
    let mut run = super::tank::tank_in(PluginRun::new(), &files, objects);
    for line in [
        "/tank set EnableCombat false",
        "/tank set EnableBuffing false",
        "/tank set EnableLooting false",
        "/tank set BuffbotMinutes 10",
        "/tank set Weapon Long Sword",
        "/tank profile sell.utl",
    ] {
        run.type_line(line);
    }
    run.place_player(START_NS, START_EW + (BUD_EAST - 3.0) / 240.0);
    run.select(BUD);
    run.type_line("/tank buffbot set");
    run.place_player(START_NS, START_EW);
    run.select(VENDOR);
    run.type_line("/tank vendor set");
    // Hunting 300 m north; buffed first, one buff lasting fifteen minutes.
    let hunt = (START_NS + 300.0 / 240.0, START_EW);
    run.place_player(hunt.0, hunt.1);
    run.type_line("/tank start");
    until(&mut run, "buffbot: asked Bud for buffs", 240.0);
    buff_lands(&mut run, 2, 900.0);
    buff_lands(&mut run, 1133, 3600.0);
    bud_says(&mut run, "Done: 2 buffs cast on you.");
    until(&mut run, "buffbot trip: buffed by Bud", 240.0);
    let from_hunt = |run: &PluginRun| {
        let (ns, ew, _) = run.player_place().expect("placed");
        ((ns - hunt.0).powi(2) + (ew - hunt.1).powi(2)).sqrt() * 240.0
    };
    assert!(from_hunt(&run) <= 10.0, "back: {:.1} m", from_hunt(&run));
    // To the vendor: the fifteen-minute buff, now under twenty, is renewed on the way back.
    run.type_line("/tank vendor");
    until(&mut run, "vendor trip: on to the buffbot", 240.0);
    assert!(logged(&run, "sold 2 (Rusty Dagger, Rusty Dagger)"));
    let asks = |run: &PluginRun| {
        run.sent()
            .iter()
            .filter(|s| *s == "tell Bud: buffs")
            .count()
    };
    let mut t = 0.0;
    while t < 120.0 && asks(&run) < 2 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(asks(&run), 2, "{:#?}", run.console(TANK));
    buff_lands(&mut run, 2, 3600.0);
    bud_says(&mut run, "Done: 1 buffs cast on you.");
    let mut t = 0.0;
    while t < 240.0 && from_hunt(&run) > 10.0 {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(
        from_hunt(&run) <= 10.0,
        "back where the vendor trip set out: {:.1} m: {:#?}",
        from_hunt(&run),
        run.console(TANK)
    );
    assert!(!logged(&run, "travelling back from Tester the Shopkeeper"));
    // An hour left: the vendor trip goes back by itself.
    run.type_line("/tank vendor");
    until(
        &mut run,
        "vendor trip: back from Tester the Shopkeeper",
        480.0,
    );
    assert_eq!(asks(&run), 2, "{:#?}", run.console(TANK));
    // A buffbot trip under way when the vendor trip comes up goes on from the vendor, and back
    // from the buffbot to where it set out.
    let count =
        |run: &PluginRun, s: &str| run.console(TANK).iter().filter(|l| l.contains(s)).count();
    run.type_line("/tank vendor");
    run.type_line("/tank buffbot");
    let mut t = 0.0;
    while t < 240.0 && count(&run, "vendor trip: on to the buffbot") < 2 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(
        count(&run, "vendor trip: on to the buffbot"),
        2,
        "{:#?}",
        run.console(TANK)
    );
    let mut t = 0.0;
    while t < 120.0 && asks(&run) < 3 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(asks(&run), 3, "{:#?}", run.console(TANK));
    bud_says(&mut run, "Done: 0 buffs cast on you.");
    let mut t = 0.0;
    while t < 240.0 && from_hunt(&run) > 10.0 {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(from_hunt(&run) <= 10.0, "back: {:.1} m", from_hunt(&run));
    assert_eq!(count(&run, "travelling back from Tester the Shopkeeper"), 1);
    assert_eq!(run.status(TANK), "running");
    no_errors(&run);
    note_asserted("plugins.samples.the-tank-renews-its-buffbots-buffs-on-a-vendor-trip-when-they-would-run-low-before-the-next");
}

/// After a death the character comes back to life at its life stone, 400 m away in another
/// landblock: the Tank travels back toward where it fell (a journey, stopped once in the corpse's
/// landblock), walks on from there, stopping to fight the monster on the way, opens
/// the corpse of its own name and takes back everything in it (what its loot profile would leave
/// too), and carries on; a death the game says left nothing is not gone back for, and three
/// deaths in a row on the way back give the recovery up.
fn the_tank_goes_back_to_its_corpse_after_a_death_and_takes_everything_again() {
    const CORPSE: u32 = 0x8000_00E0;
    const COINS: u32 = 0x8000_00E1;
    const JUNK: u32 = 0x8000_00E2;
    const KILLER: u32 = 0x8000_00E3;
    let (mut run, _) = tank(Vec::new());
    for line in [
        "/tank set EnableBuffing false",
        "/tank set EnableLooting false",
        "/tank set StopMacroOnDeath false",
    ] {
        run.type_line(line);
    }
    run.type_line("/tank start");
    run.advance(1.0);
    run.die();
    // Back to life at the life stone, the corpse where it fell, and a monster on the way to it.
    run.teleport(START_NS + 400.0 / 240.0, START_EW);
    run.spawn(vec![super::tank::monster(
        KILLER,
        "Drudge Skulker",
        START_NS + 60.0 / 240.0,
        START_EW,
    )]);
    run.spawn(vec![
        ScriptedObject {
            id: CORPSE,
            name: "Corpse of Tester".into(),
            obj_type: item_type::CONTAINER,
            bitfield: bitfield::CORPSE | bitfield::OPENABLE | bitfield::STUCK,
            place: Some((START_NS, START_EW)),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            id: COINS,
            name: "Pyreal".into(),
            obj_type: item_type::MONEY,
            container: Some(CORPSE),
            stack: Some(250),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            id: JUNK,
            name: "Rusty Dagger".into(),
            obj_type: item_type::MELEE_WEAPON,
            container: Some(CORPSE),
            ..ScriptedObject::default()
        },
    ]);
    until(&mut run, "target Drudge Skulker", 200.0);
    assert!(logged(&run, "alive again: back for the corpse first"));
    assert!(logged(&run, "travelling back to the corpse"));
    assert!(
        !logged(&run, "corpse: recovered"),
        "the monster first: {:#?}",
        run.console(TANK)
    );
    // It falls.
    run.remove(KILLER);
    until(&mut run, "corpse: recovered", 120.0);
    assert_eq!(run.container_of(COINS), Some(SCRIPTED_PLAYER));
    assert_eq!(run.container_of(JUNK), Some(SCRIPTED_PLAYER));
    // A death that left nothing: nothing to go back for.
    run.die();
    run.server_says(&[ui_event(
        &dereth_protocol::comms::CommunicationTextboxString {
            text: "You have retained all your items. You do not need to recover your corpse!"
                .into(),
            text_type: 0,
        },
    )]);
    run.teleport(START_NS + 400.0 / 240.0, START_EW);
    until(&mut run, "corpse: nothing was left in it", 15.0);
    run.advance(15.0);
    assert_eq!(
        run.console(TANK)
            .iter()
            .filter(|l| l.contains("travelling back to the corpse"))
            .count(),
        1
    );
    // Dead again and again on the way back: after three deaths in a row on the way, the
    // recovery is given up.
    for _ in 0..4 {
        run.die();
        run.advance(2.0);
    }
    until(
        &mut run,
        "corpse: given up after dying 3 times on the way back",
        5.0,
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-tank-goes-back-to-its-corpse-after-a-death-and-takes-everything-again",
    );
}
