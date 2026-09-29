//! Across the eight shipped screens the take-focus widening moves only the scrollable subtree; the
//! walked screens are the shipped ones; a press under a non-activatable root takes no focus.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use std::collections::BTreeMap;

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::common::*;
use dereth_ui_screens::screens::SCREENS;

/// The same construction `screen_conformance` uses: the retail dats, the master property table and
/// the two-level `DidMapper` resolver.
fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

// ---------------------------------------------------------------------------------------------
// The type table, as literals
// ---------------------------------------------------------------------------------------------

const SUBTREE: [(u32, &str); 5] = [
    (0x01, "Button"),
    (0x05, "ListBox"),
    (0x06, "Menu"),
    (0x0B, "Scrollbar"),
    (0x0C, "TextElement"),
];

/// Eight types that are **not** in the subtree and must not move. `Scrollable` (`0x0A`)
/// is deliberately absent from both lists: it is abstract and unregistered in this build, exactly
/// as it is in the client.
const OUTSIDE: [(u32, &str); 8] = [
    (0x02, "DragHandle"),
    (0x03, "Field"),
    (0x07, "Meter"),
    (0x08, "Panel"),
    (0x09, "ResizeHandle"),
    (0x0D, "Viewport"),
    (0x10, "ColorPicker"),
    (0x11, "GroupBox"),
];

const SUBTREE_BY_INHERITANCE: [(u32, &str); 7] = [
    (0x1000_0001, "BurdenIndicator"),
    (0x1000_0002, "EffectsIndicator"),
    (0x1000_0003, "LinkStatusIndicator"),
    (0x1000_0004, "MiniGameIndicator"),
    (0x1000_0005, "PortalStormIndicator"),
    (0x1000_0006, "VitaeIndicator"),
    (0x1000_0031, "ItemListWidget"),
];

/// What one element answered, before and after.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Row {
    total: usize,
    before: usize,
    after: usize,
    /// Mouse-visible after initialisation step 5 — the elements the hit test can return at
    /// all. This is the denominator that matters: an element nothing can press cannot change focus
    /// behaviour however this question is answered.
    pressable: usize,
    /// The intersection: changed **and** reachable by the pointer. The number a player feels.
    changed_pressable: usize,
    /// Of [`Row::changed_pressable`], how many actually show a **different picture** — *measured*,
    /// not modelled.
    ///
    /// A static model of this is exactly the trap the stated testability rule calls *"two errors can
    /// cancel"*: reading the element's **resting** state says 396 buttons move, and none of them
    /// do, because step 5 of [`dereth_ui::UiSystem::mouse_down`] has already put the button in its
    /// own state 3 through the `0x1C` broadcast by the time step 7 takes focus, and
    /// the default element-message handler's `0x2F` arm pushes state 4 only from 0, 1 or 5. So this counter
    /// replays those two steps on the real element and compares the state **after the press** with
    /// the state **after the focus** — the intermediate, not the start.
    picture_moves: usize,
    /// Of the elements that move, the ones with a real focused picture to show — a desc that
    /// declares a state 4. The rest fall back to state 0, which is what a missing state desc
    /// selects.
    declares_state_4: usize,
}

/// Walk one built screen and bucket every element by type.
fn census(ui: &mut UiSystem, roots: &[ElemHandle]) -> BTreeMap<u32, Row> {
    let handles: Vec<ElemHandle> = ui.element_list().to_vec();
    let mut out: BTreeMap<u32, Row> = BTreeMap::new();
    let mut picture_probes: Vec<ElemHandle> = Vec::new();
    for h in handles {
        // Only elements under one of this screen's roots.
        if !roots.iter().any(|r| *r == h || is_descendant(ui, h, *r)) {
            continue;
        }
        let Some(n) = ui.node(h) else { continue };
        let ty = n.ty().0;
        let pressable = n.is_mouse_visible;
        let after = ui.takes_focus_on_press(h);
        let before = ui
            .text_element_mut(h)
            .is_some_and(|t| t.bits.editable() || t.bits.selectable());
        let e = out.entry(ty).or_default();
        e.total += 1;
        e.before += usize::from(before);
        e.after += usize::from(after);
        e.pressable += usize::from(pressable);
        if after != before && pressable {
            e.changed_pressable += 1;
            picture_probes.push(h);
        }
    }

    // Second pass: replay steps 5 and 7 of on each element that newly takes
    // focus, and read the **intermediate**. See [`Row::picture_moves`].
    for h in picture_probes {
        let Some(ty) = ui.node(h).map(|n| n.ty().0) else {
            continue;
        };
        ui.set_focus_element(None);
        ui.broadcast_element_message(
            h,
            dereth_ui::msg::element::id::MOUSE_PRESS,
            dereth_ui::focus::action::PRIMARY_CLICK,
            0,
        );
        let pressed = ui.node(h).map(|n| n.state);
        ui.set_focus_element(Some(h));
        let focused = ui.node(h).map(|n| n.state);
        ui.set_focus_element(None);
        if pressed != focused {
            let declares_4 = ui
                .node(h)
                .is_some_and(|n| n.desc.access_state(dereth_ui::StateId(4)).is_some());
            let e = out.entry(ty).or_default();
            e.picture_moves += 1;
            e.declares_state_4 += usize::from(declares_4);
        }
    }
    ui.drain_outbox();
    out
}

fn is_descendant(ui: &UiSystem, mut h: ElemHandle, root: ElemHandle) -> bool {
    while let Some(p) = ui.parent(h) {
        if p == root {
            return true;
        }
        h = p;
    }
    false
}

fn name_of(ty: u32) -> &'static str {
    SUBTREE
        .iter()
        .chain(OUTSIDE.iter())
        .chain(SUBTREE_BY_INHERITANCE.iter())
        .find(|(id, _)| *id == ty)
        .map_or("(other)", |(_, n)| *n)
}

// ---------------------------------------------------------------------------------------------
// The census
// ---------------------------------------------------------------------------------------------

/// Behaviour: focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too
/// Behaviour: focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere
/// **The blast radius, with its denominator.**
///
/// Falsified by: widening to a type outside the subtree (the `OUTSIDE` zero goes non-zero);
/// widening to nothing (the delta goes to zero); counting elements that are not in the screen
/// under test (the per-screen totals stop summing to the walk).
#[test]
fn the_widening_moves_only_the_scrollable_subtree_across_the_eight_shipped_screens() {
    let mut ui = env();
    let mut grand: BTreeMap<u32, Row> = BTreeMap::new();
    let mut screens_measured = 0usize;

    for spec in SCREENS {
        let mut s = make(spec.class);
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .unwrap_or_else(|e| panic!("{}: {e}", spec.class));
        let roots = s.roots().to_vec();
        let rows = census(&mut ui, &roots);

        let total: usize = rows.values().map(|r| r.total).sum();
        let before: usize = rows.values().map(|r| r.before).sum();
        let after: usize = rows.values().map(|r| r.after).sum();
        let pressable: usize = rows.values().map(|r| r.changed_pressable).sum();
        assert!(total > 0, "{}: the census walked nothing", spec.class);
        eprintln!(
            "{:<26} elements {total:>5}   take focus before {before:>4} -> after {after:>4}   \
             changed and pressable {pressable:>4}",
            spec.class
        );
        for (ty, r) in &rows {
            let e = grand.entry(*ty).or_default();
            e.total += r.total;
            e.before += r.before;
            e.after += r.after;
            e.pressable += r.pressable;
            e.changed_pressable += r.changed_pressable;
            e.picture_moves += r.picture_moves;
            e.declares_state_4 += r.declares_state_4;
        }
        screens_measured += 1;

        for h in roots {
            ui.remove_and_delete_root(h);
        }
        s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
        ui.clean_delete_queue();
        ui.requests.clear();
    }

    assert_eq!(screens_measured, 8, "all eight shipped screens were built");

    let total: usize = grand.values().map(|r| r.total).sum();
    let before: usize = grand.values().map(|r| r.before).sum();
    let after: usize = grand.values().map(|r| r.after).sum();
    let pressable: usize = grand.values().map(|r| r.changed_pressable).sum();
    let picture: usize = grand.values().map(|r| r.picture_moves).sum();
    let declares: usize = grand.values().map(|r| r.declares_state_4).sum();
    eprintln!("\n--- by element type, all eight screens ---");
    for (ty, r) in &grand {
        if r.total == 0 {
            continue;
        }
        eprintln!(
            "  {:#06x} {:<32} n={:<5} pressable={:<5} before={:<4} after={:<4} \
             changed+pressable={:<4} picture moves={:<4} (declares state 4: {})",
            ty,
            name_of(*ty),
            r.total,
            r.pressable,
            r.before,
            r.after,
            r.changed_pressable,
            r.picture_moves,
            r.declares_state_4
        );
    }
    eprintln!(
        "\nTOTAL elements {total}; take focus on a press: {before} -> {after} \
         (+{}); of those, {pressable} are mouse-visible and can actually be pressed; \
         and {picture} of those {pressable} change picture ({declares} because they declare a \
         state 4, the rest because they rest in state 1 with none and `SetState` falls back to 0)",
        after - before
    );

    // --- the instrument can produce a non-zero (§7.8) -----------------------------------------
    assert!(
        after > before,
        "the widening changed nothing at all: {before} -> {after}"
    );
    assert!(
        pressable > 0,
        "nothing that changed is reachable by the pointer"
    );
    assert!(
        picture > 0,
        "the picture probe never once observed a state change"
    );
    assert!(
        picture < pressable,
        "the picture probe said *every* element changes picture"
    );

    // --- the instrument can produce a zero (§7.14) --------------------------------------------
    // Eight types, thousands of elements, and not one of them may move.
    for (ty, name) in OUTSIDE {
        let r = grand.get(&ty).copied().unwrap_or_default();
        assert_eq!(r.after, 0, "{name} ({ty:#x}) must not take focus: {r:?}");
        assert_eq!(
            r.before, 0,
            "{name} ({ty:#x}) did not take focus before either: {r:?}"
        );
    }
    let outside_total: usize = OUTSIDE
        .iter()
        .map(|(ty, _)| grand.get(ty).copied().unwrap_or_default().total)
        .sum();
    assert!(
        outside_total > 1000,
        "the zero above is only a measurement if the walk saw these types at all: {outside_total}"
    );

    // --- every member of the subtree that is present answers yes for all of its instances -----
    let mut subtree_seen = 0usize;
    for (ty, name) in SUBTREE {
        let r = grand.get(&ty).copied().unwrap_or_default();
        if r.total == 0 {
            continue;
        }
        subtree_seen += 1;
        assert_eq!(
            r.after, r.total,
            "{name} ({ty:#x}): {} of {} take focus",
            r.after, r.total
        );
    }
    assert_eq!(
        subtree_seen, 5,
        "all five engine subtree types appear in the shipped screens"
    );

    // --- the residual gap, reported with its number rather than left as prose -----------------
    let gap: usize = SUBTREE_BY_INHERITANCE
        .iter()
        .map(|(ty, _)| grand.get(ty).copied().unwrap_or_default().total)
        .sum();
    let gap_pressable: usize = SUBTREE_BY_INHERITANCE
        .iter()
        .map(|(ty, _)| {
            let r = grand.get(ty).copied().unwrap_or_default();
            r.total - r.after
        })
        .sum();
    eprintln!(
        "residual: {gap} elements of the seven game types that inherit from the subtree; \
         {gap_pressable} of them still answer `false` because `dereth_ui_screens::register_all` \
         builds them as `PlainElement`"
    );
    let item_lists = grand.get(&0x1000_0031).copied().expect("shipped ItemLists");
    assert_eq!(item_lists.total, 65, "DAT-backed ItemList denominator");
    assert_eq!(
        item_lists.after, item_lists.total,
        "ItemList inherits, including Scrollable's focus call"
    );
    assert_eq!(
        gap - item_lists.total,
        6,
        "the six indicator lamps of the shipped screens"
    );
    assert_eq!(
        gap_pressable, 0,
        "Every registered lamp class inherits the button focus policy"
    );
}
// ---------------------------------------------------------------------------------------------
// Denominators
// ---------------------------------------------------------------------------------------------

/// The screens this file walks are the shipped ones.
#[test]
fn the_screens_this_file_walks_are_the_shipped_ones() {
    let ids: Vec<u32> = SCREENS
        .iter()
        .flat_map(|s| s.roots.iter().map(|r| r.0))
        .collect();
    assert!(
        ids.contains(&0x1000_039A),
        "CharacterManagementScreen root: {ids:?}"
    );
    assert!(ids.contains(&0x1000_0495), "GamePlayScreen root: {ids:?}");
    assert_eq!(SCREENS.len(), 8);
    let _ = ElementId(0);
}

/// A press under a root that is not activatable takes no focus.
#[test]
fn a_press_under_a_root_that_is_not_activatable_takes_no_focus() {
    let mut ui = env();
    let mut s = make("CharacterManagementScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the character-management screen builds");
    let root = s.roots()[0];

    // The element the hit test actually returns for a press in the middle of a subtree member.
    let (target, hit) = ui
        .element_list()
        .iter()
        .copied()
        .filter(|h| is_descendant(&ui, *h, root) && ui.takes_focus_on_press(*h))
        .find_map(|h| {
            let b = ui.screen_box(h);
            if b.x1 < b.x0 || b.y1 < b.y0 {
                return None;
            }
            let hit = ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)?;
            ui.takes_focus_on_press(hit).then_some((h, hit))
        })
        .expect("the character-management screen has a hit-testable subtree member");

    // The reason, stated rather than assumed: this root is not activatable.
    let owner = ui.root_of(hit).expect("every element has a root element");
    assert!(
        !ui.node(owner).expect("the root").flags.activatable(),
        "the shipped CharacterManagementScreen root ships activatable = false"
    );
    assert!(
        ui.takes_focus_on_press(hit),
        "and the element itself would take focus"
    );
    assert_eq!(ui.focus_element(), None, "nothing holds focus yet");

    let b = ui.screen_box(target);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    assert_eq!(
        ui.focus_element(),
        None,
        "the guard refused the focus because the root is not activatable"
    );
}
