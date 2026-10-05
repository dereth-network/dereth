//! The frame's interaction step runs the object range checks: an open vendor closes once the player
//! is out of its range, and with no local body the checks are skipped rather than run. The
//! selection range watch arms at the radar radius the player's own cell chooses: 75 m on the
//! landscape, 25 m in an interior cell, and back to 75 m when the body walks out again.
//! Fixture: `dereth_client_runtime::interaction::use_time` (the step `App::frame` calls) over a real
//! `WorldScene` and character body (and an interior cell of the default landblock) on a headless
//! WARP device; the retail dats; no window, no input.
//!
//! # Order and both directions
//!
//! The range checks run as the last thing in the player-module time step, immediately after
//! `Interaction::run_player_module_use_time`. Two registrations are armed over the same frames:
//!
//! * the **vendor**, whose object has no physics body: the range query answers 0 when either
//!   object has no body, so it is out of range and must close;
//! * a **control** watch on the player's own id, whose body is the character's and whose distance
//!   to itself is 0: it must survive.
//!
//! Without the control, "everything closed" and "the geometry answered `None` for everything" are
//! the same observation. The first frame is driven *before* the one-second poll interval has
//! elapsed, so "the frame closed it" is separated from "the frame closes things".

#![cfg(gpu)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_model::range::RangeHandler;
use dereth_client_runtime::character::PLAYER_OBJECT_ID;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::types::PublicWeenieDesc;
use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const VENDOR: ObjectId = ObjectId(0x8000_0001);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn put(w: &mut dereth_client_model::World, id: ObjectId, pwd: PublicWeenieDesc) {
    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = pwd;
    it.valid = true;
    w.tables.weenies.insert(id, it);
}

/// Behaviour: selection.range-checks.a-frame-closes-a-vendor-the-player-walked-away-from
///
/// The whole chain, in the application's own function.
#[test]
fn a_frame_closes_the_vendor_the_player_has_walked_away_from() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    assert!(
        scene.character.is_some(),
        "the range checks need the world controller's player"
    );

    let mut objects = ObjectStream::new();
    put(
        &mut objects.world,
        PLAYER_OBJECT_ID,
        PublicWeenieDesc::default(),
    );
    objects.world.player = Some(PLAYER_OBJECT_ID);
    objects.world.tables.inventories.insert(
        PLAYER_OBJECT_ID,
        dereth_client_model::objects::ObjectInventory::new(PLAYER_OBJECT_ID),
    );
    put(
        &mut objects.world,
        VENDOR,
        PublicWeenieDesc {
            use_radius: Some(3.0),
            ..PublicWeenieDesc::default()
        },
    );

    // Vendor info emits the notice that opens the vendor panel, and its tail arms the vendor's
    // range watch.
    let mut out = dereth_client_model::RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();
    objects.world.handle_vendor_info(
        &dereth_protocol::trade::VendorInfo {
            merchant_id: VENDOR,
            profile: dereth_protocol::trade::VendorProfile::default(),
            items: Vec::new(),
        },
        &mut out,
        &mut req,
        ServerTime(0.0),
    );
    assert!(objects.world.shop.is_open(), "the shop opened");
    assert!(objects
        .world
        .object_range_checks
        .is_watching(RangeHandler::Vendor, VENDOR));

    // The control: the player watching himself, which the geometry must answer as 0 m.
    objects.world.object_range_checks.register(
        RangeHandler::Selection,
        PLAYER_OBJECT_ID,
        1.0,
        true,
        true,
        dereth_client_model::range::POLL_INTERVAL,
        0.0,
        0.0,
    );

    let mut inter = Interaction::new();

    // Frame 1, before the poll interval has elapsed. Nothing may be polled and nothing may close.
    let (unowned, left) = interaction::use_time(
        &mut inter,
        &store,
        Some(&scene),
        &mut objects,
        None,
        Vec::new(),
        false,
        (800, 600),
        LocalTime(0.5),
    );
    assert!(unowned.is_empty() && left.is_empty());
    assert_eq!(
        inter.stats.range_polls, 0,
        "armed at t=0, first due at t=1.0"
    );
    assert_eq!(inter.stats.range_exits, 0);
    assert!(
        objects.world.shop.is_open(),
        "the frame did not close it early"
    );

    // Frame 2, after it. Both are polled; the one with no body leaves range, the control does not.
    let (unowned, left) = interaction::use_time(
        &mut inter,
        &store,
        Some(&scene),
        &mut objects,
        None,
        Vec::new(),
        false,
        (800, 600),
        LocalTime(2.0),
    );
    assert!(unowned.is_empty() && left.is_empty());
    assert_eq!(
        inter.stats.range_polls, 2,
        "both registrations were consulted"
    );
    assert_eq!(inter.stats.range_exits, 1, "exactly one left range");
    assert!(
        !objects.world.shop.is_open(),
        "the vendor window closed because `interaction::use_time` ran the range checks"
    );
    assert!(
        objects
            .world
            .object_range_checks
            .is_watching(RangeHandler::Selection, PLAYER_OBJECT_ID),
        "and the control, whose body exists and is 0 m away, is still armed -- so the geometry \
         answered a real distance rather than `None` for everything"
    );
}

/// With no local body there is no player to measure from, so the checks do not run at all.
///
/// This is the guard `SceneRangeGeometry::new` carries, and it matters because the alternative —
/// running the checks against a missing player — reports **every** watched object out of range and
/// closes every panel on the frame the scene is torn down.
#[test]
fn without_a_body_the_checks_are_skipped_rather_than_run_against_nothing() {
    let store = store();
    let mut objects = ObjectStream::new();
    put(
        &mut objects.world,
        PLAYER_OBJECT_ID,
        PublicWeenieDesc::default(),
    );
    objects.world.player = Some(PLAYER_OBJECT_ID);
    objects.world.tables.inventories.insert(
        PLAYER_OBJECT_ID,
        dereth_client_model::objects::ObjectInventory::new(PLAYER_OBJECT_ID),
    );
    put(
        &mut objects.world,
        VENDOR,
        PublicWeenieDesc {
            use_radius: Some(3.0),
            ..PublicWeenieDesc::default()
        },
    );
    let mut out = dereth_client_model::RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();
    objects.world.handle_vendor_info(
        &dereth_protocol::trade::VendorInfo {
            merchant_id: VENDOR,
            profile: dereth_protocol::trade::VendorProfile::default(),
            items: Vec::new(),
        },
        &mut out,
        &mut req,
        ServerTime(0.0),
    );

    let mut inter = Interaction::new();
    for now in [0.5_f64, 2.0, 10.0] {
        let _ = interaction::use_time(
            &mut inter,
            &store,
            None,
            &mut objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            LocalTime(now),
        );
    }
    assert_eq!(inter.stats.range_polls, 0, "no body, no poll");
    assert_eq!(inter.stats.range_exits, 0);
    assert!(objects.world.shop.is_open(), "and nothing closed");
    assert!(objects
        .world
        .object_range_checks
        .is_watching(RangeHandler::Vendor, VENDOR));
}

/// Through the frame, the selection range watch arms at the radar radius the player's own cell
/// chooses: 75 m on the landscape, 25 m in an interior cell, and back to 75 m when the body walks
/// out again.
mod range_watch {
    //! # How the radius is chosen
    //!
    //! `Interaction::run_object_range_checks` arms its watches at
    //! `radar_range(is_outdoors(character.position().cell))`: a cell is outside when
    //! `cell & 0xFFFF < 0x100`, read on the player's own cell. The station varies the player's
    //! **cell** and nothing else: the same selected object, at the same place, over the same
    //! frames.
    //!
    //! The body has to stand *in* the room, not merely be labelled with it. Placement resolves its
    //! own cell: an interior id at a point in no visible cell of a room marked `seen_outside` is
    //! put on the land cell the building stands in. So the body is stood at a point the room's own
    //! `cell_bsp` says is inside it (`WorldScene::standable_point`).

    use dereth_scene::world_scene::SceneWrites;
    use std::sync::Arc;

    use dereth_client_model::range::{RangeHandler, RADAR_RADIUS_INDOORS, RADAR_RADIUS_OUTDOORS};
    use dereth_client_runtime::character::PLAYER_OBJECT_ID;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_dat::RetailDatStore;
    use dereth_physics::LandSource;
    use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::types::PublicWeenieDesc;
    use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};
    use {
        dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
        dereth_world_data::landblock::DEFAULT_LANDBLOCK,
    };

    const TARGET: ObjectId = ObjectId(0x8000_0011);

    fn store() -> Arc<RetailDatStore> {
        crate::common::dats()
    }

    fn put(w: &mut dereth_client_model::World, id: ObjectId) {
        let mut it = dereth_client_model::Weenie::new(id);
        it.pwd = PublicWeenieDesc::default();
        it.valid = true;
        w.tables.weenies.insert(id, it);
    }

    /// The range the selection watch is currently armed at, or `None` if it is not armed.
    fn armed_selection_range(w: &dereth_client_model::World) -> Option<f64> {
        w.object_range_checks
            .live()
            .find(|e| e.handler == RangeHandler::Selection)
            .map(|e| e.range)
    }

    /// Drive one frame of `Interaction::use_time`, which is what `App::frame` calls.
    fn frame(
        inter: &mut Interaction,
        store: &Arc<RetailDatStore>,
        scene: &WorldScene,
        objects: &mut ObjectStream,
        now: f64,
    ) {
        let (unowned, left) = interaction::use_time(
            inter,
            store,
            Some(scene),
            objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            LocalTime(now),
        );
        assert!(unowned.is_empty() && left.is_empty());
    }

    /// The first resident interior cell of the scene's own landblock **and a point inside it**.
    ///
    /// A **real** environment cell from `client_cell_1.dat` rather than a made-up id, so that
    /// "indoors" here is a place the client can actually put a player rather than an index chosen
    /// to satisfy `is_outdoors`, and a point the cell's own `cell_bsp` says is in the room, because
    /// placement puts a body handed an interior id at a point outside the room **outside** (see the
    /// module doc). A cell with no standable point is skipped rather than used.
    fn a_room(scene: &WorldScene) -> (CellId, Vec3) {
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        for index in 0x0100_u32..0x0200 {
            let id = CellId((u32::from(DEFAULT_LANDBLOCK) << 16) | index);
            if land.env_cell(id).is_none() {
                continue;
            }
            if let Some(inside) = scene.standable_point(id) {
                return (id, inside);
            }
        }
        panic!("landblock {DEFAULT_LANDBLOCK:#06X}: no resident environment cell in 0x100..0x200 is standable");
    }

    /// Behaviour: selection.range-watch.arms-at-the-radius-the-players-cell-chooses
    ///
    /// **The selection watch arms at 75 m outdoors and 25 m in an interior cell, through the
    /// frame.**
    ///
    /// Both arms in one run, over one scene, one selected object and one body — the only thing that
    /// changes between the two readings is the cell the body stands in. Each arm asserts its own
    /// premise (that `is_outdoors` really answered what the station intended) before asserting the
    /// range, so a teleport that silently failed cannot read as a range that was chosen.
    #[test]
    fn the_selection_watch_arms_at_the_range_the_players_own_cell_chooses() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let mut scene =
            WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        assert!(
            scene.character.is_some(),
            "the range checks need the world controller's player"
        );

        let mut objects = ObjectStream::new();
        put(&mut objects.world, PLAYER_OBJECT_ID);
        put(&mut objects.world, TARGET);
        objects.world.player = Some(PLAYER_OBJECT_ID);
        objects.world.tables.inventories.insert(
            PLAYER_OBJECT_ID,
            dereth_client_model::objects::ObjectInventory::new(PLAYER_OBJECT_ID),
        );

        let mut inter = Interaction::new();
        let mut out = dereth_client_model::RecordingSink::default();

        // ---- Outdoors. -------------------------------------------------------------------------
        let outdoor_cell = scene.character.as_ref().expect("a body").position().cell;
        assert!(
            dereth_physics::landdefs::is_outdoors(outdoor_cell),
            "the premise: the body starts on the landscape, in {outdoor_cell:?}"
        );
        objects
            .world
            .set_selected_object(Some(TARGET), false, &mut out);
        assert_eq!(
            armed_selection_range(&objects.world),
            None,
            "nothing is armed before a frame"
        );
        frame(&mut inter, &store, &scene, &mut objects, 0.1);
        let outdoors = armed_selection_range(&objects.world).expect("the frame armed the watch");
        assert!(
            (outdoors - f64::from(RADAR_RADIUS_OUTDOORS)).abs() < 1e-6,
            "outdoors the selection watch must arm at 75.0, got {outdoors}"
        );

        // ---- The same object, the same place, with the body in a real interior cell. -----------
        //
        // Disarm first: the selected-item notice re-registers on the *edge*, so the watch has to be
        // allowed to go away before it can be armed again with a different radius.
        objects.world.set_selected_object(None, false, &mut out);
        frame(&mut inter, &store, &scene, &mut objects, 0.2);
        assert_eq!(
            armed_selection_range(&objects.world),
            None,
            "the watch was disarmed"
        );

        let (room, inside) = a_room(&scene);
        scene
            .character
            .as_mut()
            .expect("a body")
            .teleport(Position::new(room, Frame::new(inside, Quat::IDENTITY)));
        let indoor_cell = scene.character.as_ref().expect("a body").position().cell;
        // Placement keeps an interior id whose point the visible-cell search finds in the cell
        // itself.
        assert_eq!(indoor_cell, room, "the placement kept the room");
        assert!(
            !dereth_physics::landdefs::is_outdoors(indoor_cell),
            "the premise: the body is now in an environment cell, {indoor_cell:?}"
        );

        objects
            .world
            .set_selected_object(Some(TARGET), false, &mut out);
        frame(&mut inter, &store, &scene, &mut objects, 0.3);
        let indoors =
            armed_selection_range(&objects.world).expect("the frame armed the watch again");
        assert!(
            (indoors - f64::from(RADAR_RADIUS_INDOORS)).abs() < 1e-6,
            "in {indoor_cell:?} the selection watch must arm at 25.0, got {indoors}"
        );

        // And the two readings really differ, which is the whole claim: the range is chosen.
        assert!(
            (outdoors - indoors).abs() > 49.0,
            "one selected object, one place, two cells: {outdoors} vs {indoors}"
        );
    }

    /// The same producer, read from the other end: `run_object_range_checks` must take its answer
    /// from the **body's own cell**, not from the scene, the camera or a default.
    ///
    /// A plausible constant in the place a choice belongs looks exactly like a decision, so here
    /// the two cells are visited in the opposite order to the test above, so a build that answered
    /// "whatever it answered last" is caught as well as one that answered a constant.
    #[test]
    fn the_range_follows_the_body_when_it_goes_back_outside() {
        let store = store();
        let mut gpu = crate::common::test_gpu(800, 600);
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let mut scene =
            WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");

        let mut objects = ObjectStream::new();
        put(&mut objects.world, PLAYER_OBJECT_ID);
        put(&mut objects.world, TARGET);
        objects.world.player = Some(PLAYER_OBJECT_ID);
        objects.world.tables.inventories.insert(
            PLAYER_OBJECT_ID,
            dereth_client_model::objects::ObjectInventory::new(PLAYER_OBJECT_ID),
        );

        let mut inter = Interaction::new();
        let mut out = dereth_client_model::RecordingSink::default();

        let outdoor = scene.character.as_ref().expect("a body").position();
        let (room, inside) = a_room(&scene);

        // Indoors first — at a point in the room, or the placement puts the body back outside.
        scene
            .character
            .as_mut()
            .expect("a body")
            .teleport(Position::new(room, Frame::new(inside, Quat::IDENTITY)));
        assert_eq!(
            scene.character.as_ref().expect("a body").position().cell,
            room
        );
        objects
            .world
            .set_selected_object(Some(TARGET), false, &mut out);
        frame(&mut inter, &store, &scene, &mut objects, 0.1);
        let indoors = armed_selection_range(&objects.world).expect("armed");
        assert!(
            (indoors - f64::from(RADAR_RADIUS_INDOORS)).abs() < 1e-6,
            "indoors first: {indoors}"
        );

        // Back outside, and re-armed.
        objects.world.set_selected_object(None, false, &mut out);
        frame(&mut inter, &store, &scene, &mut objects, 0.2);
        scene.character.as_mut().expect("a body").teleport(outdoor);
        assert!(dereth_physics::landdefs::is_outdoors(
            scene.character.as_ref().expect("a body").position().cell
        ));
        objects
            .world
            .set_selected_object(Some(TARGET), false, &mut out);
        frame(&mut inter, &store, &scene, &mut objects, 0.3);
        let outdoors = armed_selection_range(&objects.world).expect("armed");
        assert!(
            (outdoors - f64::from(RADAR_RADIUS_OUTDOORS)).abs() < 1e-6,
            "outdoors after: {outdoors}"
        );
    }
}
