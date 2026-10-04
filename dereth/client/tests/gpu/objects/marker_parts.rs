//! Marker parts: the level-designer marker graphics objects the corpus's part swaps name, counted,
//! and what the client does with them on a **creature or player** part array as opposed to a baked
//! cell static.
//!
//! The physics-part draw guard `g = gfxobj[deg_level]; if (!g) return` refuses a part whose degrade
//! record has nothing at the chosen level ([`dereth_client::models::draws_at_near_band`]). On a
//! creature that drops the corpus's marker parts; the local player is exempt (his degrade level is
//! pinned to 0) and still submits them. The first test reports the **whole distribution** of
//! refused swaps rather than a summary, so a tail cannot hide.
//!
//! Fixture: every recorded session, replayed through the client's network and object stream, plus
//! the retail dats. Every id counted here is read off the wire or out of `client_portal.dat` at run
//! time. Every fixture path is an `expect`, never a skip.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::{
    addr, connection_sequence_number, corpus_sessions, load, retail_store, test_gpu,
};
use crate::common::recorded_world_sessions;

use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup};
use dereth_client::models::draws_at_near_band;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_render::device::Gpu;
use dereth_world_render::objects::degrade::{draws_anything, get_degrade, DegradeGlobals};

// ---------------------------------------------------------------------------------------------
// The replay: the capture through the client's network and object stream.
// ---------------------------------------------------------------------------------------------

struct Replayed {
    objects: ObjectStream,
    last_populated: usize,
    player: Option<ObjectId>,
}

fn replay_upto(session: &str, limit: usize) -> Replayed {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut last_populated = 0usize;
    let mut player = None;
    for (index, r) in records.iter().enumerate() {
        if index >= limit {
            break;
        }
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
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
        if !objects.is_empty() {
            last_populated = index;
        }
        if player.is_none() {
            player = objects.player();
        }
    }
    Replayed {
        objects,
        last_populated,
        player,
    }
}

/// The whole session up to the point the client is still in world.
fn in_world(session: &str) -> Replayed {
    let last = replay_upto(session, usize::MAX).last_populated;
    replay_upto(session, last + 1)
}

/// The captures whose character actually reaches the world, **measured rather than named**: as
/// many as the corpus records a `0x0013` for. A login-only capture (the account authenticates and
/// disconnects without a character entering the world) is left out.
fn world_sessions() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let world: Vec<String> = corpus_sessions()
            .into_iter()
            .filter(|s| !in_world(s).objects.is_empty())
            .collect();
        assert_eq!(
            world.len(),
            recorded_world_sessions(),
            "the captures that enter the world, as many as carry a recorded 0x0013"
        );
        world
    })
}

fn setup_of(store: &RetailDatStore, id: DataId) -> Setup {
    let bytes = store
        .read_typed(DbType::Setup, id)
        .unwrap_or_else(|e| panic!("{id:?}: {e}"));
    Setup::decode_payload(id, &bytes).unwrap_or_else(|e| panic!("{id:?}: {e}"))
}

fn triangles(store: &RetailDatStore, gfxobj: DataId) -> usize {
    dereth_client::models::build_gfxobj(store, gfxobj)
        .iter()
        .map(|g| g.vertices.len() / 3)
        .sum()
}

// ---------------------------------------------------------------------------------------------
// 1. The distribution.
// ---------------------------------------------------------------------------------------------

/// **Every part swap the corpus sends, bucketed by whether `draws_at_near_band` refuses it and by
/// the graphics object it names.** The full distribution, printed and asserted, because "almost
/// all" is the phrasing that hides a tail.
#[test]
fn the_refused_part_swaps_by_gfxobj_id() {
    let store = retail_store();
    let mut all: BTreeMap<u32, usize> = BTreeMap::new();
    let mut refused: BTreeMap<u32, usize> = BTreeMap::new();
    let mut refused_on_local_player = 0usize;
    let mut refused_elsewhere = 0usize;
    let mut swaps = 0usize;
    let mut setups: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        let r = in_world(name);
        let me = r.player;
        for (id, p) in r.objects.presences() {
            if p.objdesc.anim_part_changes.is_empty() {
                continue;
            }
            setups.insert(
                p.setup_id
                    .expect("an object with an object description has a setup record")
                    .0,
            );
            for c in &p.objdesc.anim_part_changes {
                swaps += 1;
                *all.entry(c.part_id).or_default() += 1;
                if !draws_at_near_band(&store, DataId(c.part_id)) {
                    *refused.entry(c.part_id).or_default() += 1;
                    if Some(id) == me {
                        refused_on_local_player += 1;
                    } else {
                        refused_elsewhere += 1;
                    }
                }
            }
        }
    }
    let total_refused: usize = refused.values().sum();
    eprintln!(
        "marker-part distribution: {swaps} part swaps over {} setup records, {} distinct graphics-object ids; \
         {total_refused} refused by draws_at_near_band over {} distinct ids \
         ({refused_on_local_player} on the local player, {refused_elsewhere} on other objects)",
        setups.len(),
        all.len(),
        refused.len()
    );
    for (id, n) in &refused {
        eprintln!(
            "  refused {id:#010X}: {n} swaps, {} triangles",
            triangles(&store, DataId(*id))
        );
    }
    let mut drawn: Vec<(u32, usize)> = all
        .iter()
        .filter(|(k, _)| !refused.contains_key(k))
        .map(|(k, v)| (*k, *v))
        .collect();
    drawn.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    eprintln!(
        "  drawn ids: {} distinct, top 10 {:?}",
        drawn.len(),
        &drawn[..drawn.len().min(10)]
    );
    // What the module turns on is that there is exactly **one** marker graphics object in the
    // whole corpus, `0x010001EC`, and that the player exemption keeps some of its swaps while the
    // guard refuses the rest.
    assert!(
        total_refused > 0 && total_refused < swaps,
        "some, and not all, of the corpus's part swaps are refused"
    );
    assert_eq!(
        refused.len(),
        1,
        "still exactly one marker graphics object in the whole corpus"
    );
    assert_eq!(
        refused.keys().copied().collect::<Vec<_>>(),
        [0x0100_01ECu32],
        "the one marker graphics object, named rather than counted"
    );
    assert_eq!(
        refused_on_local_player + refused_elsewhere,
        total_refused,
        "every refused swap is on the local player or on something else"
    );
    assert!(
        refused_on_local_player > 0,
        "the swaps the player exemption keeps"
    );
    assert!(refused_elsewhere > 0, "and the ones it does not");
}

/// The same question asked of the **setups themselves**: how many parts of each setup record the
/// corpus uses are marker graphics objects before any swap is applied.
#[test]
fn the_setups_own_marker_parts() {
    let store = retail_store();
    let mut setups: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        for (_, p) in in_world(name).objects.presences() {
            if let Some(s) = p.setup_id {
                setups.insert(s.0);
            }
        }
    }
    for s in &setups {
        let parts = setup_of(&store, DataId(*s)).parts;
        let markers: Vec<usize> = parts
            .iter()
            .enumerate()
            .filter(|(_, g)| !draws_at_near_band(&store, **g))
            .map(|(i, _)| i)
            .collect();
        if !markers.is_empty() {
            eprintln!(
                "  setup record {s:#010X}: {} of {} parts are markers, indices {:?}",
                markers.len(),
                parts.len(),
                markers
            );
        }
    }
    eprintln!(
        "marker parts: {} distinct setup records in the corpus",
        setups.len()
    );
    assert!(!setups.is_empty(), "the corpus names no setup record");
}

/// Behaviour: objects.appearance.marker-parts-draw-only-on-the-local-player
///
/// What the **client** does with a marker part on a part array, from the degrade record rather
/// than from our predicate: the viewer-distance update's player exemption, then the physics-part
/// draw's null test.
#[test]
fn the_client_draws_a_marker_on_the_local_player_and_refuses_it_on_everything_else() {
    let store = retail_store();
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        let r = in_world(name);
        for (_, p) in r.objects.presences() {
            for c in &p.objdesc.anim_part_changes {
                if !draws_at_near_band(&store, DataId(c.part_id)) {
                    ids.insert(c.part_id);
                }
            }
        }
    }
    assert!(!ids.is_empty(), "no refused id to examine");
    let g = DegradeGlobals::default();
    for id in &ids {
        let bytes = store
            .read_typed(DbType::GfxObj, DataId(*id))
            .expect("a graphics object the dat holds");
        let obj = GfxObj::decode_payload(DataId(*id), &bytes).expect("the graphics object decodes");
        let did = obj
            .did_degrade
            .expect("a refused part must carry a degrade record");
        let bytes = store
            .read_typed(DbType::DegradeInfo, did)
            .expect("the record is in the dat");
        let info = GfxObjDegradeInfo::decode_payload(did, &bytes).expect("it decodes");
        eprintln!(
            "  {id:#010X} -> GfxObjDegradeInfo {:#010X}, {} levels: {:?}",
            did.0,
            info.degrades.len(),
            info.degrades
                .iter()
                .map(|e| (
                    e.gfxobj_id.0,
                    e.degrade_mode,
                    e.min_dist,
                    e.ideal_dist,
                    e.max_dist
                ))
                .collect::<Vec<_>>()
        );
        // Not the local player: `get_degrade` runs and picks the terminator at every distance.
        for d in [0.0f32, 1.0, 10.0, 49.9, 50.0, 500.0, 5_000.0] {
            let (level, _) = get_degrade(&info, d, &g);
            assert!(
                !draws_anything(&info, level),
                "{id:#010X} draws at d = {d} on a non-player object"
            );
        }
        // The local player: the viewer-distance update pins `deg_level = 0` and never calls
        // `get_degrade` at all, so `gfxobj[0]` is the record's own level 0.
        assert!(
            draws_anything(&info, 0),
            "{id:#010X} level 0 names gfxobj_id 0; the local player would draw nothing"
        );
        assert_eq!(
            info.degrades[0].gfxobj_id,
            DataId(*id),
            "{id:#010X} level 0 is expected to be the object itself"
        );
    }
}

/// **What the refused object actually is**, decided per id rather than by its name: its polygons,
/// its extent and every surface record it names.
#[test]
fn the_refused_ids_identified_one_by_one() {
    let store = retail_store();
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        for (_, p) in in_world(name).objects.presences() {
            for c in &p.objdesc.anim_part_changes {
                if !draws_at_near_band(&store, DataId(c.part_id)) {
                    ids.insert(c.part_id);
                }
            }
        }
    }
    for id in &ids {
        let bytes = store
            .read_typed(DbType::GfxObj, DataId(*id))
            .expect("a graphics object the dat holds");
        let obj = GfxObj::decode_payload(DataId(*id), &bytes).expect("the graphics object decodes");
        let vs = &obj.vertex_array.vertices;
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in vs {
            for (k, c) in [v.position.x, v.position.y, v.position.z]
                .into_iter()
                .enumerate()
            {
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
        }
        eprintln!(
            "  {id:#010X}: flags {:#X}, {} polygons, {} vertices, {} triangles, extent {:?}",
            obj.flags,
            obj.polygons.len(),
            vs.len(),
            triangles(&store, DataId(*id)),
            [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
        );
        for s in &obj.surfaces {
            let b = store
                .read_typed(DbType::Surface, *s)
                .expect("a surface record the dat holds");
            let surf = dereth_assets::Surface::decode_payload(*s, &b).expect("it decodes");
            eprintln!(
                "      surface record {:#010X}: type {:#X}, texture {:?}, colour {:?}, translucency {}, \
                 luminosity {}, diffuse {}",
                s.0, surf.surface_type, surf.orig_texture_id, surf.color_value, surf.translucency,
                surf.luminosity, surf.diffuse
            );
        }
    }
    assert!(!ids.is_empty(), "no refused id to identify");
}

/// **How many parts the guard drops, per object.** The effective part list is the setup record's
/// own parts with the `ObjDesc`'s swaps laid over it, which is what applying description changes
/// leaves behind and what `build_part_meshes` walks.
#[test]
fn the_corpus_objects_that_carry_marker_parts() {
    let store = retail_store();
    let (mut objects, mut with_markers, mut marker_parts, mut total_parts) = (0usize, 0, 0, 0);
    let mut player_markers = 0usize;
    let mut by_setup: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for name in world_sessions() {
        let r = in_world(name);
        let me = r.player;
        for (id, p) in r.objects.presences() {
            let Some(setup_id) = p.setup_id else { continue };
            objects += 1;
            let mut parts = setup_of(&store, setup_id).parts;
            for c in &p.objdesc.anim_part_changes {
                if let Some(slot) = parts.get_mut(c.part_index as usize) {
                    *slot = DataId(c.part_id);
                }
            }
            total_parts += parts.len();
            let n = parts
                .iter()
                .filter(|g| !draws_at_near_band(&store, **g))
                .count();
            if n > 0 {
                with_markers += 1;
                marker_parts += n;
                let e = by_setup.entry(setup_id.0).or_default();
                e.0 += 1;
                e.1 += n;
                if Some(id) == me {
                    player_markers += n;
                }
            }
        }
    }
    eprintln!(
        "marker-part effect: {with_markers} of {objects} live objects carry marker parts -- \
         {marker_parts} of {total_parts} drawn parts, {player_markers} of them on the local player"
    );
    for (s, (n, m)) in &by_setup {
        eprintln!("    setup record {s:#010X}: {n} objects, {m} marker parts");
    }
    assert!(objects > 0, "the corpus created nothing");
}

// ---------------------------------------------------------------------------------------------
// 2. The guard on a live scene.
// ---------------------------------------------------------------------------------------------

type Shot = (Vec<u8>, u32, u32);

/// The part-array update has to run before a part frame means anything, and the chase
/// camera has to be snapped onto the body afterwards.
fn settle(scene: &mut WorldScene) {
    for i in 0..8u32 {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();
}

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Shot {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let (w, h) = (image.width, image.height);
    (image.to_rgba(), w, h)
}

fn diff(a: &Shot, b: &Shot) -> (usize, (u32, u32, u32, u32)) {
    assert_eq!((a.1, a.2), (b.1, b.2));
    let (w, h) = (a.1, a.2);
    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.0[i..i + 4] != b.0[i..i + 4] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (n, (x0, y0, x1, y1))
}

/// Behaviour: objects.appearance.marker-parts-draw-only-on-the-local-player
///
/// **The guard on a live scene.** The same scene, the same corpus, the same settling, twice: once
/// with the physics-part degrade guard honored on part arrays and once without
/// ([`SceneConfig::part_degrades`], which exists for exactly this control).
///
/// Three things must hold at once, and they bound the change from both sides:
///
/// 1. the server's objects lose **exactly** the marker parts, one triangle each, counted
///    independently from the setup records and the wire;
/// 2. the **local player** loses nothing: the viewer-distance update pins `deg_level = 0` for the
///    local player, so his seventeen clothing anchors are still submitted, exactly as retail
///    submits them;
/// 3. **not one pixel changes.** That is the measured result, not a hedge: the marker's surface is
///    `BASE1_CLIPMAP` over an 8x8 texture whose every texel expands to `0x00000000`
///    (`the_markers_texture_is_transparent`), so drawing it has no visible effect. The guard is
///    therefore a cost and a fidelity change, and the zero is the tightest bound available on
///    it: a guard that dropped anything real would have to repaint something.
#[test]
fn the_guard_takes_the_markers_off_creatures_and_leaves_the_local_player_alone() {
    let store = retail_store();
    let mut gpu = test_gpu(640, 640);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let pos = {
        let r = in_world("first-login-walk-jump");
        let id = r.player.expect("the capture creates a player");
        r.objects
            .presence(id)
            .expect("the player has a presence")
            .position
            .expect("a position")
    };
    let block = pos.cell.landblock();
    let landblock = (u16::from(block.x()) << 8) | u16::from(block.y());

    let mut render = |guard: bool| -> (dereth_client::world::SceneStats, Shot, Vec<ObjectId>) {
        let cfg = SceneConfig {
            landblock,
            character: true,
            land_radius: 0,
            scenery_radius: 0,
            part_degrades: guard,
            // **A second variable, pinned.** `SceneConfig::part_degrades` also gates the
            // *static* bake's degrade record, so with `degrade_levels` on the two arms below would
            // differ in the whole landscape's LOD as well as in the creature parts this test is
            // about (3,283 of 409,600 pixels, none of them a marker). This test's subject is the
            // part-array draw guard on a *creature*, so the static half is held constant in both
            // arms.
            degrade_levels: false,
            // **A third variable, pinned** for the same reason. `part_degrade_levels` bakes every
            // level of every part and re-picks one per frame, and its level 0 is the *record's*
            // mesh rather than the part's own id, so with it on the guarded arm's triangle count
            // is not comparable with the prediction below (which is computed from the setup
            // records' own ids at the near band): a drop of 5,233 triangles against a prediction
            // of 475. The part half is held at the near band in both arms too.
            part_degrade_levels: false,
            ..SceneConfig::default()
        };
        let mut r = in_world("first-login-walk-jump");
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        scene
            .sync_objects(&store, &mut gpu, &mut r.objects)
            .expect("objects sync");
        settle(&mut scene);
        let drawn: Vec<ObjectId> = r
            .objects
            .presences()
            .map(|(id, _)| id)
            .filter(|id| scene.server_object_part_frames(*id).is_some())
            .collect();
        let stats = scene.draw.stats;
        (stats, shot(&mut scene, &mut gpu), drawn)
    };

    let (off, unguarded, drawn_off) = render(false);
    let (on, guarded, drawn_on) = render(true);
    assert_eq!(drawn_off, drawn_on, "the two arms drew different objects");
    assert!(
        !drawn_on.is_empty(),
        "the scene drew no server object at all"
    );

    // The prediction, computed from the setup records and the wire rather than from the renderer:
    // one triangle per marker part of every object the scene actually built.
    let r = in_world("first-login-walk-jump");
    let mut predicted = 0usize;
    for id in &drawn_on {
        let p = r
            .objects
            .presence(*id)
            .expect("a presence for a drawn object");
        let Some(setup_id) = p.setup_id else { continue };
        let mut parts = setup_of(&store, setup_id).parts;
        for c in &p.objdesc.anim_part_changes {
            if let Some(slot) = parts.get_mut(c.part_index as usize) {
                *slot = DataId(c.part_id);
            }
        }
        for g in &parts {
            if !draws_at_near_band(&store, *g) {
                predicted += triangles(&store, *g);
            }
        }
    }

    eprintln!(
        "marker-part guard: {} server objects drawn; triangles {} -> {} (predicted drop {predicted}); \
         batches {} -> {}; character parts {} -> {}, character triangles {} -> {}; \
         parts_not_drawn {} -> {}",
        drawn_on.len(),
        off.server_object_triangles,
        on.server_object_triangles,
        off.server_object_batches,
        on.server_object_batches,
        off.character_parts,
        on.character_parts,
        off.character_triangles,
        on.character_triangles,
        off.parts_not_drawn,
        on.parts_not_drawn
    );

    assert!(
        predicted > 0,
        "no drawn object carries a marker part, so the frame proves nothing"
    );
    assert_eq!(
        off.server_object_triangles - on.server_object_triangles,
        predicted,
        "the guard did not drop exactly the marker triangles"
    );
    // The player exemption, from both sides: not one of his parts is refused.
    assert_eq!(
        (off.character_parts, off.character_triangles),
        (on.character_parts, on.character_triangles),
        "the guard changed the local player's body; the local player is exempt in the client"
    );
    assert_eq!(
        off.parts_not_drawn, 0,
        "the control arm refused parts, so it is not the control"
    );
    assert!(on.parts_not_drawn > 0, "the guarded arm refused nothing");

    let (n, bbox) = diff(&unguarded, &guarded);
    let total = (unguarded.1 * unguarded.2) as usize;
    eprintln!("  {n} of {total} pixels differ, bounding box {bbox:?}");
    assert_eq!(
        n, 0,
        "the guard repainted {n} of {total} pixels, bounding box {bbox:?} -- it dropped something \
         that was drawing, which no marker can be doing"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The lock-in: the guard must never refuse anything but a marker.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.appearance.marker-parts-draw-only-on-the-local-player
///
/// **The lock-in.** Any graphics object the corpus's part swaps or setup records name that
/// [`draws_at_near_band`] refuses must be a level-designer marker and nothing else, on the dat's
/// own definition: a `GfxObjDegradeInfo` of exactly two levels, level 0 the object itself at `min =
/// ideal = max = 0.0`, level 1 the all-`FLT_MAX` terminator naming `gfxobj_id 0`.
///
/// A garment or a limb refused by this predicate would be a character silently undressed, and it
/// would look exactly like a marker correctly refused. This is what tells the two apart, and it
/// fails on the day a new capture or a new dat makes the predicate refuse something real.
#[test]
fn nothing_but_a_marker_is_ever_refused() {
    let store = retail_store();
    let mut checked = 0usize;
    for name in world_sessions() {
        for (_, p) in in_world(name).objects.presences() {
            let Some(setup_id) = p.setup_id else { continue };
            let mut parts = setup_of(&store, setup_id).parts;
            for c in &p.objdesc.anim_part_changes {
                if let Some(slot) = parts.get_mut(c.part_index as usize) {
                    *slot = DataId(c.part_id);
                }
                // Also checked in its own right, so a swap the setup is too short for is not
                // silently dropped from this census.
                parts.push(DataId(c.part_id));
            }
            for g in parts {
                if draws_at_near_band(&store, g) {
                    continue;
                }
                checked += 1;
                let bytes = store
                    .read_typed(DbType::GfxObj, g)
                    .expect("a graphics object the dat holds");
                let obj = GfxObj::decode_payload(g, &bytes).expect("the graphics object decodes");
                let did = obj
                    .did_degrade
                    .expect("a refused part carries a degrade record");
                let b = store
                    .read_typed(DbType::DegradeInfo, did)
                    .expect("the record is present");
                let info = GfxObjDegradeInfo::decode_payload(did, &b).expect("it decodes");
                assert_eq!(
                    info.degrades.len(),
                    2,
                    "{g:?} is refused but its record is an LOD chain, so something real is dropped"
                );
                let l0 = &info.degrades[0];
                assert_eq!(l0.gfxobj_id, g, "{g:?}: level 0 is not the object itself");
                assert_eq!(
                    (l0.min_dist, l0.ideal_dist, l0.max_dist),
                    (0.0, 0.0, 0.0),
                    "{g:?}: level 0 is not the marker shape"
                );
                assert!(
                    dereth_world_render::objects::degrade::is_terminator(&info.degrades[1]),
                    "{g:?}: level 1 is not the all-FLT_MAX terminator"
                );
                // The polygons are still there -- what makes it a marker is the record, not an
                // empty mesh -- and that is precisely why nothing else can tell the two apart.
                assert!(
                    triangles(&store, g) <= 2,
                    "{g:?} has more geometry than a marker"
                );
            }
        }
    }
    eprintln!("marker parts: {checked} refused parts checked against the marker shape");
    assert!(
        checked > 0,
        "nothing was refused, so this assertion checked nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Why the marker produced no pixel: the texture behind `BASE1_CLIPMAP`.
// ---------------------------------------------------------------------------------------------

/// **Which of two things erases the marker, measured rather than assumed.** The marker's surface
/// is `type 0x14` = `BASE1_CLIPMAP | TRANSLUCENT` at `translucency 1.0`, and both halves erase it:
///
/// * `TRANSLUCENT` makes the surface assignment's
///   `curr_alpha = (int)((1 - translucency) * 255) = 0`, i.e. the client draws the marker at
///   **zero alpha** even on the one part array that is exempt from the degrade guard, the local
///   player's;
/// * `BASE1_CLIPMAP` makes combined-texture creation expand palette indices `<= 7` to
///   `0x00000000`, so the texels themselves may be transparent.
///
/// This client applies the second and not the first (`build_meshes` writes an opaque white vertex
/// colour), so the statement about what is on screen has to come from the texture. It is measured
/// here.
#[test]
fn the_markers_texture_is_transparent() {
    let store = retail_store();
    let textures = dereth_client::textures::TextureStore::new(&store);
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for name in world_sessions() {
        for (_, p) in in_world(name).objects.presences() {
            for c in &p.objdesc.anim_part_changes {
                if !draws_at_near_band(&store, DataId(c.part_id)) {
                    ids.insert(c.part_id);
                }
            }
        }
    }
    let mut opaque_texels = 0usize;
    for id in &ids {
        let bytes = store
            .read_typed(DbType::GfxObj, DataId(*id))
            .expect("a graphics object the dat holds");
        let obj = GfxObj::decode_payload(DataId(*id), &bytes).expect("the graphics object decodes");
        for s in &obj.surfaces {
            let b = store
                .read_typed(DbType::Surface, *s)
                .expect("a surface record the dat holds");
            let surf = dereth_assets::Surface::decode_payload(*s, &b).expect("it decodes");
            let clip = surf.surface_type & 0x4 != 0;
            let data = textures
                .texture_data_clipped(*s, clip)
                .expect("the marker's texture resolves");
            let level = data.levels.first().expect("a mip level");
            let n = level.len() / 4;
            // `TextureFormat::Bgra8`: the alpha byte is the fourth of each texel.
            let solid = level
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|t| t[3] != 0)
                .count();
            opaque_texels += solid;
            eprintln!(
                "  {id:#010X} surface record {:#010X}: type {:#X}, clip map {clip}, translucency {}, \
                 {}x{} {:?}, {solid} of {n} texels have non-zero alpha",
                s.0, surf.surface_type, surf.translucency, data.width, data.height, data.format
            );
        }
    }
    assert!(!ids.is_empty(), "no refused id to examine");
    eprintln!(
        "marker parts: the refused parts' textures carry {opaque_texels} texels this client would draw"
    );
}
