use super::*;

// =============================================================================================
// character-sheet.*
//
// The subject is the character-information panel -- the sheet the **burden** lamp opens, not a burden
// panel: the load section is one of its six and the other five have nothing to do with
// encumbrance. Every claim here is about what the shipped text element ends up saying, so every
// one of them reads the panel's own composed sections *and* the live element, because a test that
// read only the panel's cache would be green over a sheet that never reached the tree.
//
// The recording is `long-solo-play` and not this file's usual `first-login-walk-jump`, and that is
// load-bearing: its character was made moments before the capture, so its playtime is fifteen
// seconds and six of the seven terms of that sentence are blank -- which is the half of the
// duration row worth testing -- and its melee mastery is the first entry of the melee list where
// `first-login-walk-jump`'s is the sixth.
// =============================================================================================

/// The recording these claims are read off. See the section header.
const CHARACTER_SHEET_SESSION: &str = "long-solo-play";

/// The player this file's shard creates and adopts.
const CHARACTER_SHEET_PLAYER: ObjectId = ObjectId(0x5000_0001);

/// Shipped element `0x100000F7` -- the burden lamp that opens the character-information panel.
const CHARACTER_SHEET_BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);

/// What `long-solo-play`'s `0x0013` carries for `98 CreationTimestamp`, and the sentence it makes.
const CHARACTER_SHEET_CORPUS_CREATED: i32 = 1_788_565_581;

/// What it carries for `125 Age`: fifteen seconds. A character made moments before the capture.
const CHARACTER_SHEET_CORPUS_AGE: i32 = 15;

/// `0x14D LumAugDamageRating` -- one of the four ratings whose line splits at five. The **next**
/// pair, `0x14E`, is deliberately left ungranted, so that a specialised value leaking out of this
/// one would show up as a line that must not be there.
const CHARACTER_SHEET_LUM_DAMAGE_RATING: u32 = 0x14D;
/// `0x152 LumAugSurgeChanceRating` -- a single rating, which has no cap of five.
const CHARACTER_SHEET_LUM_SURGE_CHANCE_RATING: u32 = 0x152;
/// `0xDA AugmentationInnateStrength` -- one augmentation row, for the plural block every row has.
const CHARACTER_SHEET_AUG_INNATE_STRENGTH: u32 = 0xDA;
/// `PropertyInt` **43** `NumDeaths`, which is *text* on this sheet and so the visible witness
/// for a property the shard clears.
const CHARACTER_SHEET_NUM_DEATHS: u32 = 43;

/// The recorded character, **with a shard attached**, for a chosen recording.
///
/// This is `a_recorded_character_and_a_shard` with the recording as a parameter: that one is
/// fixed to this file's `SESSION`, and every claim below is read off `long-solo-play` instead.
///
/// The character is made by the shard rather than written into the tables, for the reason that
/// helper gives: an ordered game event is addressed to an object, and the ordered queue only has
/// somewhere to put one for an object the client's own object stream has been told about. The
/// **description** still comes through the corpus step, because it is nearly two kilobytes and is
/// a description rather than a panel notice, so nothing here turns on which way it arrived.
fn character_sheet_a_recorded_character_and_a_shard(
    session: &'static str,
) -> (HeadlessClient, Peer) {
    let end = description_blob(session).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut shard = Peer::attach(&mut c, CHARACTER_SHEET_PLAYER);

    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: CHARACTER_SHEET_PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    shard.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    assert!(
        c.world_mut().set_player(CHARACTER_SHEET_PLAYER),
        "the identity is adopted once"
    );

    c.when(Inbound::from_corpus(session, 0..end + 1));
    c.tick(4);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the recorded description has to install a quality store or nothing here reads a quality"
    );
    (c, shard)
}

/// Open the burden lamp's panel with element message 1 and `p1 = 7`, which is the message a
/// released button raises and the one the six lamp classes answer. The lamp's own gesture is
/// claimed elsewhere; what this file's claims are about is the text behind it.
fn character_sheet_open_the_burden_sheet(c: &mut HeadlessClient) {
    let h = el(c, CHARACTER_SHEET_BURDEN_LAMP);
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    c.tick(6);
    assert!(
        el_visible(c, dereth_ui_screens::panels::characterinfo::PANEL),
        "the burden lamp has to open the character sheet before its text is an observable"
    );
}

/// The sheet's six sections, in the order the composer appends them.
fn character_sheet_sections(c: &HeadlessClient) -> Vec<String> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .character_info
        .sections
        .clone()
}

/// One section of the sheet, with the count asserted rather than indexed into blindly.
fn character_sheet_section(c: &HeadlessClient, n: usize) -> String {
    let s = character_sheet_sections(c);
    assert!(
        s.len() > n,
        "the sheet has been written and carries a section {n}: {s:?}"
    );
    s[n].clone()
}

/// How many times the composer has written the sheet.
fn character_sheet_updates(c: &HeadlessClient) -> u32 {
    c.view().expect_app().hud().panels.character_info.updates
}

/// **What the live information-text element is actually showing**, read back rather than off the
/// panel's own cache.
fn character_sheet_sheet_on_the_element(c: &mut HeadlessClient) -> String {
    el_text(c, dereth_ui_screens::panels::characterinfo::INFO_TEXT)
}

/// A `0x02CD Qualities_PrivateUpdateInt` for the player, **through the transport** -- bytes from
/// the shard rather than a model handed to the panel -- and the frames that consume it.
fn character_sheet_set_int(
    c: &mut HeadlessClient,
    shard: &mut Peer,
    sequence: u8,
    property: u32,
    value: i32,
) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesPrivateUpdateInt(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id: property,
                value,
            },
        ),
    );
    c.tick(6);
}

/// A `0x01D1 Qualities_PrivateRemoveIntEvent` for the player, the same way.
fn character_sheet_remove_int(
    c: &mut HeadlessClient,
    shard: &mut Peer,
    sequence: u8,
    property: u32,
) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesPrivateRemoveInt(
            dereth_protocol::qualities::PrivateRemove {
                sequence,
                property_id: property,
            },
        ),
    );
    c.tick(6);
}

/// The integer table of the **recording's own** character description, decoded out of the recorded
/// blob rather than read back out of the client.
///
/// This is the premise every claim below rests on, and it is asserted rather than assumed: a green
/// run over absent qualities would prove nothing, so each scenario carries the reading it needs.
/// It is the whole table in one call because the corpus is loaded to answer it.
fn character_sheet_recorded_ints(session: &str) -> Vec<(u32, i32)> {
    let d = recorded_description(session);
    d.qualities
        .base
        .tables
        .ints
        .as_ref()
        .expect("the recorded description carries an integer table")
        .entries
        .clone()
}

/// One integer property of it.
fn character_sheet_recorded_int(ints: &[(u32, i32)], property: u32) -> Option<i32> {
    ints.iter().find(|(k, _)| *k == property).map(|(_, v)| *v)
}

/// **The born line.** Creation-timestamp property `0x62` is gated on lookup success rather than
/// its numeric value. It is then converted through local time and formatted as `%c` into a
/// `0x400`-byte buffer.
///
/// **Not UTC, and not `asctime`'s shape.** The retail client's initialization calls
/// `setlocale(LC_ALL, "English")`, so `%c` is `English_United States.1252`'s
/// `M/d/yyyy h:mm:ss tt`, and the zone is the machine's. The expected string is therefore built
/// with the host's own offset, the same way the client builds it, so that this is green in every
/// zone -- **the zone is not what this pins**; the dates scenarios do that. The format is, and the
/// discrimination that matters is that the sentence is in the locale short-date shape and not in
/// `asctime`'s: no weekday, no month name, and an AM/PM that `asctime` has not got.
pub(super) fn the_born_line_is_the_day_and_time_the_character_was_made() {
    use dereth_ui_screens::panels::characterinfo::prop;

    let ints = character_sheet_recorded_ints(CHARACTER_SHEET_SESSION);
    let premise = character_sheet_recorded_int(&ints, prop::CREATION_TIMESTAMP)
        == Some(CHARACTER_SHEET_CORPUS_CREATED);

    let (mut c, _shard) = character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let composed = character_sheet_updates(&c) > 0;
    let born = character_sheet_section(&c, 0);
    let expected = format!(
        "You were born on {}.",
        dereth_ui_screens::ctime::strftime_c(
            i64::from(CHARACTER_SHEET_CORPUS_CREATED),
            dereth_desktop::platform::local_utc_offset_secs(i64::from(
                CHARACTER_SHEET_CORPUS_CREATED
            )),
        )
    );
    let says_it = born.contains(&expected);
    let on_the_element = character_sheet_sheet_on_the_element(&mut c).contains(&expected);
    // `%c` under English_United States has no month name and no weekday...
    let not_asctime = !born.contains("Sep") && !born.contains("Fri");
    // ...and it does have AM/PM.
    let has_the_meridiem = born.contains(" AM.") || born.contains(" PM.");

    c.assert_behaviour(
        "character-sheet.born.the-line-is-the-day-and-time-the-character-was-made",
        move |_| {
            premise && composed && says_it && on_the_element && not_asctime && has_the_meridiem
        },
    );
    c.shutdown();
}

/// **The playtime line**, and the first row in this workspace that needs the metalanguage.
///
/// Integer property `0x7D Age` is rendered through `ID_DurationFormat` on table
/// enum 2. The recorded character is **fifteen seconds** old, which exercises the interesting half
/// of the row: six of the seven terms are blank and `{ …[!b]}` has to delete each of them, along
/// with its separator, instead of rendering a brace.
///
/// The refresh is the second half of the claim and it is not decoration. Initialization
/// registers exactly one player-quality handler, for integer property `0x7D`, so `125` is *the*
/// id whose arrival has to recompose the
/// sheet. Both new figures go through the wire as a `0x02CD`, bytes and all.
///
/// Falsified by: dropping the arm; interleaving the row's pieces instead of rendering it (the
/// sheet reads `#1:{ year[1!b]| years[!b]}…`); rendering a blank term (`0 years 0 months …`);
/// making the plural a suffix rule rather than the block's own alternative.
pub(super) fn the_playtime_line_spells_out_only_the_terms_that_are_not_zero() {
    use dereth_ui_screens::panels::characterinfo::prop;

    let premise = character_sheet_recorded_int(
        &character_sheet_recorded_ints(CHARACTER_SHEET_SESSION),
        prop::AGE,
    ) == Some(CHARACTER_SHEET_CORPUS_AGE);

    let (mut c, mut shard) =
        character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let born_and_played = character_sheet_section(&c, 0);
    let fifteen_seconds = born_and_played.contains("You have played for 15 seconds.");
    let no_markup = !born_and_played.contains('{') && !born_and_played.contains("#1:");

    let before = character_sheet_updates(&c);
    character_sheet_set_int(&mut c, &mut shard, 1, prop::AGE, 90_061);
    let recomposed = character_sheet_updates(&c) > before;
    let after = character_sheet_section(&c, 0);
    // 90 061 s is 1 day 1 hour 1 minute 1 second, each term singular.
    let singular = after.contains("You have played for 1 day 1 hour 1 minute 1 second.");
    let reached_the_element =
        character_sheet_sheet_on_the_element(&mut c).contains("1 day 1 hour 1 minute 1 second");

    // The singular/plural choice is the block's, not a suffix rule: nudge it by one second.
    character_sheet_set_int(&mut c, &mut shard, 2, prop::AGE, 90_062);
    let plural = character_sheet_section(&c, 0).contains("1 day 1 hour 1 minute 2 seconds.");

    c.assert_behaviour(
        "character-sheet.played.the-line-spells-out-only-the-terms-that-are-not-zero",
        move |_| {
            premise
                && fifteen_seconds
                && no_markup
                && recomposed
                && singular
                && reached_the_element
                && plural
        },
    );
    c.shutdown();
}

/// **The mastery lines.**
///
/// They are not with the fake skills: they are the **first three lines of the augmentations
/// section**. The three reads use
/// properties `0x162`, `0x163`, and `0x16A`.
///
/// The fake-skills section reads `ChessRank` and `FakeFishingSkill` and nothing else.
///
/// The recorded character has `354 = 1` and `355 = 8` and **no** `362`, so two lines appear and
/// one does not -- which is what makes this able to fail in both directions. It also carries no
/// `43 NumDeaths`, so the deaths line takes its zero arm; that reading is asserted here because it
/// is the premise the sections claim and the deaths claim both rest on.
///
/// Falsified by: dropping the section; naming the value instead of indexing the list (the sheet
/// says `1` and `8`); using the melee list for the ranged line (`8` would be *"Unknown"*);
/// emitting the summoning line for an absent quality.
pub(super) fn the_mastery_lines_name_the_weapon_group_rather_than_the_number() {
    use dereth_ui_screens::panels::characterinfo::prop;

    // The whole recorded premise, in one place: the four integers the four lines are made of, and
    // the two absences that make two of the assertions below able to fail.
    let ints = character_sheet_recorded_ints(CHARACTER_SHEET_SESSION);
    let premise = character_sheet_recorded_int(&ints, prop::CREATION_TIMESTAMP) == Some(CHARACTER_SHEET_CORPUS_CREATED)
        && character_sheet_recorded_int(&ints, prop::AGE) == Some(CHARACTER_SHEET_CORPUS_AGE)
        // 354 = 1 is "Unarmed Weapons" on the melee list.
        && character_sheet_recorded_int(&ints, prop::WEAPON_MASTERY) == Some(1)
        // 355 = 8 is "Bows" on the ranged one, which is indexed at [v - 8].
        && character_sheet_recorded_int(&ints, prop::MISSILE_MASTERY) == Some(8)
        && character_sheet_recorded_int(&ints, prop::SUMMONING_MASTERY).is_none()
        // ...and no deaths either, so that line takes its zero arm.
        && character_sheet_recorded_int(&ints, CHARACTER_SHEET_NUM_DEATHS).is_none();

    let (mut c, mut shard) =
        character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let augs = character_sheet_section(&c, 4);
    // 354 = 1 is the first line of the augmentations section.
    let melee = augs.starts_with("Your melee mastery is Unarmed Weapons.");
    // 355 = 8 indexes the *ranged* list at [v-8].
    let ranged = augs.contains("Your ranged mastery is Bows.");
    // The recording carries no 362, so the line must not be there.
    let no_summoning = !augs.contains("summoning mastery");
    let on_the_element =
        character_sheet_sheet_on_the_element(&mut c).contains("Your ranged mastery is Bows.");

    // A shard that grants one moves the line, and the three summoning arms are named by value.
    character_sheet_set_int(&mut c, &mut shard, 1, prop::SUMMONING_MASTERY, 2);
    // 362 = 2 maps to the displayed mastery group `Necromancer`.
    let summoning =
        character_sheet_section(&c, 4).contains("Your summoning mastery is Necromancer.");

    c.assert_behaviour(
        "character-sheet.mastery.the-lines-name-the-weapon-group-rather-than-the-number-that-picks-it",
        move |_| premise && melee && ranged && no_summoning && on_the_element && summoning,
    );
    c.shutdown();
}

/// **The luminance augmentations**, header and ratings.
///
/// `ID_CharacterInfo_Luminance_Header` is emitted with **no gate at all**, so it is
/// on every character's sheet; the eleven ratings behind it are each `if (v > 0)`, and four of
/// them are pairs split at 5 that can show two lines from one quality.
///
/// Each pair can therefore contribute both its capped base line and its specialized remainder.
///
/// Falsified by: dropping the section (no header); gating the header on having a rating; clamping
/// a single rating at five the way the pairs are clamped; forgetting the specialised half of a
/// pair; leaking the previous pair's specialised value into the next (the accumulator is cleared
/// between pairs, and omitting that reset would show a phantom line).
pub(super) fn the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single() {
    let (mut c, mut shard) =
        character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let augs = character_sheet_section(&c, 4);
    let header_is_unconditional = augs.contains("Luminance Augmentations:");
    // The recording carries no rating, so no aura line may appear yet.
    let nothing_behind_it = !augs.contains("Aura of");

    // `0x14D LumAugDamageRating = 8` -> base 5 and specialised 3, i.e. **both** lines.
    character_sheet_set_int(&mut c, &mut shard, 1, CHARACTER_SHEET_LUM_DAMAGE_RATING, 8);
    // `0x152 LumAugSurgeChanceRating = 7` -> one line that says **7**, not 5: the singles have no
    // split at 5; this single has only a `> 0` gate.
    character_sheet_set_int(
        &mut c,
        &mut shard,
        2,
        CHARACTER_SHEET_LUM_SURGE_CHANCE_RATING,
        7,
    );

    let augs = character_sheet_section(&c, 4);
    let base_caps_at_five = augs.contains("Aura of Valor. Your Damage Rating is increased by 5.");
    let the_remainder_is_specialised =
        augs.contains("Your Seer grants you an increase to Damage Rating of 3.");
    let a_single_is_not_clamped =
        augs.contains("Your Aetheria's chance to surge is increased by a rating of 7.");
    // 0x14E is absent, so the next pair's specialised line must not appear; clearing the
    // specialised accumulator between pairs prevents the prior remainder from leaking into it.
    let no_leak_into_the_next_pair = !augs.contains("Aura of Invulnerability");
    let on_the_element = character_sheet_sheet_on_the_element(&mut c).contains("Aura of Valor");

    c.assert_behaviour(
        "character-sheet.luminance.the-heading-is-always-there-and-a-rating-over-five-splits-in-two",
        move |_| {
            header_is_unconditional
                && nothing_behind_it
                && base_caps_at_five
                && the_remainder_is_specialised
                && a_single_is_not_clamped
                && no_leak_into_the_next_pair
                && on_the_element
        },
    );
    c.shutdown();
}

/// **An augmentation row, and the plural block every one of them carries.**
///
/// `{time[1]|times}` is an **unnumbered** choice block matched against the row's unnumbered
/// variable, so it is the auto-derived `1` flag on the value that picks the singular. Without
/// `dereth_ui::text::metalanguage` the sheet would read *"3 {time[1]|times}"*.
pub(super) fn an_augmentation_row_says_time_once_and_times_more_than_once() {
    let (mut c, mut shard) =
        character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    character_sheet_set_int(
        &mut c,
        &mut shard,
        1,
        CHARACTER_SHEET_AUG_INNATE_STRENGTH,
        1,
    );
    let one =
        character_sheet_section(&c, 4).contains("You have augmented your Innate Strength 1 time.");

    character_sheet_set_int(
        &mut c,
        &mut shard,
        2,
        CHARACTER_SHEET_AUG_INNATE_STRENGTH,
        3,
    );
    let augs = character_sheet_section(&c, 4);
    let three = augs.contains("You have augmented your Innate Strength 3 times.");
    let no_markup = !augs.contains('{');
    let on_the_element = character_sheet_sheet_on_the_element(&mut c)
        .contains("You have augmented your Innate Strength 3 times.");

    c.assert_behaviour(
        "character-sheet.augmentations.a-row-says-time-once-and-times-more-than-once",
        move |_| one && three && no_markup && on_the_element,
    );
    c.shutdown();
}

/// **The whole sheet, in the composer's order.** The composer appends six sections
/// separated by a literal `"\n"`, and augmentations is the **fifth** -- between the fake-skills
/// section and the load section.
///
/// A section list in the wrong order would pass every claim above and still draw the wrong sheet,
/// so the order is read twice: once off the panel's own sections and once off the live element,
/// where the three landmarks have to come in the same sequence.
pub(super) fn the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them() {
    let (mut c, _shard) = character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let s = character_sheet_sections(&c);
    // Six string appends, six sections.
    let six = s.len() == 6;
    let in_order = six
        && s[0].starts_with("You were born on")
        // ...and the third arm of the first section is still last in it.
        && s[0].contains("You have never died!")
        && s[1].starts_with("Natural Resistances:")
        && s[2].starts_with("Innate Strength:")
        && s[3].starts_with("Chess Rank:")
        && s[4].starts_with("Your melee mastery is")
        && s[5].starts_with("You are not overburdened");

    // And the whole thing reaches the element in that order.
    let text = character_sheet_sheet_on_the_element(&mut c);
    let born = text.find("You were born on");
    let mastery = text.find("Your melee mastery");
    let load = text.find("You are not overburdened");
    let on_the_element = match (born, mastery, load) {
        (Some(b), Some(m), Some(l)) => b < m && m < l,
        _ => false,
    };

    c.assert_behaviour(
        "character-sheet.sections.the-six-parts-are-drawn-in-the-order-the-sheet-builds-them",
        move |_| six && in_order && on_the_element,
    );
    c.shutdown();
}

/// **A property the shard clears disappears from the panel instead of sticking.**
///
/// This is the remove family's one claim that reads the shipped element tree; the others are
/// `cpu` scenarios.
///
/// The birth/age/deaths update's third arm reads integer property `43 NumDeaths` and picks one
/// of four plural forms, so this one integer is *text* on the character sheet. Seven deaths, then
/// a `0x01D1` clearing the property, and the line has to change back -- because lookup on a
/// deleted key answers false and the panel's pre-seeded default takes over, which is what
/// "disappears" means at the byte level.
///
/// What the sheet says for a character the shard has never told about deaths is captured *before*
/// the property exists, so the after-state is compared with a real reading and not with a string
/// this scenario invented.
///
/// Falsified by: deleting the remove arm from the HUD (the sheet keeps saying seven); making the
/// remove a no-op on the player's store (the same); leaving the panel's own cache unchanged so the
/// composer does not re-run.
pub(super) fn a_removed_death_count_disappears_from_the_character_sheet() {
    use dereth_client_model::{StatKey, StatType};

    let (mut c, mut shard) =
        character_sheet_a_recorded_character_and_a_shard(CHARACTER_SHEET_SESSION);
    character_sheet_open_the_burden_sheet(&mut c);

    let with_none = character_sheet_section(&c, 0);
    let updates_at_start = character_sheet_updates(&c);

    character_sheet_set_int(&mut c, &mut shard, 1, CHARACTER_SHEET_NUM_DEATHS, 7);
    let with_seven = character_sheet_section(&c, 0);
    let seven_reached_the_sheet = with_seven.contains('7');
    let the_line_moved = with_seven != with_none;
    let recomposed_for_the_update = character_sheet_updates(&c) > updates_at_start;
    let updates_before_remove = character_sheet_updates(&c);

    // `0x01D1 Qualities_PrivateRemoveIntEvent` -- ACE clears `NumDeaths` nowhere, but the retail
    // client has the handler and this is the shape of every property a shard ever clears.
    character_sheet_remove_int(&mut c, &mut shard, 2, CHARACTER_SHEET_NUM_DEATHS);

    let key_is_gone = c
        .view()
        .world()
        .player_qualities()
        .expect("the player's store")
        .get(StatKey::new(StatType::Int, CHARACTER_SHEET_NUM_DEATHS))
        .is_none();
    let after = character_sheet_section(&c, 0);
    let stopped_saying_seven = !after.contains('7');
    let reads_as_it_did = after == with_none;
    // A stale cache would show the same text for the wrong reason.
    let recomposed_for_the_remove = character_sheet_updates(&c) > updates_before_remove;

    c.assert_behaviour(
        "character-sheet.deaths.a-count-the-shard-clears-leaves-the-sheet",
        move |_| {
            seven_reached_the_sheet
                && the_line_moved
                && recomposed_for_the_update
                && key_is_gone
                && stopped_saying_seven
                && reads_as_it_did
                && recomposed_for_the_remove
        },
    );
    c.shutdown();
}
