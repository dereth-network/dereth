use super::*;
// -------------------------------------------------------------------------------------------
// 14. spellbook.click.reveals-and-selects-the-row
// -------------------------------------------------------------------------------------------

/// Clicking a clipped spell row scrolls it into view and selects it, in the same frame.
pub fn a_spellbook_click_reveals_and_selects() {
    use dereth_client::pump::Pump;
    use dereth_primitives::DataId;
    use dereth_ui::widgets::listbox::{scroll_offset_of, set_scroll_offset};
    use dereth_ui_screens::panels::remaining::SPELL_PAGE;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::SpellEntry;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.hud_mut().spells = (1..=40)
        .map(|id| SpellEntry {
            id,
            name: format!("Spell {id}"),
            icon: Some(DataId(0x0600_13A5)),
            school: 4,
            level: 1,
            icon_power: 1,
            display_order: i32::try_from(id).expect("a small id"),
            bitfield: 0,
        })
        .collect();
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("shell");
        let screen = shell.flow.current_mut().expect("gameplay");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        let page = shell
            .ui
            .get_child_recursive(gameplay.root().expect("a root"), SPELL_PAGE)
            .expect("the spell page is in the shipped layout");
        let panel_id = gameplay
            .panels
            .pages
            .iter()
            .find(|p| p.handle == page)
            .expect("its panel")
            .panel_id;
        gameplay
            .panels
            .recv_set_panel_visibility(&mut shell.ui, panel_id, true);
    }
    c.tick(1);

    let (h, ch) = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .spellbook
            .list
            .as_ref()
            .expect("the list");
        assert_eq!(list.slots.iter().filter(|s| s.spell.is_some()).count(), 40);
        (list.handle, list.cell.1)
    };
    let view_box = c.view().expect_app().ui().expect("shell").ui.screen_box(h);
    let sy = if view_box.height() % ch == 0 {
        ch / 2
    } else {
        0
    };
    set_scroll_offset(&mut c.app_mut().ui_mut().expect("shell").ui, h, 0, sy);
    let index = usize::try_from((view_box.height() + sy - 2) / ch).expect("a row index");
    let (slot, ring) = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .spellbook
            .list
            .as_ref()
            .expect("the list");
        let row = &list.slots[index];
        assert!(!row.selected, "no selection is pre-supplied");
        (
            row.handle,
            row.selected_ring.expect("the shipped selection ring"),
        )
    };
    let (x, y) = (view_box.x0 + 10, view_box.y1 - 1);
    {
        let ui = &c.view().expect_app().ui().expect("shell").ui;
        let hit = ui
            .hit_test_screen(x, y)
            .expect("a visible row under the pointer");
        assert!(
            hit == slot || ui.is_ancestor_of(slot, hit),
            "the pointer must hit that row"
        );
        assert!(
            ui.node(slot).expect("the row").region.box_.y1 >= view_box.height(),
            "and the row must start clipped, or there is nothing to reveal"
        );
    }

    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), 100_000),
        pump.mouse_button_message(winit::event::MouseButton::Left, true, 100_010)
            .expect("the left button"),
        pump.mouse_button_message(winit::event::MouseButton::Left, false, 100_020)
            .expect("the left button"),
    ];
    for m in messages {
        pump.dispatch(m);
        c.app_mut()
            .input_manager_mut()
            .expect("real input maps")
            .on_message(m);
    }
    c.tick(1);

    let expected_y = i32::try_from(index).expect("a row index") * ch - view_box.height() + ch;
    let after = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        (
            scroll_offset_of(ui, h),
            ui.node(slot).expect("the row").region.box_.y1,
            ui.node(ring).expect("the ring").region.flags.visible,
            app.hud()
                .panels
                .spellbook
                .list
                .as_ref()
                .expect("the list")
                .slots
                .iter()
                .enumerate()
                .all(|(i, s)| s.selected == (i == index)),
        )
    };
    // No further input: an unchanged pass must not erase the ring or move the viewport.
    c.tick(1);

    c.assert_behaviour("spellbook.click.reveals-and-selects-the-row", move |v| {
        let app = v.expect_app();
        let list = app.hud().panels.spellbook.list.as_ref().expect("the list");
        after.0 == Some((0, expected_y))
            && after.1 == view_box.height() - 1
            && after.2
            && after.3
            && list.slots[index].selected
            && scroll_offset_of(&app.ui().expect("shell").ui, h) == Some((0, expected_y))
    });
    c.shutdown();
}

/// The rows the bar's tab actually draws -- the list's own cells, not the model.
fn bar_rows(c: &HeadlessClient, tab: usize) -> Vec<u32> {
    c.view().hud().panels.spellcasting.lists[tab]
        .as_ref()
        .expect("the shipped bar bound its lists")
        .slots
        .iter()
        .filter_map(|s| s.spell)
        .collect()
}

/// A character who knows five of the shipped table's spells plus [`KNOWN_BUT_UNDRAWABLE`], with
/// all six on the first tab of the bar -- delivered as the description the login sends, so the
/// panels are built the way the client builds them.
fn a_character_with_six_favourites() -> (HeadlessClient, Vec<u32>) {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::qualities::SpellBookPage;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let known: Vec<u32> = c
        .view()
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table")
        .spells
        .keys()
        .copied()
        .take(5)
        .collect();
    assert_eq!(known.len(), 5, "the shipped table has spells");
    assert!(
        !c.view()
            .hud()
            .spell_table
            .as_ref()
            .expect("the shipped spell table")
            .spells
            .contains_key(&KNOWN_BUT_UNDRAWABLE),
        "and no row for the undrawable one"
    );

    let bar: Vec<u32> = known
        .iter()
        .copied()
        .chain([KNOWN_BUT_UNDRAWABLE])
        .collect();
    let page = SpellBookPage {
        casting_likelihood: 1.0,
        legacy: None,
    };
    let mut d = dereth_protocol::login::LoginPlayerDescription::default();
    d.qualities.spell_book = Some(PackedHash {
        table_size: 8,
        entries: bar.iter().map(|id| (*id, page)).collect(),
    });
    d.player_module.spell_bars = {
        let mut bars = vec![Vec::new(); 8];
        bars[0] = bar.clone();
        bars
    };

    // The description is the player weenie's, so a described player needs one.
    c.world_mut()
        .seed_player_desc(A_PLAYER, dereth_client_model::Qualities::default());
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));
    c.world_mut().player_system.spell_tabs[0] = bar;
    c.tick(1);
    (c, known)
}

/// The shard's removal reaches the book, the bar and the copy a relog reads.
pub fn a_removed_spell_leaves_the_book_the_bar_and_the_copy() {
    let (mut c, known) = a_character_with_six_favourites();
    let doomed = known[2];

    let drawn_before = c.view().hud().panels.spellbook.shown.contains(&doomed)
        && bar_rows(&c, 0).contains(&doomed)
        && c.view().hud().stats.spells_removed == 0;
    let outbound_before = c.view().outbound().len();

    // The shard takes it away.
    c.when(Inbound::message(
        &dereth_protocol::qualities::MagicRemoveSpell {
            layered_spell_id: doomed,
        },
    ));
    let book_lost_it = c.view().hud().stats.spells_removed == 1
        && !c.view().hud().spells.iter().any(|s| s.id == doomed)
        // and took out nothing else
        && c.view().hud().spells.len() == known.len() - 1;

    // The frame after it: the panels follow, and the bar tells the shard it has lost the row.
    c.tick(1);
    let panels_followed = !c.view().hud().panels.spellbook.shown.contains(&doomed)
        && !bar_rows(&c, 0).contains(&doomed)
        && c.view().interaction().stats.spell_favorites_changed == 1;

    let pruned: Vec<(u32, i32)> = c.view().outbound()[outbound_before..]
        .iter()
        .filter_map(|r| match r {
            Request::RemoveSpellFavorite(m) => Some((m.spell_id, m.spell_bank)),
            _ => None,
        })
        .collect();

    let packed_lost_it = {
        let w = c.view().world();
        !w.player_system.spell_tabs[0].contains(&doomed)
            && w.player_system
                .client_packed_module()
                .expect("a module to re-pack")
                .spell_bars[0]
                .contains(&doomed)
                == false
    };

    c.assert_behaviour(
        "spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads",
        move |_| {
            drawn_before
            && book_lost_it
            && panels_followed
            // one prune, one tab, one spell
            && pruned == vec![(doomed, 0)]
            && packed_lost_it
        },
    );
    c.shutdown();
}

/// A spell the shipped table cannot draw keeps its place on the bar.
pub fn the_bar_keeps_a_spell_the_table_cannot_draw() {
    let (mut c, _known) = a_character_with_six_favourites();
    let outbound_before = c.view().outbound().len();
    // A second frame, so that a prune that merely took one more pass would still show up.
    c.tick(1);

    let still_a_favourite =
        c.view().world().player_system.spell_tabs[0].contains(&KNOWN_BUT_UNDRAWABLE);
    let not_drawn = !bar_rows(&c, 0).contains(&KNOWN_BUT_UNDRAWABLE);
    let not_pruned = c.view().interaction().stats.spell_favorites_changed == 0;
    let nothing_sent = !c.view().outbound()[outbound_before..].iter().any(
        |r| matches!(r, Request::RemoveSpellFavorite(m) if m.spell_id == KNOWN_BUT_UNDRAWABLE),
    );

    c.assert_behaviour(
        "spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw",
        move |v| {
            still_a_favourite
            && not_drawn
            && not_pruned
            && nothing_sent
            // and the character really does still know it
            && v.hud().view(v.objects()).is_spell_known(KNOWN_BUT_UNDRAWABLE)
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// spellbook.redraw.*
//
// The panel is bound off the real gameplay screen's spell page, as its own startup binds it, and
// driven with a book the scenario writes, because the production host derives every field but the
// id from the shipped table and cannot produce the frame these claims are about. The dat-free half
// of the same guard is tested in `dereth-ui-screens` itself.
// ---------------------------------------------------------------------------------------------

pub fn an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();

    let filled = book::update(&mut c, &mut p, &before);
    let order = book::slot_order(&p);
    let idle = book::update(&mut c, &mut p, &before);

    let mut moved = book::a_book();
    moved.0.remove(1);
    let rebuilt = book::update(&mut c, &mut p, &moved);
    let shorter = book::slot_order(&p);
    let idle_again = book::update(&mut c, &mut p, &moved);

    let want = book::ids(&before);
    let want_shorter = book::ids(&moved);

    c.assert_behaviour(
        "spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does",
        move |_| {
            filled && order == want && !idle && rebuilt && shorter == want_shorter && !idle_again
        },
    );
    c.shutdown();
}

pub fn a_spellbook_badge_follows_a_spells_own_flags() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();
    let subject = before.0[1].id;

    assert!(
        book::update(&mut c, &mut p, &before),
        "the first pass always fills"
    );
    assert!(
        !book::update(&mut c, &mut p, &before),
        "and the gate is live"
    );
    let plain = book::badge(&c, &p, subject);
    let order_before = book::slot_order(&p);

    let mut after = book::a_book();
    after.0[1].bitfield = book::FELLOWSHIP;
    assert_eq!(
        book::ids(&before),
        book::ids(&after),
        "the spell list is identical across this"
    );

    let rebuilt = book::update(&mut c, &mut p, &after);
    let lit = book::badge(&c, &p, subject);
    let order_after = book::slot_order(&p);
    let closed = !book::update(&mut c, &mut p, &after);

    // And back, which is the other direction and also the control.
    let back = book::update(&mut c, &mut p, &before);
    let plain_again = book::badge(&c, &p, subject);
    let closed_again = !book::update(&mut c, &mut p, &before);

    c.assert_behaviour(
        "spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list",
        move |_| {
            plain.is_none()
                && rebuilt
                && lit.is_some()
                && order_after == order_before
                && closed
                && back
                && plain_again.is_none()
                && closed_again
        },
    );
    c.shutdown();
}

pub fn the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();

    assert!(
        book::update(&mut c, &mut p, &before),
        "the first pass always fills"
    );
    assert!(
        !book::update(&mut c, &mut p, &before),
        "and the gate is live"
    );
    let order_before = book::slot_order(&p);

    let mut after = book::a_book();
    after.0[0].display_order = 40;
    assert_eq!(
        before
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        after
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        "only the player's own order moves"
    );

    let rebuilt = book::update(&mut c, &mut p, &after);
    let order_after = book::slot_order(&p);
    let closed = !book::update(&mut c, &mut p, &after);

    let back = book::update(&mut c, &mut p, &before);
    let order_back = book::slot_order(&p);
    let closed_again = !book::update(&mut c, &mut p, &before);

    let want_after = {
        let mut rows = after.0.clone();
        rows.sort_by_key(|s| s.display_order);
        rows.iter().map(|s| s.id).collect::<Vec<u32>>()
    };

    c.assert_behaviour(
        "spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves",
        move |_| {
            rebuilt
                && order_after == want_after
                && order_after != order_before
                && closed
                && back
                && order_back == order_before
                && closed_again
        },
    );
    c.shutdown();
}
