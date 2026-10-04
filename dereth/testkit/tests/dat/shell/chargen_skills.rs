//! Shell fixtures and scenarios for chargen skills.

use super::*;
use dereth_ui_screens::screens::chargen;
// =============================================================================================
// chargen.skills.* -- the wizard's skills page
//
// Five scenarios, five rows, and none of them is a transcription -- every expected name, score and
// price below is read out of the shipped skill table rather than written down, and the last one is
// the page compared against the original client's own drawing of the same character.
// =============================================================================================

/// What one entry of the skills list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillEntry {
    /// One of the four category rows, by its place in the page's own order.
    Heading(usize),
    /// A skill row, by the skill it carries.
    Row(u32),
}

/// The list in the order it is drawn, top to bottom.
fn skill_items(c: &mut HeadlessClient) -> Vec<dereth_ui::ElemHandle> {
    with_wizard(c, |_, w| {
        w.skill_list
            .as_ref()
            .expect("the skills list is bound")
            .items
            .clone()
    })
}

fn skill_entries(c: &mut HeadlessClient) -> Vec<(dereth_ui::ElemHandle, SkillEntry)> {
    let headings = with_wizard(c, |_, w| w.skill_headers.clone());
    skill_items(c)
        .into_iter()
        .map(|h| {
            if let Some(i) = headings.iter().position(|x| *x == h) {
                return (h, SkillEntry::Heading(i));
            }
            let node = c
                .view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .node(h)
                .expect("a live list item");
            match node.instance_properties.get(ATTR_ROW_SKILL_ID) {
                Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => (h, SkillEntry::Row(*v)),
                other => panic!("a list item that is neither a heading nor a skill row: {other:?}"),
            }
        })
        .collect()
}

/// One cell of a row, by the child the page binds.
fn skill_cell(c: &mut HeadlessClient, row: dereth_ui::ElemHandle, id: ElementId) -> String {
    let h = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, id)
        .unwrap_or_else(|| panic!("the skill row template has no {id:?}"));
    wizard_text(c, h)
}

/// The state of one of a row's arrows.
fn arrow_state(
    c: &HeadlessClient,
    row: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::StateId {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let h = shell
        .ui
        .get_child_recursive(row, id)
        .expect("the row's arrow");
    shell.ui.node(h).expect("a live node").state
}

/// A row's two prices and whether either move is possible at all, after the people's own
/// overrides -- written out here so that deleting the page's own copy cannot make a scenario
/// pass by agreeing with itself.
fn skill_prices(
    cg: &dereth_assets::tables::CharGen,
    sk: &dereth_assets::tables::SkillTable,
    heritage: u32,
    id: u32,
) -> (i32, i32, bool, bool) {
    let base = sk.skills.get(&id).expect("the skill is in the table");
    let mut train = base.trained_cost;
    let mut spec = base.specialized_cost;
    let mut untrainable = train != 0;
    let mut unspecializable = spec != 0;
    if let Some(h) = cg.heritage_groups.get(&heritage) {
        if let Some((_, normal, primary)) = h.skills.iter().find(|(s, _, _)| *s == id) {
            if *normal == 0 {
                spec -= train;
                train = 0;
                untrainable = false;
            }
            if *primary == 0 {
                spec = 0;
                unspecializable = false;
            }
        }
    }
    (train, spec, untrainable, unspecializable)
}

/// Which heading a skill belongs under, from its level and whether it can be used untrained.
fn heading_for(level: SkillAdvancementClass, min_level: u32) -> usize {
    match level {
        SkillAdvancementClass::Specialized => 0,
        SkillAdvancementClass::Trained => 1,
        SkillAdvancementClass::Untrained if min_level < 2 => 2,
        SkillAdvancementClass::Untrained => 3,
        SkillAdvancementClass::Inactive => panic!("a skill with no row has no heading"),
    }
}

/// Aluvian, the profession at `profession`, then the skills page.
fn on_the_skills_page(profession: usize) -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click_wizard(&mut c, chargen::PROFESSION_BUTTONS[profession].0);
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    assert_eq!(with_wizard(&mut c, |_, w| w.progress), EcgProgress::Skills);
    c
}

/// The four headings, in the page's own order.
const SKILL_HEADINGS: [&str; 4] = [
    "Specialized Skills",
    "Trained Skills",
    "Useable Untrained Skills",
    "Unuseable Untrained Skills",
];

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-four-headings-sit-above-their-own-groups
// ---------------------------------------------------------------------------------------------

/// Heading, group, heading, group -- and each group in name order.
pub(super) fn the_four_headings_sit_above_their_own_groups() {
    let mut c = on_the_skills_page(5);
    let (_, sk) = chargen_tables(&c);

    let entries = skill_entries(&mut c);
    let whole_table = entries.len() > 40;

    let heading_at: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, (_, e))| matches!(e, SkillEntry::Heading(_)).then_some(i))
        .collect();
    let four_in_order = heading_at.len() == 4
        && heading_at.iter().enumerate().all(|(n, i)| entries[*i].1 == SkillEntry::Heading(n))
        && heading_at[0] == 0
        // The whole finding in one line: they are **not** the first four items.
        && heading_at != vec![0, 1, 2, 3];

    let headings = with_wizard(&mut c, |_, w| w.skill_headers.clone());
    let named = headings
        .iter()
        .zip(SKILL_HEADINGS)
        .all(|(h, want)| skill_cell(&mut c, *h, chargen::skills_page::HEADER_TEXT) == want);

    // Every row sits under the heading its level and its own usability name.
    let mut group = usize::MAX;
    let mut counts = [0_usize; 4];
    let mut under_the_right_one = true;
    for (_, e) in &entries {
        match e {
            SkillEntry::Heading(i) => group = *i,
            SkillEntry::Row(id) => {
                let base = sk
                    .skills
                    .get(id)
                    .expect("the row names a skill in the table");
                let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
                under_the_right_one &= group == heading_for(level, base.min_level);
                counts[group] += 1;
            }
        }
    }
    let populated = counts[0] >= 4 && counts[1] > 0 && counts[2] > 0;

    // And inside a group the rows are in name order.
    let mut prev: Option<&str> = None;
    let mut sorted = true;
    for (_, e) in &entries {
        match e {
            SkillEntry::Heading(_) => prev = None,
            SkillEntry::Row(id) => {
                let name = sk.skills[id].name.as_str();
                if let Some(p) = prev {
                    sorted &= p <= name;
                }
                prev = Some(name);
            }
        }
    }

    c.assert_behaviour("chargen.skills.the-four-headings-sit-above-their-own-groups-and-each-group-is-in-name-order", move |_| {
        whole_table && four_in_order && named && under_the_right_one && populated && sorted
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices
// ---------------------------------------------------------------------------------------------

/// The cell-by-cell reading a picture of the page cannot make: which string is in which box.
pub(super) fn every_row_shows_its_own_score_and_its_own_two_prices() {
    let mut c = on_the_skills_page(5);
    let (cg, sk) = chargen_tables(&c);

    let entries = skill_entries(&mut c);
    let heritage = with_wizard(&mut c, |_, w| w.state.heritage_group);
    let credits = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);

    let mut every_cell = true;
    let mut no_heading_text = true;
    let mut checked: Vec<u32> = Vec::new();
    for (h, e) in &entries {
        let SkillEntry::Row(id) = e else { continue };
        let base = sk
            .skills
            .get(id)
            .expect("the row names a skill in the table");
        let (train, spec, untrainable, unspecializable) = skill_prices(&cg, &sk, heritage, *id);
        let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
        let score = with_wizard(&mut c, |_, w| w.state.skill_score(&sk, *id));

        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_NAME) == base.name;
        // A number -- the score this skill would start at -- and not a level's name and certainly
        // not a heading's.
        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_LEVEL) == score.to_string();

        let printed = |v: i32| {
            if v < 999 {
                v.to_string()
            } else {
                String::new()
            }
        };
        let (up, down, up_on, down_on) = match level {
            SkillAdvancementClass::Untrained => {
                (printed(train), "0".to_string(), credits >= train, false)
            }
            SkillAdvancementClass::Trained => (
                printed(spec - train),
                train.to_string(),
                credits >= spec - train,
                untrainable,
            ),
            SkillAdvancementClass::Specialized => (
                "0".to_string(),
                (spec - train).to_string(),
                false,
                unspecializable,
            ),
            SkillAdvancementClass::Inactive => panic!("a skill with no row"),
        };
        every_cell &= skill_cell(&mut c, *h, chargen::skills_page::ROW_UP_COST) == up
            && skill_cell(&mut c, *h, chargen::skills_page::ROW_DOWN_COST) == down;
        // An arrow is lit exactly when the move it offers can be made.
        every_cell &= arrow_state(&c, *h, chargen::skills_page::ROW_INCREASE)
            == if up_on {
                STATE_ARROW_ON
            } else {
                STATE_ARROW_OFF
            }
            && arrow_state(&c, *h, chargen::skills_page::ROW_DECREASE)
                == if down_on {
                    STATE_ARROW_ON
                } else {
                    STATE_ARROW_OFF
                };

        // The defect this was reported as: a heading's words inside a row.
        for cell_id in [
            chargen::skills_page::ROW_NAME,
            chargen::skills_page::ROW_LEVEL,
            chargen::skills_page::ROW_UP_COST,
            chargen::skills_page::ROW_DOWN_COST,
        ] {
            let text = skill_cell(&mut c, *h, cell_id);
            no_heading_text &= !SKILL_HEADINGS.contains(&text.as_str());
        }
        checked.push(*id);
    }
    checked.sort_unstable();

    // Every skill the character really has a level for has exactly one row, and nothing else does.
    let mut want: Vec<u32> = with_wizard(&mut c, |_, w| {
        sk.skills
            .keys()
            .copied()
            .filter(|id| w.state.skill_level(*id) != SkillAdvancementClass::Inactive)
            .collect()
    });
    want.sort_unstable();
    let one_row_each = checked == want;

    c.assert_behaviour(
        "chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices",
        move |_| every_cell && no_heading_text && one_row_each,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest
// ---------------------------------------------------------------------------------------------

/// Which group a row sits in now.
fn group_of_row(c: &mut HeadlessClient, row: dereth_ui::ElemHandle) -> Option<usize> {
    let mut group = None;
    for (h, e) in skill_entries(c) {
        match e {
            SkillEntry::Heading(i) => group = Some(i),
            SkillEntry::Row(_) if h == row => return group,
            SkillEntry::Row(_) => {}
        }
    }
    None
}

/// Training a skill moves its row, and re-prices every other row.
pub(super) fn training_a_skill_moves_its_row_and_re_prices_the_rest() {
    // The hand-built profession leaves credits to spend; the one the last scenario used has none.
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);

    // An affordable untrained skill, chosen off the character rather than by name.
    let (skill, row, cost) = with_wizard(&mut c, |_, w| {
        let credits = w.state.remaining_skill_credits;
        let r = w
            .skill_rows
            .iter()
            .find(|r| {
                r.level == SkillAdvancementClass::Untrained
                    && sk.skills[&r.skill].min_level < 2
                    && r.train_cost > 0
                    && r.train_cost <= credits
            })
            .expect("an affordable, useable untrained skill");
        (r.skill, r.element.expect("its row"), r.train_cost)
    });
    let starts_untrained = group_of_row(&mut c, row) == Some(2);

    let plus = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, chargen::skills_page::ROW_INCREASE)
        .expect("the row's + arrow");
    let credits_before = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let headings = with_wizard(&mut c, |_, w| w.skill_headers.clone());
    press_handle(&mut c, plus);

    let trained = with_wizard(&mut c, |_, w| w.state.skill_level(skill))
        == SkillAdvancementClass::Trained
        && with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == credits_before - cost;
    let moved_in_the_list = group_of_row(&mut c, row) == Some(1);

    // **And in the frame, not only in the list.** A client that re-ordered what it holds and drew
    // the old picture would pass everything above.
    c.tick(4);
    let moved_on_the_screen = {
        let hs = skill_items(&mut c);
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let boxes: Vec<dereth_ui::region::Box2D> = hs
            .iter()
            .map(|h| shell.ui.node(*h).expect("a live item").region.box_)
            .collect();
        let mut y = boxes[0].y0;
        let mut stacked = true;
        for b in &boxes {
            stacked &= b.y0 == y;
            y = b.y1 + 1;
        }
        let drawn = shell.ui.node(row).expect("the trained row").region.box_.y0;
        let trained_heading = shell
            .ui
            .node(headings[1])
            .expect("the trained heading")
            .region
            .box_
            .y0;
        let untrained_heading = shell
            .ui
            .node(headings[2])
            .expect("the untrained heading")
            .region
            .box_
            .y0;
        stacked && drawn > trained_heading && drawn < untrained_heading
    };

    // Still in name order in its new group, and its own cells followed it.
    let mut group = usize::MAX;
    let mut prev: Option<&str> = None;
    let mut sorted = true;
    for (_, e) in &skill_entries(&mut c) {
        match e {
            SkillEntry::Heading(i) => {
                group = *i;
                prev = None;
            }
            SkillEntry::Row(id) => {
                let name = sk.skills[id].name.as_str();
                if let Some(p) = prev {
                    sorted &= p <= name;
                }
                prev = Some(name);
            }
        }
    }
    let _ = group;
    let score = with_wizard(&mut c, |_, w| w.state.skill_score(&sk, skill));
    let cells_followed = skill_cell(&mut c, row, chargen::skills_page::ROW_LEVEL)
        == score.to_string()
        && skill_cell(&mut c, row, chargen::skills_page::ROW_DOWN_COST) == cost.to_string();

    // The re-pricing. **One purchase is not enough to see it**: with plenty of credits left
    // nothing else becomes unaffordable, so the sweep could be missing and every reading above
    // would still hold. Spend down until at least one row really is out of reach.
    let mut bought = 1_usize;
    loop {
        let (credits, next) = with_wizard(&mut c, |_, w| {
            let credits = w.state.remaining_skill_credits;
            let next = w.skill_rows.iter().position(|r| {
                r.level == SkillAdvancementClass::Untrained
                    && r.train_cost > 0
                    && r.train_cost <= credits
            });
            (credits, next)
        });
        let priced_out = with_wizard(&mut c, |_, w| {
            w.skill_rows
                .iter()
                .any(|r| r.level == SkillAdvancementClass::Untrained && r.train_cost > credits)
        });
        if priced_out {
            break;
        }
        let Some(n) = next else { break };
        let h = with_wizard(&mut c, |_, w| w.skill_rows[n].element.expect("a row"));
        let plus = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(h, chargen::skills_page::ROW_INCREASE)
            .expect("the + arrow");
        press_handle(&mut c, plus);
        bought += 1;
        assert!(
            bought < 60,
            "the credits never ran low enough to price a skill out"
        );
    }
    let credits = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let something_is_out_of_reach = with_wizard(&mut c, |_, w| {
        w.skill_rows
            .iter()
            .any(|r| r.level == SkillAdvancementClass::Untrained && r.train_cost > credits)
    });

    let mut re_priced = true;
    for (h, e) in &skill_entries(&mut c) {
        let SkillEntry::Row(id) = e else { continue };
        let (train, spec) = with_wizard(&mut c, |_, w| {
            let r = w
                .skill_rows
                .iter()
                .find(|r| r.skill == *id)
                .expect("a record");
            (r.train_cost, r.spec_cost)
        });
        let level = with_wizard(&mut c, |_, w| w.state.skill_level(*id));
        let want = match level {
            SkillAdvancementClass::Untrained => credits >= train,
            SkillAdvancementClass::Trained => credits >= spec - train,
            _ => false,
        };
        re_priced &= arrow_state(&c, *h, chargen::skills_page::ROW_INCREASE)
            == if want {
                STATE_ARROW_ON
            } else {
                STATE_ARROW_OFF
            };
    }

    c.assert_behaviour("chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest", move |_| {
        starts_untrained
            && trained
            && moved_in_the_list
            && moved_on_the_screen
            && sorted
            && cells_followed
            && something_is_out_of_reach
            && re_priced
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps
// ---------------------------------------------------------------------------------------------

/// One column, each item starting where the one above it ends.
pub(super) fn the_skill_rows_are_stacked_and_none_overlaps() {
    let mut c = on_the_skills_page(5);
    let entries = skill_entries(&mut c);
    let boxes: Vec<dereth_ui::region::Box2D> = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        entries
            .iter()
            .map(|(h, _)| shell.ui.node(*h).expect("a live item").region.box_)
            .collect()
    };
    let whole_table = boxes.len() > 40;

    let mut y = boxes[0].y0;
    let mut stacked = true;
    for b in &boxes {
        stacked &= b.x0 == boxes[0].x0 && b.y0 == y && b.y1 > b.y0;
        y = b.y1 + 1;
    }
    // The row template's own height, which is the pitch the original draws at -- not the flat
    // number the page used to place its rows by hand at.
    let pitch = boxes[0].y1 - boxes[0].y0 + 1;
    let the_templates_pitch = pitch == 26;

    c.assert_behaviour(
        "chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps",
        move |_| whole_table && stacked && the_templates_pitch,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character
// ---------------------------------------------------------------------------------------------

/// The twelve items the original client draws for this character, before the list scrolls.
///
/// Each is (name, score, the price to go up, the price to go down, the up arrow lit, the down
/// arrow lit). Nothing here is derived from this client: they were read off the original's own
/// picture of the same character, and this is what catches a row showing another skill's price,
/// which no whole-picture comparison would notice.
const AS_THE_ORIGINAL_DREW_IT: [(&str, &str, &str, &str, bool, bool); 12] = [
    ("Specialized Skills", "", "", "", false, false),
    ("Dirty Fighting", "77", "0", "2", false, true),
    ("Heavy Weapons", "77", "0", "6", false, true),
    ("Melee Defense", "60", "0", "10", false, true),
    ("Shield", "110", "0", "2", false, true),
    ("Trained Skills", "", "", "", false, false),
    ("Arcane Lore", "8", "2", "0", false, false),
    ("Healing", "42", "4", "6", false, true),
    ("Jump", "105", "4", "0", false, false),
    ("Loyalty", "5", "2", "0", false, false),
    ("Magic Defense", "8", "12", "0", false, false),
    ("Missile Weapons", "55", "6", "6", false, true),
];

/// Item for item against the original's own drawing.
pub(super) fn the_skills_page_reads_the_way_the_original_drew_it() {
    let mut c = on_the_skills_page(5);

    // The original's own credit meter reads nothing left for this character.
    let meter = {
        let f = element(&c, chargen::skills_page::CREDITS_FIELD);
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .get_child_recursive(f, chargen::skills_page::READOUT_TEXT)
            .expect("the number")
    };
    let no_credits_left = wizard_text(&mut c, meter) == "0";

    let entries = skill_entries(&mut c);
    let mut item_for_item = true;
    for (i, want) in AS_THE_ORIGINAL_DREW_IT.iter().enumerate() {
        let (h, e) = entries[i];
        match e {
            SkillEntry::Heading(_) => {
                item_for_item &= skill_cell(&mut c, h, chargen::skills_page::HEADER_TEXT) == want.0;
            }
            SkillEntry::Row(_) => {
                let got = (
                    skill_cell(&mut c, h, chargen::skills_page::ROW_NAME),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_LEVEL),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_UP_COST),
                    skill_cell(&mut c, h, chargen::skills_page::ROW_DOWN_COST),
                    arrow_state(&c, h, chargen::skills_page::ROW_INCREASE) == STATE_ARROW_ON,
                    arrow_state(&c, h, chargen::skills_page::ROW_DECREASE) == STATE_ARROW_ON,
                );
                item_for_item &= (
                    got.0.as_str(),
                    got.1.as_str(),
                    got.2.as_str(),
                    got.3.as_str(),
                    got.4,
                    got.5,
                ) == *want;
            }
        }
    }

    c.assert_behaviour(
        "chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character",
        move |_| no_credits_left && item_for_item,
    );
    c.shutdown();
}

// =============================================================================================
// chargen.skills.* -- the row moves in the picture, not only in the list
//
// This sits beside the skills scenarios because they read the client's own list and say nothing
// about the screen: a live page can put the new price and the new arrows on a row and **leave the
// row where it was** while every list reading is right.
//
// The instrument's own calibration -- a reader that says the picture follows the list must also say
// so when it does not -- is not a row; it is folded in here as the premise of the scenario it
// calibrates.
//
// Every reading below is of a rectangle on the screen, and every one is taken after the frames
// the snap-back needed: the revert happened on the frame *after* the press, so a scenario that
// pressed and looked at once could not have seen it.
// =============================================================================================

/// A press with the frames the list's own per-frame pass needs after it.
fn press_and_settle(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(4);
}

/// Every item's own rectangle, in the order the client holds them.
fn skill_boxes(c: &mut HeadlessClient) -> Vec<dereth_ui::region::Box2D> {
    let hs = skill_items(c);
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    hs.iter()
        .map(|h| shell.ui.node(*h).expect("a live list item").region.box_)
        .collect()
}

fn top_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> i32 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("a live item")
        .region
        .box_
        .y0
}

/// Where the four headings are **drawn**.
fn heading_tops(c: &mut HeadlessClient) -> Vec<i32> {
    let hs = with_wizard(c, |_, w| w.skill_headers.clone());
    hs.iter().map(|h| top_of(c, *h)).collect()
}

/// Which group a row is **drawn** in: the last heading drawn above it.
///
/// Deliberately not the same question as which group the client's own list puts it in. This is
/// the one a player answers by looking at the screen.
fn drawn_group(c: &mut HeadlessClient, row: dereth_ui::ElemHandle) -> Option<usize> {
    let y = top_of(c, row);
    let mut found = None;
    for (i, t) in heading_tops(c).iter().enumerate() {
        if *t < y {
            found = Some(i);
        }
    }
    found
}

/// The picture follows the list: one column, no gaps, no overlaps, in the list's own order.
fn geometry_follows_the_list(c: &mut HeadlessClient) -> bool {
    let bs = skill_boxes(c);
    if bs.len() <= 40 {
        return false;
    }
    let mut y = bs[0].y0;
    let mut ok = true;
    for b in &bs {
        ok &= b.x0 == bs[0].x0 && b.y1 > b.y0 && b.y0 == y;
        y = b.y1 + 1;
    }
    ok
}

/// The list in the order it is drawn, top to bottom.
fn drawn_order(c: &mut HeadlessClient) -> Vec<(dereth_ui::ElemHandle, i32)> {
    let hs = skill_items(c);
    let mut v: Vec<(dereth_ui::ElemHandle, i32)> = hs.iter().map(|h| (*h, top_of(c, *h))).collect();
    v.sort_by_key(|(_, y)| *y);
    v
}

fn row_arrow(c: &HeadlessClient, row: dereth_ui::ElemHandle, up: bool) -> dereth_ui::ElemHandle {
    let id = if up {
        chargen::skills_page::ROW_INCREASE
    } else {
        chargen::skills_page::ROW_DECREASE
    };
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(row, id)
        .expect("the row's arrow")
}

/// The rows **drawn** under heading `g`, in the order they are drawn, ascending by name.
fn drawn_group_is_in_name_order(
    c: &mut HeadlessClient,
    sk: &dereth_assets::tables::SkillTable,
    g: usize,
) -> bool {
    let tops = heading_tops(c);
    let lo = tops[g];
    let hi = tops.get(g + 1).copied().unwrap_or(i32::MAX);
    let headings = with_wizard(c, |_, w| w.skill_headers.clone());
    let mut prev: Option<String> = None;
    let mut seen = 0_usize;
    let mut ok = true;
    for (h, y) in drawn_order(c) {
        if y <= lo || y >= hi || headings.contains(&h) {
            continue;
        }
        let id = {
            let shell = c.view().expect_app().ui().expect("the UI shell is up");
            match shell
                .ui
                .node(h)
                .expect("live")
                .instance_properties
                .get(ATTR_ROW_SKILL_ID)
            {
                Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => *v,
                other => panic!("a drawn item that is neither a heading nor a row: {other:?}"),
            }
        };
        let name = sk.skills[&id].name.clone();
        if let Some(p) = &prev {
            ok &= p.as_str() <= name.as_str();
        }
        prev = Some(name);
        seen += 1;
    }
    ok && seen > 0
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again
// ---------------------------------------------------------------------------------------------

/// Every row of the page, driven both ways, read off the screen each time.
pub(super) fn every_row_is_drawn_under_its_new_heading_and_back_again() {
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);
    let on_entry = geometry_follows_the_list(&mut c);

    // **The calibration.** A reader that says the picture follows the list has to say so when it
    // does not, or "it agrees" and "my reader is blind" are the same reading. Two items of the
    // client's own list are swapped by hand, with nothing re-laid-out, and the reader must
    // disagree; then it is swapped back.
    {
        with_wizard(&mut c, |_, w| {
            w.skill_list.as_mut().expect("the list").items.swap(1, 2)
        });
    }
    let reader_can_disagree = !geometry_follows_the_list(&mut c);
    {
        with_wizard(&mut c, |_, w| {
            w.skill_list.as_mut().expect("the list").items.swap(1, 2)
        });
    }
    let and_agrees_again = geometry_follows_the_list(&mut c);

    let total = with_wizard(&mut c, |_, w| w.skill_rows.len());
    let mut driven = 0_usize;
    let mut out_ok = 0_usize;
    let mut back_ok = 0_usize;
    let mut trained_ok = 0_usize;
    let mut untrained_ok = 0_usize;
    let mut accounted = 0_usize;

    for i in 0..total {
        let (skill, row, cost, spec, level, untrainable, unspecializable, credits) =
            with_wizard(&mut c, |_, w| {
                let credits = w.state.remaining_skill_credits;
                let r = &w.skill_rows[i];
                (
                    r.skill,
                    r.element,
                    r.train_cost,
                    r.spec_cost,
                    r.level,
                    r.untrainable,
                    r.unspecializable,
                    credits,
                )
            });
        let Some(row) = row else { continue };
        let min_level = sk.skills[&skill].min_level;

        // Which way this row can be driven first, so that it ends where it began, and which class
        // each press lands it in. A skill a people gets free cannot be given up, so its return
        // leg cannot be driven at all and it is counted rather than pretended about.
        let (out_is_up, want_class) = match level {
            SkillAdvancementClass::Untrained if untrainable && cost > 0 && cost <= credits => {
                (true, SkillAdvancementClass::Trained)
            }
            SkillAdvancementClass::Untrained => {
                accounted += 1;
                continue;
            }
            SkillAdvancementClass::Trained if untrainable => {
                (false, SkillAdvancementClass::Untrained)
            }
            SkillAdvancementClass::Trained
                if unspecializable && spec - cost >= 0 && spec - cost <= credits =>
            {
                (true, SkillAdvancementClass::Specialized)
            }
            SkillAdvancementClass::Trained => {
                accounted += 1;
                continue;
            }
            SkillAdvancementClass::Specialized if unspecializable => {
                (false, SkillAdvancementClass::Trained)
            }
            _ => {
                accounted += 1;
                continue;
            }
        };

        let group_before = drawn_group(&mut c, row).expect("the row is drawn under a heading");
        let y_before = top_of(&c, row);
        let class_before = level;
        let want_group = heading_for(want_class, min_level);
        assert_ne!(
            want_group, group_before,
            "the row is driven across a group boundary"
        );

        // Out.
        let arrow = row_arrow(&c, row, out_is_up);
        press_and_settle(&mut c, arrow);
        let landed = with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == want_class
            && geometry_follows_the_list(&mut c)
            && drawn_group(&mut c, row) == Some(want_group)
            && top_of(&c, row) != y_before
            && drawn_group_is_in_name_order(&mut c, &sk, want_group);
        if landed {
            out_ok += 1;
        }
        driven += 1;
        accounted += 1;
        if want_class == SkillAdvancementClass::Trained && out_is_up {
            trained_ok += 1;
        }
        if want_class == SkillAdvancementClass::Untrained {
            untrained_ok += 1;
        }

        // Back.
        let arrow = row_arrow(&c, row, !out_is_up);
        press_and_settle(&mut c, arrow);
        let returned = with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == class_before
            && geometry_follows_the_list(&mut c)
            && drawn_group(&mut c, row) == Some(group_before)
            && top_of(&c, row) == y_before
            && with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == credits;
        if returned {
            back_ok += 1;
        }
        if class_before == SkillAdvancementClass::Trained && !out_is_up {
            trained_ok += 1;
        }
        if class_before == SkillAdvancementClass::Untrained {
            untrained_ok += 1;
        }
    }

    // Essentially the whole table was really driven, both ways, and the trained boundary is the
    // bulk of it -- without which "every driven row landed" could be a claim about two of them.
    let every_one = out_ok == driven && back_ok == driven && accounted == total;
    let enough = driven >= 37 && trained_ok >= 30 && untrained_ok >= 30;

    c.assert_behaviour(
        "chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again",
        move |_| on_entry && reader_can_disagree && and_agrees_again && every_one && enough,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing
// ---------------------------------------------------------------------------------------------

/// The observation as it was reported, as one gesture.
pub(super) fn the_reported_press_moves_the_row_it_names() {
    let mut c = on_the_skills_page(0);
    let (_, sk) = chargen_tables(&c);

    // The skill is named because the report named it; every number is still read off the shipped
    // table rather than written down here.
    let skill = sk
        .skills
        .iter()
        .find(|(_, b)| b.name == "Armor Tinkering")
        .map(|(id, _)| *id)
        .expect("the shipped skill table has it");
    let i = with_wizard(&mut c, |_, w| {
        w.skill_rows
            .iter()
            .position(|r| r.skill == skill)
            .expect("it has a row on this page")
    });
    let (row, cost) = with_wizard(&mut c, |_, w| {
        (
            w.skill_rows[i].element.expect("its row"),
            w.skill_rows[i].train_cost,
        )
    });
    let credits_before = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits);
    let the_reported_start =
        credits_before == 52 && cost == 4 && drawn_group(&mut c, row) == Some(2);
    let y_before = top_of(&c, row);

    let arrow = row_arrow(&c, row, true);
    press_and_settle(&mut c, arrow);

    let paid = with_wizard(&mut c, |_, w| w.state.remaining_skill_credits) == 48;
    let moved = geometry_follows_the_list(&mut c)
        && drawn_group(&mut c, row) == Some(1)
        && top_of(&c, row) != y_before;

    // And leaving the page and coming back draws the same list it was already drawing -- which is
    // what used to be the only way to see the move at all.
    let before_reentry: Vec<i32> = drawn_order(&mut c).iter().map(|(_, y)| *y).collect();
    click_wizard(
        &mut c,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    c.tick(4);
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let after_reentry: Vec<i32> = drawn_order(&mut c).iter().map(|(_, y)| *y).collect();
    let nothing_changed = before_reentry == after_reentry
        && with_wizard(&mut c, |_, w| w.state.skill_level(skill)) == SkillAdvancementClass::Trained;

    c.assert_behaviour("chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing", move |_| {
        the_reported_start && paid && moved && nothing_changed
    });
    c.shutdown();
}
