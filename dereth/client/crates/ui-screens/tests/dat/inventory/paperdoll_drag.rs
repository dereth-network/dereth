//! Equipment dragging selects only a worn item under the pointer.
//! Fixture: shared shipped widget tree and equipment view.

use crate::common::widget_fixture::*;

/// The point on the figure hits the drag mask and resolves to the worn breastplate.
#[test]
fn the_point_on_the_figure_hits_the_drag_mask_and_resolves_to_the_worn_breastplate() {
    let view = Dressed::default();
    let (mut ui, mut s, _panels) = screen(&view);
    let (mask, at) = dressed_doll(&mut ui, &mut s, &view);

    let hit = ui
        .hit_test_screen(at.0, at.1)
        .expect("something is under the pointer");
    assert_eq!(
        hit,
        mask,
        "the figure's hit test answers the paper-doll drag mask 0x100001D6; it answered {:#010X} at \
         {at:?}, mask box {:?}",
        ui.node(hit).expect("alive").element_id().0,
        ui.screen_box(mask)
    );
    assert_eq!(
        s.inventory.paper_doll_region_under_mouse(&ui, at.0, at.1),
        0x0202,
        " reads the chest colour at this point"
    );
}

/// Behaviour: inventory.drag.a-drag-from-the-paper-doll-lifts-the-worn-item
/// *"Items cannot be dragged from the paper doll."* A press on the figure followed by a move past
/// the drag start's four-pixel threshold picks the worn item up: a drag proxy exists and
/// the drop-icon info reads the breastplate's id back off it.
#[test]
fn a_real_drag_from_the_paper_doll_picks_the_worn_item_up() {
    let view = Dressed::default();
    let (mut ui, mut s, mut panels) = screen(&view);
    let (_mask, at) = dressed_doll(&mut ui, &mut s, &view);
    ui.requests.clear();

    ui.mouse_move(LocalTime(0.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    for i in 1..=6 {
        ui.mouse_move(LocalTime(f64::from(i)), at.0 + i * 4, at.1 + i * 4);
        pump(&mut ui, &mut s, &mut panels, &view);
    }

    let proxy = ui
        .drag_state()
        .element
        .expect("starting the drag at (0x10, 0x10) made no drag proxy at all");
    let info = dereth_ui_screens::items::widget::inq_drop_icon_info(&ui, proxy);
    assert_eq!(
        info.item,
        Some(BREASTPLATE),
        "the drag proxy carries no item id, so nothing was picked up off the figure -- and it must \
         be the breastplate rather than the shirt under it, which is the higher-priority test's \
         0x7F promotion inside the armour band"
    );
    assert!(
        info.is_inventory_move(),
        "the doll has no list flavour to report, so writes two \
         properties and not the item list's five: (flags & 0xE) == 0"
    );
    // `EquipmentPanel`'s `0x21` arm has **no** waiting-state set — unlike
    // the item list's begin-drag, which ghosts the slot at pick-up. Transcribed, so this is
    // a divergence guard rather than an observation.
    assert!(
        ui.requests
            .take()
            .iter()
            .all(|r| !matches!(r, UiRequest::SetItemWaiting(_))),
        "the figure's drag arm does not ghost; only an item list's does"
    );
}

/// The negative control, and the half that keeps the arm honest: a press-and-move over a part of
/// the figure that **nothing is worn on** starts no drag, because
/// the lookup for the highest-priority item worn over that region finds nothing, falls back to and
/// answers the player himself, and `prepare_drag_icon` has no item to paint.
#[test]
fn a_drag_from_a_bare_region_of_the_figure_picks_nothing_up() {
    let view = Dressed::default();
    let (mut ui, mut s, mut panels) = screen(&view);
    open_the_backpack(&mut ui, &mut s);
    s.inventory.update(&mut ui, &view);
    let mask = find(&ui, &s, inventory::PAPER_DOLL_DRAG_MASK);
    let origin = ui.screen_origin(mask);
    let map = s
        .inventory
        .click_map
        .as_ref()
        .expect("the click map")
        .clone();
    let mut at = None;
    // The head colour `00 00 FF` -- nothing is worn there in this fixture.
    for y in 0..213 {
        for x in 0..99 {
            if map.mask_at(x, y) == 0x0001 {
                at = Some((origin.0 + x, origin.1 + y));
                break;
            }
        }
        if at.is_some() {
            break;
        }
    }
    let at = at.expect("the shipped click map paints a head region");
    ui.requests.clear();

    ui.mouse_move(LocalTime(0.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    for i in 1..=6 {
        ui.mouse_move(LocalTime(f64::from(i)), at.0 + i * 4, at.1 + i * 4);
        pump(&mut ui, &mut s, &mut panels, &view);
    }
    assert!(
        ui.drag_state().element.is_none(),
        "a bare head is not an item and must not become a drag proxy"
    );
    assert!(
        ui.requests
            .take()
            .iter()
            .all(|r| !matches!(r, UiRequest::SetItemWaiting(_))),
        "nothing was picked up, so nothing may be ghosted"
    );
}
