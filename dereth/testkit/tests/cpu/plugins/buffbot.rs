//! The Buffbot sample: tells answered, a line of players buffed in turn and told their place, a
//! cap on each player's requests, players out of range asked to come closer and skipped, fizzles
//! cast again, its own mana got back and its components watched, each run from
//! `dereth/plugins/buffbot` in a scripted world on the run's own clock.

use dereth_primitives::ObjectId;
use dereth_rules::weenie::{bitfield, item_type};
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{
    read_package, ui_event, PluginRun, ScriptedObject, SCRIPTED_PLAYER, START_EW, START_NS,
};

use super::samples;

dereth_testkit::scenarios! {
    scenario_the_buffbot_answers_tells_and_buffs_each_player_in_turn => the_buffbot_answers_tells_and_buffs_each_player_in_turn ["plugins.samples.the-buffbot-answers-tells-and-buffs-each-player-in-turn"],
    scenario_the_buffbot_casts_a_fizzle_again_and_waits_for_a_player_out_of_range => the_buffbot_casts_a_fizzle_again_and_waits_for_a_player_out_of_range ["plugins.samples.the-buffbot-casts-a-fizzle-again-and-waits-for-a-player-out-of-range"],
    scenario_the_buffbot_caps_each_players_requests_and_its_line => the_buffbot_caps_each_players_requests_and_its_line ["plugins.samples.the-buffbot-caps-each-players-requests-and-its-line"],
    scenario_the_buffbot_gets_its_mana_back_and_warns_of_low_components => the_buffbot_gets_its_mana_back_and_warns_of_low_components ["plugins.samples.the-buffbot-gets-its-mana-back-and-warns-of-low-components"],
    scenario_the_buffbot_buys_its_components_again_when_low_and_goes_back_to_its_post => the_buffbot_buys_its_components_again_when_low_and_goes_back_to_its_post ["plugins.samples.the-buffbot-buys-its-components-again-when-low-and-goes-back-to-its-post"],
    scenario_the_buffbot_casts_its_own_buffs_by_a_self_profile_and_keeps_them_up => the_buffbot_casts_its_own_buffs_by_a_self_profile_and_keeps_them_up ["plugins.samples.the-buffbot-casts-its-own-buffs-by-a-self-profile-and-keeps-them-up"],
}

pub(crate) const BUFFBOT: &str = "network.dereth.buffbot";

const BOB: u32 = 0x5000_0002;
const CARL: u32 = 0x5000_0003;
const DAVE: u32 = 0x5000_0004;
const FAR: u32 = 0x5000_0005;

/// The buffs for another the scripted character knows besides Strength Other I: two levels of
/// Endurance Other and one of Invulnerability Other.
const BUFFS: &[(u32, &str, u32)] = &[
    (1355, "Endurance Other I", 4),
    (1356, "Endurance Other II", 4),
    (17, "Invulnerability Other I", 4),
];

/// Another player's character `east` meters east of the buffbot.
fn player(id: u32, name: &str, east: f64) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::CREATURE,
        bitfield: bitfield::PLAYER,
        place: Some((START_NS, START_EW + east / 240.0)),
        ..ScriptedObject::default()
    }
}

/// A wand in the pack, which the buffbot takes up to cast.
fn wand() -> ScriptedObject {
    ScriptedObject {
        id: 0x8000_00B0,
        name: "Wand".into(),
        obj_type: item_type::CASTER,
        container: Some(SCRIPTED_PLAYER),
        locations: Some(0x0100_0000),
        ..ScriptedObject::default()
    }
}

/// A Buffbot in a scripted world of `objects` that denies combat and allows support, buffing and
/// moving: a buffbot needs no leave to fight. Its profile `test` casts Strength Other, Endurance
/// Other and Invulnerability Other, and it answers requests.
fn buffbot(run: PluginRun, objects: Vec<ScriptedObject>) -> PluginRun {
    let mut run = run.with_buffs_for_others(BUFFS);
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ncombat = \"deny\"\nsupport = \"allow\"\nbuff = \"allow\"\nnav = \"allow\"\nvendor = \"allow\"\n",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("buffbot")));
    let mut all = vec![wand()];
    all.extend(objects);
    run.scripted_world("Tester", all);
    run.advance(0.5);
    run.type_line("/buffbot profile test Strength Other, Endurance Other, Invulnerability Other");
    run.type_line("/buffbot on");
    run
}

/// A tell to the buffbot from `name`.
fn tell(run: &mut PluginRun, name: &str, id: u32, said: &str) {
    use dereth_protocol::comms::CommunicationHearDirectSpeech;
    run.server_says(&[ui_event(&CommunicationHearDirectSpeech {
        message: said.into(),
        sender_name: name.into(),
        sender_id: ObjectId(id),
        target_id: ObjectId(SCRIPTED_PLAYER),
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    })]);
}

fn logged(run: &PluginRun, s: &str) -> bool {
    run.console(BUFFBOT).iter().any(|l| l.contains(s))
}

/// Where in the console the first line containing `s` is.
fn at(run: &PluginRun, s: &str) -> usize {
    run.console(BUFFBOT)
        .iter()
        .position(|l| l.contains(s))
        .unwrap_or_else(|| panic!("{s:?} not in {:#?}", run.console(BUFFBOT)))
}

/// Whether the buffbot told `name` the line `text`; the lines it sent and its console are printed
/// when it did not.
fn told(run: &PluginRun, name: &str, text: &str) -> bool {
    let line = format!("tell {name}: {text}");
    let found = run.sent().iter().any(|s| s == &line);
    if !found {
        eprintln!(
            "not sent {line:?}: {:#?}\n{:#?}",
            run.sent(),
            run.console(BUFFBOT)
        );
    }
    found
}

/// Run until the console has `line`, at most `secs` seconds of the run's clock (half-second
/// steps).
fn until(run: &mut PluginRun, line: &str, secs: f64) {
    let mut t = 0.0;
    while t < secs && !logged(run, line) {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(logged(run, line), "{line:?}: {:#?}", run.console(BUFFBOT));
}

fn no_errors(run: &PluginRun) {
    let console = run.console(BUFFBOT);
    assert!(
        !console.iter().any(|l| l.starts_with("error:")),
        "{console:#?}"
    );
    assert_eq!(run.status(BUFFBOT), "running", "{console:#?}");
}

/// In a world that denies combat and allows support: `help` and `list` are answered; a player's
/// request starts its buffs (each family at the highest level known: Endurance Other II, not I)
/// and "Done" ends them; a second player asking meanwhile is told its place and buffed next.
fn the_buffbot_answers_tells_and_buffs_each_player_in_turn() {
    let mut run = buffbot(
        PluginRun::new(),
        vec![player(BOB, "Bob", 5.0), player(CARL, "Carl", 6.0)],
    );
    tell(&mut run, "Bob", BOB, "help");
    run.advance(2.0);
    assert!(
        told(
            &run,
            "Bob",
            "Tell me a profile's name for its buffs: buffs, mage, melee, missile, test. Tell me list for each profile's spells, or cancel to leave the line."
        ),
        "{:?}",
        run.sent()
    );
    tell(&mut run, "Bob", BOB, "list");
    run.advance(8.0);
    assert!(
        told(
            &run,
            "Bob",
            "test: Strength Other I, Endurance Other II, Invulnerability Other I"
        ),
        "{:?}",
        run.sent()
    );
    tell(&mut run, "Bob", BOB, "test");
    run.advance(0.2);
    tell(&mut run, "Carl", CARL, "Test");
    until(&mut run, "done: Carl, 3 buffs", 60.0);
    // The tells go out a little over a second apart.
    run.advance(6.0);
    assert!(logged(
        &run,
        "request: Bob asks for test (number 1 in line)"
    ));
    assert!(told(&run, "Carl", "I am busy; you are number 1 in line."));
    assert!(told(&run, "Bob", "Buffing you now: test, 3 spells."));
    assert!(told(&run, "Bob", "Done: 3 buffs cast on you."));
    assert!(told(&run, "Carl", "Done: 3 buffs cast on you."));
    // Bob's three in the profile's order, then Carl's.
    let bob = [
        at(&run, "cast Strength Other I on Bob"),
        at(&run, "cast Endurance Other II on Bob"),
        at(&run, "cast Invulnerability Other I on Bob"),
    ];
    assert!(bob[0] < bob[1] && bob[1] < bob[2]);
    assert!(bob[2] < at(&run, "buffing Carl: test, 3 spells"));
    assert!(
        !logged(&run, "Endurance Other I on"),
        "the lower level is not cast"
    );
    // Into the magic stance, for support, in a world that denies combat.
    assert!(run
        .sent()
        .contains(&"wield 0x800000b0 0x1000000".to_owned()));
    no_errors(&run);
    note_asserted("plugins.samples.the-buffbot-answers-tells-and-buffs-each-player-in-turn");
}

/// A fizzle casts the same spell again, three tries at most; a player further than its range is
/// asked to come closer and waited for, buffed once near, and skipped when it does not come.
fn the_buffbot_casts_a_fizzle_again_and_waits_for_a_player_out_of_range() {
    let mut run = buffbot(
        PluginRun::new().with_cast_failures([0x0402, 0x0402]),
        vec![player(BOB, "Bob", 5.0), player(FAR, "Far", 90.0)],
    );
    tell(&mut run, "Bob", BOB, "test");
    until(&mut run, "done: Bob, 3 buffs", 60.0);
    assert!(logged(
        &run,
        "Strength Other I on Bob: fizzled (try 1 of 3)"
    ));
    assert!(logged(
        &run,
        "Strength Other I on Bob: fizzled (try 2 of 3)"
    ));
    assert!(at(&run, "fizzled (try 2 of 3)") < at(&run, "cast Strength Other I on Bob"));
    // Out of range: asked, and buffed once it comes near.
    tell(&mut run, "Far", FAR, "test");
    until(&mut run, "Far is out of range: waiting 60 s", 10.0);
    run.advance(2.0);
    assert!(told(
        &run,
        "Far",
        "Please come within 20 m of me; I'll wait 60 seconds."
    ));
    run.move_object(FAR, START_NS, START_EW + 8.0 / 240.0);
    until(&mut run, "done: Far, 3 buffs", 60.0);
    // Out of range again, and it never comes: skipped after the wait.
    run.move_object(FAR, START_NS, START_EW + 90.0 / 240.0);
    let mut run = run;
    run.type_line("/buffbot set PerPlayerCap 10");
    tell(&mut run, "Far", FAR, "test");
    until(&mut run, "skipped Far: not near enough", 75.0);
    run.advance(2.0);
    assert!(told(
        &run,
        "Far",
        "You didn't come near enough; tell me again when you're close."
    ));
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-buffbot-casts-a-fizzle-again-and-waits-for-a-player-out-of-range",
    );
}

/// A player asks at most PerPlayerCap times an hour; the line holds at most QueueMax; a player
/// already in line is told its place again; `cancel` leaves the line. (Bob, out of range at first,
/// is waited for while the others ask.)
fn the_buffbot_caps_each_players_requests_and_its_line() {
    let mut run = buffbot(
        PluginRun::new(),
        vec![
            player(BOB, "Bob", 90.0),
            player(CARL, "Carl", 6.0),
            player(DAVE, "Dave", 7.0),
        ],
    );
    run.type_line("/buffbot set PerPlayerCap 1");
    run.type_line("/buffbot set QueueMax 1");
    tell(&mut run, "Bob", BOB, "test");
    run.advance(1.0);
    tell(&mut run, "Carl", CARL, "test");
    run.advance(0.2);
    tell(&mut run, "Carl", CARL, "test");
    run.advance(0.2);
    tell(&mut run, "Dave", DAVE, "test");
    run.advance(4.0);
    assert!(told(&run, "Carl", "I am busy; you are number 1 in line."));
    assert!(told(&run, "Carl", "You are already number 1 in line."));
    assert!(told(
        &run,
        "Dave",
        "The line is full; ask again in a few minutes."
    ));
    tell(&mut run, "Carl", CARL, "cancel");
    run.advance(2.0);
    assert!(told(&run, "Carl", "You have left the line."));
    run.move_object(BOB, START_NS, START_EW + 5.0 / 240.0);
    until(&mut run, "done: Bob, 3 buffs", 60.0);
    tell(&mut run, "Bob", BOB, "test");
    run.advance(3.0);
    assert!(
        told(
            &run,
            "Bob",
            "Your requests this hour: 1 of 1; ask again in 60 minutes."
        ),
        "{:?}",
        run.sent()
    );
    assert!(!logged(&run, "buffing Carl"), "Carl left the line");
    no_errors(&run);
    note_asserted("plugins.samples.the-buffbot-caps-each-players-requests-and-its-line");
}

/// With its mana under ManaRest between players, the buffbot gets it back (Stamina to Mana Self)
/// before the next, and tells the player waiting; a component it is using up and has fewer than
/// ComponentWarn of is warned of, once.
fn the_buffbot_gets_its_mana_back_and_warns_of_low_components() {
    let hyssop = |id: u32, n: u16| ScriptedObject {
        stack: Some(n),
        max_stack: Some(100),
        ..ScriptedObject {
            id,
            name: "Hyssop".into(),
            obj_type: item_type::SPELL_COMPONENTS,
            container: Some(SCRIPTED_PLAYER),
            ..ScriptedObject::default()
        }
    };
    let mut run = buffbot(
        PluginRun::new(),
        vec![player(BOB, "Bob", 5.0), hyssop(0x8000_00C0, 12)],
    );
    run.set_vitals((100, 100), (100, 100), (20, 100));
    tell(&mut run, "Bob", BOB, "test");
    until(&mut run, "mana: Stamina to Mana Self I", 20.0);
    assert!(logged(&run, "resting: mana 20%"));
    assert!(told(
        &run,
        "Bob",
        "I am getting my mana back; you are next."
    ));
    assert!(!logged(&run, "buffing Bob"), "not before the mana is back");
    run.set_vitals((100, 100), (100, 100), (95, 100));
    until(&mut run, "done: Bob, 3 buffs", 60.0);
    // Twelve, then eight: in use, and under ten.
    run.remove(0x8000_00C0);
    run.spawn(vec![hyssop(0x8000_00C1, 8)]);
    until(&mut run, "components low: 8 Hyssop left", 5.0);
    run.remove(0x8000_00C1);
    run.spawn(vec![hyssop(0x8000_00C2, 7)]);
    run.advance(3.0);
    let warnings = run
        .console(BUFFBOT)
        .iter()
        .filter(|l| l.starts_with("components low"))
        .count();
    assert_eq!(warnings, 1, "warned once: {:#?}", run.console(BUFFBOT));
    no_errors(&run);
    note_asserted("plugins.samples.the-buffbot-gets-its-mana-back-and-warns-of-low-components");
}

/// What the Restock setting lists runs low (ten Prismatic Tapers of a hundred, under a quarter;
/// no mana potions of ten): with no one in line, the buffbot walks to the vendor 40 m off, buys
/// them up to their counts, and walks back to where it stood. A request made while it is away is
/// told it waits, and is buffed once it is back. Stocked again, it does not go again; low again
/// with players waiting (nothing run out), it buffs them first. Its post is where it was turned
/// on: moved off it, it walks back.
fn the_buffbot_buys_its_components_again_when_low_and_goes_back_to_its_post() {
    use super::tank::{held, inside};
    use dereth_testkit::plugins::ScriptedVendor;
    const SHOP: u32 = 0x8000_0C00;
    const SHOP_TAPERS: u32 = 0x8000_0C01;
    const SHOP_POTIONS: u32 = 0x8000_0C02;
    const MY_TAPERS: u32 = 0x8000_0C03;
    let objects = vec![
        ScriptedObject {
            id: SHOP,
            name: "Tester the Archmage".into(),
            obj_type: item_type::CREATURE,
            place: Some((START_NS, START_EW + 40.0 / 240.0)),
            vendor: Some(ScriptedVendor { buys: 0 }),
            ..ScriptedObject::default()
        },
        ScriptedObject {
            value: 5,
            max_stack: Some(1000),
            ..inside(
                SHOP_TAPERS,
                "Prismatic Taper",
                item_type::SPELL_COMPONENTS,
                SHOP,
            )
        },
        ScriptedObject {
            value: 20,
            max_stack: Some(100),
            ..inside(SHOP_POTIONS, "Mana Potion", item_type::FOOD, SHOP)
        },
        ScriptedObject {
            value: 5,
            stack: Some(10),
            max_stack: Some(1000),
            ..held(MY_TAPERS, "Prismatic Taper", item_type::SPELL_COMPONENTS)
        },
        player(BOB, "Bob", 5.0),
        player(CARL, "Carl", 6.0),
        player(DAVE, "Dave", 7.0),
    ];
    let mut run = buffbot(PluginRun::new(), objects);
    run.set_coins(10_000);
    run.type_line(
        "/buffbot set Restock Prismatic Taper=100@Tester the Archmage, Mana Potion=10@Tester the Archmage",
    );
    until(
        &mut run,
        "restocking: Prismatic Taper (10 of 100), Mana Potion (0 of 10)",
        10.0,
    );
    // A request while it is away waits in line.
    tell(&mut run, "Bob", BOB, "test");
    let away = "I am buying components and will be back shortly; you are number 1 in line.";
    let mut t = 0.0;
    while t < 5.0 && !run.sent().contains(&format!("tell Bob: {away}")) {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(told(&run, "Bob", away));
    until(
        &mut run,
        "restocking: at Tester the Archmage: bought 90 Prismatic Taper, 10 Mana Potion",
        120.0,
    );
    let (ns, ew, _) = run.player_place().expect("placed");
    let from_shop =
        ((ns - START_NS).powi(2) + (ew - (START_EW + 40.0 / 240.0)).powi(2)).sqrt() * 240.0;
    assert!(
        from_shop <= 5.0,
        "bought at the vendor: {from_shop:.1} m off"
    );
    until(&mut run, "restocking: back at its post", 120.0);
    let (ns, ew, _) = run.player_place().expect("placed");
    let from_post = ((ns - START_NS).powi(2) + (ew - START_EW).powi(2)).sqrt() * 240.0;
    assert!(from_post <= 2.0, "back at its post: {from_post:.1} m");
    until(&mut run, "done: Bob", 120.0);
    assert!(at(&run, "restocking: back at its post") < at(&run, "done: Bob"));
    let held_of = |run: &PluginRun, name: &str| -> u32 {
        run.held_items()
            .iter()
            .filter(|(_, n, _)| n == name)
            .map(|(_, _, c)| *c)
            .sum()
    };
    assert_eq!(held_of(&run, "Prismatic Taper"), 100);
    assert_eq!(held_of(&run, "Mana Potion"), 10);
    // The vendor's farewell by tell is not a request: it is not answered.
    tell(
        &mut run,
        "Tester the Archmage",
        SHOP,
        "Thank you for your business. Please return soon.",
    );
    run.advance(3.0);
    assert!(
        !run.sent()
            .iter()
            .any(|l| l.starts_with("tell Tester the Archmage")),
        "{:#?}",
        run.sent()
    );
    // Stocked: no second trip.
    run.advance(30.0);
    assert_eq!(
        run.console(BUFFBOT)
            .iter()
            .filter(|l| l.starts_with("restocking: Prismatic"))
            .count(),
        1,
        "{:#?}",
        run.console(BUFFBOT)
    );
    // Low again (a hundred of a thousand) with two players waiting, none of it run out: both are
    // buffed first, and it goes once the line is empty.
    tell(&mut run, "Carl", CARL, "test");
    tell(&mut run, "Dave", DAVE, "test");
    run.type_line("/buffbot set Restock Prismatic Taper=1000@Tester the Archmage");
    until(&mut run, "restocking: Prismatic Taper (100 of 1000)", 240.0);
    assert!(at(&run, "done: Dave") < at(&run, "restocking: Prismatic Taper (100 of 1000)"));
    let trips_back = |run: &PluginRun| {
        run.console(BUFFBOT)
            .iter()
            .filter(|l| *l == "restocking: back at its post")
            .count()
    };
    let mut t = 0.0;
    while t < 240.0 && trips_back(&run) < 2 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(trips_back(&run), 2, "{:#?}", run.console(BUFFBOT));
    // Off its post (50 m north, as after a lost connection on the way): it walks back.
    run.teleport(START_NS + 50.0 / 240.0, START_EW);
    until(&mut run, "back to its post (50 m away)", 10.0);
    let mut t = 0.0;
    let home = |run: &PluginRun| run.console(BUFFBOT).iter().any(|l| l == "back at its post");
    while t < 120.0 && !home(&run) {
        run.advance(0.5);
        t += 0.5;
    }
    assert!(home(&run), "{:#?}", run.console(BUFFBOT));
    let (ns, ew, _) = run.player_place().expect("placed");
    let from_post = ((ns - START_NS).powi(2) + (ew - START_EW).powi(2)).sqrt() * 240.0;
    assert!(from_post <= 2.0, "at its post again: {from_post:.1} m");
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-buffbot-buys-its-components-again-when-low-and-goes-back-to-its-post",
    );
}

/// Its own buffs, by a self profile (Alinco's buffing): `/buffbot self test` casts each of the
/// profile's buffs once, the strength buff at its level 7 (Might of the Lugians, a name of its
/// own) since the skill casts it reliably, Blood Drinker on the sword it wields and Impenetrability
/// on the armor it wears; asked again with more than SelfRebuffMinutes left on each, it casts
/// nothing; with the margin above what is left, all again. Kept up (SelfBuff), a buff dispelled is
/// cast again, though the buffbot answers no requests.
fn the_buffbot_casts_its_own_buffs_by_a_self_profile_and_keeps_them_up() {
    use dereth_protocol::qualities::MagicDispelEnchantment;
    const SWORD: u32 = 0x8000_0C01;
    const ARMOR: u32 = 0x8000_0C02;
    let mut run = PluginRun::new()
        .with_spells(&[(70, "Focus Self I", 4)], true)
        .with_item_spells(&[(72, "Blood Drinker I"), (73, "Impenetrability I")]);
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1
[automation]
combat = \"deny\"
support = \"deny\"
buff = \"allow\"
",
        )
        .expect("a policy"),
    );
    run.load(read_package(&samples().join("buffbot")));
    // Worn, and open to spells as gear is.
    let held = |id: u32, name: &str, obj_type: u32, locations: u32| ScriptedObject {
        id,
        name: name.into(),
        obj_type,
        bitfield: bitfield::ATTACKABLE,
        container: Some(SCRIPTED_PLAYER),
        locations: Some(locations),
        worn: true,
        ..ScriptedObject::default()
    };
    run.scripted_world(
        "Tester",
        vec![
            wand(),
            held(SWORD, "Sword", item_type::MELEE_WEAPON, 0x0010_0000),
            held(ARMOR, "Breastplate", item_type::ARMOR, 0x0000_0200),
        ],
    );
    run.casts_land(true);
    run.set_skill("Creature Enchantment", 400);
    run.advance(0.5);
    run.type_line(
        "/buffbot selfprofile test Strength Self, Focus Self, Blood Drinker, Impenetrability",
    );
    run.type_line("/buffbot self test");
    until(&mut run, "buffed yourself: test, 4 cast", 60.0);
    let me = format!("{SCRIPTED_PLAYER:#x}");
    let count = |run: &PluginRun, s: &str| run.sent().iter().filter(|l| *l == s).count();
    assert_eq!(
        count(&run, &format!("cast 2087 {me}")),
        1,
        "{:#?}",
        run.console(BUFFBOT)
    );
    assert_eq!(
        count(&run, &format!("cast 1 {me}")),
        0,
        "not the lower level"
    );
    assert_eq!(count(&run, &format!("cast 70 {me}")), 1);
    assert_eq!(
        count(&run, &format!("cast 72 {SWORD:#x}")),
        1,
        "{:#?}",
        run.sent()
    );
    assert_eq!(count(&run, &format!("cast 73 {ARMOR:#x}")), 1);
    // Every buff has more than five minutes left: nothing to cast.
    run.advance(6.0);
    run.type_line("/buffbot self");
    until(&mut run, "buffed yourself: test, 0 cast", 20.0);
    // With the margin above the half hour each lasts, every one is due again.
    run.type_line("/buffbot set SelfRebuffMinutes 31");
    run.type_line("/buffbot self");
    let done = |run: &PluginRun| {
        run.console(BUFFBOT)
            .iter()
            .filter(|l| *l == "buffed yourself: test, 4 cast")
            .count()
    };
    let mut t = 0.0;
    while t < 60.0 && done(&run) < 2 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(done(&run), 2, "{:#?}", run.console(BUFFBOT));
    assert_eq!(count(&run, &format!("cast 2087 {me}")), 2);
    assert_eq!(count(&run, &format!("cast 73 {ARMOR:#x}")), 2);
    // Kept up: Focus dispelled, it is cast again on its own.
    run.type_line("/buffbot set SelfRebuffMinutes 5");
    run.type_line("/buffbot set SelfBuff on");
    run.advance(10.0);
    assert_eq!(
        count(&run, &format!("cast 70 {me}")),
        2,
        "nothing cast while all last"
    );
    run.server_says(&[ui_event(&MagicDispelEnchantment {
        layered_spell_id: 70,
    })]);
    let mut t = 0.0;
    while t < 30.0 && count(&run, &format!("cast 70 {me}")) < 3 {
        run.advance(0.5);
        t += 0.5;
    }
    assert_eq!(
        count(&run, &format!("cast 70 {me}")),
        3,
        "{:#?}",
        run.console(BUFFBOT)
    );
    assert_eq!(
        count(&run, &format!("cast 2087 {me}")),
        2,
        "only the one dispelled"
    );
    // In the magic stance for buffing, in a world that denies combat and support.
    assert!(
        !run.console(BUFFBOT)
            .iter()
            .any(|l| l.contains("could not enter the magic stance")),
        "{:#?}",
        run.console(BUFFBOT)
    );
    no_errors(&run);
    note_asserted(
        "plugins.samples.the-buffbot-casts-its-own-buffs-by-a-self-profile-and-keeps-them-up",
    );
}
