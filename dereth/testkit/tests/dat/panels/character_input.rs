use super::*;

// =============================================================================================
// advancement.raise.*
//
// The `+10` button on the attributes, vitals and skills pages is clickable whenever the unspent
// experience covers ten raises.
//
// **The button is reached by the pointer and by nothing else.** The shipped id for a ten-point
// button is carried twice in the live tree -- once under the skills page and once under the
// attributes page -- and each page has two footer containers of which its own state picks one, so
// a scenario that broadcast a click at an id would be pressing whichever one the walk found
// first. Every press here is a hit test at the button's own centre, and the element the hit test
// chose is asserted to be the one the page's own state names.
// =============================================================================================

/// How much unspent experience the character has.
fn set_available_experience(c: &mut HeadlessClient, n: i64) {
    let q = c
        .world_mut()
        .player_qualities_mut()
        .expect("the recorded description is on the player's own weenie");
    assert!(
        q.set(
            dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                dereth_client_runtime::hud::AVAILABLE_EXPERIENCE,
            ),
            dereth_client_model::StatValue::Int64(n),
        ),
        "a whole number is always storable"
    );
}

/// The footer child of `panel`'s **own** footer container -- the one the sub-panel's state names,
/// and not the first element in the tree carrying that id.
fn panel_footer_child(c: &mut HeadlessClient, panel: ElementId, child: u32) -> ElemHandle {
    let p = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let page = ui
            .get_child_recursive(root, remaining::CHARACTER_PAGE)
            .expect("the character page is in the shipped layout");
        ui.get_child_recursive(page, panel)
            .expect("the sub-panel is in the shipped layout")
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    let state = ui.node(p).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(p, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container the panel's own state names");
    ui.get_child_recursive(container, ElementId(child))
        .expect("the footer child")
}

/// Whether the button says it is out of reach -- the attribute the button's own state writes and
/// the press gate reads.
fn button_is_disabled(c: &mut HeadlessClient, h: ElemHandle) -> Option<bool> {
    let (ui, _) = gameplay_screen(c.app_mut());
    dereth_ui_screens::bind::attr_bool(ui, h, statmgmt::ATTR_DISABLED)
}

fn element_state(c: &mut HeadlessClient, h: ElemHandle) -> u32 {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(|n| n.state.0).expect("the element is alive")
}

/// Raise the character page and select whichever sub-panel `h` belongs to.
fn show_the_page_holding(c: &mut HeadlessClient, h: ElemHandle) {
    open_the_page(c, remaining::CHARACTER_PAGE);
    let sub = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let mut cur = Some(h);
        let mut found = None;
        while let Some(e) = cur {
            let id = ui.node(e).map(dereth_ui::ElementNode::element_id);
            if id == Some(skills::PANEL) || id == Some(attributes::PANEL) {
                found = id;
                break;
            }
            cur = ui.parent(e);
        }
        found
    };
    if let Some(sub) = sub {
        click_the_tab(c, remaining::CHARACTER_PAGE, sub);
    }
    c.tick(3);
}

/// Press one row of a list with the pointer, having scrolled it into view first.
fn press_the_row(c: &mut HeadlessClient, h: ElemHandle) {
    show_the_page_holding(c, h);
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.probe_mut().hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            for w in [panels.skills.list.as_mut(), panels.attributes.list.as_mut()]
                .into_iter()
                .flatten()
            {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.probe_mut().hud_mut().panels = panels;
    }
    c.tick(1);
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(1);
}

/// A real press on an element, with the element the hit test chose handed back.
fn press_and_say_what_was_under_it(
    c: &mut HeadlessClient,
    h: ElemHandle,
) -> (Option<ElemHandle>, ScreenPoint) {
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let hit = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.hit_test_screen(at.x, at.y)
    };
    c.when(Player::Click(Target::Point(at)));
    // Two frames: the press's own action becomes a request on the next frame's pass.
    c.tick(2);
    (hit, at)
}

/// Every raise this client has asked for, as `(what it names, how much)`.
fn raises_asked_for(c: &HeadlessClient) -> Vec<(u32, u32, u32)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TrainAttribute(m) => {
                Some((0x0045, m.attribute_id, m.xp_spent))
            }
            dereth_client_model::Request::TrainAttribute2nd(m) => {
                Some((0x0044, m.vital_id, m.xp_spent))
            }
            dereth_client_model::Request::TrainSkill(m) => Some((0x0046, m.skill_id, m.xp_spent)),
            _ => None,
        })
        .collect()
}

/// **All three pages at once.** One point short of what ten raises cost,
/// the ten-point button says it is out of reach and a real press on it sends nothing; with exactly
/// that much unspent experience it lights up, the pointer really lands on the page's own button,
/// and one press sends one raise naming that stat and that amount -- after which the button puts
/// itself back out of reach while it waits for the answer.
pub(super) fn the_ten_point_button_lights_up_and_a_real_press_sends_the_raise() {
    let mut c = a_recorded_character(SESSION);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);

    // What the three cases are, worked out from the character's own record rather than pinned.
    let q = c
        .view()
        .world()
        .player_qualities()
        .expect("the recorded description")
        .clone();
    let strength = q.attribute(1).expect("the recorded character has strength");
    let strength_ten = dereth_rules::advancement::attribute_cost_to_raise_10(
        &xp,
        strength.level_from_cp,
        strength.cp_spent,
        false,
    );
    let health = q
        .attribute_2nd(1)
        .expect("the recorded character has health");
    let health_ten = dereth_rules::advancement::attribute_cost_to_raise_10(
        &xp,
        health.attribute.level_from_cp,
        health.attribute.cp_spent,
        true,
    );
    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| q.skill(r.skill).is_some_and(|s| s.sac >= 2))
        .map(|r| r.skill)
        .expect("the recorded character has a trained skill with a row");
    let skill_ten = dereth_rules::advancement::skill_cost_to_raise_10(&q, &xp, skill);
    let costs_something = strength_ten > 0 && health_ten > 0 && skill_ten > 0;

    let mut every_case_holds = true;
    let cases: [(ElementId, u32, u32, u32); 3] = [
        (attributes::PANEL, 0x0045, 1, strength_ten),
        (attributes::PANEL, 0x0044, 1, health_ten),
        (skills::PANEL, 0x0046, skill, skill_ten),
    ];
    for (i, (panel, opcode, names, ten)) in cases.into_iter().enumerate() {
        // **A client of its own per half.** The footer is worked out when the row is selected, so
        // how much experience the character has must be set *before* the press -- and pressing a
        // row that is already selected is the player un-selecting it, which empties the footer.
        for enough in [false, true] {
            let mut c = a_recorded_character(SESSION);
            let row = match i {
                0 => attribute_row(&c, 1, false),
                1 => attribute_row(&c, 2, true),
                _ => skill_row(&c, skill),
            };
            set_available_experience(&mut c, i64::from(ten) - i64::from(!enough));
            press_the_row(&mut c, row);

            let button = panel_footer_child(&mut c, panel, statmgmt::child::BUTTON_10);
            let disabled = button_is_disabled(&mut c, button);
            let state = element_state(&mut c, button);
            let (hit, _) = press_and_say_what_was_under_it(&mut c, button);
            let asked = raises_asked_for(&c);
            // Whichever half this is, the pointer landed on *this* page's own button and not on
            // the other page's, nor on a container drawn over it.
            every_case_holds &= hit == Some(button);
            if enough {
                every_case_holds &= disabled == Some(false)
                    && state == statmgmt::button_state::ENABLED
                    && asked == vec![(opcode, names, ten)];
                // ...and the button puts itself back out of reach while it waits for an answer.
                let button = panel_footer_child(&mut c, panel, statmgmt::child::BUTTON_10);
                every_case_holds &= button_is_disabled(&mut c, button) == Some(true);
            } else {
                every_case_holds &= disabled == Some(true)
                    && state == statmgmt::button_state::DISABLED
                    && asked.is_empty();
            }
            c.shutdown();
        }
    }

    c.assert_behaviour(
        "advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise",
        move |_| costs_something && every_case_holds,
    );
    c.shutdown();
}

/// Raise the character page with `sub` up -- **unless it is already up**, because the tab is a
/// toggle and pressing it again would move the page to the other sub-panel. This is
/// [`show_the_skills_page`]'s guard with the sub-panel as a parameter.
fn character_page_show_the_page(c: &mut HeadlessClient, sub: ElementId) {
    open_the_page(c, remaining::CHARACTER_PAGE);
    let already_up = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, sub)
            .is_some_and(|h| ui.is_visible(h))
    };
    if !already_up {
        click_the_tab(c, remaining::CHARACTER_PAGE, sub);
    }
    c.tick(2);
}

/// A real press at the middle of `h`, with **the shipped id of the element the hit test chose**
/// handed back -- which on a list row is never the row.
fn character_page_press(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    let (hit, _) = press_and_say_what_was_under_it(c, h);
    let (ui, _) = gameplay_screen(c.app_mut());
    hit.and_then(|e| ui.node(e))
        .map(dereth_ui::ElementNode::element_id)
}

/// The shipped id of the element `h` is.
fn character_page_id_of(c: &mut HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(dereth_ui::ElementNode::element_id)
}

/// One child of a list row, as text.
fn character_page_row_text(c: &mut HeadlessClient, row: ElemHandle, child: u32) -> String {
    let h = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(row, ElementId(child))
    };
    h.map_or_else(String::new, |h| glyph_runs(c.app_mut(), h).0)
}

/// The skills footer's own child, as text. [`footer_child`] resolves it the way the panel does.
fn character_page_footer_text(c: &mut HeadlessClient, child: u32) -> String {
    let h = footer_child(c.app_mut(), child);
    glyph_runs(c.app_mut(), h).0
}

/// Press the skills footer's raise button, **when the footer the panel's own state chose carries
/// one at all** -- with nothing picked the default container has no raise button, and the answer
/// is then `false` rather than a panic.
///
/// The raise action is reached from the button's message. The
/// button under test in the *other* scenario -- the ten-point one -- is
/// the one whose claim is about what the pointer lands on. Two frames afterwards, because the
/// action the press queues is turned into a request by the next frame's pass.
fn character_page_press_the_raise_button(c: &mut HeadlessClient) -> bool {
    let button = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, remaining::CHARACTER_PAGE)
            .and_then(|page| ui.get_child_recursive(page, skills::PANEL))
            .and_then(|panel| {
                let state = ui.node(panel).map_or(0, |n| n.state.0);
                ui.get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
                    .and_then(|k| ui.get_child_recursive(k, ElementId(statmgmt::child::BUTTON)))
            })
    };
    if let Some(h) = button {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    c.tick(2);
    button.is_some()
}

/// **Selection on the attributes page.** A real press picks the row under the pointer,
/// the press lands on the list rather than on the row, the footer names what was picked, and the
/// toggle is an edge in both directions. Nothing is sent.
pub(super) fn a_press_picks_the_attribute_row_under_it_and_lands_on_the_list() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, attributes::PANEL);

    let rows: Vec<(u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .map(|r| (r.stat, r.element))
        .collect();
    // Six primaries and three vitals, and the first of them is Strength.
    let the_nine_rows = rows.len() == 9 && rows.first().map(|(s, _)| *s) == Some(1);
    // The selected-row index starts at -1, which is not the same number as row zero.
    let nothing_is_picked_to_start =
        c.view().expect_app().hud().panels.attributes.selected_index == -1;
    let sent_before = c.outbound_opcodes();

    let row = rows[0].1;
    let row_id = character_page_id_of(&mut c, row);
    let hit = character_page_press(&mut c, row);
    // Asserted rather than assumed: the press never lands on the row.
    let it_landed_on_the_list = hit != row_id && hit == Some(attributes::LIST_BOX);

    let panel = &c.view().expect_app().hud().panels.attributes;
    let it_picked_strength =
        panel.selected_index == 0 && panel.selected().map(|r| r.stat) == Some(1);
    let the_footer_names_it = panel.footer_content.title == "Strength: 30";

    // The selection-update loop puts the picked row in state 6 and
    // every other row to state 1.
    let the_rows_are_drawn_picked_and_not = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let states: Vec<u32> = rows
            .iter()
            .map(|(_, e)| ui.node(*e).map_or(0, |n| n.state.0))
            .collect();
        states[0] == statmgmt::row_state::SELECTED
            && states[1..]
                .iter()
                .all(|s| *s == statmgmt::row_state::UNSELECTED)
    };

    // The toggle is an **edge**, so it is driven three times: a harness that stops at the first
    // press cannot tell a toggle from a one-way latch.
    let _ = character_page_press(&mut c, row);
    let pressing_it_again_un_picks_it = {
        let panel = &c.view().expect_app().hud().panels.attributes;
        panel.selected_index == -1 && panel.footer_content.title == "Select an Attribute to Improve"
    };
    let _ = character_page_press(&mut c, row);
    let and_a_third_press_picks_it_again =
        c.view().expect_app().hud().panels.attributes.selected_index == 0;

    // Nothing was spent. This scenario's subject is the picking alone, and "nothing" is every
    // message the client framed and not only a raise.
    let nothing_was_sent = c.outbound_opcodes() == sent_before;

    c.assert_behaviour(
        "attributes.selection.a-press-picks-the-row-under-the-pointer-through-the-list-itself",
        move |_| {
            the_nine_rows
                && nothing_is_picked_to_start
                && it_landed_on_the_list
                && it_picked_strength
                && the_footer_names_it
                && the_rows_are_drawn_picked_and_not
                && pressing_it_again_un_picks_it
                && and_a_third_press_picks_it_again
                && nothing_was_sent
        },
    );
    c.shutdown();
}

/// **The same press on the other page, and only on that one.** The attribute and skill panels
/// both carry a `0x1000023D` list box at the **same screen rectangle**; the
/// attributes panel is offered the message first, so without `ListBoxWidget::owns` it claims every
/// press meant for a skill row. Both halves are asserted, because "a skill got picked" alone would
/// pass with the two panels' answers swapped in exactly one direction.
pub(super) fn a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, skills::PANEL);

    let rows: Vec<(u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.element))
        .collect();
    let the_recorded_skills = rows.len() == 38;
    let nothing_is_picked_to_start = c.view().expect_app().hud().panels.skills.selected_skill == 0;
    let the_other_page_before = c.view().expect_app().hud().panels.attributes.selected_index;
    let sent_before = c.outbound_opcodes();

    let (skill, row) = rows[0];
    let hit = character_page_press(&mut c, row);
    let it_landed_on_the_list = hit == Some(skills::LIST_BOX);
    let it_picked_that_skill = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let the_other_page_stayed_put =
        c.view().expect_app().hud().panels.attributes.selected_index == the_other_page_before;

    let _ = character_page_press(&mut c, row);
    let pressing_it_again_un_picks_it =
        c.view().expect_app().hud().panels.skills.selected_skill == 0;

    let nothing_was_sent = c.outbound_opcodes() == sent_before;

    c.assert_behaviour(
        "skills.selection.a-press-picks-the-skill-row-under-it-and-the-other-page-stays-put",
        move |_| {
            the_recorded_skills
                && nothing_is_picked_to_start
                && it_landed_on_the_list
                && it_picked_that_skill
                && the_other_page_stayed_put
                && pressing_it_again_un_picks_it
                && nothing_was_sent
        },
    );
    c.shutdown();
}

/// **The other half of the pair, and the one that spends.** The raise button is pressed
/// *before* anything is picked and must be silent; then a trained skill is picked, which must also
/// be silent by itself; then one press must ask for exactly one raise, naming that skill and the
/// cost the footer showed.
///
/// Without the first half a green result would say nothing about whether the gate exists at all.
pub(super) fn a_raise_sends_nothing_until_a_skill_row_is_picked() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, skills::PANEL);
    let sent_before = c.outbound_opcodes();

    // With nothing picked the default container carries no raise button at all, and the panel's
    // raise action returns false when the selected skill is zero. Either way: silence.
    let nothing_is_picked_to_start = c.view().expect_app().hud().panels.skills.selected_skill == 0;
    let _ = character_page_press_the_raise_button(&mut c);
    let silent_with_nothing_picked = c.outbound_opcodes() == sent_before;

    // A **trained** skill, whose arm is the one that sends the raise.
    let (skill, row) = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.group == skills::SkillGroup::Trained)
        .map(|r| (r.skill, r.element))
        .expect("the recorded character has a trained skill with a row");
    let _ = character_page_press(&mut c, row);
    let the_pick_is_the_gate = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let picking_is_silent_too = c.outbound_opcodes() == sent_before;

    // What the footer said the raise would cost, read before the press that spends it.
    let shown_cost = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .line_one_value
        .clone();
    let the_button_was_there = character_page_press_the_raise_button(&mut c);

    // One press, one message -- and the opcode is the one the **production sender** wrote onto the
    // blob, not a number typed here twice.
    let after = c.outbound_opcodes();
    let one_message = after.len() == sent_before.len() + 1 && after.last().copied() == Some(0x0046);
    // And that one message names the skill that was picked and carries the cost that was shown.
    let asked = raises_asked_for(&c);
    let it_names_the_skill_and_the_cost = asked.len() == 1
        && asked[0].0 == 0x0046
        && asked[0].1 == skill
        && statmgmt::num(asked[0].2) == shown_cost;

    c.assert_behaviour(
        "advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost",
        move |_| {
            nothing_is_picked_to_start
                && silent_with_nothing_picked
                && the_pick_is_the_gate
                && picking_is_silent_too
                && the_button_was_there
                && one_message
                && it_names_the_skill_and_the_cost
        },
    );
    c.shutdown();
}

/// The 38 skill rows of the recording's character as they are **drawn**, in list order:
/// `(SkillTable id, label, value cell)`.
///
/// **The independent oracle for the value column.** These are literals taken out of
/// the running panel against that recording, not restatements of anything the panel computes: no
/// symbol here is one the production code reads, so a change to the skill query, to which field
/// `write_row` writes, to the group ordering or to the row-to-skill mapping reddens it. The
/// assertion it replaces read the drawn cell back against `SkillRow::value`, and both sides are
/// written from `SkillEntry::effective` on one line of `write_row` -- one number compared with
/// itself, which would pass unchanged if all 38 rows were five too low.
///
/// **Where the numbers come from, independently of this build.** The character carries no
/// enchantments and `vitae_value() == 1.0`, but does carry `JACK_OF_ALL_TRADES` (property 326)
/// = 1, which adds **+5** to the non-raw value for each of these 38 rows. So every value
/// here is the character's base skill plus five, and *Melee Defense* is pinned separately at
/// 72 raw, 77 drawn.
const FIRST_LOGIN_WALK_JUMP_SKILL_CELLS: [(u32, &str, &str); 38] = [
    // Specialized
    (52, "Dirty Fighting", "58"),
    (49, "Dual Wield", "82"),
    (46, "Finesse Weapons", "82"),
    (14, "Arcane Lore", "30"),
    (21, "Healing", "63"),
    (32, "Item Enchantment", "28"),
    (22, "Jump", "75"),
    (23, "Lockpick", "63"),
    (36, "Loyalty", "10"),
    (15, "Magic Defense", "20"),
    (6, "Melee Defense", "77"),
    (47, "Missile Weapons", "60"),
    (24, "Run", "110"),
    (40, "Salvaging", "10"),
    // Trained
    (29, "Armor Tinkering", "50"),
    (27, "Assess Creature", "5"),
    (19, "Assess Person", "5"),
    (20, "Deception", "5"),
    (44, "Heavy Weapons", "48"),
    (18, "Item Tinkering", "85"),
    (35, "Leadership", "5"),
    (45, "Light Weapons", "48"),
    (30, "Magic Item Tinkering", "65"),
    (7, "Missile Defense", "45"),
    (48, "Shield", "70"),
    (41, "Two Handed Combat", "48"),
    (28, "Weapon Tinkering", "50"),
    // Untrained and unusable
    (38, "Alchemy", "5"),
    (39, "Cooking", "5"),
    (31, "Creature Enchantment", "5"),
    (37, "Fletching", "5"),
    (33, "Life Magic", "5"),
    (16, "Mana Conversion", "5"),
    (50, "Recklessness", "5"),
    (51, "Sneak Attack", "5"),
    (54, "Summoning", "5"),
    (43, "Void Magic", "5"),
    (34, "War Magic", "5"),
];

/// **Every skill row draws its value, and the glyphs name a real font.**
///
/// The blank column was one wrong argument. Text composition selects a font from property
/// `0x1A` with a font index, then a colour from property `0x1B` with a colour index. The row
/// update supplies font index **0** and the computed colour index. This build had passed the
/// computed colour index as the glyph's font index instead.
///
/// That index could exceed the font array and resolve to `DataId(0)`, which rasterises nothing.
/// All 38 values were invisible.
///
/// The shipped layout is the second, independent reading: the value element `0x1000012B` declares
/// **one** font and **three** colours, and the label `0x1000012A`, which never takes the index,
/// declares one of each.
pub(super) fn every_skill_row_draws_its_value_in_a_font_that_exists() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, skills::PANEL);

    let rows: Vec<(u32, i32, u32, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.value, r.font, r.element))
        .collect();
    let the_denominator = rows.len() == 38;

    let mut every_row_draws_a_value = true;
    let mut cells: Vec<(u32, String, String)> = Vec::new();
    for (skill, value, colour_index, row) in &rows {
        let label = character_page_row_text(&mut c, *row, skills::row::LABEL);
        let drawn = character_page_row_text(&mut c, *row, skills::row::VALUE);
        every_row_draws_a_value &= !label.is_empty();
        // **Not the value check** -- the value is pinned against the table below. `SkillRow::value`
        // and the drawn cell are written from `SkillEntry::effective` on the same line, so this can
        // only catch the two *drifting apart*, as would happen if a cell were rewritten without
        // touching the cache.
        every_row_draws_a_value &= drawn == value.to_string();
        cells.push((*skill, label, drawn));

        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(*row, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let want = statmgmt::font_color_at(ui, v, *colour_index)
            .unwrap_or_else(|| panic!("skill {skill}: the row declares no colour {colour_index}"));
        let sb = ui.screen_box(v);
        let placed = ui
            .text_element_mut(v)
            .map_or_else(Vec::new, |t| t.compose(sb));
        every_row_draws_a_value &= !placed.is_empty()
            // The half that was broken: the glyphs must resolve to a font, not to `DataId(0)`.
            && placed.iter().all(|p| p.font.0 != 0)
            && placed.iter().all(|p| p.color == want);
    }

    // The independent oracle. Compared as one vector so the **order** is pinned too:
    // The rebuild makes four groups back to front and re-sorts them, and a row that
    // moved would still match a per-skill lookup.
    let want: Vec<(u32, String, String)> = FIRST_LOGIN_WALK_JUMP_SKILL_CELLS
        .iter()
        .map(|(id, name, value)| (*id, (*name).to_string(), (*value).to_string()))
        .collect();
    let the_recorded_cells = cells == want;
    let and_it_agrees_with_the_other_pin = cells
        .iter()
        .find(|(id, _, _)| *id == 6)
        .map(|(_, _, v)| v.as_str())
        == Some("77");

    // The three colours, as literals out of the shipped layout rather than read back through the
    // same accessor that wrote them -- and the label, which never takes the index, as the
    // discriminating pair.
    let the_shipped_colours = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let l = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::LABEL))
            .expect("the row has a label element");
        statmgmt::font_count(ui, v) == 1
            && statmgmt::font_color_at(ui, v, 0) == Some(0xFFFF_FFFF)
            && statmgmt::font_color_at(ui, v, 1) == Some(0xFF00_FF00)
            && statmgmt::font_color_at(ui, v, 2) == Some(0xFFFF_0000)
            && statmgmt::font_color_at(ui, v, 3).is_none()
            && statmgmt::font_count(ui, l) == 1
            && statmgmt::font_color_at(ui, l, 1).is_none()
    };

    // The font helper's out-of-range rule is exercised directly because **no production
    // call site supplies an out-of-range font index any more** -- `write_row` passes 0. Without
    // this the clamp is unfalsifiable, which is what a surviving mutation said.
    let an_index_past_the_end_falls_back = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let v = ui
            .get_child_recursive(rows[0].3, ElementId(skills::row::VALUE))
            .expect("the row has a value element");
        let wrote = statmgmt::set_text_with_font(ui, v, "45", 9, 9);
        let glyphs: Vec<(u32, u32)> = ui.text_element_mut(v).map_or_else(Vec::new, |t| {
            t.glyphs.glyphs.iter().map(|g| (g.font, g.color)).collect()
        });
        let sb = ui.screen_box(v);
        let placed = ui
            .text_element_mut(v)
            .map_or_else(Vec::new, |t| t.compose(sb));
        wrote
            && glyphs == vec![(0, 0xFFFF_FFFF), (0, 0xFFFF_FFFF)]
            && !placed.is_empty()
            && placed.iter().all(|p| p.font.0 != 0)
    };

    c.assert_behaviour(
        "skills.rows.every-row-draws-its-value-in-a-font-that-exists-and-the-colour-it-declares",
        move |_| {
            the_denominator
                && every_row_draws_a_value
                && the_recorded_cells
                && and_it_agrees_with_the_other_pin
                && the_shipped_colours
                && an_index_past_the_end_falls_back
        },
    );
    c.shutdown();
}

/// **Every attribute row draws an icon, and it is that row's own icon.**
///
/// Initialization performs **nine** icon lookups, one immediately before each row is constructed:
/// group `0x10000002` for the six primaries
/// and `0x10000003` for the three vitals, with the stat id as the enum value. One fixed lookup,
/// *"one icon for every row"*, is the wrong reading; nine **distinct** ids is what makes it
/// false, and it is asserted rather than described.
///
/// The oracle is the shipped master mapper, read here rather than through the panel.
pub(super) fn every_attribute_row_draws_the_icon_its_own_group_names() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, attributes::PANEL);

    let rows: Vec<(u32, bool, ElemHandle)> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .map(|r| (r.stat, r.secondary, r.element))
        .collect();
    let the_nine_rows = rows.len() == 9;

    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut seen: std::collections::BTreeSet<dereth_primitives::DataId> =
        std::collections::BTreeSet::new();
    let mut every_row_draws_its_own = true;
    for (stat, secondary, row) in &rows {
        let group = if *secondary {
            attributes::ICON_GROUP_2ND
        } else {
            attributes::ICON_GROUP
        };
        let want = dereth_assets::did_by_enum(&*store, group, *stat)
            .unwrap_or_else(|| panic!("group {group:#010X} value {stat} resolves to nothing"));
        let label = character_page_row_text(&mut c, *row, skills::row::LABEL);
        let icon = {
            let (ui, _) = gameplay_screen(c.app_mut());
            ui.get_child_recursive(*row, ElementId(skills::row::ICON))
                .and_then(|h| ui.node(h))
                .and_then(|n| n.region.image.as_ref().map(|g| g.did))
        };
        every_row_draws_its_own &= !label.is_empty() && icon == Some(want);
        seen.insert(want);
    }
    let nine_distinct_icons = seen.len() == 9;
    // The two groups, written out rather than read back through the same constant the panel uses.
    let the_two_enum_groups =
        attributes::ICON_GROUP == 0x1000_0002 && attributes::ICON_GROUP_2ND == 0x1000_0003;

    c.assert_behaviour(
        "attributes.rows.every-row-draws-the-icon-its-own-group-names",
        move |_| {
            the_nine_rows && every_row_draws_its_own && nine_distinct_icons && the_two_enum_groups
        },
    );
    c.shutdown();
}

/// **Every large number in the character panels carries the client's own separator.**
///
/// The separator is not written here twice: [`dereth_ui_screens::panels::numfmt`] takes it from the
/// shipped language record, and this scenario pins it as a literal against the number formatter's
/// `NUMBERFMTA` configuration -- two readings that could have
/// disagreed and do not. The *values* come out of the recording's own character description.
///
/// The three grouped sites are all here: unassigned experience (footer line two), total
/// experience and experience to the next level (the header). A four-digit and a seven-digit case
/// are exercised, and a skill row's own value is the asymmetric case that separates "we grouped
/// the right things" from "we grouped everything" -- it is a `%d` in the client and must not be
/// grouped.
pub(super) fn the_character_pages_numbers_carry_the_shipped_separator() {
    let mut c = a_recorded_character(SESSION);
    character_page_show_the_page(&mut c, skills::PANEL);

    // ---- the separator itself, pinned two ways -----------------------------------------------
    let g = numfmt::shipped();
    let the_separator = g.separator == ","
        && g.size == 3
        && g.separator == numfmt::XPTOSTRING_THOUSAND_SEP
        && g.size == numfmt::XPTOSTRING_GROUPING
        // The boundaries, written out rather than derived, so a wrong `size` cannot hide.
        && numfmt::group(999, &g) == "999"
        && numfmt::group(1_000, &g) == "1,000"
        && numfmt::group(1_234_567, &g) == "1,234,567"
        // The shipped language record supplies the negative-number format. These results prove
        // that its selected separator is applied to negative values rather than merely assumed.
        && g.negative_format == "-%s"
        && numfmt::group(-1_234, &g) == "-1,234"
        && numfmt::group(-999, &g) == "-999";

    // ---- the recording's own numbers, on screen ----------------------------------------------
    let q = recorded_qualities(SESSION);
    let int64 = |k: u32| -> i64 {
        match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            k,
        )) {
            Some(dereth_client_model::StatValue::Int64(v)) => v,
            other => panic!("the recorded description carries no Int64 {k}: {other:?}"),
        }
    };
    let available = int64(dereth_client_runtime::hud::AVAILABLE_EXPERIENCE);
    let total = int64(dereth_client_runtime::hud::TOTAL_EXPERIENCE);
    let four_digit_numbers_to_read = available >= 1_000 && total >= 1_000;

    let header = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .header_content
        .clone();
    let the_header = header.total_xp == numfmt::group(total, &g)
        && header.total_xp.contains(',')
        && (header.xp_to_level.contains(',') || header.xp_to_level == "Infinity!");

    // Footer line two is `Unassigned Experience`, the third grouped site. It is read out of
    // the live element rather than off the panel's own record.
    let shown = character_page_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let the_footer = shown == numfmt::group(available, &g) && shown.contains(',');

    // The same after a pick, because the selection footer is a different function.
    let row = c.view().expect_app().hud().panels.skills.rows[0].element;
    let _ = character_page_press(&mut c, row);
    let shown = character_page_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let the_selection_footer_too = shown == numfmt::group(available, &g);

    // A row **value** is `%d` in the client and must *not* be grouped.
    let a_row_value_is_left_plain =
        !character_page_row_text(&mut c, row, skills::row::VALUE).contains(',');

    c.assert_behaviour(
        "skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator",
        move |_| {
            the_separator
                && four_digit_numbers_to_read
                && the_header
                && the_footer
                && the_selection_footer_too
                && a_row_value_is_left_plain
        },
    );
    c.shutdown();
}

// =============================================================================================
// skills.selection.*, skills.footer.*, skills.raise.*, skills.rows.*, character.header.*,
// spellbook.filter.*
//
// The character and spell panels *answering input*. Some drives below are the pointer and some
// the element message; the two are the same channel one layer apart, and each claim keeps the
// one it is measured through.
//
// The recorded character is `SESSION`'s, as it is for the rest of this file: the numbers every
// assertion here is against are recomputed in the scenario out of the recording's own character
// description and the shipped tables, read through `table` rather than back out of the panel, so
// the scenario's arithmetic and the panel's cannot share a mistake.
// =============================================================================================

/// One footer child of the **skills** sub-panel, resolved the way the panel resolves it -- the
/// sub-panel's own state picks the container, then the child id -- and `None` when that container
/// has no such child.
///
/// [`footer_child`] and [`panel_footer_child`] both panic on a missing child, and this file's one
/// claim about the *default* footer is that it carries no raise button at all, which is a
/// `None` and not a panic.
fn pinp_footer_child(c: &mut HeadlessClient, child: u32) -> Option<ElemHandle> {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root()?;
    let page = ui.get_child_recursive(root, remaining::CHARACTER_PAGE)?;
    let panel = ui.get_child_recursive(page, skills::PANEL)?;
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container =
        ui.get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))?;
    ui.get_child_recursive(container, ElementId(child))
}

/// What one footer child says, off the live element.
fn pinp_footer_text(c: &mut HeadlessClient, child: u32) -> String {
    match pinp_footer_child(c, child) {
        Some(h) => glyph_runs(c.app_mut(), h).0,
        None => String::new(),
    }
}

/// One footer child's own state -- which is what says whether a button is lit or out of reach.
fn pinp_footer_state(c: &mut HeadlessClient, child: u32) -> Option<u32> {
    let h = pinp_footer_child(c, child)?;
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(h).map(|n| n.state.0)
}

/// Every skill row's own state, in list order.
fn pinp_row_states(c: &mut HeadlessClient) -> Vec<u32> {
    let rows: Vec<ElemHandle> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.element)
        .collect();
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .map(|h| ui.node(*h).map_or(0, |n| n.state.0))
        .collect()
}

/// Press the raise button under the picked skill **with the pointer**, and say what the hit test
/// found there -- so a scenario can say the press landed on that page's own button.
fn pinp_press_the_raise_button(
    c: &mut HeadlessClient,
    child: u32,
) -> (Option<ElemHandle>, ElemHandle) {
    let button = panel_footer_child(c, skills::PANEL, child);
    let (hit, _) = press_and_say_what_was_under_it(c, button);
    (hit, button)
}

/// Every raise-the-skill request this client has put in its outbox, as `(opcode, what, how much)`.
///
/// [`raises_asked_for`] answers for the three the ten-point family is about; this one adds the
/// training-in-credits request, which is the other arm of the same button.
fn pinp_skill_requests(c: &HeadlessClient) -> Vec<(u32, u32, u32)> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::TrainSkill(m) => Some((0x0046, m.skill_id, m.xp_spent)),
            dereth_client_model::Request::TrainSkillAdvancementClass(m) => {
                Some((0x0047, m.skill_id, m.credits_spent))
            }
            _ => None,
        })
        .collect()
}

/// The bytes one request really leaves as, on the queue it leaves on.
///
/// **The bytes, not the request.** The accumulating
/// outbox this harness keeps holds the request and not the frame it was written in, so the
/// encoding is taken the way the house-abandon scenario takes it: through the client's own
/// sender, into a transport that keeps what it was handed.
fn pinp_encoded(r: &dereth_client_model::Request) -> (bool, Vec<u8>) {
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );
    let framed = dereth_client_runtime::requests::send_request(&mut session, r);
    let payload = session
        .transport
        .sent
        .last()
        .map(|p| p.payload.clone())
        .unwrap_or_default();
    (framed, payload)
}

/// The game action envelope the four requests below go out in: the ordered-action wrapper, the
/// first sequence number of a fresh session, the opcode, and then the message's own body.
fn pinp_action_bytes(opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut want = 0xF7B1_u32.to_le_bytes().to_vec();
    want.extend_from_slice(&1_u32.to_le_bytes());
    want.extend_from_slice(&opcode.to_le_bytes());
    want.extend_from_slice(body);
    want
}

/// Flip a toggle button the way the button's own release flips it -- the toggled attribute goes
/// down **before** the click is raised, which is what lets the filter read the post-click answer.
///
/// **This channel is kept deliberately.** The claim is measured through
/// `broadcast_element_message`, which is what a completed pointer click ends in one layer further
/// down; the spellbook page is not raised anywhere in this family, and raising it to drive a
/// pointer would change what the claim is measured over.
fn pinp_toggle(c: &mut HeadlessClient, id: ElementId, on: bool) {
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        let h = ui
            .get_element(id)
            .expect("the filter button is in the shipped tree");
        statmgmt::set_toggle_button_state(ui, h, on);
        let want = if on {
            dereth_ui_screens::panels::spellbook::BUTTON_ON
        } else {
            dereth_ui_screens::panels::spellbook::BUTTON_OFF
        };
        assert_eq!(
            ui.node(h).map(|n| n.state.0),
            Some(want),
            "the toggle must land before the click reads it"
        );
    }
    c.tick(1);
}

/// The click a completed button press raises, on a button named by its id.
fn pinp_click(c: &mut HeadlessClient, id: ElementId) -> bool {
    let found = {
        let (ui, _) = gameplay_screen(c.app_mut());
        match ui.get_element(id) {
            Some(h) => {
                ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
                true
            }
            None => false,
        }
    };
    // Two frames, and the second is not slack: the packet controller runs near the top of a frame
    // and the command slot near the bottom, so this click's action becomes a datagram on the next
    // frame's controller pass.
    c.tick(2);
    found
}

/// Every spellbook-filter message this client has sent, in order.
fn pinp_filters_sent(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::SpellbookFilterEvent(m) => Some(m.filter_mask),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// **The page comes up asking for a pick, and a press picks.** The default panel under the list
/// is the no-selection state and is what a second press on the same row brings back; a press on a
/// different row moves the pick rather than adding one.
pub(super) fn the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    // Nothing picked: the default container, its title and two labels -- which are not written by
    // the client but resolved out of the shipped string table -- and no raise button at all.
    let opened_default = skills_panel_state(c.app_mut()) == Some(statmgmt::state::DEFAULT);
    let title = pinp_footer_text(&mut c, statmgmt::child::TITLE);
    let credits_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let experience_label = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_LABEL);
    let no_raise_button = pinp_footer_state(&mut c, statmgmt::child::BUTTON).is_none();

    let rows: Vec<u32> = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .collect();
    assert!(
        rows.len() > 2,
        "the recorded character has rows to pick from ({})",
        rows.len()
    );
    let (first, second) = (rows[0], rows[1]);

    press_skill_row(&mut c, first);
    let picked = {
        let p = &c.view().expect_app().hud().panels.skills;
        (p.selected_skill, p.selected_index)
    } == (first, 0);
    let footer_swapped = skills_panel_state(c.app_mut()) != Some(statmgmt::state::DEFAULT);
    let states = pinp_row_states(&mut c);
    let picked_count = states
        .iter()
        .filter(|s| **s == statmgmt::row_state::SELECTED)
        .count();
    let exactly_one_is_picked = picked_count == 1
        && states[0] == statmgmt::row_state::SELECTED
        && states[1..]
            .iter()
            .all(|s| *s == statmgmt::row_state::UNSELECTED);

    // The same row again is the player un-picking it.
    press_skill_row(&mut c, first);
    let unpicked = {
        let p = &c.view().expect_app().hud().panels.skills;
        (p.selected_skill, p.selected_index)
    } == (0, -1);
    let back_to_default = skills_panel_state(c.app_mut()) == Some(statmgmt::state::DEFAULT);
    let title_again = pinp_footer_text(&mut c, statmgmt::child::TITLE);

    // And a different row moves the pick rather than adding a second one.
    press_skill_row(&mut c, second);
    let moved = c.view().expect_app().hud().panels.skills.selected_skill == second;
    let states = pinp_row_states(&mut c);
    let still_exactly_one = states
        .iter()
        .filter(|s| **s == statmgmt::row_state::SELECTED)
        .count()
        == 1
        && states[1] == statmgmt::row_state::SELECTED;

    c.assert_behaviour(
        "skills.selection.a-press-picks-a-skill-and-a-second-press-puts-the-footer-back",
        move |_| {
            opened_default
                && title == "Select a Skill to Improve"
                && credits_label == "Skill Credits Available:"
                && experience_label == "Unassigned Experience:"
                && no_raise_button
                && picked
                && footer_swapped
                && exactly_one_is_picked
                && unpicked
                && back_to_default
                && title_again == "Select a Skill to Improve"
                && moved
                && still_exactly_one
        },
    );
    c.shutdown();
}

/// **The invisible tier.** A panel showing the wrong price to raise a skill looks perfectly
/// plausible, so every number below is recomputed here out of the recording's own description and
/// the two shipped tables, read through the dats rather than back out of the panel.
pub(super) fn the_footer_under_a_picked_skill_is_that_characters_own_arithmetic() {
    let mut c = a_recorded_character(SESSION);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);
    let desc = recorded_description(SESSION);

    // Every skill the recording says the character has already trained, with the two prices the
    // panel is meant to show for it.
    let table_of_skills = desc
        .qualities
        .skills
        .as_ref()
        .expect("the recording carries skills");
    let trained: std::collections::BTreeMap<u32, (u32, u32)> = table_of_skills
        .entries
        .iter()
        .filter(|(_, s)| s.sac >= 2)
        .map(|(id, _)| {
            (
                *id,
                (
                    dereth_rules::advancement::skill_cost_to_raise(&q, &skill_table, &xp, *id),
                    dereth_rules::advancement::skill_cost_to_raise_10(&q, &xp, *id),
                ),
            )
        })
        .collect();
    assert!(
        !trained.is_empty(),
        "the recorded character has trained no skill, so this claim has no subject"
    );
    let available = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        2,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => n,
        _ => 0,
    };

    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| trained.contains_key(id))
        .expect("a trained skill has a row on the page");
    let (want_cost, want_cost_10) = trained[&skill];

    press_skill_row(&mut c, skill);
    let picked = c.view().expect_app().hud().panels.skills.selected_skill == skill;

    // A trained skill takes the container with the progress meter in it.
    let state = skills_panel_state(c.app_mut());
    let line_one_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let shown_cost = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_VALUE);
    let line_two_label = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_LABEL);
    let shown_available = pinp_footer_text(&mut c, statmgmt::child::LINE_TWO_VALUE);
    let cost_reads_right = if want_cost == 0 {
        // A skill that can go no further says so rather than showing a price.
        shown_cost == "Infinity!"
    } else {
        shown_cost == statmgmt::num(want_cost)
    };

    // The two buttons' states are the same two comparisons, made here from the same three numbers.
    let want_button = if want_cost == 0 || available < i64::from(want_cost) {
        statmgmt::button_state::DISABLED
    } else {
        statmgmt::button_state::ENABLED
    };
    let want_button_10 = if want_cost_10 == 0 || available < i64::from(want_cost_10) {
        statmgmt::button_state::DISABLED
    } else {
        statmgmt::button_state::ENABLED
    };
    let button = pinp_footer_state(&mut c, statmgmt::child::BUTTON);
    let button_10 = pinp_footer_state(&mut c, statmgmt::child::BUTTON_10);

    // How far through the current point the skill is, computed from the shipped curve.
    let s = q
        .skill(skill)
        .copied()
        .expect("the picked skill is in the recording");
    let sac = dereth_rules::skills::Sac::from_raw(s.sac);
    let rank = usize::from(s.level_from_pp);
    let lo = dereth_rules::advancement::experience_to_skill_level(&xp, sac, rank);
    let hi = dereth_rules::advancement::experience_to_skill_level(&xp, sac, rank + 1);
    #[allow(clippy::cast_precision_loss)]
    let want_meter = if hi == lo {
        0.0f32
    } else {
        (s.pp - lo) as f32 / (hi.wrapping_sub(lo)) as f32
    };
    let meter = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .meter;
    let meter_reads_right = meter.is_some_and(|m| (m - want_meter).abs() < 1e-6);

    c.assert_behaviour(
        "skills.footer.the-numbers-under-a-picked-skill-are-the-characters-own",
        move |_| {
            picked
                && state == Some(statmgmt::state::SELECTION_METER)
                && line_one_label == "Experience To Raise:"
                && cost_reads_right
                && line_two_label == "Unassigned Experience:"
                && shown_available == statmgmt::num(available)
                && button == Some(want_button)
                && button_10 == Some(want_button_10)
                && meter_reads_right
        },
    );
    c.shutdown();
}

/// **What a press on the raise button really asks for**, on both kinds of skill, in bytes.
///
/// Nothing is spent on any shard: no server is contacted and every assertion is over the request
/// the client would send.
pub(super) fn the_raise_button_spends_experience_or_credits_by_what_the_skill_is() {
    // ---- the trained arm ---------------------------------------------------------------------
    let mut c = a_recorded_character(SESSION);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);
    let desc = recorded_description(SESSION);
    let entries = desc
        .qualities
        .skills
        .as_ref()
        .expect("the recording carries skills");
    let trained: Vec<u32> = entries
        .entries
        .iter()
        .filter(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .collect();

    let skill = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| trained.contains(id))
        .expect("a trained skill has a row on the page");
    let want_xp = dereth_rules::advancement::skill_cost_to_raise(&q, &skill_table, &xp, skill);

    press_skill_row(&mut c, skill);
    let (hit, button) = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let landed_on_the_button = hit == Some(button);
    let asked = pinp_skill_requests(&c);
    let one_raise = asked == vec![(0x0046, skill, want_xp)];

    // And the bytes that raise carries, through the client's own sender.
    let mut body = skill.to_le_bytes().to_vec();
    body.extend_from_slice(&want_xp.to_le_bytes());
    let request = dereth_client_model::Request::TrainSkill(dereth_protocol::admin::TrainSkill {
        skill_id: skill,
        xp_spent: want_xp,
    });
    let (framed, payload) = pinp_encoded(&request);
    let bytes_are_right = framed && payload == pinp_action_bytes(0x0046, &body);

    // The double-spend latch: the button puts itself out of reach and a second press asks nothing.
    let latched = c.view().expect_app().hud().panels.skills.awaiting_raise;
    let greyed = pinp_footer_state(&mut c, statmgmt::child::BUTTON)
        == Some(statmgmt::button_state::DISABLED);
    let _ = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let still_one = pinp_skill_requests(&c) == vec![(0x0046, skill, want_xp)];
    c.shutdown();

    // ---- the untrained arm -------------------------------------------------------------------
    let mut c = a_recorded_character(SESSION);
    let untrained = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.skill)
        .find(|id| q.skill(*id).is_none_or(|s| s.sac <= 1) && skill_table.skills.contains_key(id))
        .expect("the recorded character has an untrained skill with a row");
    let want_credits =
        u32::try_from(skill_table.skills[&untrained].trained_cost).expect("a non-negative cost");

    press_skill_row(&mut c, untrained);
    // An untrained skill takes the container *without* a meter, and its line talks about credits.
    let untrained_state = skills_panel_state(c.app_mut());
    let credits_label = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_LABEL);
    let credits_value = pinp_footer_text(&mut c, statmgmt::child::LINE_ONE_VALUE);

    let (hit, button) = pinp_press_the_raise_button(&mut c, statmgmt::child::BUTTON);
    let landed_on_the_untrained_button = hit == Some(button);
    let asked_in_credits = pinp_skill_requests(&c) == vec![(0x0047, untrained, want_credits)];
    let mut credit_body = untrained.to_le_bytes().to_vec();
    credit_body.extend_from_slice(&want_credits.to_le_bytes());
    let credit_request = dereth_client_model::Request::TrainSkillAdvancementClass(
        dereth_protocol::admin::TrainSkillAdvancementClass {
            skill_id: untrained,
            credits_spent: want_credits,
        },
    );
    let (credit_framed, credit_payload) = pinp_encoded(&credit_request);
    let credit_bytes_are_right =
        credit_framed && credit_payload == pinp_action_bytes(0x0047, &credit_body);

    // The gate on the credits arm is not decorative: the same sender refuses a skill the character
    // already has, so a press that reached the wrong arm would send nothing rather than the wrong
    // thing.
    let gate_holds = {
        let mut req = dereth_client_model::RecordingRequests::default();
        let takes_the_untrained =
            dereth_client_model::advancement::send_train_skill_advancement_class(
                &q,
                &mut req,
                untrained,
                want_credits,
            );
        let mut refused = dereth_client_model::RecordingRequests::default();
        let refuses_the_trained =
            !dereth_client_model::advancement::send_train_skill_advancement_class(
                &q,
                &mut refused,
                skill,
                1,
            );
        takes_the_untrained && refuses_the_trained && refused.0.is_empty()
    };

    c.assert_behaviour(
        "skills.raise.the-button-spends-experience-on-a-trained-skill-and-credits-on-an-untrained-one",
        move |_| {
            landed_on_the_button
                && one_raise
                && bytes_are_right
                && latched
                && greyed
                && still_one
                && untrained_state == Some(statmgmt::state::SELECTION)
                && credits_label == "Skill Credits To Train"
                && credits_value == want_credits.to_string()
                && landed_on_the_untrained_button
                && asked_in_credits
                && credit_bytes_are_right
                && gate_holds
        },
    );
    c.shutdown();
}

/// **The colour, not the lettering.** The three colours a skill row's value cell ships are the
/// whole of its palette, and a number asked for a colour the cell does not have is drawn plain
/// rather than not at all -- which is the half that was really broken, because an index the cell
/// does not declare resolves to nothing and rasterises nothing.
///
/// The premise is asserted rather than assumed: this recording's character carries the
/// jack-of-all-trades augmentation and no spell at all, so every skill is raised by the same flat
/// amount and every row legitimately draws in the raised colour. A recording without it would
/// change the expected set, and this says so instead of quietly agreeing.
pub(super) fn a_skill_values_colour_is_one_of_the_rows_own_three() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let skill_table: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let q = recorded_qualities(SESSION);

    // ---- the premise -------------------------------------------------------------------------
    let desc = recorded_description(SESSION);
    let enchantments = desc.qualities.enchantments.as_ref().map_or(0, |e| {
        e.multiplicative.as_ref().map_or(0, Vec::len)
            + e.additive.as_ref().map_or(0, Vec::len)
            + usize::from(e.vitae.is_some())
    });
    assert_eq!(
        enchantments, 0,
        "this recording must carry no spell; it carries {enchantments}"
    );
    assert_eq!(
        q.inq_int(dereth_rules::skills::aug::JACK_OF_ALL_TRADES),
        1,
        "this character must carry jack-of-all-trades for the flat difference below to be it"
    );

    // ---- the oracle, recomputed here ---------------------------------------------------------
    let want: std::collections::BTreeMap<u32, u32> = skill_table
        .skills
        .keys()
        .map(|id| {
            let raw = dereth_rules::skills::inq_skill(&q, &skill_table, *id, true).unwrap_or(0);
            let eff = dereth_rules::skills::inq_skill(&q, &skill_table, *id, false).unwrap_or(0);
            assert_eq!(
                i64::from(eff) - i64::from(raw),
                5,
                "skill {id}: the only source of a difference here is the augmentation"
            );
            (*id, ladder(i64::from(raw), i64::from(eff)))
        })
        .collect();
    let thirty_eight = want.len() == 38 && want.values().all(|f| *f == 1);

    let rows = c.view().expect_app().hud().panels.skills.rows.clone();
    let got: std::collections::BTreeMap<u32, u32> =
        rows.iter().map(|r| (r.skill, r.font)).collect();
    let every_row_agrees = rows.len() == 38 && got == want;

    // ---- the palette the cell itself ships ---------------------------------------------------
    let value_child = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(rows[0].element, ElementId(skills::row::VALUE))
            .expect("the row has a value cell")
    };
    // Three colours, and the words a reader would use for them: white plain, green raised, red
    // lowered. A read that went back through the same constant could not see a wrong one.
    let (white, green, red) = state_colours(c.app_mut(), value_child);
    let one_lettering_three_colours = {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::font_count(ui, value_child) == 1
            && statmgmt::font_color_at(ui, value_child, 3).is_none()
    };

    // ---- and the colour really reaches the glyphs --------------------------------------------
    let raised = glyph_runs(c.app_mut(), value_child).1;
    let raised_is_green =
        !raised.is_empty() && raised.iter().all(|(f, col)| *f == 0 && *col == green);
    // It must also resolve to a real lettering, which is the half that was broken: a cell asked
    // for a lettering it does not declare composes nothing at all.
    let really_composed = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let sb = ui.screen_box(value_child);
        let placed = ui
            .text_element_mut(value_child)
            .map_or_else(Vec::new, |t| t.compose(sb));
        !placed.is_empty() && placed.iter().all(|p| p.font.0 != 0)
    };

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::set_text_with_font(ui, value_child, "45", 0, 2);
    }
    let lowered_is_red = glyph_runs(c.app_mut(), value_child).1 == vec![(0, red), (0, red)];

    // A colour past the end of the cell's own three leaves the colour alone; it must never blank
    // the cell.
    {
        let (ui, _) = gameplay_screen(c.app_mut());
        statmgmt::set_text_with_font(ui, value_child, "45", 9, 9);
    }
    let out_of_range_falls_back =
        glyph_runs(c.app_mut(), value_child).1 == vec![(0, white), (0, white)];

    {
        let (ui, _) = gameplay_screen(c.app_mut());
        if let Some(t) = ui.text_element_mut(value_child) {
            t.set_text("678");
        }
    }
    let plain_text_is_plain = glyph_runs(c.app_mut(), value_child)
        .1
        .iter()
        .all(|(f, _)| *f == 0);

    c.assert_behaviour(
        "skills.rows.the-colour-a-value-is-drawn-in-is-one-of-the-rows-own-three",
        move |_| {
            thirty_eight
                && every_row_agrees
                && one_lettering_three_colours
                && raised_is_green
                && really_composed
                && lowered_is_red
                && out_of_range_falls_back
                && plain_text_is_plain
        },
    );
    c.shutdown();
}

/// **The heading a paired sweep found empty.** The three labels come from the shipped layout; the
/// values beside them were written by nothing, and a page built from the rows up cannot see a
/// blank heading. Both sub-panels are read, because each carries its own copy of the eight
/// elements.
pub(super) fn both_character_pages_head_with_the_name_the_level_and_the_experience() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let xp: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let q = recorded_qualities(SESSION);

    // ---- the oracle, off the recording's own description -------------------------------------
    let total = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        1,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => u64::try_from(n).unwrap_or(0),
        _ => 0,
    };
    let level = q.inq_int(0x19);
    assert!(level > 0, "the recorded character has a level ({level})");
    let this_level =
        dereth_rules::advancement::experience_to_level(&xp, usize::try_from(level).unwrap_or(0))
            .expect("the level is inside the shipped curve");
    let next_level = dereth_rules::advancement::experience_to_level(
        &xp,
        usize::try_from(level).unwrap_or(0) + 1,
    )
    .expect("and so is the next one");
    let to_level = next_level.saturating_sub(total);
    // The name's oracle is the recording's own, read off the description: the object table is not
    // it, because a client is in exactly this window between the description and being put in the
    // world, and the heading has to be right there too.
    let name = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::String,
        1,
    )) {
        Some(dereth_client_model::StatValue::Str(n)) => n,
        _ => String::new(),
    };
    assert!(!name.is_empty(), "the recording names the character");
    #[allow(clippy::cast_precision_loss)]
    let want_meter = (total - this_level) as f32 / (next_level - this_level) as f32;

    // ---- what each sub-panel worked out ------------------------------------------------------
    let mut both_halves_agree = true;
    for got in [
        c.view()
            .expect_app()
            .hud()
            .panels
            .skills
            .header_content
            .clone(),
        c.view()
            .expect_app()
            .hud()
            .panels
            .attributes
            .header_content
            .clone(),
    ] {
        both_halves_agree &= got.name == name
            && got.level == statmgmt::num(level)
            && got.total_xp == statmgmt::num(total)
            && got.xp_to_level == statmgmt::num(to_level)
            && (got.meter - want_meter).abs() < 1e-6;
    }

    // ---- and it is on the live elements, not only in the panel's own copy ---------------------
    let mut both_halves_drew_it = true;
    for panel in [skills::PANEL, attributes::PANEL] {
        for (id, want) in [
            (statmgmt::header::NAME, name.clone()),
            (statmgmt::header::LEVEL, statmgmt::num(level)),
            (statmgmt::header::TOTAL_XP, statmgmt::num(total)),
            (statmgmt::header::XP_TO_LEVEL, statmgmt::num(to_level)),
        ] {
            let h = {
                let (ui, screen) = gameplay_screen(c.app_mut());
                let root = screen.root().expect("the gameplay screen has a root");
                let page = ui
                    .get_child_recursive(root, remaining::CHARACTER_PAGE)
                    .expect("the character page is in the shipped layout");
                let p = ui
                    .get_child_recursive(page, panel)
                    .expect("the sub-panel is in the layout");
                ui.get_child_recursive(p, ElementId(id))
                    .unwrap_or_else(|| panic!("{panel:?} has no {id:#010X}"))
            };
            both_halves_drew_it &= glyph_runs(c.app_mut(), h).0 == want;
        }
    }

    c.assert_behaviour(
        "character.header.the-character-page-heads-with-the-name-the-level-and-the-experience",
        move |_| both_halves_agree && both_halves_drew_it,
    );
    c.shutdown();
}

/// **The mask is the invisible half.** The book filters on one set of bits and the wire carries
/// another, and the only symptom would be a spell quietly missing. The oracle for what is sent is
/// the recording's own filter word with exactly the pressed button's bit cleared.
pub(super) fn turning_a_school_off_hides_its_spells_and_sends_the_whole_list() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    let mask0 = recorded_description(SESSION).player_module.spell_filters;
    let before = c.view().expect_app().hud().panels.spellbook.shown.clone();
    assert!(
        !before.is_empty(),
        "the recorded spellbook has spells to filter"
    );

    // The buttons already say which schools the shard said are showing -- all thirteen of them,
    // read off the live tree.
    let buttons_match_the_shard = {
        let (ui, _) = gameplay_screen(c.app_mut());
        dereth_ui_screens::panels::spellbook::FILTER_BUTTONS
            .iter()
            .all(|(id, bit)| {
                let h = ui
                    .get_element(ElementId(*id))
                    .expect("the filter button is in the tree");
                let want = if mask0 & bit != 0 {
                    dereth_ui_screens::panels::spellbook::BUTTON_ON
                } else {
                    dereth_ui_screens::panels::spellbook::BUTTON_OFF
                };
                ui.node(h).map(|n| n.state.0) == Some(want)
            })
    };

    // Which school every spell on show belongs to -- the one turned off has to be showing.
    let school_of: std::collections::BTreeMap<u32, u32> = {
        use dereth_ui_screens::view::GameView as _;
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        view.spellbook().iter().map(|s| (s.id, s.school)).collect()
    };
    let (button, bit, school) = dereth_ui_screens::panels::spellbook::School::ALL
        .iter()
        .filter_map(|s| {
            let bit = dereth_ui_screens::panels::spellbook::filter_bit(s.button())?;
            let m = s.magic_school();
            before
                .iter()
                .any(|id| school_of.get(id) == Some(&m))
                .then_some((s.button(), bit, m))
        })
        .next()
        .expect("some spell on show belongs to a school with a button of its own");

    pinp_toggle(&mut c, button, false);
    let found = pinp_click(&mut c, button);

    let after = c.view().expect_app().hud().panels.spellbook.shown.clone();
    let the_list_changed = after != before;
    let that_school_is_gone = after.iter().all(|id| school_of.get(id) != Some(&school));
    // ...and one of them was there before, or this proves nothing.
    let it_was_there_before = before.iter().any(|id| school_of.get(id) == Some(&school));

    let sent_off = pinp_filters_sent(&c);
    let client_side = {
        use dereth_ui_screens::view::GameView as _;
        let app = c.view().expect_app();
        app.hud().view(app.objects()).spell_filters()
    };
    let (framed_off, payload_off) =
        pinp_encoded(&dereth_client_model::Request::SpellbookFilterEvent(
            dereth_protocol::combat::CharacterSpellbookFilterEvent {
                filter_mask: mask0 & !bit,
            },
        ));
    let off_bytes_are_right =
        framed_off && payload_off == pinp_action_bytes(0x0286, &(mask0 & !bit).to_le_bytes());

    // Turning it back on restores the book exactly, which is what says the buttons filter rather
    // than edit -- and the message is the whole list again, not the one bit that moved.
    pinp_toggle(&mut c, button, true);
    pinp_click(&mut c, button);
    let the_book_came_back = c.view().expect_app().hud().panels.spellbook.shown == before;
    let sent_both = pinp_filters_sent(&c);

    c.assert_behaviour(
        "spellbook.filter.turning-a-school-off-hides-its-spells-and-sends-the-whole-list",
        move |_| {
            found
                && buttons_match_the_shard
                && the_list_changed
                && that_school_is_gone
                && it_was_there_before
                && sent_off == vec![mask0 & !bit]
                && client_side == mask0 & !bit
                && off_bytes_are_right
                && the_book_came_back
                && sent_both == vec![mask0 & !bit, mask0]
        },
    );
    c.shutdown();
}
