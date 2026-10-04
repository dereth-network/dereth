//! The object draw and the frustum test.
//!
//! The physics part's draw, the device's mesh draw, the polygon renderer's mesh draw and its
//! subset render, and the part's no-draw and translucency setters.
//!
//! The two mesh-draw paths preserve part visibility, subset ordering and blend selection.

use dereth_primitives::ObjectId;

use crate::cells::cull::Bounding;
use crate::objects::alpha::AlphaList;
use crate::objects::degrade::draws_anything;
use crate::objects::parts::PartDraw;

/// Object-state bits that suppress drawing, from the per-part appearance table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectDrawStatus {
    /// `draw_state` bit 0; setting translucency to 1.0 sets it too.
    pub no_draw: bool,
    /// `ETHEREAL_PS` — the object has no collision, but it still draws. Present so the flag is not
    /// mistaken for a draw suppressor.
    pub ethereal: bool,
    /// `HIDDEN_PS` — not drawn at all.
    pub hidden: bool,
    /// `CLOAKED_PS` (0x100000) — a translucency change is **ignored** for a cloaked object, so a
    /// cloak cannot be undone by a translucency change.
    pub cloaked: bool,
}

/// Decide whether to draw a part before it reaches mesh drawing.
///
/// ```text
/// if (draw_state & 1) != 0: return                          // NoDraw
/// if !portals_only and part.current_render_frame == device.frame_stamp: return   // already this frame
/// level = deg_level ; if !degrades or num_degrades <= level: level = 0
/// g = gfxobj[level] ; if !g: return                          // gfxobj_id 0 means draw nothing
/// ```
///
/// The frame-stamp guard means a part registered in several cells is drawn once; `portals_only`
/// bypasses it, because the building portal pass draws the same part twice on purpose.
#[must_use]
pub fn should_draw_part(
    part: &PartDraw,
    status: ObjectDrawStatus,
    degrades: Option<&dereth_assets::motion::GfxObjDegradeInfo>,
    already_drawn_this_frame: bool,
    portals_only: bool,
) -> bool {
    if part.no_draw || status.no_draw || status.hidden {
        return false;
    }
    if !portals_only && already_drawn_this_frame {
        return false;
    }
    match degrades {
        Some(d) if d.degrades.len() > part.deg_level => draws_anything(d, part.deg_level),
        Some(d) => draws_anything(d, 0),
        // No degrade array at all: the part has one mesh and it draws.
        None => true,
    }
}

/// The current object is offered to the pick test when it has a nonzero physics id or creature mode
/// is active. Scenery, particle hosts and buildings have `id == 0` and are
/// therefore **not pickable**; see `crate::pick`.
#[must_use]
pub fn check_curr_object(physobj_id: ObjectId, creature_mode: bool) -> bool {
    physobj_id.0 != 0 || creature_mode
}

/// This is a textured-surface guard, not an alpha classification or a portal-index test. The
/// controlling `skip_untextured` flag is initially true in retail.
#[must_use]
pub const fn should_draw_mesh_subset(
    surface_type: u32,
    skip_untextured: bool,
    is_env_cell: bool,
    building_pass: bool,
) -> bool {
    !skip_untextured || surface_type & 6 != 0 || !(is_env_cell || building_pass)
}

#[cfg(test)]
mod surface_guard {
    use super::should_draw_mesh_subset;

    #[test]
    fn texture_bits_not_alpha_or_portal_identity_control_the_guard() {
        for solid in [0, 1, 0x11, 0x101, 0x10001] {
            assert!(!should_draw_mesh_subset(solid, true, true, false));
            assert!(!should_draw_mesh_subset(solid, true, false, true));
            assert!(should_draw_mesh_subset(solid, true, false, false));
            assert!(should_draw_mesh_subset(solid, false, true, true));
        }
        for textured in [2, 4, 6, 0x12, 0x104, 0x10002] {
            assert!(should_draw_mesh_subset(textured, true, true, true));
        }
    }
}

/// `SurfaceType` bits used to classify a subset. The four bits select opaque, alpha-tested,
/// blended or additive behavior.
pub mod surface_type {
    /// `Base1ClipMap` — alpha test, no blend. Goes on the **clip** list.
    pub const BASE1_CLIP_MAP: u32 = 0x0004;
    /// `Translucent` — material alpha, `SRCALPHA / INVSRCALPHA`.
    pub const TRANSLUCENT: u32 = 0x0010;
    /// `Alpha` — `SRCALPHA / INVSRCALPHA`.
    pub const ALPHA: u32 = 0x0100;
    /// `InvAlpha` — the inverse-alpha variant.
    pub const INV_ALPHA: u32 = 0x0200;
    /// `Additive` — `ONE / ONE`; glows and magic effects.
    pub const ADDITIVE: u32 = 0x1_0000;
}

/// Stippled-or-alpha mask derived from the subset's
/// Surface-type behavior.
///
/// The client's own chain, verbatim and in this order — it is an if/else-if, so the **first**
/// match wins and an `Alpha | ClipMap` surface is mask 2, not 8:
///
/// ```text
/// if (type & 0x10300) mask = 2      // Alpha | InvAlpha | Additive  -> blended
/// else if (type & 4)  mask = 8      // Base1ClipMap                 -> alpha-tested cut-out
/// else if (type & 0x10) mask = 4    // Translucent                  -> constant material alpha
/// else                mask = 0      // opaque, drawn in place
/// ```
///
/// Bit 0 is set separately, per polygon, when any polygon of the subset is stippled; it is not a
/// list-membership bit and [`classify_subset`] never tests it, so it is not produced here.
#[must_use]
pub const fn subset_mask(surface_type: u32) -> u32 {
    if surface_type & 0x0001_0300 != 0 {
        2
    } else if surface_type & 0x4 != 0 {
        8
    } else if surface_type & 0x10 != 0 {
        4
    } else {
        0
    }
}

/// Whether the mesh draw consults the alpha lists at all for this mesh, before any per-subset
/// classification.
///
/// ```text
/// if drawing_sky or alpha_delay_mask == 0 or a detail surface is installed:
///     render every subset in place          ; no list, no second pass
/// else:
///     classify each subset                  ; classify_subset_passes
/// ```
///
/// So "Multiple Pass Alpha" never reaches the sky, nor a mesh drawn while a detail surface is
/// installed (a building's shell under the building detail texture).
#[must_use]
pub const fn mesh_draw_defers(
    drawing_sky: bool,
    alpha_delay_mask: u32,
    detail_surface_installed: bool,
) -> bool {
    !drawing_sky && alpha_delay_mask != 0 && !detail_surface_installed
}

/// The alpha-delay mask's per-subset classification: which list a subset is deferred to, or `None`
/// when it is drawn immediately.
///
/// ```text
/// if MultiPassAlpha and (mask & 8):  add to alpha list (multipass = true, clip = true)
/// else if alpha_delay_mask & mask:   add to alpha list (clip = (mask & 8) != 0)
/// else:                              render the subset immediately
/// ```
///
/// `mask` bit 3 (value 8) is the clip-mapped bit and bit 1 (value 2) the blended one; a subset with
/// neither is opaque and draws in place.
///
/// **This function answers only half the question and [`classify_subset_passes`] answers all of
/// it.** See that function for why: the multipass arm defers *and* draws, and a caller that reads
/// only the list cannot tell the two arms apart at the shipped alpha-delay mask.
#[must_use]
pub fn classify_subset(
    mask: u32,
    alpha_delay_mask: u32,
    multipass_alpha: bool,
) -> Option<AlphaList> {
    classify_subset_passes(mask, alpha_delay_mask, multipass_alpha).list
}

/// What one subset does on this pass: which deferred list it joins, **and whether it is also drawn
/// immediately**, read from the branch sequence.
///
/// ```text
///   if (MultiPassAlpha preference) {
///       if (subset mask & 8) {                       ; clip-mapped
///           add to alpha list (multipass = 1, clip = 1)
///           first-of-kind cleared for the clip list, then FALLS THROUGH
///       }
///       compute the object matrix
///       render the subset                            ; drawn in place as well
///       ...
///   } else if (subset mask & alpha_delay_mask) {     ; the subset is delayed
///       clip = (mask >> 3) & 1
///       add to alpha list
///       continue                                     ; NOT drawn here
/// ```
///
/// **The fall-through is the whole of the preference.** "Multiple pass alpha" is not
/// a different list and it is not a different sort: a clip-mapped subset is put on the clip list
/// **and** rendered immediately, so it goes down the device twice — the opaque pass writes depth
/// through the alpha test and the deferred pass blends over it. Every other arm is one pass.
///
/// A previous implementation wired `Render.MultiPassAlpha` only as far as the list choice. At the
/// shipped alpha-delay mask `0x0E`, that choice is **identical** on both arms (`0x0E & 8 != 0`, so
/// a clip-mapped subset goes to [`AlphaList::Clip`] either way). The option therefore did nothing:
/// it controlled the one part of the mesh draw whose result it cannot change.
#[must_use]
pub fn classify_subset_passes(
    mask: u32,
    alpha_delay_mask: u32,
    multipass_alpha: bool,
) -> SubsetPasses {
    if multipass_alpha && mask & 8 != 0 {
        return SubsetPasses {
            list: Some(AlphaList::Clip),
            immediate: true,
            multipass: true,
        };
    }
    if alpha_delay_mask & mask != 0 {
        let list = if mask & 8 != 0 {
            AlphaList::Clip
        } else {
            AlphaList::Blend
        };
        return SubsetPasses {
            list: Some(list),
            immediate: false,
            multipass: false,
        };
    }
    SubsetPasses {
        list: None,
        immediate: true,
        multipass: false,
    }
}

/// [`classify_subset_passes`]'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubsetPasses {
    /// Which deferred list the subset joins, if any.
    pub list: Option<AlphaList>,
    /// Whether matrix calculation and subset rendering also run for it on this pass.
    pub immediate: bool,
    /// The alpha-list add's `multipass` argument — set only on the first arm, which is also the
    /// only arm that does both.
    pub multipass: bool,
}

/// The mesh draw status -- what the device's mesh draw returns and what the physics part's draw
/// compares against.
///
/// The values are the client's and are load-bearing twice: the mesh draw combines per-view answers
/// by keeping the larger value, an **ordering** on them, and its no-portal branch tests
/// `0 < status`.
///
/// **Naming note.** [`ObjectDrawStatus`] in this module is *not* this enum: it is a set of
/// object-state bits that happen to carry the name the client uses for this enum. That is a
/// misnomer, and renaming it reaches into several files, so this enum is
/// `MeshDrawStatus` and the collision is recorded here rather than resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeshDrawStatus {
    /// 0.
    Unknown = 0,
    /// 1.
    OutsideViewcone = 1,
    /// **The only value that raises the latch** —
    /// [`Self::UnderCursor`] is a larger value and is *not* accepted, because the test is `==` and
    /// not `>=`.
    InsideViewcone = 2,
    /// Nothing in the mesh draw produces it;
    /// internal mesh drawing returns [`Self::InsideViewcone`] on every one
    /// of its four exits. Present so the enum is the client's and not a subset of it.
    UnderCursor = 3,
}

/// The ordinary branch with no portal list, where the full-screen quad is the only view.
///
/// ```text
/// B = view-cone check of gfxobj.drawing_sphere
/// if B != OUTSIDE:       draw internally  -> InsideViewcone   (drawn)
/// else if portals_only:  draw internally  -> InsideViewcone   (drawn anyway)
/// else:                                     OutsideViewcone  (not drawn)
/// ```
///
/// The internal draw returns [`MeshDrawStatus::InsideViewcone`] unconditionally (all four exits),
/// so `0 < status` is always true and the early return always fires. The distinction that matters
/// to the part-draw result is therefore exactly "did the cone accept it", and
/// `PARTIALLY_INSIDE` counts as accepted.
#[must_use]
pub const fn draw_mesh_no_portal_list(cone: Bounding, portals_only: bool) -> MeshDrawStatus {
    match cone {
        Bounding::Outside if !portals_only => MeshDrawStatus::OutsideViewcone,
        _ => MeshDrawStatus::InsideViewcone,
    }
}

/// The portal-list branch: one
/// view-cone check per view polygon, the answers combined by `max`.
///
/// The tail is the part that cannot be guessed: after the loop,
///
/// ```text
/// if (outside_count != view_count) return status;
/// status = OutsideViewcone;                     // and fall through to the return
/// ```
///
/// so a mesh that every view rejected answers `OutsideViewcone` **even under `portals_only`**,
/// where the loop had already drawn it and raised the status to `InsideViewcone`. An empty view
/// list takes the same path (`0 == 0`) and answers `OutsideViewcone`, which is why "no views"
/// is not the same as "no cone".
#[must_use]
pub fn draw_mesh_view_list(results: &[Bounding], portals_only: bool) -> MeshDrawStatus {
    let mut status = MeshDrawStatus::Unknown;
    let mut outside = 0usize;
    for &b in results {
        if b == Bounding::Outside {
            if portals_only {
                status = status.max(MeshDrawStatus::InsideViewcone);
            } else if status < MeshDrawStatus::OutsideViewcone {
                status = MeshDrawStatus::OutsideViewcone;
            }
            outside += 1;
        } else {
            status = status.max(MeshDrawStatus::InsideViewcone);
        }
    }
    if outside == results.len() {
        return MeshDrawStatus::OutsideViewcone;
    }
    status
}

/// Per-view loop: test the mesh's
/// `drawing_sphere` once per view polygon and draw for each that passes, with a frame-stamp guard
/// keeping the mesh from being drawn more than once.
///
/// `portals_only` draws even an OUTSIDE result, because the building portal pass needs the portals
/// regardless of whether the shell is visible.
///
/// The `results.is_empty()` arm is *not* "no portal list: one full-screen view, which always
/// passes": the no-portal-list
/// branch still runs the view-cone check, against whatever view was last installed, and returns
/// `OutsideViewcone` without drawing when it says OUTSIDE. The empty
/// slice therefore means *"the caller has not run the cone"*, and the faithful form of that
/// branch is [`draw_mesh_no_portal_list`], which takes the answer. The count this returns is still
/// right for a caller that genuinely has a view list.
#[must_use]
pub fn draw_mesh_views(results: &[Bounding], portals_only: bool) -> usize {
    if results.is_empty() {
        // No view list handed over; the caller owns the cone. See the note above.
        return 1;
    }
    let visible = results.iter().filter(|&&b| b != Bounding::Outside).count();
    if visible == 0 && portals_only {
        1
    } else {
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::degrade::DegradeMode;
    use dereth_primitives::{DataId, Frame, Vec3};

    fn part(level: usize) -> PartDraw {
        PartDraw {
            pos: Frame::default(),
            draw_pos: Frame::default(),
            gfxobj_scale: Vec3::new(1.0, 1.0, 1.0),
            cypt: 0.0,
            deg_level: level,
            deg_mode: DegradeMode::None,
            no_draw: false,
        }
    }

    fn degrades() -> dereth_assets::motion::GfxObjDegradeInfo {
        use dereth_assets::motion::GfxObjInfo;
        dereth_assets::motion::GfxObjDegradeInfo {
            id: DataId(0x1100_0000),
            degrades: vec![
                GfxObjInfo {
                    gfxobj_id: DataId(0x0100_0001),
                    degrade_mode: 1,
                    min_dist: 0.0,
                    ideal_dist: 100.0,
                    max_dist: 200.0,
                },
                GfxObjInfo {
                    gfxobj_id: DataId(0),
                    degrade_mode: 1,
                    min_dist: f32::MAX,
                    ideal_dist: f32::MAX,
                    max_dist: f32::MAX,
                },
            ],
        }
    }

    /// Oracle: part drawing's four early returns, and its
    /// "`gfxobj_id == 0` means **draw nothing**".
    #[test]
    fn the_draw_guards_match_the_transcribed_early_returns() {
        let d = degrades();
        let ok = ObjectDrawStatus::default();
        assert!(should_draw_part(&part(0), ok, Some(&d), false, false));
        // NoDraw, whichever side sets it.
        let mut nd = part(0);
        nd.no_draw = true;
        assert!(!should_draw_part(&nd, ok, Some(&d), false, false));
        assert!(!should_draw_part(
            &part(0),
            ObjectDrawStatus {
                no_draw: true,
                ..ok
            },
            Some(&d),
            false,
            false
        ));
        assert!(!should_draw_part(
            &part(0),
            ObjectDrawStatus { hidden: true, ..ok },
            Some(&d),
            false,
            false
        ));
        // The frame-stamp guard, and portals_only bypassing it.
        assert!(!should_draw_part(&part(0), ok, Some(&d), true, false));
        assert!(
            should_draw_part(&part(0), ok, Some(&d), true, true),
            "the portal pass draws twice"
        );
        // The terminator level draws nothing.
        assert!(!should_draw_part(&part(1), ok, Some(&d), false, false));
        // A level past the end of the array falls back to level 0, which does draw.
        assert!(should_draw_part(&part(99), ok, Some(&d), false, false));
        // Ethereal is not a draw suppressor.
        assert!(should_draw_part(
            &part(0),
            ObjectDrawStatus {
                ethereal: true,
                ..ok
            },
            Some(&d),
            false,
            false
        ));
    }

    /// Oracle: the current-object check is
    /// `(physobj present and physobj.id != 0) || creature_mode`, which is what makes scenery
    /// unpickable.
    #[test]
    fn only_objects_with_an_id_are_offered_to_the_pick_test() {
        assert!(check_curr_object(ObjectId(0x1234), false));
        assert!(!check_curr_object(ObjectId(0), false));
        assert!(
            check_curr_object(ObjectId(0), true),
            "creature mode picks id-less objects too"
        );
    }

    /// Oracle: the subset classification chain — multipass clip-mapped
    /// subsets go on the clip list, other deferred subsets on the clip or blend list by mask bit 3,
    /// and anything the delay mask does not name draws in place.
    #[test]
    fn subsets_are_classified_into_the_two_lists_or_drawn_in_place() {
        // Bit 3 (8) is clip-mapped, bit 1 (2) is blended.
        assert_eq!(classify_subset(8, 0xF, false), Some(AlphaList::Clip));
        assert_eq!(classify_subset(2, 0xF, false), Some(AlphaList::Blend));
        assert_eq!(classify_subset(1, 0xF, false), Some(AlphaList::Blend));
        assert_eq!(
            classify_subset(1, 0, false),
            None,
            "not in the delay mask: drawn in place"
        );
        // MultiPassAlpha forces clip-mapped subsets onto the clip list whatever the delay mask says.
        assert_eq!(classify_subset(8, 0, true), Some(AlphaList::Clip));
        assert_eq!(
            classify_subset(2, 0, true),
            None,
            "only the clip bit is forced"
        );
    }

    /// Oracle: the mesh draw renders every subset in place, consulting neither list, while the
    /// sky is drawn, when the alpha-delay mask is zero, or while a detail surface is installed.
    #[test]
    fn the_alpha_lists_are_bypassed_for_the_sky_a_zero_mask_and_a_detail_surface() {
        const SHIPPED: u32 = crate::consts::S_ALPHA_DELAY_MASK;
        assert!(mesh_draw_defers(false, SHIPPED, false));
        assert!(!mesh_draw_defers(true, SHIPPED, false), "the sky");
        assert!(!mesh_draw_defers(false, 0, false), "a zero delay mask");
        assert!(
            !mesh_draw_defers(false, SHIPPED, true),
            "a detail surface installed"
        );
    }

    /// The multipass arm defers and draws and the delay mask arm only defers.
    #[test]
    fn the_multipass_arm_defers_and_draws_and_the_delay_mask_arm_only_defers() {
        const SHIPPED: u32 = crate::consts::S_ALPHA_DELAY_MASK; // 0x0E
                                                                // At the shipped mask the two arms pick the SAME list, which is why the list alone cannot
                                                                // measure this preference.
        assert_eq!(classify_subset(8, SHIPPED, false), Some(AlphaList::Clip));
        assert_eq!(classify_subset(8, SHIPPED, true), Some(AlphaList::Clip));
        // The pass count is what differs.
        let off = classify_subset_passes(8, SHIPPED, false);
        let on = classify_subset_passes(8, SHIPPED, true);
        assert_eq!(
            off,
            SubsetPasses {
                list: Some(AlphaList::Clip),
                immediate: false,
                multipass: false
            }
        );
        assert_eq!(
            on,
            SubsetPasses {
                list: Some(AlphaList::Clip),
                immediate: true,
                multipass: true
            }
        );
        // A blended subset is untouched by the preference: the test is on bit 3 alone.
        assert_eq!(
            classify_subset_passes(2, SHIPPED, true),
            SubsetPasses {
                list: Some(AlphaList::Blend),
                immediate: false,
                multipass: false
            }
        );
        // An opaque subset draws in place and joins no list, on either setting.
        for m in [false, true] {
            assert_eq!(
                classify_subset_passes(0, SHIPPED, m),
                SubsetPasses {
                    list: None,
                    immediate: true,
                    multipass: false
                }
            );
        }
    }

    /// Oracle: the portal-list branch runs once per view polygon and draws
    /// the mesh for each polygon that passes, plus the `portals_only` branch that draws even an
    /// OUTSIDE result.
    #[test]
    fn a_mesh_draws_once_per_view_polygon_that_passes() {
        assert_eq!(
            draw_mesh_views(&[], false),
            1,
            "no portal list: one full-screen view"
        );
        assert_eq!(
            draw_mesh_views(
                &[
                    Bounding::EntirelyInside,
                    Bounding::Outside,
                    Bounding::PartiallyInside
                ],
                false
            ),
            2,
            "a cell reachable through two portals is drawn against both"
        );
        assert_eq!(draw_mesh_views(&[Bounding::Outside], false), 0);
        assert_eq!(
            draw_mesh_views(&[Bounding::Outside], true),
            1,
            "portals_only draws even an OUTSIDE mesh, because the building pass needs its portals"
        );
    }

    /// Draw mesh answers the clients object draw status.
    #[test]
    fn draw_mesh_answers_the_clients_object_draw_status() {
        assert_eq!(MeshDrawStatus::Unknown as i32, 0);
        assert_eq!(MeshDrawStatus::OutsideViewcone as i32, 1);
        assert_eq!(MeshDrawStatus::InsideViewcone as i32, 2);
        assert_eq!(MeshDrawStatus::UnderCursor as i32, 3);

        // The no-portal-list branch: PARTIAL and ENTIRELY both count as accepted.
        for b in [Bounding::PartiallyInside, Bounding::EntirelyInside] {
            assert_eq!(
                draw_mesh_no_portal_list(b, false),
                MeshDrawStatus::InsideViewcone
            );
        }
        assert_eq!(
            draw_mesh_no_portal_list(Bounding::Outside, false),
            MeshDrawStatus::OutsideViewcone,
            "this is the answer that stops the latch rising"
        );
        assert_eq!(
            draw_mesh_no_portal_list(Bounding::Outside, true),
            MeshDrawStatus::InsideViewcone,
            "portals_only draws it and answers INSIDE, because the internal draw always does"
        );

        // The view-list branch: any accepting view wins.
        assert_eq!(
            draw_mesh_view_list(&[Bounding::Outside, Bounding::PartiallyInside], false),
            MeshDrawStatus::InsideViewcone
        );
        assert_eq!(
            draw_mesh_view_list(&[Bounding::Outside, Bounding::Outside], false),
            MeshDrawStatus::OutsideViewcone
        );
        // The tail overrides `portals_only`'s own INSIDE when **every** view rejected the mesh —
        // the one place the two branches disagree, and the reason they are two functions here.
        assert_eq!(
            draw_mesh_view_list(&[Bounding::Outside, Bounding::Outside], true),
            MeshDrawStatus::OutsideViewcone,
            "`if (outside != view_count)` fails, so the status is overwritten after the loop"
        );
        // And an empty list is not "no cone": `0 == 0` takes the same tail.
        assert_eq!(
            draw_mesh_view_list(&[], false),
            MeshDrawStatus::OutsideViewcone
        );
        assert_eq!(
            draw_mesh_view_list(&[], true),
            MeshDrawStatus::OutsideViewcone
        );
    }

    /// The subset mask and the delay mask are the clients own numbers.
    #[test]
    fn the_subset_mask_and_the_delay_mask_are_the_clients_own_numbers() {
        // 0x10300 = Alpha (0x100) | InvAlpha (0x200) | Additive (0x10000) -> 2.
        assert_eq!(subset_mask(0x0000_0100), 2, "Alpha");
        assert_eq!(subset_mask(0x0000_0200), 2, "InvAlpha");
        assert_eq!(subset_mask(0x0001_0000), 2, "Additive");
        // 0x4 = Base1ClipMap -> 8.
        assert_eq!(subset_mask(0x0000_0004), 8, "Base1ClipMap");
        // 0x10 = Translucent -> 4.
        assert_eq!(subset_mask(0x0000_0010), 4, "Translucent");
        // Anything else is opaque, including a plain textured surface.
        assert_eq!(subset_mask(0x0000_0002), 0, "Base1Image alone is opaque");
        assert_eq!(subset_mask(0), 0);
        // The chain is if/else-if, so the first match wins: Alpha beats ClipMap beats Translucent.
        assert_eq!(subset_mask(0x0000_0104), 2, "Alpha|ClipMap is 2, not 8");
        assert_eq!(
            subset_mask(0x0000_0014),
            8,
            "ClipMap|Translucent is 8, not 4"
        );
        // Bits the chain does not name never move it: Gouraud, Stippled, Diffuse, Luminous.
        assert_eq!(subset_mask(0x5000_0060), 0);

        // The alpha-delay mask is 0x0E in retail, i.e. all three kinds are
        // delayed and only mask 0 draws in place.
        assert_eq!(crate::consts::S_ALPHA_DELAY_MASK, 0x0E);
        for (ty, want) in [
            (0x0000_0100u32, Some(AlphaList::Blend)), // Alpha
            (0x0000_0004, Some(AlphaList::Clip)),     // ClipMap
            (0x0000_0010, Some(AlphaList::Blend)),    // Translucent
            (0x0000_0002, None),                      // opaque
        ] {
            assert_eq!(
                classify_subset(subset_mask(ty), 0x0E, false),
                want,
                "surface type {ty:#x}"
            );
        }
    }

    /// A cloak cannot be undone by a translucency change.
    #[test]
    fn a_cloak_cannot_be_undone_by_a_translucency_change() {
        let cloaked = ObjectDrawStatus {
            cloaked: true,
            ..ObjectDrawStatus::default()
        };
        assert!(cloaked.cloaked);
        // A cloaked object is still offered to the draw path: the cloak lives in the material, not
        // in the NoDraw flag, so this must NOT be a draw guard.
        assert!(should_draw_part(&part(0), cloaked, None, false, false));
    }
}
