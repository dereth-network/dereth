//! Texture descriptor slots are released by reference count, not by LRU: a released graphics
//! object drops one link, and the texture is freed once the cache's own link is the last. Three
//! populations, each on a drive in which only it moves: a landblock's bake returns its links when
//! the block leaves the window or is re-baked in place (as retail's block teardown destroys the
//! block's objects), so a diagonal walk out and home twice comes home holding the same slots; the
//! local body returns its old parts' links when an appearance change replaces them (as retail's
//! part replacement releases the old array first), over sixteen `Item_ObjDescEvent 0xF625`
//! changes; and the combined-texture cache, keyed by (palette, texture), serves hits so two groups
//! naming one pair share one slot. Fixture: the retail dats on a WARP device, a 5x5 window at
//! Holtburg and a body in cell `0x8602_01AD`. Fails without the dats or a device is absent.
//!
//! Sections of this module:
//! * `terrain_surfaces`: a departed or re-meshed block returns its merged terrain surfaces.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{SceneReads, SceneWrites};
use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{block_xy, load_region, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemObjDescEvent, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{
    AnimPartChange, ObjDesc as ProtocolObjDesc, PhysicsDesc, PhysicsEventStamp, PublicWeenieDesc,
};
use dereth_protocol::{write_body, Opcode};
use dereth_render::device::Gpu;

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// `(blockX, blockY)` as a [`LandblockId`], which is `x << 8 | y`.
fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above. Not a float conversion.
    LandblockId(((x as u16) << 8) | (y as u16))
}

// ---------------------------------------------------------------------------------------------
// The streaming window
// ---------------------------------------------------------------------------------------------

/// One lap: out far enough that **every** block of the starting window has left it, and back to
/// exactly where it began.
///
/// Diagonal, as in `world::landblock_interior_release`: a straight walk east enters one new block
/// column per station and re-uses most of the window, so a leak has little to accumulate. A
/// diagonal turns over four blocks a station.
const LAP: [(i32, i32); 12] = [
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 4),
    (5, 5),
    (6, 6),
    (5, 5),
    (4, 4),
    (3, 3),
    (2, 2),
    (1, 1),
    (0, 0),
];

/// **Two laps, and the second one is the measurement.** The first station is the window as
/// `WorldScene::load` left it, and the acceptance is that **lap 2's home reading equals lap 1's**,
/// which is exactly "a block that leaves and re-enters does not accumulate".
///
/// All three home stations (load, lap 1, lap 2) read **345** pairs and **333** scenery objects
/// over the same 25 blocks. The load figure is reported beside the laps and bounded from above
/// rather than pinned, because it is an upper bound on the whole window's residency and a leak
/// would put the laps above it.
const LAPS: usize = 2;

/// Where the home reading of lap `n` sits in the station list: station 0 is the load, and each lap
/// adds [`LAP`]`.len()` stations ending at home.
const fn home_station(lap: usize) -> usize {
    lap * LAP.len()
}

/// What one station of a lap holds, in terms of the window's slot residency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Station {
    /// Blocks resident in the window. The denominator: a walk that streamed nothing would hold
    /// every number below constant for the most boring possible reason.
    blocks: usize,
    /// `BakeCache::textures_uploaded`: descriptor pairs the **shared surface memo** holds now.
    bake_slots: u32,
    /// Distinct `GroupKey`s ever resolved. Cumulative, so it must rise while `bake_slots` does
    /// not: that is the difference between "the memo is working" and "the memo was deleted".
    resolved: u32,
    /// Links a departed block's bake handed back, and how many of those freed a pair.
    released: u64,
    freed: u64,
    /// `BakeCache::unowned_releases`. A double release. Must be zero.
    unowned: u32,
    /// The device's own live slot count, so a leak that escaped the memo's own counter is still
    /// visible.
    live: u32,
    /// The block the window is centred on. It is **not** always the block the body was teleported
    /// to: `WorldScene::recenter` centres on `viewpoint()`, which is the *camera*, and the chase
    /// camera sits behind the body, so the window a teleport settles on depends on the heading
    /// the body arrived with. Recorded because it is the difference between "the same 25 blocks"
    /// and "25 blocks", and only the first makes a slot count comparable.
    centre: Option<(i32, i32)>,
    /// What the resident blocks' bakes actually contain, summed over the window: the denominator
    /// for [`Self::bake_slots`]. Two windows over the same blocks holding different numbers of
    /// pairs is a fact about the release; two windows holding different numbers of *placements* is
    /// a fact about the bake.
    scenery: usize,
    statics: usize,
    batches: usize,
}

fn read(scene: &WorldScene, gpu: &Gpu) -> Station {
    Station {
        centre: scene.viewer_block(),
        scenery: scene.draw.stats.scenery_objects,
        statics: scene.draw.stats.static_objects,
        batches: scene.draw.stats.object_batches,
        blocks: scene.resident_blocks(),
        bake_slots: scene.draw.stats.textures_uploaded,
        resolved: scene.draw.stats.surfaces_resolved,
        released: scene.draw.stats.block_texture_releases,
        freed: scene.draw.stats.block_textures_freed,
        unowned: scene.draw.stats.bake_unowned_releases,
        live: gpu.descriptor_usage().live,
    }
}

/// Stand the body in the middle of one landblock and let the window catch up. A teleport rather
/// than a walk: this is about the block **window**, and a scripted walk would make the reading a
/// function of the physics as well.
fn go_to(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu, block: LandblockId) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
    scene.stream(store, gpu).expect("the streamed blocks build");
}

/// The scene, and the two halves of what it holds before the walk starts: the pairs the window's
/// own bake took, and the pairs the **body** added on top of it. Split so that the load-time
/// figure reported below is attributable rather than a single number to shrug at.
fn embodied(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scenery_radius: u32,
) -> (WorldScene, u32, u32) {
    // `land_radius: 2` is a 5x5 window. `scenery_radius` is the knob that decides *which* of its
    // slots bake objects, and the two tests below run it at both settings deliberately, because
    // the two settings reach different release sites (see
    // `a_block_that_scrolls_out_of_the_scenery_radius_returns_its_links_in_place`).
    let cfg = SceneConfig {
        land_radius: 2,
        scenery_radius,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let window_only = scene.draw.stats.textures_uploaded;
    let region = load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    let with_body = scene.draw.stats.textures_uploaded;
    (scene, window_only, with_body)
}

/// Behaviour: world.streaming.a-block-that-scrolls-away-and-back-holds-the-same-slots
///
/// A window that scrolls away from its blocks and back to them holds the same number of
/// descriptor pairs at the end as at the beginning.
///
/// The slot count is **flat across landblock churn rather than rising**, and a block that leaves
/// and re-enters does not accumulate. Both are the same assertion made at the two ends of one
/// walk, and the walk returns home so that the comparison needs no arithmetic.
///
/// Three things are asserted separately, because any one of them alone has a way to pass on
/// nothing:
///
/// 1. **The premise.** Blocks were actually released and links actually went back: a walk that
///    never scrolled, or a release path that was never called, satisfies every equality below.
/// 2. **The memo still shares.** `surfaces_resolved` must keep rising while `bake_slots` does not.
///    A cache that had simply been deleted would also hold a flat residency, at the cost of
///    re-uploading everything every frame.
/// 3. **The equality**, at home and at the far station.
#[test]
fn a_window_that_scrolls_away_from_its_blocks_and_back_holds_the_same_slots() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let (mut scene, window_only, with_body) = embodied(&store, &mut gpu, 2);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

    // Station 0 is the window as `load` left it; then `LAPS` laps, each ending at home.
    let mut walk: Vec<(i32, i32)> = vec![(0, 0)];
    for _ in 0..LAPS {
        walk.extend_from_slice(&LAP);
    }
    let mut stations: Vec<Station> = Vec::with_capacity(walk.len());
    for &(dx, dy) in &walk {
        go_to(&mut scene, &store, &mut gpu, block_at(hx + dx, hy + dy));
        stations.push(read(&scene, &gpu));
    }
    for (i, s) in stations.iter().enumerate() {
        let (dx, dy) = walk[i];
        eprintln!(
            "station {i:2} body at ({:3},{:3}) window centred {:?}: {:2} blocks, bake \
             slots {:5}, resolved {:5}, device live {:5}, block links released {:5} (freed \
             {:5}), unowned {}, scenery {}, statics {}, batches {}",
            hx + dx,
            hy + dy,
            s.centre,
            s.blocks,
            s.bake_slots,
            s.resolved,
            s.live,
            s.released,
            s.freed,
            s.unowned,
            s.scenery,
            s.statics,
            s.batches
        );
    }

    let loaded = stations[0];
    let far = stations[LAP.len() / 2];
    let home = stations[home_station(1)];
    let back = stations[home_station(LAPS)];

    // 1. The premise, in three parts: the window moved, the release edge fired, and it freed
    //    something. Without all three the equality below is a statement about a walk that stood
    //    still.
    assert!(
        loaded.blocks > 1,
        "the window holds {} block(s); nothing can churn",
        loaded.blocks
    );
    assert!(
        back.released > 0 && back.freed > 0,
        "no departed block returned a link ({} released, {} freed) -- the release edge never ran, \
         and every equality in this test is then about a walk that released nothing",
        back.released,
        back.freed
    );
    assert!(
        far.resolved > loaded.resolved,
        "the far station resolved no new surface groups ({} then {}), so the walk never left the \
         blocks it started on",
        loaded.resolved,
        far.resolved
    );

    // 2. The memo still shares. A deleted cache is flat too, and much worse.
    assert!(
        back.resolved > far.resolved,
        "the return leg resolved nothing new ({} then {}); a block that came back should have \
         re-resolved the groups its departure freed",
        far.resolved,
        back.resolved
    );
    assert!(
        u32::try_from(back.released).expect("fits") > back.bake_slots,
        "only {} links were ever returned against {} pairs resident -- the walk is too short for \
         the residency to mean anything",
        back.released,
        back.bake_slots
    );

    // 3. The measurement. Two laps of home-away-home over the *same* blocks, so anything the
    //    second lap adds is growth that does not depend on what is on screen. A departed block
    //    that dropped its bake without returning its links would make `bake_slots` only rise.
    assert_eq!(
        back.bake_slots, home.bake_slots,
        "lap {LAPS} came home holding {} descriptor pairs where lap 1 came home with {} -- a \
         block that leaves and re-enters is accumulating",
        back.bake_slots, home.bake_slots
    );
    assert_eq!(
        back.live, home.live,
        "the device's own live slot count moved between the laps"
    );
    assert_eq!(
        back.blocks, home.blocks,
        "the laps ended with different windows"
    );
    // And nothing the laps did added to what the load held. The load is *not* asserted equal --
    // it holds a few more pairs than a scrolled window does, which is a property of the one-pass
    // fetch and is reported rather than pinned -- but it is an upper bound, and a leak would put
    // the laps above it rather than below.
    assert!(
        back.bake_slots <= loaded.bake_slots,
        "the walk ended holding {} pairs against the {} the load held",
        back.bake_slots,
        loaded.bake_slots
    );
    eprintln!(
        "home readings: load {} pairs ({window_only} the window's bake, \
         {} the body's), lap 1 home {} pairs, lap {LAPS} home {} pairs \
         ({} groups resolved over the walk, {} block links returned, {} pairs freed)",
        loaded.bake_slots,
        with_body - window_only,
        home.bake_slots,
        back.bake_slots,
        back.resolved,
        back.released,
        back.freed
    );

    // 5. **A block that re-enters the window re-bakes its scenery.**
    //
    // `dereth_world_render::scenery::generate_scenery` bakes scenery only for a full-detail block
    // (`side_cell_count == 8`), as retail places static scenery only there, and a block enters the
    // window at an **outer LOD ring**. Retail's size-change path destroys a block's objects on
    // demotion out of full detail and the visible-cell traversal rebuilds them for every
    // full-detail block, so a change of `side_cell_count` invalidates the bake (see
    // `WorldScene::build_slot`'s `reuse` and the `SlotWork` docs); reusing the outer-ring bake
    // on the scroll inward would keep a scenery-free block for ever.
    //
    // Asserted at the home station on `scenery_objects` **alone**: `object_triangles` is an
    // aggregate the statics satisfy by themselves.
    assert_eq!(
        loaded.scenery, 333,
        "the load window over Holtburg baked {} scenery objects; the readings below are written \
         against 333 and the number has moved",
        loaded.scenery
    );
    assert_eq!(
        back.scenery, loaded.scenery,
        "the walk came home with {} scenery objects against the {} the load held: a block that \
         left the window and came back is not re-baking its scenery",
        back.scenery, loaded.scenery
    );
    assert_eq!(
        home.scenery, loaded.scenery,
        "lap 1 came home with {} scenery objects against the load's {}",
        home.scenery, loaded.scenery
    );
    assert_eq!(
        back.statics, loaded.statics,
        "the statics are the control: they DO re-bake on re-entry ({} at load, {} on the return), \
         which is what makes the scenery reading a defect rather than a property of the walk",
        loaded.statics, back.statics
    );

    // 4. The tolerated failures, asserted rather than printed.
    assert_eq!(back.unowned, 0, "a texture slot was released twice");
    assert_eq!(
        gpu.texture_table_stats().unknown_releases,
        0,
        "a release named a slot the texture table did not hold"
    );
    assert_eq!(
        gpu.descriptor_stats().invalid_releases,
        0,
        "a descriptor slot was released twice"
    );
    assert_eq!(
        gpu.descriptor_stats().exhaustions,
        0,
        "the descriptor heap ran out"
    );
}

/// **The second release site, and it is a different one.**
///
/// A departed block gives its links back through `WorldScene::release_departed_blocks`, which
/// [`a_window_that_scrolls_away_from_its_blocks_and_back_holds_the_same_slots`] exercises. A block
/// that stays resident but is **re-baked in place** gives them back through `build_slot`, and with
/// `scenery_radius == land_radius` that arm is never reached: every resident slot wants objects,
/// so no resident block ever loses its bake.
///
/// Set the scenery radius **inside** the window and it is reached constantly: a block that
/// scrolls into the scenery radius grows its objects and one that scrolls out releases them, as
/// retail destroys a block's static objects, so `queue` pushes `SlotWork::Full` at a **resident**
/// block on every ring crossing and `build_slot` replaces the previous `BlockDraw`, whose links
/// must go back with it; the client does this several times per block walked.
#[test]
fn a_block_that_scrolls_out_of_the_scenery_radius_returns_its_links_in_place() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    // Scenery radius 1 inside a radius-2 window: the outer ring holds terrain and no objects, so
    // every block crosses the boundary twice per lap while staying resident.
    let (mut scene, _, _) = embodied(&store, &mut gpu, 1);
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

    let mut walk: Vec<(i32, i32)> = vec![(0, 0)];
    for _ in 0..LAPS {
        walk.extend_from_slice(&LAP);
    }
    let mut stations: Vec<Station> = Vec::with_capacity(walk.len());
    for &(dx, dy) in &walk {
        go_to(&mut scene, &store, &mut gpu, block_at(hx + dx, hy + dy));
        stations.push(read(&scene, &gpu));
    }
    let loaded = stations[0];
    let home = stations[home_station(1)];
    let back = stations[home_station(LAPS)];
    eprintln!(
        "scenery-radius churn: load {} pairs, lap 1 home {} pairs, lap {LAPS} home {} \
         pairs; {} block links returned, {} pairs freed, {} groups resolved",
        loaded.bake_slots,
        home.bake_slots,
        back.bake_slots,
        back.released,
        back.freed,
        back.resolved
    );

    // The premise: this configuration really does re-bake resident blocks, which is what
    // `object_batches` moving between the stations shows, and the release really ran.
    assert!(
        back.released > 0 && back.freed > 0,
        "no link came back at all"
    );
    assert!(
        back.resolved > loaded.resolved * 2,
        "only {} groups resolved against {} at load; a walk that never re-baked would sit near \
         the load figure and this test would be measuring nothing",
        back.resolved,
        loaded.resolved
    );

    // The measurement: two identical laps hold identical residencies.
    assert_eq!(
        back.bake_slots, home.bake_slots,
        "lap {LAPS} came home holding {} pairs where lap 1 came home with {}",
        back.bake_slots, home.bake_slots
    );
    assert_eq!(
        back.live, home.live,
        "the device's live slot count moved between the laps"
    );
    assert!(
        back.bake_slots <= loaded.bake_slots,
        "the walk ended holding {} pairs against the {} the load held",
        back.bake_slots,
        loaded.bake_slots
    );
    assert_eq!(back.unowned, 0, "a texture slot was released twice");
    assert_eq!(gpu.texture_table_stats().unknown_releases, 0);
    assert_eq!(gpu.descriptor_stats().invalid_releases, 0);
}

// ---------------------------------------------------------------------------------------------
// The local body
// ---------------------------------------------------------------------------------------------

/// The Aluvian male body, the setup `Character::new` builds. 34 parts.
const BODY: DataId = DataId(0x0200_0001);

/// A landblock and cell that load, used by the graphics-object seam tests too.
const LANDBLOCK: u16 = 0x8602;
const CELL: u32 = 0x8602_01AD;

const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// Two shipped graphics objects whose meshes differ: dressing the body alternately in them is a
/// real change of geometry and not a re-application of one outfit.
const LOD_A: DataId = DataId(0x0100_0001);
const LOD_B: DataId = DataId(0x0100_001C);

/// How many times the outfit is changed. The claim rests on **repeated** changes: one change
/// cannot tell a leak from a working set, because a working set of two outfits looks exactly like
/// one outfit leaked once.
const CHANGES: usize = 16;

fn wire_swap(index: u8, to: DataId) -> ProtocolObjDesc {
    ProtocolObjDesc {
        anim_part_changes: vec![AnimPartChange {
            part_index: index,
            part_id: to.0,
        }],
        ..ProtocolObjDesc::default()
    }
}

/// `0xF745 Item_CreateObject` for the player, carrying `od`.
fn player_create(od: ProtocolObjDesc) -> Vec<SessionEvent> {
    let physicsdesc = PhysicsDesc {
        setup_id: Some(BODY.0),
        bitfield: flags::SETUP,
        ..PhysicsDesc::default()
    };
    let body = write_body(&ItemCreateObject(ObjectCreatePayload {
        id: PLAYER,
        objdesc: od,
        physicsdesc,
        wdesc: PublicWeenieDesc::default(),
    }))
    .expect("encode");
    vec![
        SessionEvent::PlayerCreated(PLAYER),
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        },
    ]
}

/// `0xF625 Item_ObjDescEvent`, the real path an appearance change arrives on.
fn objdesc_event(od: ProtocolObjDesc, ts: u16) -> SessionEvent {
    let body = write_body(&ItemObjDescEvent {
        id: PLAYER,
        objdesc: od,
        timestamps: PhysicsEventStamp {
            instance: 0,
            event: ts,
        },
    })
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_OBJ_DESC_EVENT,
        body,
    }
}

/// Sixteen appearance changes on the player leave the descriptor residency where two changes
/// left it.
///
/// The baseline is taken after the outfit has been applied **once in each direction**: by then
/// both outfits' groups are in the memo and the working set is complete, and every change after
/// that must cost nothing. `set_character_parts` returns the old parts' links as it replaces
/// them; dropping the old `Vec<PartLevels>` instead would strand a body's worth of textures per
/// change, for ever, on a client whose player re-equips all day.
#[test]
fn repeated_appearance_changes_on_the_player_do_not_accumulate_slots() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        start_cell: Some(dereth_primitives::CellId(CELL)),
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let region = load_region(&store).expect("the region decodes");
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    let mut stream = ObjectStream::new();

    let mut apply = |scene: &mut WorldScene, gpu: &mut Gpu, ev: Vec<SessionEvent>, ts: u16| {
        for e in &ev {
            stream.apply_event(e, LocalTime(f64::from(ts)));
        }
        scene
            .sync_objects(&store, gpu, &mut stream)
            .expect("objects sync");
    };

    // The create, then one change in each direction: after this the memo holds both outfits and
    // the working set is complete. Everything measured below is what the *fourth* through
    // *nineteenth* changes cost.
    apply(&mut scene, &mut gpu, player_create(wire_swap(0, LOD_A)), 1);
    apply(
        &mut scene,
        &mut gpu,
        vec![objdesc_event(wire_swap(0, LOD_B), 2)],
        2,
    );
    apply(
        &mut scene,
        &mut gpu,
        vec![objdesc_event(wire_swap(0, LOD_A), 3)],
        3,
    );

    let base_slots = scene.draw.stats.textures_uploaded;
    let base_live = gpu.descriptor_usage().live;
    let base_resolved = scene.draw.stats.surfaces_resolved;
    let base_releases = scene.draw.stats.body_texture_releases;
    let base_applied = scene.draw.stats.objdescs_applied;

    for i in 0..CHANGES {
        let to = if i % 2 == 0 { LOD_B } else { LOD_A };
        // The `0xF625` timestamp is `PhysicsEventStamp::event`, and `ObjectStream` refuses a
        // description that is not newer than the one it holds -- so a fixed stamp would apply the
        // first change and silently drop fifteen. `objdescs_applied` is asserted below for
        // exactly that reason.
        let ts = u16::try_from(4 + i).expect("CHANGES is small");
        apply(
            &mut scene,
            &mut gpu,
            vec![objdesc_event(wire_swap(0, to), ts)],
            ts,
        );
    }

    let slots = scene.draw.stats.textures_uploaded;
    let live = gpu.descriptor_usage().live;
    eprintln!(
        "after {CHANGES} appearance changes: bake slots {base_slots} -> {slots}, device \
         live {base_live} -> {live}, groups resolved {base_resolved} -> {}, body links returned \
         {base_releases} -> {} (freed {}), descriptions applied {base_applied} -> {}",
        scene.draw.stats.surfaces_resolved,
        scene.draw.stats.body_texture_releases,
        scene.draw.stats.body_textures_freed,
        scene.draw.stats.objdescs_applied
    );

    // 1. The premise. Sixteen descriptions really were applied and really did rebuild the body,
    //    and the release edge really ran; without this the equalities are about a client that
    //    ignored every `0xF625`.
    assert_eq!(
        scene.draw.stats.objdescs_applied - base_applied,
        CHANGES as u64,
        "only {} of {CHANGES} descriptions reached the body",
        scene.draw.stats.objdescs_applied - base_applied
    );
    assert_eq!(
        scene.draw.stats.objdesc_failures, 0,
        "a swap to a shipped graphics object failed"
    );
    assert!(
        scene.draw.stats.body_texture_releases - base_releases >= CHANGES as u64,
        "{CHANGES} rebuilds returned only {} links; a body has 34 parts, so fewer than one link \
         per change means the release edge is not on the rebuild path",
        scene.draw.stats.body_texture_releases - base_releases
    );

    // 2. The measurement.
    assert_eq!(
        slots, base_slots,
        "{CHANGES} appearance changes moved the shared memo's residency from {base_slots} to \
         {slots} pairs -- each change is stranding the set it replaced"
    );
    assert_eq!(
        live, base_live,
        "{CHANGES} appearance changes moved the device's live slot count from {base_live} to \
         {live}"
    );

    // 3. And the memo did not simply stop resolving: the alternation keeps asking, and the answer
    //    keeps coming from the memo rather than from a fresh upload.
    assert!(
        scene.draw.stats.surfaces_resolved >= base_resolved,
        "the resolve counter went backwards"
    );
    assert_eq!(
        scene.draw.stats.bake_unowned_releases, 0,
        "a texture slot was released twice"
    );
    assert_eq!(
        gpu.texture_table_stats().unknown_releases,
        0,
        "an unknown slot was released"
    );
    assert_eq!(
        gpu.descriptor_stats().invalid_releases,
        0,
        "a descriptor slot was released twice"
    );
}

// ---------------------------------------------------------------------------------------------
// The keyed upload, on the smallest subject that can show it
// ---------------------------------------------------------------------------------------------

/// **The combined-texture cache is wired and it hits.**
///
/// The mesh-eviction replay test reports the corpus-scale effect; this says the mechanism is
/// present at all, as distinct from a key that is computed and never matches. `texture_key_hits`
/// is the counter: a non-zero value is a resolve that took a link on a texture another `GroupKey`
/// had already uploaded.
///
/// It also reports and bounds the **clip-map** conflict count, which is retail's own behaviour and
/// not a defect: combined-texture creation keys by `(palette DID, indexed-texture DID)` and
/// *nothing else* (not the clip-map flag, not the texture scale), so two surfaces that share a
/// pair and disagree about `BASE1_CLIPMAP` share one texture in retail too, and whichever asked
/// first decides how it was expanded. The bound is **derived** (a conflict cannot outnumber the
/// hits it is a subset of) rather than pinned to this window's population; the figures are printed.
#[test]
fn the_combined_texture_cache_is_wired_and_serves_hits_over_a_shipped_window() {
    let store = store();
    let mut gpu = crate::common::test_gpu(320, 240);
    let (mut scene, _, _) = embodied(&store, &mut gpu, 2);
    let s = &scene.draw.stats;
    eprintln!(
        "keyed upload over a 5x5 window at Holtburg: {} groups resolved, {} served by \
         a combined-texture cache hit and {} by the solid-colour texel's, {} descriptor \
         pairs held, {} clip-map key conflicts",
        s.surfaces_resolved,
        s.texture_key_hits,
        s.solid_texel_key_hits,
        s.textures_uploaded,
        s.clipmap_key_conflicts
    );
    assert!(
        s.surfaces_resolved > 0,
        "the window resolved no surface group at all"
    );
    // **Two assertions, not one.** The textured half is the combined-texture cache and the
    // untextured half is the surface assignment's solid-colour texel; they share a key space and
    // a table, and a single combined "hits" figure **cannot fail** when either is unwired,
    // because the other keeps it non-zero: with `combined_key` forced to `UNCACHED` (no textured
    // cache at all) the combined counter still reads 90-odd.
    assert!(
        s.texture_key_hits > 0,
        "{} groups resolved and not one **textured** group shared a (palette, texture) pair with \
         another. The key is being computed and never matching, which is `upload_texture_keyed` \
         wired to nothing",
        s.surfaces_resolved
    );
    assert!(
        s.solid_texel_key_hits > 0,
        "not one untextured surface shared its colour word with another, so the surface resolver's \
         solid-colour texel is one 1x1 texture per surface group again"
    );
    assert!(
        s.textures_uploaded < s.surfaces_resolved,
        "{} pairs held for {} groups: the cache is not saving a single slot",
        s.textures_uploaded,
        s.surfaces_resolved
    );
    // The clip-map conflict count, asserted **structurally** rather than pinned to the 1 this
    // window happens to produce: a conflict is by construction a *second* group arriving on a key
    // another group already holds, so it can never exceed the number of hits, and a count that
    // did would mean the counter was measuring something else. The absolute figures are printed
    // above, and by the mesh-eviction replay test for every recording in the corpus.
    assert!(
        s.clipmap_key_conflicts <= s.texture_key_hits + s.solid_texel_key_hits,
        "{} clip-map conflicts against {} cache hits: a conflict is a hit that disagreed about \
         BASE1_CLIPMAP, so it cannot outnumber them",
        s.clipmap_key_conflicts,
        s.texture_key_hits + s.solid_texel_key_hits
    );
    assert_eq!(s.bake_unowned_releases, 0);
    // The scene has to survive its own teardown: `release_textures` hands back one link per owned
    // *slot*, and with two groups on one slot "one per memo entry" would over-release.
    let handed_back = scene.release_textures(&mut gpu);
    eprintln!(
        "teardown handed back {handed_back} pairs, leaving {} live",
        gpu.descriptor_usage().live
    );
    assert_eq!(
        gpu.texture_table_stats().unknown_releases,
        0,
        "the teardown released a slot the texture table did not hold -- the release is counting \
         memo entries where it should be counting textures"
    );
    assert_eq!(gpu.descriptor_stats().invalid_releases, 0);
}

// ---------------------------------------------------------------------------------------------
// A departed block returns its merged terrain surfaces
// ---------------------------------------------------------------------------------------------

mod terrain_surfaces {
    //! A departed or re-meshed landblock gives back its merged **terrain** surfaces. As in retail,
    //! landblock destruction first walks all `side_cell_count²` cells and releases each cell's
    //! surface; a surface's reference count drops per cell and at zero the surface is freed, its slot
    //! cleared and its hash entry removed. The reference is taken per cell, not per key, so a block
    //! whose 64 cells merge to one surface holds 64 references; a freed surface index is handed out
    //! again, keeping `TerrainMergeCache::order` bounded by the peak live count. The discriminating
    //! reading is the **device's** live slot count at lap 1 home against the load over the same 25
    //! blocks: the `BakeCache` memo stays flat either way. A second lap must reproduce the first
    //! station for station. Fixture: the retail dats on a WARP device, a 5x5 window walked
    //! diagonally out of Holtburg and home twice. Fails without the dats or a device is absent.

    use super::{block_at, store, LAP, LAPS};
    use dereth_client::world::{SceneReads, SceneWrites};
    use std::sync::Arc;

    use dereth_client::character::CharacterInput;
    use dereth_client::world::{block_xy, load_region, SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
    use dereth_render::device::Gpu;

    // ---------------------------------------------------------------------------------------------
    // Fixtures: the same shapes `world::streaming_slot_release` uses, so the two are comparable.
    // ---------------------------------------------------------------------------------------------

    const fn home_station(lap: usize) -> usize {
        lap * LAP.len()
    }

    /// What one station holds, in terms of terrain-surface and device-slot residency.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Station {
        /// The denominator. A walk that streamed nothing holds every number below constant for the
        /// most boring possible reason.
        blocks: usize,
        /// The block the window is centred on: the *camera's*, not the body's. Two slot counts are
        /// only comparable over the same twenty-five blocks.
        centre: Option<(i32, i32)>,
        /// `TerrainMergeCache::len()`: merged surfaces resident **now**. The subject.
        terrain: usize,
        /// `BakeCache::textures_uploaded`: the object memo. Quoted beside the device's count because
        /// it stays flat whether or not terrain surfaces are released.
        bake_slots: u32,
        /// The device's own live slot count: the sum of the two above plus the body's, and the only
        /// number that can see a leak in a population nothing else enumerates.
        live: u32,
        /// Cells torn down, and surfaces whose last cell that was.
        terrain_releases: u64,
        terrain_freed: u64,
        /// A removal naming a key the merge cache does not hold. Must be zero.
        terrain_unowned: u32,
        /// Blocks re-meshed **in place**: `build_slot`'s reuse arm, the second release site. Counted
        /// so the test below can say the arm *ran* rather than assume it from the configuration.
        remeshed: u64,
        /// Distinct `GroupKey`s ever resolved: cumulative, so it must keep rising while the residency
        /// does not. That is the difference between "the caches are working" and "the caches were
        /// deleted".
        resolved: u32,
        /// The bake's own contents, as the control for `bake_slots`.
        scenery: usize,
        statics: usize,
    }

    fn read(scene: &WorldScene, gpu: &Gpu) -> Station {
        Station {
            blocks: scene.resident_blocks(),
            centre: scene.viewer_block(),
            terrain: scene.draw.stats.terrain_surfaces,
            bake_slots: scene.draw.stats.textures_uploaded,
            live: gpu.descriptor_usage().live,
            terrain_releases: scene.draw.stats.terrain_surface_releases,
            terrain_freed: scene.draw.stats.terrain_surfaces_freed,
            terrain_unowned: scene.draw.stats.terrain_unowned_releases,
            remeshed: scene.draw.stats.blocks_remeshed_in_place,
            resolved: scene.draw.stats.surfaces_resolved,
            scenery: scene.draw.stats.scenery_objects,
            statics: scene.draw.stats.static_objects,
        }
    }

    /// Stand the body in the middle of one landblock and let the window catch up. A teleport rather
    /// than a walk, so the reading is not also a function of the physics.
    fn go_to(
        scene: &mut WorldScene,
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        block: LandblockId,
    ) {
        let land = Arc::clone(scene.character.as_ref().expect("a body").land());
        let mid = 96.0f32;
        let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 1.0;
        {
            let c = scene.character.as_mut().expect("a body");
            c.teleport(Position::new(
                block.cell(1),
                Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
            ));
        }
        scene.follow_character_now();
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(0.0),
            0.0,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
    }

    fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scenery_radius: u32) -> WorldScene {
        let cfg = SceneConfig {
            land_radius: 2,
            scenery_radius,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
        let region = load_region(store).expect("the region decodes");
        scene
            .attach_character(store, &region, gpu)
            .expect("the body is created");
        scene
    }

    /// Walk the two laps and report every station.
    fn walk(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> Vec<Station> {
        let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
        let mut route: Vec<(i32, i32)> = vec![(0, 0)];
        for _ in 0..LAPS {
            route.extend_from_slice(&LAP);
        }
        let mut stations = Vec::with_capacity(route.len());
        for &(dx, dy) in &route {
            go_to(scene, store, gpu, block_at(hx + dx, hy + dy));
            stations.push(read(scene, gpu));
        }
        for (i, s) in stations.iter().enumerate() {
            let (dx, dy) = route[i];
            eprintln!(
                "station {i:2} body at ({:3},{:3}) window centred {:?}: {:2} blocks, terrain \
             surfaces {:5}, bake slots {:5}, device live {:5}, cells torn down {:6} (surfaces \
             freed {:5}), unowned {}, resolved {:5}, scenery {}, statics {}",
                hx + dx,
                hy + dy,
                s.centre,
                s.blocks,
                s.terrain,
                s.bake_slots,
                s.live,
                s.terrain_releases,
                s.terrain_freed,
                s.terrain_unowned,
                s.resolved,
                s.scenery,
                s.statics
            );
        }
        stations
    }

    /// Behaviour: world.streaming.a-departed-block-returns-its-merged-terrain-surfaces
    ///
    /// A window that scrolls away from its blocks and back to them holds the same number of
    /// **device** slots at the end as at the beginning, not merely the same number of memo entries.
    ///
    /// Five things are asserted separately, because each has a way to pass on nothing:
    ///
    /// 1. **The premise**: the window churned, the terrain release edge
    ///    (`TerrainMergeCache::remove_surface`) fired, and it actually freed surfaces. Without all
    ///    three every equality below is about a walk that released nothing.
    /// 2. **The subject moved**: the far window shares no block with the load's, and the merge cache's
    ///    residency at the far station is *below* the load's, so at least that many of the load
    ///    window's surfaces were genuinely destroyed on the way out and rebuilt on the way home.
    ///    Without a release edge the residency could only rise, so this is a leak detector too.
    /// 3. **The caches still share**: `surfaces_resolved` keeps rising while the residencies do not.
    /// 4. **The measurement**: the load and both homecomings agree, on the device's count as well as
    ///    on the memo's and the merge cache's.
    /// 5. **The tolerated failures**, asserted rather than printed.
    #[test]
    fn a_window_that_scrolls_away_from_its_blocks_and_back_holds_the_same_device_slots() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let mut scene = embodied(&store, &mut gpu, 2);
        let stations = walk(&mut scene, &store, &mut gpu);

        let loaded = stations[0];
        let far = stations[LAP.len() / 2];
        let home = stations[home_station(1)];
        let back = stations[home_station(LAPS)];

        // 1. The premise.
        assert!(
            loaded.blocks > 1,
            "the window holds {} block(s); nothing can churn",
            loaded.blocks
        );
        assert!(
        back.terrain_releases > 0 && back.terrain_freed > 0,
        "no departed block returned a terrain surface ({} cells torn down, {} surfaces freed) -- \
         the landblock terrain-surface teardown never ran, and every equality below is \
         then about a walk that released nothing",
        back.terrain_releases,
        back.terrain_freed
    );
        assert_eq!(
        loaded.centre, home.centre,
        "the walk did not come home: the window is centred on {:?} against the load's {:?}, so \
         the two readings are over different blocks and nothing below is a comparison",
        home.centre, loaded.centre
    );
        assert_eq!(
            loaded.centre, back.centre,
            "lap {LAPS} did not come home either"
        );
        assert_eq!(
            loaded.blocks, home.blocks,
            "the laps ended with a different window"
        );

        // 2. **The subject moved, and this is the part a homecoming equality cannot state for
        //    itself.** Two separate facts, because either alone is satisfiable by a walk that stood
        //    still.
        //
        //    (a) The far window and the load window share no block at all. `land_radius: 2` is a 5x5
        //    window, so two windows overlap only while their centres are within 2*2 = 4 blocks of each
        //    other, and the lap's far station is 6 blocks out diagonally. Asserted from the constants
        //    rather than from the observed centres, so it stays true if the lap is edited.
        //
        //    (b) The merge cache's residency at the far station is BELOW the load's. Since a surface
        //    only leaves this cache when its last cell lets go, at least `loaded - far` of the
        //    surfaces the load window held were destroyed on the way out -- and part 4's equality then
        //    says they were all rebuilt on the way home. With no release edge the residency could
        //    only ever rise, so this assertion is a leak detector in its own right and not merely a
        //    premise.
        let (fx, fy) = LAP[LAP.len() / 2 - 1];
        assert!(
        fx.min(fy) > 2 * 2,
        "the far station is ({fx},{fy}) blocks out and a land_radius-2 window is 5 blocks wide, \
         so the far window still overlaps the load's and the homecoming equality is about a \
         window that never fully turned over"
    );
        assert!(
        far.terrain < loaded.terrain,
        "the far station holds {} merged terrain surfaces against the load's {}. A surface only \
         leaves this cache when its last cell lets go, so a residency that did not fall means \
         none of the load window's surfaces were ever destroyed -- and the homecoming equality is \
         then satisfied by a leak rather than by a release",
        far.terrain,
        loaded.terrain
    );

        // 3. The caches still share. A merge cache that had simply been emptied on every scroll would
        //    hold a flat residency too, at the cost of re-compositing and re-uploading every surface.
        assert!(
        back.resolved > far.resolved && far.resolved > loaded.resolved,
        "surface groups resolved went {} -> {} -> {}; it must keep rising, or the walk stood still",
        loaded.resolved,
        far.resolved,
        back.resolved
    );
        assert!(
        u64::from(u32::try_from(loaded.terrain).expect("fits")) < back.terrain_freed,
        "only {} surfaces were ever freed against the {} a single window holds -- the walk is too \
         short for the residency to mean anything",
        back.terrain_freed,
        loaded.terrain
    );

        // 4. **The measurement, and a memo-only reading cannot make it.** The same twenty-five
        //    blocks at the load and at each homecoming, so the same merged surfaces. Unreleased
        //    terrain surfaces would raise the device count at lap 1 home with the memo flat, and lap 2
        //    would then match lap 1, because it introduces no merge key the first lap did not leak.
        assert_eq!(
        home.terrain, loaded.terrain,
        "lap 1 came home holding {} merged terrain surfaces where the load held {} over the same \
         blocks -- a block that left the window took its surfaces with it",
        home.terrain, loaded.terrain
    );
        assert_eq!(
        home.live, loaded.live,
        "lap 1 came home with {} live device slots against the load's {} over the same twenty-five \
         blocks. The memo read {} then and {} now, which is why this could not be seen from \
         `BakeCache` alone",
        home.live, loaded.live, loaded.bake_slots, home.bake_slots
    );
        assert_eq!(
            home.bake_slots, loaded.bake_slots,
            "the object half moved: {} pairs against the load's {} in the object memo",
            home.bake_slots, loaded.bake_slots
        );
        // And the second lap reproduces the first exactly, station for station, because one lap
        // cannot distinguish a leak from a working set.
        //
        // Station 0 is the **load**, so lap 1 is stations 1..=12 and lap 2 is 13..=24; the `+ 1` is
        // what keeps this a lap-against-lap comparison rather than a load-against-lap one.
        for i in 0..LAP.len() {
            let (a, b) = (
                stations[home_station(0) + 1 + i],
                stations[home_station(1) + 1 + i],
            );
            assert_eq!(
                (a.terrain, a.bake_slots, a.live, a.blocks, a.centre),
                (b.terrain, b.bake_slots, b.live, b.blocks, b.centre),
                "station {i} of lap 2 does not reproduce station {i} of lap 1: \
             terrain {} vs {}, bake {} vs {}, device live {} vs {}",
                a.terrain,
                b.terrain,
                a.bake_slots,
                b.bake_slots,
                a.live,
                b.live
            );
        }

        // The bake's own contents, as the control: the *reason* the two windows hold the same slots
        // has to be that they hold the same scene, not that both are empty.
        assert_eq!(
            home.scenery, loaded.scenery,
            "the windows do not hold the same scenery"
        );
        assert_eq!(
            home.statics, loaded.statics,
            "the windows do not hold the same statics"
        );
        assert!(
            loaded.scenery > 0 && loaded.statics > 0,
            "the load window baked nothing at all"
        );

        eprintln!(
            "home readings: load {} terrain surfaces / {} bake pairs / {} device live; lap 1 \
         home {} / {} / {}; lap {LAPS} home {} / {} / {} ({} cells torn down, {} surfaces freed, \
         {} groups resolved)",
            loaded.terrain,
            loaded.bake_slots,
            loaded.live,
            home.terrain,
            home.bake_slots,
            home.live,
            back.terrain,
            back.bake_slots,
            back.live,
            back.terrain_releases,
            back.terrain_freed,
            back.resolved
        );

        // 5. The tolerated failures.
        assert_eq!(
            back.terrain_unowned, 0,
            "a merged terrain surface was released twice"
        );
        assert_eq!(
            gpu.texture_table_stats().unknown_releases,
            0,
            "a release named a slot the texture table did not hold"
        );
        assert_eq!(
            gpu.descriptor_stats().invalid_releases,
            0,
            "a descriptor slot was released twice"
        );
        assert_eq!(
            gpu.descriptor_stats().exhaustions,
            0,
            "the descriptor heap ran out"
        );
    }

    /// **The second release site, and it is a different one.**
    ///
    /// A departed block gives its merged surfaces back through `WorldScene::release_departed_blocks`.
    /// A block that stays resident but is **re-meshed in place** gives them back through `build_slot`'s
    /// reuse arm, and that arm is a site the object half does not have: an object bake survives a
    /// re-stitch at an unchanged detail level, and a *terrain* bake never does, because
    /// `LandContext::generate` runs unconditionally and takes a fresh reference per cell every time.
    ///
    /// With `scenery_radius == land_radius` the reuse arm is reached for every resident slot on every
    /// scroll (`SlotWork::Mesh` with `wants_objects` and a matching `side_cell_count`). Set the
    /// scenery radius *inside* the window, as `world::streaming_slot_release` does for its own second
    /// site, and the arm is also reached with the objects being dropped. Both configurations are
    /// exercised, and the assertion is the same in each.
    #[test]
    fn a_block_re_meshed_in_place_returns_the_terrain_surfaces_its_previous_mesh_held() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        for radius in [1u32, 2u32] {
            let mut scene = embodied(&store, &mut gpu, radius);
            let stations = walk(&mut scene, &store, &mut gpu);
            let loaded = stations[0];
            let home = stations[home_station(1)];
            let back = stations[home_station(LAPS)];
            eprintln!(
                "scenery_radius {radius}: load {} terrain / {} device live, lap 1 home {} / {}, \
             lap {LAPS} home {} / {}; {} cells torn down, {} surfaces freed, {} blocks re-meshed \
             in place",
                loaded.terrain,
                loaded.live,
                home.terrain,
                home.live,
                back.terrain,
                back.live,
                back.terrain_releases,
                back.terrain_freed,
                back.remeshed
            );
            assert!(
                back.terrain_freed > 0,
                "no terrain surface was freed at scenery_radius {radius}"
            );
            // **The premise this test exists for, and it is a measurement rather than a configuration
            // argument.** The reuse arm is reachable only when a resident block gets `SlotWork::Mesh`
            // with `wants_objects` and an unchanged `side_cell_count`, which `scenery_radius`,
            // `land_radius` and the LOD ring table decide between them. Without this the two
            // equalities below are satisfied entirely by the *departed*-block site the previous test
            // already covers, and this whole test would be a duplicate that reads as a second site.
            assert!(
            back.remeshed > 0,
            "no block was re-meshed in place at scenery_radius {radius}, so `build_slot`'s reuse \
             arm never ran and the assertions below are about the departed-block site alone"
        );
            assert_eq!(
            home.terrain, loaded.terrain,
            "at scenery_radius {radius} lap 1 came home with {} merged terrain surfaces against \
             the load's {}",
            home.terrain, loaded.terrain
        );
            assert_eq!(
                home.live, loaded.live,
                "at scenery_radius {radius} lap 1 came home with {} live device slots against the \
             load's {}",
                home.live, loaded.live
            );
            assert_eq!(
                back.live, home.live,
                "the two laps disagree at scenery_radius {radius}"
            );
            assert_eq!(back.terrain_unowned, 0, "a surface was released twice");
            scene.release_textures(&mut gpu);
        }
        assert_eq!(
            gpu.descriptor_stats().invalid_releases,
            0,
            "a descriptor slot was released twice"
        );
        assert_eq!(
            gpu.texture_table_stats().unknown_releases,
            0,
            "a release named a slot the texture table did not hold"
        );
    }

    /// **Teardown.** `WorldScene::release_textures` returns the merged terrain surfaces as well as the
    /// baked ones; they are a large share of the slots a loaded 5x5 window holds.
    ///
    /// The premise is asserted first and it is the part that matters: the scene must actually hold
    /// merged surfaces before the teardown, or a teardown that frees nothing looks identical to one
    /// that frees everything. And **the device's live count after the teardown is the assertion**, not
    /// the number `release_textures` reports, because the one thing a self-reported figure cannot do
    /// is notice a population it does not enumerate.
    #[test]
    fn tearing_a_scene_down_returns_its_merged_terrain_surfaces_too() {
        let store = store();
        let mut gpu = crate::common::test_gpu(320, 240);
        let before = gpu.descriptor_usage().live;
        let mut scene = embodied(&store, &mut gpu, 2);
        let held = gpu.descriptor_usage().live;
        let terrain = scene.draw.stats.terrain_surfaces;
        let bake = scene.draw.stats.textures_uploaded;
        assert!(
            terrain > 0,
            "the scene holds no merged terrain surfaces; there is nothing to release"
        );
        assert!(bake > 0, "the scene holds no baked surfaces either");
        let handed_back = scene.release_textures(&mut gpu);
        let after = gpu.descriptor_usage().live;
        eprintln!(
            "teardown: {before} live before the load, {held} with the scene up ({terrain} merged \
         terrain surfaces and {bake} bake pairs of it), {handed_back} pairs handed back, {after} \
         live after"
        );
        assert_eq!(
            after,
            before,
            "the teardown left {} slot(s) live over the {before} the device held before the scene \
         existed; the scene held {terrain} merged terrain surfaces and {bake} bake pairs",
            after - before
        );
        assert!(
        u64::from(handed_back) >= u64::from(bake) + terrain as u64,
        "{handed_back} pairs handed back does not cover {bake} bake pairs plus {terrain} merged \
         terrain surfaces"
    );
        // A second teardown must be a no-op rather than a double release: the resident blocks still
        // name the keys the first one took.
        assert_eq!(
            scene.release_textures(&mut gpu),
            0,
            "the second teardown released something"
        );
        assert_eq!(
            gpu.texture_table_stats().unknown_releases,
            0,
            "a release named a slot the texture table did not hold"
        );
        assert_eq!(
            gpu.descriptor_stats().invalid_releases,
            0,
            "a descriptor slot was released twice"
        );
        assert_eq!(
            scene.draw.stats.terrain_unowned_releases, 0,
            "a merged terrain surface was released twice"
        );
    }
}
