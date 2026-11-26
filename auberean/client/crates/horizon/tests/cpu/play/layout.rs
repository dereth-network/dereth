//! The HUD layout: its editor moving and resizing the HUD's elements, what it keeps and reads
//! back, and the elements answering the pointer where they are shown.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::state::Effect;
use auberean_horizon::ui::hud;
use auberean_horizon::ui::layout::{
    element, element_title, file_name, parse, render, Edge, Edges, HudLayout, Placement,
};

use crate::{centre, in_world, Harness};

/// The editor open for character `Kara` of world `Kelderam`, drawn once.
fn editing(h: &mut Harness) {
    h.ui.layout.start(file_name("Kelderam", "Kara"));
    h.ui.layout.read(None);
    h.ui.layout.open_editor();
    h.frame(&in_world());
}

/// Where element `name` is outlined in the editor this frame.
fn outline(h: &Harness, name: &str) -> auberean_horizon::draw::Rect {
    h.ui.layout
        .outlines
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{name} outlined"))
}

#[test]
fn a_layout_reads_back_what_it_wrote_and_leaves_out_the_elements_not_moved() {
    let mut placements = std::collections::BTreeMap::new();
    placements.insert(
        "minimap".to_owned(),
        Placement {
            dx: -40.0,
            dy: 12.5,
            scale: 1.2,
            edges: None,
        },
    );
    placements.insert("chat".to_owned(), Placement::default());
    let text = render(&placements);
    assert_eq!(text, "minimap -40.0 12.5 1.20\n");
    let back = parse(&text);
    assert_eq!(back.len(), 1);
    assert_eq!(back["minimap"].scale, 1.2);
    let odd = parse("hotbar 0 0 9\nnonsense\nchat 1 2\nparty NaN 0 1\nmenu 1 2 1 xy\n");
    assert_eq!(odd.len(), 1, "bad lines skipped");
    let edged = parse("menu 108.0 58.0 1.00 rb\nchat 16.0 -3.5 1.00 cm\n");
    assert_eq!(
        render(&edged),
        "chat 16.0 -3.5 1.00 cm\nmenu 108.0 58.0 1.00 rb\n"
    );
    assert_eq!(
        edged["menu"].edges,
        Some(Edges {
            across: Edge::End,
            down: Edge::End
        })
    );
    assert_eq!(odd["hotbar"].scale, 2.0, "held to the largest");
    assert_eq!(
        file_name("Kelderam", "Kara the Bold"),
        "auberean-horizon-hud-Kelderam-Kara_the_Bold.txt"
    );
}

#[test]
fn dragging_an_element_in_the_editor_moves_it_and_save_keeps_it_for_the_character() {
    let mut h = Harness::new();
    editing(&mut h);
    let s = in_world();
    let k = h.k();
    let menu = outline(&h, element::MENU);
    let from = centre(menu);
    let to = (from.0 - 300.0 * k, from.1 - 200.0 * k);
    h.drag(&s, from, to);
    let moved = h.ui.layout.placement(element::MENU);
    assert!(
        (moved.dx + 300.0).abs() < 0.5 && (moved.dy + 200.0).abs() < 0.5,
        "{moved:?}"
    );
    h.frame(&s);
    let shown = outline(&h, element::MENU);
    assert!((shown.x - (menu.x - 300.0 * k)).abs() < 0.5);
    // Nothing kept until Save.
    assert!(h.ui.layout.unsaved());
    assert_eq!(h.ui.layout.take_write(), None);
    let c = HudLayout::controls(h.ui.layout_editor_body().expect("open"), k);
    h.click(&s, centre(c.save));
    let (file, text) = h.ui.layout.take_write().expect("saved");
    assert_eq!(file, "auberean-horizon-hud-Kelderam-Kara.txt");
    // Kept by the right edge and the foot, where it stands.
    let shown = outline(&h, element::MENU);
    let right = (h.screen.0 - shown.right()) / k;
    let foot = (h.screen.1 - shown.bottom()) / k;
    assert_eq!(text, format!("menu {right:.1} {foot:.1} 1.00 rb\n"));
    assert!(!h.ui.layout.unsaved());
}

#[test]
fn the_wheel_resizes_the_element_selected_within_its_range_and_closing_unsaved_puts_it_back() {
    let mut h = Harness::new();
    editing(&mut h);
    let s = in_world();
    let chat = centre(outline(&h, element::CHAT));
    h.click(&s, chat);
    assert_eq!(h.ui.layout.selected, Some(element::CHAT));
    h.input.mouse = chat;
    h.input.wheel = 1.0;
    h.frame(&s);
    assert!((h.ui.layout.placement(element::CHAT).scale - 1.1).abs() < 1e-5);
    for _ in 0..20 {
        h.input.wheel = 1.0;
        h.frame(&s);
    }
    assert_eq!(h.ui.layout.placement(element::CHAT).scale, 2.0, "at most");
    // Escape closes the editor: the change was never saved, so it is put back.
    let mut acts = Vec::new();
    assert!(h.ui.close_top(&mut acts));
    assert!(!h.ui.layout.editing());
    assert!(h.ui.layout.placement(element::CHAT).is_default());
}

#[test]
fn an_element_moved_answers_the_pointer_where_it_is_shown_and_not_where_it_was() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.layout.read(Some("menu -400 -300 1\n"));
    h.frame(&s);
    let k = h.k();
    let was = hud::menu_rect(h.screen, k);
    let now = (centre(was).0 - 400.0 * k, centre(was).1 - 300.0 * k);
    assert!(h.ui.covers(now.0, now.1), "covers where it is shown");
    assert!(
        !h.ui.covers(centre(was).0, centre(was).1),
        "not where it was"
    );
    // The system menu's button, at the bar's right end, where it was and where it is shown.
    let button = hud::menu_button_rect(h.screen, k, hud::MenuButton::ALL.len() - 1);
    let moved = (centre(button).0 - 400.0 * k, centre(button).1 - 300.0 * k);
    h.click(&s, centre(button));
    assert!(!h.ui.system_menu);
    h.click(&s, moved);
    assert!(
        h.ui.system_menu,
        "the system menu's button pressed where shown"
    );
}

#[test]
fn an_element_moved_or_grown_past_the_screen_s_edge_is_kept_on_it() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.layout.read(Some("menu 500 500 2\n"));
    h.frame(&s);
    h.ui.layout.open_editor();
    h.frame(&s);
    let r = outline(&h, element::MENU);
    assert!(
        r.right() <= h.screen.0 + 0.5 && r.bottom() <= h.screen.1 + 0.5,
        "{r:?}"
    );
    assert!(r.x >= 0.0 && r.y >= 0.0);
}

#[test]
fn the_settings_hud_layout_button_opens_the_editor_and_the_hud_takes_no_pointer_meanwhile() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.settings.toggle();
    h.frame(&s);
    let c = auberean_horizon::ui::settings::Settings::controls(
        h.ui.settings.body().expect("open"),
        h.k(),
    );
    h.click(&s, centre(c.layout));
    assert!(h.ui.layout.editing(), "the editor open");
    assert!(!h.ui.settings.is_open(), "the settings closed");
    // The menu bar under the editor's outline does nothing while editing.
    let menu = hud::menu_button_rect(h.screen, h.k(), hud::MenuButton::ALL.len() - 1);
    h.click(&s, centre(menu));
    assert!(!h.ui.system_menu);
    assert_eq!(
        h.ui.layout.selected,
        Some(element::MENU),
        "selected instead"
    );
}

#[test]
fn a_shortcut_dragged_between_the_slots_of_a_hotbar_moved_lands_in_the_slot_shown_there() {
    use auberean_client_sdk::contract::view as v;
    use auberean_client_sdk::contract::{shortcut_kind, Action, ToolbarShortcut};
    use auberean_horizon::abilities::{Bar, Slot};
    use auberean_horizon::ui::hotbar::slot_rect;
    let mut h = Harness::new();
    let mut s = in_world();
    let mut slots = vec![Slot::default(); 100];
    slots[0] = Slot {
        shortcut: v::Shortcut {
            kind: shortcut_kind::SKILL,
            data_enum: 7,
            ..v::Shortcut::default()
        },
        name: "Slash".into(),
        ..Slot::default()
    };
    s.bar = Some(Bar {
        version: 1,
        slots,
        set: 1,
        rows: 1,
        locked: false,
        numbers: true,
    });
    h.ui.layout.read(Some("hotbar 0 -300 1\n"));
    h.frame(&s);
    let k = h.k();
    let up = |r: auberean_horizon::draw::Rect| (centre(r).0, centre(r).1 - 300.0 * k);
    let from = up(slot_rect(h.screen, k, 0, 0));
    let to = up(slot_rect(h.screen, k, 0, 3));
    let out = h.drag(&s, from, to);
    assert_eq!(
        out.acts,
        vec![
            Action::SetToolbarSlot {
                slot: 0,
                shortcut: ToolbarShortcut::default()
            },
            Action::SetToolbarSlot {
                slot: 3,
                shortcut: ToolbarShortcut {
                    kind: shortcut_kind::SKILL,
                    data_enum: 7,
                    ..ToolbarShortcut::default()
                }
            },
        ]
    );
}

#[test]
fn every_element_is_outlined_and_named_in_the_editor_with_nothing_to_show() {
    let mut h = Harness::new();
    // No effects, nothing selected, no fellowship, no pets, no mount, no experience told, and
    // no gamepad.
    editing(&mut h);
    for name in element::ALL {
        let r = outline(&h, name);
        assert!(r.w > 1.0 && r.h > 1.0, "{name}: {r:?}");
        assert_ne!(element_title(name), name, "{name} has a title");
        assert!(
            h.ui.layout.labels.iter().any(|(n, _)| *n == name),
            "{name} named"
        );
    }
    assert_eq!(h.ui.layout.outlines.len(), element::ALL.len());
}

#[test]
fn dragging_the_empty_status_row_moves_it_and_the_effects_then_draw_there() {
    let mut h = Harness::new();
    editing(&mut h);
    let mut s = in_world();
    let k = h.k();
    let row = outline(&h, element::STATUS);
    let from = centre(row);
    h.drag(&s, from, (from.0 - 200.0 * k, from.1 + 300.0 * k));
    let c = HudLayout::controls(h.ui.layout_editor_body().expect("open"), k);
    h.click(&s, centre(c.save));
    h.click(&s, centre(c.close));
    assert!(!h.ui.layout.editing());
    s.effects = vec![Effect {
        id: 1,
        name: "Effect".into(),
        icon: 0,
        left: None,
    }];
    h.frame(&s);
    let first = hud::effect_rect(h.screen, k, 0);
    let tile = h
        .list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && q.colour == 0xC020_2830)
        .map(|q| q.dst)
        .expect("the effect's tile");
    assert!(
        (tile.x - (first.x - 200.0 * k)).abs() < 0.5
            && (tile.y - (first.y + 300.0 * k)).abs() < 0.5,
        "{tile:?} from {first:?}"
    );
}

#[test]
fn the_log_windows_tabs_popped_out_are_placed_by_the_editor_while_it_is_open() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    assert!(!h.ui.chat.editing);
    editing(&mut h);
    assert!(h.ui.chat.editing, "placed as the HUD's elements are");
    let mut acts = Vec::new();
    assert!(h.ui.close_top(&mut acts));
    h.frame(&s);
    assert!(!h.ui.chat.editing, "the editor closed");
}

#[test]
fn the_shipped_layout_is_a_new_characters_and_reset_puts_it_back() {
    let mut h = Harness::new();
    h.ui.layout = HudLayout::with_default("menu -40.0 -20.0 1.00\n");
    editing(&mut h);
    let s = in_world();
    let k = h.k();
    let menu = h.ui.layout.placement(element::MENU);
    assert!((menu.dx + 40.0).abs() < 1e-5 && (menu.dy + 20.0).abs() < 1e-5);
    assert!(!h.ui.layout.unsaved(), "nothing to save yet");
    let from = centre(outline(&h, element::MINIMAP));
    h.drag(&s, from, (from.0 - 100.0 * k, from.1));
    h.drag(
        &s,
        centre(outline(&h, element::MENU)),
        (300.0 * k, 300.0 * k),
    );
    assert!(!h.ui.layout.placement(element::MINIMAP).is_default());
    let c = HudLayout::controls(h.ui.layout_editor_body().expect("open"), k);
    h.click(&s, centre(c.reset));
    assert!(h.ui.layout.placement(element::MINIMAP).is_default());
    assert_eq!(
        h.ui.layout.placement(element::MENU),
        menu,
        "the shipped place"
    );
    // A character with a file of its own has its own layout.
    h.ui.layout.start(file_name("Kelderam", "Tova"));
    h.ui.layout.read(Some("chat 10 0 1\n"));
    assert!(h.ui.layout.placement(element::MENU).is_default());
}

#[test]
fn every_line_of_the_shipped_layout_reads_and_names_an_element() {
    let shipped = parse(auberean_horizon::ui::layout::SHIPPED);
    let lines = auberean_horizon::ui::layout::SHIPPED
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(shipped.len(), lines, "every line reads");
    for name in shipped.keys() {
        assert!(
            element::ALL.contains(&name.as_str()),
            "{name} is an element"
        );
    }
    assert_eq!(HudLayout::default().default_placements(), &shipped);
}

#[test]
fn a_saved_element_keeps_its_distance_from_the_screen_edge_it_stands_nearest() {
    // Moved on a 1920 by 1080 screen and saved: the menu bar stays by the right and the foot,
    // the minimap moved across to the left keeps to the left, and the party list moved toward
    // the middle keeps to the middle.
    let mut h = Harness::new();
    h.ui.layout.start(file_name("Kelderam", "Kara"));
    h.ui.layout.read(Some(
        "menu -100 -50 1\nminimap -1500 0 1\nparty 600 400 1\n",
    ));
    h.ui.layout.open_editor();
    h.frame(&in_world());
    h.ui.layout.save();
    let (_, text) = h.ui.layout.take_write().expect("saved");
    let kept = parse(&text);
    let letters = |n: &str| kept[n].edges.map(|e| e.letters());
    assert_eq!(letters(element::MENU).as_deref(), Some("rb"), "{text}");
    assert_eq!(letters(element::MINIMAP).as_deref(), Some("lt"), "{text}");
    assert_eq!(
        letters(element::PARTY).as_deref().map(|l| &l[..1]),
        Some("c"),
        "{text}"
    );
    // In layout units: from the left, the middle and the right, then the top, the middle and the
    // bottom.
    let at = |screen: (f32, f32), name: &str| {
        let mut h = Harness::new();
        h.screen = screen;
        h.ui.layout.read(Some(&text));
        // The editor outlines the party list out of a fellowship too.
        h.ui.layout.open_editor();
        h.frame(&in_world());
        let k = h.k();
        let r = outline(&h, name);
        [
            r.x / k,
            (r.x + r.w / 2.0 - screen.0 / 2.0) / k,
            (screen.0 - r.right()) / k,
            r.y / k,
            (r.y + r.h / 2.0 - screen.1 / 2.0) / k,
            (screen.1 - r.bottom()) / k,
        ]
    };
    let near = |a: f32, b: f32| (a - b).abs() < 1.5;
    let first = (1920.0, 1080.0);
    // The same shape at another size, a wider one, and one smaller in layout units.
    for screen in [(3840.0, 2160.0), (2560.0, 1080.0), (2560.0, 1440.0)] {
        let (menu, was) = (at(screen, element::MENU), at(first, element::MENU));
        assert!(
            near(menu[2], was[2]) && near(menu[5], was[5]),
            "menu on {screen:?}"
        );
        let (map, was) = (at(screen, element::MINIMAP), at(first, element::MINIMAP));
        assert!(
            near(map[0], was[0]) && near(map[3], was[3]),
            "minimap on {screen:?}: {map:?} from {was:?}"
        );
        let (party, was) = (at(screen, element::PARTY), at(first, element::PARTY));
        assert!(near(party[1], was[1]), "party on {screen:?}");
    }
}

#[test]
fn a_line_kept_without_edges_still_reads_as_an_offset_from_the_default_place() {
    let old = parse("menu -100 -50 1\n");
    assert_eq!(old["menu"].edges, None);
    let mut h = Harness::new();
    h.ui.layout.read(Some("menu -100 -50 1\n"));
    h.frame(&in_world());
    let k = h.k();
    let was = hud::menu_rect(h.screen, k);
    let now = outline(&h, element::MENU);
    assert!((now.x - (was.x - 100.0 * k)).abs() < 0.5 && (now.y - (was.y - 50.0 * k)).abs() < 0.5);
}
