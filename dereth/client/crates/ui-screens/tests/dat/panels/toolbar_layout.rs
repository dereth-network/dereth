//! The shipped toolbar is clamped into its parent on local layout restore; move precedes resize;
//! missing saved pairs keep dat placement; hidden ancestor stays hidden; refresh keeps it in the
//! current parent.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::mode;
use dereth_ui::persist::{SavedWindow, ScreenLayout};
use dereth_ui::region::Box2D;
use dereth_ui::{ElemHandle, ElementId, Screen, UiFlow, UiSystem};
use dereth_ui_screens::hud::floaty::WindowPlacement;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen, PlayerSettingsView};

fn env(display: (i32, i32)) -> (UiSystem, UiFlow) {
    let (mut ui, flow, _store) =
        crate::common::layout::load(display, RegistrationOrder::BeforeResolver);
    ui.requests.clear();
    (ui, flow)
}

fn screen(display: (i32, i32)) -> (UiSystem, GamePlayScreen) {
    let (mut ui, _) = env(display);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("shipped gameplay");
    (ui, s)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    ui.get_child_recursive(s.root().expect("gameplay root"), id)
        .expect("DAT child")
}

fn rect(ui: &UiSystem, h: ElemHandle) -> Box2D {
    ui.node(h).expect("live node").region.box_
}

fn saved(b: Box2D, x: i32, y: i32) -> SavedWindow {
    SavedWindow {
        x,
        y,
        w: b.width(),
        h: b.height(),
    }
}

fn layout(w: SavedWindow) -> ScreenLayout {
    ScreenLayout::parse(&format!(
        "<TBAR> X:{} Y: {} W: {} H: {} ",
        w.x, w.y, w.w, w.h
    ))
    .expect("private retail-format row")
}

fn clamped(parent: Box2D, own: Box2D, x: i32, y: i32) -> (i32, i32) {
    (
        x.min(parent.width() - own.width()).max(0),
        y.min(parent.height() - own.height()).max(0),
    )
}

fn assert_drawn(ui: &UiSystem, toolbar: ElemHandle) {
    let backpack = ui
        .get_child_recursive(toolbar, ElementId(0x1000_01B1))
        .expect("backpack button");
    let shortcut = ui
        .get_child_recursive(toolbar, ElementId(0x1000_01A7))
        .expect("shortcut 1");
    let trace = ui.draw_trace();
    for h in [toolbar, backpack, shortcut] {
        assert!(
            trace.iter().any(|e| e.who == h),
            "real DAT toolbar/button/shortcut must reach draw: {h:?}"
        );
    }
}

/// Behaviour: toolbar.layout.a-restored-toolbar-is-clamped-inside-its-parent
#[test]
fn local_restore_clamps_the_real_toolbar_without_showing_or_moving_pans() {
    for display in [(800, 600), (1024, 768)] {
        let (mut ui, mut s) = screen(display);
        let h = find(&ui, &s, window::TOOLBAR);
        let pans = find(&ui, &s, window::PANEL_STACK);
        let parent = rect(&ui, ui.parent(h).expect("TBAR parent"));
        let before = rect(&ui, h);
        let pans_before = rect(&ui, pans);
        assert_eq!(
            ui.node(h).unwrap().ty().0,
            0x1000_0051,
            "actual floating toolbar"
        );
        assert_drawn(&ui, h);
        let source = layout(saved(before, display.0 + 400, display.1 + 200));
        assert_eq!(s.load_screen_layout(&mut ui, &source), 1);
        let after = rect(&ui, h);
        assert_eq!(
            (after.x0, after.y0),
            clamped(parent, before, display.0 + 400, display.1 + 200),
            "MoveTo must constrain the restored toolbar; DAT original={before:?}"
        );
        assert_drawn(&ui, h);
        assert_eq!(
            rect(&ui, pans),
            pans_before,
            "PANS is a different floaty window"
        );
        assert!(
            !ui.node(pans).unwrap().region.flags.visible,
            "layout load does not open the backpack"
        );
        let round_trip = s.save_screen_layout(&ui);
        let row = round_trip
            .windows
            .iter()
            .find(|(tag, _)| *tag == "<TBAR>")
            .unwrap()
            .1;
        assert_eq!(
            (row.x, row.y),
            ui.screen_origin(h),
            "save reads actual clamped placement"
        );
        assert_eq!(
            source.windows[0].1.x,
            display.0 + 400,
            "input layout is not rewritten"
        );
    }
}

#[test]
fn direct_move_uses_actual_parent_and_no_parent_is_not_the_display() {
    let (mut ui, s) = screen((800, 600));
    let h = find(&ui, &s, window::TOOLBAR);
    let pans = find(&ui, &s, window::PANEL_STACK);
    let parent = ui.parent(h).unwrap();
    let size = rect(&ui, h);
    for (x, y) in [(-91, -73), (7777, 8888), (11, 22)] {
        ui.move_to(h, x, y);
        let b = rect(&ui, h);
        assert_eq!((b.x0, b.y0), clamped(rect(&ui, parent), size, x, y));
    }
    // The hook must not silently become a global clamp for every PlainElement/game class.
    ui.move_to(pans, -91, 8888);
    assert_eq!((rect(&ui, pans).x0, rect(&ui, pans).y0), (-91, 8888));
    ui.set_parent(h, None);
    ui.move_to(h, -91, 8888);
    assert_eq!(
        (rect(&ui, h).x0, rect(&ui, h).y0),
        (-91, 8888),
        "retail null-parent guard"
    );
    let tiny = ui.create_hollow(Some(parent));
    ui.resize_to(tiny, size.width() - 10, size.height() - 10);
    ui.move_to(tiny, 47, 53);
    ui.set_parent(h, Some(tiny));
    ui.move_to(h, 999, -3);
    assert_eq!(
        (rect(&ui, h).x0, rect(&ui, h).y0),
        (0, 0),
        "oversized toolbar clamps to zero, not a negative maximum"
    );
    assert_eq!(
        ui.screen_origin(h),
        ui.screen_origin(tiny),
        "local parent, not display coordinates"
    );
}

/// Behaviour: toolbar.layout.visibility-wins-over-a-saved-layout
#[test]
fn player_module_move_precedes_resize_and_visibility_still_wins_over_local_layout() {
    let (mut ui, mut s) = screen((800, 600));
    let h = find(&ui, &s, window::TOOLBAR);
    let before = rect(&ui, h);
    let parent = rect(&ui, ui.parent(h).unwrap());
    let id = s.window_id_of(window::TOOLBAR);
    assert_ne!(id, 0, "window id read from DAT 0x1000007E");
    // The shipped toolbar fixes its width to 310, so its normal size cannot distinguish call
    // order. This one PRIVATE in-memory attribute boundary fixture permits 40 extra pixels;
    // the real toolbar class/tree, restore calls, and all other DAT properties remain in use.
    // It tests the source's ordering, not an observed retail layout or a new resize affordance.
    assert_eq!(
        ui.node(h)
            .unwrap()
            .merged_properties()
            .get_int(dereth_ui::props::attr::MAX_WIDTH),
        Some(before.width())
    );
    ui.set_attribute_int(h, dereth_ui::props::attr::MAX_WIDTH, before.width() + 40);
    let mut pm = PlayerSettingsView::default();
    pm.placements.set(
        id,
        WindowPlacement {
            x: Some(9999),
            y: Some(9999),
            w: Some(before.width() + 40),
            h: Some(before.height()),
            visible: Some(false),
            ..WindowPlacement::default()
        },
    );
    s.update_from_player_module(&mut ui, &pm);
    let after = rect(&ui, h);
    assert_eq!(
        (after.x0, after.y0),
        clamped(parent, before, 9999, 9999),
        "server restore clamps using OLD size"
    );
    assert_eq!(
        after.width(),
        before.width() + 40,
        "private boundary fixture permits a distinct resized width"
    );
    assert!(!ui.node(h).unwrap().region.flags.visible);
    assert!(!ui.draw_trace().iter().any(|e| e.who == h));
    s.load_screen_layout(&mut ui, &layout(saved(before, 9999, 9999)));
    let from_file = rect(&ui, h);
    assert_eq!(from_file.width(), before.width());
    assert_eq!(
        (from_file.x0, from_file.y0),
        clamped(parent, before, 9999, 9999),
        "file restore clamps using NEW size"
    );
    assert!(
        !ui.node(h).unwrap().region.flags.visible,
        "loading geometry never forces visibility"
    );
    pm.placements.set(
        id,
        WindowPlacement {
            x: Some(-99),
            y: Some(-99),
            visible: Some(true),
            ..WindowPlacement::default()
        },
    );
    s.update_from_player_module(&mut ui, &pm);
    assert_eq!(
        rect(&ui, h),
        from_file,
        "local geometry wins over PM geometry"
    );
    assert_drawn(&ui, h);
    pm.placements.set(
        id,
        WindowPlacement {
            visible: Some(false),
            ..WindowPlacement::default()
        },
    );
    s.update_from_player_module(&mut ui, &pm);
    assert_eq!(rect(&ui, h), from_file);
    assert!(
        !ui.draw_trace().iter().any(|e| e.who == h),
        "intentional hidden state still applies"
    );
}

#[test]
fn missing_player_module_pairs_preserve_dat_placement_and_hidden_ancestor_stays_hidden() {
    let (mut ui, mut s) = screen((800, 600));
    let h = find(&ui, &s, window::TOOLBAR);
    let before = rect(&ui, h);
    let id = s.window_id_of(window::TOOLBAR);
    let mut pm = PlayerSettingsView::default();
    pm.placements.set(
        id,
        WindowPlacement {
            x: Some(-900),
            w: Some(9999),
            ..WindowPlacement::default()
        },
    );
    s.update_from_player_module(&mut ui, &pm);
    assert_eq!(
        rect(&ui, h),
        before,
        "missing Y/height means neither pair is applied"
    );
    assert_drawn(&ui, h);
    let root = s.root().unwrap();
    ui.set_visible(root, false);
    s.load_screen_layout(&mut ui, &layout(saved(before, 9999, 9999)));
    pm.placements.set(
        id,
        WindowPlacement {
            visible: Some(true),
            ..WindowPlacement::default()
        },
    );
    s.update_from_player_module(&mut ui, &pm);
    assert!(
        ui.node(h).unwrap().region.flags.visible,
        "own visibility still follows PM"
    );
    assert!(!ui.node(root).unwrap().region.flags.visible);
    assert!(
        !ui.draw_trace().iter().any(|e| e.who == h),
        "restoration never unhides an ancestor"
    );
}

#[test]
fn refresh_and_same_mode_generation_keep_the_toolbar_in_the_current_parent() {
    let (mut ui, mut flow) = env((1024, 768));
    flow.queue(mode::GAME_PLAY);
    flow.use_new_mode(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    let old;
    {
        let any: &mut dyn std::any::Any = flow.current_mut().unwrap().as_mut();
        let s = any.downcast_mut::<GamePlayScreen>().unwrap();
        old = find(&ui, s, window::TOOLBAR);
        let b = rect(&ui, old);
        s.load_screen_layout(
            &mut ui,
            &layout(saved(b, 1024 - b.width(), 768 - b.height())),
        );
    }
    ui.refresh_event((800, 600));
    let b = rect(&ui, old);
    let parent = rect(&ui, ui.parent(old).unwrap());
    assert_eq!(
        (b.x0, b.y0),
        clamped(parent, b, b.x0, b.y0),
        "refresh_event must dispatch the toolbar's own move"
    );
    assert_drawn(&ui, old);
    flow.queue(mode::GAME_PLAY);
    flow.use_new_mode(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
    assert!(
        ui.node(old).is_none(),
        "old toolbar destroyed by same-mode generation"
    );
    let any: &mut dyn std::any::Any = flow.current_mut().unwrap().as_mut();
    let s = any.downcast_mut::<GamePlayScreen>().unwrap();
    let new = find(&ui, s, window::TOOLBAR);
    let b = rect(&ui, new);
    s.load_screen_layout(&mut ui, &layout(saved(b, 9999, 9999)));
    assert_drawn(&ui, new);
    assert_ne!(new, old);
}

mod persistence {
    //! The shipped toolbar writes equal calls and achieved values back in call order; the class caches
    //! only the post-init enum and zero stays guarded.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::{RegistrationOrder, Store};
    use dereth_ui::{ElemHandle, Screen, UiSystem};
    use dereth_ui_screens::{
        screens::gameplay::{window, GamePlayScreen},
        view::UiRequest,
    };
    use std::rc::Rc;

    fn screen() -> (UiSystem, GamePlayScreen, Rc<Store>) {
        let (mut ui, _flow, store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui.requests.clear();
        let mut screen = GamePlayScreen::default();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .unwrap();
        (ui, screen, store)
    }
    fn rect(ui: &UiSystem, h: ElemHandle) -> dereth_ui::region::Box2D {
        ui.node(h).unwrap().region.box_
    }
    fn writes(ui: &mut UiSystem, window: u32) -> Vec<(u32, i32)> {
        ui.requests
            .take_placement_updates()
            .into_iter()
            .filter_map(|r| match r {
                UiRequest::SetChatWindowOption {
                    window: w,
                    property,
                    value,
                } if w == window => Some((property, value)),
                _ => None,
            })
            .collect()
    }

    /// Behaviour: toolbar.layout.the-toolbar-writes-its-achieved-position-and-size-back-in-order
    #[test]
    fn actual_toolbar_writes_equal_calls_and_achieved_values_in_source_order() {
        let (mut ui, screen, _) = screen();
        let h = ui
            .get_child_recursive(screen.root().unwrap(), window::TOOLBAR)
            .unwrap();
        let id = screen.window_id_of(window::TOOLBAR);
        assert_ne!(id, 0);
        assert!(
            writes(&mut ui, id).is_empty(),
            "ctor ID zero and no synthetic startup SetVisible tail"
        );
        let b = rect(&ui, h);
        let visible = ui.node(h).unwrap().region.flags.visible;
        ui.move_to(h, b.x0, b.y0);
        ui.resize_to(h, b.width(), b.height());
        ui.set_visible(h, visible);
        assert_eq!(
            writes(&mut ui, id),
            vec![
                (0x1000_0086, b.x0),
                (0x1000_0087, b.y0),
                (0x1000_0088, b.width()),
                (0x1000_0089, b.height()),
                (0x1000_008A, i32::from(visible))
            ],
            "base no-op does not skip the derived writeback tail"
        );
        let p = rect(&ui, ui.parent(h).unwrap());
        ui.move_to(h, 9999, -9999);
        ui.resize_to(h, 9999, 9999);
        let achieved = rect(&ui, h);
        ui.set_visible(screen.root().unwrap(), false);
        ui.set_visible(h, true);
        assert_eq!(
            writes(&mut ui, id),
            vec![
                (0x1000_0086, (p.width() - b.width()).max(0)),
                (0x1000_0087, 0),
                (0x1000_0088, achieved.width()),
                (0x1000_0089, achieved.height()),
                (0x1000_008A, 1)
            ]
        );
        assert!(
            !ui.draw_trace().iter().any(|e| e.who == h),
            "stored visibility is the argument, not ancestor-effective visibility"
        );
        let pans = ui
            .get_child_recursive(screen.root().unwrap(), window::PANEL_STACK)
            .unwrap();
        ui.move_to(pans, -99, 9999);
        ui.resize_to(pans, 9999, 9999);
        ui.set_visible(pans, true);
        assert!(
            ui.requests.take_placement_updates().is_empty(),
            "default hooks add no tails to other floaty classes"
        );
    }

    #[test]
    fn actual_class_caches_only_the_post_init_enum_and_zero_stays_guarded() {
        let (mut ui, screen, store) = screen();
        let h = ui
            .get_child_recursive(screen.root().unwrap(), window::TOOLBAR)
            .unwrap();
        let node = ui.node(h).unwrap();
        let original = node.desc.clone();
        let layout = dereth_ui::desc::LayoutDesc {
            did: node.layout_did,
            display_width: node.layout_design.width(),
            display_height: node.layout_design.height(),
            ..Default::default()
        };
        let id = screen.window_id_of(window::TOOLBAR);
        for value in [
            None,
            Some(dereth_ui::props::PropertyValue::Enum(0)),
            Some(dereth_ui::props::PropertyValue::Integer(id as i32)),
            Some(dereth_ui::props::PropertyValue::Enum(id)),
        ] {
            // Only the private window-ID property differs; actual class, DAT desc and children remain.
            let mut desc = original.clone();
            desc.base.properties.remove(0x1000_007E);
            if let Some(v) = value.clone() {
                desc.base.properties.set(0x1000_007E, v);
            }
            let h = ui
                .create_element(store.as_ref(), &layout, &desc)
                .unwrap()
                .unwrap();
            ui.set_parent(h, Some(ui.root()));
            ui.requests.clear();
            ui.move_to(h, 13, 17);
            ui.resize_to(h, 9999, 9999);
            ui.set_visible(h, false);
            assert!(
                ui.requests.take_placement_updates().is_empty(),
                "ctor must not cache the desc's ID early"
            );
            ui.initialize_tree(h);
            ui.requests.clear();
            ui.move_to(h, 23, 29);
            let got = ui.requests.take_placement_updates();
            if value == Some(dereth_ui::props::PropertyValue::Enum(id)) {
                assert_eq!(
                    got,
                    vec![
                        UiRequest::SetChatWindowOption {
                            window: id,
                            property: 0x1000_0086,
                            value: 23
                        },
                        UiRequest::SetChatWindowOption {
                            window: id,
                            property: 0x1000_0087,
                            value: 29
                        }
                    ]
                );
                ui.set_attribute_enum(h, 0x1000_007E, 0);
                ui.move_to(h, 31, 37);
                assert_eq!(
                    writes(&mut ui, id),
                    vec![(0x1000_0086, 31), (0x1000_0087, 37)],
                    "post-init caches once"
                );
            } else {
                assert!(
                    got.is_empty(),
                    "missing, zero or wrong-type window ID is guarded"
                );
            }
        }
    }
}
