use super::*;

/// The character page of the toolbar's panel stack, raised the way the toolbar button raises it.
pub(super) fn open_the_character_page(c: &mut HeadlessClient) {
    let app = c.app_mut();
    {
        let (ui, screen) = gameplay_screen(app);
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == remaining::CHARACTER_PAGE)
            .map(|p| p.panel_id)
            .expect("the character page is one of the shipped registered pages");
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    c.tick(3);
}

/// Raise the character page and select its skills tab.
///
/// **Selecting the tab rebuilds the list**, so a row handle taken before this call is a handle to
/// an element that no longer exists: every caller resolves its row afterwards.
pub(super) fn show_the_skills_page(c: &mut HeadlessClient) {
    open_the_character_page(c);
    // **The tab is a toggle, not a destination.** Pressing it while its own sub-panel is already
    // up moves the page to the other one, which is how a second selection in one scenario ends up
    // clicking an attribute row and reading "select a skill" back.
    let already_up = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, skills::PANEL)
            .is_some_and(|h| ui.is_visible(h))
    };
    if already_up {
        return;
    }
    let app = c.app_mut();
    let tab = {
        let (ui, screen) = gameplay_screen(app);
        let root = screen.root().expect("the gameplay screen has a root");
        ui.get_child_recursive(root, remaining::CHARACTER_PAGE)
            .and_then(|ph| {
                let n = ui.node(ph)?;
                let b = n.behaviour.as_ref()?;
                let p = (**b)
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()?;
                p.tab_to_page
                    .iter()
                    .find(|(_, pg)| **pg == skills::PANEL)
                    .map(|(t, _)| *t)
            })
            .and_then(|t| ui.get_child_recursive(root, t))
    };
    if let Some(th) = tab {
        let (ui, _) = gameplay_screen(app);
        ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    }
    c.tick(3);
}

/// The live row `skill` is drawn in, on the page as it stands now.
pub(super) fn skill_row(c: &HeadlessClient, skill: u32) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == skill)
        .map(|r| r.element)
        .unwrap_or_else(|| panic!("skill {skill} has no row on the page"))
}

/// Press `skill`'s row **through the real pointer**: the page is raised, the row is scrolled into
/// view, and `Player::Click` goes to its centre through the client's own pump and input manager.
///
/// The row is addressed by **screen position** and not by element id on purpose: a list box builds
/// its rows from one template, so every row carries the same shipped id and `Target::Element`
/// would always answer the first one.
pub(super) fn press_skill_row(c: &mut HeadlessClient, skill: u32) {
    show_the_skills_page(c);
    let h = skill_row(c, skill);
    {
        let app = c.app_mut();
        let mut panels = std::mem::take(&mut app.probe_mut().hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            if let Some(w) = panels.skills.list.as_mut() {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.probe_mut().hud_mut().panels = panels;
    }
    c.tick(1);
    let (x, y) = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(ScreenPoint::new(x, y))));
}

/// One footer child, resolved the way the panel resolves it: the sub-panel's own state picks the
/// footer container, then the child id.
pub(super) fn footer_child(app: &mut App, child: u32) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel is in the shipped layout");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let container = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container is in the shipped layout");
    ui.get_child_recursive(container, ElementId(child))
        .expect("the footer child is in the shipped layout")
}

/// The skills sub-panel's own state, which is what selects the footer.
pub(super) fn skills_panel_state(app: &mut App) -> Option<u32> {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root()?;
    let page = ui.get_child_recursive(root, remaining::CHARACTER_PAGE)?;
    let panel = ui.get_child_recursive(page, skills::PANEL)?;
    ui.node(panel).map(|n| n.state.0)
}

/// One additive enchantment of `family` keyed on `key`, as the shard sends one.
pub(super) fn enchantment(
    spell_id: u16,
    family: u32,
    key: u32,
    delta: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    use dereth_client_model::enchant::ench_type;
    dereth_protocol::types::qualities::Enchantment {
        id: u32::from(spell_id),
        category_word: 0,
        power_level: 1,
        start_time: 0.0,
        duration: 600.0,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: family | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
            key,
            value: delta,
        },
        spell_set_id: None,
    }
}

/// Land one enchantment on the client, through the shard's own message.
fn land(c: &mut HeadlessClient, e: dereth_protocol::types::qualities::Enchantment) {
    deliver(c, &dereth_protocol::qualities::MagicUpdateEnchantment(e));
}

/// The oracle's own copy of the registry takes the same enchantment.
fn oracle_takes(
    q: &mut dereth_client_model::Qualities,
    e: &dereth_protocol::types::qualities::Enchantment,
) {
    let ge = dereth_client_model::enchant::Enchantment::from_wire(e, LocalTime(0.0));
    assert!(
        q.enchantments.update_enchantment(ge),
        "the oracle's registry took the enchantment"
    );
}

/// The ladder both row updaters share: raised is 1, lowered is 2, unmodified is 0.
pub(super) const fn ladder(raw: i64, effective: i64) -> u32 {
    if raw < effective {
        1
    } else if effective < raw {
        2
    } else {
        0
    }
}

// =============================================================================================
// skills.list.*
// =============================================================================================

/// **The gate.** After the shard's description has landed, the skills page holds one heading per
/// group and one row per skill, and every one of them was really created.
pub(super) fn the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);

    let skills: Vec<u32> = c
        .view()
        .expect_app()
        .hud()
        .skills
        .iter()
        .map(|s| s.id)
        .collect();
    let panel = &c.view().expect_app().hud().panels.skills;
    assert!(panel.list.is_some(), "the shipped list box bound");

    // The four headings are pinned as a number as well as read through the table: a check that
    // reads a constant through the symbol it writes it through cannot see a wrong one.
    assert_eq!(
        skills::HEADER_TEMPLATES.len(),
        4,
        "the page makes four group headings"
    );
    let created = panel.rows_created() as usize;
    let failures = panel.create_failures();
    let rows = panel.rows.len();
    let shown: std::collections::BTreeSet<u32> = panel.shown().into_iter().collect();

    // And the rows are the recording's own skills, so the count above counts something real.
    let every_skill_has_a_row = skills.iter().all(|id| shown.contains(id));

    c.assert_behaviour(
        "skills.list.a-header-per-group-and-a-row-per-skill-the-shard-sent",
        move |_| {
            created > 0
                && created == 4 + rows
                && failures == 0
                && rows == skills.len()
                && every_skill_has_a_row
        },
    );
    c.shutdown();
}

/// The other half, and the reason the page keeps a count of what it built at all: **a page that
/// built nothing reports no failure either**, so the failure count cannot be the guard.
///
/// Three states of one instrument, on one live list box: never rebuilt, rebuilt with no character
/// behind it, and rebuilt with the recording's own skills. The first two are indistinguishable by
/// failures alone, and the third is the known positive that says the instrument can move.
pub(super) fn an_empty_skills_page_is_distinguishable_from_a_full_one() {
    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let skills: Vec<dereth_ui_screens::view::SkillEntry> =
        c.view().expect_app().hud().skills.clone();

    let app = c.app_mut();
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay screen has a root");
    let page = ui
        .get_child_recursive(root, remaining::CHARACTER_PAGE)
        .expect("the character page is in the shipped layout");

    // A second panel bound to the same live list box, so its counters start at zero without
    // disturbing the one the client drives.
    let mut fresh = skills::SkillsPanel::default();
    fresh.post_init(ui, page);
    let bound = fresh.list.is_some();
    let untouched = (fresh.rows_created(), fresh.create_failures());

    fresh.rebuild(ui, &[]);
    let with_nobody = (fresh.rows_created(), fresh.create_failures());

    fresh.rebuild(ui, &skills);
    let with_the_character = (
        fresh.rows_created() as usize,
        fresh.create_failures(),
        fresh.rows.len(),
    );

    c.assert_behaviour(
        "skills.list.a-page-that-built-nothing-is-told-apart-from-one-that-built-everything",
        move |_| {
            bound
                && untouched == (0, 0)
                // The ambiguity, measured: the unbuilt page and the built one agree on failures.
                && with_nobody == (0, 0)
                && with_the_character == (4 + skills.len(), 0, skills.len())
        },
    );
    c.shutdown();
}

// =============================================================================================
// skills.footer.*
//
// The footer title's two claims -- the gain and the loss -- are one row: the footer carries the
// change, signed and coloured, and an implementation that got one arm right and the other wrong
// fails here.
// =============================================================================================

/// **The gate.** Select a skill: the footer is titled `Name: value`, in the plain colour, and the
/// change a spell made follows it in brackets -- green and signed `+` when it is a gain, red when
/// it is a loss.
pub(super) fn the_footer_title_is_the_name_the_number_and_the_signed_change() {
    let q = recorded_qualities(SESSION);

    // ---- the gain the recorded character already carries -------------------------------------
    //
    // **Two clients, and not one with two selections in it.** Pressing the row a second time is
    // pressing the row that is *already selected*, which the list box reads as the player
    // un-selecting it -- the footer goes back to saying "select a skill" and the second arm would
    // be measuring that instead of the debuff. So each arm gets a fresh client.
    let (gain_reads, gain_font, gain_copy, want_gain, selected, metered, skill, name, white, red) = {
        let mut c = a_recorded_character(SESSION);
        let t: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
        let (skill, name) = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| q.skill(r.skill).is_some_and(|s| s.sac >= 2))
            .map(|r| (r.skill, r.name.clone()))
            .expect("the recorded character has a trained skill with a row");

        let raw =
            i64::from(dereth_client_model::skills::inq_skill(&q, &t, skill, true).expect("raw"));
        let gained = i64::from(
            dereth_client_model::skills::inq_skill(&q, &t, skill, false).expect("enchanted"),
        );
        assert!(
            q.enchantments.vitae.is_none(),
            "the recorded character carries no vitae"
        );
        let up = gained - raw;
        assert!(
            up > 0,
            "the premise: this character's skill {skill} is already raised"
        );

        press_skill_row(&mut c, skill);
        let selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;
        let metered = skills_panel_state(c.app_mut()) == Some(statmgmt::state::SELECTION_METER);

        let title = footer_child(c.app_mut(), statmgmt::child::TITLE);
        let (white, green, red) = state_colours(c.app_mut(), title);
        let (text, glyphs) = glyph_runs(c.app_mut(), title);
        let base = format!("{name}: {gained}");
        let suffix = format!(" (+{up})");
        let n = base.encode_utf16().count();
        let reads = text == format!("{base}{suffix}")
            && glyphs.len() == n + suffix.encode_utf16().count()
            && one_run(&glyphs[..n], &base, white)
            && one_run(&glyphs[n..], &suffix, green);
        let font = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .footer_content
            .title_font;
        let copy = c
            .view()
            .expect_app()
            .hud()
            .panels
            .skills
            .footer_content
            .title
            .clone();
        let want = format!("{base}{suffix}");
        c.shutdown();
        (
            reads, font, copy, want, selected, metered, skill, name, white, red,
        )
    };

    // ---- the loss, which no recording carries, through the shard's own message ----------------
    let mut c = a_recorded_character(SESSION);
    let t: dereth_assets::tables::SkillTable = table(&c, 0x0E00_0004);
    let mut q2 = q.clone();
    let e = enchantment(
        0x1234,
        dereth_client_model::enchant::ench_type::SKILL,
        skill,
        -20.0,
    );
    oracle_takes(&mut q2, &e);
    let raw2 =
        i64::from(dereth_client_model::skills::inq_skill(&q2, &t, skill, true).expect("raw"));
    let lost = i64::from(
        dereth_client_model::skills::inq_skill(&q2, &t, skill, false).expect("enchanted"),
    );
    let down = lost - raw2;
    assert!(
        down < 0,
        "the premise: the loss outweighs what the character already had"
    );

    // **One spell landing is enough.** The panel does not wait for a skill update:
    // `Magic_UpdateEnchantment` raises `EnchantmentsChanged`, the frame's panels callback answers
    // it, and the answer is the rebuild. No second message is sent here, so the row can only move
    // because the enchantment itself rebuilt it.
    land(&mut c, e);
    c.tick(3);

    press_skill_row(&mut c, skill);
    let still_selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;

    let title = footer_child(c.app_mut(), statmgmt::child::TITLE);
    let (text, glyphs) = glyph_runs(c.app_mut(), title);
    let base2 = format!("{name}: {lost}");
    let suffix2 = format!(" ({down})");
    let n2 = base2.encode_utf16().count();
    let loss_reads = text == format!("{base2}{suffix2}")
        && glyphs.len() == n2 + suffix2.encode_utf16().count()
        && one_run(&glyphs[..n2], &base2, white)
        && one_run(&glyphs[n2..], &suffix2, red);
    let loss_font = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .footer_content
        .title_font;

    c.assert_behaviour(
        "skills.footer.the-title-is-the-name-and-the-number-with-the-change-spelled-out-after-it",
        move |_| {
            selected
                && metered
                && gain_reads
                && gain_font == 1
                && gain_copy == want_gain
                && still_selected
                && loss_reads
                && loss_font == 2
        },
    );
    c.shutdown();
}

/// **The words under the title, and the fact that they can be read.**
///
/// A line that holds the right text can still draw nothing, so text alone is not the observable:
/// the line has to compose glyphs in a real font at an opaque colour, and nothing the panel paints
/// after it may cover its box. The meter behind it is opaque and is painted **first**.
pub(super) fn the_experience_to_raise_line_is_legible_and_uncovered() {
    let mut c = a_recorded_character(SESSION);
    let q = recorded_qualities(SESSION);
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

    press_skill_row(&mut c, skill);
    let selected = c.view().expect_app().hud().panels.skills.selected_skill == skill;
    let metered = skills_panel_state(c.app_mut()) == Some(statmgmt::state::SELECTION_METER);

    let label = footer_child(c.app_mut(), statmgmt::child::LINE_ONE_LABEL);
    let meter = footer_child(c.app_mut(), statmgmt::child::METER);
    let (text, _) = glyph_runs(c.app_mut(), label);

    let (ui, _) = gameplay_screen(c.app_mut());
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let has_trough = ui
        .node(meter)
        .and_then(|n| n.region.image.as_ref())
        .is_some();
    let at = |who: ElemHandle| back.calls.iter().position(|call| call.who == who);
    let meter_at = at(meter).expect("the meter draws");
    let label_at = at(label).expect("the label draws");
    let box_of = ui.screen_box(label);
    let covering = back.calls[label_at + 1..]
        .iter()
        .filter(|call| {
            (call.image.is_some() || call.fills.iter().any(|f| !f.is_invisible()))
                && call.clip.intersect(&box_of).is_valid()
                && call.screen.intersect(&box_of).is_valid()
        })
        .count();
    let visible = ui.is_visible(label);
    let boxed = box_of.x1 > box_of.x0 && box_of.y1 > box_of.y0;
    let placed = ui
        .text_element_mut(label)
        .map_or_else(Vec::new, |t| t.compose(box_of));
    let composed = !placed.is_empty()
        && placed.iter().all(|p| p.font.0 != 0)
        && placed.iter().all(|p| p.color >> 24 != 0);

    c.assert_behaviour(
        "skills.footer.the-experience-line-is-legible-and-nothing-is-painted-over-it",
        move |_| {
            selected
                && metered
                && text == "Experience To Raise:"
                && has_trough
                && meter_at < label_at
                && covering == 0
                && visible
                && boxed
                && composed
        },
    );
    c.shutdown();
}

// =============================================================================================
// attributes.rows.*
// =============================================================================================

/// The row element carrying a stat's value: child `VALUE` of the row.
fn value_of(app: &mut App, row: ElemHandle) -> ElemHandle {
    let (ui, _) = gameplay_screen(app);
    ui.get_child_recursive(row, ElementId(skills::row::VALUE))
        .expect("the row's value element")
}

/// One attribute or vital row of the live panel.
pub(super) fn attribute_row(c: &HeadlessClient, stat: u32, secondary: bool) -> ElemHandle {
    c.view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.stat == stat && r.secondary == secondary)
        .map(|r| r.element)
        .unwrap_or_else(|| panic!("no row for stat {stat} secondary={secondary}"))
}

/// The three colours the row's value element declares. See [`state_colours`].
fn value_colours(c: &mut HeadlessClient, row: ElemHandle) -> (u32, u32, u32) {
    let value = value_of(c.app_mut(), row);
    state_colours(c.app_mut(), value)
}

/// What the row's value element says, and the runs it says it in.
fn value_runs(c: &mut HeadlessClient, row: ElemHandle) -> (String, Vec<(u32, u32)>) {
    let value = value_of(c.app_mut(), row);
    glyph_runs(c.app_mut(), value)
}

/// What one row draws and in what colour, plus the colour index the panel kept for itself.
fn row_reads(c: &mut HeadlessClient, row: ElemHandle, want: &str, colour: u32) -> bool {
    let (text, glyphs) = value_runs(c, row);
    let font = c
        .view()
        .expect_app()
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.element == row)
        .map(|r| r.font);
    text == want && one_run(&glyphs, want, colour) && font.is_some()
}

/// **The gate.** Six attribute rows, one raised, one lowered, four untouched: each draws the
/// number the character actually has, in the colour that says which of the three it is.
pub(super) fn a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white() {
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let q = recorded_qualities(SESSION);

    // The premise: nothing on this character touches an attribute, so every row starts white.
    let plain = attributes::ATTRIBUTE_ROWS.iter().all(|(stat, _)| {
        dereth_client_model::attributes::inq_attribute(&q, *stat, true)
            == dereth_client_model::attributes::inq_attribute(&q, *stat, false)
    });
    let strength = attribute_row(&c, 1, false);
    let (white, green, red) = value_colours(&mut c, strength);
    let mut started_plain = true;
    for (stat, _) in attributes::ATTRIBUTE_ROWS {
        let eff =
            dereth_client_model::attributes::inq_attribute(&q, stat, false).expect("enchanted");
        let row = attribute_row(&c, stat, false);
        started_plain &= row_reads(&mut c, row, &eff.to_string(), white);
    }

    // One spell raises Strength, another lowers Endurance, through the shard's own message.
    let up = enchantment(0x1234, ench_type::ATTRIBUTE, 1, 20.0);
    let down = enchantment(0x1235, ench_type::ATTRIBUTE, 2, -20.0);
    let mut q2 = q.clone();
    oracle_takes(&mut q2, &up);
    oracle_takes(&mut q2, &down);
    land(&mut c, up);
    land(&mut c, down);
    c.tick(3);
    let mut all_six = true;
    let mut seen = [false; 3];
    for (stat, _) in attributes::ATTRIBUTE_ROWS {
        let raw = i64::from(
            dereth_client_model::attributes::inq_attribute(&q2, stat, true).expect("raw"),
        );
        let eff = i64::from(
            dereth_client_model::attributes::inq_attribute(&q2, stat, false).expect("enchanted"),
        );
        let font = ladder(raw, eff);
        seen[font as usize] = true;
        let colour = [white, green, red][font as usize];
        let row = attribute_row(&c, stat, false);
        all_six &= row_reads(&mut c, row, &eff.to_string(), colour);
    }

    c.assert_behaviour(
        "attributes.rows.a-buffed-value-is-green-a-debuffed-one-red-and-an-unmodified-one-white",
        move |_| plain && started_plain && all_six && seen == [true, true, true],
    );
    c.shutdown();
}

/// A vital row is `current/maximum`, and the colour follows the **maximum**: a spell that raises
/// the ceiling colours the row even though the current value did not move.
pub(super) fn a_vital_row_is_current_over_maximum_and_colours_by_the_maximum() {
    use dereth_client_model::attributes::{inq_attribute_2nd, vital};
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let q = recorded_qualities(SESSION);
    let t: dereth_assets::tables::Attribute2ndTable = table(&c, 0x0E00_0003);
    let f: dereth_assets::tables::QualityFilter = table(&c, 0x0E01_0001);
    let f = Some(&f);
    assert!(
        q.enchantments.vitae.is_none(),
        "the recorded character carries no vitae"
    );

    let plain = attributes::SECONDARY_ROWS.iter().all(|(stat, _, _)| {
        let max = stat - 1;
        inq_attribute_2nd(&q, &t, max, true, f) == inq_attribute_2nd(&q, &t, max, false, f)
    });

    let up = enchantment(0x1236, ench_type::SECOND_ATT, vital::MAX_HEALTH, 50.0);
    let down = enchantment(0x1237, ench_type::SECOND_ATT, vital::MAX_STAMINA, -50.0);
    let mut q2 = q.clone();
    oracle_takes(&mut q2, &up);
    oracle_takes(&mut q2, &down);
    land(&mut c, up);
    land(&mut c, down);
    c.tick(3);

    let health = attribute_row(&c, 2, true);
    let (white, green, red) = value_colours(&mut c, health);
    let mut all_three = true;
    let mut seen = [false; 3];
    for (stat, _, _) in attributes::SECONDARY_ROWS {
        let max = stat - 1;
        let rawmax = i64::from(inq_attribute_2nd(&q2, &t, max, true, f).expect("raw maximum"));
        let effmax =
            i64::from(inq_attribute_2nd(&q2, &t, max, false, f).expect("enchanted maximum"));
        let cur = inq_attribute_2nd(&q2, &t, stat, false, f).expect("current");
        let font = ladder(rawmax, effmax);
        seen[font as usize] = true;
        let colour = [white, green, red][font as usize];
        let row = attribute_row(&c, stat, true);
        all_three &= row_reads(&mut c, row, &format!("{cur}/{effmax}"), colour);
    }

    c.assert_behaviour(
        "attributes.rows.a-vital-row-is-current-over-maximum-and-takes-its-colour-from-the-maximum",
        move |_| plain && all_three && seen == [true, true, true],
    );
    c.shutdown();
}

/// A spell whose effect is a fraction is rounded to the nearest whole number **in the rows the
/// player reads**, and casting the same spell again over itself refreshes the row.
pub(super) fn a_fractional_enchantment_rounds_in_the_rows_the_player_reads() {
    use dereth_client_model::enchant::ench_type;

    let mut c = a_recorded_character(SESSION);
    open_the_character_page(&mut c);
    let strength = attribute_row(&c, 1, false);
    let health = attribute_row(&c, 2, true);

    let (base_strength, base_health) = {
        let s = value_runs(&mut c, strength).0;
        let h = value_runs(&mut c, health).0;
        (s.parse::<u32>().expect("the strength row is a number"), h)
    };
    let (current, maximum) = base_health
        .split_once('/')
        .expect("the health row is current over maximum");
    let base_max: u32 = maximum.parse().expect("the maximum is a number");
    let current = current.to_owned();
    let (white, green, _red) = value_colours(&mut c, strength);

    // Either side of a half, and the half itself. The expected steps are literals: an oracle that
    // shared the client's own rounding could share its mistake too.
    let mut rounded = true;
    for (delta, step) in [(0.49_f32, 0_u32), (0.5, 1), (0.51, 1)] {
        land(&mut c, enchantment(0x1234, ench_type::ATTRIBUTE, 1, delta));
        land(&mut c, enchantment(0x1235, ench_type::SECOND_ATT, 1, delta));
        c.tick(3);
        let colour = if step == 0 { white } else { green };
        rounded &= row_reads(
            &mut c,
            strength,
            &(base_strength + step).to_string(),
            colour,
        );
        rounded &= row_reads(
            &mut c,
            health,
            &format!("{current}/{}", base_max + step),
            colour,
        );
    }

    c.assert_behaviour(
        "attributes.rows.a-fractional-enchantment-rounds-the-way-the-panel-draws-it",
        move |_| rounded,
    );
    c.shutdown();
}

// =============================================================================================
// advancement.cost.*
//
// A `+10` button must light the same under an optimised and an unoptimised build: every step of
// the arithmetic between the shipped table and the button is a plain integer comparison, so the
// two profiles cannot differ.
// =============================================================================================

/// The character these scenarios cost against: an attribute at rank 30, with what has been spent
/// on it and the experience still unassigned.
const RANK: u32 = 30;
const SPENT: u32 = 32_676;
const AVAILABLE: u64 = 20_887_465;

/// **The table, and the two ways ten points are costed.** An attribute's ten points cost the
/// distance from where the character is to ten ranks on; a skill's cost the distance between the
/// same two entries of its own table, and **not** the distance to the table's last entry.
pub(super) fn ten_points_cost_the_distance_between_two_entries_of_the_shipped_table() {
    use dereth_client_model::advancement as adv;
    use dereth_client_model::qualities::Qualities;
    use dereth_client_model::skills::Sac;
    use dereth_protocol::types::qualities::Skill;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let t: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);

    let attribute = adv::max_attribute_level(&t) == 190
        && adv::attribute_cost_to_raise(&t, RANK, SPENT, false) == 2_456
        && adv::attribute_cost_to_raise_10(&t, RANK, SPENT, false) == 31_202;

    // The sibling arithmetic: any skill id, because what is read of the record is its class, its
    // level and what has been sunk into it.
    let id = 1;
    let mut skill = true;
    for sac in [Sac::Trained, Sac::Specialized] {
        let column = if sac == Sac::Trained {
            &t.trained_xp
        } else {
            &t.specialized_xp
        };
        let mut q = Qualities::new();
        q.set_skill(
            id,
            Skill {
                level_from_pp: 30,
                format_version: 1,
                sac: sac as u32,
                pp: column[30],
                init_level: 0,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            },
        );
        let want = column[40] - column[30];
        // The premise: ten ranks on and the table's end are different numbers, so a cost that
        // reached for the cap would be visible here.
        skill &= want != column[column.len() - 1] - column[30]
            && adv::skill_cost_to_raise_10(&q, &t, id) == want;
    }

    c.assert_behaviour(
        "advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table",
        move |_| attribute && skill,
    );
    c.shutdown();
}

/// **At rank 30.** The `+10` and the `+1` are both affordable against the experience that
/// character had not spent, both from the literal cost and from the cost the table really answers.
pub(super) fn the_plus_ten_button_lights_when_the_unassigned_experience_covers_it() {
    use dereth_client_model::advancement as adv;
    use dereth_ui_screens::panels::statmgmt::{button_state, Footer};

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let t: dereth_assets::tables::XpTable = table(&c, 0x0E00_0018);
    let cost_10 = u64::from(adv::attribute_cost_to_raise_10(&t, RANK, SPENT, false));
    let cost_1 = u64::from(adv::attribute_cost_to_raise(&t, RANK, SPENT, false));

    let lit = Footer::enable_for(31_202, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(2_456, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(cost_10, AVAILABLE) == button_state::ENABLED
        && Footer::enable_for(cost_1, AVAILABLE) == button_state::ENABLED;
    // The premise: the same test can say no, so a button that always lit would not pass.
    let dark = Footer::enable_for(cost_10, cost_10 - 1) != button_state::ENABLED;

    c.assert_behaviour(
        "advancement.cost.the-plus-ten-button-is-affordable-when-the-unassigned-experience-covers-it",
        move |_| lit && dark,
    );
    c.shutdown();
}
