//! The quickbar shows its nine numbered slots and the items in them: eighteen shortcut lists each
//! build one item slot from their own `ItemSlot` root, bank 1's nine empty-state images are nine
//! distinct numerals (the numbered tile is the empty-slot frame, layout data rather than a number
//! renderer) while bank 2 falls below the toolbar window, a shortcut lands in its slot with its own
//! icon, the numeral comes from the plain or ghosted array by stance (the shipped data has no
//! empty-numeral array; an empty slot hides the numeral), use is Use and secondary use is Select,
//! a drop anywhere inside a slot resolves to that slot, and the tiles and icons change pixels only
//! inside their own boxes.
//! Fixture: the retail dats, the `first-login-walk-jump` recording replayed offline (its object
//! ids and icons seed three synthesized shortcuts), and headless Apps on a software GPU device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use std::collections::BTreeSet;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::login::ShortCutData;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use dereth_ui_screens::toolbar::shortcuts::{slot_element, SLOT_COUNT};
use dereth_ui_screens::view::{DropTarget, UiRequest};

// ---------------------------------------------------------------------------------------------
// Harness — the same headless gameplay setup used by the panel tests.
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

fn app_in_gameplay(frames: u32) -> Option<App> {
    let cfg = Config {
        ui: true,
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
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
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

// ---------------------------------------------------------------------------------------------
// Local capture replay following the panel-test pattern.
// ---------------------------------------------------------------------------------------------

/// The replay, plus **every object the capture ever created**, by id, icon and name.
///
/// The roster has to be accumulated as the replay runs rather than read off the finished stream:
/// The retained session ends with a log-off, whose release clears the whole object table, so the
/// tables are empty by the last datagram. That is also why the populated-scene test measures its
/// creation counter instead of the final size.
type CapturedObject = (ObjectId, u32, String);

fn replay_full(session: &str) -> Option<(Vec<SessionEvent>, Vec<CapturedObject>)> {
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
    let mut seen: std::collections::BTreeMap<ObjectId, (u32, String)> =
        std::collections::BTreeMap::new();
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
        for (id, w) in objects.world.tables.weenies.iter() {
            seen.entry(id)
                .or_insert_with(|| (w.pwd.icon_id, w.pwd.name.clone()));
        }
    }
    Some((
        events,
        seen.into_iter().map(|(id, (i, n))| (id, i, n)).collect(),
    ))
}

/// The events only, for a test that needs no object tables.
fn replay(session: &str) -> Option<Vec<SessionEvent>> {
    replay_full(session).map(|(e, _)| e)
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

fn player_description(
    events: &[SessionEvent],
) -> Option<&dereth_protocol::login::LoginPlayerDescription> {
    events.iter().find_map(|e| match e {
        SessionEvent::PlayerDescription(d) => Some(&**d),
        _ => None,
    })
}

// ---------------------------------------------------------------------------------------------
// 1. The bar exists at all — eighteen lists, eighteen slot elements, nine numbered tiles.
// ---------------------------------------------------------------------------------------------

/// Gameplay-screen initialization built the shortcut array, and every list built its slot.
///
/// The counts are the point: a quickbar whose item lists create no children draws *nothing*.
/// Eighteen lists × one `UI_ItemList_FixedListSize` = eighteen item-slot elements and zero
/// failures, stated as arithmetic rather than as a look at a screenshot.
///
/// The per-slot `ItemSlot` root ids are **read off the live tree** (attribute `0x1000000E`) and
/// only then compared with the two documented banks, so this fails if the layout ever moves them.
///
/// Falsified by deleting the `init_shortcut_array` call from `GamePlayScreen::post_init`: `created`
/// goes to 0 and every assertion below the first fails.
#[test]
fn init_shortcut_array_builds_eighteen_slots_from_eighteen_per_slot_item_slot_roots() {
    have_dats();
    let mut app =
        app_in_gameplay(4).expect("an app in gameplay: retail dats and a software GPU device");
    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");

    assert_eq!(
        screen.shortcuts.slots.len(),
        SLOT_COUNT as usize,
        "eighteen shortcut lists"
    );
    assert_eq!(
        screen.shortcuts.created, SLOT_COUNT,
        "eighteen item-slot elements were created"
    );
    assert_eq!(
        screen.shortcuts.create_failures, 0,
        "every ItemSlot layout resolved"
    );

    // The eighteen list element ids, in the shortcut-array initialization order.
    for (i, w) in screen.shortcuts.slots.iter().enumerate() {
        let slot = u32::try_from(i).unwrap();
        assert_eq!(
            w.element,
            slot_element(slot).unwrap(),
            "list element for slot {slot}"
        );
        assert_eq!(w.slots.len(), 1, "slot {slot} is FixedListSize = 1");
        assert!(
            w.shortcut_list,
            "slot {slot} carries UI_ItemList_IsShortcut"
        );
        assert_eq!(w.fixed_list_size, 1, "slot {slot}");
    }

    // `UI_ItemList_ItemSlotID`, read off the live tree, against the two banks.
    let roots: Vec<u32> = screen.shortcuts.slots.iter().map(|w| w.slot_id.0).collect();
    let bank1: Vec<u32> = (0..9).map(|i| 0x1000_043B + i).collect();
    let bank2: Vec<u32> = (0..9).map(|i| 0x1000_06C1 + i).collect();
    assert_eq!(&roots[..9], &bank1[..], "bank 1 slot roots");
    assert_eq!(&roots[9..], &bank2[..], "bank 2 slot roots");
    // Every one of the eighteen is its own root: a shared root would give one shared numeral.
    assert_eq!(roots.iter().copied().collect::<BTreeSet<u32>>().len(), 18);

    // Each slot element really is in the tree under its own list, and is an item-slot element.
    for (i, w) in screen.shortcuts.slots.iter().enumerate() {
        let h = w.slots[0].handle;
        let n = ui.node(h).expect("slot element");
        assert_eq!(n.element_id().0, roots[i], "slot {i}");
        assert_eq!(n.ty().0, 0x1000_0032, "slot {i} is an item slot");
    }
    app.shutdown();
}

/// Behaviour: panels.quickbar.shows-nine-numbered-slots-and-their-items
///
/// **The nine numbers.** Bank 1's nine slots carry nine *distinct* empty-state images and bank 2's
/// nine share one, which is why retail shows `1`…`9` and no more.
///
/// The images are read out of each root's own icon-element description — nothing here writes a
/// DataID — and then asserted to be nine different ones. That is the whole claim behind "the
/// quickbar shows its nine numbered slots": the plate *is* the numeral.
///
/// Falsified by deleting `ItemSlot::clear`'s `set_state(EMPTY)` (the tile keeps no image at all) or
/// by pointing every list at one root.
#[test]
fn the_nine_bank_one_tiles_carry_nine_distinct_empty_state_numerals() {
    have_dats();
    let mut app =
        app_in_gameplay(4).expect("an app in gameplay: retail dats and a software GPU device");
    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");

    let plate = |ui: &UiSystem, w: &dereth_ui_screens::items::widget::ItemListWidget| -> DataId {
        let icon = w.slots[0].icon.expect("slot icon element");
        ui.node(icon)
            .and_then(|n| n.region.image.as_ref().map(|g| g.did))
            .expect("empty frame")
    };
    let bank1: Vec<DataId> = screen.shortcuts.slots[..9]
        .iter()
        .map(|w| plate(ui, w))
        .collect();
    let bank2: Vec<DataId> = screen.shortcuts.slots[9..]
        .iter()
        .map(|w| plate(ui, w))
        .collect();

    assert_eq!(
        bank1.iter().copied().collect::<BTreeSet<DataId>>().len(),
        9,
        "the nine on-screen tiles must be nine different pictures: {bank1:?}"
    );
    assert_eq!(
        bank2.iter().copied().collect::<BTreeSet<DataId>>().len(),
        1,
        "bank 2 is off screen and shares one plate: {bank2:?}"
    );
    // Contiguous, which is what a numeral strip looks like in the dat.
    for (i, d) in bank1.iter().enumerate() {
        assert_eq!(d.0, bank1[0].0 + u32::try_from(i).unwrap(), "tile {i}");
    }
    app.shutdown();
}

/// **Only nine are on screen**: eighteen slots, one visible row.
///
/// Nothing hides bank 2 and nothing needs to: `<TBAR>` is 100 pixels tall and bank 2 starts 90
/// pixels down the strip inside it. The test is geometric and reads both rectangles off the tree.
#[test]
fn bank_two_falls_outside_the_toolbar_window_so_retail_shows_one_row_of_nine() {
    have_dats();
    let mut app =
        app_in_gameplay(4).expect("an app in gameplay: retail dats and a software GPU device");
    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
    let root = root_of(ui).expect("gameplay root");
    let tbar = ui
        .get_child_recursive(root, window::TOOLBAR)
        .expect("<TBAR>");
    let win = ui.screen_box(tbar);

    for (i, w) in screen.shortcuts.slots.iter().enumerate() {
        let b = ui.screen_box(w.handle);
        let inside = b.y0 >= win.y0 && b.y1 <= win.y1 && b.x0 >= win.x0 && b.x1 <= win.x1;
        if i < 9 {
            assert!(inside, "bank 1 slot {i} must be inside {win:?}, is {b:?}");
        } else {
            assert!(
                !inside,
                "bank 2 slot {i} must fall outside {win:?}, is {b:?}"
            );
        }
    }
    // The nine are side by side in one row, left to right — the arrangement inside each list is
    // per list, but the nine lists themselves are placed by the layout.
    let ys: BTreeSet<i32> = screen.shortcuts.slots[..9]
        .iter()
        .map(|w| ui.screen_box(w.handle).y0)
        .collect();
    assert_eq!(ys.len(), 1, "the nine tiles share one row");
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 2. The items in them.
// ---------------------------------------------------------------------------------------------

/// An application in the game phase with the capture applied and a shortcut list in its player
/// module.
///
/// # Where the objects and their icons come from, and why they are copied rather than replayed
///
/// **The retained session's character has an empty hotbar** — its `0x0013` player description has
/// no `shortcuts` section at all — so this fixture synthesizes three shortcuts at assigned indices.
/// Their object ids and icon DataIDs are still the capture's: `replay_full` pumps the recorded stream
/// through a real [`ObjectStream`], `0xF745 ItemCreateObject` fills its public object descriptions, and the
/// `(id, icon)` pairs used below are read straight out of it.
///
/// They have to be *copied* into the application's own tables because the replay harness this file
/// inherits from the panel tests feeds the application `SessionEvent`s only — `apply_hud_events`
/// applies `0x0013`, and the object creates land in the harness's stream rather than the
/// application's. Copying a `(id, icon, name)` triple across that boundary is the same seeding
/// used by the interaction tests, and it keeps the property that matters for these three shortcuts:
/// **their object ids and icon DataIDs are not written by hand.** The separate drop test deliberately
/// uses a synthetic id because it tests only slot resolution and request shape.
fn app_with_shortcuts() -> Option<(App, Vec<ShortCutData>)> {
    app_with_shortcuts_icons(false)
}

/// [`app_with_shortcuts`], optionally with the **first two shortcut objects' icons exchanged**.
///
/// That swap is the control for the icon differential: two applications identical in every other
/// respect, including which object sits in which slot, differing only in the DataID given to the
/// first slot. Both icons are the capture's own.
fn app_with_shortcuts_icons(swap: bool) -> Option<(App, Vec<ShortCutData>)> {
    let (events, recorded) = replay_full("first-login-walk-jump")?;
    let mut app = app_in_gameplay(4)?;
    let _ = app.apply_hud_events(events_in_world(&events));
    let d = player_description(events_in_world(&events))?;

    if let Some(sc) = d.player_module.shortcuts.as_ref().filter(|s| !s.is_empty()) {
        eprintln!(
            "quickbar: {} shortcuts out of the capture's own PlayerModule",
            sc.len()
        );
        if let Some(m) = app.probe_mut().hud_mut().player_module.as_mut() {
            m.shortcuts = Some(sc.clone());
        }
        // The player-system shortcut store is the one the bar draws from, and the client keeps
        // two copies of the `0x0013` blob: the HUD's verbatim clone above, and the
        // player-system copy filled from the same bytes and updated on every drop. The HUD shortcut
        // lookup reads the second, so a seed that only wrote the first is seeding the wrong copy.
        for s in sc {
            app.probe_mut()
                .objects_mut()
                .world
                .player_system
                .add_shortcut(*s);
        }
        for _ in 0..4 {
            app.frame();
        }
        return Some((app, sc.clone()));
    }

    // Three objects the capture's own `0xF745`s described, in id order, keeping only distinct
    // icons so the swap below is a real difference.
    //
    // **Never the local player.** The icon path substitutes the backpack tile -- enum value
    // `0x10000004`, group `7`, DataID `0x0600127E` -- for the one object identified as the
    // player, whatever its description's icon says (`object_recipe`). The capture's first object
    // with an icon is `0x50000002`, the player, so taking objects "in id order" would put the
    // player in slot 0 and expect its ordinary icon. The player's tile belongs to the icon
    // composite tests; the claim here is that an ordinary object's own icon reaches its own tile.
    let player = app.objects().world.player.or(app.hud().player);
    let mut seen_icons: BTreeSet<u32> = BTreeSet::new();
    let mut items: Vec<CapturedObject> = recorded
        .iter()
        .filter(|(id, i, _)| Some(*id) != player && *i != 0 && seen_icons.insert(*i))
        .take(4)
        .cloned()
        .collect();
    // The fourth object goes in no slot; its icon is only there to be the swap's other value, so
    // that the two arms of the icon differential differ in **one** tile.
    let spare = items.pop().map(|(_, i, _)| i);
    if swap {
        if let (Some(first), Some(other)) = (items.first_mut(), spare) {
            first.1 = other;
        }
    }
    eprintln!(
        "quickbar: the capture's hotbar is empty; {} of its {} created objects carry an icon",
        recorded.iter().filter(|(_, i, _)| *i != 0).count(),
        recorded.len()
    );
    for (id, icon, name) in &items {
        eprintln!("quickbar:   {id:?} icon={icon:#010X} {name:?}");
    }
    assert!(
        !items.is_empty(),
        "the capture created no object with an icon; the shortcut tests have no oracle"
    );
    {
        let w = &mut app.probe_mut().objects_mut().world;
        for (id, icon, name) in &items {
            if w.tables.weenies.get(*id).is_none() {
                w.tables
                    .weenies
                    .insert(*id, dereth_client_model::Weenie::new(*id));
            }
            let e = w.tables.weenies.get_mut(*id).expect("just inserted");
            e.pwd.icon_id = *icon;
            e.pwd.name.clone_from(name);
        }
    }
    let sc: Vec<ShortCutData> = items
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| ShortCutData {
            index: i32::try_from(i).unwrap(),
            object_id: *id,
            spell_id: 0,
        })
        .collect();
    // Each shortcut is written at its recorded index; the blob is the same one the player-description
    // update reads, so putting the list there is exactly what `0x0013` does. It also
    // has to go into the player-system store, because that is the copy the HUD lookup reads and the
    // copy a drop writes.
    if let Some(m) = app.probe_mut().hud_mut().player_module.as_mut() {
        m.shortcuts = Some(sc.clone());
    }
    for s in &sc {
        app.probe_mut()
            .objects_mut()
            .world
            .player_system
            .add_shortcut(*s);
    }
    for _ in 0..4 {
        app.frame();
    }
    Some((app, sc))
}

/// Every shortcut in the player module is in its own slot, with the icon the object table gives
/// that object.
///
/// Falsified by deleting the `update_shortcuts` call from `Hud::drive`, or `HudView::shortcut`'s
/// body: `item_at` goes to `None` for every slot.
#[test]
fn every_shortcut_in_the_player_module_lands_in_its_own_slot_with_its_icon() {
    have_dats();
    let (mut app, sc) = app_with_shortcuts().expect("an app with the shipped shortcut bar");
    assert!(!sc.is_empty(), "no shortcut to place");
    let want: Vec<(u32, ObjectId)> = sc
        .iter()
        .map(|s| (u32::try_from(s.index).unwrap(), s.object_id))
        .collect();

    assert!(
        app.hud().stats.shortcuts_written > 0,
        "the quickbar never ran"
    );

    // Each object's own description icon is the only icon the slot update may install.
    let icons: Vec<Option<DataId>> = want
        .iter()
        .map(|(_, id)| {
            app.objects()
                .world
                .weenie(*id)
                .map(|w| w.pwd.icon_id)
                .filter(|i| *i != 0)
                .map(DataId)
        })
        .collect();

    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
    for ((slot, id), want_icon) in want.iter().zip(&icons) {
        assert_eq!(screen.shortcuts.item_at(*slot), Some(*id), "slot {slot}");
        // The icon is on the slot's icon element. It must be **this object's** icon: the element
        // keeps the numbered empty-slot plate until
        // something overwrites it, so `is_some()` alone would pass with `set_icon` deleted.
        let w = &screen.shortcuts.slots[*slot as usize];
        let icon = w.slots[0].icon.expect("slot icon element");
        let drawn = ui
            .node(icon)
            .and_then(|n| n.region.image.as_ref().map(|g| g.did));
        assert_eq!(drawn, *want_icon, "slot {slot} must draw {id:?}'s own icon");
    }
    // Every other slot is empty and still numbered.
    let held: BTreeSet<u32> = want.iter().map(|(s, _)| *s).collect();
    for slot in 0..SLOT_COUNT {
        if held.contains(&slot) {
            continue;
        }
        assert_eq!(
            screen.shortcuts.item_at(slot),
            None,
            "slot {slot} must be empty"
        );
    }
    app.shutdown();
}

/// The shortcut-number update has three arms, seen on the numeral element.
///
/// * an occupied slot in a non-magic stance takes its picture from `0x10000042`, indexed by the
///   slot number, and becomes visible;
/// * the same slot in `MAGIC_COMBAT_MODE` takes it from `0x10000043` — the toolbar-active state
///   selects between these two arrays;
/// * an empty slot passes no shortcut number and hides the numeral outright.
///
/// The two arrays are read off the live tree here as well, so the assertion is "the picture the
/// slot ended up with is the one that array holds at that index" and not a hard-coded DataID.
///
/// Falsified by deleting `ItemSlot::set_shortcut_num`'s array lookup (no picture), or by making
/// `ShortcutBar::update` pass `false` for the ghost flag (the two stances stop differing).
#[test]
fn an_occupied_slots_numeral_comes_from_the_plain_array_and_magic_mode_takes_the_ghosted_one() {
    have_dats();
    let (mut app, sc) = app_with_shortcuts().expect("an app with the shipped shortcut bar");
    let slot = u32::try_from(sc[0].index).unwrap();

    // The two eighteen-entry arrays, read off the numeral element in this very slot.
    let arrays = {
        let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
        let h = screen.shortcuts.slots[slot as usize].slots[0]
            .shortcut_num_elem
            .expect("shortcut numeral element");
        let props = ui.node(h).expect("numeral").merged_properties();
        let read = |id: u32| -> Vec<DataId> {
            match props.get(id) {
                Some(dereth_assets::ui::PropertyValue::Array(v)) => v
                    .iter()
                    .filter_map(|b| match &b.value {
                        dereth_assets::ui::PropertyValue::DataFile(d) => Some(*d),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        (read(0x1000_0042), read(0x1000_0043), read(0x1000_005E))
    };
    let (plain, ghosted, empty) = arrays;
    assert_eq!(plain.len(), 18, "UI_ItemList_ShortcutOverlayArray");
    assert_eq!(
        ghosted.len(),
        18,
        "UI_ItemList_ShortcutOverlayArray_Ghosted"
    );
    assert!(
        empty.is_empty(),
        "the selected shortcut numeral has no UI_ItemList_ShortcutOverlayArray_Empty; \
         the empty-shortcut numeral arm has no array in this fixture"
    );
    assert_ne!(
        plain[slot as usize], ghosted[slot as usize],
        "the ghosted numeral differs"
    );

    // (a) A non-magic stance: the plain numeral, visible.
    {
        let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
        assert!(
            screen.shortcuts.toolbar_active,
            "the capture's character is not in magic mode"
        );
        assert_eq!(
            screen.shortcuts.numeral_of(ui, slot),
            Some((Some(plain[slot as usize]), true)),
            "slot {slot} must wear its own numeral"
        );
    }

    // (b) This test directly changes combat mode to magic and advances a frame; the resulting
    // HUD update clears toolbar-active state and renumbers every occupied slot from the ghosted array.
    app.probe_mut().objects_mut().world.combat.combat_mode =
        dereth_client_model::combat::CombatMode::from_raw(
            dereth_ui_screens::toolbar::combat_mode::MAGIC,
        );
    app.frame();
    {
        let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
        assert!(
            !screen.shortcuts.toolbar_active,
            "magic mode clears toolbar-active state"
        );
        assert_eq!(
            screen.shortcuts.numeral_of(ui, slot),
            Some((Some(ghosted[slot as usize]), true)),
            "in magic mode the numeral is the ghosted one"
        );
    }

    // (c) An empty slot: no shortcut number, so the numeral is hidden.
    let free = (0..SLOT_COUNT)
        .find(|s| sc.iter().all(|d| u32::try_from(d.index).ok() != Some(*s)))
        .expect("some slot is free");
    {
        let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
        let (_, visible) = screen
            .shortcuts
            .numeral_of(ui, free)
            .expect("numeral element");
        assert!(!visible, "an empty slot shows no numeral overlay");
    }
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 3. What the bar does when it is used.
// ---------------------------------------------------------------------------------------------

/// Shortcut use and the four action ranges that reach it.
///
/// Primary use produces a use request, secondary use produces a selection request, and an empty
/// slot takes the missing-item early return — nothing at all, not a deselect.
///
/// Falsified by deleting `ShortcutBar::use_shortcut`'s body.
#[test]
fn a_shortcut_action_uses_the_object_in_its_slot_and_an_empty_slot_does_nothing() {
    have_dats();
    let (mut app, sc) = app_with_shortcuts().expect("an app with the shipped shortcut bar");
    let slot = u32::try_from(sc[0].index).unwrap();
    let item = sc[0].object_id;
    let free = (0..SLOT_COUNT)
        .find(|s| sc.iter().all(|d| u32::try_from(d.index).ok() != Some(*s)))
        .expect("some slot is free");

    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");
    // The action table: `0x10000042 + n` is slot n primary, `0x1000004E + n` secondary.
    assert_eq!(
        screen.on_input_action(ui, 0x1000_0042 + slot),
        Some(UiRequest::Use(item)),
        "primary use"
    );
    assert_eq!(
        screen.on_input_action(ui, 0x1000_004E + slot),
        Some(UiRequest::Select(item)),
        "secondary use is a selection"
    );
    assert_eq!(
        screen.on_input_action(ui, 0x1000_0042 + free),
        None,
        "an empty slot is inert"
    );
    assert_eq!(
        screen.on_input_action(ui, 0x0000_1234),
        None,
        "an action that is not a shortcut"
    );
    app.shutdown();
}

/// A drag released over a shortcut slot resolves to
/// **that slot's number**, whichever descendant of the list the pointer was actually over.
///
/// The three handles tried are the three the client's ancestor sweep has to cope with: the list,
/// its item-slot element, and one of that item's icon layers.
///
/// Falsified by deleting `ShortcutBar::slot_under`'s `is_ancestor` arm — the icon-layer case starts
/// resolving to nothing, which is the shape of the bug that would make a drop land in the wrong
/// place rather than fail loudly.
#[test]
fn a_drop_anywhere_inside_a_shortcut_slot_resolves_to_that_slots_number() {
    have_dats();
    let mut app =
        app_in_gameplay(4).expect("an app in gameplay: retail dats and a software GPU device");
    let item = ObjectId(0x5000_1234);
    let (ui, screen) = gameplay_screen(&mut app).expect("the gameplay screen is up");

    for slot in [0u32, 4, 8, 17] {
        let w = &screen.shortcuts.slots[slot as usize];
        let list = w.handle;
        let uiitem = w.slots[0].handle;
        let layer = w.slots[0].icon.expect("slot icon element");
        for (what, h) in [
            ("the list", list),
            ("the item", uiitem),
            ("an icon layer", layer),
        ] {
            assert_eq!(
                screen.shortcuts.slot_under(ui, h),
                Some(slot),
                "{what} of slot {slot}"
            );
        }
        // The drop handler **forks** on `flags & 14` (the alias mask, not one bit)
        // rather than merely gating on it. Flags `0` means a regular item dragged from another
        // list.
        assert_eq!(
            screen.shortcuts.handle_drop_release(ui, layer, item, 0),
            Some(UiRequest::DragDrop {
                item,
                target: DropTarget::ShortcutSlot(slot)
            }),
        );
        // The shortcut drop flag (4) selects the tile-to-tile shortcut-alias move.
        assert_eq!(
            screen.shortcuts.handle_drop_release(ui, layer, item, 4),
            Some(UiRequest::DragDrop {
                item,
                target: DropTarget::ShortcutAlias {
                    slot,
                    from: screen.shortcuts.last_dragged
                },
            }),
        );
        // The vendor drop flag (2) reaches the `else if` and is **declined** by it: the client's
        // second arm tests the shortcut flag (4) specifically, not the whole mask.
        assert_eq!(
            screen.shortcuts.handle_drop_release(ui, layer, item, 2),
            None,
            "a vendor proxy dropped on slot {slot} is accepted by neither drop-handler arm"
        );
    }
    // Something that is not in the bar at all.
    let root = root_of(ui).expect("root");
    assert_eq!(
        screen.shortcuts.slot_under(ui, root),
        None,
        "the screen root is not a slot"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 4. It draws, and only where it said it would.
// ---------------------------------------------------------------------------------------------

/// Two matched application instances differ only in whether the quickbar's tiles are up, with every
/// changed pixel inside the nine on-screen slots' own rectangles and zero outside.
///
/// The two arms use the same scene setup, capture and frame count, and
/// differ by clearing visibility on each **bank 1** slot element, which is the narrowest statement
/// of "the tiles are what is being drawn". A control that skipped the whole toolbar would also move
/// the stance icon, the panel buttons and the selected-object read-out, which is exactly why it is
/// not the control.
///
/// Bank 2 is deliberately left alone in both arms. Its nine lists sit at y = 90…121 inside a
/// 100-pixel window, so `screen_clip_box`'s intersection with `<TBAR>` leaves a few rows of them
/// showing along the very bottom of the strip — the client's own arithmetic does the same, and
/// hiding them here would be this test choosing what the toolbar looks like.
///
/// If the numbered plates were not actually rasterised the differential would be zero pixels; if
/// they were drawn outside their own lists the outside count would not be.
#[test]
fn the_nine_tiles_change_pixels_and_only_inside_the_nine_slots_boxes() {
    have_dats();
    let events = replay("first-login-walk-jump").expect("the capture corpus session");

    type Shot = (u32, u32, Vec<u8>, Vec<dereth_ui::Box2D>);
    let shot = |hide: bool| -> Option<Shot> {
        let mut app = app_in_gameplay(4)?;
        let _ = app.apply_hud_events(events_in_world(&events));
        for _ in 0..6 {
            app.frame();
        }
        let boxes = {
            let (ui, screen) = gameplay_screen(&mut app)?;
            screen.shortcuts.slots[..9]
                .iter()
                .map(|w| ui.screen_box(w.handle))
                .collect::<Vec<_>>()
        };
        if hide {
            let (ui, screen) = gameplay_screen(&mut app)?;
            for w in &screen.shortcuts.slots[..9] {
                for s in &w.slots {
                    ui.set_visible(s.handle, false);
                }
            }
        }
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, boxes))
    };

    let (w, h, shown, boxes) =
        shot(false).expect("a rendered frame: retail dats and a software GPU device");
    let (w2, h2, hidden, _) =
        shot(true).expect("a rendered frame: retail dats and a software GPU device");
    assert_eq!((w, h), (w2, h2));
    assert_eq!(boxes.len(), 9);
    assert!(
        boxes.iter().all(|b| b.width() > 0 && b.height() > 0),
        "{boxes:?}"
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if shown[i..i + 4] == hidden[i..i + 4] {
                continue;
            }
            if boxes
                .iter()
                .any(|b| x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1)
            {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "the quickbar's tiles drew nothing at all");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the nine slots {boxes:?}"
    );
}

/// **The items in the slots**, as pixels.
///
/// Two matched application instances whose only difference is **which icon the object in slot 0
/// carries**: both arms hold the same object in the same slot with the same numeral, and the two
/// icons are two of the capture's own, one of them belonging to a fourth object that goes in no
/// slot. So the differential is exactly the slot update's installation of the object's description
/// icon and nothing else.
///
/// **The obvious control is the wrong one.** Flushing the slot instead moves pixels whether or not
/// the icon draws, because applying empty state `0x1000001C` also takes the whole icon-overlay
/// subtree down with it — that control passes with `set_icon`'s body deleted, so the swap is the
/// control.
///
/// Falsified by deleting `ItemSlot::set_icon`'s body: both arms become identical and `inside > 0`
/// fails.
#[test]
fn a_shortcuts_own_icon_changes_pixels_and_only_inside_its_own_tile() {
    have_dats();
    type Shot = (u32, u32, Vec<u8>, dereth_ui::Box2D, Option<DataId>);
    let shot = |swap: bool| -> Option<Shot> {
        let (mut app, sc) = app_with_shortcuts_icons(swap)?;
        let slot = u32::try_from(sc[0].index).ok()?;
        for _ in 0..4 {
            app.frame();
        }
        let (ui, screen) = gameplay_screen(&mut app)?;
        let w = &screen.shortcuts.slots[slot as usize];
        let tile = ui.screen_box(w.handle);
        let drawn = w.slots[0]
            .icon
            .and_then(|h| ui.node(h)?.region.image.as_ref().map(|g| g.did));
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, tile, drawn))
    };

    let (w, h, plain, tile, icon_a) =
        shot(false).expect("a rendered frame: retail dats and a software GPU device");
    let (w2, h2, swapped, _, icon_b) =
        shot(true).expect("a rendered frame: retail dats and a software GPU device");
    assert_eq!((w, h), (w2, h2));
    assert!(tile.width() > 0 && tile.height() > 0, "{tile:?}");
    assert!(
        icon_a.is_some() && icon_b.is_some(),
        "the slot drew no icon at all"
    );
    assert_ne!(
        icon_a, icon_b,
        "the two arms must differ in the icon; the swap did nothing"
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if plain[i..i + 4] == swapped[i..i + 4] {
                continue;
            }
            if x >= tile.x0 && x <= tile.x1 && y >= tile.y0 && y <= tile.y1 {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 0, "the shortcut's icon never reached the screen");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the tile {tile:?}"
    );
}
