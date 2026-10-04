use super::*;
pub fn a_secondary_click_on_a_spell_opens_its_description() {
    use dereth_ui_screens::panels::{examination, spell_examine};

    let mut c = examine::a_book_of_two();
    let shut_before = examine::window_is_up(&mut c);
    let spell = examine::shipped_spell(&c, FLAME_BOLT);
    let components = examine::component_names(&c, &spell.comps);

    assert_eq!(
        spell.school, 1,
        "the shipped row is the war bolt this scenario drives"
    );
    assert!(
        spell.base_mana >= 1 && spell.mana_mod == 0,
        "with a plain mana cost"
    );
    assert!(spell.duration.is_none(), "a bolt is not an enchantment");
    assert_eq!(components.len(), 5, "and its formula has five slots");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    // Every expected line is built from the shipped row, not written down.
    let school_line = format!("School: {}", examine::school_name(spell.school));
    let mana_line = format!("Mana: {}", spell.base_mana);
    let range_line = format!("Range: {:.1} yds.", examine::yards(&spell));
    let mut description = spell.description.clone();
    description.push_str("\n\nCOMPONENTS:");
    for n in &components {
        description.push_str("\n     ");
        description.push_str(n);
    }

    let snap = c.ui_snapshot();
    let drawn = (
        snap.is_visible(examination::WINDOW),
        snap.is_visible(spell_examine::BASE),
        snap.text_of(examination::DISPLAYED_NAME_TEXT).to_owned(),
        snap.text_of(spell_examine::MAGIC_SCHOOL_TEXT).to_owned(),
        snap.text_of(spell_examine::MANA_TEXT).to_owned(),
        snap.text_of(spell_examine::RANGE_TEXT).to_owned(),
        snap.text_of(spell_examine::DURATION_TEXT).to_owned(),
        snap.text_of(spell_examine::DISPLAY_TEXT).to_owned(),
    );
    let pane = examine::pane_facts(&mut c);
    // The cursor is the half the client must not get wrong: looking at a spell and looking at
    // nothing are two different things, and only one of them arms the pointer.
    let cursor = c.view().expect_app().interaction().target_mode();
    let sent = c.outbound().len();
    let name = spell.name.clone();

    c.assert_behaviour(
        "spell-examine.pane.a-secondary-click-on-a-known-spell-opens-its-description",
        move |_| {
            !shut_before
                && drawn.0
                && drawn.1
                && drawn.2 == name
                && drawn.3 == school_line
                && drawn.4 == mana_line
                && drawn.5 == range_line
                && drawn.6.is_empty()
                && drawn.7 == description
                && pane.spell == FLAME_BOLT
                && pane.component_names == components
                && pane.rows_drawn == 5
                && pane.component_scids.len() == 5
                && cursor == dereth_client::interaction::TargetMode::None
                && sent == 0
        },
    );
    c.shutdown();
}

pub fn the_primary_click_selects_the_row_and_examines_nothing() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    examine::press(&mut c, x, y, 100_000, examine::PRIMARY);

    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let pulled = examine::pane_facts(&mut c).examines_pulled;
    let selected = c.view().expect_app().hud().panels.spellbook.selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-primary-click-selects-the-row-and-examines-nothing",
        move |_| !up && pulled == 0 && selected == FLAME_BOLT,
    );
    c.shutdown();
}

pub fn the_secondary_click_moves_the_books_own_selection_too() {
    let mut c = examine::a_book_of_two();
    let before = c.view().expect_app().hud().panels.spellbook.selected_spell;
    let name = examine::shipped_spell(&c, STRENGTH_SELF).name;

    let (x, y) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let title = examine::title(&mut c);
    let after = c.view().expect_app().hud().panels.spellbook.selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-secondary-click-moves-the-books-own-selection-too",
        move |_| before != STRENGTH_SELF && after == STRENGTH_SELF && title == name,
    );
    c.shutdown();
}

pub fn an_enchantment_shows_how_long_it_lasts_and_no_range() {
    use dereth_ui_screens::panels::spell_examine;

    let mut c = examine::a_book_of_two();
    let spell = examine::shipped_spell(&c, STRENGTH_SELF);
    let components = examine::component_names(&c, &spell.comps);
    let seconds = spell
        .duration
        .map(|(d, _, _)| d)
        .expect("an enchantment lasts a while");

    assert!(
        seconds >= 60.0,
        "the shipped row is long enough for the minutes arm"
    );
    assert!(
        spell.base_range_constant == 0.0 && spell.base_range_mod == 0.0,
        "and it reaches nowhere, which is what leaves the range line empty"
    );

    let (x, y) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let school_line = format!("School: {}", examine::school_name(spell.school));
    let mana_line = format!("Mana: {}", spell.base_mana);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let duration_line = format!("Duration: {} min.", (seconds / 60.0).round() as u32);

    let snap = c.ui_snapshot();
    let drawn = (
        snap.text_of(spell_examine::MAGIC_SCHOOL_TEXT).to_owned(),
        snap.text_of(spell_examine::MANA_TEXT).to_owned(),
        snap.text_of(spell_examine::DURATION_TEXT).to_owned(),
        snap.text_of(spell_examine::RANGE_TEXT).to_owned(),
    );
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.pane.an-enchantment-shows-how-long-it-lasts-and-no-range",
        move |_| {
            drawn.0 == school_line
                && drawn.1 == mana_line
                && drawn.2 == duration_line
                && drawn.3.is_empty()
                && pane.component_names == components
        },
    );
    c.shutdown();
}

pub fn a_second_look_re_opens_the_window_the_close_control_shut() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);

    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let opened = c.ui_snapshot().is_visible(examination::WINDOW);
    let closes_before = examine::pane_facts(&mut c).closed;

    let (cx, cy) = examine::centre_of(&mut c, examination::CLOSE_BUTTON);
    examine::press(&mut c, cx, cy, 200_000, examine::PRIMARY);
    let shut = !c.ui_snapshot().is_visible(examination::WINDOW);
    let closes_after = examine::pane_facts(&mut c).closed;

    examine::press(&mut c, x, y, 300_000, examine::SECONDARY);
    let re_opened = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);

    c.assert_behaviour(
        "spell-examine.pane.a-second-look-re-opens-the-window-the-close-control-shut",
        move |_| opened && shut && closes_after == closes_before + 1 && re_opened && title == name,
    );
    c.shutdown();
}

pub fn the_same_look_from_the_cast_bar_opens_it_without_selecting() {
    use dereth_ui_screens::panels::{examination, spell_examine};

    let mut c = examine::a_book_of_two();
    // The favourite bank the character's own description carries, and the stance the bar is shown
    // in.
    {
        let w = c.world_mut();
        w.player_system.spell_tabs[0] = vec![FLAME_BOLT];
        w.combat.combat_mode = dereth_client_model::combat::CombatMode::Magic;
    }
    c.tick(3);
    let spell = examine::shipped_spell(&c, FLAME_BOLT);
    let name = spell.name.clone();
    let components = examine::component_names(&c, &spell.comps);
    let shut_before = examine::window_is_up(&mut c);

    let (tab, x, y) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell").ui;
        let tab = app.hud().panels.spellcasting.open_sub_menu_index(ui);
        let list = app.hud().panels.spellcasting.lists[tab]
            .as_ref()
            .expect("the bound sub-menu list");
        assert_eq!(
            list.slots.first().and_then(|s| s.spell),
            Some(FLAME_BOLT),
            "the favourite reached the bar"
        );
        let b = ui.screen_box(list.slots[0].handle);
        assert!(
            b.width() > 0 && b.height() > 0,
            "and the row has an extent to press on"
        );
        (tab, b.x0 + 4, b.y0 + 4)
    };
    let selected_before =
        c.view().expect_app().hud().panels.spellcasting.sub_menus[tab].selected_spell;

    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let snap = c.ui_snapshot();
    let up = (
        snap.is_visible(examination::WINDOW),
        snap.is_visible(spell_examine::BASE),
    );
    let title = examine::title(&mut c);
    let pane = examine::pane_facts(&mut c);
    let selected_after =
        c.view().expect_app().hud().panels.spellcasting.sub_menus[tab].selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-same-look-from-the-cast-bar-opens-it-without-selecting",
        move |_| {
            !shut_before
                && up.0
                && up.1
                && title == name
                && pane.component_names == components
                && selected_after == selected_before
        },
    );
    c.shutdown();
}

pub fn a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does() {
    use dereth_client::interaction::TargetMode;
    use dereth_client_contract::UiRequest;
    use dereth_testkit::Player;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;

    // The toolbar's Examine button with a selection: the only way this client puts an appraisal in
    // flight without a pick in the scene.
    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::Examine(AN_ITEM))).tick(3);
    let asked = examine::appraisals(&c, mark);
    let in_flight = examine::pane_facts(&mut c).awaiting;

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    let mark = c.outbound().len();
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let cancelled = examine::appraisals(&c, mark);
    let cursor_after_cancel = c.view().expect_app().interaction().target_mode();
    let still_examining = c.view().world().appraisal.examining;
    let pane = examine::pane_facts(&mut c);
    let title = examine::title(&mut c);

    // The second look: both ids are already clear, so it has nothing left to forget.
    let (x2, y2) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    let mark = c.outbound().len();
    examine::press(&mut c, x2, y2, 200_000, examine::SECONDARY);
    let again = examine::appraisals(&c, mark);
    let second_pane = examine::pane_facts(&mut c);

    // The other zero, in the same scenario, so the two meanings cannot collapse into one: looking
    // at *nothing* arms the pointer and asks the shard nothing at all.
    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::Examine(dereth_primitives::ObjectId(
        0,
    ))))
    .tick(3);
    let null_look = (
        examine::appraisals(&c, mark),
        c.view().expect_app().interaction().target_mode(),
    );

    c.assert_behaviour(
        "spell-examine.cancel.a-look-cancels-an-appraisal-in-flight-and-only-the-first-of-two-does",
        move |_| {
            asked == vec![AN_ITEM]
                && in_flight == Some(AN_ITEM)
                && cancelled == vec![dereth_primitives::ObjectId(0)]
                && cursor_after_cancel == TargetMode::None
                && still_examining.is_none()
                && pane.awaiting.is_none()
                && pane.current.is_none()
                && pane.cancels == 1
                && pane.examines_pulled == 1
                && title == name
                && again.is_empty()
                && second_pane.examines_pulled == 2
                && second_pane.cancels == 1
                && null_look.0.is_empty()
                && null_look.1 == TargetMode::Examine
        },
    );
    c.shutdown();
}

pub fn a_look_with_nothing_in_flight_asks_the_shard_nothing() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let quiet = examine::pane_facts(&mut c);

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    let mark = c.outbound().len();
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let sent = examine::appraisals(&c, mark);
    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.cancel.a-look-with-nothing-in-flight-asks-the-shard-nothing",
        move |_| {
            quiet.awaiting.is_none()
                && quiet.current.is_none()
                && up
                && title == name
                && sent.is_empty()
                && pane.examines_pulled == 1
                && pane.cancels == 0
        },
    );
    c.shutdown();
}

pub fn the_account_the_shard_names_is_the_one_the_client_keeps() {
    let mut c = examine::a_client();
    let before = c.view().world().player_system.account.clone();

    examine::greet(&mut c, AN_ACCOUNT);
    let first = c.view().world().player_system.account.clone();
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let second = c.view().world().player_system.account.clone();

    c.assert_behaviour(
        "spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps",
        move |_| before.is_empty() && first == AN_ACCOUNT && second == ANOTHER_ACCOUNT,
    );
    c.shutdown();
}

pub fn the_formula_the_client_would_cast_follows_that_account() {
    let mut c = examine::a_client();
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let stored = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);

    assert_eq!(
        spell.school, LIFE_MAGIC,
        "the shipped row is the Life spell this scenario drives"
    );
    assert_eq!(
        stored.iter().filter(|s| **s != 0).count(),
        8,
        "and its formula fills every slot"
    );

    let nameless = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, AN_ACCOUNT);
    let mine = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let theirs = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.the-formula-the-client-would-cast-follows-that-account",
        move |_| mine != nameless && mine != stored && theirs != mine && theirs != stored,
    );
    c.shutdown();
}

pub fn the_look_lists_the_tapers_this_account_must_carry() {
    let mut c = examine::a_book_of_one(REGENERATION_SELF_V, AN_ACCOUNT);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let stored = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);
    let mine = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let from_the_table = examine::component_names(&c, &stored);
    let theirs = {
        let mut f = stored;
        dereth_client_model::magic::randomize_for_name(
            &mut f,
            ANOTHER_ACCOUNT,
            spell.formula_version,
        );
        examine::component_names(&c, &f)
    };

    assert_ne!(
        mine, from_the_table,
        "the premise: this account's tapers are not the table's"
    );
    assert_ne!(mine, theirs, "nor the other account's");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.the-look-lists-the-tapers-this-account-must-carry",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == mine
                && pane.component_names != from_the_table
                && pane.component_names != theirs
                && pane.rows_drawn == 8
        },
    );
    c.shutdown();
}

pub fn two_accounts_are_shown_different_tapers_for_one_spell() {
    let mut c = examine::a_book_of_one(REGENERATION_SELF_V, ANOTHER_ACCOUNT);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let theirs = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let mine = {
        let mut f = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);
        dereth_client_model::magic::randomize_for_name(&mut f, AN_ACCOUNT, spell.formula_version);
        examine::component_names(&c, &f)
    };

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.two-accounts-are-shown-different-tapers-for-one-spell",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == theirs
                && pane.component_names != mine
        },
    );
    c.shutdown();
}

pub fn the_shipped_foci_table_reaches_the_game_model_with_the_description() {
    let mut c = examine::a_client();
    let shipped: Vec<(u32, u32)> = {
        let mut rows = c.view().expect_app().hud().school_pack_wcid.clone();
        rows.sort_unstable();
        rows
    };
    assert!(
        shipped.len() >= 5,
        "the shipped mapper names a foci for each of the spell schools"
    );
    assert!(
        shipped.iter().all(|(_, w)| *w != 0),
        "and every one is a real class"
    );

    let before: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    examine::describe(&mut c);
    let after: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    let want: Vec<u32> = shipped.iter().map(|(_, w)| *w).collect();
    // A school the shipped table has no foci for stays at nothing on either side.
    let unmapped: u32 = (1..64)
        .find(|s| !shipped.iter().any(|(k, _)| k == s))
        .expect("a school with none");
    let none = c.view().world().school_of_magic_to_wcid(unmapped);

    c.assert_behaviour(
        "spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description",
        move |_| before.iter().all(|w| *w == 0) && after == want && none == 0,
    );
    c.shutdown();
}

pub fn carrying_a_foci_changes_what_the_client_would_spend() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    let after = c.view().world().spell_formula(&spell);

    let before_len = before.iter().filter(|s| **s != 0).count();
    let after_len = after.iter().filter(|s| **s != 0).count();

    c.assert_behaviour(
        "spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend",
        move |_| before_len == 8 && after_len < before_len && after != before,
    );
    c.shutdown();
}

pub fn a_foci_for_another_school_leaves_the_long_formula_alone() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let wrong = examine::foci_of(&c, examine::another_school(&c, LIFE_MAGIC));
    assert_ne!(
        wrong,
        examine::foci_of(&c, LIFE_MAGIC),
        "the premise: it is another school's"
    );
    examine::carry_the_foci(&mut c, me, wrong);
    let after = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone",
        move |_| after == before && before.iter().filter(|s| **s != 0).count() == 8,
    );
    c.shutdown();
}

pub fn the_look_with_a_foci_lists_a_scarab_and_four_tapers() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    examine::learn_and_open(&mut c, &[REGENERATION_SELF_V]);

    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let want = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    assert_eq!(want.len(), 5, "a scarab and four tapers");
    assert_eq!(
        want[1..]
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "and the four are all one component"
    );

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.the-look-with-a-foci-lists-a-scarab-and-four-tapers",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == want
                && pane.rows_drawn == 5
        },
    );
    c.shutdown();
}

pub fn the_look_without_a_foci_lists_the_long_per_account_formula() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    examine::learn_and_open(&mut c, &[REGENERATION_SELF_V]);

    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let long = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    assert_eq!(long.len(), 8, "the long formula fills every slot");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    // What the same spell on the same client would cost *with* the foci, so "no taper in this
    // list" is a comparison against the short list and not against a word.
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    let short = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let taper = short[1].clone();
    assert_eq!(short.len(), 5, "the foci really does shorten it");

    c.assert_behaviour(
        "spell-examine.formula.the-look-without-a-foci-lists-the-long-per-account-formula",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == long
                && pane.rows_drawn == 8
                && !pane.component_names.contains(&taper)
        },
    );
    c.shutdown();
}
