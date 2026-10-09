//! Gamepad mode's reach into the interface: what the focus can rest on, the shoulder buttons'
//! tabs, the pad's menu, and what is drawn for it.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::Item;
use dereth_horizon::ui::game::{GameState, Prompt};
use dereth_horizon::ui::input::PadHints;
use dereth_horizon::ui::nav::{Cursor, Dir, Kind, PanelKind, Stepped};
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn click_at(h: &mut Harness, state: &GameState, at: (f32, f32)) {
    h.move_to(at.0, at.1);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
}

#[test]
fn a_shoulder_button_turns_the_tab_of_the_window_the_focus_is_in() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let window = snap.layers.len();
    assert!(window > 0, "Settings is a layer of its own");
    let bar = snap.tabs_in(window).expect("Settings' tabs are noted");
    let before = h.ui.windows.options_page.tab;
    h.input.pad.tab_step = Some((bar, 1));
    h.frame(&state);
    assert_eq!(h.ui.windows.options_page.tab, before + 1);
    h.input.pad.tab_step = Some((bar, -1));
    h.frame(&state);
    h.input.pad.tab_step = Some((bar, -1));
    h.frame(&state);
    assert_eq!(
        h.ui.windows.options_page.tab, 4,
        "round from the first tab to the last"
    );
}

#[test]
fn the_focus_reaches_a_slider_and_knows_where_its_knob_is() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(&state);
    // The Controls tab, where the camera's sliders are.
    h.ui.windows.options_page.tab = 3;
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let mut cursor = Cursor::default();
    cursor.enter(&snap, snap.layers.len());
    let mut reached = None;
    for _ in 0..20 {
        if let Some(t) = cursor
            .focus
            .filter(|t| matches!(t.kind, Kind::Slider { .. }))
        {
            reached = Some(t);
            break;
        }
        if cursor.step(&snap, Dir::Down) != Stepped::Moved {
            break;
        }
    }
    let slider = reached.expect("the d-pad reaches a slider going down the page");
    let Kind::Slider { x0, x1, t } = slider.kind else {
        unreachable!()
    };
    assert!(x0 < x1 && (0.0..=1.0).contains(&t));
    assert!(slider.layer > 0, "in the window, not the HUD under it");
}

#[test]
fn a_modal_box_leaves_only_its_buttons_in_reach() {
    let mut state = in_world();
    state.prompts = vec![Prompt {
        id: 3,
        question: true,
        modal: true,
        waiting: 0,
        text: "Really?".into(),
    }];
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = &h.input.nav.last;
    let top = snap.layers.len();
    assert_eq!(snap.targets.len(), 2, "Yes and No: {:?}", snap.targets);
    assert!(snap.targets.iter().all(|t| t.layer == top));
    assert_eq!(
        snap.cancel_of(top),
        snap.targets
            .iter()
            .map(|t| t.rect)
            .max_by(|a, b| a.x.total_cmp(&b.x)),
        "the pad's cancel is the box's last button, No"
    );
    let mut cursor = Cursor::default();
    cursor.follow(snap);
    let yes = cursor.focus.expect("the focus goes into the box");
    assert_eq!(
        cursor.step(snap, Dir::Right),
        Stepped::Moved,
        "and across to No"
    );
    assert!(cursor.focus.unwrap().rect.x > yes.rect.x);
}

#[test]
fn the_pad_s_menu_opens_the_window_chosen_on_it_and_closes() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.input.pad.menu = true;
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let top = snap.layers.len();
    let mut buttons: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| t.layer == top && t.rect.w > 100.0)
        .map(|t| t.rect)
        .collect();
    buttons.sort_by(|a, b| a.y.total_cmp(&b.y));
    assert_eq!(
        buttons.len(),
        10,
        "an entry for each window, and logging out"
    );
    // The second entry: Inventory.
    click_at(&mut h, &state, centre(buttons[1]));
    assert!(h.ui.windows.is_open(WindowId::Inventory));
    assert!(!h.input.pad.menu, "chosen, the menu closes");
}

#[test]
fn gamepad_mode_marks_the_focus_with_the_pointer_alone_and_shows_no_line_of_hints() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    let band = |h: &Harness| {
        h.list
            .quads
            .iter()
            .filter(|q| q.dst.y >= 80.0 && q.dst.bottom() <= 130.0)
            .count()
    };
    let quiet = band(&h);
    h.input.pad.mode = Some(PadHints::Cursor);
    let focus = Rect::new(900.0, 400.0, 44.0, 46.0);
    h.input.pad.focus = Some(focus);
    h.frame(&state);
    assert_eq!(band(&h), quiet, "no line of hints along the top");
    let left_of = h.list.quads.iter().any(|q| {
        q.dst.right() <= focus.x + 1.0 && q.dst.y >= focus.y && q.dst.bottom() <= focus.bottom()
    });
    assert!(left_of, "the pointer at its left edge");
    let around = focus.inset(-8.0);
    let ringed = h.list.quads.iter().any(|q| {
        q.tex.is_none()
            && q.dst.x > focus.x
            && q.dst.right() <= around.right()
            && q.dst.y >= around.y
            && q.dst.bottom() <= around.bottom()
            && (q.colour & 0x00FF_FFFF) == 0x00F0_C860
    });
    assert!(!ringed, "no ring round it");
}

#[test]
fn a_window_is_cancelled_by_its_close_button_and_the_chat_log_is_a_panel_of_its_own() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let chat = snap
        .layers
        .iter()
        .position(|l| l.kind == PanelKind::Chat)
        .expect("the chat log is a panel")
        + 1;
    let inventory = snap
        .layers
        .iter()
        .position(|l| l.name == "Inventory")
        .expect("the inventory is a panel")
        + 1;
    assert!(snap.has_targets(chat), "the chat log has somewhere to rest");
    assert_eq!(snap.layers[chat - 1].kind, PanelKind::Chat);
    assert_eq!(snap.layers[inventory - 1].kind, PanelKind::Window);
    assert!(snap.panels().contains(&chat) && snap.panels().contains(&inventory));
    // The pad's cancel on the window closes it, as its close button would.
    h.input.pad.close = Some(snap.layers[inventory - 1].rect);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Inventory));
}

fn carrying(n: u32) -> GameState {
    GameState {
        pack: (0..n)
            .map(|i| Item {
                id: dereth_primitives::ObjectId(0x8000_1000 + i),
                name: format!("Thing {i}"),
                stack: Some(3),
                ..Item::default()
            })
            .collect(),
        ..in_world()
    }
}

#[test]
fn the_inventory_scrolls_with_the_focus_and_its_packs_step_with_the_shoulders() {
    let state = carrying(90);
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let inventory = snap
        .layers
        .iter()
        .position(|l| l.name == "Inventory")
        .unwrap()
        + 1;
    let (packs, at) = snap.layers[inventory - 1]
        .steps
        .clone()
        .expect("the packs step");
    assert_eq!((packs.len(), at), (1, 0), "the main pack, shown");
    assert!(
        snap.hidden.iter().any(|t| t.layer == inventory),
        "rows out of sight are kept for scrolling to"
    );
    // The lowest row in sight: a step down scrolls instead of leaving the grid.
    let lowest = snap
        .targets
        .iter()
        .filter(|t| t.layer == inventory && t.clip.is_some() && t.rect.w < 60.0)
        .max_by(|a, b| a.rect.y.total_cmp(&b.rect.y))
        .copied()
        .unwrap();
    let mut cursor = Cursor::default();
    cursor.enter_inside(&snap, inventory);
    cursor.focus = Some(lowest);
    let pitch = snap
        .targets
        .iter()
        .filter(|t| t.layer == inventory && t.clip.is_some() && t.rect.w < 60.0)
        .map(|t| t.rect.y)
        .filter(|y| *y < lowest.rect.y - 2.0)
        .fold(f32::MIN, f32::max);
    assert_eq!(
        cursor.step(&snap, Dir::Down),
        Stepped::ScrollBy {
            area: snap.list_of(&lowest).unwrap().area,
            by: lowest.rect.y - pitch,
        },
        "exactly one row"
    );
    assert_eq!(
        cursor.focus,
        Some(lowest),
        "the focus stays, now on the next row"
    );
}

#[test]
fn x_on_an_item_in_gamepad_mode_opens_its_options_and_split_opens_the_split_box() {
    let state = carrying(4);
    let mut h = Harness::new(Default::default());
    h.input.pad.mode = Some(PadHints::Cursor);
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    // The grid's first tile, right of the pack column.
    let tiles: Vec<_> = snap
        .targets
        .iter()
        .filter(|t| t.clip.is_some() && t.rect.w < 60.0)
        .copied()
        .collect();
    let column = tiles.iter().map(|t| t.rect.x).fold(f32::MAX, f32::min);
    let tile = tiles
        .iter()
        .filter(|t| t.rect.x > column + 50.0)
        .min_by(|a, b| {
            (a.rect.y, a.rect.x)
                .partial_cmp(&(b.rect.y, b.rect.x))
                .unwrap()
        })
        .copied()
        .unwrap();
    let (x, y) = centre(tile.rect);
    h.move_to(x, y);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    let out = h.frame(&state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(&state);
    assert!(out.requests.is_empty(), "no examine: {:?}", out.requests);
    let menu =
        h.ui.windows
            .item_menu
            .clone()
            .expect("the item's options open");
    assert_eq!(menu.item.name, "Thing 0");
    let snap = h.input.nav.last.clone();
    let top = snap.layers.len();
    assert_eq!(snap.layers[top - 1].name, "Item Menu");
    let mut entries: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| t.layer == top && t.rect.w > 100.0)
        .map(|t| t.rect)
        .collect();
    entries.sort_by(|a, b| a.y.total_cmp(&b.y));
    // Use, Give (greyed: nobody selected, but still a place to rest), Drop, Split, Examine,
    // Set as Target, Set to Hotbar; no Equip, as nothing says the thing can be worn.
    assert_eq!(entries.len(), 7);
    assert_eq!(
        snap.home_in(top).map(|t| t.rect),
        Some(entries[0]),
        "the focus starts on Use, the first entry"
    );
    let mut cursor = Cursor::default();
    cursor.focus = Some(snap.home_in(top).unwrap());
    assert_eq!(cursor.step(&snap, Dir::Up), Stepped::Stayed);
    assert_eq!(
        cursor.wrap(&snap, Dir::Up),
        Stepped::Moved,
        "up from the first goes round"
    );
    assert_eq!(
        cursor.focus.map(|t| t.rect),
        Some(entries[6]),
        "to the last"
    );
    // Give, greyed, does nothing.
    click_at(&mut h, &state, centre(entries[1]));
    assert!(
        h.ui.windows.item_menu.is_some(),
        "Give greyed: nothing chosen"
    );
    click_at(&mut h, &state, centre(entries[3]));
    assert!(
        h.ui.windows.item_menu.is_none(),
        "chosen, the options close"
    );
    let (item, amount) = h.ui.windows.split_box.clone().expect("the split box opens");
    assert_eq!((item.name.as_str(), amount), ("Thing 0", 1));
}

fn blip(
    id: u32,
    dx: f32,
    dy: f32,
    kind: dereth_horizon::ui::game::BlipKind,
) -> dereth_horizon::ui::game::Blip {
    dereth_horizon::ui::game::Blip {
        id: dereth_primitives::ObjectId(id),
        dx,
        dy,
        kind,
        colour: 0,
        shape: 0,
        name: format!("Thing {id:x}"),
    }
}

/// A world with a person close behind the player, a chicken ahead and a little to the right,
/// and a gem further ahead.
fn surrounded() -> GameState {
    use dereth_horizon::ui::game::BlipKind as K;
    GameState {
        player_id: Some(dereth_primitives::ObjectId(0x5000_0001)),
        radar_range: 75.0,
        combat_mode: 1,
        targets: vec![
            blip(0x10, 0.0, -3.0, K::Npc),
            blip(0x20, 2.0, 8.0, K::Creature),
            blip(0x30, -1.0, 20.0, K::Item),
        ],
        // On screen: the gem a little left of the middle, the chicken right of it; the person
        // behind is not in view.
        on_screen: [
            (dereth_primitives::ObjectId(0x20), (1200.0, 600.0)),
            (dereth_primitives::ObjectId(0x30), (900.0, 500.0)),
        ]
        .into_iter()
        .collect(),
        ..in_world()
    }
}

#[test]
fn a_with_nothing_selected_takes_the_thing_best_in_line_with_where_the_player_looks() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::input::PadWorld;
    let state = surrounded();
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Use);
    let out = h.frame(&state);
    // The person behind is out of sight; the chicken ahead, a little right and near, beats the
    // gem further ahead.
    assert_eq!(
        out.requests,
        vec![UiRequest::Select(dereth_primitives::ObjectId(0x20))]
    );
}

#[test]
fn a_chicken_close_ahead_is_chosen_over_a_crier_far_off_to_the_side() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::game::BlipKind as K;
    use dereth_horizon::ui::input::PadWorld;
    use dereth_primitives::ObjectId;
    for by_camera in [false, true] {
        let mut state = surrounded();
        state.look_by_camera = by_camera;
        // 15 m straight ahead, and 40 m off at 30 degrees.
        state.targets = vec![
            blip(0x71, 0.0, 15.0, K::Creature),
            blip(0x72, 20.0, 34.6, K::Npc),
        ];
        let mut h = Harness::new(Default::default());
        h.input.pad.world.push(PadWorld::Use);
        assert_eq!(
            h.frame(&state).requests,
            vec![UiRequest::Select(ObjectId(0x71))]
        );
    }
}

#[test]
fn the_alternate_selection_cycles_what_is_in_front_and_only_a_takes_it() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::input::PadWorld;
    let state = surrounded();
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Alternate(1));
    assert!(
        h.frame(&state).requests.is_empty(),
        "the selection stays as it was"
    );
    assert_eq!(
        h.ui.hud.alt,
        Some(dereth_primitives::ObjectId(0x20)),
        "nearest in front"
    );
    h.input.pad.world.push(PadWorld::Alternate(1));
    h.frame(&state);
    assert_eq!(h.ui.hud.alt, Some(dereth_primitives::ObjectId(0x30)));
    h.input.pad.world.push(PadWorld::Alternate(1));
    h.frame(&state);
    assert_eq!(
        h.ui.hud.alt,
        Some(dereth_primitives::ObjectId(0x20)),
        "round again; the person behind is never offered"
    );
    h.input.pad.world.push(PadWorld::TakeAlternate);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::Select(dereth_primitives::ObjectId(0x20))]
    );
    assert_eq!(h.ui.hud.alt, None);
    h.input.pad.world.push(PadWorld::Alternate(-1));
    h.frame(&state);
    h.input.pad.world.push(PadWorld::DropAlternate);
    assert!(
        h.frame(&state).requests.is_empty(),
        "let go, nothing selected"
    );
    assert_eq!(h.ui.hud.alt, None);
}

#[test]
fn the_alternate_selection_goes_across_the_screen_in_every_stance() {
    use dereth_horizon::ui::game::BlipKind as K;
    use dereth_primitives::ObjectId;
    let mut state = surrounded();
    // A gem straight ahead and near: across the screen, not by distance.
    state.targets.push(blip(0x40, 0.0, 4.0, K::Item));
    state.on_screen.insert(ObjectId(0x40), (960.0, 700.0));
    let peace = dereth_horizon::ui::hud::pad::alternates(&state);
    assert_eq!(
        peace,
        [ObjectId(0x30), ObjectId(0x40), ObjectId(0x20)],
        "left to right: the far gem a little left, the near one ahead, the chicken right"
    );
    state.combat_mode = 2;
    let fight = dereth_horizon::ui::hud::pad::alternates(&state);
    assert_eq!(
        fight, peace,
        "the same across the screen in a fighting stance"
    );
}

#[test]
fn up_and_down_with_no_fellowship_select_the_player() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::input::PadWorld;
    let state = surrounded();
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Fellow(1));
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::Select(dereth_primitives::ObjectId(0x5000_0001))]
    );
}

#[test]
fn cancel_on_character_select_brings_the_focus_to_exit_without_pressing_it() {
    // The screen notes Exit as its way out; the pad's cancel moves the focus there and CONFIRM is
    // left to press it.
    let state = GameState {
        connected: true,
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let exit = snap
        .screen_cancel
        .expect("character select notes its way out");
    assert!(
        snap.targets.iter().any(|t| t.rect == exit),
        "and it is a place to rest"
    );
}

#[test]
fn the_spellbook_offers_the_stance_s_controls_beside_its_level_filters_to_the_cross_hotbars() {
    use dereth_horizon::ui::hud::cross::{CrossBind, PowerAct};
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.input.pad.mode = Some(PadHints::Cursor);
    h.ui.windows.open(WindowId::Actions, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let book = snap
        .layers
        .iter()
        .position(|l| l.name == "Spellbook")
        .unwrap()
        + 1;
    // Five tiles of 36 in a row under the details, low in the window.
    let mut tiles: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| {
            t.layer == book && (t.rect.w - 36.0).abs() < 1.0 && (t.rect.h - 36.0).abs() < 1.0
        })
        .map(|t| t.rect)
        .collect();
    tiles.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(tiles.len(), 5, "a stop for each of the five: {tiles:?}");
    assert!(
        tiles.iter().all(|t| (t.y - tiles[0].y).abs() < 1.0),
        "one row"
    );
    // Confirming the third (attack low) brings up the bind-to-hotbar notice with it.
    click_at(&mut h, &state, centre(tiles[2]));
    assert_eq!(
        h.ui.hud.cross.binding,
        Some(CrossBind::Power(PowerAct::Low))
    );
}

/// A character with two attributes, one it can raise and one it cannot afford.
fn with_attributes() -> GameState {
    use dereth_horizon::ui::game::StatRow;
    let row = |name: &str, wire: u32, cost: u32| StatRow {
        name: name.into(),
        wire,
        vital: false,
        shown: "10".into(),
        colour: 0,
        cost,
        cost_10: cost * 10,
    };
    GameState {
        stats: vec![row("Strength", 1, 100), row("Endurance", 2, 5_000)],
        unassigned_xp: 1_000,
        ..in_world()
    }
}

#[test]
fn the_attributes_are_a_page_of_rows_each_with_its_buttons_even_those_it_cannot_afford() {
    let state = with_attributes();
    let mut h = Harness::new(Default::default());
    h.input.pad.mode = Some(PadHints::Cursor);
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let mut cursor = Cursor::default();
    cursor.follow(&snap);
    // From the paper doll's side, right onto the page's first row.
    let start = snap
        .targets
        .iter()
        .filter(|t| snap.list_of(t).is_some())
        .min_by(|a, b| {
            (a.rect.y, a.rect.x)
                .partial_cmp(&(b.rect.y, b.rect.x))
                .unwrap()
        })
        .copied()
        .expect("the attributes' rows are stops");
    cursor.focus = Some(start);
    assert_eq!(cursor.step(&snap, Dir::Right), Stepped::Moved);
    let plus1 = cursor.focus.unwrap();
    assert!(plus1.rect.x > start.rect.right(), "+1, right of the row");
    assert_eq!(cursor.step(&snap, Dir::Right), Stepped::Moved);
    let plus10 = cursor.focus.unwrap();
    assert!(plus10.rect.x > plus1.rect.x, "+10");
    assert_eq!(cursor.step(&snap, Dir::Right), Stepped::Stayed);
    assert_eq!(cursor.step(&snap, Dir::Down), Stepped::Moved);
    let below = cursor.focus.unwrap();
    assert_eq!(
        below.rect.x, plus10.rect.x,
        "Endurance's +10, unaffordable but a stop"
    );
    assert_eq!(cursor.step(&snap, Dir::Left), Stepped::Moved);
    assert_eq!(cursor.step(&snap, Dir::Left), Stepped::Moved);
    assert_eq!(
        cursor.focus.map(|t| t.rect.x),
        Some(start.rect.x),
        "back on the row"
    );
}

#[test]
fn the_alternate_selection_follows_where_things_are_drawn_not_their_bearing_from_the_player() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::game::BlipKind as K;
    use dereth_horizon::ui::input::PadWorld;
    use dereth_primitives::ObjectId;
    // A servant close beside the player on the left (from the player, almost square to the
    // side), a slinker far off ahead-left (from the player, nearly ahead), a vat near and a corpse
    // ahead. The camera behind the player draws the slinker left of the servant.
    let mut state = surrounded();
    state.targets = vec![
        blip(0x51, -3.0, 0.5, K::Creature),
        blip(0x52, -6.0, 15.0, K::Creature),
        blip(0x53, 1.0, 2.0, K::Item),
        blip(0x54, 0.0, 4.0, K::Item),
    ];
    state.on_screen = [
        (ObjectId(0x51), (500.0, 700.0)),
        (ObjectId(0x52), (300.0, 420.0)),
        (ObjectId(0x53), (1100.0, 800.0)),
        (ObjectId(0x54), (990.0, 760.0)),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        dereth_horizon::ui::hud::pad::alternates(&state),
        [
            ObjectId(0x52),
            ObjectId(0x51),
            ObjectId(0x54),
            ObjectId(0x53)
        ]
    );
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Alternate(-1));
    h.frame(&state);
    assert_eq!(
        h.ui.hud.alt,
        Some(ObjectId(0x51)),
        "the first left of the middle"
    );
    h.input.pad.world.push(PadWorld::Alternate(1));
    h.frame(&state);
    assert_eq!(
        h.ui.hud.alt,
        Some(ObjectId(0x54)),
        "right of the servant: the corpse ahead"
    );
    h.input.pad.world.push(PadWorld::TakeAlternate);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::Select(ObjectId(0x54))]
    );
}

#[test]
fn a_with_nothing_selected_takes_what_is_best_in_line_with_the_facing_and_only_a_thing_seen() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::game::BlipKind as K;
    use dereth_horizon::ui::input::PadWorld;
    use dereth_primitives::ObjectId;
    let mut state = surrounded();
    // A chest 10 m straight ahead (score 20), a gem 2 m off 45 degrees (score 45 + 2.8).
    state.targets = vec![
        blip(0x61, 0.0, 10.0, K::Item),
        blip(0x62, 1.4, 1.4, K::Item),
    ];
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Use);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::Select(ObjectId(0x61))]
    );
    // The chest behind a wall: not seen, so the gem.
    state.in_sight = Some([ObjectId(0x62)].into_iter().collect());
    h.input.pad.world.push(PadWorld::Use);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::Select(ObjectId(0x62))]
    );
}

#[test]
fn a_with_a_use_armed_puts_it_on_what_is_in_front_and_b_puts_it_down() {
    use dereth_client_contract::UiRequest;
    use dereth_horizon::ui::input::PadWorld;
    let mut state = surrounded();
    state.targeting = true;
    state.armed = Some(dereth_primitives::ObjectId(0x8000_0001));
    let mut h = Harness::new(Default::default());
    h.input.pad.world.push(PadWorld::Use);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::ExecuteTargetItem(dereth_primitives::ObjectId(
            0x20
        ))],
        "the thing in front"
    );
    h.input.pad.world.push(PadWorld::Deselect);
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::SetTargetMode(
            dereth_client_contract::view::TargetMode::None
        )]
    );
}

#[test]
fn b_over_the_interface_with_a_use_armed_puts_it_down_and_leaves_the_focus() {
    use dereth_client_contract::UiRequest;
    let mut state = in_world();
    state.targeting = true;
    let mut h = Harness::new(Default::default());
    h.input.pad.cancel_targeting = true;
    assert_eq!(
        h.frame(&state).requests,
        vec![UiRequest::SetTargetMode(
            dereth_client_contract::view::TargetMode::None
        )]
    );
}
