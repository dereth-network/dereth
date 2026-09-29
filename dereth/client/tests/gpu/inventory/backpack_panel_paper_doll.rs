//! The backpack panel's title, headings and paper doll. The panel shows `Inventory of <name>`
//! (element `0x100001D3`), a 3D paper doll (`0x100001D5`) wearing the character's equipped gear,
//! and `Contents of Backpack` over the grid (`0x100001C5`), retitled `Contents of <container>` when
//! another container is opened. The doll is asserted on the `GfxObj` each baked mesh was built from
//! (`PreviewObject::built_from`) and on pixel differentials, not on the model list: applying the
//! visual-description changes after the bake changes the model list but neither the baked source
//! ids nor the pixels. The camera, light, pose and armour-slot visibility follow the client's
//! paper-doll initialization.
//! Fixture: the `first-login-walk-jump` recording (its own `0xF745` for the player dresses the
//! doll) replayed into a headless gameplay `App` with the retail dats; no network.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::gpu::PreviewId;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_ui::framework::Screen;
use dereth_ui::{Box2D, ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::inventory::{
    contents_text, paper_doll as pd, title_text, BACKPACK_CONTENTS, CONTENTS_TEXT,
    PAPER_DOLL_ANIMATION_ENUM, PAPER_DOLL_VIEWPORT, TITLE_TEXT,
};
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

const SESSION: &str = "first-login-walk-jump";

// =================================================================================================
// The capture reader.
// =================================================================================================

fn store() -> Arc<dereth_dat::RetailDatStore> {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    Arc::new(dereth_client::assets::open_data_files(&d).expect("the retail dats open"))
}

/// The capture's server half replayed into an [`ObjectStream`], up to record `limit` inclusive.
///
/// Returns both halves because the two are wanted together: the `SessionEvent`s drive the HUD,
/// while the `ObjectStream` supplies the current player's physics object and visual description
/// on this side of the seam.
fn replay_to(session: &str, limit: usize) -> (Vec<SessionEvent>, ObjectStream) {
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
    for r in records.iter().take(limit.saturating_add(1)) {
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
    (events, objects)
}

/// The capture replayed to the **last record at which the player still exists**.
///
/// The logout at the tail of every recording ends the character session, and
/// `ObjectStream::apply`'s `StateChanged(CharacterSelect) | LoggedOff` arm calls `reset()` — so a
/// replay of the whole file ends with an empty object table and no player at all. That is correct
/// behaviour and it is why the limit is measured rather than guessed: a replay of everything leaves
/// a table the recording has legitimately torn down.
///
/// The limit is found by a full pass, and the pass **asserts its own denominator**: a capture whose
/// player never appears cannot be this file's oracle.
fn replay(session: &str) -> (Vec<SessionEvent>, ObjectStream) {
    let records = shared_session(session);
    let (_, mut objects) = (Vec::<SessionEvent>::new(), ObjectStream::new());
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut entered = false;
    let mut last_with_player: Option<usize> = None;
    for (i, r) in records.iter().enumerate() {
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
        }
        if objects
            .player()
            .is_some_and(|p| objects.presence(p).is_some())
        {
            last_with_player = Some(i);
        }
    }
    let limit = last_with_player.unwrap_or_else(|| {
        panic!(
            "{session} never created the player's own object, so it cannot be this file's oracle"
        )
    });
    let (events, objects) = replay_to(session, limit);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::PlayerDescription(_))),
        "{session} never reached 0x0013, so it cannot be this test's oracle"
    );
    assert!(
        objects.player().is_some(),
        "the replay to record {limit} of {} lost the player again",
        records.len()
    );
    (events, objects)
}

/// The capture's own player: the id, the setup record the server dressed and the descriptor it
/// dressed it with, as `dereth_animation` wants them.
fn capture_player(objects: &ObjectStream) -> (ObjectId, DataId, dereth_animation::parts::ObjDesc) {
    let id = objects.player().expect("the capture names a player");
    let p = objects
        .presence(id)
        .expect("the capture creates the player's own object");
    let setup = p
        .setup_id
        .expect("the capture's player create carries a setup record");
    let od = dereth_client::world::to_anim_objdesc(&p.objdesc);
    assert!(
        !od.part_changes.is_empty() || !od.texture_changes.is_empty(),
        "the capture's player wears nothing, so this test has no oracle for 'the right gear'"
    );
    (id, setup, od)
}

// =================================================================================================
// The application harness.
// =================================================================================================

fn app_in_gameplay(frames: u32) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the D3D12 device and the shipped UI");
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

fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

/// Open the backpack the way the toolbar's own button does — through the panel stack, not by
/// writing `visible` onto an element.
fn open_the_backpack(app: &mut App) {
    let (ui, screen) = gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

fn close_the_backpack(app: &mut App) {
    let (ui, screen) = gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, false);
}

fn handle_of(app: &mut App, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = *screen.roots().first().expect("root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// The text an element is really showing: the glyph list's `inq_text` result, meaning the glyphs
/// that compose, not a string the panel remembered.
fn glyph_text(app: &mut App, id: ElementId) -> String {
    let h = handle_of(app, id);
    let (ui, _) = gameplay_screen(app);
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The application in gameplay with the capture's world **and** its object table installed, and
/// the backpack open.
fn app_with_the_capture() -> (App, ObjectStream) {
    let (events, objects) = replay(SESSION);
    let mut app = app_in_gameplay(4);
    // The object half first: `apply_hud_events` writes into `objects.world`, so installing the
    // stream afterwards would throw the qualities away.
    *app.objects_mut() = objects;
    let _ = app.apply_hud_events(&events);
    open_the_backpack(&mut app);
    for _ in 0..6 {
        app.frame();
    }
    let objects = replay(SESSION).1;
    (app, objects)
}

// =================================================================================================
// 1. The literals, the ids and the constants
// =================================================================================================

/// The three format literals (`Inventory of %s`, `Contents of Backpack`, `Contents of %s`) and the
/// three child ids the panel looks up: title `0x100001D3`, contents `0x100001C5`, doll `0x100001D5`.
///
/// The `"Contents of %s"` arm matters more than it looks: new-parent-container handling compares
/// the container id with the player id, so opening a *chest* must retitle the grid and
/// opening one's own pack must not. A rebuild that always wrote the literal would look correct in
/// the paired frame and be wrong the moment a container was opened.
#[test]
fn the_two_format_strings_and_the_three_element_ids_are_the_clients_own() {
    assert_eq!(title_text("Aldwynewa"), "Inventory of Aldwynewa");
    assert_eq!(BACKPACK_CONTENTS, "Contents of Backpack");
    assert_eq!(contents_text(None), "Contents of Backpack");
    assert_eq!(contents_text(Some("Chest")), "Contents of Chest");

    assert_eq!(
        TITLE_TEXT,
        ElementId(0x1000_01D3),
        "the inventory title label"
    );
    assert_eq!(
        CONTENTS_TEXT,
        ElementId(0x1000_01C5),
        "the item-grid contents label"
    );
    assert_eq!(
        PAPER_DOLL_VIEWPORT,
        ElementId(0x1000_01D5),
        "the paper-doll child"
    );
}

/// The constants paper-doll initialization uses for its camera and light.
///
/// The z of the camera and the y of the light are asserted as **bit patterns** as well as values:
/// retail's float is `0x3F6147AE`, which a rounded decimal such as `0.87` does not reproduce.
#[test]
fn the_paper_doll_camera_and_light_are_the_floats_post_init_pushes() {
    assert_eq!(pd::CAMERA_POSITION, (0.12, -2.4, 0.88));
    assert_eq!(pd::CAMERA_POSITION.0.to_bits(), 0x3DF5_C28F);
    assert_eq!(pd::CAMERA_POSITION.1.to_bits(), 0xC019_999A);
    assert_eq!(pd::CAMERA_POSITION.2.to_bits(), 0x3F61_47AE);
    assert_eq!(pd::CAMERA_TARGET, (0.0, 0.0, 0.0));

    // `(0.3, +1.9, 0.65)` — **positive** y, like the character-generation view and unlike the
    // portal space.
    assert_eq!(pd::LIGHT_DIRECTION, (0.3, 1.9, 0.65));
    assert_eq!(pd::LIGHT_DIRECTION.1.to_bits(), 0x3FF3_3333);
    assert!(
        pd::LIGHT_DIRECTION.1 > 0.0,
        "the portal space's is -1.9; getting the sign wrong lights the doll from behind"
    );
    assert_eq!(pd::LIGHT_INTENSITY, 2.0);
    assert_eq!(pd::LIGHT_INTENSITY.to_bits(), 0x4000_0000);

    // The redressing pose.
    assert_eq!(pd::HEADING_DEGREES, 191.3679);
    assert_eq!(pd::LOW_FRAME, 1);
    assert_eq!(pd::FRAMERATE, 0.0, "the doll is a held pose, not a loop");

    // The race update has six cases and no default camera update.
    assert_eq!(pd::RACE_CAMERA.len(), 6);
    assert_eq!(
        pd::for_race(1),
        None,
        "an Aluvian falls through and keeps the initialized camera"
    );
    assert_eq!(pd::for_race(6), Some(((0.12, -3.0, 0.88), None)));
    assert_eq!(pd::for_race(8), Some(((0.12, -3.4, 1.0), None)));
    // The two Olthoi heritages swap the animation data id as well as the camera.
    assert_eq!(pd::for_race(12).and_then(|(_, a)| a), Some(0x1000_0011));
    assert_eq!(pd::for_race(13).and_then(|(_, a)| a), Some(0x1000_0013));
}

/// Paper-doll construction assigns animation enum `0x10000005`, and the shipped enum mapper chain
/// maps `0x25000000` → group **7 `UIASSET`** → `0x25000010`.
///
/// The group is what this asserts: `0x10000005` in group 5 answers a `LayoutDesc` and in group 9 a
/// `Font`, and `set_sequence_animation` handed either of those starts nothing and says nothing.
#[test]
fn the_paper_doll_animation_resolves_through_the_uiasset_group_to_an_animation() {
    let store = store();
    let assets: &dyn dereth_primitives::AssetSource = &*store;
    assert_eq!(PAPER_DOLL_ANIMATION_ENUM, 0x1000_0005);
    assert_eq!(
        PAPER_DOLL_ANIMATION_ENUM,
        dereth_client::preview::ENUM_PAPERDOLL_ANIMATION
    );
    let id = dereth_client::assets::enum_did(
        assets,
        dereth_client::preview::UIASSET_GROUP,
        PAPER_DOLL_ANIMATION_ENUM,
    )
    .expect("UIASSET 0x10000005 (PaperDollAnimation) resolves");
    assert_eq!(
        id.0 >> 24,
        0x03,
        "set_sequence_animation wants an Animation, got {id:?}"
    );
    assert!(
        dereth_primitives::AssetSource::exists(&*store, id),
        "{id:?} is in this dat build"
    );
}

// =================================================================================================
// 2. The title and the headings, on the live tree
// =================================================================================================

/// Behaviour: inventory.backpack.the-panel-shows-its-title-headings-and-a-dressed-paper-doll
///
/// **The paired frame's top line and its middle heading, against the capture's own name.**
///
/// The name is never written here: it is the singular object name from the `0xF745` the recorded
/// server sent for the player. The assertion is on the element's **glyph list** — what composes and
/// draws — and then on this frame's draw list, so a panel that remembered the string without ever
/// putting it on an element cannot pass.
///
/// Without `InventoryPanels::set_title`'s element write the glyph list is empty, and without the
/// `set_contents_title` call in `post_init` the heading is empty.
#[test]
fn the_panel_shows_the_title_and_the_heading_the_capture_names() {
    let (mut app, objects) = app_with_the_capture();
    let player = objects.player().expect("the capture names a player");
    let name = objects
        .world
        .weenie(player)
        .map(|w| w.pwd.name.clone())
        .filter(|n| !n.is_empty())
        .expect("the capture's player create carries a name");

    let title = glyph_text(&mut app, TITLE_TEXT);
    let heading = glyph_text(&mut app, CONTENTS_TEXT);
    println!("title {title:?}, heading {heading:?}, capture name {name:?}");
    assert_eq!(title, format!("Inventory of {name}"));
    assert_eq!(heading, "Contents of Backpack");

    // …and both really reached this frame's blit list as glyphs, at the elements that own them.
    let title_h = handle_of(&mut app, TITLE_TEXT);
    let contents_h = handle_of(&mut app, CONTENTS_TEXT);
    app.frame();
    let drawn = |who: ElemHandle| -> usize {
        app.ui_draw_list()
            .iter()
            .filter(|c| c.who == who)
            .map(|c| c.glyphs.len())
            .sum::<usize>()
    };
    let t = drawn(title_h);
    let c = drawn(contents_h);
    println!("{t} title glyphs and {c} heading glyphs in the draw list");
    assert_eq!(
        t,
        title.chars().count(),
        "the title's glyphs are in this frame's draw list"
    );
    assert_eq!(
        c,
        heading.chars().count(),
        "the heading's glyphs are in this frame's draw list"
    );

    // **The burden meter's input, reported rather than assumed:** `HudView::load` feeds the
    // meter a nonzero load level.
    let (_, screen) = gameplay_screen(&mut app);
    let burden = screen
        .inventory
        .burden
        .expect("the burden load level was recorded");
    println!("burden meter position {} and text {}%", burden.0, burden.1);
    assert!(burden.1 > 0, "the burden meter has an input: {burden:?}");
    app.shutdown();
}

/// The other arm of new-parent-container handling: a container that is **not** the player
/// retitles the grid.
///
/// The container is the capture's own — one of the side packs `0x0013` placed — so the string is
/// the recorded server's, not this file's.
#[test]
fn opening_a_side_pack_retitles_the_grid_with_that_containers_name() {
    let (mut app, objects) = app_with_the_capture();
    let player = objects.player().expect("a player");
    let pack = {
        let (_, screen) = gameplay_screen(&mut app);
        screen
            .inventory
            .container_list
            .as_ref()
            .and_then(|w| w.slots.iter().find_map(|s| s.item))
            .expect("the capture's character carries at least one side pack")
    };
    assert_ne!(pack, player, "a side pack is not the player");
    let want = objects
        .world
        .weenie(pack)
        .map(|w| w.pwd.name.clone())
        .filter(|n| !n.is_empty())
        .expect("the capture named the pack");

    {
        let (ui, screen) = gameplay_screen(&mut app);
        assert!(
            screen.inventory.open_container(&mut ui.requests, pack),
            "the item list accepted the container"
        );
    }
    for _ in 0..3 {
        app.frame();
    }
    let heading = glyph_text(&mut app, CONTENTS_TEXT);
    println!("opened {pack:?} ({want:?}); heading is {heading:?}");
    assert_eq!(heading, format!("Contents of {want}"));

    // Back to the player's own pack: the literal returns, which is the `==` arm.
    {
        let (ui, screen) = gameplay_screen(&mut app);
        assert!(screen.inventory.open_container(&mut ui.requests, player));
    }
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(glyph_text(&mut app, CONTENTS_TEXT), "Contents of Backpack");
    app.shutdown();
}

// =================================================================================================
// 3. The doll wears the capture's gear
// =================================================================================================

/// A preview space holding one object built from `setup`, dressed with `objdesc` or not.
fn doll_space(
    store: &Arc<dereth_dat::RetailDatStore>,
    setup: DataId,
    objdesc: Option<&dereth_animation::parts::ObjDesc>,
) -> dereth_client::gpu::Renderer {
    let mut r = dereth_client::gpu::Renderer::new(None, 256, 256).expect("a WARP device comes up");
    let assets = Arc::new(dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(
        store,
    )));
    assert!(
        r.ensure_preview(PreviewId::PaperDoll, &assets),
        "the space is created once"
    );
    let i = r
        .add_preview_object_dressed(PreviewId::PaperDoll, store, setup, objdesc)
        .expect("the object is built")
        .expect("the player's setup record loads");
    assert_eq!(i, 0, "the first appended preview object has index zero");
    r
}

/// **The doll wears the *player's* gear, and the geometry on the GPU is
/// the geometry the descriptor asked for.**
///
/// Two objects from one setup record in one process, differing only in whether visual-description
/// changes were applied before the bake. The comparison is on
/// [`PreviewObject::built_from`] — the `GfxObj` each mesh slot was baked from — and the changed
/// indices are required to be **exactly** the ones the capture's `ObjDesc` names.
///
/// This is the assertion that separates "the model was updated" from "the drawn geometry was":
/// with the `do_obj_desc_changes_from_default` call in `PreviewSpace::add_object_dressed` moved
/// *after* `build_part_meshes`, the part array is right, `built_from` is untouched, and this test
/// fails while every model-level assertion still passes.
#[test]
fn the_paper_doll_wears_the_gear_the_capture_sent() {
    let store = store();
    let (_, objects) = replay(SESSION);
    let (_, setup, od) = capture_player(&objects);

    let naked = doll_space(&store, setup, None);
    let dressed = doll_space(&store, setup, Some(&od));

    let n = naked
        .preview(PreviewId::PaperDoll)
        .expect("space")
        .object(0)
        .expect("object");
    let d = dressed
        .preview(PreviewId::PaperDoll)
        .expect("space")
        .object(0)
        .expect("object");

    assert_eq!(
        n.dressed, None,
        "the undressed object applied no visual-description changes"
    );
    assert_eq!(
        d.dressed,
        Some(true),
        "visual-description changes reported a part index the setup record does not have"
    );
    assert_eq!(
        n.built_from().len(),
        d.built_from().len(),
        "one setup record, one part count -- dropping a part slides every later one onto the wrong bone"
    );
    assert!(
        d.drawn_parts() > 10,
        "a body is more than a handful of parts: {}",
        d.drawn_parts()
    );

    // What the descriptor named, and what actually changed on the GPU.
    let named: BTreeSet<u32> = od.part_changes.iter().map(|c| c.part_index).collect();
    let changed: BTreeSet<u32> = n
        .built_from()
        .iter()
        .zip(d.built_from())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| u32::try_from(i).expect("part index"))
        .collect();
    println!(
        "the capture's player ObjDesc names {} part swaps and {} texture swaps; \
         {} of the baked GfxObjs differ from the naked setup's",
        od.part_changes.len(),
        od.texture_changes.len(),
        changed.len()
    );
    assert!(
        !changed.is_empty(),
        "the ObjDesc changed no geometry at all"
    );
    assert!(
        changed.is_subset(&named),
        "a GfxObj changed at a part the descriptor never named: {:?}",
        changed.difference(&named).collect::<Vec<_>>()
    );
    // The other half of the wardrobe: a tunic that recolours a body part without replacing it.
    // 2,017 of the corpus's 2,967 part swaps name the id the setup already has (see
    // `dereth_animation::parts::objdesc`), so `changed` is a strict subset of `named` and the texture
    // half is where most of the visible difference lives.
    assert!(
        !od.texture_changes.is_empty(),
        "the capture's player has no texture swaps, so the surface half is untested"
    );
    let overrides = d.parts_with_surface_overrides();
    println!("{overrides} parts carry a surface override after the redress");
    assert_eq!(
        n.parts_with_surface_overrides(),
        0,
        "an undressed setup overrides no surface"
    );
    assert!(overrides > 0, "no part carries the ObjDesc's texture swaps");
}

// =================================================================================================
// 4. The pixels
// =================================================================================================

fn viewport_box(app: &mut App) -> Box2D {
    let h = handle_of(app, PAPER_DOLL_VIEWPORT);
    let (ui, _) = gameplay_screen(app);
    ui.screen_clip_box(h)
}

/// **The paired frame: the doll draws, and only where retail draws it.**
///
/// Viewport rendering runs only when the space holds an object, so emptying the space is exactly
/// what a panel with no preview does — and the difference between the two frames is the pass with
/// nothing else changed. Two things are asserted: that the changed pixels are **confined to the
/// viewport** (a pass that cleared colour instead of depth would erase the panel art around it),
/// and that there are enough of them to be a figure rather than a stray triangle.
///
/// **The two frames come from two applications driven the same number of frames, not from two
/// successive frames of one:** the world behind the panel advances with the application's clock,
/// so successive frames differ outside the viewport. Two separately-built runs at the same frame
/// index are deterministic and differ only in the thing under test.
#[test]
fn the_paper_doll_draws_inside_its_viewport_and_nowhere_else() {
    let (events, _) = replay(SESSION);

    let shot = |show: bool| -> (u32, u32, Vec<u8>, Box2D, u64, u64) {
        let mut app = app_in_gameplay(4);
        *app.objects_mut() = replay(SESSION).1;
        let _ = app.apply_hud_events(&events);
        open_the_backpack(&mut app);
        for _ in 0..6 {
            app.frame();
        }
        assert_eq!(
            app.renderer_mut()
                .preview(PreviewId::PaperDoll)
                .map(dereth_client::preview::PreviewSpace::object_count),
            Some(1),
            "one doll, not one per frame"
        );
        if !show {
            app.renderer_mut()
                .preview_mut(PreviewId::PaperDoll)
                .expect("the space exists")
                .remove_all_objects();
        }
        let area = viewport_box(&mut app);
        app.frame();
        let drawn = app.renderer_mut().ui_stats.previews_drawn;
        let empty = app.renderer_mut().ui_stats.previews_empty;
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("the frame captures");
        app.shutdown();
        (w, h, bgra, area, drawn, empty)
    };

    let (w, h, with, area, drawn, _) = shot(true);
    let (w2, h2, without, area2, _, empty) = shot(false);
    assert_eq!((w, h), (w2, h2));
    assert_eq!(area, area2, "the viewport moved between the two runs");
    assert!(
        area.is_valid(),
        "the paper-doll viewport has a box: {area:?}"
    );
    assert!(drawn > 0, "the preview pass ran at least once");
    assert!(
        empty > 0,
        "an empty preview space must be counted as declined rather than drawn"
    );

    let (mut changed, mut outside) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if with[i..i + 3] == without[i..i + 3] {
                continue;
            }
            changed += 1;
            let (px, py) = (x as i32, y as i32);
            if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                outside += 1;
            }
        }
    }
    let box_area = ((area.x1 - area.x0 + 1) * (area.y1 - area.y0 + 1)) as u32;
    println!("the doll changes {changed} pixels of {box_area}, {outside} outside {area:?}");
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the viewport"
    );
    assert!(
        changed > box_area / 20,
        "the doll covers a real part of its viewport: {changed}"
    );
}

/// **The gear reaches the pixels.** Two renders differing only in whether the capture's `ObjDesc`
/// dressed the doll, with every changed pixel inside the viewport.
///
/// A doll that drew the naked setup would pass the test above — it draws, it is confined — and
/// disagree with the retail frame on every garment. This is the differential that separates them,
/// and it is the pixel-level counterpart of `built_from`.
#[test]
fn dressing_the_doll_changes_the_pixels_inside_the_viewport() {
    let (events, objects) = replay(SESSION);
    let (_, setup, od) = capture_player(&objects);

    let shot = |dress: bool| -> (u32, u32, Vec<u8>, Box2D) {
        let mut app = app_in_gameplay(4);
        *app.objects_mut() = replay(SESSION).1;
        let _ = app.apply_hud_events(&events);
        open_the_backpack(&mut app);
        for _ in 0..6 {
            app.frame();
        }
        // Replace the app's own doll with one built the same way but undressed. The rebuild key
        // is `(setup, objdesc)` and neither has changed, so `paper_doll_use_time` leaves it alone.
        //
        // **The heading *and* the sequence animation are re-issued**, not just the heading. With
        // only the heading, the naked doll falls back to the setup's own default animation at
        // 30 fps while the dressed one holds `PaperDollAnimation`'s frame 1, and the two frames
        // differ by the *pose*, not by the gear. Passing `None` for the descriptor in
        // `paper_doll_use_time` makes this test fail.
        if !dress {
            let store = store();
            let a: &dyn dereth_primitives::AssetSource = &*store;
            let anim = dereth_client::assets::enum_did(
                a,
                dereth_client::preview::UIASSET_GROUP,
                PAPER_DOLL_ANIMATION_ENUM,
            )
            .expect("PaperDollAnimation resolves");
            app.renderer_mut()
                .preview_mut(PreviewId::PaperDoll)
                .expect("the space exists")
                .remove_all_objects();
            app.renderer_mut()
                .add_preview_object(PreviewId::PaperDoll, &store, setup)
                .expect("the naked object builds")
                .expect("the setup record loads");
            let space = app
                .renderer_mut()
                .preview_mut(PreviewId::PaperDoll)
                .expect("space");
            space.set_heading(0, pd::HEADING_DEGREES);
            assert!(
                space.set_sequence_animation(0, anim, true, pd::LOW_FRAME, pd::FRAMERATE),
                "the naked doll holds the same pose as the dressed one"
            );
            assert_eq!(space.curr_frame_number(0), 1, "both dolls are on frame 1");
        }
        let area = viewport_box(&mut app);
        app.frame();
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("the frame captures");
        let n = app
            .renderer_mut()
            .preview(PreviewId::PaperDoll)
            .and_then(|s| s.object(0))
            .map(dereth_client::preview::PreviewObject::drawn_parts)
            .expect("a doll either way");
        assert!(n > 10, "the doll drew {n} parts");
        app.shutdown();
        (w, h, bgra, area)
    };

    let (w, h, dressed, area) = shot(true);
    let (w2, h2, naked, area2) = shot(false);
    assert_eq!((w, h), (w2, h2));
    assert_eq!(area, area2, "the viewport moved between the two runs");
    assert!(!od.part_changes.is_empty());

    let (mut changed, mut outside) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if dressed[i..i + 3] == naked[i..i + 3] {
                continue;
            }
            changed += 1;
            let (px, py) = (x as i32, y as i32);
            if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                outside += 1;
            }
        }
    }
    println!("the gear changes {changed} pixels, {outside} outside the viewport {area:?}");
    assert_eq!(
        outside, 0,
        "dressing the doll changed pixels outside its own viewport"
    );
    assert!(
        changed > 200,
        "the gear made almost no difference to the frame: {changed} pixels"
    );
}

// =================================================================================================
// 5. The pose, and the gate
// =================================================================================================

/// Redressing starts the sequence at low frame 1 with frame rate **0.0**.
///
/// The paper doll is a held pose. Sequence advancement adds `framerate * dt`, so a zero frame rate
/// stays on `low_frame` for ever — and a rebuild that "helpfully" ran the animation at the client's
/// default 30 fps would animate a figure retail keeps still. Ten seconds of elapsed time is
/// asserted, which is 300 frames at that default.
#[test]
fn the_doll_holds_frame_one_because_its_framerate_is_zero() {
    let store = store();
    let (_, objects) = replay(SESSION);
    let (_, setup, od) = capture_player(&objects);
    let mut r = doll_space(&store, setup, Some(&od));
    let a: &dyn dereth_primitives::AssetSource = &*store;
    let anim = dereth_client::assets::enum_did(
        a,
        dereth_client::preview::UIASSET_GROUP,
        PAPER_DOLL_ANIMATION_ENUM,
    )
    .expect("PaperDollAnimation resolves");

    let space = r.preview_mut(PreviewId::PaperDoll).expect("the space");
    assert!(
        space.set_sequence_animation(0, anim, true, pd::LOW_FRAME, pd::FRAMERATE),
        "the sequence really started -- an animation the dat does not hold is dropped silently"
    );
    assert_eq!(
        space.curr_frame_number(0),
        1,
        "set_sequence_animation starts at low_frame"
    );
    for _ in 0..600 {
        space.use_time(1.0 / 60.0);
    }
    assert_eq!(
        space.curr_frame_number(0),
        1,
        "ten seconds at 0 fps is still frame 1; a 30 fps doll would be at 300"
    );
}

/// **The gate.** `Renderer::draw_ui`'s fallback arm draws a queued space whose element emitted no
/// blit command — which is right for the portal preview's transparent full-screen region, and
/// would put the doll over the world whenever the backpack was closed.
///
/// So the space is not even created while the page is down, and is created and drawn the moment it
/// comes up. Both directions are asserted, because a test that only checked the closed state would
/// pass on a client that never built the doll at all.
#[test]
fn the_doll_is_not_built_or_drawn_while_the_backpack_is_closed() {
    let (events, _) = replay(SESSION);
    let mut app = app_in_gameplay(4);
    *app.objects_mut() = replay(SESSION).1;
    let _ = app.apply_hud_events(&events);
    for _ in 0..6 {
        app.frame();
    }
    assert!(
        app.renderer_mut().preview(PreviewId::PaperDoll).is_none(),
        "the closed backpack built a preview space it cannot show"
    );
    let before = app.renderer_mut().ui_stats.previews_drawn;

    open_the_backpack(&mut app);
    for _ in 0..6 {
        app.frame();
    }
    assert_eq!(
        app.renderer_mut()
            .preview(PreviewId::PaperDoll)
            .map(dereth_client::preview::PreviewSpace::object_count),
        Some(1),
        "opening the backpack builds the doll"
    );
    let opened = app.renderer_mut().ui_stats.previews_drawn;
    assert!(
        opened > before,
        "the doll drew once the panel was up: {before} -> {opened}"
    );

    close_the_backpack(&mut app);
    for _ in 0..3 {
        app.frame();
    }
    let a = app.renderer_mut().ui_stats.previews_drawn;
    app.frame();
    let b = app.renderer_mut().ui_stats.previews_drawn;
    assert_eq!(a, b, "the doll kept drawing after the backpack was closed");
    app.shutdown();
}

/// **Initialization hides nine icon grids that otherwise stand where the doll goes.**
///
/// It binds each armour-coverage slot, registers its drag handler and tooltip, and then hides the
/// slot; binding them and leaving them up draws nine icon grids over the figure.
///
/// Both directions are asserted. A test that only checked the nine were down would pass on a panel
/// that had hidden the whole page.
///
/// Without the `set_slot_view(ui, false)` call at the end of `InventoryPanels::post_init` this
/// fails.
#[test]
fn post_init_hides_the_nine_armour_grids_and_leaves_the_doll_up() {
    let (mut app, _) = app_with_the_capture();
    let up = |app: &mut App, id: ElementId| -> bool {
        let h = handle_of(app, id);
        let (ui, _) = gameplay_screen(app);
        ui.node(h).is_some_and(|n| n.region.flags.visible)
    };
    for id in dereth_ui_screens::panels::inventory::ARMOUR_COVERAGE_SLOTS {
        assert!(
            !up(&mut app, id),
            "armour slot {id:?} is drawn over the doll"
        );
    }
    for id in dereth_ui_screens::panels::inventory::SIGIL_SLOTS {
        assert!(
            !up(&mut app, id),
            "sigil slot {id:?} is up on a character with no aetheria"
        );
    }
    assert!(
        up(&mut app, PAPER_DOLL_VIEWPORT),
        "the doll's own viewport is down"
    );
    // …and the four slots that are *not* armour coverage stay up, which is the other direction:
    // the neck, the two rings and the ready weapon are visible in retail's frame.
    for id in [
        ElementId(0x1000_01DA),
        ElementId(0x1000_01DC),
        ElementId(0x1000_01DE),
        ElementId(0x1000_01DF),
    ] {
        assert!(up(&mut app, id), "slot {id:?} should still be visible");
    }
    app.shutdown();
}

/// **The *Slots* checkbox.** Element-message handling for message 1 from element
/// `0x100005BE` shows the nine grids and hides the doll when checked; unchecked does the reverse.
/// The two halves are mirror images in the client and are one function here.
///
/// The click is an element message broadcast through `UiSystem`, not a direct call into the panel.
/// Injected mouse events would test a different boundary, while calling the handler directly would
/// not prove the message is wired.
///
/// Without the `self.inventory.on_slot_checkbox(ui, m.source)` line in `GamePlayScreen`'s
/// `BUTTON_CLICKED` arm (a handler that exists but is not wired) this fails.
#[test]
fn the_slots_checkbox_swaps_the_doll_for_the_armour_grids_and_back() {
    let (mut app, _) = app_with_the_capture();
    let cb = handle_of(
        &mut app,
        dereth_ui_screens::panels::inventory::SLOT_CHECKBOX,
    );
    let head = handle_of(&mut app, ElementId(0x1000_05AB));
    let doll = handle_of(&mut app, PAPER_DOLL_VIEWPORT);
    let state = |app: &mut App| -> (bool, bool) {
        let (ui, _) = gameplay_screen(app);
        (
            ui.node(head).is_some_and(|n| n.region.flags.visible),
            ui.node(doll).is_some_and(|n| n.region.flags.visible),
        )
    };
    assert_eq!(
        state(&mut app),
        (false, true),
        "initialized state: the doll, not the grids"
    );

    // Check it, and click. Attribute 0x0E is the checkbox state written before message 1 is raised.
    let click = |app: &mut App, checked: bool| {
        let (ui, _) = gameplay_screen(app);
        dereth_ui_screens::bind::set_attr_bool(
            ui,
            cb,
            dereth_ui_screens::bind::attr::CHECKED,
            checked,
        );
        ui.broadcast_element_message(cb, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
        app.frame();
    };
    click(&mut app, true);
    assert_eq!(
        state(&mut app),
        (true, false),
        "checked: the grids, not the doll"
    );
    click(&mut app, false);
    assert_eq!(
        state(&mut app),
        (false, true),
        "unchecked: back to the doll"
    );
    app.shutdown();
}

/// **The live space carries the initialized camera and light and the redressing heading.**
///
/// The three constants above are asserted against retail's values; this asserts that the *running*
/// panel actually applied them, which is the other half. The heading is checked as "not identity",
/// not against a recomputed quaternion: comparing `set_heading(191.3679)` with `set_heading(HEADING)`
/// would agree with itself whatever either did, and 191.3679° is the whole reason the doll is not
/// seen from behind.
#[test]
fn the_live_space_carries_the_camera_the_light_and_the_heading() {
    let (mut app, _) = app_with_the_capture();
    let space = app
        .renderer_mut()
        .preview(PreviewId::PaperDoll)
        .expect("the doll's space");
    let p = space.mode.view_frame.origin;
    assert_eq!(
        (p.x, p.y, p.z),
        pd::CAMERA_POSITION,
        "the space is not at the initialized paper-doll camera"
    );
    assert!(
        space.mode.use_sharp_mode,
        "sharp preview mode is the final initialized paper-doll setting"
    );
    assert_eq!(
        space.mode.lights.len(),
        1,
        "light initialization replaces the list with exactly one"
    );
    let l = &space.mode.lights[0];
    assert_eq!(l.intensity, pd::LIGHT_INTENSITY);
    assert_eq!(
        (l.direction.x, l.direction.y, l.direction.z),
        pd::LIGHT_DIRECTION,
        "the light points the portal space's way, which lights the doll from behind"
    );
    let q = space.object(0).expect("the doll").frame.rotation;
    assert!(
        (q.w.abs() - 1.0).abs() > 1e-3,
        "the doll's heading was never set: orientation is identity, so it faces +Y and the \
         camera looks at its back"
    );
    app.shutdown();
}
