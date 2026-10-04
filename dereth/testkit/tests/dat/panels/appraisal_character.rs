use super::*;

// =============================================================================================
// examine.character.*, examine.creature.* -- the rest of the pane
//
// The recorded player's own body is the oracle for the armour rows and the three lines beside
// them: the recording carries them and nothing here builds them. The rows the corpus has **no**
// body for -- society, allegiance, the ratings and the seven tail rows -- are the recording's own
// answer with one property added to it, which is the same shape every other scenario in this file
// uses: the shard's answer goes in at the wire and everything after it is the client's.
// =============================================================================================

/// The recorded player's own armour, as the pane writes it out.
const RECORDED_ARMOUR: [&str; 3] = ["AL: 170/150/150", "AL: 150/150/170", "AL: 150/150/170"];

/// The three colours a row of the character pane's extra list declares.
const MISC_TABLE: [u32; 3] = [0xFFFF_FFFF, 0xFF00_FF00, 0xFFFF_0000];

/// The same profile with these strings **added to** whatever it already carries.
fn with_more_strings(base: &AppraisalProfile, pairs: &[(u32, &str)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    let mut entries = p
        .tables
        .strings
        .clone()
        .map(|h| h.entries)
        .unwrap_or_default();
    for (k, v) in pairs {
        entries.retain(|(ek, _)| ek != k);
        entries.push((*k, (*v).to_owned()));
    }
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries,
    });
    p
}

/// The same profile with no armour block at all.
fn without_base_armour(base: &AppraisalProfile) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    p.flags &= !flags::BASE_ARMOR;
    p.base_armor = None;
    p
}

/// What the character pane is showing, read back off the panel after an answer.
#[derive(Debug, Clone)]
struct CharPane {
    sub: Option<examination::ExamineSubUi>,
    rows: Vec<examination::MiscRow>,
    drawn: u32,
    heritage: Option<String>,
    profession: Option<String>,
    pk: Option<String>,
    allegiance: Option<String>,
    title: Option<String>,
}

fn char_pane(c: &mut HeadlessClient) -> CharPane {
    let (_, screen) = gameplay_screen(c.app_mut());
    let p = &screen.examination;
    CharPane {
        sub: p.active,
        rows: p.misc_row_text.clone(),
        drawn: p.misc_rows_drawn,
        heritage: p.heritage_text.clone(),
        profession: p.profession_text.clone(),
        pk: p.pk_status_text.clone(),
        allegiance: p.allegiance_name_text.clone(),
        title: p.title_text.clone(),
    }
}

/// The pane's rows as `(label, value)`, which is how every claim below reads them.
fn row_pairs(rows: &[examination::MiscRow]) -> Vec<(String, String)> {
    rows.iter()
        .map(|r| (r.label.clone(), r.value.clone()))
        .collect()
}

/// Assess `object` again with `profile`, and hand back what the pane then shows.
fn reassess(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) -> CharPane {
    assess(c, peer, object, profile);
    char_pane(c)
}

/// One live row of the pane's extra list, with the glyph colours of both its cells and the
/// colours each cell's own template declares.
#[derive(Debug, Clone)]
struct DrawnMiscRow {
    label: String,
    value: String,
    label_colours: Vec<u32>,
    value_colours: Vec<u32>,
    label_declared: Vec<u32>,
    value_declared: Vec<u32>,
}

/// Every row of the extra list after layout and composition -- the **draw data**, not the panel's
/// own copy of the selector under test.
fn drawn_misc_rows(c: &mut HeadlessClient) -> Vec<DrawnMiscRow> {
    let window = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.window.expect("the window is bound")
    };
    let list = c
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .get_child_recursive(window, examination::MISC_LIST)
        .expect("the character pane's extra list is in the shipped layout");
    let rows = c.app_mut().ui().expect("the shell is up").ui.children(list);
    let mut out = Vec::new();
    for row in rows {
        let pair = {
            let ui = &c.app_mut().ui().expect("the shell is up").ui;
            (
                ui.get_child_recursive(row, examination::ROW_LABEL),
                ui.get_child_recursive(row, examination::ROW_VALUE),
            )
        };
        let (Some(label), Some(value)) = pair else {
            continue;
        };
        let cell = |c: &mut HeadlessClient, h: ElemHandle| {
            let app = c.app_mut();
            let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
            let t = app
                .ui_mut()
                .expect("the shell is up")
                .ui
                .text_element_mut(h)
                .expect("a row cell is text");
            let text = t.glyphs.inq_text(false);
            let declared = t.font_colors.clone();
            let colours: Vec<u32> = t.compose(box_).into_iter().map(|g| g.color).collect();
            (text, colours, declared)
        };
        let (label, label_colours, label_declared) = cell(c, label);
        let (value, value_colours, value_declared) = cell(c, value);
        out.push(DrawnMiscRow {
            label,
            value,
            label_colours,
            value_colours,
            label_declared,
            value_declared,
        });
    }
    out
}

/// Tell the client which society the **player** belongs to, as a real update through the shard.
fn set_viewer_society(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    sequence: u8,
    value: i32,
) {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: 0x119,
            value,
        },
    );
    peer.event(c, &m);
    c.tick(2);
}

/// **The recorded player's own body.** The armour rows are the recording's nine numbers, the three
/// lines beside them are its gender, its heritage and its character title, and the pane's list of
/// rows it cannot draw is empty because they were drawn.
pub(super) fn the_character_pane_rows_are_the_shards_own_numbers() {
    let (mut c, _peer, profile) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let carries = profile.creature_profile.is_some()
        && profile.base_armor == Some([170, 150, 150, 150, 150, 170, 150, 150, 170]);

    let p = char_pane(&mut c);
    let got = row_pairs(&p.rows);
    let rows_read = got.len() == 5
        && p.rows[0] == examination::MiscRow::default()
        && got[1] == ("Head/Chest/Groin".to_owned(), RECORDED_ARMOUR[0].to_owned())
        && got[2] == ("Bicep/Wrist/Hand".to_owned(), RECORDED_ARMOUR[1].to_owned())
        && got[3] == ("Thigh/Shin/Foot".to_owned(), RECORDED_ARMOUR[2].to_owned())
        && got[4]
            == (
                examination::UNENCHANTABLE_FOOTNOTE.to_owned(),
                String::new(),
            );

    // And every one of them reached an element of the list, not only the panel's own vector.
    let all_drawn = p.drawn == 5;

    let beside = p.sub == Some(examination::ExamineSubUi::Char)
        && p.heritage.as_deref() == Some("Female Sho")
        && p.profession.as_deref() == Some("Life Caster")
        && p.pk.as_deref() == Some("Non-Player Killer")
        // Nothing said the character has an allegiance, so the line is cleared and not left
        // holding the last one.
        && p.allegiance.is_none();

    c.assert_behaviour(
        "examine.character.the-armour-rows-are-the-shards-own-numbers-and-the-three-lines-beside-them",
        move |_| carries && rows_read && all_drawn && beside,
    );
    c.shutdown();
}

/// The society row names the society and the band the rank falls in, drops the band when there is
/// no rank at all, and says as much when the society is one it has no name for -- and it is drawn
/// at the top of the list.
pub(super) fn the_society_row_names_the_society_and_its_rank_band() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    // The recording's own character belongs to no society, so the row must be absent first.
    let absent_first = !char_pane(&mut c).rows.iter().any(|r| r.label == "Society:");

    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 2), (0x120, 350)]),
    );
    let banded = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "Eldrytch Web ~ Knight" && r.color == 0);
    let drawn_first = p.rows.first().is_some_and(|r| r.label == "Society:");

    // A society with no rank at all: the band is dropped rather than printed as nothing.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 1)]),
    );
    let unranked = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "Celestial Hand");

    // A society the pane has no name for says so, rather than drawing an empty row.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x119, 0x10)]),
    );
    let unknown = p
        .rows
        .iter()
        .find(|r| r.label == "Society:")
        .is_some_and(|r| r.value == "???");

    c.assert_behaviour(
        "examine.character.the-society-row-names-the-society-and-the-band-its-rank-falls-in",
        move |_| absent_first && banded && drawn_first && unranked && unknown,
    );
    c.shutdown();
}

/// The society row is coloured by the **viewer's own** society: plain when they have none, the
/// row template's green for a fellow member and its red for a rival -- read off the glyphs each
/// cell composed, and off the colours each cell's own template declares.
///
/// Both cells take the same choice, so both have to declare and resolve the whole table: a label
/// that stayed white while its value went red would be a half-coloured row.
pub(super) fn the_society_row_is_coloured_by_the_viewers_own_society() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // The viewer has to have been described before anything can be said about their own society.
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    c.tick(2);

    let target = with_ints(&base, &[(0x119, 1), (0x11F, 175)]);
    let look = |c: &mut HeadlessClient, peer: &mut dereth_testkit::Peer| -> DrawnMiscRow {
        let p = reassess(c, peer, APPRAISED_PLAYER, &target);
        assert_eq!(
            p.drawn as usize,
            p.rows.len(),
            "every row the pane worked out reached the real list"
        );
        drawn_misc_rows(c)
            .into_iter()
            .find(|r| r.label == "Society:")
            .expect("the society row is a live child of the list")
    };

    let neutral = look(&mut c, &mut peer);
    let plain = neutral.value == "Celestial Hand ~ Adept"
        && neutral.label_declared == MISC_TABLE
        && neutral.value_declared == MISC_TABLE
        && !neutral.label_colours.is_empty()
        && neutral
            .label_colours
            .iter()
            .all(|col| *col == MISC_TABLE[0])
        && !neutral.value_colours.is_empty()
        && neutral
            .value_colours
            .iter()
            .all(|col| *col == MISC_TABLE[0]);

    set_viewer_society(&mut c, &mut peer, 1, 1);
    let same = look(&mut c, &mut peer);
    let fellow = !same.label_colours.is_empty()
        && !same.value_colours.is_empty()
        && same.label_colours.iter().all(|col| *col == MISC_TABLE[1])
        && same.value_colours.iter().all(|col| *col == MISC_TABLE[1]);

    set_viewer_society(&mut c, &mut peer, 2, 2);
    let rival_row = look(&mut c, &mut peer);
    let rival = !rival_row.label_colours.is_empty()
        && !rival_row.value_colours.is_empty()
        && rival_row
            .label_colours
            .iter()
            .all(|col| *col == MISC_TABLE[2])
        && rival_row
            .value_colours
            .iter()
            .all(|col| *col == MISC_TABLE[2]);

    c.assert_behaviour(
        "examine.character.the-society-row-is-coloured-by-the-viewers-own-society",
        move |_| plain && fellow && rival,
    );
    c.shutdown();
}

/// The allegiance rows fork on the two titles: a vassal with a monarch and a different patron gets
/// two rows, one whose monarch *is* their patron gets one row naming both, and a monarch gets a
/// count of followers with the singular and the plural told apart. A character of no rank gets
/// none of it, and the allegiance name beside the rows is cleared rather than left holding the
/// last character's.
pub(super) fn the_allegiance_rows_fork_on_the_two_titles() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let vassal = with_more_strings(
        &with_ints(&base, &[(0x1E, 3)]),
        &[
            (0x2F, "The Lost Light"),
            (0x15, "High King Al"),
            (0x23, "Baron Bob"),
        ],
    );
    let p = reassess(&mut c, &mut peer, APPRAISED_PLAYER, &vassal);
    let got = row_pairs(&p.rows);
    let two_rows = got.contains(&("Monarch:".to_owned(), "High King Al".to_owned()))
        && got.contains(&("Patron:".to_owned(), "Baron Bob".to_owned()))
        && !got.iter().any(|(l, _)| l == "Monarch/Patron:")
        && p.allegiance.as_deref() == Some("The Lost Light");

    let direct = with_more_strings(
        &with_ints(&base, &[(0x1E, 2)]),
        &[(0x15, "High King Al"), (0x23, "High King Al")],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &direct).rows);
    let one_row = got.contains(&("Monarch/Patron:".to_owned(), "High King Al".to_owned()));

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x1E, 9), (0x23, 1)]),
        )
        .rows,
    );
    let one_follower = got.contains(&("Alleg. Monarch:".to_owned(), "1 Follower".to_owned()));
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x1E, 9), (0x23, 12)]),
        )
        .rows,
    );
    let many_followers = got.contains(&("Alleg. Monarch:".to_owned(), "12 Followers".to_owned()));

    // No rank at all, which is the same thing as rank nothing: the whole block is skipped and the
    // name element is cleared.
    let p = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_more_strings(&base, &[(0x2F, "The Lost Light")]),
    );
    let got = row_pairs(&p.rows);
    let none = !got
        .iter()
        .any(|(l, _)| l.starts_with("Monarch") || l.starts_with("Alleg"))
        && p.allegiance.is_none();

    c.assert_behaviour(
        "examine.character.the-allegiance-rows-fork-on-whether-the-monarch-is-also-the-patron",
        move |_| two_rows && one_row && one_follower && many_followers && none,
    );
    c.shutdown();
}

/// An allegiance rank puts its own title in front of the character's name in the window's title
/// bar -- and a rank there is no title for leaves the bare name.
pub(super) fn an_allegiance_rank_puts_its_title_in_front_of_the_name() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let plain = char_pane(&mut c).title.expect("the window has a title");
    let filled_first = !plain.is_empty();

    let titled = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x1E, 3)]),
    )
    .title
    .expect("the window has a title");
    let prefixed = titled != plain && titled.ends_with(&plain) && titled.len() > plain.len();

    // A rank outside the range the titles cover: the bare name stands.
    let untitled = reassess(
        &mut c,
        &mut peer,
        APPRAISED_PLAYER,
        &with_ints(&base, &[(0x1E, 44)]),
    )
    .title;
    let bare = untitled.as_deref() == Some(plain.as_str());

    c.assert_behaviour(
        "examine.character.an-allegiance-rank-puts-its-title-in-front-of-the-name",
        move |_| filled_first && prefixed && bare,
    );
    c.shutdown();
}

/// A body part nothing can be cast on is marked with a star and its armour reads as what is left
/// -- and a character the shard sent no armour block for loses the three rows and the blank line
/// above them, keeping only the footnote, which sits outside every guard.
pub(super) fn an_unenchantable_body_part_is_marked_with_a_star() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let mut starred = base.clone();
    starred.base_armor = Some([170 + 9999, 150, 150, 150, 150, 170, 150, 150, 9999]);
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &starred).rows);
    let marked = got.contains(&("Head/Chest/Groin".to_owned(), "AL: *170/150/150".to_owned()))
        // Exactly the marker and nothing else is still marked, and reads as nothing left.
        && got.contains(&("Thigh/Shin/Foot".to_owned(), "AL: 150/150/*0".to_owned()));

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &without_base_armour(&base),
        )
        .rows,
    );
    let only_the_footnote = got
        == vec![(
            examination::UNENCHANTABLE_FOOTNOTE.to_owned(),
            String::new(),
        )];

    c.assert_behaviour(
        "examine.character.a-body-part-nothing-can-be-cast-on-is-marked-and-reads-what-is-left",
        move |_| marked && only_the_footnote,
    );
    c.shutdown();
}

/// The three rating groups draw the pairs the pane prints and no others: the number that opens a
/// group is a guard and is never itself drawn, and one of the numbers the pane reads is read and
/// then never printed at all.
pub(super) fn the_rating_groups_draw_the_pairs_the_pane_prints() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // The first group alone, opened by a number that is a guard and never drawn itself.
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&base, &[(0x139, 7)]),
        )
        .rows,
    );
    // The group is opened and its pair reads nothing, which is the guard not being drawn: the
    // number that opened it is seven and the row says nothing of the sort.
    let guard_only = got.contains(&("Dmg/CritDmg".to_owned(), "Rating: 0/0".to_owned()))
        && !got
            .iter()
            .any(|(l, v)| l.starts_with("Dmg") && v != "Rating: 0/0");

    let all = with_ints(
        &base,
        &[
            (0x133, 11),
            (0x13A, 22),
            (0x134, 33),
            (0x13C, 44),
            (0x15E, 55),
            (0x15F, 66),
            (0x143, 77),
        ],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &all).rows);
    let three_groups = got.contains(&("Dmg/CritDmg".to_owned(), "Rating: 11/22".to_owned()))
        && got.contains(&("Dmg/CritDmg".to_owned(), "Resist: 33/44".to_owned()))
        && got.contains(&("DoT/Life:".to_owned(), "Resist: 55/66".to_owned()))
        // ...and the one the pane reads and never draws.
        && !got.iter().any(|(_, v)| v.contains("77"));

    c.assert_behaviour(
        "examine.character.the-rating-groups-draw-the-pairs-the-pane-prints-and-no-others",
        move |_| guard_only && three_groups,
    );
    c.shutdown();
}

/// The creature pane shares the same list and draws the same rating rows, with no footnote of its
/// own -- and a creature the shard sent no rating above nothing for draws no rows at all.
pub(super) fn the_creature_pane_draws_the_same_rating_rows_without_the_footnote() {
    let (mut c, mut peer, golem) = assessing_the_recorded(EXAMINE_SESSION, GOLEM);
    let p = char_pane(&mut c);
    let creature_pane = p.sub == Some(examination::ExamineSubUi::Creature);
    // The recorded creature carries no rating above nothing, and the creature pane has no
    // footnote, so it draws nothing at all here.
    let empty = p.rows.is_empty();

    let p = reassess(
        &mut c,
        &mut peer,
        GOLEM,
        &with_ints(&golem, &[(0x133, 5), (0x13A, 6)]),
    );
    let got = row_pairs(&p.rows);
    let drawn =
        got == vec![
            (String::new(), String::new()),
            ("Dmg/CritDmg".to_owned(), "Rating: 5/6".to_owned()),
            (String::new(), String::new()),
        ] && p.drawn == 3;

    c.assert_behaviour(
        "examine.creature.the-creature-pane-draws-the-same-rating-rows-without-the-footnote",
        move |_| creature_pane && empty && drawn,
    );
    c.shutdown();
}

/// The seven rows at the foot of the character pane, each with the thing behind it -- and the two
/// of them whose keys are the same number in two different tables really are two rows, which is
/// the only way to believe either.
pub(super) fn the_tail_rows_are_the_seven_the_pane_lists() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let bare = without_base_armour(&base);

    let full = with_more_strings(
        &with_ints(
            &bare,
            &[
                (0x7D, 3_723),
                (0xB5, 1_600),
                (0xC0, 42),
                (0x2B, 7),
                (0x106, 17),
            ],
        ),
        &[(0x0A, "The Lost Light"), (0x2B, "4/13/2026 7:14:52 PM")],
    );
    let got = row_pairs(&reassess(&mut c, &mut peer, APPRAISED_PLAYER, &full).rows);
    let want: Vec<(String, String)> = [
        ("Fellowship:", "The Lost Light"),
        ("Arrived in Dereth:", "4/13/2026 7:14:52 PM"),
        ("Time in Dereth:", "1h 2m 3s"),
        ("Chess Rank:", "1600"),
        ("Fishing Skill:", "42"),
        ("Deaths:", "7"),
        ("Titles Earned:", "17"),
        (examination::UNENCHANTABLE_FOOTNOTE, ""),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let in_order = got == want;

    // Never having died is a sentence and not a number...
    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_ints(&bare, &[(0x2B, 0)]),
        )
        .rows,
    );
    let never_died = got.contains(&("Deaths:".to_owned(), "Has never died".to_owned()))
        // ...and the same key number in the other table is a different row entirely.
        && !got.iter().any(|(l, _)| l == "Arrived in Dereth:");

    let got = row_pairs(
        &reassess(
            &mut c,
            &mut peer,
            APPRAISED_PLAYER,
            &with_more_strings(&bare, &[(0x2B, "4/13/2026 7:14:52 PM")]),
        )
        .rows,
    );
    let the_other_way = got.contains(&(
        "Arrived in Dereth:".to_owned(),
        "4/13/2026 7:14:52 PM".to_owned(),
    )) && !got.iter().any(|(l, _)| l == "Deaths:");

    c.assert_behaviour(
        "examine.character.the-seven-tail-rows-each-draw-the-thing-behind-them",
        move |_| in_order && never_died && the_other_way,
    );
    c.shutdown();
}

/// **The whole pane at once, in the order it writes it** -- so a row with the right words in the
/// wrong place is still a failure -- and every one of them reaching a real element of the list.
pub(super) fn every_character_row_draws_in_the_panes_own_order() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let everything = with_more_strings(
        &with_ints(
            &base,
            &[
                (0x119, 4),
                (0x121, 1_200),
                (0x1E, 5),
                (0x133, 11),
                (0x13A, 22),
                (0x7D, 90),
                (0xB5, 1_600),
                (0xC0, 42),
                (0x2B, 3),
                (0x106, 17),
            ],
        ),
        &[
            (0x2F, "The Lost Light"),
            (0x15, "High King Al"),
            (0x23, "Baron Bob"),
            (0x0A, "The Second Light"),
        ],
    );
    let p = reassess(&mut c, &mut peer, APPRAISED_PLAYER, &everything);
    let got = row_pairs(&p.rows);
    let want: Vec<(String, String)> = [
        ("Society:", "Radiant Blood ~ Master"),
        ("Monarch:", "High King Al"),
        ("Patron:", "Baron Bob"),
        ("", ""),
        ("Head/Chest/Groin", RECORDED_ARMOUR[0]),
        ("Bicep/Wrist/Hand", RECORDED_ARMOUR[1]),
        ("Thigh/Shin/Foot", RECORDED_ARMOUR[2]),
        ("", ""),
        ("Dmg/CritDmg", "Rating: 11/22"),
        ("", ""),
        ("Fellowship:", "The Second Light"),
        ("Time in Dereth:", "1m 30s"),
        ("Chess Rank:", "1600"),
        ("Fishing Skill:", "42"),
        ("Deaths:", "3"),
        ("Titles Earned:", "17"),
        (examination::UNENCHANTABLE_FOOTNOTE, ""),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    let in_order = got == want;
    let all_drawn = p.drawn as usize == p.rows.len();

    c.assert_behaviour(
        "examine.character.every-row-draws-in-the-panes-own-order-and-reaches-the-list",
        move |_| in_order && all_drawn,
    );
    c.shutdown();
}
