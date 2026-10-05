//! Part arrays → per-part draw frames.
//!
//! The part array's init and update, its scale setter and the frame combine; a physics part's
//! viewer-distance updates and its draw-frame calculation; the device's cell update; and the
//! shadow part's insertion sort.
//!
//! This module preserves the part hierarchy, transforms, viewer distance and LOD selection.
//!
//! Rendering consumes a part array's current draw frames and meshes; it does **not** advance them.
//! Part-array updates, frame playback, motion tables and hooks run in the animation system.
//! The resulting animation frame arrives here as data.

use dereth_primitives::{Frame, Vec3};

use crate::objects::degrade::{calc_draw_frame, DegradeMode};
use {
    dereth_terrain::math::combine, dereth_terrain::math::l2g,
    dereth_terrain::math::localtoglobalvec, dereth_terrain::math::V3,
};

/// The horizontal distance past which the per-cell object update stops measuring each object and
/// hands every object in a land cell the cell's own distance and direction
/// ([`cell_viewer_distance`]).
pub const PER_PART_DISTANCE: f32 = 50.0;

/// One part of an object, as far as drawing is concerned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartDraw {
    /// The part's world frame, from the animation frame composed with
    /// the object frame.
    pub pos: Frame,
    /// The draw frame after the billboarding mode is applied.
    pub draw_pos: Frame,
    /// `setup.default_scale[i] ⊙ scale`, or `scale` when the setup has none. Installed before the
    /// draw; it scales the **mesh**, while the part's local offset is scaled during frame composition.
    pub gfxobj_scale: Vec3,
    /// Distance from the viewer to the part's sort center. Also the key used to sort a cell's parts.
    pub cypt: f32,
    /// The selected degrade level and its billboarding mode.
    pub deg_level: usize,
    pub deg_mode: DegradeMode,
    /// `draw_state` bit 0 — the NoDraw flag. Setting a translucency of 1.0 sets it too.
    pub no_draw: bool,
}

/// Scaling frame composition: the object's rotation is
/// applied to `scale ⊙ af.origin` and the rotations are composed.
///
/// The part's local offset is therefore scaled here, while the part's own mesh is scaled separately
/// through `gfxobj_scale`. Conflating the two makes large creatures' parts drift apart.
#[must_use]
pub fn combine_scaled(obj_frame: &Frame, anim_frame: &Frame, scale: Vec3) -> Frame {
    let scaled = Frame::new(
        Vec3::new(
            anim_frame.origin.x * scale.x,
            anim_frame.origin.y * scale.y,
            anim_frame.origin.z * scale.z,
        ),
        anim_frame.rotation,
    );
    combine(obj_frame, &scaled)
}

/// Update the parts against an object frame.
///
/// ```text
/// af = current animation frame
/// if there is no af: nothing happens (parts keep their last frames)
/// n = min(num_parts, af.num_parts)
/// for i in 0 .. n-1: parts[i].pos.frame = combine_scaled(obj_frame, af.frame[i], scale)
/// ```
///
/// Note the `min`: an animation frame with fewer parts than the setup leaves the surplus parts
/// where they were, and a `None` animation frame leaves **all** of them where they were.
pub fn update_parts(
    parts: &mut [PartDraw],
    obj_frame: &Frame,
    anim_frames: Option<&[Frame]>,
    scale: Vec3,
) {
    let Some(af) = anim_frames else { return };
    let n = parts.len().min(af.len());
    for i in 0..n {
        parts[i].pos = combine_scaled(obj_frame, &af[i], scale);
    }
}

/// Set `parts[i].gfxobj_scale = default_scale[i] ⊙ s`,
/// or `= s` when the setup has no default scale.
pub fn set_scale(parts: &mut [PartDraw], default_scale: Option<&[Vec3]>, scale: Vec3) {
    for (i, p) in parts.iter_mut().enumerate() {
        p.gfxobj_scale = match default_scale.and_then(|d| d.get(i)) {
            Some(d) => Vec3::new(d.x * scale.x, d.y * scale.y, d.z * scale.z),
            None => scale,
        };
    }
}

/// The **within-50-units** path, where each part measures itself.
///
/// ```text
/// c = gfxobj_scale ⊙ gfxobj[0]->sort_center
/// v = offset from viewer position to the transformed center c
/// cypt = |v|
/// viewer_heading = (cypt <= 0.0002) ? (0,0,1) : v / cypt
/// ```
///
/// Returns `(cypt, viewer_heading)`. `sort_center` is the `GfxObj`'s own, in part-local space, so it
/// goes through the part frame.
#[must_use]
pub fn update_viewer_distance(part: &PartDraw, sort_center: Vec3, viewer_pos: Vec3) -> (f32, Vec3) {
    let c = Vec3::new(
        sort_center.x * part.gfxobj_scale.x,
        sort_center.y * part.gfxobj_scale.y,
        sort_center.z * part.gfxobj_scale.z,
    );
    let world = part
        .pos
        .origin
        .add(localtoglobalvec(l2g(part.pos.rotation), c));
    viewer_distance_and_heading(world, viewer_pos)
}

/// Viewer-distance tail for a caller that already holds the sort center in
/// world space.
///
/// ```text
/// v = sort_centre_world - viewer_pos
/// cypt = |v|
/// viewer_heading = (cypt <= 0.0002) ? (0,0,1) : v / cypt
/// ```
///
/// Split out of [`update_viewer_distance`] because the baked-static path needs the *heading*
/// too: a static's `pos.frame` never moves, so `WorldScene`'s bake precomputes the
/// world sort centre once and the per-frame work is only this tail. One implementation and two
/// callers keeps the degenerate branch on both paths.
///
/// The degenerate branch matters to a billboard specifically: a part sitting exactly on the camera
/// has no heading to face, and without the `(0,0,1)` fallback `calc_draw_frame` would be handed a
/// zero vector and produce a NaN frame.
#[must_use]
pub fn viewer_distance_and_heading(sort_center_world: Vec3, viewer_pos: Vec3) -> (f32, Vec3) {
    let v = sort_center_world.sub(viewer_pos);
    let cypt = v.magnitude();
    let heading = if cypt <= dereth_terrain::consts::EPSILON {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        v.mul(1.0 / cypt)
    };
    (cypt, heading)
}

/// The object-level half of the viewer-distance update: whether an object's parts measure
/// themselves or are all handed the object's own distance and heading.
///
/// ```text
/// v = object_origin - viewer_pos
/// cypt = |v|
/// if v.x² + v.y² < share_distance_2dsq:  every part measures itself   -> None
/// else: heading = (cypt <= 0.0002) ? (0,0,1) : v / cypt
///       every part takes (cypt, heading)                              -> Some((cypt, heading))
/// ```
///
/// The test is on the **horizontal** distance and against the governor's squared share distance
/// ([`crate::degrade_loop::DegradeLevel::share_distance_2dsq`]); what is handed on is the full
/// distance from the viewer to the object's origin, not to any part's sort centre. A part handed
/// the object's distance still divides it by its own `gfxobj_scale.z` before the lookup, and
/// sorts at it.
///
/// Only a horizontal distance that compares below the share distance keeps the parts measuring
/// themselves, so an unordered one shares.
#[must_use]
pub fn shared_viewer_distance(
    object_origin: Vec3,
    viewer_pos: Vec3,
    share_distance_2dsq: f32,
) -> Option<(f32, Vec3)> {
    let v = object_origin.sub(viewer_pos);
    if v.x * v.x + v.y * v.y < share_distance_2dsq {
        None
    } else {
        Some(viewer_distance_and_heading(object_origin, viewer_pos))
    }
}

/// The horizontal centre of the land cell holding `p` (`z` is zero), in any frame whose origin is
/// a landblock corner: the point a land cell's viewer distance is measured to.
#[must_use]
pub fn land_cell_centre(p: Vec3) -> Vec3 {
    use dereth_primitives::num::consts::CELL_SIZE;
    let half = CELL_SIZE * 0.5;
    Vec3::new(
        (p.x / CELL_SIZE).floor() * CELL_SIZE + half,
        (p.y / CELL_SIZE).floor() * CELL_SIZE + half,
        0.0,
    )
}

/// The cell-level half of the viewer-distance update, which runs before the object-level one.
///
/// ```text
/// v = cell_centre - viewer_pos ; v.z = 0
/// d = sqrt(v.x² + v.y²)
/// if d > 50:  every object in the cell takes (d, v / d)      -> Some((d, heading))
/// else:       each object runs its own share test            -> None
/// ```
///
/// Only land cells have a distance; an interior cell always leaves its objects to their own
/// test. The distance and the heading are both **horizontal**, so a far cell's objects face a
/// level heading whatever the camera's height. Only a distance that compares above
/// [`PER_PART_DISTANCE`] hands the cell's distance out, so an unordered one does not.
#[must_use]
pub fn cell_viewer_distance(cell_centre: Vec3, viewer_pos: Vec3) -> Option<(f32, Vec3)> {
    let x = cell_centre.x - viewer_pos.x;
    let y = cell_centre.y - viewer_pos.y;
    let d = (x * x + y * y).sqrt();
    if d > PER_PART_DISTANCE {
        let f = 1.0 / d;
        Some((d, Vec3::new(x * f, y * f, 0.0)))
    } else {
        None
    }
}

/// An object's viewer distance and heading when every one of its parts is handed the same one:
/// its land cell's past [`PER_PART_DISTANCE`] ([`cell_viewer_distance`]), otherwise its own past
/// the share distance ([`shared_viewer_distance`]); `None` when each part measures itself.
///
/// `outdoors` is whether the object's cell is a land cell; the cell is the one under its origin.
#[must_use]
pub fn object_viewer_distance(
    object_origin: Vec3,
    outdoors: bool,
    viewer_pos: Vec3,
    share_distance_2dsq: f32,
) -> Option<(f32, Vec3)> {
    outdoors
        .then(|| cell_viewer_distance(land_cell_centre(object_origin), viewer_pos))
        .flatten()
        .or_else(|| shared_viewer_distance(object_origin, viewer_pos, share_distance_2dsq))
}

/// One part's viewer distance and heading: the object's when [`shared_viewer_distance`] handed
/// them out, otherwise the part's own from [`update_viewer_distance`].
#[must_use]
pub fn part_viewer_distance(
    part: &PartDraw,
    sort_center: Vec3,
    viewer_pos: Vec3,
    shared: Option<(f32, Vec3)>,
) -> (f32, Vec3) {
    shared.unwrap_or_else(|| update_viewer_distance(part, sort_center, viewer_pos))
}

/// The LOD selection for one part, as `UpdateViewerDistance` performs it.
///
/// The **player is always full detail**: the call runs only when the object degrades and is not the
/// player, so the local player never degrades however far the camera pulls back.
///
/// Note the argument: `cypt / gfxobj_scale.z`, not `cypt` — a scaled-up object degrades at
/// proportionally longer range.
pub fn select_level(
    part: &mut PartDraw,
    degrades: Option<&dereth_assets::motion::GfxObjDegradeInfo>,
    is_player: bool,
    cypt: f32,
    viewer_heading: Vec3,
    globals: &crate::objects::degrade::DegradeGlobals,
) {
    part.cypt = cypt;
    let (level, mode) = match degrades {
        Some(d) if !is_player => {
            let z = part.gfxobj_scale.z;
            let dist = if z != 0.0 { cypt / z } else { cypt };
            crate::objects::degrade::get_degrade(d, dist, globals)
        }
        _ => (0, DegradeMode::None),
    };
    part.deg_level = level;
    part.deg_mode = mode;
    part.draw_pos = calc_draw_frame(&part.pos, mode, viewer_heading);
}

/// Sort a cell's shadow part list **descending by
/// `cypt`** (viewer distance), farthest first, so the part-cell draw draws back to front within the cell.
///
/// The client's loop shifts an element toward a lower index while its distance is *greater* than the
/// key, which yields descending order. It is an **insertion sort**, so it is stable: two parts at
/// exactly the same distance keep their registration order, and that decides which of two coplanar
/// translucent surfaces wins.
pub fn insertion_sort_by_cypt(parts: &mut [PartDraw]) {
    insertion_sort_by_cypt_key(parts, |p| p.cypt);
}

/// [`insertion_sort_by_cypt`] over anything that carries a viewer distance.
///
/// Split out because the same sort is needed over the client's *draw submissions*
/// rather than over `PartDraw`s: this build draws a moving object's parts through
/// [`crate::objects::alpha::AlphaLists`] and the entry it queues is not a `PartDraw`. There is one
/// implementation and two callers, the same shape as
/// [`viewer_distance_and_heading`], so the loop has only one implementation.
///
/// The loop is exactly the client's: shift while the neighbour's distance is **greater** than
/// the key's, which yields descending order and is stable, so equal distances keep registration
/// order.
pub fn insertion_sort_by_cypt_key<T: Copy>(items: &mut [T], cypt: impl Fn(&T) -> f32) {
    for i in 1..items.len() {
        let key = items[i];
        let k = cypt(&key);
        let mut j = i;
        while j > 0 && cypt(&items[j - 1]) < k {
            items[j] = items[j - 1];
            j -= 1;
        }
        items[j] = key;
    }
}

/// Where one item of a merged far-to-near submission comes from: the `n`th of the first list or
/// of the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merged {
    First(usize),
    Second(usize),
}

/// Two submission lists, each already far to near, merged into the one order the part sort would
/// have given their concatenation: descending viewer distance, and at equal distance the first
/// list's item first, as the stable sort keeps registration order.
///
/// An emitter's particles are parts of their object, sorted with every other part, so this is
/// how the particles take their places among the objects' parts.
#[must_use]
pub fn merge_far_to_near(first: &[f32], second: &[f32]) -> Vec<Merged> {
    let mut out = Vec::with_capacity(first.len() + second.len());
    let (mut i, mut j) = (0, 0);
    while i < first.len() || j < second.len() {
        let take_second = match (first.get(i), second.get(j)) {
            (Some(a), Some(b)) => b > a,
            (None, Some(_)) => true,
            _ => false,
        };
        if take_second {
            out.push(Merged::Second(j));
            j += 1;
        } else {
            out.push(Merged::First(i));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    /// Oracle: the part sort is a stable descending insertion sort over every part, particles
    /// included, so merging two descending lists gives the sort of their concatenation.
    #[test]
    fn two_far_to_near_lists_merge_as_the_part_sort_of_both() {
        let parts = [90.0, 40.0, 40.0, 5.0];
        let particles = [60.0, 40.0, 1.0];
        let merged = merge_far_to_near(&parts, &particles);
        assert_eq!(
            merged,
            vec![
                Merged::First(0),
                Merged::Second(0),
                Merged::First(1),
                Merged::First(2),
                Merged::Second(1),
                Merged::First(3),
                Merged::Second(2),
            ]
        );
        // The same answer as sorting the concatenation.
        let mut all: Vec<(f32, Merged)> = parts
            .iter()
            .enumerate()
            .map(|(i, d)| (*d, Merged::First(i)))
            .chain(
                particles
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (*d, Merged::Second(i))),
            )
            .collect();
        insertion_sort_by_cypt_key(&mut all, |x| x.0);
        assert_eq!(all.iter().map(|x| x.1).collect::<Vec<_>>(), merged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::Quat;

    fn part(pos: Frame) -> PartDraw {
        PartDraw {
            pos,
            draw_pos: pos,
            gfxobj_scale: Vec3::new(1.0, 1.0, 1.0),
            cypt: 0.0,
            deg_level: 0,
            deg_mode: DegradeMode::None,
            no_draw: false,
        }
    }

    /// Oracle: frame composition applies the object's
    /// rotation to `scale ⊙ af.origin` and composes the rotations, so the **part's local offset is
    /// scaled but the part's own mesh is scaled separately**.
    #[test]
    fn the_part_offset_is_scaled_but_the_mesh_scale_is_separate() {
        let obj = Frame::new(Vec3::new(100.0, 0.0, 0.0), Quat::IDENTITY);
        let af = [Frame::new(Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY)];
        let mut parts = vec![part(Frame::default())];
        update_parts(&mut parts, &obj, Some(&af), Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(
            parts[0].pos.origin,
            Vec3::new(106.0, 0.0, 0.0),
            "2 * 3 + 100"
        );
        // gfxobj_scale is untouched by UpdateParts: it comes from SetScaleInternal.
        assert_eq!(parts[0].gfxobj_scale, Vec3::new(1.0, 1.0, 1.0));
        set_scale(&mut parts, None, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(parts[0].gfxobj_scale, Vec3::new(3.0, 3.0, 3.0));
        // With a setup default scale the two multiply component-wise.
        set_scale(
            &mut parts,
            Some(&[Vec3::new(2.0, 1.0, 0.5)]),
            Vec3::new(3.0, 3.0, 3.0),
        );
        assert_eq!(parts[0].gfxobj_scale, Vec3::new(6.0, 3.0, 1.5));
    }

    /// Oracle: `UpdateParts` — with no animation frame nothing happens (parts keep their last
    /// frames) — and the `min(num_parts, af.num_parts)` bound.
    #[test]
    fn a_missing_or_short_animation_frame_leaves_parts_where_they_were() {
        let obj = Frame::new(Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY);
        let mut parts = vec![part(Frame::default()), part(Frame::default())];
        update_parts(&mut parts, &obj, None, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(
            parts[0].pos,
            Frame::default(),
            "no animation frame, no movement"
        );
        let af = [Frame::new(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY)];
        update_parts(&mut parts, &obj, Some(&af), Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(parts[0].pos.origin, Vec3::new(6.0, 0.0, 0.0));
        assert_eq!(
            parts[1].pos,
            Frame::default(),
            "the surplus part is left alone"
        );
    }

    /// Oracle: the viewer-distance update -- `distance = |v|` and
    /// `viewer_heading = (distance <= 0.0002) ? (0,0,1) : v / distance`. The degenerate branch is what stops
    /// a part sitting exactly on the camera from producing a NaN billboard.
    #[test]
    fn viewer_distance_and_its_degenerate_branch() {
        let p = part(Frame::new(Vec3::new(3.0, 4.0, 0.0), Quat::IDENTITY));
        let (d, h) = update_viewer_distance(&p, Vec3::ZERO, Vec3::ZERO);
        assert!((d - 5.0).abs() < 1e-5);
        assert!((h.x - 0.6).abs() < 1e-5 && (h.y - 0.8).abs() < 1e-5);
        // A part exactly at the viewer.
        let p = part(Frame::default());
        let (d, h) = update_viewer_distance(&p, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(d, 0.0);
        assert_eq!(
            h,
            Vec3::new(0.0, 0.0, 1.0),
            "the degenerate heading is straight up"
        );
        // The sort centre is scaled by gfxobj_scale before being transformed.
        let mut p = part(Frame::default());
        p.gfxobj_scale = Vec3::new(2.0, 1.0, 1.0);
        let (d, _) = update_viewer_distance(&p, Vec3::new(3.0, 0.0, 0.0), Vec3::ZERO);
        assert!((d - 6.0).abs() < 1e-5, "the sort centre is scaled: {d}");
    }

    /// Oracle: the player is always full detail, and the degrade
    /// distance argument is `distance / gfxobj_scale.z`, not viewer distance.
    #[test]
    fn the_player_never_degrades_and_scale_stretches_the_bands() {
        use dereth_assets::motion::{GfxObjDegradeInfo, GfxObjInfo};
        use dereth_primitives::DataId;
        let info = GfxObjDegradeInfo {
            id: DataId(0x1100_0000),
            degrades: vec![
                GfxObjInfo {
                    gfxobj_id: DataId(1),
                    degrade_mode: 1,
                    min_dist: 0.0,
                    ideal_dist: 100.0,
                    max_dist: 200.0,
                },
                GfxObjInfo {
                    gfxobj_id: DataId(2),
                    degrade_mode: 5,
                    min_dist: 100.0,
                    ideal_dist: 400.0,
                    max_dist: 800.0,
                },
            ],
        };
        let g = crate::objects::degrade::DegradeGlobals::default();
        let mut p = part(Frame::default());
        select_level(
            &mut p,
            Some(&info),
            false,
            150.0,
            Vec3::new(0.0, 1.0, 0.0),
            &g,
        );
        assert_eq!(
            p.deg_level, 1,
            "150 is past the first ideal distance of 100"
        );
        // The same object scaled 2x in z degrades at twice the range: 150/2 = 75 < 100.
        let mut p2 = part(Frame::default());
        p2.gfxobj_scale = Vec3::new(1.0, 1.0, 2.0);
        select_level(
            &mut p2,
            Some(&info),
            false,
            150.0,
            Vec3::new(0.0, 1.0, 0.0),
            &g,
        );
        assert_eq!(
            p2.deg_level, 0,
            "a 2x-scaled object stays at level 0 twice as far out"
        );
        // The player never degrades, however far away.
        let mut pl = part(Frame::default());
        select_level(
            &mut pl,
            Some(&info),
            true,
            10_000.0,
            Vec3::new(0.0, 1.0, 0.0),
            &g,
        );
        assert_eq!(pl.deg_level, 0);
        assert_eq!(pl.deg_mode, DegradeMode::None);
    }

    /// Oracle: cell preparation sorts the shadow-part list
    /// **descending by the part's viewer distance** — farthest first ... i.e. **back to front** within a cell —
    /// the same direction as the cell and block orders. It is an insertion sort, so equal
    /// distances keep registration order.
    #[test]
    fn parts_sort_farthest_first_and_ties_keep_registration_order() {
        let mut parts: Vec<PartDraw> = [5.0f32, 1.0, 9.0, 3.0]
            .into_iter()
            .map(|d| {
                let mut p = part(Frame::default());
                p.cypt = d;
                p
            })
            .collect();
        insertion_sort_by_cypt(&mut parts);
        assert_eq!(
            parts.iter().map(|p| p.cypt).collect::<Vec<_>>(),
            vec![9.0, 5.0, 3.0, 1.0],
            "descending: farthest first"
        );
        // Stability on ties: the two parts at distance 4 keep their relative order, distinguished
        // here by their deg_level.
        let mut parts: Vec<PartDraw> = (0..4u8)
            .map(|i| {
                let mut p = part(Frame::default());
                p.cypt = 4.0;
                p.deg_level = usize::from(i);
                p
            })
            .collect();
        insertion_sort_by_cypt(&mut parts);
        assert_eq!(
            parts.iter().map(|p| p.deg_level).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    /// Oracle: beyond 50 units the **cell's** distance
    /// and direction are reused for every object in it; within 50 units each part measures itself.
    /// The constant is what the two paths hinge on.
    #[test]
    fn the_per_part_distance_threshold_is_fifty() {
        assert_eq!(PER_PART_DISTANCE, 50.0);
    }

    /// Two parts of one object, either side of its origin at `(30, 0, 0)`, each with its own
    /// sort centre.
    fn two_parts() -> [PartDraw; 2] {
        [
            part(Frame::new(Vec3::new(29.0, 1.0, 0.0), Quat::IDENTITY)),
            part(Frame::new(Vec3::new(31.0, -1.0, 2.0), Quat::IDENTITY)),
        ]
    }

    /// Behaviour: rendering.degrade.past-the-share-distance-every-part-takes-the-objects-distance-and-heading
    #[test]
    fn past_the_share_distance_every_part_takes_the_objects_distance_and_heading() {
        let origin = Vec3::new(30.0, 0.0, 0.0);
        let viewer = Vec3::ZERO;
        let shared = shared_viewer_distance(origin, viewer, 25.0).expect("30 m is past 5 m");
        assert!((shared.0 - 30.0).abs() < 1e-4, "{shared:?}");
        assert!((shared.1.x - 1.0).abs() < 1e-6, "{shared:?}");
        for p in two_parts() {
            let (cypt, heading) = part_viewer_distance(&p, Vec3::ZERO, viewer, Some(shared));
            assert_eq!((cypt, heading), shared, "the object's, not the part's own");
            assert_ne!(
                update_viewer_distance(&p, Vec3::ZERO, viewer),
                shared,
                "the part's own measurement differs"
            );
        }
        // Two billboarding parts handed the same heading turn to the same orientation.
        let g = crate::objects::degrade::DegradeGlobals::default();
        let card = dereth_assets::motion::GfxObjDegradeInfo {
            id: dereth_primitives::DataId(0x1100_0000),
            degrades: vec![dereth_assets::motion::GfxObjInfo {
                gfxobj_id: dereth_primitives::DataId(1),
                degrade_mode: 2,
                min_dist: 0.0,
                ideal_dist: f32::MAX,
                max_dist: f32::MAX,
            }],
        };
        let mut turned = two_parts();
        for p in &mut turned {
            let (cypt, heading) = part_viewer_distance(p, Vec3::ZERO, viewer, Some(shared));
            select_level(p, Some(&card), false, cypt, heading, &g);
        }
        assert_eq!(turned[0].draw_pos.rotation, turned[1].draw_pos.rotation);
        assert_eq!(turned[0].cypt, turned[1].cypt, "and they sort together");
    }

    /// Behaviour: rendering.degrade.inside-the-share-distance-each-part-measures-itself
    #[test]
    fn inside_the_share_distance_each_part_measures_its_own_distance_and_heading() {
        let origin = Vec3::new(30.0, 0.0, 0.0);
        // 2 m from the origin horizontally, inside a 5 m share distance.
        let viewer = Vec3::new(28.0, 0.0, 0.0);
        assert_eq!(shared_viewer_distance(origin, viewer, 25.0), None);
        let [a, b] = two_parts();
        let own_a = part_viewer_distance(&a, Vec3::ZERO, viewer, None);
        let own_b = part_viewer_distance(&b, Vec3::ZERO, viewer, None);
        assert_eq!(own_a, update_viewer_distance(&a, Vec3::ZERO, viewer));
        assert_ne!(own_a, own_b, "each part is measured where it is");
        // The test is on the horizontal distance only: a viewer 100 m straight above the origin
        // is inside, and a viewer exactly on the share distance is not.
        assert_eq!(
            shared_viewer_distance(origin, Vec3::new(30.0, 0.0, 100.0), 25.0),
            None
        );
        assert!(shared_viewer_distance(origin, Vec3::new(25.0, 0.0, 0.0), 25.0).is_some());
        // At the lowest bias the object distance is zero and every object shares.
        assert!(shared_viewer_distance(origin, origin, 0.0).is_some());
    }

    /// Behaviour: rendering.degrade.past-fifty-units-every-object-in-a-land-cell-takes-the-cells-distance
    #[test]
    fn past_fifty_units_every_object_in_a_land_cell_takes_the_cells_horizontal_distance_and_heading(
    ) {
        // The cell (4, 0) spans x in [96, 120); its centre is (108, 12). The viewer is 30 m up.
        let viewer = Vec3::new(12.0, 12.0, 30.0);
        let (d, heading) =
            cell_viewer_distance(land_cell_centre(Vec3::new(100.0, 3.0, 7.0)), viewer)
                .expect("96 m away horizontally");
        assert_eq!(
            d, 96.0,
            "horizontal, to the cell's centre, not to the object"
        );
        assert_eq!(heading, Vec3::new(1.0, 0.0, 0.0), "a level heading");
        // Two objects in that cell, one of them a particle emitter's, take the same answer and
        // never reach the share test, whose answer for either would be its own distance.
        for origin in [Vec3::new(100.0, 3.0, 7.0), Vec3::new(119.0, 23.0, 0.0)] {
            for share in [25.0, 16.0] {
                assert_eq!(
                    object_viewer_distance(origin, true, viewer, share),
                    Some((d, heading))
                );
                assert_ne!(
                    shared_viewer_distance(origin, viewer, share),
                    Some((d, heading))
                );
            }
        }
        // Exactly fifty keeps the per-object test, and so does an interior cell however far.
        let at_fifty = Vec3::new(62.0, 12.0, 0.0);
        assert_eq!(cell_viewer_distance(at_fifty, viewer), None);
        assert_eq!(
            object_viewer_distance(Vec3::new(100.0, 3.0, 7.0), false, viewer, 25.0),
            shared_viewer_distance(Vec3::new(100.0, 3.0, 7.0), viewer, 25.0)
        );
        // A near cell's object still runs its own test: inside the share distance it measures
        // its parts, past it it hands out its own distance.
        let near = Vec3::new(14.0, 12.0, 30.0);
        assert_eq!(object_viewer_distance(near, true, viewer, 25.0), None);
        let near_past = Vec3::new(30.0, 12.0, 30.0);
        assert_eq!(
            object_viewer_distance(near_past, true, viewer, 25.0),
            shared_viewer_distance(near_past, viewer, 25.0)
        );
        // An unordered distance compares nowhere, so it never hands out the cell's.
        assert_eq!(
            cell_viewer_distance(Vec3::new(f32::NAN, 0.0, 0.0), viewer),
            None
        );
        // The cell is found from coordinates on either side of a landblock corner.
        assert_eq!(
            land_cell_centre(Vec3::new(-0.5, 191.9, 3.0)),
            Vec3::new(-12.0, 180.0, 0.0)
        );
    }
}
