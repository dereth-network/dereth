use super::*;

// =============================================================================================
// examine.description.*, examine.portal.*, examine.failed-assess.*
//
// The same harness as the rest of this window: the recorded object, the recorded answer, and the
// one property the scenario adds to it. Everything downstream of the answer is the client's own
// production path.
// =============================================================================================

/// The recorded player, which takes the **character** pane and carries the creature block.
pub(super) const APPRAISED_PLAYER: ObjectId = ObjectId(0x5000_0003);

/// The two description keys the item pane forks on, the bits it decorates with, and the
/// restrictions of a destroyed portal -- which is what every one of that kind carries in the
/// reference server's own world data.
const SHORT_DESC: u32 = 0x0F;
const LONG_DESC: u32 = 0x10;
const DESTROYED_PORTAL: i32 = 49;

const NO_RECALL_LINE: &str = "This portal cannot be recalled nor linked to.";
const NO_SUMMON_LINE: &str = "This portal cannot be summoned.";
const NO_PK_LINE: &str = "Player Killers may not use this portal.";
const NO_PK_LITE_LINE: &str = "Lite Player Killers may not use this portal.";
const NO_NPK_LINE: &str = "Non-Player Killers may not use this portal.";

/// The same profile with these whole numbers set, replacing any it already carries.
pub(super) fn with_ints(base: &AppraisalProfile, pairs: &[(u32, i32)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    let mut entries = p.tables.ints.clone().map(|h| h.entries).unwrap_or_default();
    for (k, v) in pairs {
        entries.retain(|(ek, _)| ek != k);
        entries.push((*k, *v));
    }
    p.flags |= flags::INT;
    p.tables.ints = Some(dereth_protocol::archive::PackedHash {
        table_size: 32,
        entries,
    });
    p
}

/// The same profile with exactly these strings, and no others.
fn with_strings(base: &AppraisalProfile, entries: &[(u32, &str)]) -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = base.clone();
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: entries.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
    });
    p
}

/// The assessment window in the live tree.
fn exam_window(c: &mut HeadlessClient) -> ElemHandle {
    let app = c.app_mut();
    let shell = app.ui().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is current").roots()[0];
    shell
        .ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("the window is in the layout")
}

/// One element under `root`.
fn exam_find(c: &mut HeadlessClient, root: ElemHandle, id: ElementId) -> Option<ElemHandle> {
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .get_child_recursive(root, id)
}

/// Every composed glyph of an element as `(character, colour)` -- the **draw data**, and not a
/// field the panel keeps beside it.
fn painted(c: &mut HeadlessClient, h: ElemHandle) -> Vec<(char, u32)> {
    let app = c.app_mut();
    let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    let Some(t) = ui.text_element_mut(h) else {
        return Vec::new();
    };
    t.compose(box_)
        .into_iter()
        .map(|g| (char::from_u32(u32::from(g.ch)).unwrap_or('?'), g.color))
        .collect()
}

/// The distinct colours the first occurrence of `needle` is painted in, in order.
fn colors_of(p: &[(char, u32)], needle: &str) -> Vec<u32> {
    let text: String = p.iter().map(|(ch, _)| *ch).collect();
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not drawn; the element reads:\n{text}"));
    let mut out = Vec::new();
    for (_, colour) in &p[at..at + needle.chars().count()] {
        if !out.contains(colour) {
            out.push(*colour);
        }
    }
    out
}

/// The plain colour the item description pane's own array declares.
fn description_plain(c: &mut HeadlessClient) -> u32 {
    let w = exam_window(c);
    let h = exam_find(c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let app = c.app_mut();
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(h)
        .expect("a text element")
        .font_color_at(0)
}

/// The colour index each of the nine stat rows was drawn with, as the panel recorded it.
fn creature_row_colors(c: &mut HeadlessClient) -> Vec<u8> {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.creature_row_colors.clone()
}

/// The drawn rows of the stat list as `(label, label colour, value, value colour)`, read off the
/// **glyphs each cell is carrying** -- so a colour that never reached a cell fails here and not in
/// the panel's own bookkeeping.
fn drawn_stat_rows(c: &mut HeadlessClient) -> Vec<(String, u32, String, u32)> {
    let w = exam_window(c);
    let base = exam_find(c, w, examination::CREATURE_BASE).expect("the creature pane");
    let list = exam_find(c, base, examination::CREATURE_STAT_LIST).expect("the stat list");
    let rows = c.app_mut().ui().expect("the shell is up").ui.children(list);
    let mut out = Vec::new();
    for row in rows {
        let (Some(label), Some(value)) = (
            exam_find(c, row, examination::ROW_LABEL),
            exam_find(c, row, examination::ROW_VALUE),
        ) else {
            continue;
        };
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let cell = |ui: &mut dereth_ui::UiSystem, h: ElemHandle| -> (String, u32) {
            let Some(t) = ui.text_element_mut(h) else {
                return (String::new(), 0);
            };
            let text: String = t
                .glyphs
                .glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.data)).unwrap_or('?'))
                .collect();
            let colour = t.glyphs.glyphs.first().map_or(0, |g| g.color);
            (text, colour)
        };
        let (label_text, label_colour) = cell(ui, label);
        let (value_text, value_colour) = cell(ui, value);
        out.push((label_text, label_colour, value_text, value_colour));
    }
    out
}

/// The colour array the stat list's value cell authors -- **four** where the item description
/// pane authors three, and the fourth is the one the failure arm indexes.
fn value_cell_colors(c: &mut HeadlessClient) -> Vec<u32> {
    let w = exam_window(c);
    let base = exam_find(c, w, examination::CREATURE_BASE).expect("the creature pane");
    let list = exam_find(c, base, examination::CREATURE_STAT_LIST).expect("the stat list");
    let row = c
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .children(list)
        .first()
        .copied()
        .expect("the stat list has a row");
    let value = exam_find(c, row, examination::ROW_VALUE).expect("the row's value cell");
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(value)
        .expect("a text element")
        .font_colors
        .clone()
}

/// **The last line of the item pane.** A gem that drains experience says how much, in the
/// shipped sentence with the shipped separators, and says nothing at all when the shard did not
/// name a cost -- which is presence and not a positive number.
pub(super) fn the_augmentation_cost_line_comes_off_the_shipped_sentence() {
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    // The sentence itself, off the shipped string table -- a premise, so a data change fails here
    // rather than quietly rewriting the claim.
    //
    // **The row is found by what it says and not by the hash of its name.** The hashing crate the
    // client keys these rows with is not a dependency of this one, and taking one would move the
    // lock file. What the premise needs is that the shipped
    // data carries this sentence -- in two pieces with their own newlines, with exactly one thing
    // interpolated between them -- and that only one row carries it.
    let table: dereth_assets::ui::StringTable = table(&c, 0x2300_0001);
    let want = vec![
        "\\nUsing this gem will drain ".to_owned(),
        " points of your available experience.\\n".to_owned(),
    ];
    let matching: Vec<_> = table
        .strings
        .iter()
        .filter(|(_, r)| r.strings == want)
        .map(|(_, r)| r.clone())
        .collect();
    let shipped = (
        matching.len() == 1,
        matching.first().is_some_and(|r| r.variables.len() == 1),
    );

    let base = with_strings(&base, &[(LONG_DESC, "Augmentation description.")]);
    assess(&mut c, &mut peer, POTION, &base);
    let without = item_text(&mut c);
    let ends_with_description = without.ends_with("Augmentation description.");

    let mut every_cost_reads = true;
    for (cost, formatted) in [
        (5_000_000_000_i64, "5,000,000,000"),
        // The client converts the shard's whole number to a double before it takes its digits
        // out, and this is the first positive one that cannot survive that exactly.
        (9_007_199_254_740_993, "9,007,199,254,740,992"),
        (0, "0"),
        (-1234, "-1,234"),
    ] {
        let mut p = base.clone();
        p.flags |= flags::INT64;
        p.tables.int64s = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: vec![(3, cost)],
        });
        assess(&mut c, &mut peer, POTION, &p);
        every_cost_reads &= item_text(&mut c)
            == format!(
                "{without}\nUsing this gem will drain {formatted} points of your available \
                 experience.\n"
            );
    }

    assess(&mut c, &mut peer, POTION, &base);
    let removed = item_text(&mut c) == without;

    c.assert_behaviour(
        "examine.augmentation.the-cost-line-is-the-shipped-sentence-with-the-number-in-it",
        move |_| shipped == (true, true) && ends_with_description && every_cost_reads && removed,
    );
    c.shutdown();
}

/// A thing with a cooldown says how long it is, and how long is left of the player's own -- the
/// second only when the player really is waiting on *that* group, and neither unless the shard
/// named a duration.
pub(super) fn the_cooldown_lines_reach_the_item_pane() {
    use dereth_protocol::types::appraisal::flags;

    /// The two keys: which cooldown group the thing is in, and how long its own is.
    const GROUP: u32 = 0x118;
    const DURATION: u32 = 0xA7;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let mut p = with_ints(&base, &[(GROUP, 3)]);
    p.flags |= flags::FLOAT;
    p.tables.floats = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(DURATION, 90.9)],
    });

    // **The looker has to have been described.** The pane asks the *player's own* quality bag
    // what is still running, and this harness's looker is a body the shard created and nothing
    // more -- so the shard describes it first, which is what a real session does at login.
    peer.event(
        &mut c,
        &dereth_protocol::login::LoginPlayerDescription::default(),
    );
    c.tick(2);

    // The recording carries no live cooldown, so the player is given one: the claim is about
    // what the pane reads out of the player's own list, and the list has to have something in it.
    let start = c.view().expect_app().hud().now.0;
    c.world_mut()
        .player_qualities_mut()
        .expect("the recorded character description")
        .enchantments
        .cooldown_list
        .push(dereth_rules::enchant::Enchantment {
            id: 0x8003,
            spell_category: 0,
            power_level: 0,
            start_time: start,
            duration: 45.75,
            caster: ObjectId(0),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: start,
            smod: dereth_protocol::types::qualities::StatMod {
                kind: dereth_rules::enchant::ench_type::COOLDOWN,
                key: 0,
                value: 0.0,
            },
            spell_set_id: None,
        });

    assess(&mut c, &mut peer, POTION, &p);
    let text = item_text(&mut c);
    let now = c.view().expect_app().hud().now.0;
    #[allow(clippy::cast_possible_truncation)]
    let seconds = (45.75 - (now - start)).trunc() as i32;
    assert!(
        (1..60).contains(&seconds),
        "the running scenario must not outlive its own fixture"
    );
    let remaining = format!("Cooldown Remaining: {seconds}s");
    let duration_line = "Cooldown When Used: 1m 30s";
    let both_in_the_model = text.contains(duration_line) && text.contains(&remaining);

    // ...and both really reached the glyphs the player reads.
    let drawn = description(&mut c);
    let both_drawn = drawn.contains(duration_line) && drawn.contains(&remaining);

    // A different group is not the player's, so only the duration is drawn.
    assess(&mut c, &mut peer, POTION, &with_ints(&p, &[(GROUP, 4)]));
    let other = item_text(&mut c);
    let other_group = other.contains(duration_line) && !other.contains("Cooldown Remaining:");

    // An expired one draws no remaining either.
    c.world_mut()
        .player_qualities_mut()
        .expect("the description")
        .enchantments
        .cooldown_list[0]
        .duration = 0.0;
    assess(&mut c, &mut peer, POTION, &p);
    let expired = !item_text(&mut c).contains("Cooldown Remaining:");

    // The group on its own draws neither line: the duration is what opens the block.
    let mut no_duration = p.clone();
    no_duration.tables.floats = None;
    no_duration.flags &= !flags::FLOAT;
    assess(&mut c, &mut peer, POTION, &no_duration);
    let group_alone = !item_text(&mut c).contains("Cooldown");

    // ...and a duration of none is still a duration.
    let mut zero = p.clone();
    zero.tables.floats = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(DURATION, 0.0)],
    });
    zero.flags |= flags::FLOAT;
    assess(&mut c, &mut peer, POTION, &zero);
    let zero_duration = item_text(&mut c).contains("Cooldown When Used: 0s");

    c.assert_behaviour(
        "examine.cooldown.the-pane-says-how-long-it-is-and-how-long-is-left-of-the-players-own",
        move |_| {
            both_in_the_model
                && both_drawn
                && other_group
                && expired
                && group_alone
                && zero_duration
        },
    );
    c.shutdown();
}

/// The long description is decorated in place -- how well it is made in front of it, what it is
/// made of woven into it, what it is set with after it -- and a plating name replaces it outright.
pub(super) fn the_long_description_is_decorated_where_the_player_reads_it() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    /// The keys the decoration reads: which parts are known, the workmanship, the material, how
    /// many gems and which kind, and the plating name.
    const KNOWN: u32 = 0xAC;
    const WORKMANSHIP: u32 = 0x69;
    const MATERIAL: u32 = 0x83;
    const GEM_COUNT: u32 = 0xB1;
    const GEM_KIND: u32 = 0xB2;
    const PLATING_NAME: u32 = 0x34;

    let named = with_strings(&base, &[(LONG_DESC, "Steel sword")]);
    let decorated = with_ints(
        &named,
        &[
            (KNOWN, 7),
            (WORKMANSHIP, 3),
            (MATERIAL, 0x40),
            (GEM_COUNT, 2),
            (GEM_KIND, 0x26),
        ],
    );
    assess(&mut c, &mut peer, POTION, &decorated);
    let whole = item_text(&mut c).ends_with("Finely crafted Steel sword, set with 2 Rubies");

    let mut every_gem_reads = true;
    for (count, gem, expected) in [
        (1, 0x26, "Steel sword, set with 1 Ruby"),
        (1000, 0x26, "Steel sword, set with 1,000 Rubies"),
        (2, 0x20, "Steel sword, set with 2 pieces of Onyx"),
        (2, 0x1C, "Steel sword, set with 2 Lapis Lazuli"),
    ] {
        let p = with_ints(
            &decorated,
            &[(KNOWN, 4), (GEM_COUNT, count), (GEM_KIND, gem)],
        );
        assess(&mut c, &mut peer, POTION, &p);
        every_gem_reads &= item_text(&mut c).ends_with(expected);
    }

    // The material survives when the bit that nominally guards it is clear...
    let no_bits = with_ints(&decorated, &[(KNOWN, 0)]);
    assess(&mut c, &mut peer, POTION, &no_bits);
    let material_survives = item_text(&mut c).ends_with("Steel sword");

    // ...and a description the material's name is not already in is not trimmed by the miss.
    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&no_bits, &[(LONG_DESC, "  sword  ")]),
    );
    let miss_does_not_trim = item_text(&mut c).ends_with("Steel   sword  ");

    let plated = with_strings(
        &base,
        &[
            (LONG_DESC, "old description"),
            (PLATING_NAME, "Plated sword"),
        ],
    );
    assess(&mut c, &mut peer, POTION, &plated);
    let plating_replaces = item_text(&mut c).ends_with("Plated sword");

    // The short one, when it is what is used, is used undecorated and unplated.
    let short = with_strings(
        &decorated,
        &[
            (SHORT_DESC, "short fallback"),
            (PLATING_NAME, "Plated sword"),
        ],
    );
    assess(&mut c, &mut peer, POTION, &short);
    let short_is_bare = item_text(&mut c).ends_with("short fallback");

    c.assert_behaviour(
        "examine.description.the-long-one-is-decorated-and-a-plating-name-replaces-it",
        move |_| {
            whole
                && every_gem_reads
                && material_survives
                && miss_does_not_trim
                && plating_replaces
                && short_is_bare
        },
    );
    c.shutdown();
}

/// The short description is drawn only when there is no long one at all -- an **empty** long one
/// is still one, and suppresses it.
pub(super) fn the_short_description_is_used_only_when_there_is_no_long_one() {
    const SHORT: &str = "The short fallback reached the shipped description pane.";
    const LONG: &str = "The long description keeps priority.";

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(SHORT_DESC, SHORT)]),
    );
    let fallback_used = item_text(&mut c).contains(SHORT);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(LONG_DESC, LONG), (SHORT_DESC, SHORT)]),
    );
    let with_both = item_text(&mut c);
    let long_wins = with_both.contains(LONG) && !with_both.contains(SHORT);

    assess(
        &mut c,
        &mut peer,
        POTION,
        &with_strings(&base, &[(LONG_DESC, ""), (SHORT_DESC, SHORT)]),
    );
    let empty_still_suppresses = !item_text(&mut c).contains(SHORT);

    assess(&mut c, &mut peer, POTION, &with_strings(&base, &[]));
    let neither = {
        let t = item_text(&mut c);
        !t.contains(SHORT) && !t.contains(LONG)
    };

    c.assert_behaviour(
        "examine.description.the-short-one-is-drawn-only-when-there-is-no-long-one-at-all",
        move |_| fallback_used && long_wins && empty_still_suppresses && neither,
    );
    c.shutdown();
}

/// **A destroyed portal.** Assessing a destroyed portal draws both of its restriction sentences
/// and none of the other three, each restriction draws its own sentence and only its own, they
/// come in the pane's own order, and each is drawn plain.
///
/// Three of the five sentences are tails of one another, so *which* one was drawn is a question
/// about whole lines: asking it with a substring search is a scenario that cannot fail for the
/// first of them.
pub(super) fn each_portal_restriction_draws_its_own_sentence_and_only_its_own() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let carries_none = base.tables.ints.as_ref().is_none_or(|t| {
        !t.entries
            .iter()
            .any(|(k, _)| *k == examination::PORTAL_BITMASK)
    });
    assert!(
        carries_none,
        "the corpus carries no portal restrictions, which is why this adds one"
    );

    // A plain thing first, so "the lines were already there" cannot pass for "the lines landed".
    let plain_before = item_text(&mut c);
    let not_a_portal =
        !plain_before.contains(NO_RECALL_LINE) && !plain_before.contains(NO_SUMMON_LINE);

    let portal = |mask: i32| with_ints(&base, &[(examination::PORTAL_BITMASK, mask)]);

    assess(&mut c, &mut peer, POTION, &portal(DESTROYED_PORTAL));
    let text = item_text(&mut c);
    let destroyed = text.contains(NO_RECALL_LINE)
        && text.contains(NO_SUMMON_LINE)
        && !text.contains(NO_PK_LINE)
        && !text.contains(NO_PK_LITE_LINE)
        && !text.contains(NO_NPK_LINE)
        && text.find(NO_RECALL_LINE) < text.find(NO_SUMMON_LINE);

    // Each bit alone.
    let mut each_alone = true;
    for (bit, literal) in examination::PORTAL_RESTRICTION_LINES {
        // The unrestricted bit is on in every one of the world's own values and has no sentence.
        assess(&mut c, &mut peer, POTION, &portal(0x01 | bit));
        let drawn: Vec<String> = item_text(&mut c).lines().map(str::to_owned).collect();
        let want = literal.trim_end_matches('\n');
        each_alone &= drawn.iter().any(|l| l == want);
        for (other, other_literal) in examination::PORTAL_RESTRICTION_LINES {
            if other == bit {
                continue;
            }
            let unwanted = other_literal.trim_end_matches('\n');
            each_alone &= !drawn.iter().any(|l| l == unwanted);
        }
        each_alone &= drawn
            .iter()
            .filter(|l| l.ends_with("this portal.") || l.starts_with("This portal"))
            .count()
            == 1;
    }

    // All five at once: the pane's own order, and every one of them plain.
    assess(
        &mut c,
        &mut peer,
        POTION,
        &portal(0x01 | 0x02 | 0x04 | 0x08 | 0x10 | 0x20),
    );
    let all = item_text(&mut c);
    let mut at = 0_usize;
    let mut in_order = true;
    for (_, literal) in examination::PORTAL_RESTRICTION_LINES {
        let want = literal.trim_end_matches('\n');
        match all[at..].find(want) {
            Some(found) => at += found + want.len(),
            None => in_order = false,
        }
    }
    let plain = description_plain(&mut c);
    let w = exam_window(&mut c);
    let pane = exam_find(&mut c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let glyphs = painted(&mut c, pane);
    let all_plain = [
        NO_PK_LINE,
        NO_PK_LITE_LINE,
        NO_NPK_LINE,
        NO_RECALL_LINE,
        NO_SUMMON_LINE,
    ]
    .into_iter()
    .all(|line| colors_of(&glyphs, line) == vec![plain]);

    c.assert_behaviour(
        "examine.portal.each-restriction-draws-its-own-sentence-and-only-its-own",
        move |_| not_a_portal && destroyed && each_alone && in_order && all_plain,
    );
    c.shutdown();
}

/// The block's own denominator: a portal with no restriction at all still gains the two
/// separators the block always writes, and a thing that is not a portal gains neither.
///
/// Exact text, because that is what makes the separators assertable at all.
pub(super) fn a_portal_with_no_restrictions_still_gets_the_blocks_separators() {
    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let without = item_text(&mut c);
    let portal = |mask: i32| with_ints(&base, &[(examination::PORTAL_BITMASK, mask)]);

    assess(&mut c, &mut peer, POTION, &portal(0x01));
    let separators_only = item_text(&mut c) == format!("{without}\n\n");

    assess(&mut c, &mut peer, POTION, &portal(DESTROYED_PORTAL));
    let with_two =
        item_text(&mut c) == format!("{without}\n\n{NO_RECALL_LINE}\n{NO_SUMMON_LINE}\n");

    assess(&mut c, &mut peer, POTION, &base.clone());
    let block_skipped = item_text(&mut c) == without;

    c.assert_behaviour(
        "examine.portal.a-portal-with-no-restriction-still-gains-the-blocks-two-separators",
        move |_| separators_only && with_two && block_skipped,
    );
    c.shutdown();
}

/// **A failed assessment, and a successful one.** An assessment that failed draws all
/// nine value cells in the unknown colour and leaves every label alone -- and the *successful*
/// assessment of the recorded player was wrong the same way, because the nine cells the shard
/// marked raised were drawn white too.
///
/// The colour array is read off the shipped row first: the value cell authors **four** colours
/// where the item description pane authors three, and the fourth is the one the failure arm
/// indexes. Without it the failure would have nothing to index and would leave the cell as it was.
pub(super) fn a_failed_assessment_draws_every_value_cell_in_the_unknown_colour() {
    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    let palette = value_cell_colors(&mut c);
    let four_colours = palette == vec![0xFFFF_FFFF_u32, 0xFF00_FF00, 0xFFFF_0000, 0xFFFF_FF00]
        && palette[usize::from(examination::UNKNOWN_FONT)] == 0xFFFF_FF00;

    let succeeded = base.success_flag != 0;
    // The recording's own answer: all nine stats enchanted and all nine raised.
    let recorded_bits = base
        .creature_profile
        .clone()
        .and_then(|p| p.enchantment_bitfield)
        == Some(0x01FF_01FF);
    let all_raised = creature_row_colors(&mut c) == vec![examination::MOD_HIGH_FONT; 9];
    let raised = palette[usize::from(examination::MOD_HIGH_FONT)];
    let unknown = palette[usize::from(examination::UNKNOWN_FONT)];
    let raised_drawn = drawn_stat_rows(&mut c)
        .into_iter()
        .all(|(_, _, _, colour)| colour == raised);

    let mut failed = base.clone();
    failed.success_flag = 0;
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &failed);

    let all_unknown = creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];
    let rows = drawn_stat_rows(&mut c);
    let nine_rows = rows.len() == 9;
    let values_unknown_labels_not = rows.iter().all(|(_, label_colour, _, value_colour)| {
        *value_colour == unknown && *label_colour != unknown
    });

    c.assert_behaviour(
        "examine.failed-assess.every-value-cell-takes-the-unknown-colour-and-no-label-does",
        move |_| {
            four_colours
                && succeeded
                && recorded_bits
                && all_raised
                && raised_drawn
                && all_unknown
                && nine_rows
                && values_unknown_labels_not
        },
    );
    c.shutdown();
}

/// The **words** on a failed assessment are chosen separately from the colour, and the two do not
/// line up the way *"the ??? values"* suggests: the six attributes keep whatever numbers the
/// shard already described, health falls back to a bare percentage, and only stamina and mana
/// read as unknown -- while a body the shard described nothing of is unknown nine times.
pub(super) fn a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage() {
    use dereth_protocol::types::appraisal::CreatureAppraisalProfile;

    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);
    let creature = base
        .creature_profile
        .clone()
        .expect("the recorded answer carries a creature");
    let described = creature.attributes.is_some();

    let mut failed = base.clone();
    failed.success_flag = 0;
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &failed);

    let rows = creature_rows(&mut c);
    let nine = rows.len() == 9;
    let six_keep_their_numbers = rows.iter().take(6).all(|(_, value)| value != "???");
    let health_is_a_percentage = rows[6].1.ends_with(" %") && !rows[6].1.contains('/');
    let the_other_two = rows[7].1 == "???" && rows[8].1 == "???";

    // The same answer with the creature block emptied. **The block itself has to stay**: the
    // pane forks on whether the thing is a creature at all, so an answer with no block is an
    // item pane and would leave these nine cells showing the last creature.
    let mut bare = failed.clone();
    if let Some(cp) = bare.creature_profile.as_mut() {
        cp.flags &= !CreatureAppraisalProfile::HAS_ATTRIBUTES;
        cp.attributes = None;
        cp.health = 0;
        cp.max_health = 0;
    }
    assess(&mut c, &mut peer, APPRAISED_PLAYER, &bare);
    let bare_rows = creature_rows(&mut c);
    let nine_unknowns = bare_rows.iter().filter(|(_, v)| v == "???").count() == 9;
    let still_the_failure_colour =
        creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];

    c.assert_behaviour(
        "examine.failed-assess.the-numbers-the-shard-described-are-kept-and-health-is-a-percentage",
        move |_| {
            described
                && nine
                && six_keep_their_numbers
                && health_is_a_percentage
                && the_other_two
                && nine_unknowns
                && still_the_failure_colour
        },
    );
    c.shutdown();
}

/// A stat that is both enchanted and unassessed reads unknown and not enchanted: the failure is
/// decided before the question about enchantment is asked at all.
///
/// The same stats on a successful assessment take the enchantment colours, which is what makes
/// the first half a precedence and not an accident.
pub(super) fn the_failure_colour_outranks_the_enchantment_colour() {
    use dereth_protocol::types::appraisal::CreatureAppraisalProfile;

    let (mut c, mut peer, base) = assessing_the_recorded(ARMOUR_SESSION, APPRAISED_PLAYER);

    // Strength enchanted and raised, endurance enchanted and lowered, the maximum health raised.
    let bits = 0x1 | 0x1_0000 | 0x2 | 0x40 | 0x40_0000;
    let enchanted = |success: u32| {
        let mut p = base.clone();
        p.success_flag = success;
        if let Some(cp) = p.creature_profile.as_mut() {
            cp.flags |= CreatureAppraisalProfile::HAS_ENCHANTMENTS;
            cp.enchantment_bitfield = Some(bits);
        }
        p
    };

    assess(&mut c, &mut peer, APPRAISED_PLAYER, &enchanted(1));
    let ok = creature_row_colors(&mut c);
    let succeeded = ok[0] == examination::MOD_HIGH_FONT
        && ok[1] == examination::MOD_LOW_FONT
        // Not enchanted at all, so no colour of any kind.
        && ok[2] == 0
        // Health asks about its *maximum*, which is the one that was raised.
        && ok[6] == examination::MOD_HIGH_FONT;

    assess(&mut c, &mut peer, APPRAISED_PLAYER, &enchanted(0));
    let failed = creature_row_colors(&mut c) == vec![examination::UNKNOWN_FONT; 9];

    c.assert_behaviour(
        "examine.failed-assess.an-unassessed-stat-reads-unknown-even-when-it-is-enchanted",
        move |_| succeeded && failed,
    );
    c.shutdown();
}

/// The two places a failure does **not** colour, asserted so that a later change cannot quietly
/// widen the first: the item pane's own unknown value line is drawn plain.
pub(super) fn a_failed_assessment_leaves_the_item_panes_value_line_plain() {
    use dereth_protocol::types::appraisal::flags;

    let (mut c, mut peer, base) = assessing_the_recorded(EXAMINE_SESSION, POTION);
    let mut failed = base.clone();
    failed.success_flag = 0;
    failed.tables.ints = None;
    failed.flags &= !flags::INT;
    assess(&mut c, &mut peer, POTION, &failed);

    let starts_unknown = item_text(&mut c).starts_with("Value: ???");
    let plain = description_plain(&mut c);
    let w = exam_window(&mut c);
    let pane = exam_find(&mut c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane");
    let glyphs = painted(&mut c, pane);
    let drawn_plain = colors_of(&glyphs, "Value: ???") == vec![plain];

    c.assert_behaviour(
        "examine.failed-assess.the-item-panes-own-unknown-value-line-is-drawn-plain",
        move |_| starts_unknown && drawn_plain,
    );
    c.shutdown();
}

// =============================================================================================
// examine.scroll.*, examine.enchanted.*
//
// Two things about the assessment window: the description pane's scrollbar, and the enchantment
// highlighting.
//
// **No recorded assessment in the corpus carries an enchantment bitfield at all**, so the
// highlighting scenario uses a built answer. Everything from the datagram onward is the client's
// own path.
// =============================================================================================

/// The scrollbar the description pane names, the plate behind it, the arrow at the bottom of the
/// bar and the thumb inside it.
const DESC_SCROLLBAR: ElementId = ElementId(0x1000_013D);
const DESC_SCROLLBAR_PLATE: ElementId = ElementId(0x1000_05F8);
const SCROLL_ARROW_DOWN: ElementId = ElementId(0x1000_0071);
const SCROLL_THUMB: ElementId = ElementId(1);

/// The thing these scenarios assess, and the last line of the description they give it: it sits
/// below the bottom edge of the pane and only a scroll brings it inside.
const GAUNTLETS_SUBJECT: ObjectId = ObjectId(0x7DA5_5047);
const MARKER: &str = "THE LAST LINE";

/// The three colours the shipped description pane declares.
const PLAIN: u32 = 0xFFFF_FFFF;
const RAISED: u32 = 0xFF00_FF00;
const LOWERED: u32 = 0xFFFF_0000;

/// A client, a shard, and a thing of the scenario's own to assess.
fn a_client_and_a_thing_to_assess() -> (HeadlessClient, dereth_testkit::Peer) {
    let (mut c, mut peer) = a_client_and_a_shard();
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: GAUNTLETS_SUBJECT,
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
    c.world_mut()
        .weenie_mut(GAUNTLETS_SUBJECT)
        .expect("the shard created the subject")
        .pwd
        .name = "Bronze Gauntlets".to_owned();
    (c, peer)
}

/// A whole numbers table, as the decoder produces one.
fn int_table(entries: Vec<(u32, i32)>) -> dereth_protocol::archive::PackedHash<u32, i32> {
    dereth_protocol::archive::PackedHash {
        table_size: 16,
        entries,
    }
}

/// An assessment whose description is longer than the pane, ending in [`MARKER`].
///
/// The lines are short on purpose: an answer whose string table does not fit one fragment is
/// refused by the wire, which is a thing this scenario would rather not be about.
fn a_long_description() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut body = String::new();
    for i in 1..=24 {
        body.push_str(&format!("Line {i:02}.\n"));
    }
    body.push_str(MARKER);
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50)]));
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x10, body)],
    });
    p
}

/// The same thing with one line of description: the negative control for every claim about the
/// bar.
fn a_short_description() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50)]));
    p.flags |= flags::STRING;
    p.tables.strings = Some(dereth_protocol::archive::PackedHash {
        table_size: 8,
        entries: vec![(0x10, "A pair of gauntlets.".to_owned())],
    });
    p
}

/// A piece of armour whose armour level was **raised** and whose slashing resistance was
/// **lowered**, with every one of its eight modifiers the same -- so the only thing separating the
/// three rows these claims read is which of them is enchanted.
fn enchanted_armour() -> AppraisalProfile {
    use dereth_protocol::types::appraisal::flags;
    /// Which bit says a thing is enchanted, and which says it was raised rather than lowered.
    const ARMOUR_LEVEL_ENCHANTED: u32 = 0x1;
    const ARMOUR_LEVEL_RAISED: u32 = 0x1_0000;
    const SLASH_ENCHANTED: u32 = 0x2;

    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    p.flags |= flags::INT;
    // The armour level is the block's own gate: nothing of it is drawn without one.
    p.tables.ints = Some(int_table(vec![(0x13, 200), (5, 50), (0x1C, 20)]));
    p.flags |= flags::ARMOR_PROFILE;
    p.armor_profile = Some(dereth_protocol::types::appraisal::ArmorProfile {
        mod_vs_slash: 1.0,
        mod_vs_pierce: 1.0,
        mod_vs_bludgeon: 1.0,
        mod_vs_cold: 1.0,
        mod_vs_fire: 1.0,
        mod_vs_acid: 1.0,
        mod_vs_electric: 1.0,
        mod_vs_nether: 1.0,
    });
    p.flags |= flags::ARMOR_ENCHANTMENT;
    p.armor_enchantment = Some(ARMOUR_LEVEL_ENCHANTED | ARMOUR_LEVEL_RAISED | SLASH_ENCHANTED);
    p
}

/// The description pane itself, and the bar beside it.
fn description_pane(c: &mut HeadlessClient) -> ElemHandle {
    let w = exam_window(c);
    exam_find(c, w, examination::ITEM_DISPLAY_TEXT).expect("the description pane is in the layout")
}

fn description_bar(c: &mut HeadlessClient) -> ElemHandle {
    let w = exam_window(c);
    exam_find(c, w, DESC_SCROLLBAR).expect("the bar is in the layout")
}

/// How tall the pane says its own content is, against how tall the pane is -- the two numbers the
/// bar's sizer compares to decide whether it has anything to do.
fn description_extent(c: &mut HeadlessClient) -> (i32, i32) {
    let h = description_pane(c);
    let app = c.app_mut();
    let view = app.ui().expect("the shell is up").ui.screen_box(h).height();
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    (
        ui.text_element_mut(h)
            .expect("a text element")
            .scroll
            .height,
        view,
    )
}

/// How far down the pane has been scrolled.
fn description_offset(c: &mut HeadlessClient) -> i32 {
    let h = description_pane(c);
    let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
    ui.text_element_mut(h).expect("a text element").scroll.y
}

/// Every character of the description whose composed cell lies **inside the pane's box** -- what a
/// player can actually read, with the scroll already applied.
fn readable_description(c: &mut HeadlessClient) -> String {
    let h = description_pane(c);
    let app = c.app_mut();
    let box_ = app.ui().expect("the shell is up").ui.screen_box(h);
    let ui = &mut app.ui_mut().expect("the shell is up").ui;
    let t = ui.text_element_mut(h).expect("a text element");
    t.compose(box_)
        .into_iter()
        .filter(|g| g.y >= box_.y0 && g.y < box_.y1)
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect()
}

/// **The scrollbar, from both sides.** A description that does not fit the pane shows the
/// bar; one that fits keeps it off the screen -- which is what makes the first half a measurement
/// and not a bar that is simply always there.
///
/// The bar is a real one and the strip beside it is only its backing plate: both are
/// read off the shipped layout first, so a failure in the layout reads as a failure in the layout.
pub(super) fn the_description_pane_shows_a_bar_only_when_its_text_does_not_fit() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_short_description());

    let w = exam_window(&mut c);
    let plate = exam_find(&mut c, w, DESC_SCROLLBAR_PLATE).expect("the plate is in the layout");
    let b = description_bar(&mut c);
    let pane = description_pane(&mut c);
    let (plate_is_a_static, bar_is_a_bar) = {
        let ui = &c.app_mut().ui().expect("the shell is up").ui;
        (
            ui.node(plate).expect("a node").ty().0 == 0x03,
            ui.node(b).expect("a node").ty().0 == 0x0B,
        )
    };
    let bound = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        ui.text_element_mut(pane)
            .expect("a text element")
            .scroll
            .v_scrollbar
            == Some(DESC_SCROLLBAR)
    };

    let (short_content, short_view) = description_extent(&mut c);
    let short_fits = short_content <= short_view;
    let hidden = !element_visible(&mut c, b);

    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_long_description());
    let (long_content, long_view) = description_extent(&mut c);
    let overflows = long_content > long_view;
    let long_bar = description_bar(&mut c);
    let shown = element_visible(&mut c, long_bar);

    c.assert_behaviour(
        "examine.scroll.the-description-pane-shows-a-bar-only-when-its-text-does-not-fit",
        move |_| {
            plate_is_a_static && bar_is_a_bar && bound && short_fits && hidden && overflows && shown
        },
    );
    c.shutdown();
}

/// **The gesture, both of the ways a player makes it.** A press on the arrow at the bottom of the
/// bar, and a drag of the thumb to the bottom of its track, each bring the end of a description
/// that overflows into view.
///
/// Each half begins by asserting that the last line is *not* readable, so an instrument pointed at
/// a pane that never overflowed reports that rather than passing.
pub(super) fn the_bar_really_scrolls_the_description() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &a_long_description());

    let before_arrow = readable_description(&mut c);
    let arrow = {
        let b = description_bar(&mut c);
        exam_find(&mut c, b, SCROLL_ARROW_DOWN).expect("the bar's own arrow")
    };
    let at = centre_of(&mut c, arrow);
    for _ in 0..40 {
        c.when(Player::Click(Target::Point(at)));
    }
    let arrow_offset = description_offset(&mut c);
    let after_arrow = readable_description(&mut c);

    // A fresh client for the drag: the pane this one is looking at is already at the bottom.
    let (mut c2, mut peer2) = a_client_and_a_thing_to_assess();
    assess(
        &mut c2,
        &mut peer2,
        GAUNTLETS_SUBJECT,
        &a_long_description(),
    );
    let before_drag = readable_description(&mut c2);
    let bar = description_bar(&mut c2);
    let thumb = exam_find(&mut c2, bar, SCROLL_THUMB).expect("the bar's thumb");
    let from = centre_of(&mut c2, thumb);
    let track = c2
        .app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .screen_box(bar);
    c2.when(Player::Drag {
        from: Target::Point(from),
        to: Target::Point(ScreenPoint::new(from.x, track.y1 - 2)),
        hold_frames: 1,
    });
    let drag_offset = description_offset(&mut c2);
    let after_drag = readable_description(&mut c2);
    c2.shutdown();

    c.assert_behaviour(
        "examine.scroll.the-bar-brings-the-end-of-a-long-description-into-view",
        move |_| {
            !before_arrow.contains(MARKER)
                && arrow_offset > 0
                && after_arrow.contains(MARKER)
                && !before_drag.contains(MARKER)
                && drag_offset > 0
                && after_drag.contains(MARKER)
        },
    );
    c.shutdown();
}

/// **Highlighting.** A line about something the shard says was raised is drawn green,
/// one about something lowered red, and everything else plain -- three colours on one pane at
/// once, out of the pane's own array of three, and one colour when nothing is enchanted.
///
/// The array is read off the live element first, so the colours below are the shipped ones and not
/// numbers written down here.
pub(super) fn a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain() {
    let (mut c, mut peer) = a_client_and_a_thing_to_assess();
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &enchanted_armour());

    let pane = description_pane(&mut c);
    let (declared, fonts) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(pane).expect("a text element");
        (t.font_colors.clone(), t.fonts.clone())
    };
    let shipped = declared == vec![PLAIN, RAISED, LOWERED]
        && fonts == vec![dereth_primitives::DataId(0x4000_0001)];

    let glyphs = painted(&mut c, pane);
    let coloured = colors_of(&glyphs, "Armor Level: 20") == vec![RAISED]
        && colors_of(&glyphs, "Slashing: Average  (20)") == vec![LOWERED]
        // Neither bit is set for this one, so it is plain.
        && colors_of(&glyphs, "Piercing: Average  (20)") == vec![PLAIN]
        && colors_of(&glyphs, "Value: 200") == vec![PLAIN]
        && colors_of(&glyphs, "Burden: 50") == vec![PLAIN];
    let mut distinct: Vec<u32> = glyphs.iter().map(|(_, col)| *col).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let three_at_once = distinct.len() == 3;

    // The same armour with nothing enchanted: one colour, and the armour level plain.
    let mut plain = enchanted_armour();
    plain.flags &= !dereth_protocol::types::appraisal::flags::ARMOR_ENCHANTMENT;
    plain.armor_enchantment = None;
    assess(&mut c, &mut peer, GAUNTLETS_SUBJECT, &plain);
    let pane = description_pane(&mut c);
    let glyphs = painted(&mut c, pane);
    let mut distinct: Vec<u32> = glyphs.iter().map(|(_, col)| *col).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let one_when_nothing_is =
        distinct == vec![PLAIN] && colors_of(&glyphs, "Armor Level: 20") == vec![PLAIN];

    c.assert_behaviour(
        "examine.enchanted.a-raised-line-is-green-a-lowered-one-red-and-everything-else-plain",
        move |_| shipped && coloured && three_at_once && one_when_nothing_is,
    );
    c.shutdown();
}

// =============================================================================================
// examine.window.*
//
// The identify button raises the identify window: a chain of four links -- click, question,
// answer, pane -- and a window that never opens can be missing any one of them.
//
// Every recorded assessment filling the pane from its own body is what the twenty-odd `examine.*`
// scenarios above this one cover, each with one recorded answer and one claim.
// =============================================================================================

/// The second recorded thing in the assessment recording, so that a claim about *which* object the
/// pane was waiting for has two to choose between.
const OTHER_SUBJECT: ObjectId = ObjectId(0x8000_09B4);

/// Whether the assessment window is on the screen.
fn exam_window_visible(c: &mut HeadlessClient) -> bool {
    let h = exam_window(c);
    c.app_mut()
        .ui()
        .expect("the shell is up")
        .ui
        .node(h)
        .expect("a live node")
        .region
        .flags
        .visible
}

/// Put the window away, as the player closing it does.
fn close_exam_window(c: &mut HeadlessClient) {
    let h = exam_window(c);
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .set_visible(h, false);
}

/// Create one recorded thing in this client's world and hand back the answer the shard recorded
/// about it -- without asking about it, which is what these two claims are about.
fn a_recorded_thing(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    session: &str,
    object: ObjectId,
) -> AppraisalProfile {
    let (create, _blob, profile) = recorded_object(session, object);
    peer.send(c, dereth_testkit::replay::OBJECT_QUEUE, create);
    c.tick(1);
    assert!(
        c.view().world().weenie(object).is_some(),
        "the recorded create is what puts {object:?} in this client's world"
    );
    profile
}

/// Tell the pane it is waiting for `object`, the way the request does.
pub(super) fn await_the_answer_for(c: &mut HeadlessClient, object: ObjectId) {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen.examination.examine_object(object);
}

/// Deliver one assessment through the shard, without asking for it first.
pub(super) fn deliver_the_answer(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    object: ObjectId,
    profile: &AppraisalProfile,
) {
    peer.event(
        c,
        &dereth_protocol::objects::ItemSetAppraiseInfo {
            object,
            profile: profile.clone(),
        },
    );
    c.tick(2);
}

/// **The guard, both ways.** An answer the pane never asked for opens nothing, an answer about a
/// different thing from the one it asked about opens nothing -- and an answer about the *same*
/// thing, arriving again after the player has put the window away, refills it without putting it
/// back on the screen.
///
/// The last half is the one that needs saying: while something is being fought the client asks
/// again about four times a second, and a pane that reopened on each answer would fight the player
/// for the screen.
pub(super) fn an_answer_the_pane_did_not_ask_for_opens_nothing() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let first = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, POTION);
    let second = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, OTHER_SUBJECT);

    // Nothing was asked for at all.
    deliver_the_answer(&mut c, &mut peer, POTION, &first);
    let applied_with_nothing_asked = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.replies_applied
    };
    let down_with_nothing_asked = !exam_window_visible(&mut c);

    // A different thing was asked about.
    await_the_answer_for(&mut c, OTHER_SUBJECT);
    deliver_the_answer(&mut c, &mut peer, POTION, &first);
    let applied_with_the_wrong_one = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.replies_applied
    };
    let down_with_the_wrong_one = !exam_window_visible(&mut c);

    // The one it did ask about.
    deliver_the_answer(&mut c, &mut peer, OTHER_SUBJECT, &second);
    let up = exam_window_visible(&mut c);
    let (opened, current, awaiting) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.opened,
            screen.examination.current,
            screen.examination.awaiting,
        )
    };

    // The player puts it away, and the same answer comes round again.
    close_exam_window(&mut c);
    c.tick(1);
    deliver_the_answer(&mut c, &mut peer, OTHER_SUBJECT, &second);
    let (refilled, still_opened_once) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.replies_applied,
            screen.examination.opened,
        )
    };
    let stayed_down = !exam_window_visible(&mut c);

    c.assert_behaviour(
        "examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it",
        move |_| {
            applied_with_nothing_asked == 0
                && down_with_nothing_asked
                && applied_with_the_wrong_one == 0
                && down_with_the_wrong_one
                && up
                && opened == 1
                && current == Some(OTHER_SUBJECT)
                // The question is consumed by the answer to it.
                && awaiting.is_none()
                && refilled == 2
                && still_opened_once == 1
                && stayed_down
        },
    );
    c.shutdown();
}

/// **The click that starts the chain.** Pressing the identify button with something selected is
/// what makes the pane wait for that thing's answer -- the middle link a window that never opens
/// is most likely missing -- and the answer then opens the window: click, question, answer, pane.
pub(super) fn the_identify_button_is_what_starts_the_chain() {
    let (mut c, mut peer) = a_client_and_a_shard();
    let profile = a_recorded_thing(&mut c, &mut peer, EXAMINE_SESSION, POTION);

    let nothing_awaited_before = {
        let (_, screen) = gameplay_screen(c.app_mut());
        screen.examination.awaiting.is_none()
    };

    // The real gesture: the thing is under the pointer and the shipped button is pressed.
    examine(&mut c, POTION);

    let (awaited, asked) = {
        let (_, screen) = gameplay_screen(c.app_mut());
        (
            screen.examination.awaiting,
            screen.examination.replies_applied,
        )
    };
    let still_down = !exam_window_visible(&mut c);

    deliver_the_answer(&mut c, &mut peer, POTION, &profile);
    let up = exam_window_visible(&mut c);

    c.assert_behaviour(
        "examine.window.the-identify-button-is-what-makes-the-pane-wait-for-an-answer",
        move |_| {
            nothing_awaited_before && awaited == Some(POTION) && asked == 0 && still_down && up
        },
    );
    c.shutdown();
}
