//! The pointer over each lamp hits that lamp; a click at the pointer toggles the panel each lamp
//! names; a lamp with nothing to show is disabled and swallows its click.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::LocalTime;
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::common::*;
use dereth_ui_screens::env::create_and_add_root_element;

const STRIP: [(u32, u32, &str); 7] = [
    (0x1000_00F8, 0x1000_0003, "link status"),
    (0x1000_00F5, 0x1000_0002, "buffs"),
    (0x1000_00F6, 0x1000_0002, "debuffs"),
    (0x1000_00F4, 0x1000_0006, "vitae"),
    (0x1000_00F7, 0x1000_0001, "burden"),
    (0x1000_00F3, 0x1000_0004, "mini-game"),
    (
        0x1000_00FA,
        0x0000_0001,
        "log out -- the plain button that always worked",
    ),
];

/// The six of [`STRIP`] that are lamps; the seventh row is the control.
const LAMPS: usize = 6;

/// `button input action`, the attribute fires.
const BUTTON_INPUT_ACTION: u32 = 0x12;
/// `element input action`, the attribute reads.
const INPUT_ACTION: u32 = 0x57;

// ---------------------------------------------------------------------------------------------
// Layout-backed screen fixture.
// ---------------------------------------------------------------------------------------------

fn gameplay_tree() -> (UiSystem, ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay root");
    (ui, root)
}

/// The centre of an element's **absolute** box — the pointer position a player would use.
fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    assert!(b.is_valid(), "the element has no box to point at: {b:?}");
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

fn find(ui: &UiSystem, root: ElemHandle, id: u32) -> ElemHandle {
    ui.get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("{id:#010X} is not in the classic_gameplay tree"))
}

// ---------------------------------------------------------------------------------------------
// 1. The hit test
// ---------------------------------------------------------------------------------------------

/// The pointer over every lamp in the strip hits that lamp.
#[test]
fn the_pointer_over_every_lamp_in_the_strip_hits_that_lamp() {
    let (ui, root) = gameplay_tree();
    let mut misses = Vec::new();
    for (i, (id, ty, name)) in STRIP.iter().enumerate() {
        let h = find(&ui, root, *id);
        assert_eq!(
            ui.node(h).expect("alive").ty().0,
            *ty,
            "{name}: wrong element type"
        );
        let (x, y) = centre(&ui, h);
        let hit = ui.hit_test_screen(x, y);
        let got = hit.and_then(|g| ui.node(g)).map(|n| n.element_id().0);
        if got != Some(*id) {
            misses.push(format!(
                "  {name}: pointer ({x},{y}) over {id:#010X} hit {}",
                got.map_or_else(|| "nothing at all".to_string(), |g| format!("{g:#010X}"))
            ));
        }
        // The lamp half of the same fact, one layer down:
        // initialization's step 5 makes an element mouse-visible if it was flagged to be at
        // creation or its class's own should-be-mouse-visible answer is true (every button
        // class answers true).
        // Collected rather than asserted on the spot, so one run prints the whole census instead
        // of stopping at the first lamp.
        if i < LAMPS && !ui.node(h).expect("alive").is_mouse_visible {
            misses.push(format!(
                "  {name}: {id:#010X} is not mouse-visible -- inherited button visibility is \
                 enabled for every button subclass, and all \
                 six lamp classes inherit it"
            ));
        }
    }
    assert!(
        misses.is_empty(),
        "the pointer cannot reach these:\n{}",
        misses.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The click, all the way to the panel
// ---------------------------------------------------------------------------------------------

/// Everything the client would raise `0x31` on when `action` fires — the `0x57` table, read off
/// the shipped tree rather than hard-coded, so a wrong panel id cannot make a test pass.
fn toggle_targets(ui: &UiSystem, all: &[ElemHandle], action: u32) -> Vec<ElemHandle> {
    all.iter()
        .copied()
        .filter(|h| {
            ui.node(*h)
                .is_some_and(|n| n.merged_properties().get_enum(INPUT_ACTION) == Some(action))
        })
        .collect()
}

fn visibility(ui: &UiSystem, hs: &[ElemHandle]) -> Vec<bool> {
    hs.iter()
        .map(|h| ui.node(*h).expect("alive").region.flags.visible)
        .collect()
}

fn action_of(ui: &UiSystem, h: ElemHandle, name: &str) -> u32 {
    ui.node(h)
        .expect("alive")
        .merged_properties()
        .get_enum(BUTTON_INPUT_ACTION)
        .unwrap_or_else(|| panic!("{name}: no attribute 0x12, so nothing to fire"))
}

/// The three events a player produces, and nothing else.
fn click_at(ui: &mut UiSystem, x: i32, y: i32) {
    ui.mouse_move(LocalTime(0.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
}

/// Behaviour: hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down
/// A click at the pointer toggles the panel each lamp names.
#[test]
fn a_click_at_the_pointer_toggles_the_panel_each_lamp_names() {
    let (mut ui, root) = gameplay_tree();
    let mut all = Vec::new();
    walk(&ui, root, &mut all);

    let mut moved = 0usize;
    let mut dead = Vec::new();
    for (id, _, name) in STRIP.iter().take(LAMPS) {
        let lamp = find(&ui, root, *id);
        let action = action_of(&ui, lamp, name);
        let targets = toggle_targets(&ui, &all, action);
        assert!(
            !targets.is_empty(),
            "{name}: action {action:#010X} has no 0x57 listener in the shipped tree"
        );

        light(&mut ui, lamp);
        let before = visibility(&ui, &targets);
        let (x, y) = centre(&ui, lamp);
        click_at(&mut ui, x, y);
        let after = visibility(&ui, &targets);

        for (i, h) in targets.iter().enumerate() {
            let tid = ui.node(*h).expect("alive").element_id().0;
            if before[i] == after[i] {
                dead.push(format!(
                    "  {name}: clicking ({x},{y}) left {tid:#010X} at visible={} -- the click \
                     never reached handle_button_click",
                    before[i]
                ));
            } else {
                moved += 1;
            }
        }
    }
    assert!(
        dead.is_empty(),
        "these lamps are still not wired to their panel:\n{}",
        dead.join("\n")
    );
    assert!(
        moved >= LAMPS,
        "every lamp moved a panel; {moved} moves over {LAMPS} lamps"
    );
    eprintln!("{LAMPS} lamps clicked from a pointer position, {moved} panel toggles");
}

// ---------------------------------------------------------------------------------------------
// 3. The rule that keeps four of them dark, which is retail's and not ours
// ---------------------------------------------------------------------------------------------

/// Behaviour: hud.lamp-row.a-dark-lamp-swallows-its-own-click
/// A lamp with nothing to show is a disabled button and swallows its own click.
#[test]
fn a_lamp_with_nothing_to_show_is_a_disabled_button_and_swallows_its_own_click() {
    let (mut ui, root) = gameplay_tree();
    let mut all = Vec::new();
    walk(&ui, root, &mut all);

    /// Each lamp's initial lit state.
    const AT_REST: [(u32, bool); LAMPS] = [
        (0x1000_00F8, true),  // link status
        (0x1000_00F5, false), // buffs      -- no enchantment registry
        (0x1000_00F6, false), // debuffs    -- no enchantment registry
        (0x1000_00F4, false), // vitae      -- no vitae penalty
        (0x1000_00F7, true),  // burden     -- `burden_state(None)` is 0x0E, never STATE_NOTHING
        (0x1000_00F3, false), // mini-game  -- no invitation
    ];

    for ((id, lit_at_rest), (_, _, name)) in AT_REST.iter().zip(STRIP.iter()) {
        let lamp = find(&ui, root, *id);
        let disabled = ui
            .node(lamp)
            .expect("alive")
            .merged_properties()
            .get_bool(0x0D)
            .unwrap_or(false);
        assert_eq!(
            disabled, !*lit_at_rest,
            "{name}: attribute 0x0D is 's answer to its resting state"
        );

        let action = action_of(&ui, lamp, name);
        let targets = toggle_targets(&ui, &all, action);
        let before = visibility(&ui, &targets);
        let (x, y) = centre(&ui, lamp);
        click_at(&mut ui, x, y);
        let after = visibility(&ui, &targets);

        if *lit_at_rest {
            assert_ne!(
                before, after,
                "{name}: rests lit, so a click at ({x},{y}) must open it"
            );
        } else {
            assert_eq!(
                before, after,
                "{name}: rests at 0x0D, and a disabled button swallows its own click -- \
                 the button's element-message arm stops the message before the click handler"
            );
            // …and lighting it is the whole of what it takes, with no second click needed to
            // undo a state this test changed.
            light(&mut ui, lamp);
            let before = visibility(&ui, &targets);
            click_at(&mut ui, x, y);
            assert_ne!(
                before,
                visibility(&ui, &targets),
                "{name}: once lit, the same pointer position opens the panel"
            );
        }
    }
}
