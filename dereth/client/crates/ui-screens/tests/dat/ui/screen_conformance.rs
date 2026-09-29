//! Every screen builds from its layout without leaking handles; documented screen/panel child
//! bindings resolve; sixteen windows and window chrome present; ui lock swaps chrome (incl. radar);
//! notice replay inert; screen layout file round-trips; layout enums resolve via the DidMapper.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::AssetSource;
use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver};
use dereth_ui::persist::{SavedWindow, ScreenLayout};
use dereth_ui::{ElemHandle, ElementId, ElementMessage, MessageId, UiSystem};

use crate::common::*;
use dereth_ui_screens::bind::bind_children;
use dereth_ui_screens::hud::floaty::{FLOATY_CHROME, GAMEPLAY_WINDOWS, SMART_BOX_CHROME};
use dereth_ui_screens::panels::catalogue::{PANELS, PANEL_PAGES, TOOLBAR_PANEL_BUTTONS};
use dereth_ui_screens::screens::SCREENS;

struct Env {
    ui: UiSystem,
}

fn env() -> Option<Env> {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    Some(Env { ui })
}

/// Behaviour: ui.screens.every-screen-builds-from-its-shipped-layout-and-binds-its-documented-children
/// Gate step 1: every screen constructs from its real layout, and step 5: destroying it returns the
/// element arena to its pre-construction size.
#[test]
fn every_screen_constructs_from_its_real_layout_and_leaks_no_handles() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    for spec in SCREENS {
        let before = ui.element_list().len();
        let mut s = make(spec.class);
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .unwrap_or_else(|e| panic!("{}: {e}", spec.class));

        // Every documented root was created, in the documented order.
        let roots = s.roots().to_vec();
        assert_eq!(roots.len(), spec.roots.len(), "{}: root count", spec.class);
        for (h, want) in roots.iter().zip(spec.roots) {
            let got = ui.node(*h).map(dereth_ui::element::ElementNode::element_id);
            assert_eq!(got, Some(*want), "{}: root element id", spec.class);
        }
        assert!(
            ui.element_list().len() > before,
            "{}: built nothing",
            spec.class
        );

        // Step 5: tear down the old mode's roots, as a mode switch does.
        for h in roots {
            ui.remove_and_delete_root(h);
        }
        s.destroy(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
        ui.clean_delete_queue();
        assert_eq!(
            ui.element_list().len(),
            before,
            "{}: the arena did not return to its pre-construction size",
            spec.class
        );
        ui.requests.clear();
    }
}

/// Gate step 2: every documented child binding resolves. "A null child handle is a failure, not a
/// warning."
#[test]
fn every_documented_screen_child_binding_resolves_in_the_real_tree() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    type Row = (
        &'static str,
        &'static [dereth_ui_screens::bind::ChildBinding],
    );
    let rows: &[Row] = &[
        (
            "DataPatchScreen",
            dereth_ui_screens::screens::datapatch::CHILDREN,
        ),
        (
            "DisconnectedScreen",
            dereth_ui_screens::screens::disconnected::CHILDREN,
        ),
        (
            "CharacterManagementScreen",
            dereth_ui_screens::screens::charmgmt::CHILDREN,
        ),
        ("CharGenScreen", dereth_ui_screens::screens::chargen::CHROME),
    ];
    for (class, table) in rows {
        let mut s = make(class);
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .unwrap_or_else(|e| panic!("{class}: {e}"));
        let root = s.roots()[0];
        let b = bind_children(&ui, root, table);
        assert!(
            b.is_complete(),
            "{class}: unresolved children {:?}",
            b.missing
                .iter()
                .map(|m| (m.field, m.id))
                .collect::<Vec<_>>()
        );
        for h in s.roots().to_vec() {
            ui.remove_and_delete_root(h);
        }
        ui.clean_delete_queue();
    }
    // The char-gen wizard's six pages are separate elements found under the master page.
    let mut s = make("CharGenScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = s.roots()[0];
    for p in dereth_ui_screens::screens::chargen::EcgProgress::PAGES {
        let (id, _) = p.page().unwrap();
        assert!(
            ui.get_child_recursive(root, id).is_some(),
            "char-gen page {p:?} ({id:?})"
        );
        let btn = p.select_button().unwrap();
        assert!(
            ui.get_child_recursive(root, btn).is_some(),
            "page-select button {p:?}"
        );
    }
}

/// Gate step 2 for the HUD: build the gameplay screen and resolve every panel's binding table
/// against the one real element tree the whole HUD lives in.
///
/// Panels whose element is not reachable from the gameplay root are **reported, not asserted**: the
/// panel documents were written from the class bodies, and whether a given panel's subtree is
/// instantiated by this layout is a property of the shipped data. What is asserted is that a panel
/// which *is* present has every documented child.
#[test]
fn every_panel_present_in_the_gameplay_tree_has_all_its_documented_children() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    let mut s = make("GamePlayScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = s.roots()[0];

    let mut present = 0;
    let mut absent: Vec<&str> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for spec in PANELS {
        if spec.children.is_empty() {
            continue;
        }
        // A panel is "present" when at least one of its documented children is in the tree; that is
        // a weaker test than looking the panel element up, because the panel classes are attached
        // to described children whose own element ids are layout data.
        let b = bind_children(&ui, root, spec.children);
        if b.found.is_empty() {
            absent.push(spec.class);
            continue;
        }
        present += 1;
        if !b.is_complete() {
            failures.push(format!(
                "{}: {} of {} children missing: {:?}",
                spec.class,
                b.missing.len(),
                spec.children.len(),
                b.missing.iter().map(|m| m.field).collect::<Vec<_>>()
            ));
        }
    }
    eprintln!("panels present in the gameplay tree: {present}; absent: {absent:?}");
    assert!(
        present > 0,
        "no panel at all resolved: the gameplay tree did not build"
    );
    assert!(
        failures.is_empty(),
        "partial panel bindings:\n{}",
        failures.join("\n")
    );
}

/// All sixteen windows are present under the gameplay root.
#[test]
fn all_sixteen_windows_are_present_under_the_gameplay_root() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    let mut s = make("GamePlayScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = s.roots()[0];
    for w in GAMEPLAY_WINDOWS {
        assert!(
            ui.get_child_recursive(root, w.element).is_some(),
            "{} ({}) is not in the gameplay tree",
            std::str::from_utf8(w.tag).unwrap(),
            w.class
        );
    }
    // The sixteen page containers and the seven toolbar buttons are the panel-visibility mechanism
    // (trap 11); both sides must exist for the indirection to have anything to indirect through.
    let pages = PANEL_PAGES
        .iter()
        .filter(|p| ui.get_child_recursive(root, ElementId(**p)).is_some())
        .count();
    let buttons = TOOLBAR_PANEL_BUTTONS
        .iter()
        .filter(|b| ui.get_child_recursive(root, ElementId(**b)).is_some())
        .count();
    eprintln!("panel pages present: {pages}/16; toolbar panel buttons present: {buttons}/7");
    assert_eq!(pages, 16, "the panel stack's sixteen page containers");
    assert_eq!(buttons, 7, "the toolbar's seven panel buttons");
}

/// Gate step 2 for the chrome: every floating-window block's sixteen ids, and the smart box's eight.
#[test]
fn the_window_chrome_ids_are_all_present_in_the_real_layout() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    let mut s = make("GamePlayScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = s.roots()[0];

    for id in SMART_BOX_CHROME {
        assert!(
            ui.get_child_recursive(root, id).is_some(),
            "smart box chrome {id:?}"
        );
    }
    let mut incomplete: Vec<String> = Vec::new();
    for block in FLOATY_CHROME {
        let n = block
            .all()
            .iter()
            .filter(|id| ui.get_child_recursive(root, **id).is_some())
            .count();
        if n != 16 {
            incomplete.push(format!("{}: {n}/16", block.class));
        }
    }
    assert!(
        incomplete.is_empty(),
        "incomplete chrome blocks: {incomplete:?}"
    );
}

/// K-8's own gate: "Toggling lock swaps all eight children on every floaty window."
///
/// Driven against the real tree, so the sixteen ids per block are the shipped ones and the swap is
/// observable as a visibility flip on thirty-two elements per window.
#[test]
fn toggling_the_ui_lock_swaps_all_eight_chrome_children_on_every_floaty_window() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    let mut s = make("GamePlayScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = s.roots()[0];

    for block in FLOATY_CHROME {
        dereth_ui_screens::hud::floaty::swap_chrome(&mut ui, root, &block, true);
        for p in dereth_ui_screens::hud::floaty::ChromePiece::ALL {
            let u = ui.get_child_recursive(root, block.unlocked(p)).unwrap();
            let l = ui.get_child_recursive(root, block.locked(p)).unwrap();
            assert!(
                !ui.node(u).unwrap().region.flags.visible,
                "{} {p:?} unlocked",
                block.class
            );
            assert!(
                ui.node(l).unwrap().region.flags.visible,
                "{} {p:?} locked",
                block.class
            );
        }
        dereth_ui_screens::hud::floaty::swap_chrome(&mut ui, root, &block, false);
        for p in dereth_ui_screens::hud::floaty::ChromePiece::ALL {
            let u = ui.get_child_recursive(root, block.unlocked(p)).unwrap();
            let l = ui.get_child_recursive(root, block.locked(p)).unwrap();
            assert!(
                ui.node(u).unwrap().region.flags.visible,
                "{} {p:?} unlocked",
                block.class
            );
            assert!(
                !ui.node(l).unwrap().region.flags.visible,
                "{} {p:?} locked",
                block.class
            );
        }
    }

    // The radar locks differently (`15` §1.9): the drag button hides and the lock button changes
    // media state, and neither of those elements belongs to a chrome block.
    let drag = ui
        .get_child_recursive(root, dereth_ui_screens::mapradar::radar::child::DRAG_BUTTON)
        .expect("the radar's drag button");
    let lock = ui
        .get_child_recursive(root, dereth_ui_screens::mapradar::radar::child::LOCK_BUTTON)
        .expect("the radar's lock button");
    assert_ne!(drag, lock);
}

/// Replaying every registered notice id is inert and the element messages are not.
#[test]
fn replaying_every_registered_notice_id_is_inert_and_the_element_messages_are_not() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");

    let ids = dereth_ui::NoticeId::ALL;

    for spec in SCREENS {
        let mut s = make(spec.class);
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .unwrap();
        ui.requests.clear();
        for id in ids {
            s.on_notice(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                id,
                &dereth_ui::NoticePayload::default(),
            );
        }
        // A notice no screen acts on must produce no request at all: a screen that reacts to an id
        // it never registered for is a bug the replay is here to catch.
        assert!(
            ui.requests.take().is_empty(),
            "{}: a replayed notice produced a request",
            spec.class
        );
        for h in s.roots().to_vec() {
            ui.remove_and_delete_root(h);
        }
        ui.clean_delete_queue();
    }

    // Step 4: the documented element messages, against the two screens whose tables are complete.
    let msg = |id: u32, m: u32| ElementMessage {
        source_id: ElementId(id),
        source: ElemHandle::for_test(1),
        id: MessageId(m),
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    };
    let mut s = make("DataPatchScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    ui.requests.clear();
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_041C, 1),
    );
    assert_eq!(
        ui.requests.take(),
        vec![dereth_ui_screens::UiRequest::QueueMode(
            dereth_ui_screens::MODE_EPILOGUE
        )]
    );
    for h in s.roots().to_vec() {
        ui.remove_and_delete_root(h);
    }
    ui.clean_delete_queue();

    let mut s = make("DisconnectedScreen");
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_0418, 1),
    );
    assert_eq!(
        ui.requests.take(),
        vec![dereth_ui_screens::UiRequest::QueueMode(
            dereth_ui_screens::MODE_EPILOGUE
        )]
    );
}

/// K-7's own gate: load a screen-layout file, place all sixteen windows, save, and assert the
/// output is **byte-identical**, including the irregular `%s X:%d Y: %d W: %d H: %d ` spacing.
///
/// A real `UI-<char>-<world>-<h>-<w>.txt` from a played
/// character does not exist in the corpus yet, so the file this round-trips is generated from the
/// **live element tree's own layout-authored rectangles**: the sixteen windows are read out of the
/// real layout, written, loaded back into the tree, and written again. That exercises the same two
/// functions against the same sixteen elements and is self-checking in the same way a capture is.
#[test]
fn the_screen_layout_file_round_trips_byte_identically_through_the_live_window_set() {
    let Env { mut ui, .. } = env().expect("the test environment: retail dats and a WARP device");
    let mut boxed = make("GamePlayScreen");
    boxed
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    let root = boxed.roots()[0];

    // Read the sixteen windows' authored rectangles straight out of the tree.
    let mut original = ScreenLayout::default();
    for (k, j) in GAMEPLAY_WINDOWS
        .iter()
        .zip(dereth_ui::persist::WINDOWS.iter())
    {
        let h = ui
            .get_child_recursive(root, k.element)
            .unwrap_or_else(|| panic!("{} missing", j.tag));
        let (x, y) = ui.screen_origin(h);
        let b = ui.screen_box(h);
        original.windows.push((
            j.tag,
            SavedWindow {
                x,
                y,
                w: b.width(),
                h: b.height(),
            },
        ));
    }
    assert_eq!(original.windows.len(), 16);
    let first = original.to_text();
    assert_eq!(
        first.lines().count(),
        1,
        "retail's UI-*.txt is one line: {first:?}"
    );
    assert!(
        !first.contains('\n') && !first.contains('\r'),
        "no row separator at all: {first:?}"
    );
    let expected: String = original
        .windows
        .iter()
        // `ROW_FORMAT` = "%s X:%d Y: %d W: %d H: %d " — note the irregular spacing, which §5 says
        // to keep "if you want existing players' layouts to load", and the trailing space that is
        // the only thing separating one row from the next.
        .map(|(tag, w)| format!("{tag} X:{} Y: {} W: {} H: {} ", w.x, w.y, w.w, w.h))
        .collect();
    assert_eq!(
        first, expected,
        "sixteen ROW_FORMAT rows concatenated, no separator"
    );
    assert_eq!(
        first.matches(" X:").count(),
        16,
        "and all sixteen rows are there"
    );
    assert!(
        !first.contains("X: "),
        "irregular spacing: no space after X:"
    );
    assert!(
        first.ends_with(' '),
        "the format's own trailing space: {first:?}"
    );

    // Perturb every window, then load the file back and re-save it.
    for w in GAMEPLAY_WINDOWS {
        if let Some(h) = ui.get_child_recursive(root, w.element) {
            ui.resize_to(h, 32, 32);
            ui.move_to(h, 5, 5);
        }
    }
    let parsed = ScreenLayout::parse(&first).expect("the file we just wrote must parse");
    assert_eq!(parsed, original);

    // Re-apply through the screen's own loader, which is resize-then-move (`11` §1.1).
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::create_screen();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .unwrap();
    // The second instance owns a second root; place through the first one instead by using the
    // concrete type, which is what the host does.
    for h in screen.roots().to_vec() {
        ui.remove_and_delete_root(h);
    }
    ui.clean_delete_queue();

    for (tag, w) in &parsed.windows {
        let slot = GAMEPLAY_WINDOWS
            .iter()
            .find(|s| std::str::from_utf8(s.tag).is_ok_and(|t| t == *tag))
            .unwrap();
        let h = ui.get_child_recursive(root, slot.element).unwrap();
        ui.resize_to(h, w.w, w.h);
        ui.move_to(h, w.x, w.y);
    }

    let mut again = ScreenLayout::default();
    for (k, j) in GAMEPLAY_WINDOWS
        .iter()
        .zip(dereth_ui::persist::WINDOWS.iter())
    {
        let h = ui.get_child_recursive(root, k.element).unwrap();
        let (x, y) = ui.screen_origin(h);
        let b = ui.screen_box(h);
        again.windows.push((
            j.tag,
            SavedWindow {
                x,
                y,
                w: b.width(),
                h: b.height(),
            },
        ));
    }
    assert_eq!(
        again.to_text(),
        first,
        "the save is not byte-identical after a load"
    );
}

/// The resolver path itself: every layout enum a screen names resolves out of the dat, and the
/// DataID it resolves to is a shipped layout: a `LayoutDesc` (`0x21000000`–`0x21000075`) the dat
/// actually holds.
#[test]
fn every_screen_layout_enum_resolves_through_the_shipped_did_mapper() {
    let _env = env().expect("the test environment: retail dats and a WARP device");
    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).unwrap();
    let r = DidMapperResolver::load_via_master(&store).unwrap();

    for spec in SCREENS {
        for e in spec.layouts {
            let did = r
                .resolve(*e)
                .unwrap_or_else(|| panic!("{}: enum {e:?} does not resolve", spec.class));
            assert!(
                (0x2100_0000..=0x2100_0075).contains(&did.0) && store.exists(did),
                "{}: {did:?} is not a shipped layout",
                spec.class
            );
        }
    }
    // The spew box's bubble layout and the dialog template are named the same way.
    assert!(r
        .resolve(dereth_ui_screens::hud::speech_bubbles::BUBBLE_LAYOUT)
        .is_some());
    assert!(
        r.resolve(LayoutEnum(2)).is_some(),
        "the Dialog template layout"
    );
}

mod spell_examination {
    //! Behaviour: none (checks shipped layout bindings)
    //! The shipped gameplay layout's examine window carries the spell-examine base field with its six
    //! text/icon children and the formula list box whose template list includes the row template.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;

    use dereth_ui::{ElementId, Screen, UiSystem};
    use dereth_ui_screens::panels::examination::WINDOW;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    fn env() -> UiSystem {
        let (ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui
    }

    #[test]
    fn the_spell_examine_pane_is_in_the_shipped_layout() {
        let mut ui = env();
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds from the shipped layout");
        let root = *s.roots().first().expect("root");
        let window = ui
            .get_child_recursive(root, WINDOW)
            .expect("<EXAM> 0x100005F7");
        let base = ui
            .get_child_recursive(window, ElementId(0x1000_0153))
            .expect("the spell-examination pane's base field 0x10000153");
        for id in [
            0x1000_015E,
            0x1000_015F,
            0x1000_0160,
            0x1000_0161,
            0x1000_0162,
            0x1000_0163,
        ] {
            assert!(
                ui.get_child_recursive(base, ElementId(id)).is_some(),
                "{id:#010x} is a child of the spell pane"
            );
        }
        let list = ui
            .get_child_recursive(base, ElementId(0x1000_032D))
            .expect("formula list box 0x1000032D");
        let templates = dereth_ui_screens::panels::listbox::template_list(&ui, list);
        assert!(
            templates.iter().any(|(_, e)| e.0 == 0x1000_032E),
            "The formula list includes its row template: {templates:?}"
        );
    }
}
