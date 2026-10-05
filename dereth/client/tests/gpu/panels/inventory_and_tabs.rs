//! The in-game panels: a tabbed page shows exactly one sub-panel, tab clicks swap it and closing
//! the panel takes every sub-panel down; the inventory panel's item lists create their slot
//! elements, the backpack grid, side-pack strip and paper doll populate from the player
//! description, a drop on an item list becomes a container move that leaves the item in place and
//! ghosted, a side-pack click points the grid at that pack, and the filled grid changes pixels only
//! inside its own box.
//! Fixture: the retail dats (`classic_gameplay` layout), the `first-login-walk-jump` recording's
//! `0x0013 Login_PlayerDescription` as the oracle, and headless Apps rendered through the selected
//! GPU backend; clicks are element messages broadcast through `UiSystem`, not injected input.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use dereth_ui_screens::view::{DropTarget, UiRequest};

// ---------------------------------------------------------------------------------------------
// Harness — the same one `gameplay_hud.rs` uses.
// ---------------------------------------------------------------------------------------------

/// **An `expect`, never a skip.** A test that returns early is counted as a pass and
/// is invisible in the summary line; if the retail dats are not where `$DERETH_TEST_DAT_DIR` says,
/// the run is not a pass.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn app_in_gameplay(ui: bool, frames: u32) -> Option<App> {
    let cfg = Config {
        ui,
        ..base_config()
    };
    let mut app = App::new(cfg).unwrap_or_else(|e| panic!("a headless App: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the headless App's UI shell: {e}"));
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s)
        .unwrap_or_else(|e| panic!("the static scene: {e}"));
    if ui {
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    }
    for _ in 0..frames {
        app.frame();
    }
    Some(app)
}

fn gameplay_screen(app: &mut App) -> Option<(&mut UiSystem, &mut GamePlayScreen)> {
    let shell = app.ui_mut()?;
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut()?;
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any.downcast_mut::<GamePlayScreen>()?;
    Some((ui, screen))
}

fn root_of(ui: &UiSystem) -> Option<ElemHandle> {
    ui.get_element(ElementId(0x1000_0495))
}

fn visible(ui: &UiSystem, root: ElemHandle, id: u32) -> Option<bool> {
    let h = ui.get_child_recursive(root, ElementId(id))?;
    Some(ui.node(h)?.region.flags.visible)
}

// ---------------------------------------------------------------------------------------------
// The capture corpus.
// ---------------------------------------------------------------------------------------------

fn replay(session: &str) -> Option<Vec<SessionEvent>> {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    Some(events)
}

fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen_desc = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture never reached 0x0013");
    let end = events[seen_desc..]
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(events.len(), |i| seen_desc + i);
    &events[..end]
}

/// The capture's own `0x0013`, which is the only oracle for what is in the player's pack.
fn player_description(
    events: &[SessionEvent],
) -> Option<&dereth_protocol::login::LoginPlayerDescription> {
    events.iter().find_map(|e| match e {
        SessionEvent::PlayerDescription(d) => Some(&**d),
        _ => None,
    })
}

/// An application in the game phase with a recorded session's `0x0013` applied.
fn app_with_capture(session: &str) -> Option<(App, Vec<SessionEvent>)> {
    let events = replay(session)?;
    let mut app = app_in_gameplay(true, 4)?;
    let _ = app.apply_hud_events(events_in_world(&events));
    for _ in 0..4 {
        app.frame();
    }
    Some((app, events))
}

// ---------------------------------------------------------------------------------------------
// 1. The tabbed pages — one sub-panel at a time.
// ---------------------------------------------------------------------------------------------

/// The six panel pages of the panel stack and their sub-panels, from the shipped
/// layout. The tab and page ids are **not** written here — they are read off attribute `0x2E` at
/// run time, exactly as tab-page setup does.
const TAB_PAGES: [u32; 6] = [
    0x1000_018C,
    0x1000_018D,
    0x1000_018E,
    0x1000_0559,
    0x1000_018F,
    0x1000_0190,
];

/// Behaviour: panels.tabs.a-tabbed-page-shows-one-sub-panel
///
/// **Tabs do not stack on top of one another.**
///
/// Panel setup hides the stack's sixteen pages. Six of those sixteen are tabbed panels — the map,
/// options, character, quest, social and spell pages — and each holds two to four **sub-panels
/// sharing one rectangle**, all of them marked visible by `classic_gameplay`. Tab-page setup reads
/// attribute `0x2E UICore_Panel_pages`, and its update shows exactly the one whose entry carries
/// `0x32 UICore_Panel_page_open`; without `add_tab` populating that table every sub-panel draws at
/// once.
///
/// The assertion is on the live tree, and it names no page id: the pairs come from the layout.
#[test]
fn a_tabbed_page_shows_exactly_one_of_its_sub_panels() {
    have_dats();
    let mut app =
        app_in_gameplay(true, 4).expect("an app in gameplay: retail dats and a WARP device");
    let Some((ui, _screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");

    let mut checked = 0;
    for page in TAB_PAGES {
        let h = ui
            .get_child_recursive(root, ElementId(page))
            .unwrap_or_else(|| panic!("page {page:#010X} is not in classic_gameplay"));
        // The tab table the widget built from the layout.
        let pairs = {
            let n = ui.node(h).expect("live");
            let b = n.behaviour.as_ref().expect("a behaviour");
            let p = (**b)
                .as_any()
                .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
                .unwrap_or_else(|| panic!("page {page:#010X} is not a panel element"));
            (p.tab_to_page.clone(), p.open_page)
        };
        let (tab_to_page, open) = pairs;
        assert!(
            tab_to_page.len() >= 2,
            "page {page:#010X} has {} tabs",
            tab_to_page.len()
        );
        assert!(open.is_some(), "page {page:#010X} opened no tab");

        // Every sub-panel except the open one is down. This is the assertion the shipped layout
        // fails without the tab-page setup: all of them are authored visible.
        for sub in tab_to_page.values() {
            let up = visible(ui, root, sub.0).unwrap_or_else(|| panic!("{sub:?} missing"));
            assert!(
                !up,
                "sub-panel {:#010X} of page {page:#010X} is stacked on top",
                sub.0
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 6, "all six tabbed pages were checked");
}

/// Open and close are consistent.
///
/// Opening a panel shows its default sub-panel and nothing else; clicking a tab swaps to that
/// sub-panel and nothing else; closing the panel takes every sub-panel down, so re-opening cannot
/// show two. Those are the panel message handler's three arms: the `0x19`/`0x29` tab
/// click, and the `0x18` on the panel itself with its first parameter clear and set.
#[test]
fn clicking_a_tab_swaps_the_page_and_closing_the_panel_takes_them_all_down() {
    have_dats();
    let mut app =
        app_in_gameplay(true, 4).expect("an app in gameplay: retail dats and a WARP device");

    // The character page's own tab table, read off the tree rather than written here.
    let (page_id, panel_id, tabs) = {
        let Some((ui, screen)) = gameplay_screen(&mut app) else {
            panic!("no gameplay screen")
        };
        let root = root_of(ui).expect("root");
        let page = ElementId(0x1000_018E);
        let h = ui
            .get_child_recursive(root, page)
            .expect("the character page");
        let n = ui.node(h).expect("live");
        let p = (**n.behaviour.as_ref().expect("behaviour"))
            .as_any()
            .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
            .expect("panel element");
        let tabs: Vec<(ElementId, ElementId)> =
            p.tab_to_page.iter().map(|(a, b)| (*a, *b)).collect();
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|q| q.element == page)
            .map(|q| q.panel_id)
            .expect("the character page carries a panel id");
        (page, panel_id, tabs)
    };
    assert!(
        tabs.len() >= 3,
        "the character page has {} tabs",
        tabs.len()
    );

    // Open the page through the panel stack, the way the toolbar button does.
    {
        let Some((ui, screen)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        screen.recv_set_panel_visibility(ui, panel_id, true);
    }
    app.frame();
    {
        let Some((ui, _)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        let root = root_of(ui).expect("root");
        assert_eq!(visible(ui, root, page_id.0), Some(true), "the page is up");
        let up: Vec<u32> = tabs
            .iter()
            .filter(|(_, p)| visible(ui, root, p.0) == Some(true))
            .map(|(_, p)| p.0)
            .collect();
        assert_eq!(up.len(), 1, "exactly one sub-panel is up, got {up:?}");
    }

    // Click each tab in turn. Element message 0x19 is what the tab-message handler switches on.
    for (tab, want) in tabs.clone() {
        {
            let Some((ui, _)) = gameplay_screen(&mut app) else {
                unreachable!()
            };
            let root = root_of(ui).expect("root");
            let h = ui.get_child_recursive(root, tab).expect("the tab element");
            ui.broadcast_element_message(h, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
        }
        app.frame();
        let Some((ui, _)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        let root = root_of(ui).expect("root");
        let up: Vec<u32> = tabs
            .iter()
            .filter(|(_, p)| visible(ui, root, p.0) == Some(true))
            .map(|(_, p)| p.0)
            .collect();
        assert_eq!(
            up,
            vec![want.0],
            "clicking tab {:#010X} must show only {:#010X}",
            tab.0,
            want.0
        );
    }

    // Close the panel: every sub-panel goes down with it.
    {
        let Some((ui, screen)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        screen.recv_set_panel_visibility(ui, panel_id, false);
    }
    app.frame();
    let Some((ui, _)) = gameplay_screen(&mut app) else {
        unreachable!()
    };
    let root = root_of(ui).expect("root");
    assert_eq!(
        visible(ui, root, page_id.0),
        Some(false),
        "the page is down"
    );
    for (_, p) in &tabs {
        assert_eq!(
            visible(ui, root, p.0),
            Some(false),
            "sub-panel {:#010X} survived the close",
            p.0
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The item lists exist as elements at all.
// ---------------------------------------------------------------------------------------------

/// **An item list has live item-slot elements**; without them it has no children and draws
/// nothing whatever is in the pack.
///
/// Item-list initialization creates `UI_ItemList_FixedListSize` slots (or one, when that
/// attribute is `-1`) and builds each from the **`ItemSlot` layout**, enum
/// `0x10000038`, with the per-list element id in `0x1000000E UI_ItemList_ItemSlotID`. The numbers
/// asserted here are the shipped layout's own.
#[test]
fn every_inventory_item_list_has_created_its_slot_elements() {
    have_dats();
    let mut app =
        app_in_gameplay(true, 4).expect("an app in gameplay: retail dats and a WARP device");
    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let inv = &screen.inventory;

    let top = inv
        .top_container
        .as_ref()
        .expect("the backpack's top-container field 0x100001C9");
    let containers = inv
        .container_list
        .as_ref()
        .expect("the backpack container list 0x100001CA");
    let items = inv
        .item_list
        .as_ref()
        .expect("the item-grid list 0x100001C6");

    // Each list's slot count is its own `UI_ItemList_FixedListSize`.
    assert_eq!(top.slots.len(), 1, "the main-pack strip is one slot");
    assert_eq!(containers.slots.len(), 7, "the side-pack strip is seven");
    assert_eq!(items.slots.len(), 102, "the item grid is 102");
    assert_eq!(
        inv.doll.len(),
        24,
        "the paper doll's twenty-four one-slot lists"
    );

    // Not one slot creation failed: a miss means the `ItemSlot` layout did not resolve, and
    // the whole panel would be silently empty.
    for w in [top, containers, items] {
        assert_eq!(
            w.create_failures, 0,
            "list {:#010X} could not build a slot",
            w.element.0
        );
        assert!(
            w.cell.0 > 0 && w.cell.1 > 0,
            "list {:#010X} has no cell size",
            w.element.0
        );
    }
    assert!(
        inv.slots_created >= 134,
        "only {} slots created",
        inv.slots_created
    );

    // And each slot really is a live item-slot element with the icon child its state update controls.
    for w in [top, containers, items] {
        for s in &w.slots {
            assert!(
                s.icon.is_some(),
                "a slot of {:#010X} has no 0x1000033B icon",
                w.element.0
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 3. It populates from the object tables, and the tables come from the capture.
// ---------------------------------------------------------------------------------------------

/// Behaviour: inventory.panel.the-backpack-and-doll-hold-the-captures-items
///
/// The backpack shows the items the recorded server said the player was carrying.
///
/// **The oracle is `fixtures/packet-captures/first-login-walk-jump`'s own `0x0013 Login_PlayerDescription`**, whose
/// `content_profiles` is the only message that ever names the contents of the player's pack: the
/// individual `0xF745` creates that follow carry each item's container id and never the list. The
/// expected ids are read out of that message here and compared against what the grid holds; nothing
/// in this test chooses a value.
///
/// Player-description handling ends by viewing the player's object contents and updating object
/// inventory, so both decoded lists must reach the panel.
#[test]
fn the_backpack_holds_the_items_the_captures_own_0x0013_named() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let d = player_description(&events).expect("the capture's 0x0013");
    let expected: Vec<ObjectId> = d
        .content_profiles
        .iter()
        .filter(|p| p.container_properties == 0)
        .map(|p| p.iid)
        .collect();
    let expected_packs: Vec<ObjectId> = d
        .content_profiles
        .iter()
        .filter(|p| p.container_properties != 0)
        .map(|p| p.iid)
        .collect();
    assert!(
        !expected.is_empty(),
        "the capture's character was carrying nothing; this test needs a session with items"
    );

    let counters = app.hud().stats;
    assert_eq!(
        counters.content_profiles,
        u64::try_from(d.content_profiles.len()).unwrap(),
        "every ContentProfile the capture carried was applied"
    );

    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let grid = screen.inventory.item_list.as_ref().expect("the item grid");
    let got: Vec<ObjectId> = grid.slots.iter().filter_map(|s| s.item).collect();
    assert_eq!(
        got, expected,
        "the grid holds the capture's own loose items, in its own order"
    );

    let strip = screen
        .inventory
        .container_list
        .as_ref()
        .expect("the side-pack strip");
    let packs: Vec<ObjectId> = strip.slots.iter().filter_map(|s| s.item).collect();
    assert_eq!(
        packs, expected_packs,
        "the side-pack strip holds the capture's own containers"
    );

    // Player-description handling adds the player id to the top-container list, so the
    // main-pack strip holds exactly that one entry.
    let top = screen
        .inventory
        .top_container
        .as_ref()
        .expect("the main-pack strip");
    assert_eq!(top.slots.len(), 1);
    assert!(top.slots[0].item.is_some(), "the main pack shows no player");
}

/// The equipped gear, from the same message's `inventory_placements`.
///
/// Paper-doll rebuilding clears every slot with a zero item and location mask
/// `0x7FFFFFFF`, then handles each placement in turn by testing its location against each slot's
/// own mask. The slot masks come from the paper-doll lookup table and the placements come
/// from the capture, so this test writes neither.
#[test]
fn the_paper_doll_holds_the_equipment_the_capture_named() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let d = player_description(&events).expect("the capture's 0x0013");
    assert!(
        !d.inventory_placements.is_empty(),
        "the capture's character was wearing nothing; this test needs a dressed session"
    );

    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let doll = &screen.inventory.doll;
    assert_eq!(doll.len(), 24);

    // Every placement the capture carried lands in every slot whose mask it intersects, and in no
    // other. That is the whole of paper-doll placement.
    let mut placed = 0;
    for (mask, w) in doll {
        let want: Option<ObjectId> = d
            .inventory_placements
            .iter()
            .filter(|p| p.location & mask != 0)
            .map(|p| p.iid)
            .next_back();
        let got = w.item_at(0);
        assert_eq!(
            got, want,
            "doll slot {:#010X} (mask {mask:#X})",
            w.element.0
        );
        if want.is_some() {
            placed += 1;
        }
    }
    assert!(
        placed > 0,
        "not one of the capture's placements reached a doll slot"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. …and sends its requests.
// ---------------------------------------------------------------------------------------------

/// A drop on an item list carries a *list element id and a slot index*, and only the panel that
/// filled the list knows which container object that list is showing;
/// `GamePlayScreen::accept_drag_object` resolves it here, where the map is.
///
/// Item-list drop handling reads its message's first parameter as the drag record and takes the
/// drag's **owner** from it, which is why `dereth_ui::focus` now puts the owner in `p2`.
///
/// **Nothing moves.** The assertion is that the item is still where it was and its icon is ghosted
/// by setting its waiting state: the client predicts no move.
#[test]
fn a_drop_on_an_item_list_becomes_a_container_move_and_ghosts_the_icon() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let d = player_description(&events).expect("the capture's 0x0013");
    let item = d
        .content_profiles
        .iter()
        .find(|p| p.container_properties == 0)
        .map(|p| p.iid)
        .expect("the capture's character was carrying nothing");
    let pack = d
        .content_profiles
        .iter()
        .find(|p| p.container_properties != 0)
        .map(|p| p.iid)
        .expect("the capture's character had no side pack to drop into");

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let (owner, target, before) = {
        let Some((ui, screen)) = gameplay_screen(&mut app) else {
            panic!("no gameplay screen")
        };
        let grid = screen.inventory.item_list.as_ref().expect("the grid");
        let strip = screen.inventory.container_list.as_ref().expect("the strip");
        let src = grid
            .slots
            .iter()
            .find(|s| s.item == Some(item))
            .expect("the item is in the grid")
            .handle;
        // The drop catcher is the **list**, not the slot: only the list carries attribute 0x36 in
        // the shipped data, so catcher lookup walks up to it.
        let before = grid.slots.iter().filter_map(|s| s.item).count();
        let strip = strip.handle;
        // **The ghost goes on here, at the pick-up.** Begin-drag ghosts the icon before handing
        // it to drag-and-drop. Drop release never adds that state; its only waiting-state write
        // takes the ghost **off** when the drop-acceptance check refuses and skips the write on
        // an accept. `ItemListWidget::ghost` is the same call `begin_drag` makes, so what the
        // assertion claims is: **an accepted drop leaves the pick-up ghost alone.**
        screen
            .inventory
            .item_list
            .as_mut()
            .expect("the grid")
            .ghost(ui, item);
        (src, strip, before)
    };

    let r = {
        let Some((ui, screen)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        screen.handle_drop_release(ui, target, owner)
    };
    // The strip's slot 0 holds the player's first side pack, so the drop resolves to that object.
    //
    // The resolved target keeps the slot index, because accept-drag computes the placement from
    // it (a bare container target would send every drag into a pack with `place = 0`). The pack
    // is the destination (`under` names it, and a nonzero item capacity re-aims the move).
    match r {
        Some(UiRequest::DragDrop {
            item: i,
            target: DropTarget::ItemListSlot { under, index, .. },
        }) => {
            assert_eq!(i, item, "the item that was dragged");
            assert_eq!(
                under,
                Some(pack),
                "the list element resolves to the container it shows"
            );
            assert_eq!(index, 0, "the strip's slot 0");
        }
        ref other => panic!("expected a resolved item-list drop; got {other:?}"),
    }
    let queued = app.ui_mut().expect("the UI shell is up").ui.requests.take();
    assert!(
        queued.contains(&r.clone().unwrap()),
        "the request reached the outbox: {queued:?}"
    );

    // Nothing moved, and the icon is ghosted instead.
    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        unreachable!()
    };
    let grid = screen.inventory.item_list.as_ref().expect("the grid");
    assert_eq!(
        grid.slots.iter().filter_map(|s| s.item).count(),
        before,
        "the client predicts nothing: the grid still holds every item"
    );
    let s = grid
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .expect("still there");
    assert!(
        s.waiting,
        "the source icon must be ghosted while the server has not answered"
    );
}

/// The same drop routed through `resolve_drop` alone, which is the map itself.
#[test]
fn an_item_list_element_resolves_to_the_container_it_is_showing() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let d = player_description(&events).expect("the capture's 0x0013");
    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let inv = &screen.inventory;

    // The grid is showing the player's own pack, so a drop on one of its **empty** slots is a drop
    // on the player.
    let grid = inv.item_list.as_ref().expect("the grid");
    let empty = grid
        .slots
        .iter()
        .position(|s| s.item.is_none())
        .expect("the 102-slot grid has an empty slot");
    let target = DropTarget::ItemList {
        list: grid.element,
        slot: u32::try_from(empty).unwrap(),
    };
    assert_eq!(
        inv.resolve_drop(target),
        grid.parent_container,
        "an empty slot is its container"
    );
    assert!(
        grid.parent_container.is_some(),
        "the grid never took a parent container"
    );

    // A drop on an **occupied** slot is a drop on that object — a merge or a put-in-container.
    if let Some(first) = d
        .content_profiles
        .iter()
        .find(|p| p.container_properties == 0)
    {
        let at = grid
            .slots
            .iter()
            .position(|s| s.item == Some(first.iid))
            .expect("in the grid");
        let target = DropTarget::ItemList {
            list: grid.element,
            slot: u32::try_from(at).unwrap(),
        };
        assert_eq!(inv.resolve_drop(target), Some(first.iid));
    }

    // A toolbar shortcut slot is deliberately not resolved here (the quickbar handles it).
    assert_eq!(inv.resolve_drop(DropTarget::ShortcutSlot(0)), None);
}

/// Clicking a side pack in the container strip puts **that pack's** contents in the grid.
///
/// This follows the child-list join: both container strips drive the one grid. The grid
/// stops showing the player's own loose items and shows the pack's. What the pack holds comes from
/// the capture too — the `0x0013` names the
/// pack, and the server's contents message for it is what would fill it, so a pack whose
/// contents this session never saw shows **empty**, which is still the right answer and is what is
/// asserted: the grid changed and it changed to the pack.
#[test]
fn clicking_a_side_pack_points_the_grid_at_it() {
    have_dats();
    let (mut app, events) =
        app_with_capture("first-login-walk-jump").expect("an app driven from the capture corpus");
    let d = player_description(&events).expect("the capture's 0x0013");
    let pack = d
        .content_profiles
        .iter()
        .find(|p| p.container_properties != 0)
        .map(|p| p.iid)
        .expect("the capture's character had no side pack");

    let (slot, before) = {
        let Some((_ui, screen)) = gameplay_screen(&mut app) else {
            panic!("no gameplay screen")
        };
        let strip = screen.inventory.container_list.as_ref().expect("the strip");
        let slot = strip
            .slots
            .iter()
            .find(|s| s.item == Some(pack))
            .expect("the pack is in the strip")
            .handle;
        (slot, screen.inventory.open_container)
    };
    assert_ne!(before, Some(pack), "the pack is not already the open one");

    {
        let Some((ui, _)) = gameplay_screen(&mut app) else {
            unreachable!()
        };
        // The message is `0x1C MOUSE_PRESS` with the input action in `p1`, not `0x19
        // MOUSE_CLICK`: the item-list message handler has exactly two arms, message `0x1C` and
        // message `0x15`. The `0x1C` arm switches on the message's first parameter, the input
        // action, and case 7 opens the container.
        ui.broadcast_element_message(
            slot,
            dereth_ui::msg::element::id::MOUSE_PRESS,
            dereth_ui::focus::action::PRIMARY_CLICK,
            0,
        );
    }
    app.frame();

    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        unreachable!()
    };
    assert_eq!(
        screen.inventory.open_container,
        Some(pack),
        "the grid follows the click"
    );
    let grid = screen.inventory.item_list.as_ref().expect("the grid");
    let got: Vec<ObjectId> = grid.slots.iter().filter_map(|s| s.item).collect();
    // The client predicts nothing here either: the grid holds exactly what the object tables say
    // that pack contains, which for a pack the session never opened is nothing.
    assert!(
        got.iter().all(|id| *id != pack),
        "the pack must not appear inside itself: {got:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. It draws, and only where it said it would.
// ---------------------------------------------------------------------------------------------

/// Two renders of **one** application differing only in whether the item grid holds its items,
/// with every changed pixel inside the grid's own rectangle and zero outside.
///
/// The two runs are the same application, the same capture, the same open panel and the same
/// frame; between them the grid is flushed, returning every slot to empty state `0x1000001C`.
/// Nothing else on the frame can move, which is what
/// makes "zero pixels outside" a real statement: a control that simply skipped the capture would
/// also change the vitals, the radar and the doll.
///
/// If the item icons were not actually rasterised the differential would be zero pixels; if they
/// were drawn outside their list the outside count would not be.
#[test]
fn the_filled_backpack_changes_pixels_and_only_inside_the_item_grids_box() {
    have_dats();
    let events = replay("first-login-walk-jump").expect("the capture corpus session");

    // Two applications with the *same* history — same capture, same open panel, same frame count —
    // differing only in one item-list flush before the last frame. A control that skipped the
    // capture instead would also move the vitals, the radar and the doll, and the frame is not
    // idempotent either (the day cycle and the particles advance), so both arms must be stepped
    // the same number of times.
    let shot = |flush: bool| -> Option<(u32, u32, Vec<u8>, dereth_ui::Box2D, usize)> {
        let mut app = app_in_gameplay(true, 4)?;
        let _ = app.apply_hud_events(events_in_world(&events));
        {
            let (ui, screen) = gameplay_screen(&mut app)?;
            let page = screen
                .panels
                .pages
                .iter()
                .find(|p| p.element == window::INVENTORY_PAGE)
                .copied()?;
            screen.recv_set_panel_visibility(ui, page.panel_id, true);
        }
        for _ in 0..6 {
            app.frame();
        }
        let (grid, filled) = {
            let (ui, screen) = gameplay_screen(&mut app)?;
            let w = screen.inventory.item_list.as_ref()?;
            (
                ui.screen_box(w.handle),
                w.slots.iter().filter(|s| s.item.is_some()).count(),
            )
        };
        if flush {
            // The flush returns every slot to empty state `0x1000001C`. `update_inventory` guards
            // on its own snapshot, which has not changed, so it does not refill behind the flush.
            let (ui, screen) = gameplay_screen(&mut app)?;
            screen.inventory.item_list.as_mut()?.flush(ui);
        }
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, grid, filled))
    };

    let (w, h, full, grid, filled_slots) =
        shot(false).expect("a rendered frame: retail dats and a WARP device");
    let (w2, h2, empty, _, _) =
        shot(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));
    assert!(
        filled_slots > 0,
        "the capture's items never reached the grid"
    );
    assert!(
        grid.width() > 0 && grid.height() > 0,
        "the grid has no screen rectangle: {grid:?}"
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if empty[i..i + 4] == full[i..i + 4] {
                continue;
            }
            if x >= grid.x0 && x <= grid.x1 && y >= grid.y0 && y <= grid.y1 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "filling the backpack changed no pixels at all");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the item grid's box {grid:?}"
    );
}
