//! Pointer gestures in the game view: a real click in the viewport arms a pick; a click on a stance
//! icon (`0x10000192` through `0x10000195`) toggles combat mode and sends it; and a click on a
//! side pack sets the world's open container. (The picker itself, against recorded objects, is
//! the dat tier's `selection::recorded_object_pick`.)
//! Fixture: the retail dats, the corpus's own recorded objects, and a headless `App` in gameplay
//! whose pointer messages are built by its own `Pump` and delivered to its input manager.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::interaction::SearchReason;
use dereth_client::pump::{Pump, Win32Message};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::{RecordingRequests, Request};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_ui::ElementId;
use winit::event::MouseButton;

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// Fails when the retail dats are not where `$DERETH_TEST_DAT_DIR` says.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The pointer, driven as Windows would drive it
// ---------------------------------------------------------------------------------------------

/// The pointer. Every `MSG` is built by the application's own [`Pump`].
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 100_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn move_to(&mut self, app: &mut App, x: f64, y: f64) {
        self.time_ms += 10;
        let m = self.pump.mouse_move_message(x, y, self.time_ms);
        self.send(app, m);
    }

    fn button(&mut self, app: &mut App, which: MouseButton, pressed: bool, gap_ms: u32) {
        self.time_ms += gap_ms;
        let m = self
            .pump
            .mouse_button_message(which, pressed, self.time_ms)
            .expect("the button is in the 0x200 block");
        self.send(app, m);
    }

    fn click(&mut self, app: &mut App, which: MouseButton, at: (f64, f64)) {
        self.move_to(app, at.0, at.1);
        self.button(app, which, true, 10);
        self.button(app, which, false, 10);
        app.frame();
    }
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// An application with the world loaded and gameplay active, using the HUD test's harness shape.
fn app_in_gameplay(frames: u32) -> App {
    let mut app = App::new(Config {
        ui: true,
        ..base_config()
    })
    .expect("an App with the UI up: retail dats and a WARP device");
    app.start_shell().expect("the shell starts");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn find(app: &App, id: ElementId) -> Option<dereth_ui::ElemHandle> {
    let shell = app.ui()?;
    let root = shell
        .flow
        .current()
        .and_then(|s| s.roots().first().copied())?;
    fn walk(
        ui: &dereth_ui::UiSystem,
        h: dereth_ui::ElemHandle,
        id: ElementId,
    ) -> Option<dereth_ui::ElemHandle> {
        if ui.node(h)?.element_id() == id {
            return Some(h);
        }
        ui.children(h).into_iter().find_map(|c| walk(ui, c, id))
    }
    walk(&shell.ui, root, id)
}

fn centre(app: &App, h: dereth_ui::ElemHandle) -> (f64, f64) {
    let b = app.ui().expect("shell").ui.screen_box(h);
    (f64::from(b.x0 + b.x1) / 2.0, f64::from(b.y0 + b.y1) / 2.0)
}

/// Left press (button 7), while the search reason is below examine,
/// requests targeted use when target mode is set and selection otherwise. Right release
/// (button 8) requests examine. Both ask the world picker to search the pointer position.
///
/// This test checks request-counter deltas after pump messages pass through input/action
/// dispatch, `UiShell::take_mouse_events` and `Interaction::wrapper_mouse` into the picker.
///
/// The counter has a second producer: the client picks every frame, with no pointer event at all.
///
/// * Frame processing runs queued UI work before the element update. That update
///   broadcasts global message 3 after tooltip checking and before input ticking and dirty drawing.
/// * The viewport forwards message 3 to its per-frame hover search. Its gates are a search
///   reason below mouse-over (1) and an existing input manager, not motion, dirtiness or a timer.
/// * Pick completion unconditionally resets the reason to none (0).
///
/// So the reason is zero again at the end of every frame and the hover pick is re-armed in the
/// next one. **An idle in-game frame arms exactly one pick**, and a frame carrying a left press
/// or a right release arms two: the hover search first, then the pointer request overwrites it.
/// Left press allows reasons below examine; right release allows reasons below 3. Hover's 1
/// passes both gates, so only the final request completes in that frame.
///
/// Arming a search sends no wire traffic: it stores pointer x/y, clears
/// the selection result to object 0 and part index -1, then sets or clears the selection cursor.
/// Neither branch sends a message; later completion can drive a separate interaction.
#[test]
fn a_real_click_in_the_viewport_asks_the_world_view_to_find_an_object() {
    have_dats();
    let mut app = app_in_gameplay(12);

    // 1. Per-frame hover search alone. Use deltas so the harness's frame count is not baked in.
    let before = app.interaction().pick.stats;
    let selections_before = app.interaction().stats.selections;
    for _ in 0..3 {
        app.frame();
    }
    let s = app.interaction().pick.stats;
    assert_eq!(
        s.requests - before.requests,
        3,
        "one `world-object lookup` per idle frame, from global message 3"
    );
    assert_eq!(
        s.outside_viewport, before.outside_viewport,
        "and the resting pointer is inside `<SBOX>`'s rectangle, so every one of them armed"
    );
    assert_eq!(
        (s.missed + s.found) - (before.missed + before.found),
        3,
        "each completed in its own frame, as the pick sweep does"
    );
    assert_eq!(
        app.interaction().stats.selections,
        selections_before,
        "hover picking never selects: its completion handler sets no selected object"
    );
    // The reset lets the next frame's hover search pass its gate again.
    assert_eq!(app.interaction().search_reason(), SearchReason::None);

    // 2. The middle of the screen, which the gameplay layout gives to `<SBOX>`.
    let before = app.interaction().pick.stats;
    let mut hand = Hand::new();
    hand.click(&mut app, MouseButton::Left, (400.0, 300.0));
    let s = app.interaction().pick.stats;
    assert_eq!(
        s.requests - before.requests,
        2,
        concat!(
            "the click frame arms twice: the per-frame hover pick, then ",
            "the left-press selection pick over the top of it"
        )
    );
    assert_eq!(s.outside_viewport, before.outside_viewport);
    assert_eq!(
        (s.missed + s.found) - (before.missed + before.found),
        1,
        "but only one notice: the second arm overwrote the first before the pick sweep read it"
    );
    // Completion clears the search reason after the gesture.
    assert_eq!(app.interaction().search_reason(), SearchReason::None);

    // 3. A right click requests examine on release (button 8).
    let before = app.interaction().pick.stats;
    hand.click(&mut app, MouseButton::Right, (400.0, 300.0));
    assert_eq!(
        app.interaction().pick.stats.requests - before.requests,
        2,
        "the same two: the per-frame hover pick, then the right release's examine pick"
    );
    app.shutdown();
}

/// Behaviour: combat.mode.the-toggle-out-of-combat-reaches-the-shard
///
/// Message 1 from `0x10000192`, `0x10000193`,
/// `0x10000194` or `0x10000195` toggles combat. From peace it selects the default mode for the
/// equipped weapon.
///
/// The fixture supplies the player and wielded melee weapon needed by default-mode selection.
#[test]
fn a_real_click_on_the_stance_icon_toggles_combat_mode_and_sends_it() {
    have_dats();
    let mut app = app_in_gameplay(12);
    let button = find(&app, ElementId(0x1000_0192))
        .expect("the toolbar's stance icon 0x10000192 is in the shipped layout");
    // Seed the player and his weapon, which is what `0x0013` does on a real login.
    {
        let w = &mut app.objects_mut().world;
        let player = ObjectId(0x5000_0002);
        seed_player(w, player);
        let sword = ObjectId(0x8000_00AA);
        seed_weapon(w, sword, player);
        w.inventory_mask = dereth_client_model::inventory::slots::loc::MELEE_WEAPON;
    }
    assert_eq!(
        app.objects().world.combat.combat_mode,
        dereth_client_model::combat::CombatMode::NonCombat,
        "the player starts in peace mode"
    );

    let at = centre(&app, button);
    let mut hand = Hand::new();
    let undeliverable = app.interaction().stats.requests_undeliverable;
    hand.click(&mut app, MouseButton::Left, at);

    assert_eq!(
        app.interaction().stats.combat_mode_toggles,
        1,
        "the click reached the combat-mode toggle"
    );
    assert_eq!(
        app.objects().world.combat.combat_mode,
        dereth_client_model::combat::CombatMode::Melee,
        "peace -> the default mode for a wielded melee weapon"
    );
    // ...and it tried to tell the server. There is no session in this test, so the request is
    // counted as undeliverable rather than sent; what matters is that one was produced.
    //
    // The cumulative undeliverable counter also carries the allegiance panel's unconditional
    // sends: allegiance initialization requests an update when gameplay opens, and the player
    // description notification requests another (an already-set awaiting-update latch skips the
    // busy-count increment but not the update request with argument 1). So the click is judged by
    // `last_sent`, which is cleared at the head of every `registered_systems_use_time`;
    // `Hand::click` runs exactly one `app.frame()`, so it holds precisely the requests this click
    // produced. It fails if the icon sends nothing, sends twice, or sends the wrong mode.
    let modes: Vec<u32> = app
        .interaction()
        .last_sent
        .iter()
        .filter_map(|r| match r {
            Request::ChangeCombatMode(m) => Some(m.combat_mode),
            _ => None,
        })
        .collect();
    assert_eq!(
        modes,
        vec![dereth_client_model::combat::CombatMode::Melee as u32],
        "one Combat_ChangeCombatMode was produced, and it names Melee: {:?}",
        app.interaction().last_sent
    );
    assert_eq!(
        app.interaction().stats.requests_undeliverable - undeliverable,
        u64::try_from(app.interaction().last_sent.len()).unwrap(),
        "no session, so every request this frame produced was counted undeliverable"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// Seeds — the fields `0x0013` and `0xF745` fill in, written here because these tests have no
// server. Description fields are paired with explicit world, inventory and player bookkeeping.
// ---------------------------------------------------------------------------------------------

pub(crate) fn weenie(
    w: &mut dereth_client_model::World,
    id: ObjectId,
) -> &mut dereth_client_model::Weenie {
    if w.tables.weenies.get(id).is_none() {
        w.tables
            .weenies
            .insert(id, dereth_client_model::Weenie::new(id));
    }
    w.tables.weenies.get_mut(id).expect("just inserted")
}

pub(crate) fn seed_player(w: &mut dereth_client_model::World, id: ObjectId) {
    w.player = Some(id);
    weenie(w, id).pwd.name = "Aldis".into();
    w.tables
        .inventories
        .insert(id, dereth_client_model::objects::ObjectInventory::new(id));
}

pub(crate) fn seed_item(w: &mut dereth_client_model::World, id: ObjectId, container: ObjectId) {
    let it = weenie(w, id);
    it.pwd.name = "Pyreal".into();
    it.pwd.container_id = Some(container);
}

pub(crate) fn seed_container(w: &mut dereth_client_model::World, id: ObjectId) {
    let c = weenie(w, id);
    c.pwd.name = "Chest".into();
    c.pwd.obj_type = dereth_client_model::weenie::item_type::CONTAINER;
}

fn seed_weapon(w: &mut dereth_client_model::World, id: ObjectId, wielder: ObjectId) {
    {
        let it = weenie(w, id);
        it.pwd.name = "Sword".into();
        it.pwd.obj_type = dereth_client_model::weenie::item_type::MELEE_WEAPON;
        it.pwd.wielder_id = Some(wielder);
        it.pwd.location = Some(dereth_client_model::inventory::slots::loc::WEAPON);
    }
    if let Some(inv) = w.tables.inventories.get_mut(wielder) {
        inv.set_placement(id, dereth_client_model::inventory::slots::loc::WEAPON, 0);
    }
}

/// A side pack of the player's, in his container list, the way `0x0013`'s `content_profiles`
/// would put it there.
fn seed_side_pack(w: &mut dereth_client_model::World, id: ObjectId, player: ObjectId) {
    {
        let c = weenie(w, id);
        c.pwd.name = "Backpack".into();
        c.pwd.obj_type = dereth_client_model::weenie::item_type::CONTAINER;
        // The container predicate is the openable bit or either nonzero capacity, not the
        // item-type field.
        c.pwd.bitfield = dereth_client_model::weenie::bitfield::OPENABLE;
        c.pwd.items_capacity = Some(24);
        c.pwd.containers_capacity = Some(0);
        c.pwd.container_id = Some(player);
    }
    if let Some(inv) = w.tables.inventories.get_mut(player) {
        inv.containers.push(id);
    }
    // The side-pack strip uses its owner's container capacity. Without it the player has zero
    // slots and this click has nowhere to land. Seven is the strip's cap.
    {
        let p = weenie(w, player);
        p.pwd.containers_capacity = Some(7);
        p.pwd.items_capacity = Some(102);
    }
    w.tables
        .inventories
        .insert(id, dereth_client_model::objects::ObjectInventory::new(id));
}

/// Behaviour: inventory.pack.clicking-a-side-pack-refills-the-grid-in-the-same-frame
///
/// A click on a side pack writes the world's open-container field.
///
/// The item list notifies when its parent container changes. The receiving player handler
/// updates the open-container id only if the object exists and is owned by the player. Without
/// the write, a pickup goes to the player regardless of the pack open in the grid.
///
/// What is asserted is the whole chain, not the handler: a real press on the
/// container strip -> `GamePlayScreen::on_item_list_press` -> `InventoryPanels::on_slot_clicked`
/// -> `requests::emit` -> `UiShell::frame`'s drain -> `Interaction::queue` ->
/// `run_ui_requests` -> the world.
///
/// **Falsified by** removing the `UiRequest::NewParentContainer` arm from `run_ui_requests`, by
/// removing the `requests::emit` from `InventoryPanels::open_container`, by either guard in
/// the world's `on_new_parent_container`, or by leaving the departed pack as the shared
/// inventory parent after a notified move.
#[test]
fn clicking_a_side_pack_is_the_writer_world_open_container_did_not_have() {
    have_dats();
    let mut app = app_in_gameplay(12);
    let player = ObjectId(0x5000_0002);
    let pack = ObjectId(0x8000_00BB);
    {
        let w = &mut app.objects_mut().world;
        seed_player(w, player);
        seed_side_pack(w, pack, player);
    }
    // The inventory page must be visible: hit testing rejects slots inside invisible ancestors,
    // regardless of the slot's own mouse-visible flag.
    let panel_id = gameplay(&mut app)
        .panels
        .pages
        .iter()
        .find(|q| q.element == ElementId(0x1000_018B))
        .map(|q| q.panel_id)
        .expect("the inventory page 0x1000018B is in the shipped panel stack");
    {
        let shell = app.ui_mut().expect("shell");
        let ui = &mut shell.ui;
        let g = as_gameplay(shell.flow.current_mut().expect("a screen"));
        g.recv_set_panel_visibility(ui, panel_id, true);
    }
    app.frame();
    app.frame();

    assert_eq!(
        app.objects().world.open_container,
        None,
        "nothing has pointed the grid anywhere yet"
    );

    // The slot on the **container strip** that is showing the pack -- found on the live tree, not
    // named, because the element id belongs to `dereth-ui-screens`.
    let slot = {
        let g = gameplay(&mut app);
        g.inventory
            .container_list
            .iter()
            .chain(g.inventory.top_container.iter())
            .filter(|w| w.container_list)
            .find_map(|w| {
                w.slots
                    .iter()
                    .find(|s| s.item == Some(pack))
                    .map(|s| s.handle)
            })
            .unwrap_or_else(|| {
                let g2 = &g.inventory;
                panic!(
                    "no strip slot for the pack: lists top={:?} strip={:?} open={:?}",
                    g2.top_container.as_ref().map(|w| w
                        .slots
                        .iter()
                        .map(|s| s.item)
                        .collect::<Vec<_>>()),
                    g2.container_list.as_ref().map(|w| w
                        .slots
                        .iter()
                        .map(|s| s.item)
                        .collect::<Vec<_>>()),
                    g2.open_container,
                )
            })
    };
    let (cx, cy) = centre(&app, slot);
    #[allow(clippy::cast_possible_truncation)]
    let (cx, cy) = (cx as i32, cy as i32);
    app.ui_mut()
        .expect("shell")
        .ui
        .mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    app.frame();

    assert_eq!(
        gameplay(&mut app).inventory.open_container,
        Some(pack),
        "the panel's own open-container id moved"
    );
    assert_eq!(
        app.objects().world.open_container,
        Some(pack),
        "and `the player's open-container id` followed it"
    );
    assert_eq!(
        app.interaction().stats.open_containers_changed,
        1,
        "exactly one notice changed it: the strip's, not one per frame"
    );

    // A second frame with no further click must not raise it again --
    // the item list notifies only when the parent-container id changes.
    app.frame();
    assert_eq!(
        app.interaction().stats.open_containers_changed,
        1,
        "no per-frame re-raise"
    );

    // A notified departure restores the shared parent before the next frame.
    {
        let w = &mut app.objects_mut().world;
        w.server_says_move_item(
            pack,
            ObjectId(0),
            0,
            ObjectId(0),
            0,
            true,
            &mut dereth_client_model::NullSink,
        );
        assert_eq!(
            w.open_container,
            Some(player),
            "the move restores the pickup destination before any UI frame"
        );
    }
    app.frame();
    assert_eq!(
        gameplay(&mut app).inventory.open_container,
        Some(player),
        "the grid projects the restored main pack"
    );
    assert_eq!(
        app.objects().world.open_container,
        Some(player),
        "the world already restored the main pack during the move"
    );
    app.frame();
    assert_eq!(
        app.interaction().stats.open_containers_changed,
        1,
        "projecting the shared fallback emits no redundant parent request"
    );

    {
        let w = &mut app.objects_mut().world;
        // A notice naming the already-open container reports no change and must not count twice.
        assert!(
            !w.on_new_parent_container(player),
            "the same container again is not a change"
        );
        assert_eq!(w.open_container, Some(player));
        // And a container the player does not own is refused, which is what keeps a corpse out of
        // the owned open-container field; ground-container state handles that separate case.
        let corpse = ObjectId(0x8000_00CC);
        seed_container(w, corpse);
        assert!(
            !w.on_new_parent_container(corpse),
            "the ownership guard refuses it"
        );
        // A containment notification can precede the referenced object's creation. Such an id,
        // absent from the object table, is refused by the other guard.
        assert!(
            !w.on_new_parent_container(ObjectId(0x8000_0FFF)),
            "the object-existence guard refuses it"
        );
        assert_eq!(w.open_container, Some(player), "neither refusal moved it");
    }

    // ---- `MainPackPreferred` still overrides the open pack, which nothing else here can see ----
    // The pickup destination is the player if useMainPack or MainPackPreferred is set. No
    // recorded character has options2 bit14 enabled, so this synthetic toggle exercises the
    // preference instead of relying on recordings.
    {
        let loot = ObjectId(0x8000_00DD);
        let w = &mut app.objects_mut().world;
        seed_side_pack(w, pack, player);
        seed_item(w, loot, ObjectId(0));
        assert!(w.on_new_parent_container(pack), "the pack is open again");

        let dest = |w: &mut dereth_client_model::World| -> Option<ObjectId> {
            let mut req = RecordingRequests::default();
            w.request_lock = dereth_client_model::inventory::requests::RequestLock::default();
            w.place_in_backpack(
                &mut req,
                &mut dereth_client_model::NullSink,
                loot,
                false,
                SplitState::default(),
                ServerTime(0.0),
            );
            req.0.iter().find_map(|r| match r {
                Request::PutItemInContainer(m) => Some(m.container),
                _ => None,
            })
        };
        assert_eq!(
            dest(w),
            Some(pack),
            "with the pack open the pickup goes into it"
        );
        w.player_system.options.set(
            dereth_client_model::player::options::option::MAIN_PACK_PREFERRED,
            true,
        );
        assert!(w.player_system.options.main_pack_preferred());
        assert_eq!(
            dest(w),
            Some(player),
            "MainPackPreferred sends it to the player instead"
        );
    }

    app.shutdown();
}

type GamePlayScreenAlias = dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// `GamePlayScreen`, off the live `UiFlow`.
fn as_gameplay(screen: &mut Box<dyn dereth_ui::framework::Screen>) -> &mut GamePlayScreenAlias {
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<GamePlayScreenAlias>()
        .expect("gameplay screen")
}

fn gameplay(app: &mut App) -> &mut GamePlayScreenAlias {
    as_gameplay(
        app.ui_mut()
            .expect("shell")
            .flow
            .current_mut()
            .expect("a screen"),
    )
}
