//! The chase camera keeps the combat target framed. With the `ViewCombatTarget` option on, in
//! melee or missile mode, with an attackable creature selected, the camera takes
//! `LOOK_AT_PIVOT | LOOK_AT_OBJECT` and names the selection; each of the four refusals (option off,
//! not in a combat mode, nothing selected, not attackable) sends a zero-target update, which is not
//! a no-op: it clears the targeting and restores the ordinary `ALIGN_WITH_PLANE` chase camera.
//! Tracking is re-evaluated on three edges: an option change (only when its value changes), a
//! selection change, and a combat-mode change. Fixture: a headless `App` with a body on the retail
//! `DEFAULT_LANDBLOCK`, a seeded player and creature, read back through the camera's own
//! `target_status` mask and `target_object_id` (the framing itself is not pixel-checked).

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::app::App;
use dereth_client::camera::target;
use dereth_client::config::Config;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_model::combat::CombatMode;
use dereth_primitives::ObjectId;

const PLAYER: ObjectId = ObjectId(0x5000_0002);
const MONSTER: ObjectId = ObjectId(0x8000_0777);

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

/// A real App with a real body on real terrain — the camera under test is that body's own
/// `CameraControl`, driven by `App::frame`.
fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("a headless App over the retail dats");
    app.start_shell().expect("the input shell starts");
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and a real physics body");
    frames(&mut app, 30);
    app
}

fn weenie(w: &mut dereth_client_model::World, id: ObjectId) -> &mut dereth_client_model::Weenie {
    if w.tables.weenies.get(id).is_none() {
        w.tables
            .weenies
            .insert(id, dereth_client_model::Weenie::new(id));
    }
    w.tables.weenies.get_mut(id).expect("just inserted")
}

/// The player and one creature. For a non-player, non-pet creature, attackability is bit 4 of the
/// object's bitfield, so without `ATTACKABLE` the last gate refuses.
fn seed(app: &mut App, attackable: bool) {
    let w = &mut app.objects_mut().world;
    w.player = Some(PLAYER);
    weenie(w, PLAYER).pwd.name = "Aldis".into();
    {
        let m = weenie(w, MONSTER);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        if attackable {
            m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        } else {
            m.pwd.bitfield &= !dereth_client_model::weenie::bitfield::ATTACKABLE;
        }
    }
    w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
}

/// `ViewCombatTarget`, option ordinal 7, whose side effect is target tracking.
fn set_tracking(app: &mut App, on: bool) {
    let w = &mut app.objects_mut().world;
    w.player_system
        .options
        .set(dereth_client_model::player::option::VIEW_COMBAT_TARGET, on);
}

fn camera_target(app: &App) -> (u32, ObjectId) {
    let c = app
        .world_state()
        .expect("a scene")
        .character
        .as_ref()
        .expect("a body, and therefore camera state");
    (
        c.camera.manager.target_status,
        c.camera.manager.target_object_id,
    )
}

/// Behaviour: camera.target.the-chase-camera-frames-an-attackable-target-in-melee
/// Option on, melee, an attackable selection: the camera frames the target.
#[test]
fn the_camera_frames_an_attackable_target_in_melee_mode() {
    let mut app = setup();
    seed(&mut app, true);
    set_tracking(&mut app, true);
    app.objects_mut().world.combat.combat_mode = CombatMode::Melee;
    frames(&mut app, 3);

    let (mask, id) = camera_target(&app);
    assert_ne!(
        mask & target::LOOK_AT_OBJECT,
        0,
        "LOOK_AT_OBJECT must be set -- the camera update reads it to add the pivot-to-target \
         vector. mask = {mask:#06X}"
    );
    assert_ne!(
        mask & target::LOOK_AT_PIVOT,
        0,
        "the targeted offset branch sets LOOK_AT_PIVOT|LOOK_AT_OBJECT together, not one of them. \
         mask = {mask:#06X}"
    );
    assert_eq!(
        mask & target::ALIGN_WITH_PLANE,
        0,
        "the targeting branch RETURNS -- it never reaches ALIGN_WITH_PLANE. mask = {mask:#06X}"
    );
    assert_eq!(
        id, MONSTER,
        "the camera target-object update names the selection"
    );
}

/// **The four refusal states.** Each starts from a fresh application and proves that its refusal
/// converges on a zero-target update instead of framing a target. The single-session test below
/// proves that a zero-target update *releases* an already framed target; a build that only ever
/// set the bit would pass the test above but fail that one.
#[test]
fn every_gate_that_refuses_releases_the_camera() {
    // 1. the option off.
    let mut app = setup();
    seed(&mut app, true);
    set_tracking(&mut app, false);
    app.objects_mut().world.combat.combat_mode = CombatMode::Melee;
    frames(&mut app, 3);
    let (mask, id) = camera_target(&app);
    assert_eq!(
        mask & target::LOOK_AT_OBJECT,
        0,
        "ViewCombatTarget off: mask = {mask:#06X}"
    );
    assert_eq!(
        id.0, 0,
        "ViewCombatTarget off: no target was ever handed to the camera"
    );

    // 2. the option on, but not in a combat mode: only melee (2) and missile (4) are accepted;
    //    `NonCombat` is 1.
    let mut app = setup();
    seed(&mut app, true);
    set_tracking(&mut app, true);
    app.objects_mut().world.combat.combat_mode = CombatMode::NonCombat;
    frames(&mut app, 3);
    let (mask, id) = camera_target(&app);
    assert_eq!(
        mask & target::LOOK_AT_OBJECT,
        0,
        "NonCombat: mask = {mask:#06X}"
    );
    assert_eq!(
        id.0, 0,
        "NonCombat: no target was ever handed to the camera"
    );

    // 3. in melee with nothing selected: the attack-target lookup returns zero.
    let mut app = setup();
    seed(&mut app, true);
    set_tracking(&mut app, true);
    app.objects_mut().world.combat.combat_mode = CombatMode::Melee;
    app.objects_mut()
        .world
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    frames(&mut app, 3);
    let (mask, id) = camera_target(&app);
    assert_eq!(
        mask & target::LOOK_AT_OBJECT,
        0,
        "nothing selected: mask = {mask:#06X}"
    );
    assert_eq!(
        id.0, 0,
        "nothing selected: no target was ever handed to the camera"
    );

    // 4. in melee with something selected that is not attackable.
    let mut app = setup();
    seed(&mut app, false);
    set_tracking(&mut app, true);
    app.objects_mut().world.combat.combat_mode = CombatMode::Melee;
    frames(&mut app, 3);
    let (mask, id) = camera_target(&app);
    assert_eq!(
        mask & target::LOOK_AT_OBJECT,
        0,
        "unattackable selection: mask = {mask:#06X}"
    );
    assert_eq!(
        id.0, 0,
        "unattackable selection: no target was ever handed to the camera"
    );
}

/// **The release is an edge, not a steady state** — one `App`, the target framed and then let go,
/// so the transition is seen in both directions in the same process.
///
/// A harness that only ever reads one state word cannot observe a transition, and the
/// zero-target update's whole job is the transition.
#[test]
fn dropping_the_target_in_one_session_restores_the_chase_camera() {
    let mut app = setup();
    seed(&mut app, true);
    set_tracking(&mut app, true);
    app.objects_mut().world.combat.combat_mode = CombatMode::Melee;
    frames(&mut app, 3);
    let (framed, framed_id) = camera_target(&app);
    assert_ne!(
        framed & target::LOOK_AT_OBJECT,
        0,
        "station 1: framed. mask = {framed:#06X}"
    );
    assert_eq!(framed_id, MONSTER);

    // Sheathe. A combat-mode change is one of the three target-tracking edges.
    app.objects_mut().world.combat.combat_mode = CombatMode::NonCombat;
    frames(&mut app, 3);
    let (released, released_id) = camera_target(&app);
    assert_eq!(
        released & target::LOOK_AT_OBJECT,
        0,
        "station 2: released. mask = {released:#06X}"
    );
    assert_ne!(
        released & target::ALIGN_WITH_PLANE,
        0,
        "releasing must restore the ordinary chase camera, not merely stop framing: \
         the untargeted offset branch is the one that sets ALIGN_WITH_PLANE. \
         mask = {released:#06X}"
    );
    assert_ne!(
        released, framed,
        "the mask must actually change -- equal masks at both stations would mean the instrument \
         is reading something the tracker does not write"
    );

    // **The stale id is retail's, and this assertion records it rather than fixing it.** Retail's
    // camera target tracking behaves as follows:
    //
    // ```text
    // target id == 0: targeting = false, then recompute the target offset without replacing id
    // target id != 0: targeting = true, then replace the camera's target object
    // ```
    //
    // The zero path does not replace the target object, so `target_object_id` keeps the last target
    // it was given. That is unobservable precisely because the mask gates it: the camera
    // manager's `update_camera` reads `target_object_id` only under
    // `target_status & LOOK_AT_OBJECT`, which the line above has just asserted is clear. There is
    // no clear of the id on deselect.
    assert_eq!(
        released_id, framed_id,
        "retail leaves the id where it was; only the mask is cleared on the zero-target path"
    );
}
