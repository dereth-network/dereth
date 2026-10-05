//! A drag started after a display-size change keeps the source tile's extent: the ghost is the
//! icon-sized quad under the cursor with clamped UVs at 800x600, 1920x1080 and 3840x2160, it is the
//! last child of the UI root, tooltips keep their size, and a resize during a drag still re-anchors
//! the ghost. Fixture: a headless `App` with no network endpoint on the shipped gameplay screen, a
//! hand-seeded backpack, and inventory drags driven through `UiSystem` mouse events.
//!
//! # How the drag proxy is built
//!
//! Retail copies the source description into an element with no parent, matches the source
//! element and broadcasts element message 0x14. Assigning the requested null parent to that
//! unparented element is a same-parent no-op, so parent-size adjustment does not run. Drag startup
//! marks the proxy temporary, owned and root-flagged, moves it to the mouse position minus the drag
//! offset, and moves it to the tail of the UI root's child list; that list operation changes
//! neither its parent nor its box.
//!
//! For an unparented or root-flagged element, parent-size adjustment uses the layout's authored
//! design bounds (width/height minus one) against the live display, not the parent's boxes. A new
//! drag therefore bypasses the adjustment, while a later display-size cascade reaches it through
//! the child list. Were a fresh proxy adjusted, its four AnchorStart edges would keep left/top and
//! add the display delta to right/bottom:
//!
//! | display | dw | dh | adjusted ghost |
//! |---|---|---|---|
//! | 800x600 | 0 | 0 | 32x32 |
//! | 1920x1080 | 1120 | 480 | 1152x512 |
//! | 3840x2160 | 3040 | 1560 | 3072x1592 |
//!
//! `ui_draw::quad` divides the destination extent by the picture size, not the element size, and
//! region blitting uses min(source, destination) extents over a repeating modulo grid rather than
//! scaling. A 3072-pixel destination over a 32-pixel icon spans u=0..96; crossing texture seams
//! selects wrap sampling and repeats the icon, so an oversized ghost draws as a tiled rectangle.
//!
//! `UiSystem::bring_child_to_top` links the proxy at the root-list tail without re-anchoring it.
//! Unlike retail's null parent, it records `Some(root)` for deletion, hit-testing and screen-origin
//! bookkeeping; the root begins at (0,0), so screen coordinates are preserved.
//!
//! # What the tests measure
//!
//! Each drag calls `UiSystem` mouse_down/mouse_move on an actual inventory tile. Some tests
//! inspect the proxy box; others capture `UiSystem::draw` commands and derive quad UVs and sampler
//! choices. These connected layers are not independent pixel oracles. A widened-command negative
//! must produce wrap on both axes, while a fresh icon-sized drag clamps and spans one texture copy.
//! Separate checks cover root-list position, sibling root flags, tooltips, and the retained
//! resize-during-drag behaviour.

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::layout::EdgeMode;
use dereth_ui::region::Box2D;
use dereth_ui::{ElemHandle, ElementId, UiDrawCmd, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const GEM: ObjectId = ObjectId(0x5000_0021);

/// The inventory page in the shipped panel stack.
const INVENTORY_PAGE: ElementId = ElementId(0x1000_018B);

/// SurfaceOp::ICON_EXTENT is the composited drag-picture size returned by ui_draw::composite.
/// Quad UVs divide by this picture size; a 3072-pixel destination over 32 pixels spans 96 copies.
const ICON: (u32, u32) = (
    dereth_ui::region::SurfaceOp::ICON_EXTENT,
    dereth_ui::region::SurfaceOp::ICON_EXTENT,
);

/// A 3840x2160 display extent.
const MONITOR: (u32, u32) = (3840, 2160);

// =============================================================================================
// Harness
// =============================================================================================

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.frame();
    }
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

fn open_inventory(app: &mut App) {
    let panel_id = {
        let (_ui, screen) = gameplay(app);
        screen
            .panels
            .pages
            .iter()
            .find(|q| q.element == INVENTORY_PAGE)
            .map(|q| q.panel_id)
            .expect("the inventory page 0x1000018B is in the panel stack")
    };
    let (ui, screen) = gameplay(app);
    screen.recv_set_panel_visibility(ui, panel_id, true);
    app.frame();
}

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    (ox + b.width() / 2, oy + b.height() / 2)
}

fn grid_slot_of(app: &mut App, item: ObjectId) -> ElemHandle {
    let (_ui, screen) = gameplay(app);
    let grid = screen
        .inventory
        .item_list
        .as_ref()
        .expect("the backpack grid");
    grid.slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} is in the shipped grid"))
        .handle
}

fn seed(app: &mut App) {
    let w = &mut app.probe_mut().objects_mut().world;
    w.player = Some(PLAYER);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    let mut me = dereth_client_model::Weenie::new(PLAYER);
    me.valid = true;
    me.pwd.name = "Larktest".into();
    w.tables.weenies.insert(PLAYER, me);
    let mut gem = dereth_client_model::Weenie::new(GEM);
    gem.valid = true;
    gem.pwd.name = "Peridot".into();
    gem.pwd.container_id = Some(PLAYER);
    gem.pwd.stack_size = Some(1);
    gem.pwd.icon_id = 0x0600_1234;
    w.tables.weenies.insert(GEM, gem);
    w.tables
        .inventories
        .get_mut(PLAYER)
        .expect("seeded")
        .add_content(GEM, false, 0);
    let p = w.tables.weenies.get_mut(PLAYER).expect("the player weenie");
    p.pwd.items_capacity = Some(102);
    p.pwd.containers_capacity = Some(7);
    p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
}

/// The station: a headless `App` at the authored 800x600 with the backpack open and one item in it.
fn station() -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this station's floor: none at {}",
        client_dir().display()
    );
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir().join("dereth-drag-after-resize/preferences.ini"),
        ..Config::default()
    })
    .expect("an App with the retail dats and a headless GPU device");
    app.start_shell().expect("the shell comes up");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4);
    seed(&mut app);
    open_inventory(&mut app);
    frames(&mut app, 2);
    app
}

/// Exercise the headless presentation change's renderer/UI halves: Renderer::resize, then
/// UiShell::set_display using the actual resized extent. This does not invoke an OS fullscreen
/// transition or change window styles/position; that window-system boundary is outside this
/// module.
fn go_full_screen(app: &mut App, (w, h): (u32, u32)) {
    app.renderer_mut()
        .resize(w, h)
        .expect("the swap chain resizes");
    let (aw, ah) = app.renderer().size();
    assert_eq!((aw, ah), (w, h), "the back buffer really is the new extent");
    app.ui_mut().expect("the shell").set_display((
        i32::try_from(aw).unwrap_or(i32::MAX),
        i32::try_from(ah).unwrap_or(i32::MAX),
    ));
    frames(app, 2);
    let (ui, _s) = gameplay(app);
    assert_eq!(
        ui.display(),
        (i32::try_from(w).unwrap_or(0), i32::try_from(h).unwrap_or(0)),
        "and the element manager agrees with the new display extent"
    );
}

/// Press on the source tile, then move eight pixels on both axes, exceeding the drag threshold.
fn pick_up(app: &mut App, from: ElemHandle, t: f64) -> (i32, i32) {
    let (ui, _s) = gameplay(app);
    let (fx, fy) = centre(ui, from);
    ui.mouse_move(LocalTime(t), fx, fy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, fx, fy);
    ui.mouse_move(LocalTime(t + 0.05), fx + 8, fy + 8);
    app.frame();
    let (ui, _s) = gameplay(app);
    assert!(
        ui.drag_state().element.is_some(),
        "the press started a drag with a proxy"
    );
    (fx + 8, fy + 8)
}

fn proxy(app: &mut App) -> ElemHandle {
    let (ui, _s) = gameplay(app);
    ui.drag_state().element.expect("a live drag proxy")
}

/// The ghost's box, in absolute screen coordinates.
fn ghost_box(app: &mut App, h: ElemHandle) -> Box2D {
    let (ui, _s) = gameplay(app);
    ui.screen_box(h)
}

/// Capture the ghost's UiSystem::draw command in RecordingDrawBackend. Require a command
/// for this handle, so inspecting its stored box alone cannot pass when no command is emitted.
/// This records UI output; it is not a GPU pixel readback.
fn ghost_cmd(app: &mut App, h: ElemHandle) -> UiDrawCmd {
    let (ui, _s) = gameplay(app);
    let mut rec = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut rec);
    rec.calls
        .into_iter()
        .find(|c| c.who == h)
        .expect("the drag ghost reaches the draw pass")
}

/// The tile the drag started from, so the ghost's expected size is measured rather than asserted
/// as a magic 32.
fn tile_size(app: &mut App, tile: ElemHandle) -> (i32, i32) {
    let (ui, _s) = gameplay(app);
    let b = ui.node(tile).expect("the tile is alive").region.box_;
    (b.width(), b.height())
}

// =============================================================================================
// 1. The control: windowed at the authored size
// =============================================================================================

/// At authored 800x600, layout and display reference extents match. Parent-size adjustment
/// has dw=dh=0, so even an adjusted proxy would keep its size here.
#[test]
fn windowed_the_ghost_is_the_icon_sized_quad_under_the_cursor() {
    let _g = gpu_lock();
    let mut app = station();
    let tile = grid_slot_of(&mut app, GEM);
    let want = tile_size(&mut app, tile);
    let at = pick_up(&mut app, tile, 1.0);
    let p = proxy(&mut app);
    let b = ghost_box(&mut app, p);
    assert_eq!(
        (b.width(), b.height()),
        want,
        "windowed, the ghost is the size of the tile it came from"
    );
    assert!(
        b.contains(at.0, at.1) || (b.x0 - at.0).abs() <= want.0 && (b.y0 - at.1).abs() <= want.1,
        "and it sits under the cursor at {at:?}, not somewhere else: {b:?}"
    );
}

// =============================================================================================
// 2. A drag started after the switch to full screen
// =============================================================================================

/// Behaviour: ui.drag.a-drag-ghost-keeps-the-icon-extent-after-a-resize
///
/// Resize the headless display before starting a drag. An adjusted proxy would be 1152x512 at
/// 1920x1080: 32+dw by 32+dh. Compare the fresh proxy to the actual source tile.
#[test]
fn after_going_full_screen_a_fresh_drag_is_still_the_icon_sized_quad() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, (1920, 1080));
    let tile = grid_slot_of(&mut app, GEM);
    let want = tile_size(&mut app, tile);
    let at = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let b = ghost_box(&mut app, p);
    assert_eq!(
        (b.width(), b.height()),
        want,
        "the drag ghost must be the tile's size at any display size, not the tile plus the \
         display delta. Got {b:?} at display 1920x1080 with the cursor at {at:?}: \
         the proxy must retain the source tile's extent"
    );
}

/// Repeat at a 3840x2160 display extent.
#[test]
fn the_owners_3840x2160_is_the_same_answer() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, MONITOR);
    let tile = grid_slot_of(&mut app, GEM);
    let want = tile_size(&mut app, tile);
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let b = ghost_box(&mut app, p);
    assert_eq!(
        (b.width(), b.height()),
        want,
        "at 3840x2160 the ghost is the tile's size, not 3072 x 1592 (tile plus dw 3040, dh 1560)"
    );
}

/// Inspect UiSystem::draw's emitted destination and clip rectangles, the data supplied to
/// Renderer::draw_ui. This verifies command geometry, not that every intended pixel was painted.
#[test]
fn the_draw_command_the_ghost_emits_is_the_same_small_rectangle() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, (1920, 1080));
    let tile = grid_slot_of(&mut app, GEM);
    let want = tile_size(&mut app, tile);
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let cmd = ghost_cmd(&mut app, p);
    assert_eq!(
        (cmd.screen.width(), cmd.screen.height()),
        want,
        "the emitted UiDrawCmd's destination rectangle: {:?}",
        cmd.screen
    );
    assert_eq!(
        cmd.clip, cmd.screen,
        "and nothing clips it, because it is nowhere near an edge: clip {:?}",
        cmd.clip
    );
}

// =============================================================================================
// 3. The sampler and the UVs
// =============================================================================================

/// Derive the quad from the emitted command. ui_draw::quad divides by picture extent, so an
/// unclipped icon-sized destination spans u,v=0..1 and selects clamp on both axes. An adjusted
/// destination would be 96 picture-widths across; both axes would cross seams and select wrap.
/// The next test independently widens the same command to demonstrate that nonconstant answer.
#[test]
fn the_ghosts_uvs_span_one_copy_of_the_icon_and_the_sampler_clamps() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, MONITOR);
    let tile = grid_slot_of(&mut app, GEM);
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let cmd = ghost_cmd(&mut app, p);
    let q = dereth_client_shell::ui_draw::quad(&cmd, MONITOR, ICON).expect("the ghost rasterises");
    assert_eq!(
        q.wrap,
        (false, false),
        "both axes must clamp. `wrap` true is `TEXADDRESS_WRAP`, i.e. the icon repeating across \
         the quad. Destination {:?} over a {:?} picture",
        cmd.screen,
        ICON
    );
    // And the UV span itself, read out of the emitted vertices: `u1 - u0` is
    // `dest_width / picture_width`, so one copy is 1.0 and an adjusted ghost would be 96.0.
    let (u0, u1) = (uv_of(&q.vertices, 0).0, uv_of(&q.vertices, 2).0);
    let (v0, v1) = (uv_of(&q.vertices, 0).1, uv_of(&q.vertices, 2).1);
    assert!(
        (u1 - u0 - 1.0).abs() < 1e-4 && (v1 - v0 - 1.0).abs() < 1e-4,
        "one copy of the picture and no more: u {u0}..{u1}, v {v0}..{v1}"
    );
}

/// The negative control for the test above: the sampler rule it asserts really can answer
/// `WRAP`, so a green `(false, false)` is a measurement rather than a constant. This is the *same*
/// command with the destination widened to what parent-size adjustment would produce.
#[test]
fn the_same_rule_answers_wrap_for_the_quad_the_defect_drew() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, MONITOR);
    let tile = grid_slot_of(&mut app, GEM);
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let mut cmd = ghost_cmd(&mut app, p);
    // Reconstruct 32+dw by 32+dh at 3840x2160. For each axis the reference-box difference is
    // (new end - new start) - (old end - old start), preserving inclusive-bound arithmetic.
    let (dw, dh) = (3839 - 799, 2159 - 599);
    cmd.screen = Box2D::new(
        cmd.screen.x0,
        cmd.screen.y0,
        cmd.screen.x1 + dw,
        cmd.screen.y1 + dh,
    );
    cmd.clip = cmd.screen;
    let q = dereth_client_shell::ui_draw::quad(&cmd, MONITOR, ICON).expect("it rasterises");
    assert_eq!(
        q.wrap,
        (true, true),
        "the widened rectangle must still read as WRAP; otherwise the test above proves \
         nothing"
    );
}

// =============================================================================================
// 4. Mechanism: tail insertion without parent-size adjustment
// =============================================================================================

/// Require the proxy at the UI root's child-list tail, with current parent bookkeeping set.
/// Tail order supports drawing over earlier siblings; the assertion itself checks list order.
#[test]
fn the_proxy_is_pushed_to_the_tail_of_the_roots_child_list() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, (1920, 1080));
    let tile = grid_slot_of(&mut app, GEM);
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let (ui, _s) = gameplay(&mut app);
    let root = ui.root();
    let kids = ui.children(root);
    assert_eq!(
        kids.last().copied(),
        Some(p),
        "the ghost is last in the root's child list"
    );
    assert_eq!(
        ui.parent(p),
        Some(root),
        "and the tree knows where it lives"
    );
}

/// Compare the element's stored box with the source tile before and after resizing. This
/// supplements the draw-command check with the upstream geometry; the two observations are
/// connected and can share a defect, so neither is described as an independent pixel oracle.
#[test]
fn the_proxy_keeps_the_tiles_own_extent_across_the_resize() {
    let _g = gpu_lock();
    let mut app = station();
    let tile = grid_slot_of(&mut app, GEM);
    let windowed = tile_size(&mut app, tile);
    go_full_screen(&mut app, MONITOR);
    let tile = grid_slot_of(&mut app, GEM);
    let full = tile_size(&mut app, tile);
    assert_eq!(
        windowed, full,
        "the inventory tile itself does not change size with the display — so neither may its ghost"
    );
    let _ = pick_up(&mut app, tile, 2.0);
    let p = proxy(&mut app);
    let (ui, _s) = gameplay(&mut app);
    let n = ui.node(p).expect("the proxy is alive");
    assert!(
        n.flags.is_root_element(),
        "the drag proxy retains its root-element flag"
    );
    assert_eq!(
        (n.region.box_.width(), n.region.box_.height()),
        full,
        "and being a root no longer costs it its size"
    );
}

// =============================================================================================
// 5. The siblings — every other cached-extent consumer in the UI tree
// =============================================================================================

/// Count root-flagged elements in the resized gameplay tree, those with any non-Fixed edge,
/// and those whose final box equals the display. Root-flagged parent-size adjustment uses
/// authored-layout versus live-display bounds. The non-Fixed count describes configured
/// responsiveness, not a per-element before/after movement trace.
#[test]
fn every_root_flagged_element_is_re_anchored_by_the_resize() {
    let _g = gpu_lock();
    let mut app = station();
    go_full_screen(&mut app, (1920, 1080));
    let (ui, s) = gameplay(&mut app);
    // Each drop-down on the Client Options page owns a popup root: the page's own count of them.
    let popups = s.config_page.menu_popups;
    assert!(
        popups >= 8,
        "the page's eight retail drop-downs at least: {popups}"
    );
    let root = ui.root();
    let mut stack = vec![root];
    let (mut roots, mut moving, mut stretched_to_display) = (0, 0, 0);
    while let Some(h) = stack.pop() {
        if let Some(n) = ui.node(h) {
            if n.flags.is_root_element() {
                roots += 1;
                let e = n.desc.edges;
                if e.left != EdgeMode::Fixed
                    || e.top != EdgeMode::Fixed
                    || e.right != EdgeMode::Fixed
                    || e.bottom != EdgeMode::Fixed
                {
                    moving += 1;
                }
                if n.region.box_.width() == 1920 && n.region.box_.height() == 1080 {
                    stretched_to_display += 1;
                }
            }
        }
        stack.extend(ui.children(h));
    }
    // The gameplay tree has the UI root 0x8, the gameplay root 0x10000495, the chat root
    // 0x1000001C, and one 0x10000357 popup for each drop-down of the Client Options page. A
    // separately live tooltip 0x10000395 would add one more root with four Fixed edges. Neither
    // tooltip nor drag proxy is created in this test.
    assert_eq!(
        roots,
        3 + popups,
        "the root-flagged census of a live gameplay tree"
    );
    assert_eq!(
        moving, 3,
        "and only three of them have a non-Fixed edge, i.e. only three follow the display at all: \
         the hollow root, the gameplay screen and the chat window"
    );
    assert_eq!(
        stretched_to_display, 2,
        "two of those three are supposed to be the whole screen — the hollow root and the \
         gameplay screen. The drag proxy must not be a fourth"
    );
}

/// A tooltip is another dynamically constructed root after display resize. Its shipped
/// frame 0x10000395 has four Fixed edges, so apply_live_mode_zero preserves live coordinates
/// despite display deltas; explicit resize_to then fits the text. Compare the same tooltip's
/// extents before/after resize rather than assuming that all dynamic roots behave like the proxy.
#[test]
fn a_tooltip_built_after_the_resize_is_the_same_size_as_one_built_before() {
    let _g = gpu_lock();
    let mut app = station();
    let tile = grid_slot_of(&mut app, GEM);
    let before = {
        let (ui, _s) = gameplay(&mut app);
        ui.set_tooltip(tile, Some("Peridot".into()));
        let (cx, cy) = centre(ui, tile);
        ui.mouse_move(LocalTime(1.0), cx, cy);
        let tip = ui
            .start_tooltip_at_mouse(tile, 5.0)
            .expect("the shipped tooltip frame");
        let b = ui.node(tip).expect("alive").region.box_;
        ui.reset_tooltip();
        (b.width(), b.height())
    };
    go_full_screen(&mut app, MONITOR);
    let tile = grid_slot_of(&mut app, GEM);
    let after = {
        let (ui, _s) = gameplay(&mut app);
        let (cx, cy) = centre(ui, tile);
        ui.mouse_move(LocalTime(9.0), cx, cy);
        let tip = ui
            .start_tooltip_at_mouse(tile, 5.0)
            .expect("the shipped tooltip frame");
        let b = ui.node(tip).expect("alive").region.box_;
        (b.width(), b.height())
    };
    assert_eq!(
        before, after,
        "the tooltip frame is Fixed on all four edges, so the display extent cannot reach it"
    );
}

// =============================================================================================
// 6. A resize during a drag
// =============================================================================================

/// A resize during an existing drag re-anchors the ghost as retail does.
///
/// Retail's resize traversal visits every child and applies parent-size adjustment. The drag
/// proxy is already in the root's list and root-flagged, so it uses layout-design versus live
/// display bounds and grows by dw/dh. The client keeps that result, independently of the
/// fresh-drag creation path; the exact growth equation below pins the boundary.
#[test]
fn a_resize_during_a_drag_still_re_anchors_the_ghost_exactly_as_retails_resizeto_cascade_does() {
    let _g = gpu_lock();
    let mut app = station();
    let tile = grid_slot_of(&mut app, GEM);
    let (tw, th) = tile_size(&mut app, tile);
    let _ = pick_up(&mut app, tile, 1.0);
    go_full_screen(&mut app, (1920, 1080));
    let p = proxy(&mut app);
    let b = ghost_box(&mut app, p);
    let (dw, dh) = (1919 - 799, 1079 - 599);
    assert_eq!(
        (b.width(), b.height()),
        (tw + dw, th + dh),
        "the resize cascade reaches the proxy through the root's child list; \
         this is that arithmetic and it is deliberate"
    );
}

// =============================================================================================
// Helpers
// =============================================================================================

/// The `(u, v)` of vertex `i` of a `UiQuad`'s six, which are `UI_VERTEX_BYTES` apart with the two
/// floats at offset 16.
fn uv_of(vertices: &[u8], i: usize) -> (f32, f32) {
    let at = i * dereth_client_shell::ui_draw::UI_VERTEX_BYTES + 16;
    let u = f32::from_le_bytes([
        vertices[at],
        vertices[at + 1],
        vertices[at + 2],
        vertices[at + 3],
    ]);
    let v = f32::from_le_bytes([
        vertices[at + 4],
        vertices[at + 5],
        vertices[at + 6],
        vertices[at + 7],
    ]);
    (u, v)
}
