use super::*;
// ---------------------------------------------------------------------------------------------
// spellbar.keys.*, spellbar.tabs.* and spellbar.click.*
//
// The casting bar's keys, tabs and clicks. Combat's own claims are in `combat.rs`.
//
// Nothing here writes a key down: every control is discovered from the shipped keymap, and the
// bar is the one the shipped gameplay tree carries.
// ---------------------------------------------------------------------------------------------

pub fn every_shipped_magic_key_raises_its_own_instruction() {
    use dereth_ui_screens::view::MagicNotice;

    let mut shell = bar::shell();
    shell.set_combat_input_maps(dereth_input::combat::mode::MAGIC);
    let mut d = bar::Driver::new();

    // Stage one from the shipped data: every default the casting section binds.
    let bound = bar::shipped_bindings(&shell, dereth_input::combat::MAGIC_COMBAT_MAP);
    assert!(
        bound.len() > 10,
        "the shipped casting section binds a real set of keys"
    );

    let mut every_key_holds = true;
    let mut seen = 0usize;
    for (action, qc) in &bound {
        let (down, up) = d.press_release(&mut shell, qc);
        every_key_holds &= down.iter().map(|e| e.id).eq([*action]);
        let mut bench = bar::Bench::new();
        let _ = bench.take_notices();
        bench.drive(down, 100.0);
        let raised = bench.take_notices();
        every_key_holds &= raised == vec![bar::the_instruction_for(action.0)];

        // Letting the key go raises nothing: the whole arm is behind "this is a press". Every
        // shipped default is a one-shot and produces no release of its own, so the arm is asked
        // the question directly with the event the input layer would have handed it.
        every_key_holds &= up.is_empty();
        bench.drive(vec![bar::released(*action)], 101.0);
        every_key_holds &= bench.take_notices().is_empty() && bench.magic_actions() == 1;
        seen += 1;
    }

    // The ones that name a numbered slot each name their own, counting from the first.
    let slots_are_their_own = [0_usize, 4, 8].iter().all(|n| {
        bar::the_instruction_for(
            dereth_client_contract::actions::mapped::USE_SPELL_SLOT_FIRST.0
                + u32::try_from(*n).expect("a small slot"),
        ) == MagicNotice::CastQuickslotSpell { slot: *n }
    });

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.every-shipped-magic-key-raises-its-own-instruction-and-the-release-raises-none",
        move |_| every_key_holds && seen == bound.len() && slots_are_their_own,
    );
}

pub fn the_instructions_move_the_selection_and_the_cast_keys_cast() {
    use dereth_ui_screens::view::{MagicNotice, UiRequest};

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let bound = p.spellcasting.bound() && p.spellcasting.lists_bound() == 8;

    let view = bar::StubView::with_spells(vec![101, 202, 303]);
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);
    let mut tell =
        |n: MagicNotice,
         ui: &mut dereth_ui::UiSystem,
         p: &mut dereth_ui_screens::panels::remaining::RemainingPanels| {
            let moved = p.spellcasting.recv_magic_notice(ui, n, &view);
            (moved, p.spellcasting.sub_menus[tab].selected_spell)
        };

    let nothing_at_first = p.spellcasting.sub_menus[tab].selected_spell == 0;
    let first = tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 101);
    // A second step, because a handler that selects the first every time is otherwise green.
    let steps = tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 202)
        && tell(MagicNotice::PrevSpellSelection, &mut ui, &mut p) == (true, 101);
    let ends = tell(MagicNotice::LastSpellSelection, &mut ui, &mut p) == (true, 303)
        && tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 101)
        && tell(MagicNotice::FirstSpellSelection, &mut ui, &mut p) == (true, 101)
        && tell(MagicNotice::PrevSpellSelection, &mut ui, &mut p) == (true, 303);

    // The cast instruction casts what is selected; the one naming a slot moves the selection
    // first and then casts it.
    ui.requests.clear();
    let casts = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::CastCurrentSpell, &view)
        && ui.requests.take() == vec![UiRequest::CastSpell { spell_id: 303 }];
    let by_slot = p.spellcasting.recv_magic_notice(
        &mut ui,
        MagicNotice::CastQuickslotSpell { slot: 1 },
        &view,
    ) && ui.requests.take() == vec![UiRequest::CastSpell { spell_id: 202 }]
        && p.spellcasting.sub_menus[tab].selected_spell == 202;

    // A slot past the end does nothing at all -- not even a refusal.
    let past_the_end = !p.spellcasting.recv_magic_notice(
        &mut ui,
        MagicNotice::CastQuickslotSpell { slot: 9 },
        &view,
    ) && ui.requests.take().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.the-instructions-move-the-selection-and-the-cast-keys-cast",
        move |_| {
            bound && nothing_at_first && first && steps && ends && casts && by_slot && past_the_end
        },
    );
}

pub fn the_tab_instructions_walk_the_eight_banks() {
    use dereth_ui_screens::panels::spellcasting::{
        SpellcastingPanel as S, SUB_MENU_PAGES, SUB_MENU_TAB_BUTTONS,
    };
    use dereth_ui_screens::view::MagicNotice;

    // The ladder as a pure function, in both directions and past both ends.
    let first = SUB_MENU_TAB_BUTTONS[0];
    let last = SUB_MENU_TAB_BUTTONS[SUB_MENU_TAB_BUTTONS.len() - 1];
    let ladder = S::step_tab_id(first, true) != first
        && S::step_tab_id(last, true) == first
        && S::step_tab_id(first, false) == last
        // A bank the client does not know falls back to the first, either way.
        && S::step_tab_id(0, true) == first
        && S::step_tab_id(0, false) == first;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::default();

    let start = p.spellcasting.open_sub_menu_index(&ui);
    let forward = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::NextSpellTab, &view);
    bar::pump(&mut ui);
    let moved = p.spellcasting.open_sub_menu_index(&ui) != start;
    let back = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::PrevSpellTab, &view);
    bar::pump(&mut ui);
    let returned = p.spellcasting.open_sub_menu_index(&ui) == start;

    let to_the_end = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::LastSpellTab, &view);
    bar::pump(&mut ui);
    let at_the_end = p.spellcasting.open_sub_menu_index(&ui) == SUB_MENU_PAGES.len() - 1;
    let to_the_start = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::FirstSpellTab, &view);
    bar::pump(&mut ui);
    let at_the_start = p.spellcasting.open_sub_menu_index(&ui) == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.tabs.the-tab-instructions-walk-the-eight-banks-and-stop-at-the-ends",
        move |_| {
            ladder
                && forward
                && moved
                && back
                && returned
                && to_the_end
                && at_the_end
                && to_the_start
                && at_the_start
        },
    );
}

pub fn the_frames_own_pass_delivers_what_was_queued_in_order() {
    use dereth_ui_screens::view::MagicNotice;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells(vec![101, 202, 303]);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // Nothing queued: nothing moves.
    ui.notice_inbox.clear();
    p.update(&mut ui, &view);
    let idle = p.spellcasting.sub_menus[tab].selected_spell == 0;

    // One, queued the way the client's own magic arm queues it.
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    let queued = ui.notice_inbox.len() == 1;
    p.update(&mut ui, &view);
    let drained = ui.notice_inbox.is_empty() && p.spellcasting.sub_menus[tab].selected_spell == 101;

    // Two in one frame, in order -- which is what a player mashing the key produces.
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    p.update(&mut ui, &view);
    let both = p.spellcasting.sub_menus[tab].selected_spell == 303;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.the-frames-own-pass-delivers-what-was-queued-in-order",
        move |_| idle && queued && drained && both,
    );
}

pub fn a_real_key_press_moves_the_selection_the_ring_and_the_scroll() {
    use dereth_client_contract::actions::mapped as ia;
    use dereth_ui_screens::view::UiRequest;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells((101..=124).collect());
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // **Bringing the panels up is not a gesture**, and what it leaves behind is listed rather
    // than filtered, so that a bind which starts asking for something else fails here instead of
    // being absorbed by a blanket allowance.
    let baseline = ui.requests.take();
    let bind_asks_for = baseline
        == vec![
            // The allegiance panel's question puts the busy cursor up before it is asked.
            UiRequest::Busy { raised: true },
            UiRequest::AllegianceUpdateRequest { on: true },
        ];

    let mut shell = bar::shell();
    shell.set_combat_input_maps(dereth_input::combat::mode::MAGIC);
    let qc = bar::the_shipped_control(
        &shell,
        dereth_input::combat::MAGIC_COMBAT_MAP,
        ia::COMBAT_NEXT_SPELL.0,
    );
    let mut d = bar::Driver::new();
    let mut bench = bar::Bench::new();

    let mut every_press_holds = true;
    for (i, spell) in view.spells.iter().enumerate() {
        let (down, up) = d.press_release(&mut shell, &qc);
        every_press_holds &= up.is_empty();
        #[allow(clippy::cast_precision_loss)]
        bench.drive(down, 100.0 + i as f64 * 6.0);
        // The host's hand-over: what the interaction layer raised goes to the UI's inbox.
        for n in bench.take_notices() {
            ui.notice_inbox.emit(n);
        }
        // The frame's own consumer, not a direct call into the panel.
        p.update(&mut ui, &view);
        every_press_holds &=
            p.spellcasting.sub_menus[tab].selected_spell == *spell && ui.requests.is_empty();

        let w = p.spellcasting.lists[tab]
            .as_ref()
            .expect("the open bank's list");
        // Just far enough to reveal the row that was off the end; a row already on screen is not
        // scrolled to.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let want = ((i as i32 + 1) * w.cell.0 - ui.screen_box(w.handle).width()).max(0);
        every_press_holds &= w.scroll(&ui) == (want, 0);
        every_press_holds &= w.slots.iter().enumerate().all(|(j, slot)| {
            ui.node(slot.selected_ring.expect("the shipped ring"))
                .expect("alive")
                .region
                .flags
                .visible
                == (i == j)
        });
    }

    // The bar really did overflow, or the scroll above would be a claim about nothing.
    let w = p.spellcasting.lists[tab]
        .as_ref()
        .expect("the open bank's list");
    let viewport = ui.screen_box(w.handle);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let overflowed = view.spells.len() as i32 * w.cell.0 > viewport.width();
    let tail_is_exactly_visible = ui
        .node(w.slots[view.spells.len() - 1].handle)
        .expect("alive")
        .region
        .box_
        .x1
        == viewport.width() - 1;
    let nothing_was_cast = ui.requests.take().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.a-real-key-press-moves-the-selection-the-ring-and-the-scroll",
        move |_| {
            bind_asks_for
                && every_press_holds
                && overflowed
                && tail_is_exactly_visible
                && nothing_was_cast
        },
    );
}

pub fn one_click_selects_and_only_the_double_click_casts() {
    use dereth_ui_screens::panels::spellcasting::item_action;
    use dereth_ui_screens::view::UiRequest;

    // The two identities the two arms are told apart by; without them the two gestures are one.
    let told_apart = item_action::SELECT == dereth_ui::focus::action::PRIMARY_CLICK
        && item_action::DOUBLE_CLICK == dereth_ui::focus::DOUBLE_CLICK_ACTIONS[0];

    let (mut ui, mut screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells((101..=106).collect());
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // The second row, not the first, so that "it selected *this* row" cannot be a default.
    let (slot, want) = {
        let w = p.spellcasting.lists[tab]
            .as_ref()
            .expect("the open bank's list");
        (
            w.slots[1].handle,
            w.spell_at(1).expect("row one carries a spell"),
        )
    };
    let not_a_leftover = want == 102;

    // **A pointer cannot reach anything in a window that is shut.** The bar and its open bank
    // start hidden on a freshly built tree, exactly as they do at login, and nothing in this
    // build opens them from the toolbar yet -- which is a separate finding and not this
    // scenario's subject. They are opened here with the same call the toolbar would make;
    // everything after it is the real chain.
    let opened = bar::open_the_window_over(&mut ui, slot) == 2 && ui.is_visible(slot);
    let at = {
        let b = ui.screen_box(slot);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };

    // Bringing the panels up and opening the window are not gestures; the boundary is here.
    bar::pump_to(&mut ui, &mut screen);
    let _ = screen.take_panel_messages();
    ui.requests.clear();

    let msgs = bar::press_through(&mut ui, &mut screen, item_action::SELECT, at);
    let reached = msgs
        .iter()
        .any(|m| m.id == dereth_ui::msg::element::id::MOUSE_PRESS && m.p1 == item_action::SELECT);
    let mut consumed = false;
    for m in &msgs {
        consumed |= p.on_element_message(&mut ui, m, &view);
    }
    let selected = consumed && p.spellcasting.sub_menus[tab].selected_spell == want;
    let cast_nothing = ui.requests.take().is_empty();

    ui.mouse_up(item_action::SELECT, at.0, at.1, false);
    let _ = ui.drain_outbox();
    let _ = screen.take_panel_messages();
    ui.requests.clear();

    // The same element, the same pixel, the same chain: only the gesture differs.
    let msgs = bar::press_through(&mut ui, &mut screen, item_action::DOUBLE_CLICK, at);
    let reached_twice = msgs.iter().any(|m| {
        m.id == dereth_ui::msg::element::id::MOUSE_PRESS && m.p1 == item_action::DOUBLE_CLICK
    });
    for m in &msgs {
        p.on_element_message(&mut ui, m, &view);
    }
    let casts = ui.requests.take() == vec![UiRequest::CastSpell { spell_id: want }];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.click.one-click-selects-and-only-the-double-click-casts",
        move |_| {
            told_apart
                && not_a_leftover
                && opened
                && reached
                && selected
                && cast_nothing
                && reached_twice
                && casts
        },
    );
}
