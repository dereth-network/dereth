use super::*;

// =============================================================================================
// examine.*
//
// The identify window's panes.
//
// The shard here is `replay::Peer` and not a replayed login: what these claims need from a
// recording is two blobs, the shard's own create for the object and the shard's own assessment of
// it, and replaying the several thousand datagrams between them would be replaying the log-off at
// the far end as well. The bytes are the recording's either way; `Peer` is what re-stamps them for
// a session whose ordered counter starts at zero.
// =============================================================================================

/// The recordings the assessment scenarios take their objects and answers out of.
pub(super) const EXAMINE_SESSION: &str = "long-solo-play";
pub(super) const ARMOUR_SESSION: &str = "early-inventory-and-casting";

/// The character these scenarios look out of. Nothing recorded carries it: the thing the pane is
/// about is the recording's, and whoever is looking at it only has to exist.
pub(super) const LOOKER: ObjectId = ObjectId(0x5000_0001);

const ITEM_CREATE_OBJECT: u32 = 0xF745;
const ITEM_SET_APPRAISE_INFO: u32 = 0x00C9;

fn dword(s: &[u8]) -> u32 {
    u32::from_le_bytes([s[0], s[1], s[2], s[3]])
}

/// The shard's own assessment, decoded out of a recorded blob.
fn appraisal_in(b: &CorpusBlob) -> Option<dereth_protocol::objects::ItemSetAppraiseInfo> {
    use dereth_protocol::Message as _;
    let mut r = dereth_protocol::archive::Reader::new(b.payload.get(16..)?);
    dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut r).ok()
}

/// The two blobs an assessment scenario needs: the create that made `object`, and the assessment
/// the shard answered about it. **Found rather than pinned**, so a re-promoted corpus moves them
/// instead of reddening an index nobody reads.
pub(super) fn recorded_object(
    session: &str,
    object: ObjectId,
) -> (Vec<u8>, Vec<u8>, AppraisalProfile) {
    let blobs = corpus(session).blobs;
    let create = blobs
        .iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ITEM_CREATE_OBJECT
                && b.payload.get(4..8).map(dword) == Some(object.0)
        })
        .map(|b| b.payload.clone())
        .unwrap_or_else(|| panic!("{session} never creates {object:?}"));
    let (blob, profile) = blobs
        .iter()
        .filter(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == ORDERED_EVENT
                && b.payload.get(12..16).map(dword) == Some(ITEM_SET_APPRAISE_INFO)
        })
        .find_map(|b| {
            appraisal_in(b)
                .filter(|m| m.object == object)
                .map(|m| (b.payload.clone(), m.profile))
        })
        .unwrap_or_else(|| panic!("{session} carries no assessment of {object:?}"));
    (create, blob, profile)
}

/// A client with a character to look out of, and a shard to answer it.
pub(super) fn a_client_and_a_shard() -> (HeadlessClient, dereth_testkit::Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, LOOKER);
    // **The character is created by the shard, not written into the tables.** An ordered game
    // event is addressed to an object, and the client's ordered queue only has somewhere to put
    // one for an object its own object stream has been told about.
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: LOOKER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    c.world_mut().player = Some(LOOKER);
    c.world_mut()
        .weenie_mut(LOOKER)
        .expect("the shard created the character")
        .pwd
        .name = "the looker".to_owned();
    (c, peer)
}

/// Assess `object` **the way the player does**: the thing is under the pointer, and the shipped
/// identify button on the toolbar is really pressed.
///
/// It has to be the button. The panel's own examine is called by the *screen*, from the arm that
/// handles that button -- not by the request the button emits -- so a scenario that queued
/// `UiRequest::Examine` straight into the interaction layer would set the world asking and leave
/// the pane refusing every answer -- the middle link these scenarios exist to cover.
pub(super) fn examine(c: &mut HeadlessClient, object: ObjectId) {
    {
        // What a click on the thing in the viewport calls.
        let mut sink = dereth_client_model::RecordingSink::default();
        c.world_mut()
            .set_selected_object(Some(object), false, &mut sink);
    }
    c.tick(1);
    c.when(Player::click(
        dereth_ui_screens::toolbar::target_mode::EXAMINE_BUTTON,
    ));
}

/// Every glyph the item pane composed, as the player reads it.
pub(super) fn description(c: &mut HeadlessClient) -> String {
    c.ui_snapshot()
        .text_of(examination::ITEM_DISPLAY_TEXT)
        .to_owned()
}

/// What the panel is holding for the item pane, which is the same words in the client's own model.
pub(super) fn item_text(c: &mut HeadlessClient) -> String {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen
        .examination
        .item_text
        .clone()
        .expect("an item pane is up")
}

/// The recorded object, created and assessed, with the shard's own bytes for both.
pub(super) fn assessing_the_recorded(
    session: &str,
    object: ObjectId,
) -> (HeadlessClient, dereth_testkit::Peer, AppraisalProfile) {
    let (create, blob, profile) = recorded_object(session, object);
    let (mut c, mut peer) = a_client_and_a_shard();
    peer.send(&mut c, dereth_testkit::replay::OBJECT_QUEUE, create);
    c.tick(1);
    assert!(
        c.view().world().weenie(object).is_some(),
        "the recorded create is what puts {object:?} in this client's world"
    );
    examine(&mut c, object);
    peer.replay_blob(&mut c, blob);
    c.tick(2);
    (c, peer, profile)
}

/// One assessment the scenario built, for a shape no recording carries.
pub(super) fn assess(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    p: &AppraisalProfile,
) {
    examine(c, object);
    peer.event(
        c,
        &dereth_protocol::objects::ItemSetAppraiseInfo {
            object,
            profile: p.clone(),
        },
    );
    c.tick(2);
}

// ---------------------------------------------------------------------------------------------
// examine.spells.*
// ---------------------------------------------------------------------------------------------

/// The one recorded assessment in the whole corpus whose answer carries a spell book, and the two
/// strings the shipped spell table answers for its single spell.
const FOUNTAIN: ObjectId = ObjectId(0x7DA5_5046);
const REVITALIZE: u32 = 1183;
const REVITALIZE_NAME: &str = "Revitalize Other I";
const REVITALIZE_DESC: &str = "Restores 15-35 points of the target's Stamina.";

/// An assessment shaped the way the decoder produces one, with whatever spells, whole numbers and
/// fractions the scenario wants.
fn spell_profile(
    success: u32,
    ids: Option<Vec<u32>>,
    ints: &[(u32, i32)],
    floats: &[(u32, f64)],
) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = AppraisalProfile {
        success_flag: success,
        ..AppraisalProfile::default()
    };
    if let Some(ids) = ids {
        p.flags |= flags::SPELL_BOOK;
        p.spell_book = Some(ids);
    }
    if !ints.is_empty() {
        p.flags |= flags::INT;
        p.tables.ints = Some(dereth_protocol::archive::PackedHash {
            table_size: 16,
            entries: ints.to_vec(),
        });
    }
    if !floats.is_empty() {
        p.flags |= flags::FLOAT;
        p.tables.floats = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: floats.to_vec(),
        });
    }
    p
}

/// **The gate.** The recorded fountain's own assessment: the summary line names its spell, and the
/// paragraph under it describes that spell out of the shipped table.
pub(super) fn an_assessed_item_names_its_spells_and_describes_them() {
    let (mut c, _peer, profile) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);
    // The premise, off the recorded answer itself: this is the corpus's one spell book.
    assert_eq!(
        profile.spell_book.as_deref(),
        Some(&[REVITALIZE][..]),
        "the recorded assessment of the fountain carries one spell"
    );

    let text = description(&mut c);
    let short = text.find("Spells: ");
    let long = text.find("Spell Descriptions:");
    let names = text.contains(&format!("Spells: {REVITALIZE_NAME}"));
    let describes = text.contains(&format!(
        "Spell Descriptions:\n~ {REVITALIZE_NAME}: {REVITALIZE_DESC}"
    ));
    // Each of the two starts a paragraph of its own.
    let paragraphs = text.contains(&format!("\n\nSpells: {REVITALIZE_NAME}"))
        && text.contains("\n\nSpell Descriptions:");
    // The answer carries none of the mana keys and no fractions at all, so those lines are absent
    // -- which is as much the client's doing as the two above.
    let quiet = ["Spellcraft:", "Mana:", "Mana Cost:", "Enchantments:"]
        .iter()
        .all(|w| !text.contains(w));

    c.assert_behaviour(
        "examine.spells.an-item-with-spells-on-it-names-them-and-describes-them",
        move |_| names && describes && paragraphs && quiet && short.is_some() && short < long,
    );
    c.shutdown();
}

/// A spell somebody cast on the thing is listed apart from the spells it was made with: under its
/// own heading, and out of the summary line altogether.
///
/// **No recording carries one.** Not one of the corpus's assessments names a spell on that side of
/// the split, so the answer below is built; the spell, its name and its description are still the
/// shipped table's, and everything from the datagram on is the client's own.
pub(super) fn an_enchantment_is_listed_apart_from_the_items_own_spells() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);
    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(1, Some(vec![0x8000_0000 | REVITALIZE]), &[(0x13, 200)], &[]),
    );

    let text = description(&mut c);
    // The blank line under this heading is the client's and the other heading has none: the two
    // really do disagree, and it is the first thing a tidy-up would lose.
    let under_its_own_heading = text.contains(&format!(
        "Enchantments:\n\n~ {REVITALIZE_NAME}: {REVITALIZE_DESC}"
    ));
    let not_in_the_summary = !text.contains("Spells: ");
    let not_in_the_other_paragraph = !text.contains("Spell Descriptions:");

    c.assert_behaviour(
        "examine.spells.an-enchantment-is-listed-apart-from-the-spells-the-item-was-made-with",
        move |_| under_its_own_heading && not_in_the_summary && not_in_the_other_paragraph,
    );
    c.shutdown();
}

/// The three magic lines, in the client's own order, and which of the two prices wins.
///
/// A thing that spends mana over time says so per so many seconds and the flat price is never
/// reached; one with no rate gives the flat price and the note that a skill reduces it.
pub(super) fn the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(
            1,
            Some(vec![REVITALIZE]),
            &[
                (0x13, 200),
                (0x6A, 250),
                (0x6B, 700),
                (0x6C, 800),
                (0x75, 5),
            ],
            &[(5, -0.05)],
        ),
    );
    let text = description(&mut c);
    let at = |s: &str| text.find(s);
    let by_the_rate = [
        "Spellcraft: 250.",
        "Mana: 700 / 800.",
        "Mana Cost: 1 point per 20 seconds.",
    ]
    .iter()
    .all(|l| text.contains(l))
        && !text.contains("Mana Cost: 5.")
        && !text.contains("Mana Conversion")
        && at("Spellcraft: 250.").is_some()
        && at("Spellcraft: 250.") < at("Mana: 700 / 800.")
        && at("Mana: 700 / 800.") < at("Mana Cost: 1 point per 20 seconds.")
        && at("Mana Cost: 1 point per 20 seconds.") < at("Spell Descriptions:");

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(1, Some(vec![REVITALIZE]), &[(0x13, 200), (0x75, 5)], &[]),
    );
    let text = description(&mut c);
    let flat = text.contains("Mana Cost: 5.\n(Can be reduced by the Mana Conversion skill)");

    c.assert_behaviour(
        "examine.spells.the-magic-lines-come-in-the-clients-own-order-and-a-rate-beats-a-flat-cost",
        move |_| by_the_rate && flat,
    );
    c.shutdown();
}

/// An assessment the character was not up to says the spells are unknown -- **twice**, because
/// both blocks say it, which is the client's own and not a transcription to be tidied -- and a
/// thing with no spells at all draws neither heading, whether the assessment succeeded or not.
pub(super) fn an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, FOUNTAIN);

    assess(
        &mut c,
        &mut peer,
        FOUNTAIN,
        &spell_profile(0, Some(vec![REVITALIZE]), &[], &[]),
    );
    let text = description(&mut c);
    let unknown = text.matches("Spells: unknown.").count() == 2 && !text.contains(REVITALIZE_NAME);

    let mut silent = true;
    for success in [0_u32, 1] {
        assess(
            &mut c,
            &mut peer,
            FOUNTAIN,
            &spell_profile(success, None, &[(0x13, 200)], &[]),
        );
        let text = description(&mut c);
        silent &= ["Spells:", "Spell Descriptions:", "Enchantments:"]
            .iter()
            .all(|w| !text.contains(w));
    }

    c.assert_behaviour(
        "examine.spells.an-unsuccessful-assessment-says-so-and-a-plain-item-says-nothing",
        move |_| unknown && silent,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// examine.lifespan.*
// ---------------------------------------------------------------------------------------------

/// The recorded potion the lifespan scenario hangs its built answers on.
pub(super) const POTION: ObjectId = ObjectId(0x8000_09B5);
const LIFESPAN: u32 = 0x10B;
const CREATION_TIMESTAMP: u32 = 0x62;
const REMAINING_LIFESPAN: u32 = 0x10C;

/// An assessment carrying only whole numbers.
fn int_profile(entries: &[(u32, i32)]) -> AppraisalProfile {
    AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(dereth_protocol::archive::PackedHash {
                table_size: 8,
                entries: entries.to_vec(),
            }),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    }
}

/// The line that says when the thing runs out, if there is one.
fn expiry(text: &str) -> Option<&str> {
    text.lines().find(|line| {
        line.starts_with("This item expires in ")
            || *line == "This item is in the act of disintegrating."
    })
}

/// **The gate.** How long is left is spelled out in years, days, hours, minutes and seconds; a
/// thing already past its time says it is disintegrating; and an answer missing any one of the
/// three numbers the line is built from draws no line at all.
pub(super) fn an_item_that_expires_says_when_and_needs_all_three_numbers() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    let mut spelled_out = true;
    for (lifespan, creation, remaining, want) in [
        (0, 0, 0, "This item expires in 0 seconds."),
        (-1, -1, 60, "This item expires in 60 seconds."),
        (1, 2, 61, "This item expires in 1 minutes, 1 seconds."),
        (1, 2, 3_600, "This item expires in 60 minutes, 0 seconds."),
        (1, 2, 86_400, "This item expires in 24 hours, 0 seconds."),
        (
            1,
            2,
            31_536_000,
            "This item expires in 365 days, 0 seconds.",
        ),
        (
            1,
            2,
            31_626_061,
            "This item expires in 1 years, 1 days, 1 hours, 1 minutes, 1 seconds.",
        ),
        (1, 2, -1, "This item is in the act of disintegrating."),
    ] {
        assess(
            &mut c,
            &mut peer,
            POTION,
            &int_profile(&[
                (LIFESPAN, lifespan),
                (CREATION_TIMESTAMP, creation),
                (REMAINING_LIFESPAN, remaining),
            ]),
        );
        let text = description(&mut c);
        spelled_out &= expiry(&text) == Some(want);
    }

    // The first two numbers are gates and not sources: a zero in either still draws the line, and
    // a missing one of the three draws nothing at all.
    let all = [
        (LIFESPAN, 99),
        (CREATION_TIMESTAMP, 100),
        (REMAINING_LIFESPAN, 101),
    ];
    let mut silent = true;
    for missing in [LIFESPAN, CREATION_TIMESTAMP, REMAINING_LIFESPAN] {
        let entries: Vec<(u32, i32)> = all
            .iter()
            .copied()
            .filter(|(key, _)| *key != missing)
            .collect();
        assess(&mut c, &mut peer, POTION, &int_profile(&entries));
        let text = description(&mut c);
        silent &= expiry(&text).is_none();
    }

    c.assert_behaviour(
        "examine.lifespan.an-item-that-expires-says-when-and-needs-all-three-of-its-numbers",
        move |_| spelled_out && silent,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// examine.creature.* / examine.armour.* / examine.weapon.* / examine.inscription.*
// ---------------------------------------------------------------------------------------------

/// The recorded golem, the recorded gauntlets, and the recorded thing the built answers hang on.
pub(super) const GOLEM: ObjectId = ObjectId(0x8000_09D9);
const GAUNTLETS: ObjectId = ObjectId(0x8000_0674);
const SUBJECT: ObjectId = ObjectId(0x8000_0997);

/// The nine rows of the creature pane, as the panel holds them.
pub(super) fn creature_rows(c: &mut HeadlessClient) -> Vec<(String, String)> {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.creature_row_text.clone()
}

/// **The gate.** The recorded golem: the pane names what kind of creature it is, its level, its six
/// attributes and its three vitals -- with the percentage on health alone.
pub(super) fn the_creature_pane_names_the_kind_the_level_and_the_nine_rows() {
    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, GOLEM);

    // The premise, off the recorded answer: a creature, at part health.
    let body = profile
        .creature_profile
        .expect("the recorded answer carries a creature");
    assert_eq!(
        (body.health, body.max_health),
        (12, 31),
        "the recorded golem's health"
    );

    let (title, kind, level, drawn, active) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        let p = &screen.examination;
        (
            p.title_text.clone(),
            p.creature_name_text.clone(),
            p.level_text.clone(),
            p.creature_rows_drawn,
            p.active,
        )
    };
    let want: Vec<(String, String)> = [
        ("Strength", "1"),
        ("Endurance", "1"),
        ("Coordination", "1"),
        ("Quickness", "1"),
        ("Focus", "1"),
        ("Self", "1"),
        ("Health", "12/31 (39 %)"),
        ("Stamina", "51/51"),
        ("Mana", "1/1"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
    .collect();
    let recorded_reads = title.as_deref() == Some("Sparring Golem")
        && kind.as_deref() == Some("Golem")
        && level.as_deref() == Some("1")
        && drawn == 9
        && active == Some(examination::ExamineSubUi::Creature)
        && creature_rows(&mut c) == want;

    // The same body at full health -- the percentage is on health and on neither of the others.
    let mut full = profile.clone();
    if let Some(b) = full.creature_profile.as_mut() {
        b.health = b.max_health;
    }
    assess(&mut c, &mut peer, GOLEM, &full);
    let rows = creature_rows(&mut c);
    let at_full = rows[6] == ("Health".to_owned(), "31/31 (100 %)".to_owned())
        && rows[7] == ("Stamina".to_owned(), "51/51".to_owned())
        && rows[8] == ("Mana".to_owned(), "1/1".to_owned());

    c.assert_behaviour(
        "examine.creature.the-pane-names-the-kind-the-level-six-attributes-and-three-vitals",
        move |_| recorded_reads && at_full,
    );
    c.shutdown();
}

/// The recorded gauntlets: the armour level, then the eight kinds of harm as a word and a number
/// each, in the order the pane draws them rather than the order the answer packs them in.
pub(super) fn the_armour_pane_gives_its_level_and_eight_resistances() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, GAUNTLETS);
    let a = profile
        .armor_profile
        .expect("the recorded answer carries armour");
    assert!(
        (a.mod_vs_pierce - 0.8).abs() < 1e-6,
        "the recorded piercing modifier"
    );
    assert!(
        (a.mod_vs_acid - 0.3).abs() < 1e-6,
        "the recorded acid modifier"
    );

    let text = item_text(&mut c);
    let want = "Value: 0\n\
                Burden: 270\n\
                \n\
                \n\
                Armor Level: 20\n\
                Slashing: Average  (20)\n\
                Piercing: Below Average  (16)\n\
                Bludgeoning: Average  (20)\n\
                Fire: Below Average  (10)\n\
                Cold: Below Average  (10)\n\
                Acid: Poor  (6)\n\
                Electric: Below Average  (12)\n\
                Nether: Average  (20)\n";
    let reads = text == want;

    c.assert_behaviour(
        "examine.armour.the-pane-gives-its-level-and-eight-resistances-in-the-order-it-draws-them",
        move |_| reads,
    );
    c.shutdown();
}

/// A bow as a retail screenshot shows it. **No recorded assessment carries a weapon at all**, so
/// this is built from that screenshot; the thing it is about is the recording's, and everything
/// from the datagram onwards is the client's.
fn a_bow() -> dereth_protocol::types::appraisal::WeaponProfile {
    dereth_protocol::types::appraisal::WeaponProfile {
        damage_type: 2,
        weapon_time: 40,
        weapon_skill: 47,
        weapon_damage: 0,
        damage_variance: 0.0,
        damage_mod: 1.0,
        weapon_length: 0.0,
        max_velocity: 22.5,
        weapon_offense: 1.0,
        max_velocity_estimated: 0,
    }
}

/// The weapon pane: the skill, the damage, the speed as a word and a number, the reach and what it
/// shoots -- and a modifier of exactly none prints as plus nothing rather than as a blank.
pub(super) fn the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition() {
    let (mut c, mut peer, recorded) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    assert!(
        recorded.weapon_profile.is_none(),
        "the premise: no recorded assessment carries a weapon, which is why this one is built"
    );
    {
        // The two fields of the live object the block branches on: a launcher, and arrows.
        let w = c
            .world_mut()
            .weenie_mut(SUBJECT)
            .expect("the recorded create made it");
        w.pwd.valid_locations = Some(0x0040_0000);
        w.pwd.ammo_type = Some(1);
    }

    let mut profile = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT
            | dereth_protocol::types::appraisal::flags::STRING
            | dereth_protocol::types::appraisal::flags::WEAPON_PROFILE,
        success_flag: 1,
        weapon_profile: Some(a_bow()),
        ..AppraisalProfile::default()
    };
    profile.tables.ints = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x13, 25), (5, 400), (0x161, 8)],
    });
    profile.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![
            (
                0x0E,
                "Use Oil of Rendering on this weapon to create an Academy Shortbow.".to_owned(),
            ),
            (
                0x10,
                "A shortbow used by the students of the Academy.".to_owned(),
            ),
        ],
    });
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let text = item_text(&mut c);
    // The whole pane, and not six lines of it. Two of the blank lines are the client's own -- two
    // blocks open a paragraph before they have looked at anything -- and the line about what it
    // holds is read off the **object** and not the assessment, which is the recording's doing:
    // the thing this answer is carried on is a container the recording created.
    let reads = text
        == [
            "Value: 25",
            "Burden: 400",
            "",
            "Skill: Missile Weapons (Bow)",
            "Damage Bonus: 0",
            "Damage Modifier: +0%.",
            "Speed: Average (40)",
            "Range: 55 yds.",
            "Uses arrows as ammunition.",
            "",
            "",
            "Use Oil of Rendering on this weapon to create an Academy Shortbow.",
            "",
            "Can hold up to 24 items.",
            "",
            "A shortbow used by the students of the Academy.",
        ]
        .join("\n");

    c.assert_behaviour(
        "examine.weapon.the-pane-gives-the-skill-the-damage-the-speed-the-range-and-the-ammunition",
        move |_| reads,
    );
    c.shutdown();
}

/// A thing on somebody else's hook: the client has never been sent the object itself, so where it
/// is worn comes out of the assessment. Without that the weapon lines are absent although the
/// weapon numbers are there; with it, and nothing about the live object having changed, they
/// appear.
pub(super) fn an_item_on_someone_elses_hook_takes_its_slot_from_the_reply() {
    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    {
        // The live object knows nothing: this is a thing in somebody else's house.
        let w = c
            .world_mut()
            .weenie_mut(SUBJECT)
            .expect("the recorded create made it");
        w.pwd.valid_locations = Some(0);
        w.pwd.ammo_type = Some(0);
    }

    let base = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::WEAPON_PROFILE,
        success_flag: 1,
        weapon_profile: Some(a_bow()),
        ..AppraisalProfile::default()
    };
    assess(&mut c, &mut peer, SUBJECT, &base);
    let plain = item_text(&mut c);
    let absent = !plain.contains("Skill:") && !plain.contains("Range:");

    let hooked = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::WEAPON_PROFILE
            | dereth_protocol::types::appraisal::flags::HOOK_PROFILE,
        hook_profile: Some(dereth_protocol::types::appraisal::HookAppraisalProfile {
            bitfield: 0,
            valid_locations: 0x0040_0000,
            ammo_type: 1,
        }),
        ..base
    };
    assess(&mut c, &mut peer, SUBJECT, &hooked);
    let on_a_hook = item_text(&mut c);
    let present = [
        "Skill: Missile Weapons",
        "Damage Modifier: +0%.",
        "Speed: Average (40)",
        "Range: 55 yds.",
        "Uses arrows as ammunition.",
    ]
    .iter()
    .all(|l| on_a_hook.contains(l));

    c.assert_behaviour(
        "examine.weapon.an-item-on-someone-elses-hook-takes-its-slot-from-the-reply",
        move |_| absent && present,
    );
    c.shutdown();
}

/// The place to write on a thing is there only on a thing that can be written on, and on one
/// nobody has signed it invites the player to. On anything else there is no box, not a blank one.
pub(super) fn the_inscribe_box_appears_only_on_something_inscribable() {
    /// The one bit of the object's own description that decides it.
    const INSCRIBABLE: u32 = 0x0000_0002;

    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, SUBJECT);
    let recorded_bit = c
        .view()
        .world()
        .weenie(SUBJECT)
        .is_some_and(|w| w.pwd.bitfield & INSCRIBABLE != 0);
    assert!(
        recorded_bit,
        "the premise: the recorded object can be written on"
    );
    let invited = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.inscription.clone()
    };

    // Cleared: the box is taken away, not blanked.
    c.world_mut()
        .weenie_mut(SUBJECT)
        .expect("the recorded create made it")
        .pwd
        .bitfield &= !INSCRIBABLE;
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let gone = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.inscription.clone()
    };

    // And back.
    c.world_mut()
        .weenie_mut(SUBJECT)
        .expect("the recorded create made it")
        .pwd
        .bitfield |= INSCRIBABLE;
    assess(&mut c, &mut peer, SUBJECT, &profile);
    let (again, signature) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.inscription.clone(),
            screen.examination.inscription_signature_text.clone(),
        )
    };

    // Written on and signed: the box holds what was written, and the line under it names who
    // wrote it.
    let mut written = profile.clone();
    written.flags |= dereth_protocol::types::appraisal::flags::STRING;
    written.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(7, "For Aldwyne.".to_owned()), (8, "Lark".to_owned())],
    });
    assess(&mut c, &mut peer, SUBJECT, &written);
    let (words, by) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.inscription.clone(),
            screen.examination.inscription_signature_text.clone(),
        )
    };

    c.assert_behaviour(
        "examine.inscription.the-box-is-there-only-on-something-that-can-be-inscribed",
        move |_| {
            invited.as_deref() == Some("<Inscribe here>")
                && gone.is_none()
                && again.as_deref() == Some("<Inscribe here>")
                && signature.is_empty()
                && words.as_deref() == Some("For Aldwyne.")
                && by == "--Lark"
        },
    );
    c.shutdown();
}

// =============================================================================================
// examine.key.*
//
// **Every scenario here presses the key at least twice.** The two halves of the arm differ only
// in what happens when the pane is already open, so a scenario that pressed once would agree with
// a build that could only ever open it.
//
// `Player` has no step for a bound action, so the press below goes into the client's own input
// manager.
// =============================================================================================

/// The thing the key assesses. Seeded rather than replayed: nothing here is about *which* thing,
/// and a corpus replay for it would cost a minute a scenario.
const KEYED: ObjectId = ObjectId(0xC000_0011);

/// Is `<EXAM>` up?
fn pane_is_up(c: &mut HeadlessClient) -> bool {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    let h = ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("the assessment window is in the shipped layout");
    ui.node(h).expect("a live node").region.flags.visible
}

/// How many times the pane has been hidden, by any of its close paths, and how many times the
/// arriving answer has shown it.
fn shown_and_hidden(c: &mut HeadlessClient) -> (u32, u32) {
    let (_, screen) = gameplay_screen(c.app_mut());
    (screen.examination.opened, screen.examination.closed)
}

/// How many times the key's **close-first** leg has run, and how many times the client has asked
/// the shard about something. The second moves only on the other leg, which is what says the
/// first one sent nothing.
fn closes_and_asks(c: &mut HeadlessClient) -> (u64, u64) {
    let closes = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .examine_panel_closes;
    let asks = c.world_mut().appraisal.examine_serial;
    (closes, asks)
}

/// A thing in the world for the key to be about.
fn a_thing_to_look_at(c: &mut HeadlessClient) {
    let w = c.world_mut();
    if w.weenie(KEYED).is_none() {
        w.tables
            .weenies
            .insert(KEYED, dereth_client_model::Weenie::new(KEYED));
    }
    if let Some(m) = w.tables.weenies.get_mut(KEYED) {
        m.pwd.name = "Sparring Golem".into();
    }
}

/// One press of the assess key, through the client's own input manager and one whole frame.
fn press_the_key(c: &mut HeadlessClient) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(dereth_client::interaction::action::SELECTION_EXAMINE),
        input_map: dereth_client::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start: true,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    c.app_mut()
        .input_manager_mut()
        .expect("the input manager is part of the shell this scenario asked for")
        .inject_action(e);
    c.tick(1);
}

/// The shard's answer for `KEYED`, which is what actually shows the pane.
fn answer_about_it(c: &mut HeadlessClient) {
    let mut sink = dereth_client_model::RecordingSink::default();
    c.world_mut()
        .set_appraise_info(KEYED, AppraisalProfile::default(), &mut sink);
    c.tick(1);
}

/// Put the thing under the pointer.
fn look_at_it(c: &mut HeadlessClient) {
    let mut sink = dereth_client_model::RecordingSink::default();
    c.world_mut()
        .set_selected_object(Some(KEYED), false, &mut sink);
    c.tick(1);
}

/// Open the pane the way a player does: look at something, press the key, let the answer land.
///
/// Returns with the pane **up**, asserted -- the premise every closing claim below rests on. A
/// comparison that never established the pane was open would be satisfied by one that never
/// opened.
fn a_pane_that_is_open() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(3));
    a_thing_to_look_at(&mut c);
    look_at_it(&mut c);
    assert!(!pane_is_up(&mut c), "the premise: the pane starts shut");

    let (_, asked) = closes_and_asks(&mut c);
    press_the_key(&mut c);
    let (closed, asked_now) = closes_and_asks(&mut c);
    assert_eq!(
        asked_now,
        asked + 1,
        "a press on a shut pane asks the shard"
    );
    assert_eq!(closed, 0, "and does not take the closing leg");
    assert!(
        !pane_is_up(&mut c),
        "nor does asking open the window by itself"
    );

    c.tick(1);
    answer_about_it(&mut c);
    c.tick(1);
    assert!(
        pane_is_up(&mut c),
        "the answer for the thing the pane is waiting on shows it"
    );
    assert_eq!(shown_and_hidden(&mut c).0, 1, "and exactly once");
    c
}

/// **The gate.** The key is a toggle: a press on a shut pane asks the shard and the answer opens
/// it, a press on the open pane shuts it and asks nothing, and a third press asks again -- so
/// "it closed" and "it stopped working" are different answers.
pub(super) fn the_assess_key_shuts_an_open_pane_and_opens_a_shut_one() {
    let mut c = a_pane_that_is_open();
    let (shown_before, hidden_before) = shown_and_hidden(&mut c);
    let (_, asked_before) = closes_and_asks(&mut c);

    press_the_key(&mut c);
    let shut = !pane_is_up(&mut c);
    let (closes, asked) = closes_and_asks(&mut c);
    let (shown, hidden) = shown_and_hidden(&mut c);
    // Two claims and not one: a build that shut the pane *and* asked again would look the same
    // for one frame and would pop the window straight back open on the next answer.
    let shut_without_asking = shut
        && closes == 1
        && hidden == hidden_before + 1
        && asked == asked_before
        && shown == shown_before;
    // And the pane is still waiting on the same thing, so a re-poll refills rather than shows.
    let still_waiting = c.world_mut().appraisal.examining == Some(KEYED);

    press_the_key(&mut c);
    let (closes_again, asked_again) = closes_and_asks(&mut c);
    let asked_a_second_time = asked_again == asked + 1 && closes_again == closes;

    c.tick(1);
    answer_about_it(&mut c);
    c.tick(1);
    let open_again = pane_is_up(&mut c) && shown_and_hidden(&mut c).0 == 2;

    c.assert_behaviour(
        "examine.key.the-assess-key-shuts-an-open-pane-and-opens-a-shut-one",
        move |_| shut_without_asking && still_waiting && asked_a_second_time && open_again,
    );
    c.shutdown();
}

/// The shutting happens before the key looks at what is under the pointer, so it works with
/// nothing under it; and with nothing under the pointer and the pane already shut the press does
/// nothing at all rather than opening an empty one.
pub(super) fn the_assess_key_shuts_the_pane_with_nothing_under_the_pointer() {
    let mut c = a_pane_that_is_open();

    // Nothing under the pointer, and the pane put back up by hand: dropping the selection is the
    // pane's *own* second closing path, which would otherwise be the thing being measured.
    c.world_mut().selected = None;
    c.tick(1);
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, examination::WINDOW)
            .expect("the assessment window is in the shipped layout");
        ui.set_visible(h, true);
    }
    assert!(
        pane_is_up(&mut c),
        "the premise: the pane is open and nothing is under the pointer"
    );
    let (closes_before, asked_before) = closes_and_asks(&mut c);

    press_the_key(&mut c);
    let (closes, asked) = closes_and_asks(&mut c);
    let shut_anyway = !pane_is_up(&mut c) && closes == closes_before + 1 && asked == asked_before;

    // The mirror: shut, and nothing under the pointer. The press is inert -- it neither asks nor
    // closes, which is a different reading from "the arm never ran".
    let mut c2 = HeadlessClient::new(ClientSpec::gameplay_in_world(3));
    assert!(!pane_is_up(&mut c2), "the premise: the pane starts shut");
    assert_eq!(
        c2.world_mut().selected,
        None,
        "and nothing is under the pointer"
    );
    press_the_key(&mut c2);
    let (closes2, asked2) = closes_and_asks(&mut c2);
    let inert = !pane_is_up(&mut c2) && closes2 == 0 && asked2 == 0;
    c2.shutdown();

    c.assert_behaviour(
        "examine.key.the-pane-shuts-with-nothing-under-the-pointer-and-a-shut-one-does-nothing",
        move |_| shut_anyway && inert,
    );
    c.shutdown();
}

// =============================================================================================
// examine.consumables.* / examine.capacity.* / examine.item-blocks.* / examine.character.*
//
// The four the corpus answers for are driven off recorded answers; the rest are built, because
// no recording carries them -- and each of them goes in as a datagram and comes out as the words
// in the live pane, so a block that is written and never wired fails here even though its own
// unit test passes.
// =============================================================================================

/// The recorded container every built answer in this family is carried on. It has to be something
/// the recording also created: the pane refuses an answer about a thing it has never heard of.
const CARRIER: ObjectId = SUBJECT;

/// A table of whole numbers, which is also the shape a table of flags takes.
fn ints(pairs: &[(u32, i32)]) -> dereth_protocol::archive::PackedHash<u32, i32> {
    dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries: pairs.to_vec(),
    }
}

/// An answer carrying whole numbers and nothing else.
fn plain(pairs: &[(u32, i32)]) -> AppraisalProfile {
    AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(ints(pairs)),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    }
}

/// Assess `CARRIER` with `p` and read the pane back.
fn shows(c: &mut HeadlessClient, peer: &mut dereth_testkit::Peer, p: &AppraisalProfile) -> String {
    assess(c, peer, CARRIER, p);
    item_text(c)
}

/// One whole number out of a recorded answer.
fn recorded_int(p: &AppraisalProfile, key: u32) -> Option<i32> {
    p.tables
        .ints
        .as_ref()
        .and_then(|t| t.entries.iter().find(|(i, _)| *i == key).map(|(_, v)| *v))
}

/// **The gate.** The two recorded potions say what they restore and that they cannot be sold; a
/// healing kit made out of the same answer says what it adds to the skill instead, and both say
/// how many uses are left.
pub(super) fn a_potion_says_what_it_restores_and_a_kit_what_it_adds() {
    // The recorded potion, whose two lines are the recording's own.
    let stamina = {
        let (mut c, _peer, profile) = assessing_the_recorded(EXAMINE_SESSION, POTION);
        // The recorded body, read first, so a corpus change is a failure here and not a silence.
        assert_eq!(
            recorded_int(&profile, 0x59),
            Some(4),
            "the recorded potion's kind"
        );
        assert_eq!(recorded_int(&profile, 0x5A), Some(5), "and how much of it");
        assert_eq!(
            profile.tables.bools.as_ref().and_then(|t| t
                .entries
                .iter()
                .find(|(i, _)| *i == 0x45)
                .map(|(_, v)| *v)),
            Some(0),
            "and that it cannot be sold"
        );
        let text = item_text(&mut c);
        let ok = text.contains("Restores 5 Stamina when consumed.")
            && text.contains("This item cannot be sold.");
        c.shutdown();
        ok
    };

    // The other potion of the same recording, whose kind is a different one.
    let mana = {
        let (mut c, _peer, _) = assessing_the_recorded(EXAMINE_SESSION, ObjectId(0x8000_09B4));
        let ok = item_text(&mut c).contains("Restores 5 Mana when used.");
        c.shutdown();
        ok
    };

    // One built answer, and the one bit of the object that decides which block owns the number:
    // clear, it is a potion and the number is what it restores; set, it is a kit and the number is
    // what it adds to the skill.
    const A_HEALER: u32 = 0x0001_0000;
    let (mut c2, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let built = AppraisalProfile {
        flags: dereth_protocol::types::appraisal::flags::INT
            | dereth_protocol::types::appraisal::flags::FLOAT,
        success_flag: 1,
        tables: dereth_protocol::types::PropertyTables {
            ints: Some(ints(&[
                (0x13, 20),
                (5, 50),
                (0x5A, 5),
                (0x59, 2),
                (0x5C, 4),
            ])),
            floats: Some(dereth_protocol::archive::PackedHash {
                table_size: 8,
                entries: vec![(0x64, 1.15)],
            }),
            ..dereth_protocol::types::PropertyTables::default()
        },
        ..AppraisalProfile::default()
    };
    let text = shows(&mut c2, &mut peer, &built);
    let as_a_potion = text.contains("Restores 5 Health when used.")
        && !text.contains("Bonus to Healing Skill")
        && text.contains("Number of uses remaining: 4");

    c2.world_mut()
        .weenie_mut(CARRIER)
        .expect("the recorded create made it")
        .pwd
        .bitfield |= A_HEALER;
    let text = shows(&mut c2, &mut peer, &built);
    // **114, not 115.** The fraction is not representable and the client truncates rather than
    // rounds, so asserting 115 would be asserting an arithmetic the client does not do.
    let as_a_kit = text.contains("Bonus to Healing Skill: 5")
        && text.contains("Restoration Bonus: 114%")
        && !text.contains("Restores 5 Health when used.");

    let mut unknown = built.clone();
    unknown.success_flag = 0;
    unknown.tables.ints = Some(ints(&[(0x13, 20), (5, 50), (0x5A, 5), (0x59, 2)]));
    let text = shows(&mut c2, &mut peer, &unknown);
    // Two spaces, which is the client's own and not slack.
    let uses_unknown = text.contains("Number of uses remaining:  Unknown");

    c2.assert_behaviour(
        "examine.consumables.a-potion-says-what-it-restores-and-a-kit-what-it-adds",
        move |_| stamina && mana && as_a_potion && as_a_kit && uses_unknown,
    );
    c2.shutdown();
}

/// A container says how much it holds and a book how many of its pages are used -- and the pages
/// are given used-first, which a symmetric subject could not tell from the other way round.
pub(super) fn a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used() {
    // The recorded book, and then the same book with the two numbers made different.
    let book = ObjectId(0x8000_0A76);
    let (mut c, mut peer, mut profile) = assessing_the_recorded(EXAMINE_SESSION, book);
    assert_eq!(
        recorded_int(&profile, 0xAE),
        Some(12),
        "the recorded book's used pages"
    );
    assert_eq!(
        recorded_int(&profile, 0xAF),
        Some(12),
        "and how many it has"
    );
    let full = item_text(&mut c).contains("12 of 12 pages full.");
    if let Some(t) = profile.tables.ints.as_mut() {
        for e in &mut t.entries {
            if e.0 == 0xAE {
                e.1 = 3;
            }
        }
    }
    assess(&mut c, &mut peer, book, &profile);
    let asymmetric = item_text(&mut c).contains("3 of 12 pages full.");
    c.shutdown();

    // The container: what it holds is read off the **object** and never off the answer, so no
    // recording carries it and every arm below is the client reading its own world.
    let (mut c, mut peer, profile) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let (items, containers) = {
        let w = c
            .view()
            .world()
            .weenie(CARRIER)
            .expect("the recorded create made it");
        (
            i32::from(w.pwd.items_capacity.unwrap_or(0)),
            i32::from(w.pwd.containers_capacity.unwrap_or(0)),
        )
    };
    assert!(
        items > 0 || containers > 0,
        "the premise: the recorded thing really is a container -- got {items} and {containers}"
    );
    let want = if items > 0 && containers > 0 {
        format!("Can hold up to {items} items and {containers} containers.")
    } else if items > 0 {
        format!("Can hold up to {items} items.")
    } else {
        format!("Can hold up to {containers} containers.")
    };
    let recorded_capacity = item_text(&mut c).contains(&want);

    {
        let w = c
            .world_mut()
            .weenie_mut(CARRIER)
            .expect("the recorded create made it");
        w.pwd.items_capacity = Some(0);
        w.pwd.containers_capacity = Some(0);
    }
    let holds_nothing = !shows(&mut c, &mut peer, &profile).contains("Can hold up to");

    let mut three_arms = true;
    for (i, n, want) in [
        (24_u8, 1_u8, "Can hold up to 24 items and 1 containers."),
        (24, 0, "Can hold up to 24 items."),
        (0, 7, "Can hold up to 7 containers."),
    ] {
        {
            let w = c
                .world_mut()
                .weenie_mut(CARRIER)
                .expect("the recorded create made it");
            w.pwd.items_capacity = Some(i);
            w.pwd.containers_capacity = Some(n);
        }
        three_arms &= shows(&mut c, &mut peer, &profile).contains(want);
    }

    c.assert_behaviour(
        "examine.capacity.a-container-says-how-much-it-holds-and-a-book-how-many-pages-are-used",
        move |_| full && asymmetric && recorded_capacity && holds_nothing && three_arms,
    );
    c.shutdown();
}

/// **Every line the item pane can draw, through the shard.** No recording carries any of these, so
/// each answer is built -- but each goes in as a datagram and comes out as the words the player
/// reads, which is what tells a block that is written and wired from one that is only written.
///
/// The list is deliberately long and flat: one entry is one sentence the pane must be able to
/// produce, and the list itself is the claim.
pub(super) fn every_line_the_item_pane_can_draw_reaches_it_through_the_shard() {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, _) = assessing_the_recorded(EXAMINE_SESSION, CARRIER);
    let mut ok = true;
    let mut says = |c: &mut HeadlessClient,
                    peer: &mut dereth_testkit::Peer,
                    p: &AppraisalProfile,
                    want: &[&str],
                    absent: &[&str]| {
        let text = shows(c, peer, p);
        for w in want {
            if !text.contains(w) {
                eprintln!("the pane never said {w:?}; it said:\n{text}");
                ok = false;
            }
        }
        for w in absent {
            if text.contains(w) {
                eprintln!("the pane said {w:?} and should not have:\n{text}");
                ok = false;
            }
        }
    };

    // How often it has been worked on, and how well it was made.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xAB, 1)]),
        &["This item has been tinkered 1 time."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xAB, 4)]),
        &["This item has been tinkered 4 times."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 3)]),
        &["Workmanship: Finely crafted (3)"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 10)]),
        &["Workmanship: Priceless (10)"],
        &[],
    );
    // The salvaged arm: the adjective comes off the average and not off the raw number.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x69, 7), (0xAA, 2)]),
        &["Workmanship: Exquisitely crafted (3.50)\n\nSalvaged from 2 items."],
        &[],
    );
    let mut worked = plain(&[(0xAB, 2)]);
    worked.flags |= flags::STRING;
    worked.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x27, "Bael".to_owned()), (0x28, "Asheron".to_owned())],
    });
    says(
        &mut c,
        &mut peer,
        &worked,
        &["Last tinkered by Bael.", "Imbued by Asheron."],
        &[],
    );

    // The set it belongs to, and an id that names no set at all.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x109, 0x0D)]),
        &["Set: Soldier's"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x109, 0x82)]),
        &["Set: Shimmering Shadows"],
        &[],
    );
    says(&mut c, &mut peer, &plain(&[(0x109, 0x22)]), &[], &["Set: "]);

    // The ratings, whose drawn order is not the order they are read in, and a rating of none
    // which is not a term at all.
    says(
        &mut c,
        &mut peer,
        &plain(&[
            (0x172, 1),
            (0x173, 2),
            (0x174, 3),
            (0x175, 4),
            (0x176, 5),
            (0x177, 6),
            (0x178, 7),
            (0x179, 8),
            (0x17A, 9),
            (0x17B, 10),
        ]),
        &[
            "Ratings: Dam 1, Dam Resist 2, Crit 3, Crit Dam 5, Crit Resist 4, Crit Dam Resist 6, \
             Heal Boost 7, Nether Resist 8, Life Resist 9.",
            "This item adds 10 Vitality.",
        ],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x172, 0), (0x174, 3)]),
        &["Ratings: Crit 3."],
        &[],
    );

    // What it does for defence -- and exactly none is not a line.
    let mut defence = plain(&[]);
    defence.flags |= flags::FLOAT;
    defence.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x1D, 1.08), (0x95, 1.1), (0x96, 1.0)],
    });
    says(
        &mut c,
        &mut peer,
        &defence,
        &[
            "Bonus to Melee Defense: +8.0%.",
            "Bonus to Missile Defense: +10.0%.",
        ],
        &["Bonus to Magic Defense"],
    );

    // What it does for a caster, including the quarter-weight the same number gets against
    // another player.
    let mut caster = plain(&[(0x2D, 0x40)]);
    caster.flags |= flags::FLOAT;
    caster.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x90, 0.05), (0x98, 1.2)],
    });
    says(
        &mut c,
        &mut peer,
        &caster,
        &[
            "Bonus to Mana Conversion: +5%.",
            " vs. Monsters: +20.0%.",
            " vs. Players: +5.0%.",
        ],
        &[],
    );

    // Who may use it, by level, and where it goes.
    for (min, max, want) in [
        (20, 40, "Restricted to characters of Levels 20 to 40."),
        (20, 20, "Restricted to characters of Level 20."),
        (20, 0, "Restricted to characters of Level 20 or greater."),
        (0, 40, "Restricted to characters of Level 40 or below."),
    ] {
        let mut pairs = Vec::new();
        if min != 0 {
            pairs.push((0x56, min));
        }
        if max != 0 {
            pairs.push((0x57, max));
        }
        says(&mut c, &mut peer, &plain(&pairs), &[want], &[]);
    }
    let mut portal = plain(&[]);
    portal.flags |= flags::STRING;
    portal.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x26, "Holtburg".to_owned())],
    });
    says(&mut c, &mut peer, &portal, &["Destination: Holtburg"], &[]);

    // What it takes to hold it.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 7), (0x9F, 0), (0xA0, 150)]),
        &["Wield requires level 150"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 2), (0x9F, 45), (0xA0, 250)]),
        &["Wield requires base "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 8), (0x9F, 45), (0xA0, 3)]),
        &["Wield requires specialized "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 8), (0x9F, 45), (0xA0, 2)]),
        &["Wield requires trained "],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x9E, 12), (0x9F, 0), (0xA0, 2)]),
        &["Wield requires Gharu'ndim race"],
        &[],
    );
    let mut owner = plain(&[]);
    owner.flags |= flags::BOOL;
    owner.tables.bools = Some(ints(&[(0x55, 1)]));
    says(
        &mut c,
        &mut peer,
        &owner,
        &["Wield requires the original owner"],
        &["Created by"],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x1A, 1)]),
        &["Use requires Throne of Destiny."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x144, 5)]),
        &["Wield requires Umbraen"],
        &[],
    );

    // What it takes to use it. A skill the shipped table has no row for still draws its line.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x171, 30)]),
        &["Use requires level 30."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x16E, 9999), (0x16F, 250)]),
        &["Use requires Unknown Skill of at least 250."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x170, 9999)]),
        &["Use requires specialized Unknown Skill."],
        &[],
    );

    // How far it has come along, under each of the two ladders.
    let mut level = plain(&[(0x13F, 5), (0x140, 1)]);
    level.flags |= flags::INT64;
    level.tables.int64s = Some(PackedHash {
        table_size: 8,
        entries: vec![(5, 1_000_000), (4, 3_500_000)],
    });
    says(
        &mut c,
        &mut peer,
        &level,
        &["Item Level: 3 / 5", "Item XP: 3,500,000 / 4,000,000"],
        &[],
    );
    let mut doubling = plain(&[(0x13F, 5), (0x140, 2)]);
    doubling.flags |= flags::INT64;
    doubling.tables.int64s = Some(PackedHash {
        table_size: 8,
        entries: vec![(5, 1_000), (4, 7_000)],
    });
    says(
        &mut c,
        &mut peer,
        &doubling,
        &["Item Level: 3 / 5", "Item XP: 7,000 / 15,000"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x160, 2)]),
        &["This cloak has a chance to reduce an incoming attack by 200 damage."],
        &[],
    );

    // What it takes to switch it on, and who alone may.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x6D, 250), (0x6E, 3), (0xBC, 13)]),
        &["Activation requires Arcane Lore: 250, Allegiance Rank: 3, Olthoi"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x102, 200), (0x101, 1)]),
        &["Activation requires Strength: 200"],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0x104, 150), (0x103, 1)]),
        &["Activation requires Maximum Health: 150"],
        &[],
    );
    let mut failed = plain(&[(0x6D, 250)]);
    failed.success_flag = 0;
    says(&mut c, &mut peer, &failed, &[], &["Arcane Lore"]);
    let mut only_one = plain(&[]);
    only_one.flags |= flags::BOOL | flags::STRING;
    only_one.tables.bools = Some(ints(&[(0x5E, 1)]));
    only_one.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x19, "Asheron".to_owned())],
    });
    says(
        &mut c,
        &mut peer,
        &only_one,
        &["This item can only be activated by Asheron."],
        &[],
    );

    // A mana stone, both of whose percentages are percentages.
    let mut stone = plain(&[(0x6B, 600)]);
    stone.flags |= flags::FLOAT;
    stone.tables.floats = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x57, 0.8), (0x89, 0.05)],
    });
    says(
        &mut c,
        &mut peer,
        &stone,
        &[
            "Stored Mana: 600",
            "Efficiency: 80%",
            "Chance of Destruction: 5%",
        ],
        &[],
    );
    let mut absorbed = stone.clone();
    absorbed.flags |= flags::SPELL_BOOK;
    absorbed.spell_book = Some(vec![157]);
    says(&mut c, &mut peer, &absorbed, &[], &["Stored Mana"]);

    // What is left in it.
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xC1, 1)]),
        &["Contains 1 key."],
        &[],
    );
    says(
        &mut c,
        &mut peer,
        &plain(&[(0xC1, 3)]),
        &["Contains 3 keys."],
        &[],
    );
    let mut unlimited = plain(&[(0x5C, 4)]);
    unlimited.flags |= flags::BOOL;
    unlimited.tables.bools = Some(ints(&[(0x3F, 1)]));
    says(
        &mut c,
        &mut peer,
        &unlimited,
        &["Number of uses remaining:  Unlimited"],
        &["Number of uses remaining: 4"],
    );

    // Who made it, and how rare it is.
    let mut made_by = plain(&[]);
    made_by.flags |= flags::STRING;
    made_by.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x19, "Ulgrim".to_owned())],
    });
    says(&mut c, &mut peer, &made_by, &["Created by Ulgrim."], &[]);
    let mut rare = plain(&[(0x11, 42)]);
    rare.flags |= flags::BOOL;
    rare.tables.bools = Some(ints(&[(0x6C, 1)]));
    says(
        &mut c,
        &mut peer,
        &rare,
        &[
            "Rare #42",
            "This rare item has a timer restriction of 3 minutes. You will not be able to use \
             another rare item with a timer within 3 minutes of using this one.",
        ],
        &[],
    );

    c.assert_behaviour(
        "examine.item-blocks.every-line-the-item-pane-can-draw-reaches-it-through-the-shard",
        move |_| ok,
    );
    c.shutdown();
}

/// Assessing another player goes to the character pane, which draws the same nine rows plus the
/// player's own level. The actual list elements are counted as well as the panel's rows.
pub(super) fn the_character_pane_draws_its_rows_and_has_nothing_left_undrawn() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, ObjectId(0x5000_0003));
    assert!(
        profile.creature_profile.is_some(),
        "the recorded answer carries a creature"
    );

    let (active, level, rows, misc) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        let p = &screen.examination;
        (
            p.active,
            p.level_text.clone(),
            p.creature_rows_drawn,
            p.misc_rows_drawn,
        )
    };

    c.assert_behaviour(
        "examine.character.the-pane-draws-its-rows-and-has-nothing-left-undrawn",
        move |_| {
            active == Some(examination::ExamineSubUi::Char)
                && level.as_deref() == Some("6")
                && rows == 9
                && misc == 5
        },
    );
    c.shutdown();
}
