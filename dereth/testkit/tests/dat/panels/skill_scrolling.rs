use super::*;

// =============================================================================================
// skills.scroll.* and skills.rows.* (the number and the colour a row draws)
//
// Every row of the skills list is reachable to a real click once the list is scrolled: the list's
// item-at-point lookup reads the same scroll offset the element's scrollable behavior moves the
// rows by, rather than a copy of its own that nothing writes. A row draws the non-raw skill
// query, the **enchanted** total, and its colour compares the raw value against the effective
// value minus the vitae modifier.
//
// The shipped `0x1000023D` names the scrollbar `0x1000023E` in attribute `0x72`, and the thumb is
// sized and positioned from the same offset. A second, silent copy of that one number is worse
// than an absent one: the two can only ever disagree, and the hit test could read the dead one.
// Hence the thumb geometry and the hit test are asserted as two claims and not one.
// =============================================================================================

/// The stat-management list's vertical scrollbar, attribute `0x72` on `0x1000023D` in the shipped
/// `0x21000005` layout. Read back off the live tree below rather than assumed.
const SKILL_LIST_SCROLLBAR: ElementId = ElementId(0x1000_023E);

/// The skill id for *Melee Defense*, the row the value claims are pinned against: the
/// recorded character's first specialised skill, and a number well clear of zero.
const SKILL_LIST_MELEE_DEFENSE: u32 = 6;
/// *Healing*, and *Jump* -- the second is chosen because `70 * 0.95` is exactly `66.5`, so
/// rounding half up and cutting short disagree on it.
const SKILL_LIST_HEALING: u32 = 21;
const SKILL_LIST_JUMP: u32 = 22;

/// The live `0x1000023D` under the skill panel.
fn skill_list_list_box(c: &mut HeadlessClient) -> ElemHandle {
    let (ui, screen) = gameplay_screen(c.app_mut());
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel is in the layout");
    ui.get_child_recursive(panel, skills::LIST_BOX)
        .expect("the skills list box is in the layout")
}

/// The element's own scrollable behavior: the offset that really moves the rows and the
/// content extent written when the scrollable area is resized.
fn skill_list_scrollable(
    c: &mut HeadlessClient,
    lb: ElemHandle,
) -> dereth_ui::scrollable::Scrollable {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(lb)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .expect("0x1000023D carries a list-box element behaviour")
        .scroll
}

/// The live vertical scroll offset, read off the element and never off a second copy.
fn skill_list_scroll_y(c: &mut HeadlessClient, lb: ElemHandle) -> i32 {
    skill_list_scrollable(c, lb).y
}

/// The bar the list really bound through its relative-element lookup. A bar
/// that is the list's *sibling* would not be found by a walk down from the panel.
fn skill_list_bar(c: &mut HeadlessClient, lb: ElemHandle) -> ElemHandle {
    let s = skill_list_scrollable(c, lb);
    let (ui, _) = gameplay_screen(c.app_mut());
    s.scrollbar(ui, lb, false)
        .expect("the skills list names a vertical bar of its own")
}

fn skill_list_bar_float(c: &mut HeadlessClient, bar: ElemHandle, attr: u32) -> f32 {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(bar)
        .and_then(|n| n.merged_properties().get_float(attr))
        .unwrap_or_else(|| panic!("the bar carries no float attribute {attr:#04x}"))
}

fn skill_list_bar_bool(c: &mut HeadlessClient, bar: ElemHandle, attr: u32) -> Option<bool> {
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.node(bar)
        .and_then(|n| n.merged_properties().get_bool(attr))
}

/// One of the bar's two arrows, named by the bar's increment or decrement button id rather than
/// by a transcribed id.
fn skill_list_arrow(c: &mut HeadlessClient, bar: ElemHandle, down: bool) -> ElemHandle {
    // **The shipped bar's two attributes are named the other way round from what they do here.**
    // The one it calls the increment names the arrow drawn at the *head* of the bar, and that is
    // the one that moves the list up. Which is which is therefore read off the bar and then
    // asserted by where each is drawn and by which way the list really goes -- never assumed from
    // the name, which is exactly the mistake this comment exists to stop.
    let which = if down {
        dereth_ui::widgets::scrollbar::attr::DECREMENT_BUTTON
    } else {
        dereth_ui::widgets::scrollbar::attr::INCREMENT_BUTTON
    };
    let (ui, _) = gameplay_screen(c.app_mut());
    let id = ui
        .node(bar)
        .and_then(|n| n.merged_properties().get_enum(which))
        .map(ElementId)
        .expect("the bar names that arrow");
    ui.get_child_recursive(bar, id)
        .expect("the arrow is a child of the bar")
}

/// One press of one arrow, through the arrow **button** the bar names, so the client picks which
/// message that arrow raises rather than the scenario picking it.
///
/// **One press per fire.** The bar auto-repeats while an arrow is held, so this measures the
/// per-press scroll delta and not the repeat rate.
fn skill_list_press_arrow(c: &mut HeadlessClient, bar: ElemHandle, down: bool) {
    let h = skill_list_arrow(c, bar, down);
    let (cx, cy) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    // **A press and no frame after it.** The arrows are buttons and the bar repeats while one is
    // held, so a press, a frame and a release move the list *twice*; one press per call is what
    // makes the step below a measurement of the scroll delta rather than of the repeat rate.
    let (ui, _) = gameplay_screen(c.app_mut());
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
}

/// Scroll one item into view; report whether the viewport moved.
fn skill_list_bring_into_view(c: &mut HeadlessClient, index: usize) -> bool {
    let app = c.app_mut();
    let mut panels = std::mem::take(&mut app.probe_mut().hud_mut().panels);
    let moved = {
        let (ui, _) = gameplay_screen(app);
        panels
            .skills
            .list
            .as_mut()
            .expect("the skills list is bound")
            .scroll_to_view(ui, index)
    };
    app.probe_mut().hud_mut().panels = panels;
    c.tick(1);
    moved
}

/// The centre of one list **item** -- headers included, so the index is the list's own.
fn skill_list_item_centre(c: &mut HeadlessClient, index: usize) -> ScreenPoint {
    let item = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .list
        .as_ref()
        .expect("the skills list is bound")
        .items
        .get(index)
        .copied()
        .unwrap_or_else(|| panic!("the skills list has no item {index}"));
    let (ui, _) = gameplay_screen(c.app_mut());
    let b = ui.screen_box(item);
    ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// **The discriminator.** What the band walk would name for this point with the scroll offset
/// dropped -- the answer the defect gave. If it is the item really under the pointer then a
/// scrolled station proves nothing, so it is taken before the press and asserted to differ.
fn skill_list_band_walk_without_the_offset(
    c: &mut HeadlessClient,
    lb: ElemHandle,
    y: i32,
) -> usize {
    let heights = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .list
        .as_ref()
        .expect("the skills list is bound")
        .item_heights
        .clone();
    let (ui, _) = gameplay_screen(c.app_mut());
    let local = y - ui.screen_box(lb).y0;
    let mut acc = 0i32;
    let mut idx = 0usize;
    for (i, h) in heights.iter().enumerate() {
        acc += *h;
        if local <= acc {
            idx = i;
            break;
        }
    }
    idx
}

/// Which skill rows are **drawn** under a point, from geometry alone -- an oracle that never
/// calls the list's item-at-point lookup.
fn skill_list_rows_drawn_under(c: &mut HeadlessClient, at: ScreenPoint) -> Vec<u32> {
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
    let (ui, _) = gameplay_screen(c.app_mut());
    rows.iter()
        .filter(|(_, h)| {
            let b = ui.screen_box(*h);
            at.y >= b.y0 && at.y <= b.y1 && at.x >= b.x0 && at.x <= b.x1
        })
        .map(|(s, _)| *s)
        .collect()
}

/// The skill the page has selected. `0` is "none"; selecting the current row again toggles it off.
fn skill_list_selected(c: &HeadlessClient) -> u32 {
    c.view().expect_app().hud().panels.skills.selected_skill
}

/// The three numbers used to build one row: the raw value, the enchanted/effective total, and the
/// vitae modifier, in that order.
fn skill_list_skill_numbers(c: &HeadlessClient, skill: u32) -> (i32, i32, i32) {
    let e = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .find(|e| e.id == skill)
        .unwrap_or_else(|| panic!("skill {skill} is in the retail SkillTable"));
    (e.level, e.effective, e.vitae)
}

/// What one row's value cell **draws**: the text, and the index into the element's own `0x1B`
/// colour array -- so the claim reads as the composition style's colour index, not as an ARGB
/// word. `3` is returned when the colour is none of the three the element declares.
fn skill_list_value_cell(c: &mut HeadlessClient, skill: u32) -> (String, u32) {
    let row = skill_row(c, skill);
    let cell = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(row, ElementId(skills::row::VALUE))
            .expect("the row draws its number in the shipped 0x1000012B")
    };
    let (text, runs) = glyph_runs(c.app_mut(), cell);
    let colour = runs.first().map(|(_, col)| *col);
    let (ui, _) = gameplay_screen(c.app_mut());
    let index = colour
        .and_then(|col| (0..3u32).find(|i| statmgmt::font_color_at(ui, cell, *i) == Some(col)))
        .unwrap_or(3);
    (text, index)
}

/// The colour each row was drawn in, over the whole page, so that "plain" is a property of the
/// panel and not of one row.
fn skill_list_fonts(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| r.font)
        .collect()
}

fn skill_list_by_font(c: &HeadlessClient, f: u32) -> usize {
    skill_list_fonts(c).into_iter().filter(|x| *x == f).count()
}

/// Vitae is a **single** enchantment on the registry rather than a list entry, and its `StatMod`
/// is keyed on `0`. Skill enchantment applies it before culling either spell list.
fn skill_list_vitae(multiplier: f32) -> dereth_protocol::types::qualities::Enchantment {
    dereth_protocol::types::qualities::Enchantment {
        id: 0,
        category_word: 0,
        power_level: 0,
        start_time: 0.0,
        duration: -1.0,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: dereth_rules::enchant::ench_type::VITAE,
            key: 0,
            value: multiplier,
        },
        spell_set_id: None,
    }
}

/// **A constructed station, and what it is built from.** The recording's own character
/// description, with its augmentation removed and an enchantment registry put in its place, then
/// **re-encoded and decoded again** through the production codec before the client ever sees it
/// -- so even the constructed half reaches the client as bytes its own decoder produced. Nothing
/// is synthesised from scratch and no skill entry is hand-written.
///
/// Removing property **326** is how a *plain* baseline is reached at all: the recorded
/// character's `JACK_OF_ALL_TRADES` buffs every skill by five, so with it present a vitae penalty
/// can never bring `eff - vitae` below `raw` and the vitae arm would be unobservable.
fn skill_list_a_character_respun(
    clear_augmentation: bool,
    registry: Option<dereth_protocol::types::qualities::EnchantmentRegistry>,
) -> HeadlessClient {
    use dereth_protocol::types::qualities::quality_flags;

    let mut d = recorded_description(SESSION);
    if clear_augmentation {
        let ints = d
            .qualities
            .base
            .tables
            .ints
            .as_mut()
            .expect("the int table is present");
        let before = ints.entries.len();
        ints.entries.retain(|(k, _)| *k != 326);
        assert_eq!(
            before - 1,
            ints.entries.len(),
            "property 326 was there to remove"
        );
    }
    if let Some(r) = registry {
        d.qualities.flags |= quality_flags::ENCHANTMENT_REGISTRY;
        d.qualities.enchantments = Some(r);
    }
    let bytes = dereth_protocol::write_body(&d).expect("the spliced description re-encodes");
    let back: dereth_protocol::login::LoginPlayerDescription =
        dereth_protocol::read_body(&bytes).expect("and decodes again, byte for byte");
    assert_eq!(
        back.qualities.enchantments, d.qualities.enchantments,
        "the round trip holds"
    );

    let mut c = a_recorded_character(SESSION);
    // A second description is what a client receives on a relog; the arm that unpacks it rebuilds
    // the page itself, and `apply_property_tables` replaces the int table wholesale, which is how
    // the augmentation really goes away rather than merging back in.
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(back)),
    ));
    c.tick(8);
    show_the_skills_page(&mut c);
    c
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight
// ---------------------------------------------------------------------------------------------

/// **Every number the row quoted, reproduced, and one of them corrected.** The row said "42 rows
/// of 20 px in a 159 px box"; **42 is the count of list *items*, not of skills** -- 38 skill rows
/// plus 4 group headers -- and the two are asserted apart so the figure cannot be quoted forward
/// as a skill count again.
///
/// **159 and 160 are both right and they are different numbers.** The box height is inclusive of
/// both edges; hit testing and the thumb proportion use the resulting 160-pixel height. A check
/// written against 159 would be off by one row-tenth and
/// would still look plausible.
///
/// 840 of content over a 160 view is 5.25 viewports: 8 items fit, so 34 of 42 are outside the
/// rectangle at rest, and item-at-point lookup refuses every one of them.
pub(super) fn the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);

    let (items, heights, columns, rows, headers) = {
        let p = &c.view().expect_app().hud().panels.skills;
        let l = p.list.as_ref().expect("the skills list is bound");
        (
            l.items.len(),
            l.item_heights.clone(),
            l.item_widths.len(),
            p.rows.len(),
            p.headers.len(),
        )
    };
    let counted = rows == 38 && headers == 4 && items == 42 && rows + headers == items;
    // One column: `0x1000023D` declares no `0x5F`, so its column count is 1. Every recorded item
    // height is 20, written by the layout update's first pass.
    let one_column = columns == 1 && heights.len() == 42 && heights.iter().all(|h| *h == 20);
    let content: i32 = heights.iter().sum();
    let sums = content == 840;

    let lb = skill_list_list_box(&mut c);
    let b = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.node(lb).expect("the list box has a node").region.box_
    };
    let boxed = (b.y0, b.y1) == (112, 271) && b.y1 - b.y0 == 159 && b.height() == 160;

    // A second, independent reading of the content extent: the live height on the scrollable element
    // rather than off the widget's own array. Two readings that could have disagreed.
    let declared = skill_list_scrollable(&mut c, lb).height == content;

    let fits = b.height() / 20;
    let out_of_sight =
        fits == 8 && items - usize::try_from(fits).expect("a row count fits a usize") == 34;

    c.assert_behaviour(
        "skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight",
        move |_| counted && one_column && sums && boxed && declared && out_of_sight,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is
// ---------------------------------------------------------------------------------------------

/// **The thumb, on its own.** The row asked for the thumb geometry to be asserted *separately*
/// from the hit test, because the two are independent mechanisms that happen to move together.
/// The scrollbar-size and scrollbar-position updates derive the thumb geometry, while
/// item-at-point lookup derives the hit-tested row.
/// The first can work while the second does not, so a check that only said "the list scrolls"
/// would pass on a panel where two thirds of the rows could not be clicked.
///
/// Both numbers are pinned as literals derived from the measured 160 and 840, and each is checked
/// against the formulas the client uses: `0x88` is `min(view_height / content_height, 1.0)`
/// and `0x86` is `scroll_y / (content_height - view_height)`.
///
/// **Two stations, so a stuck attribute cannot pass**: the bar is read at rest and again once the
/// viewport has moved.
pub(super) fn the_bar_beside_the_skills_list_reports_the_view_and_where_it_is() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = skill_list_list_box(&mut c);

    // Attribute `0x72` names the bar and there is no `0x71`, which is why every claim here is on
    // the y axis.
    let s = skill_list_scrollable(&mut c, lb);
    let names_a_bar = s.v_scrollbar == Some(SKILL_LIST_SCROLLBAR) && s.h_scrollbar.is_none();

    let bar = skill_list_bar(&mut c, lb);
    let real_bar = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let kind = ui.node(bar).expect("the bar has a node").ty().0;
        let lbb = ui.node(lb).expect("the list box has a node").region.box_;
        let barb = ui.node(bar).expect("the bar has a node").region.box_;
        // `0x0B` is the scrollbar element type, and the bar spans the list's own rows.
        kind == 0x0B && (barb.y0, barb.y1) == (lbb.y0, lbb.y1)
    };

    // Station 1 -- at rest.
    let at_rest = skill_list_scroll_y(&mut c, lb) == 0;
    let want = 160.0f32 / 840.0f32;
    let sized =
        (skill_list_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION) - want)
            .abs()
            < 1e-6;
    let placed =
        skill_list_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::POSITION) == 0.0;
    // 840 of content in a 160 view: the bar is live, not disabled.
    let live = skill_list_bar_bool(&mut c, bar, dereth_ui::widgets::scrollbar::attr::DISABLED)
        == Some(false);

    // Station 2 -- after four arrow presses, the bar must have moved with the viewport.
    for _ in 0..4 {
        skill_list_press_arrow(&mut c, bar, true);
    }
    let moved = skill_list_scroll_y(&mut c, lb) == 80;
    let want_pos = 80.0f32 / (840.0f32 - 160.0f32);
    let followed =
        (skill_list_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::POSITION)
            - want_pos)
            .abs()
            < 1e-6;
    // The proportion is a property of the content, so it must **not** have moved.
    let still_sized =
        (skill_list_bar_float(&mut c, bar, dereth_ui::widgets::scrollbar::attr::PROPORTION) - want)
            .abs()
            < 1e-6;

    c.assert_behaviour(
        "skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is",
        move |_| {
            names_a_bar
                && real_bar
                && at_rest
                && sized
                && placed
                && live
                && moved
                && followed
                && still_sized
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top
// ---------------------------------------------------------------------------------------------

/// **The cancelling pair, taken apart.** Never drive an operation and its inverse and compare
/// with the start: the down step, the up step and the floor are each asserted on their own, and
/// the arrow drawn at the **top** of the bar is required to be the one that scrolls **up**. Two
/// errors -- the arrows swapped and the sign inverted -- cancel exactly on a vertical bar, which
/// is why this is the shape of check that can see it.
pub(super) fn each_arrow_moves_the_skills_list_one_row_its_own_way() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = skill_list_list_box(&mut c);
    let bar = skill_list_bar(&mut c, lb);

    // Which arrow is which is read off the bar, and where each is drawn is asserted rather than
    // assumed: the one that moves the list **up** is the one at the head of the bar.
    let up = skill_list_arrow(&mut c, bar, false);
    let down = skill_list_arrow(&mut c, bar, true);
    let laid_out = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.screen_box(up).y0 < ui.screen_box(down).y0
    };

    let start = skill_list_scroll_y(&mut c, lb) == 0;
    skill_list_press_arrow(&mut c, bar, true);
    // One press moves one row: 840 / 42 = 20 pixels.
    let one = skill_list_scroll_y(&mut c, lb) == 20;
    skill_list_press_arrow(&mut c, bar, true);
    let two = skill_list_scroll_y(&mut c, lb) == 40;
    skill_list_press_arrow(&mut c, bar, false);
    let back = skill_list_scroll_y(&mut c, lb) == 20;
    skill_list_press_arrow(&mut c, bar, false);
    let home = skill_list_scroll_y(&mut c, lb) == 0;
    // The scroll offset clamps into `0 ..= content - view` unless attribute `0x73`
    // says otherwise, and `0x1000023D` declares no `0x73`, so the local reads 1 and it clamps.
    skill_list_press_arrow(&mut c, bar, false);
    let clamped = skill_list_scroll_y(&mut c, lb) == 0;

    c.assert_behaviour(
        "skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top",
        move |_| laid_out && start && one && two && back && home && clamped,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over
// ---------------------------------------------------------------------------------------------

/// **The binding criterion, and "at a scrolled viewport" is the whole of it**: the same press at
/// the top of the list passes with the defect present, because at scroll offset zero the dead
/// field and the live one agree. Measured before the fix, with the viewport at 240, a press ten
/// pixels below the top of the box -- over the row of skill 6, *Melee Defense* -- resolved to
/// item **0**, the "Specialized" header, and selected nothing.
///
/// Two things are asserted and separately: that the drawn row under the pointer and the row the
/// panel selects are the same one, and that the row the **unscrolled** band walk would have named
/// is a different one -- so a build that ignores the offset cannot pass, and the station cannot
/// be satisfied by the accident that the item happens to be somewhere plausible.
pub(super) fn a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = skill_list_list_box(&mut c);

    // Station 1 -- unscrolled. The control, and the arm the old code got right.
    let first = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .first()
        .expect("the page has 38 rows")
        .skill;
    let at_rest = skill_list_scroll_y(&mut c, lb) == 0;
    press_skill_row(&mut c, first);
    c.tick(1);
    let picked_first = skill_list_selected(&c) == first;
    // Clear the selection again so the scrolled station starts from nothing. Selecting the
    // current row again toggles it off.
    press_skill_row(&mut c, first);
    c.tick(1);
    let cleared = skill_list_selected(&c) == 0;

    // Station 2 -- scrolled, which is the one the claim is about.
    const TARGET: usize = 39;
    let brought = skill_list_bring_into_view(&mut c, TARGET);
    let y = skill_list_scroll_y(&mut c, lb);
    let scrolled = brought && y > 0;
    let at = skill_list_item_centre(&mut c, TARGET);
    // It must really be inside the list box now, or the press proves nothing.
    let inside = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let lbb = ui.screen_box(lb);
        at.y >= lbb.y0 && at.y <= lbb.y1
    };
    let discriminates = skill_list_band_walk_without_the_offset(&mut c, lb, at.y) != TARGET;
    let drawn = skill_list_rows_drawn_under(&mut c, at);
    let exactly_one = drawn.len() == 1;

    c.when(Player::Click(Target::Point(at)));
    c.tick(2);
    let picked = drawn.first().copied() == Some(skill_list_selected(&c));
    // A selection emits no request; the spend is the advancement family's claim.
    let sent_nothing = c
        .view()
        .outbound()
        .iter()
        .all(|r| !matches!(r, dereth_client_model::Request::TrainSkill(_)));

    c.assert_behaviour(
        "skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over",
        move |_| {
            at_rest
                && picked_first
                && cleared
                && scrolled
                && inside
                && discriminates
                && exactly_one
                && picked
                && sent_nothing
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to
// ---------------------------------------------------------------------------------------------

/// **One frame, not two.** The layout update builds the per-column and per-row maxima, resizes the
/// scrollable area, and *then*
/// places every item at `running_sum - scroll_offset`. Without the second half in the same
/// pass the rows sit unscrolled until the next global tick while the vertical offset is still set,
/// and anything that reads geometry in between -- a click, a screenshot -- sees a list that has
/// forgotten where it was scrolled to. The skill-list rebuild runs on
/// `PlayerDescReceived` and on `SkillAdvancementClassChanged`, so a player scrolled two thirds
/// down who trains a skill hits exactly this.
///
/// The rebuild is run with **no frame after it**, which is what makes this a claim about one
/// frame rather than about the tick that follows.
pub(super) fn rebuilding_the_skills_list_keeps_where_it_was_scrolled_to() {
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);
    let lb = skill_list_list_box(&mut c);
    let bar = skill_list_bar(&mut c, lb);

    for _ in 0..6 {
        skill_list_press_arrow(&mut c, bar, true);
    }
    let y = skill_list_scroll_y(&mut c, lb);
    let six = y == 120;

    let skills_now = c.view().expect_app().hud().skills.clone();
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.probe_mut().hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            panels.skills.rebuild(ui, &skills_now);
        }
        app.probe_mut().hud_mut().panels = panels;
    }

    let kept = skill_list_scroll_y(&mut c, lb) == y;
    // The first list item's own box: at offset 120 it must sit 120 px above the list box's top,
    // in the rebuild's own frame.
    let placed = {
        let item0 = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .list
            .as_ref()
            .expect("the skills list is bound")
            .items[0];
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.screen_box(item0).y0 - ui.screen_box(lb).y0 == -y
    };

    c.assert_behaviour(
        "skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to",
        move |_| six && kept && placed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way
// ---------------------------------------------------------------------------------------------

/// **Three stations on one claim.** The row update formats the out slot of the
/// **third** query as a decimal string. That query is
/// the non-raw skill query -- the enchanted total -- and not the *second*, raw query.
///
/// **The corpus reaches the buffed arm without any construction at all, and it is worth saying
/// how.** The recorded character carries no enchantments (both lists empty, vitae 1.0) but does
/// carry `JACK_OF_ALL_TRADES` = 1, which adds **+5** only to the non-raw result. So the enchanted
/// total is the
/// base plus five on every one of the 38 rows, and a panel that drew the raw query would draw a
/// number five too low.
///
/// The numbers are pinned as **literals from the recording** rather than read back through the
/// symbol the production code writes them through: a check that reads a constant through the same
/// symbol it writes it through cannot detect a wrong constant.
///
/// The corpus reaches neither a debuffed row nor a plain one, so those two are constructed -- and
/// the plain one is the control that shows the augmentation was doing the work above: the same
/// character, one property lighter, draws **72** in white where it drew **77** in green.
pub(super) fn a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change() {
    // ---- station 1: the recorded character, buffed on every row by its own augmentation. ------
    let mut c = a_recorded_character(SESSION);
    show_the_skills_page(&mut c);

    let augmented = {
        let q = c
            .view()
            .world()
            .player_qualities()
            .expect("the recorded description")
            .clone();
        q.inq_int(326) == 1 && q.enchantments.vitae_value() == 1.0
    };
    let numbers = skill_list_skill_numbers(&c, SKILL_LIST_MELEE_DEFENSE) == (72, 77, 0);
    // The cell carries the ENCHANTED total, not the base 72; 72 < 77 - 0, so the buffed colour.
    let cell = skill_list_value_cell(&mut c, SKILL_LIST_MELEE_DEFENSE) == ("77".to_owned(), 1);
    // And it is every row, not one: all 38 are +5 and all 38 are green.
    let every_row = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .all(|r| r.effective == r.level + 5)
        && skill_list_fonts(&c).iter().all(|f| *f == 1);

    // ---- station 2: the same character with the augmentation removed and nothing added. -------
    let plain = {
        let mut p = skill_list_a_character_respun(true, None);
        let gone = p
            .view()
            .world()
            .player_qualities()
            .expect("the description")
            .inq_int(326)
            == 0;
        let n = skill_list_skill_numbers(&p, SKILL_LIST_MELEE_DEFENSE) == (72, 72, 0);
        let drawn = skill_list_value_cell(&mut p, SKILL_LIST_MELEE_DEFENSE) == ("72".to_owned(), 0);
        let whole_page = skill_list_fonts(&p).iter().all(|f| *f == 0);
        p.shutdown();
        gone && n && drawn && whole_page
    };

    // ---- station 3: buffed and debuffed, on one character, at once. ---------------------------
    //
    // The corpus reaches the buffed arm only through an augmentation that applies to every skill
    // uniformly, so it can never show a *debuffed* row and it can never show the two side by
    // side. One `+20` and one `-20` on the plain character put all three colours on one panel, so
    // a mistake that colours the whole list one way cannot pass.
    let both = {
        let reg = dereth_protocol::types::qualities::EnchantmentRegistry {
            flags: dereth_protocol::types::qualities::EnchantmentRegistry::ADDITIVE,
            additive: Some(vec![
                spell_on(4700, 110, SKILL_LIST_MELEE_DEFENSE, 20.0),
                spell_on(4701, 111, SKILL_LIST_HEALING, -20.0),
            ]),
            ..dereth_protocol::types::qualities::EnchantmentRegistry::default()
        };
        let mut p = skill_list_a_character_respun(true, Some(reg));
        let n = skill_list_skill_numbers(&p, SKILL_LIST_MELEE_DEFENSE) == (72, 92, 0)
            && skill_list_skill_numbers(&p, SKILL_LIST_HEALING) == (58, 38, 0);
        let up = skill_list_value_cell(&mut p, SKILL_LIST_MELEE_DEFENSE) == ("92".to_owned(), 1);
        let down = skill_list_value_cell(&mut p, SKILL_LIST_HEALING) == ("38".to_owned(), 2);
        // The rest of the panel is untouched -- the enchantments are keyed, so exactly two rows
        // move. A colour bug that painted the list uniformly would fail here.
        let rest = (
            skill_list_by_font(&p, 0),
            skill_list_by_font(&p, 1),
            skill_list_by_font(&p, 2),
        ) == (36, 1, 1);
        p.shutdown();
        n && up && down && rest
    };

    c.assert_behaviour(
        "skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way",
        move |_| augmented && numbers && cell && every_row && plain && both,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered
// ---------------------------------------------------------------------------------------------

/// **The case the vitae term is invisible without.** The non-raw skill query already carries
/// vitae -- the skill-enchantment calculation applies it before culling either spell list -- so
/// the naive comparison of `raw` against `eff` reads `72 < 68` for every skill and **paints the
/// whole list red**. The row color calculation subtracts the vitae modifier, which is
/// `Enchant(raw) - raw` with only vitae applied and is therefore `-4` here, so `eff - v` is `72`
/// and the row draws plain.
///
/// The second half separates the two readings in the *other* direction: a `+8` buff under vitae
/// gives `eff = 63 < 58` raw on a naive reading, which paints **red**, where the client paints
/// **green**. Both directions, so the term cannot be right for the wrong reason.
///
/// The arithmetic is `raw * 0.95 + 0.5`, truncated. Melee Defense gives `68.4` either way, so a
/// second skill is pinned whose
/// product has a fractional part of exactly a half: *Jump*'s `70 * 0.95` is `66.5`, which the
/// client shows as **67** and a truncating build shows as 66. A case whose every product falls
/// below a half cannot see that `+ 0.5` at all.
pub(super) fn a_weakened_character_shows_the_lower_number_still_drawn_plain() {
    let reg = dereth_protocol::types::qualities::EnchantmentRegistry {
        flags: dereth_protocol::types::qualities::EnchantmentRegistry::VITAE
            | dereth_protocol::types::qualities::EnchantmentRegistry::ADDITIVE,
        additive: Some(vec![spell_on(4702, 112, SKILL_LIST_HEALING, 8.0)]),
        vitae: Some(skill_list_vitae(0.95)),
        ..dereth_protocol::types::qualities::EnchantmentRegistry::default()
    };
    let mut c = skill_list_a_character_respun(true, Some(reg));

    let reached = c
        .view()
        .world()
        .player_qualities()
        .expect("the spliced description")
        .enchantments
        .vitae_value()
        == 0.95;

    // Vitae alone: 72 * 0.95 = 68.4, + 0.5, truncated: 68 -- and the row is PLAIN, because
    // 72 == 68 - (-4).
    let alone = skill_list_skill_numbers(&c, SKILL_LIST_MELEE_DEFENSE) == (72, 68, -4)
        && skill_list_value_cell(&mut c, SKILL_LIST_MELEE_DEFENSE) == ("68".to_owned(), 0);

    // The rounding, on the panel: 70 * 0.95 = 66.5, + 0.5 = 67.0, truncated: 67, not 66.
    let rounding = skill_list_skill_numbers(&c, SKILL_LIST_JUMP) == (70, 67, -3)
        && skill_list_value_cell(&mut c, SKILL_LIST_JUMP) == ("67".to_owned(), 0);

    // Vitae plus a small buff -- green, though the number drawn is below the base level:
    // 58 * 0.95 = 55.1 -> 55, + 8 = 63; 55 - 58 = -3; and 58 < 63 - (-3) = 66.
    let buffed_under_it = skill_list_skill_numbers(&c, SKILL_LIST_HEALING) == (58, 63, -3)
        && skill_list_value_cell(&mut c, SKILL_LIST_HEALING) == ("63".to_owned(), 1);

    // The whole list, so "plain" is a property of the panel and not of one row: vitae alone
    // colours nothing, exactly the buffed row is green, and nothing is red.
    let whole_page = (
        skill_list_by_font(&c, 0),
        skill_list_by_font(&c, 1),
        skill_list_by_font(&c, 2),
    ) == (37, 1, 0);

    c.assert_behaviour(
        "skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered",
        move |_| reached && alone && rounding && buffed_under_it && whole_page,
    );
    c.shutdown();
}
