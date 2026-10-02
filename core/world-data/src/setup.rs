//! A setup record's collision half, as `dereth_physics` wants it: the spheres and heights, and
//! the parts with their physics BSPs, bounds and degrade modes. Shared by the client and the
//! server; `dereth-client-runtime` re-exports every item.

use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::SetupGeometry;
use dereth_primitives::{DataId, Frame};

/// The collision half of a decoded setup record, as [`dereth_physics::LandSource`]'s neighbour trait
/// wants it.
///
/// This is the application's asset-to-physics adapter for setups. `physics_bsp` stays `None`
/// because a setup record has no BSP of its own —
/// the trees hang off the **parts**' graphics objects, which are loaded in
/// [`setup_geometry_with_parts`]. What this function alone produces is the
/// sphere arm of object-collision discovery.
#[must_use]
pub fn setup_geometry(s: &Setup) -> SetupGeometry {
    let sphere = |x: dereth_assets::Sphere| dereth_physics::geom::Sphere::new(x.center, x.radius);
    SetupGeometry {
        sorting_sphere: sphere(s.sorting_sphere),
        selection_sphere: sphere(s.selection_sphere),
        spheres: s.spheres.iter().copied().map(sphere).collect(),
        cyl_spheres: s
            .cylspheres
            .iter()
            .map(|c| dereth_physics::geom::CylSphere {
                low_pt: c.low_pt,
                radius: c.radius,
                height: c.height,
            })
            .collect(),
        // The dat stores `step_up_height` **before** `step_down_height`; the decoder already
        // honours that, so these are a straight rename.
        step_up_height: s.step_up_height,
        step_down_height: s.step_down_height,
        radius: s.radius,
        height: s.height,
        has_physics_bsp: s.has_physics_bsp,
        physics_bsp: None,
        allow_free_heading: s.allow_free_heading,
        // The parts are loaded by
        // [`setup_geometry_with_parts`], which wraps this and needs the
        // dat store to reach each part's graphics object. Empty here means "sphere arm".
        parts: Vec::new(),
    }
}

/// What loading a setup record's **parts** did, for the log line and the tests.
///
/// Reported rather than silently absorbed for the reason the module note already gives: a part
/// that fails to load makes the object take a geometry arm the client would not have taken, and
/// "the world looks right and nothing collides" is the failure this project has already paid for
/// twice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetupPartStats {
    /// Setup records whose parts were loaded.
    pub setups: u64,
    /// physics parts made during part-array initialization.
    pub parts: u64,
    /// Parts whose graphics object carries a physics BSP, making the part-array cache's
    /// physics-BSP predicate answer yes.
    pub parts_with_bsp: u64,
    /// A part graphics object that is missing or would not decode. Part-array initialization fails the
    /// **whole** array when one part is missing; here the part is dropped and the setup keeps the
    /// rest, because refusing the whole object would make a door that half-decodes intangible
    /// rather than approximate. Counted, and asserted to be zero over the retail dat.
    pub part_undecodable: u64,
    /// A setup with **no** placement frame at any key. Placement selection calls
    /// `set_placement_frame(NULL, 0)`, then the sequence's current-frame lookup
    /// returns NULL and the part update writes nothing — so every part keeps
    /// the physics part's constructor position, the identity frame in cell **0**. Walking such a
    /// part's BSP would test geometry at the origin of the world, so the parts are dropped and the
    /// object keeps the sphere arm.
    pub no_placement_frame: u64,
    /// Setups that carry a `placement_frames` entry at key `0x65`, which is the key
    /// final object initialization requests; the rest fall back to key `0`.
    pub placement_frame_65: u64,
    /// Parts whose graphics object carries degrade info with a first `degrade_mode` other than 1, so
    /// the always-2D predicate answers **yes** and
    /// outdoor-cell collection takes their bounding sphere rather than
    /// their box. Over the whole shipped corpus this is 15 of 22,960 part
    /// references, so the counter exists because a zero here would mean the arm had gone
    /// unreachable rather than that it is rare.
    pub parts_always_2d: u64,
    /// Setups whose serialized physics-BSP flag disagrees with
    /// the scan of their parts. The scan is what the client branches
    /// on; the field is read by nothing but `UnPack`/`Pack`.
    pub bsp_flag_disagrees: u64,
}

/// One setup record's collision half **with its parts**, which is what the BSP arm of
/// object-collision discovery needs.
///
/// [`setup_geometry`] is the spheres-and-heights half and is unchanged; this
/// adds part-array initialization and the placement frame installed during final
/// object initialization:
///
/// * one `SetupPart` entry, in order — the client's physics-part array;
/// * its frame from `placement_frames[0x65]`, or from `[0]` when `0x65` is absent, which is
///   exactly the part array's own placement-frame fallback;
/// * the setup's default part scale, or `(1, 1, 1)` when the setup carries none —
///   the physics part's constructor value, which part initialization then does not overwrite;
/// * the part graphics object's own `physics_bsp`, through
///   [`crate::env_cells::gfxobj_physics_bsp`] — the same conversion the building shell uses,
///   so a door and a wall are walked by identical code.
///
/// The composition with the object's own frame is **not** done here: it is
/// [`dereth_physics::SetupGeometry::placed_part`]'s, at collision time, because a setup is shared by
/// every placement of it and frame composition takes the object's frame as its first argument.
pub fn setup_geometry_with_parts(
    store: &RetailDatStore,
    s: &Setup,
    stats: &mut SetupPartStats,
) -> SetupGeometry {
    setup_geometry_with_parts_at(store, s, PLACEMENT_FRAME_DEFAULT, stats)
}

/// [`setup_geometry_with_parts`] at a **named** placement rather than the installed `0x65`.
///
/// The server re-poses an object — a create's animframe field, a `0xF748`, a `0xF749` — and the
/// parts move with it, up to 2.16 m for `RightHandCombat` over the retail setups. Colliding at a
/// pose the object is not drawn at is invisible until something walks through it, so the caller's
/// memo is keyed on the placement as well as the setup and the id is threaded here.
///
/// The placement-frame fallback chain is unchanged: the named
/// id, then key `0`, then nothing at all. This is still its own transcription rather than a call
/// into `dereth_client_runtime::models::placement_frames`, and `tests/dat/objects/resting_placement_pose.rs` and
/// `tests/dat/objects/server_placement.rs` assert the copies agree.
pub fn setup_geometry_with_parts_at(
    store: &RetailDatStore,
    s: &Setup,
    placement_id: u32,
    stats: &mut SetupPartStats,
) -> SetupGeometry {
    let mut g = setup_geometry(s);
    stats.setups += 1;
    if s.placement_frames.contains_key(&PLACEMENT_FRAME_DEFAULT) {
        stats.placement_frame_65 += 1;
    }
    let placement = s
        .placement_frames
        .get(&placement_id)
        .or_else(|| s.placement_frames.get(&0));
    let Some(placement) = placement else {
        stats.no_placement_frame += 1;
        return g;
    };
    let mut parts: Vec<dereth_physics::source::SetupPart> = Vec::with_capacity(s.parts.len());
    for (i, part_id) in s.parts.iter().enumerate() {
        // Part-array updating iterates the smaller of `num_parts` and the animation frame's part count;
        // the decoder reads exactly `num_parts` frames per placement, so this never trims — the
        // guard is here because a part with no frame would sit at the identity in cell 0.
        let Some(frame) = placement.frames.get(i).copied() else {
            break;
        };
        let decoded_part = store
            .read_typed(DbType::GfxObj, *part_id)
            .ok()
            .and_then(|b| {
                dereth_assets::GfxObj::decode_payload_in(store.era_of(*part_id), *part_id, &b).ok()
            });
        let (bsp, bound_box, drawing_sphere, degrade_mode) = match decoded_part {
            // The bounding box and the drawing sphere come out of the same decode
            // the physics tree does: bounding-box cell collection reads the mesh's drawing
            // bounds, while transit-cell discovery uses `physics_sphere ?: drawing_sphere`.
            Some(gfx) => (
                crate::env_cells::gfxobj_physics_bsp(&gfx).map(Arc::new),
                Some(gfx_bound_box(&gfx)),
                drawing_sphere(&gfx),
                // Outdoor-cell collection branches on the always-2D
                // predicate, derived from the part mesh's own degrade info in this decode.
                first_degrade_mode(store, &gfx),
            ),
            None => {
                stats.part_undecodable += 1;
                (None, None, None, None)
            }
        };
        stats.parts += 1;
        if bsp.is_some() {
            stats.parts_with_bsp += 1;
        }
        if matches!(degrade_mode, Some(m) if m != 1) {
            stats.parts_always_2d += 1;
        }
        parts.push(dereth_physics::source::SetupPart {
            placement_frame: frame,
            default_scale: s
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i).copied())
                .unwrap_or(dereth_primitives::Vec3::new(1.0, 1.0, 1.0)),
            physics_bsp: bsp,
            bound_box,
            drawing_sphere,
            first_degrade_mode: degrade_mode,
        });
    }
    g.parts = parts;
    if g.caches_physics_bsp() != g.has_physics_bsp {
        stats.bsp_flag_disagrees += 1;
    }
    g
}

/// The mesh's drawing bounds, computed during initialization as min/max over the
/// **vertex array** — the drawing vertices, not the physics polygons, and the client keeps no
/// other box. The part's bounding-box accessor returns these bounds, so they are the
/// complete mesh bounds used when collecting the object's cells.
///
/// An empty vertex array gives the zero box, which is the client's own `else` branch.
#[must_use]
pub fn gfx_bound_box(g: &dereth_assets::GfxObj) -> dereth_physics::geom::BBox {
    dereth_physics::geom::BBox::of_points(g.vertex_array.vertices.iter().map(|v| v.position))
}

/// The drawing tree's bounding sphere, which is the
/// root node's own sphere. See [`dereth_physics::source::SetupPart::drawing_sphere`]
/// for why the collision half needs it.
#[must_use]
pub fn drawing_sphere(g: &dereth_assets::GfxObj) -> Option<dereth_physics::Sphere> {
    let s = g.drawing_bsp.as_ref()?.nodes.first()?.sphere?;
    Some(dereth_physics::Sphere::new(s.center, s.radius))
}

/// The first degrade mode, which is the only part of the degrade info used by the renderer's
/// two-dimensional classification.
///
/// A graphics object's packed flag bit 3 carries a `GfxObjDegradeInfo` id; the client loads it in
/// the part setup and holds the pointer. `None` here is the client's `degrades == NULL`
/// **and** its `num_degrades == 0`, which `Always2D` answers identically.
#[must_use]
pub fn first_degrade_mode(store: &RetailDatStore, g: &dereth_assets::GfxObj) -> Option<i32> {
    let id = g.did_degrade?;
    let bytes = store.read_typed(DbType::DegradeInfo, id).ok()?;
    let info =
        dereth_assets::GfxObjDegradeInfo::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
    info.degrades.first().map(|d| d.degrade_mode)
}

/// Final object initialization installs placement `0x65`. The key has no known name; `0x65` is
/// the literal.
pub const PLACEMENT_FRAME_DEFAULT: u32 = 0x65;

/// Build the one-part setup record the client wraps a **`0x01……`
/// graphics-object id** in.
///
/// Physics-object construction accepts a `GfxObj` id as well as a setup id
/// (part-array initialization switches on the inferred resource type), and every field of
/// the setup it builds matches retail:
///
/// * **one part**, whose id is the `GfxObj` id itself;
/// * **one `placement_frames` entry at key `0`**, holding one default identity frame — the
///   identity. That is why the building shell needs no composition;
/// * `sorting_sphere` from the root sphere of the mesh's physics tree or, if absent,
///   its drawing tree;
/// * **everything else default**: no sphere or cylinder-sphere, `default_scale` NULL (so
///   `gfxobj_scale` stays at the physics part's `1`), zero radius and height.
///
/// So such an object is solid **only** through part-array collision checking, which is why the
/// `simple_setup` counters in `CellStaticStats` and `ObjectPhysicsStats` count these setups.
pub fn simple_setup_geometry(
    store: &RetailDatStore,
    gfxobj: DataId,
    stats: &mut SetupPartStats,
) -> Option<SetupGeometry> {
    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
    let g = dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes).ok()?;
    let bsp = crate::env_cells::gfxobj_physics_bsp(&g).map(Arc::new);
    stats.setups += 1;
    stats.parts += 1;
    if bsp.is_some() {
        stats.parts_with_bsp += 1;
    }
    let sorting_sphere = bsp
        .as_ref()
        .and_then(|t| t.root().map(|n| n.sphere))
        .unwrap_or_default();
    Some(SetupGeometry {
        sorting_sphere,
        parts: vec![dereth_physics::source::SetupPart {
            placement_frame: Frame::default(),
            default_scale: dereth_primitives::Vec3::new(1.0, 1.0, 1.0),
            physics_bsp: bsp,
            bound_box: Some(gfx_bound_box(&g)),
            drawing_sphere: drawing_sphere(&g),
            first_degrade_mode: first_degrade_mode(store, &g),
        }],
        ..SetupGeometry::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALUVIAN_MALE_SETUP: DataId = DataId(0x0200_0001);

    // Oracle: the retail dats, read through the setup decoder. Contract 9.11 -- `step_up_height` is read
    // **before** `step_down_height`, and the Aluvian male's are 0.6 and 1.5, not the other way
    // round. A transposed pair is invisible on flat ground and wrong on every step.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_setup_geometry_carries_the_spheres_and_step_heights_in_the_documented_order() {
        // Absent dats fail rather than skip: a checkout with no retail dats would otherwise
        // report `... ok` having read no setup.
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let bytes = store
            .read_typed(DbType::Setup, ALUVIAN_MALE_SETUP)
            .expect("reads");
        let s = Setup::decode_payload(ALUVIAN_MALE_SETUP, &bytes).expect("decodes");
        let g = setup_geometry(&s);
        assert_eq!(g.spheres.len(), 2, "feet and chest");
        assert!(
            (g.step_up_height - 0.6).abs() < 1e-6,
            "{}",
            g.step_up_height
        );
        assert!(
            (g.step_down_height - 1.5).abs() < 1e-6,
            "{}",
            g.step_down_height
        );
        assert!(
            g.step_down_height > g.step_up_height,
            "the pair is not transposed"
        );
        assert!((g.height - 1.835).abs() < 1e-3, "{}", g.height);
        // Sphere-path initialization stores at most two, which is what the transition
        // system actually sees.
        assert_eq!(g.path_spheres().len(), 2);
    }
}
