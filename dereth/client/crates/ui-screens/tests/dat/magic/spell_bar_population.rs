//! A frame populates all eight shipped spell-bar list boxes; pointer selects clipped tail rows and
//! rejects foreign/empty rows; queued selection reveals rows and refill keeps identity and tab
//! selection; quickslots index filtered rows and padding never casts.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::view::{GameView, MagicNotice, SpellEntry, UiRequest};

#[derive(Debug)]
struct View {
    spells: Vec<SpellEntry>,
    tabs: [Vec<u32>; 8],
    endowment: Option<(dereth_primitives::ObjectId, u32)>,
}
impl GameView for View {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.spells
    }
    fn spell_tab(&self, tab: usize) -> &[u32] {
        &self.tabs[tab]
    }
    fn endowment(&self) -> Option<(dereth_primitives::ObjectId, u32)> {
        self.endowment
    }
}

fn env() -> (UiSystem, RemainingPanels, View, ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("shipped gameplay screen and production visibility setup");
    let root = screen.root().unwrap();
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    let view = View {
        spells: (1..=40)
            .map(|id| SpellEntry {
                id,
                name: format!("Spell {id}"),
                icon: Some(DataId(0x0600_13A5)),
                school: 4,
                level: 1,
                display_order: 41 - i32::try_from(id).expect("a small id"),
                bitfield: 0,
            })
            .collect(),
        tabs: std::array::from_fn(|tab| (1..=24).map(|i| (i + tab as u32) % 40 + 1).collect()),
        endowment: None,
    };
    panels.update(&mut ui, &view);
    ui.requests.clear();
    ui.notice_inbox.clear();
    (ui, panels, view, root)
}

/// Behaviour: spellbar.tabs.a-frame-fills-all-eight-bar-lists
#[test]
fn frame_populates_all_eight_actual_listboxes() {
    let (mut ui, mut panels, view, _) = env();
    panels.update(&mut ui, &view);
    for (tab, sub) in panels.spellcasting.sub_menus.iter().enumerate() {
        let h = sub.list.expect("all eight shipped submenu lists bind");
        let list = ui
            .node(h)
            .unwrap()
            .behaviour
            .as_ref()
            .unwrap()
            .as_any()
            .unwrap()
            .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            .unwrap();
        assert!(
            list.items.len() >= view.tabs[tab].len(),
            "tab {tab}: real UI rows, not just IDs"
        );
        assert!(list.items.iter().all(|row| ui.parent(*row) == Some(h)));
        let w = panels.spellcasting.lists[tab].as_ref().unwrap();
        assert_eq!(
            w.slots.iter().filter_map(|s| s.spell).collect::<Vec<_>>(),
            view.tabs[tab],
            "favorite order, not SpellTable display_order or spellbook sort"
        );
        for (i, slot) in w.slots.iter().enumerate() {
            assert_eq!(
                list.items[i], slot.handle,
                "production ListBox owns the same active row"
            );
            if let Some(id) = slot.spell {
                assert!(
                    matches!(
                        slot.icon_recipe(&ui),
                        Some(dereth_ui::region::IconRecipe::Spell {
                            icon: Some(DataId(0x0600_13A5)),
                            ..
                        })
                    ),
                    "spell {id} paints the real icon recipe"
                );
            }
            let numeral = slot.shortcut_num_elem.expect("retail numeral element");
            assert_eq!(ui.node(numeral).unwrap().region.flags.visible, i < 9);
            if i < 9 {
                assert!(ui.node(numeral).unwrap().region.image.is_some());
            }
        }
    }
}

const LISTENER: u32 = 0x7431;

fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        let Some(parent) = ui.parent(h) else { break };
        h = parent;
    }
}

fn deliver(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View) {
    for d in ui.drain_outbox() {
        if let dereth_ui::Delivery::Element {
            to: dereth_ui::ListenerId::External(LISTENER),
            msg,
        } = d
        {
            panels.on_element_message(ui, &msg, view);
        }
    }
}

/// Behaviour: spellbar.click.a-press-selects-a-clipped-row-and-refuses-foreign-or-empty-rows
#[test]
fn actual_pointer_selects_clipped_tail_and_rejects_foreign_or_empty_rows() {
    let (mut ui, mut panels, view, root) = env();
    panels.update(&mut ui, &view);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    let (h, cw) = (w.handle, w.cell.0);
    assert_eq!(w.max_columns, -1, "shipped spellbar is one horizontal row");
    reveal(&mut ui, h);
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    let viewport = ui.screen_box(h);
    let sx = cw / 2;
    dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, h, sx, 0);
    let index = usize::try_from((viewport.width() + sx - 2) / cw).unwrap();
    let row = panels.spellcasting.lists[tab].as_ref().unwrap().slots[index].handle;
    assert!(ui.node(row).unwrap().region.box_.x1 >= viewport.width());
    let point = (viewport.x1 - 1, viewport.y0 + 10);
    let hit = ui
        .hit_test_screen(point.0, point.1)
        .expect("real spellbar hit");
    assert!(hit == row || ui.is_ancestor_of(row, hit),
        "actual UIItem subtree: hit {hit:?} id {:?} box {:?}; expected {row:?} box {:?}; list {viewport:?}",
        ui.node(hit).unwrap().desc.element_id, ui.screen_box(hit), ui.screen_box(row));
    ui.mouse_down(7, point.0, point.1);
    deliver(&mut ui, &mut panels, &view);
    let expected = index as i32 * cw - viewport.width() + cw;
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    assert_eq!(
        w.scroll(&ui),
        (expected, 0),
        "scroll_to_view reveals only the clipped tail"
    );
    assert_eq!(ui.node(row).unwrap().region.box_.x1, viewport.width() - 1);
    assert_eq!(
        panels.spellcasting.sub_menus[tab].selected_spell,
        view.tabs[tab][index]
    );
    assert_rings(&ui, &panels, tab, view.tabs[tab][index]);
    ui.mouse_up(7, point.0, point.1, false);
    deliver(&mut ui, &mut panels, &view);

    // A valid coordinate and a misleading p2 must not let another consumer select this bar.
    let other = ui
        .get_child_recursive(root, ElementId(0x1000_0298))
        .unwrap();
    let point = (viewport.x0 + cw, viewport.y0 + 10);
    ui.broadcast_element_message_at(
        other,
        dereth_ui::msg::element::id::MOUSE_PRESS,
        7,
        0,
        dereth_ui::msg::MessagePoint {
            window: point,
            element: (0, 0),
        },
    );
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(
        panels.spellcasting.sub_menus[tab].selected_spell,
        view.tabs[tab][index]
    );

    // Correct ancestry, deliberately wrong p2: retail reads the pointer, never that integer.
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    let selected_index = w.item_index_at_point(&ui, cw, 10).unwrap();
    ui.broadcast_element_message_at(
        row,
        dereth_ui::msg::element::id::MOUSE_PRESS,
        7,
        999,
        dereth_ui::msg::MessagePoint {
            window: point,
            element: (0, 0),
        },
    );
    deliver(&mut ui, &mut panels, &view);
    assert_rings(&ui, &panels, tab, view.tabs[tab][selected_index]);

    // Unlike the spellbook, spellbar secondary-press (8) does not select.
    let before = panels.spellcasting.sub_menus[tab].selected_spell;
    ui.mouse_down(8, viewport.x0 + 10, viewport.y0 + 10);
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(panels.spellcasting.sub_menus[tab].selected_spell, before);
    assert!(ui.requests.take().is_empty());
    ui.mouse_up(8, viewport.x0 + 10, viewport.y0 + 10, false);
    deliver(&mut ui, &mut panels, &view);
    // Retail's double-press arm checks the hovered spell is nonzero, then Cast() reads the
    // current selection; the arm itself does not select the hovered row again.
    ui.mouse_down(10, viewport.x0 + 10, viewport.y0 + 10);
    deliver(&mut ui, &mut panels, &view);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::CastSpell { spell_id: before }]
    );
}

fn assert_rings(ui: &UiSystem, panels: &RemainingPanels, tab: usize, selected: u32) {
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    for slot in &w.slots {
        let expected = selected != 0 && slot.spell == Some(selected);
        assert_eq!(slot.selected, expected);
        assert_eq!(
            ui.node(slot.selected_ring.unwrap())
                .unwrap()
                .region
                .flags
                .visible,
            expected
        );
    }
}

#[test]
fn queued_selection_reveals_rows_and_refill_keeps_identity_and_tab_selection() {
    let (mut ui, mut panels, mut view, _) = env();
    panels.update(&mut ui, &view);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    let h = w.handle;
    let handles: Vec<_> = w.slots.iter().map(|s| s.handle).collect();
    ui.notice_inbox.emit(MagicNotice::LastSpellSelection);
    panels.update(&mut ui, &view);
    let selected = *view.tabs[tab].last().unwrap();
    assert_eq!(panels.spellcasting.sub_menus[tab].selected_spell, selected);
    assert_rings(&ui, &panels, tab, selected);
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    assert_eq!(
        w.scroll(&ui).0,
        view.tabs[tab].len() as i32 * w.cell.0 - ui.screen_box(h).width()
    );
    let offset = w.scroll(&ui);
    panels.update(&mut ui, &view);
    assert_eq!(
        panels.spellcasting.lists[tab].as_ref().unwrap().scroll(&ui),
        offset,
        "unchanged frame must not origin-align or refill"
    );
    view.tabs[tab].reverse();
    panels.update(&mut ui, &view);
    let w = panels.spellcasting.lists[tab].as_ref().unwrap();
    assert_eq!(
        w.slots.iter().map(|s| s.handle).collect::<Vec<_>>(),
        handles
    );
    assert_eq!(
        w.slots.iter().filter_map(|s| s.spell).collect::<Vec<_>>(),
        view.tabs[tab]
    );
    assert_eq!(
        w.scroll(&ui),
        (0, 0),
        "refill restores selected ID at its new position"
    );
    assert_rings(&ui, &panels, tab, selected);

    ui.notice_inbox.emit(MagicNotice::LastSpellTab);
    ui.notice_inbox.emit(MagicNotice::LastSpellSelection);
    panels.update(&mut ui, &view);
    assert_eq!(panels.spellcasting.open_sub_menu_index(&ui), 7);
    assert_rings(&ui, &panels, 7, *view.tabs[7].last().unwrap());
    ui.notice_inbox.emit(MagicNotice::FirstSpellTab);
    panels.update(&mut ui, &view);
    assert_eq!(panels.spellcasting.open_sub_menu_index(&ui), tab);
    assert_eq!(panels.spellcasting.sub_menus[tab].selected_spell, selected);
    assert_rings(&ui, &panels, tab, selected);
    // A metadata-only update must reach the existing row without changing its identity.
    view.spells
        .iter_mut()
        .find(|s| s.id == selected)
        .unwrap()
        .icon = Some(DataId(0x0600_13A6));
    panels.update(&mut ui, &view);
    let row = &panels.spellcasting.lists[tab].as_ref().unwrap().slots[0];
    assert_eq!(row.handle, handles[0]);
    assert!(matches!(
        row.icon_recipe(&ui),
        Some(dereth_ui::region::IconRecipe::Spell {
            icon: Some(DataId(0x0600_13A6)),
            ..
        })
    ));
    // `update_from_player_module` restores the stored ID even when no remaining row matches it.
    // SetSelected clears rings, not the stored ID; do not silently choose a replacement spell.
    view.tabs[tab].retain(|id| *id != selected);
    panels.update(&mut ui, &view);
    assert_eq!(panels.spellcasting.sub_menus[tab].selected_spell, selected);
    assert_rings(&ui, &panels, tab, 0);
}

/// Behaviour: spellbar.quickslots.empty-padding-never-casts
#[test]
fn quickslots_index_filtered_ui_rows_and_empty_padding_never_casts() {
    let (mut ui, mut panels, mut view, root) = env();
    view.tabs[0] = vec![999, 8, 3, 8];
    panels.update(&mut ui, &view);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::RemoveSpellFavorite {
            spell_id: 999,
            tab: 0
        }],
        "the unknown favourite is pruned, and told to the shard"
    );
    // ...and only once: this projection re-runs every frame where retail's ran on a notice and
    // unlinked the node in place, so a host that has not drained the queue is not told twice.
    panels.update(&mut ui, &view);
    assert!(
        ui.requests.take().is_empty(),
        "no second 0x01E4 for the same row"
    );
    assert_eq!(
        panels.spellcasting.lists[0]
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .filter_map(|s| s.spell)
            .collect::<Vec<_>>(),
        vec![8, 3, 8]
    );
    ui.notice_inbox
        .emit(MagicNotice::CastQuickslotSpell { slot: 1 });
    panels.update(&mut ui, &view);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::CastSpell { spell_id: 3 }]
    );
    assert_rings(&ui, &panels, 0, 3);
    ui.notice_inbox
        .emit(MagicNotice::CastQuickslotSpell { slot: 2 });
    panels.update(&mut ui, &view);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::CastSpell { spell_id: 8 }]
    );
    assert_rings(&ui, &panels, 0, 8); // SetSelected walks every matching row, not just the first.
    let w = panels.spellcasting.lists[0].as_ref().unwrap();
    assert!(
        w.slots.len() > 3,
        "shipped horizontal list has empty padding"
    );
    assert!(w.slots[3].spell.is_none());
    ui.notice_inbox
        .emit(MagicNotice::CastQuickslotSpell { slot: 3 });
    ui.notice_inbox
        .emit(MagicNotice::CastQuickslotSpell { slot: 999 });
    panels.update(&mut ui, &view);
    assert!(ui.requests.take().is_empty());
    assert_rings(&ui, &panels, 0, 8);
    let w = panels.spellcasting.lists[0].as_ref().unwrap();
    let (h, empty) = (w.handle, w.slots[3].handle);
    reveal(&mut ui, h);
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    let b = ui.screen_box(empty);
    let point = (b.x0 + 10, b.y0 + 10);
    let hit = ui.hit_test_screen(point.0, point.1).unwrap();
    assert!(
        hit == empty || ui.is_ancestor_of(empty, hit),
        "actual empty UIItem hit"
    );
    for action in [7, 10] {
        ui.mouse_down(action, point.0, point.1);
        deliver(&mut ui, &mut panels, &view);
        ui.mouse_up(action, point.0, point.1, false);
        deliver(&mut ui, &mut panels, &view);
    }
    assert!(
        ui.requests.take().is_empty(),
        "empty pointer double-press cannot cast"
    );
    assert_rings(&ui, &panels, 0, 8);
    view.endowment = Some((dereth_primitives::ObjectId(0x5000_0001), 17));
    ui.notice_inbox.emit(MagicNotice::FirstSpellSelection);
    panels.update(&mut ui, &view);
    assert!(panels.spellcasting.sub_menus[0].endowment_selected);
    assert_eq!(panels.spellcasting.sub_menus[0].selected_spell, 0);
    assert_rings(&ui, &panels, 0, 0);
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    panels.update(&mut ui, &view);
    assert!(!panels.spellcasting.sub_menus[0].endowment_selected);
    assert_rings(&ui, &panels, 0, 8);
}
