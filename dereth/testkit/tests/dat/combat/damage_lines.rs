use super::*;

// ---------------------------------------------------------------------------------------------
// combat.damage-line.*
//
// The tables below are written out rather than left behind the evidence handle, because here the
// transcription **is** the behaviour: what the player reads is the adjective,
// the damage word and the body part, and a scenario that asserted one cell of each would be a
// scenario about one fight. The corpus's single combat recording carries eight attacks, all of
// one damage type at one damage number, which is why the tables are here.
// ---------------------------------------------------------------------------------------------

/// The channel the player's own hits are written on.
const COMBAT_SELF: u8 = 22;
/// And the channel the hits he takes are written on.
const COMBAT_ENEMY: u8 = 21;

/// `(damage type, the four singular verbs, the four plural ones)`, weakest bucket first.
const ADJECTIVES: [(u32, [&str; 4], [&str; 4]); 9] = [
    (
        dt::SLASH,
        ["scratch", "cut", "slash", "mangle"],
        ["scratches", "cuts", "slashes", "mangles"],
    ),
    (
        dt::PIERCE,
        ["nick", "stab", "impale", "gore"],
        ["nicks", "stabs", "impales", "gores"],
    ),
    (
        dt::BLUDGEON,
        ["graze", "bash", "smash", "crush"],
        ["grazes", "bashes", "smashes", "crushes"],
    ),
    (
        dt::COLD,
        ["numb", "chill", "frost", "freeze"],
        ["numbs", "chills", "frosts", "freezes"],
    ),
    (
        dt::FIRE,
        ["singe", "scorch", "burn", "incinerate"],
        ["singes", "scorches", "burns", "incinerates"],
    ),
    (
        dt::ACID,
        ["blister", "sear", "corrode", "dissolve"],
        ["blisters", "sears", "corrodes", "dissolves"],
    ),
    (
        dt::ELECTRIC,
        ["spark", "shock", "jolt", "blast"],
        ["sparks", "shocks", "jolts", "blasts"],
    ),
    (
        dt::HEALTH,
        ["drain", "exhaust", "siphon", "deplete"],
        ["drains", "exhausts", "siphons", "depletes"],
    ),
    (
        dt::NETHER,
        ["scar", "twist", "wither", "eradicate"],
        ["scars", "twists", "withers", "eradicates"],
    ),
];

/// Which of the four verbs a share of the target's health picks. **The comparison is "at most"**,
/// so a share of exactly a tenth, a quarter or a half falls in the *lower* bucket.
fn bucket(percent: f64) -> usize {
    assert!(percent >= 0.0, "a negative share never reaches the table");
    if percent <= 0.10 {
        0
    } else if percent <= 0.25 {
        1
    } else if percent <= 0.50 {
        2
    } else {
        3
    }
}

/// Twelve shares: every threshold from both sides, and both ends.
const PERCENTS: [f64; 12] = [
    0.0, 0.0999, 0.1, 0.1001, 0.2499, 0.25, 0.2501, 0.4999, 0.5, 0.5001, 0.75, 1.0,
];

/// The nine damage types that have a name, in the order the client tests them.
const NAMES: [(u32, &str); 9] = [
    (dt::SLASH, "slashing"),
    (dt::PIERCE, "piercing"),
    (dt::BLUDGEON, "bludgeoning"),
    (dt::COLD, "cold"),
    (dt::FIRE, "fire"),
    (dt::ACID, "acid"),
    (dt::ELECTRIC, "electrical"),
    (dt::NETHER, "nether"),
    (dt::BASE, "prismatic"),
];

/// The word before `damage!`: the joined list and a space, or nothing at all when the list is
/// empty or does not fit the sixty-four bytes the client gives it.
fn damage_word(mask: u32) -> String {
    let joined = NAMES
        .iter()
        .filter(|(m, _)| mask & m != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() || joined.len() + 1 > 0x40 {
        String::new()
    } else {
        format!("{joined} ")
    }
}

/// The twenty-nine body parts, in the order the client indexes them.
const PARTS: [&str; 29] = [
    "undefined",
    "head",
    "chest",
    "abdomen",
    "upper arm",
    "lower arm",
    "hand",
    "upper leg",
    "lower leg",
    "foot",
    "horn",
    "front leg",
    "unknown",
    "front foot",
    "rear leg",
    "unknown",
    "rear foot",
    "torso",
    "tail",
    "arm",
    "leg",
    "claw",
    "wings",
    "breath",
    "tentacle",
    "upper tentacle",
    "lower tentacle",
    "cloak",
    "num",
];

fn part_name(part: u32) -> &'static str {
    let idx = part.wrapping_add(1);
    if idx <= 0x1c {
        PARTS[idx as usize]
    } else {
        "unknown"
    }
}

/// One notification, encoded through the production writer and delivered on the queue a panel
/// consumes -- so the blob the client reads is the blob the codec makes.
fn attacker(
    defender_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    critical: u32,
    attack_conditions: u32,
) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::combat::AttackerNotification {
        defender_name: defender_name.to_owned(),
        damage_type,
        percent,
        damage,
        critical,
        attack_conditions,
        attack_conditions_high: 0,
    })
}

#[allow(clippy::too_many_arguments)]
fn defender(
    attacker_name: &str,
    damage_type: u32,
    percent: f64,
    damage: u32,
    damage_location: u32,
    critical: u32,
    attack_conditions: u32,
) -> dereth_testkit::Inbound {
    dereth_testkit::Inbound::message(&dereth_protocol::combat::DefenderNotification {
        attacker_name: attacker_name.to_owned(),
        damage_type,
        percent,
        damage,
        damage_location,
        critical,
        attack_conditions,
        attack_conditions_high: 0,
    })
}

/// Deliver one notification and answer the single line the client composed from it.
fn line(c: &mut HeadlessClient, m: dereth_testkit::Inbound) -> (u8, String) {
    let before = c.view().chat_lines().len();
    c.when(m);
    let composed: Vec<(u8, String)> = c.view().chat_lines()[before..]
        .iter()
        .map(|l| (l.ty, l.body.clone()))
        .collect();
    assert_eq!(
        composed.len(),
        1,
        "one notification composes exactly one line, got {composed:?}"
    );
    composed[0].clone()
}

/// A whole client with the shipped gameplay screen up, which is where a damage line is drawn.
fn a_client_in_the_world() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// The two sentences, the plural rule, the damage word and the body part.
pub(super) fn a_damage_line_says_what_was_hit_how_hard_and_with_what() {
    let mut c = a_client_in_the_world();

    // The two sentences, whole. The attacker's comes from one format; the defender's is assembled
    // from fragments, and the two have different word order and a different channel.
    let attacker_sentence = line(&mut c, attacker("Drudge Slave", dt::SLASH, 0.30, 12, 0, 0))
        == (
            COMBAT_SELF,
            "You slash Drudge Slave for 12 points of slashing damage!\n".to_owned(),
        );
    let defender_sentence = line(
        &mut c,
        defender("Drudge Slave", dt::BLUDGEON, 0.125, 7, 3, 0, 0),
    ) == (
        COMBAT_ENEMY,
        "Drudge Slave bashes your upper arm for 7 points of bludgeoning damage!\n".to_owned(),
    );

    // Exactly one point drops the plural; zero takes it.
    let mut plural = true;
    for (damage, tail) in [(0u32, "0 points"), (1, "1 point"), (2, "2 points")] {
        plural &= line(&mut c, attacker("Rat", dt::PIERCE, 0.05, damage, 0, 0)).1
            == format!("You nick Rat for {tail} of piercing damage!\n");
        plural &= line(&mut c, defender("Rat", dt::PIERCE, 0.05, damage, 1, 0, 0)).1
            == format!("Rat nicks your chest for {tail} of piercing damage!\n");
    }

    // Every damage type that has a name, and the three that do not: a health, stamina or mana
    // drain names no damage at all, exactly as an unspecified type does.
    let mut words = true;
    for (ty, word) in [
        (0u32, ""),
        (dt::SLASH, "slashing "),
        (dt::PIERCE, "piercing "),
        (dt::BLUDGEON, "bludgeoning "),
        (dt::COLD, "cold "),
        (dt::FIRE, "fire "),
        (dt::ACID, "acid "),
        (dt::ELECTRIC, "electrical "),
        (dt::HEALTH, ""),
        (dt::STAMINA, ""),
        (dt::MANA, ""),
        (dt::NETHER, "nether "),
        (dt::BASE, "prismatic "),
    ] {
        words &= damage_word(ty) == word;
        words &= line(&mut c, attacker("Shreth", ty, 0.6, 40, 0, 0))
            .1
            .ends_with(&format!("for 40 points of {word}damage!\n"));
    }

    // Several types at once are joined, in the client's own order, and a list too long for the
    // room the client gives it leaves the sentence with no damage word at all.
    let eight = dt::SLASH
        | dt::PIERCE
        | dt::BLUDGEON
        | dt::COLD
        | dt::FIRE
        | dt::ACID
        | dt::ELECTRIC
        | dt::NETHER;
    let joined = line(
        &mut c,
        attacker("Virindi", dt::SLASH | dt::PIERCE, 0.2, 9, 0, 0),
    )
    .1 == "You hit Virindi for 9 points of slashing/piercing damage!\n"
        && line(&mut c, attacker("Virindi", eight, 0.2, 9, 0, 0)).1
            == "You hit Virindi for 9 points of \
                slashing/piercing/bludgeoning/cold/fire/acid/electrical/nether damage!\n"
        && line(&mut c, attacker("Virindi", eight | dt::BASE, 0.2, 9, 0, 0)).1
            == "You hit Virindi for 9 points of damage!\n";

    // Every body part row, both ends of the range check, and the two rows the client itself
    // leaves unnamed.
    let mut parts = 0usize;
    let mut body_parts = true;
    for part in (0u32..=0x1d).chain([0xFF, 0x8000_0000, 0xFFFF_FFFF]) {
        let name = part_name(part);
        let (ty, body) = line(
            &mut c,
            defender("Banderling", dt::COLD, 0.4, 18, part, 0, 0),
        );
        body_parts &= ty == COMBAT_ENEMY
            && body == format!("Banderling frosts your {name} for 18 points of cold damage!\n");
        parts += 1;
    }
    body_parts &= parts == 33
        && part_name(11) == "unknown"
        && part_name(14) == "unknown"
        && part_name(0xFFFF_FFFF) == "undefined"
        && part_name(0x1b) == "num"
        && part_name(0x1c) == "unknown";

    c.assert_behaviour(
        "combat.damage-line.says-what-was-hit-how-hard-and-with-what",
        move |_| attacker_sentence && defender_sentence && plural && words && joined && body_parts,
    );
    c.shutdown();
}

/// The four verbs, both sides of every threshold, both sentences.
pub(super) fn the_verb_steps_at_the_thresholds() {
    let mut c = a_client_in_the_world();

    let mut checked = 0usize;
    let mut held = true;
    for (ty, singular, plural) in ADJECTIVES {
        let word = damage_word(ty);
        for percent in PERCENTS {
            let b = bucket(percent);
            let (t, body) = line(&mut c, attacker("Olthoi Worker", ty, percent, 23, 0, 0));
            held &= t == COMBAT_SELF
                && body
                    == format!(
                        "You {} Olthoi Worker for 23 points of {word}damage!\n",
                        singular[b]
                    );
            let (t, body) = line(&mut c, defender("Olthoi Worker", ty, percent, 23, 0, 0, 0));
            held &= t == COMBAT_ENEMY
                && body
                    == format!(
                        "Olthoi Worker {} your head for 23 points of {word}damage!\n",
                        plural[b]
                    );
            checked += 2;
        }
    }
    let all_of_them = checked == 216;

    // A share below zero never reaches the table at all: the sentence keeps the plain verb.
    let mut negatives = true;
    for percent in [-0.0001, -0.5, -1.0] {
        negatives &= line(&mut c, attacker("Drudge", dt::FIRE, percent, 3, 0, 0)).1
            == "You hit Drudge for 3 points of fire damage!\n";
        negatives &= line(&mut c, defender("Drudge", dt::FIRE, percent, 3, 1, 0, 0)).1
            == "Drudge hits your chest for 3 points of fire damage!\n";
    }

    // And a type with no verb row of its own -- including two types combined -- says the same.
    let mut unrecognised = true;
    for ty in [
        0,
        dt::SLASH | dt::PIERCE,
        dt::SLASH | dt::BLUDGEON,
        dt::COLD | dt::FIRE,
        dt::STAMINA,
        dt::MANA,
        dt::BASE,
        0xFFFF_FFFF,
    ] {
        let word = damage_word(ty);
        unrecognised &= line(&mut c, attacker("Tusker", ty, 0.9, 5, 0, 0)).1
            == format!("You hit Tusker for 5 points of {word}damage!\n");
    }

    c.assert_behaviour(
        "combat.damage-line.the-verb-steps-at-the-thresholds-and-an-unranked-hit-just-hits",
        move |_| held && all_of_them && negatives && unrecognised,
    );
    c.shutdown();
}

/// A critical, a sneak attack and a reckless swing each announce themselves, in their own words
/// for each side and in one order.
pub(super) fn a_critical_a_sneak_and_a_reckless_swing_each_say_so() {
    let mut c = a_client_in_the_world();

    let critical = line(&mut c, attacker("Rat", dt::SLASH, 0.05, 1, 1, 0)).1
        == "Critical hit!  You scratch Rat for 1 point of slashing damage!\n"
        && line(&mut c, defender("Rat", dt::SLASH, 0.05, 1, 5, 1, 0)).1
            == "Critical hit! Rat scratches your hand for 1 point of slashing damage!\n";

    let sneak = line(
        &mut c,
        attacker("Rat", dt::SLASH, 0.05, 1, 0, cond::SNEAK_ATTACK),
    )
    .1 == "Sneak Attack! You scratch Rat for 1 point of slashing damage!\n"
        && line(
            &mut c,
            defender("Rat", dt::SLASH, 0.05, 1, 5, 0, cond::SNEAK_ATTACK),
        )
        .1 == "Sneak Attack! Rat scratches your hand for 1 point of slashing damage!\n";

    // The two sides word this one differently, which is why they are asserted separately.
    let reckless = line(
        &mut c,
        attacker("Rat", dt::SLASH, 0.05, 1, 0, cond::RECKLESSNESS),
    )
    .1 == "Recklessness! You scratch Rat for 1 point of slashing damage!\n"
        && line(
            &mut c,
            defender("Rat", dt::SLASH, 0.05, 1, 5, 0, cond::RECKLESSNESS),
        )
        .1 == "Reckless! Rat scratches your hand for 1 point of slashing damage!\n";

    let both = cond::SNEAK_ATTACK | cond::RECKLESSNESS;
    let all_three_in_order = line(&mut c, attacker("Rat", dt::SLASH, 0.05, 1, 1, both)).1
        == "Critical hit!  Sneak Attack! Recklessness! You scratch Rat for 1 point of slashing \
            damage!\n"
        && line(&mut c, defender("Rat", dt::SLASH, 0.05, 1, 5, 1, both)).1
            == "Critical hit! Sneak Attack! Reckless! Rat scratches your hand for 1 point of \
                slashing damage!\n";

    // Critical protection is the one that appends a whole sentence after the damage rather than
    // before the verb.
    let protection = line(
        &mut c,
        attacker("Drudge", dt::FIRE, 0.7, 60, 1, cond::CRITICAL_PROTECTION),
    )
    .1
        == "Critical hit!  You incinerate Drudge for 60 points of fire damage! Your target's \
                Critical Protection augmentation allows them to avoid your critical hit!\n"
        && line(
            &mut c,
            defender("Drudge", dt::FIRE, 0.7, 60, 0, 1, cond::CRITICAL_PROTECTION),
        )
        .1 == "Critical hit! Drudge incinerates your head for 60 points of fire damage! Your \
                    Critical Protection augmentation allows you to avoid a critical hit!\n";

    c.assert_behaviour(
        "combat.damage-line.a-critical-a-sneak-and-a-reckless-swing-each-say-so",
        move |_| critical && sneak && reckless && all_three_in_order && protection,
    );
    c.shutdown();
}

/// Both lines reach the window the player reads, and squelching the combat channel stops them
/// being composed at all.
pub(super) fn both_lines_reach_the_window_and_a_squelch_stops_them() {
    use dereth_ui::ElementId;
    use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

    let mut c = a_client_in_the_world();
    c.when(attacker("Drudge Slave", dt::SLASH, 0.30, 12, 0, 0));
    c.when(defender("Drudge Slave", dt::BLUDGEON, 0.125, 7, 3, 0, 0));
    let composed = c.view().chat_lines().len() == 2;
    c.tick(1);

    let (in_the_log, trimmed, drawn) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a live screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");

        let main = screen
            .chat
            .iter()
            .find(|w| w.window_id == 8)
            .expect("the main chat window");
        let log = main.log_text();
        let in_the_log = [
            "You slash Drudge Slave for 12 points of slashing damage!",
            "Drudge Slave bashes your upper arm for 7 points of bludgeoning damage!",
        ]
        .iter()
        .all(|want| log.contains(want));
        // The window trims the newline the handler appends; the bare separator rows either side
        // of a line are a different thing.
        let trimmed = main
            .log
            .iter()
            .filter(|(_, t)| t.contains("damage!"))
            .all(|(_, t)| t.ends_with('!'));

        // And the element the player actually reads carries it.
        let root = ui
            .get_element(ElementId(0x1000_0495))
            .expect("the gameplay root");
        let chat_window = ui
            .get_child_recursive(root, window::MAIN_CHAT)
            .expect("the main chat window element");
        let log_element = ui
            .get_child_recursive(chat_window, ElementId(0x1000_0011))
            .expect("the chat scrollback");
        let text: String = ui
            .node(log_element)
            .expect("the scrollback node")
            .behaviour
            .as_ref()
            .map(|b| b.compose_text(ui.screen_box(log_element)))
            .unwrap_or_default()
            .iter()
            .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
            .collect();
        // The glyph list drops the whitespace between runs, so compare on what is printed.
        let strip = |t: &str| {
            t.chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
        let drawn = strip(&text).contains(&strip(
            "You slash Drudge Slave for 12 points of slashing damage!",
        ));
        (in_the_log, trimmed, drawn)
    };
    c.shutdown();

    // The squelch, in its own client: the gate is the first thing both handlers reach, so a
    // squelched combat channel composes nothing at all. It is the negative control for every
    // sentence above.
    let mut c = a_client_in_the_world();
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(dereth_client_model::chat::text_type::COMBAT);
    c.when(attacker("Drudge", dt::SLASH, 0.3, 12, 0, 0));
    c.when(defender("Drudge", dt::SLASH, 0.3, 12, 1, 0, 0));
    let squelched = c.view().chat_lines().is_empty()
        && c.view().hud().stats.combat_lines_squelched == 2
        && c.view().hud().stats.combat_lines == 0;

    c.assert_behaviour(
        "combat.damage-line.both-lines-reach-the-window-and-a-squelch-stops-them",
        move |_| composed && in_the_log && trimmed && drawn && squelched,
    );
    c.shutdown();
}
