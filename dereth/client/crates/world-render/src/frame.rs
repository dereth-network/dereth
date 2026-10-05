//! The per-frame draw-pass sequence.
//!
//! The smart box's draw and its normal-mode arm, the landscape draw, the block draw and the
//! cell draw.
//!
//! The pass sequence below preserves frame and portal traversal ordering.
//!
//! ```text
//! Outdoors: clear -> sky pass 0 -> landblocks far->near -> sky pass 1 -> alpha list
//! Indoors:  outdoors-through-portals -> Z clear -> portal depth stamps -> env cells far->near
//!           -> alpha list
//! ```
//!
//! The frame's pass sequence is a statement about **order**, so this module produces the order as data and the
//! tests assert on it. A caller drives a `RenderBackend` from it; the order is the observable.

use dereth_primitives::RenderBackend;

use crate::cells::portal_view::{indoor_steps, ConstructedView, IndoorStep};

/// One step of an outdoor frame, in the client's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutdoorStep {
    /// Begin the frame by clearing with flags 7, color `(0, 0, 0, 1)`, and depth `1.0`.
    Clear,
    /// Draw the sky dome in pass 0, before any terrain.
    SkyPass0,
    /// Reset `in_view` on every block and cell, then test each
    /// against every view polygon.
    CheckBlocks,
    /// One landblock, by its index into `block_draw_list`. Emitted **outermost ring first, viewer
    /// block last**, because the landscape draw walks the list backwards.
    Block(u32),
    /// Draw the weather layer in pass 1, after the terrain, only when landscape weather rendering
    /// is enabled.
    SkyPass1,
    /// Flush the alpha lists with a threshold of 0.0 — everything.
    FlushAlphaList,
}

/// One step within a block, in the client's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStep {
    /// Pass A: the per-cell object update plus the shadow-part sort, for one cell.
    UpdateCell(u16),
    /// Pass B: the cell's terrain triangles.
    LandCell(u16),
    /// Pass B: the cell's building and objects.
    SortCell(u16),
    /// Flush the alpha lists with a threshold of 0.75 after each sort cell, because ` = 0.75 < 1`.
    FlushPartial,
}

/// The landscape draw's step sequence.
///
/// `blocks` is [`crate::land::order::block_draw_order`]'s output — already outermost-first — and
/// `visible` says which of them survived the landscape draw's block visibility check.
#[must_use]
pub fn outdoor_steps(
    blocks: &[u32],
    visible: &dyn Fn(u32) -> bool,
    weather_enabled: bool,
) -> Vec<OutdoorStep> {
    let mut s = vec![
        OutdoorStep::Clear,
        OutdoorStep::SkyPass0,
        OutdoorStep::CheckBlocks,
    ];
    s.extend(
        blocks
            .iter()
            .copied()
            .filter(|&b| visible(b))
            .map(OutdoorStep::Block),
    );
    if weather_enabled {
        s.push(OutdoorStep::SkyPass1);
    }
    s.push(OutdoorStep::FlushAlphaList);
    s
}

/// Two passes over one block's `draw_array`.
///
/// Pass A runs over every cell that is in view **and** has shadow objects; pass B runs over every
/// cell, drawing terrain only when the cell is in view but drawing the sort cell regardless,
/// because ` = 1`. That is why tall objects on culled cells still appear.
#[must_use]
pub fn block_steps(
    draw_array: &[u16],
    in_view: &dyn Fn(u16) -> bool,
    has_shadow_objects: &dyn Fn(u16) -> bool,
) -> Vec<BlockStep> {
    let mut s = Vec::new();
    for &c in draw_array {
        if in_view(c) && has_shadow_objects(c) {
            s.push(BlockStep::UpdateCell(c));
        }
    }
    for &c in draw_array {
        if in_view(c) {
            s.push(BlockStep::LandCell(c));
        }
        if dereth_terrain::consts::ALWAYS_DRAW_SORT_CELL || in_view(c) {
            s.push(BlockStep::SortCell(c));
            if dereth_terrain::consts::ALPHA_FLUSH_MIN_Z < 1.0 {
                s.push(BlockStep::FlushPartial);
            }
        }
    }
    s
}

/// Whether the frame takes the outdoor or the indoor path.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameSteps {
    Outdoor(Vec<OutdoorStep>),
    Indoor(Vec<IndoorStep>),
}

/// The whole frame's pass sequence. Indoors the outdoor path still runs — clipped to the portal
/// polygons — as `IndoorStep::OutdoorsThroughPortals`.
#[must_use]
pub fn frame_steps(
    indoors: bool,
    view: &ConstructedView,
    blocks: &[u32],
    visible: &dyn Fn(u32) -> bool,
    weather_enabled: bool,
) -> FrameSteps {
    if indoors {
        FrameSteps::Indoor(indoor_steps(view))
    } else {
        FrameSteps::Outdoor(outdoor_steps(blocks, visible, weather_enabled))
    }
}

/// The `RenderBackend` seam is deliberately narrow: this crate decides *what* and *in what order*, and
/// hands each decision over as a `DrawBatch`. This is the shape a caller drives.
///
/// It exists so the crate has one place where "the frame" is spelled out; the ordering itself is
/// [`frame_steps`], which is what the tests assert on.
pub trait FrameSink {
    /// Called once per step, in order.
    fn step(&mut self, step: &OutdoorStep, r: &mut dyn RenderBackend);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::portal_view::ConstructedView;

    /// Oracle: the exact draw order — outdoors the frame is clear → sky pass 0 → landblocks
    /// far→near → sky pass 1 → alpha list.
    /// The two sky passes bracket the landblocks, and the alpha list is last.
    #[test]
    fn the_outdoor_frame_brackets_the_landblocks_with_the_two_sky_passes() {
        let blocks = dereth_terrain::land::order::block_draw_order(5);
        let s = outdoor_steps(&blocks, &|_| true, true);
        assert_eq!(s[0], OutdoorStep::Clear);
        assert_eq!(s[1], OutdoorStep::SkyPass0);
        assert_eq!(s[2], OutdoorStep::CheckBlocks);
        let first_block = s
            .iter()
            .position(|x| matches!(x, OutdoorStep::Block(_)))
            .expect("blocks");
        let last_block = s
            .iter()
            .rposition(|x| matches!(x, OutdoorStep::Block(_)))
            .expect("blocks");
        let sky1 = s
            .iter()
            .position(|x| *x == OutdoorStep::SkyPass1)
            .expect("sky pass 1");
        assert!(first_block > 2, "sky pass 0 comes before every block");
        assert!(sky1 > last_block, "sky pass 1 comes after every block");
        assert_eq!(*s.last().expect("non-empty"), OutdoorStep::FlushAlphaList);
        // And the blocks are in far-to-near order, which block_draw_order already guarantees.
        let emitted: Vec<u32> = s
            .iter()
            .filter_map(|x| match x {
                OutdoorStep::Block(b) => Some(*b),
                _ => None,
            })
            .collect();
        assert_eq!(emitted, blocks);
        // The viewer's own block is last: index mid_radius*mid_width + mid_radius = 2*5+2 = 12.
        assert_eq!(*emitted.last().expect("non-empty"), 2 * 5 + 2);
    }

    /// Oracle: when weather is enabled, draw sky pass 1. With
    /// weather off there is no second pass at all.
    #[test]
    fn the_weather_pass_is_conditional() {
        let blocks = dereth_terrain::land::order::block_draw_order(3);
        assert!(outdoor_steps(&blocks, &|_| true, true).contains(&OutdoorStep::SkyPass1));
        assert!(!outdoor_steps(&blocks, &|_| true, false).contains(&OutdoorStep::SkyPass1));
    }

    /// Oracle: pass A runs over cells with shadow objects,
    /// then pass B over all of them, and "Because sort cells are drawn even when the terrain cell is
    /// culled (` = 1`), tall objects on culled cells still appear".
    #[test]
    fn a_culled_cell_still_draws_its_objects() {
        let draw_array = dereth_terrain::land::order::cell_draw_order(
            8,
            dereth_terrain::land::mesh::Direction::InViewerBlock,
            (0, 0),
        );
        // Cull exactly one cell.
        let culled = draw_array[10];
        let s = block_steps(&draw_array, &|c| c != culled, &|_| true);
        assert!(
            !s.contains(&BlockStep::LandCell(culled)),
            "the culled cell draws no terrain"
        );
        assert!(
            s.contains(&BlockStep::SortCell(culled)),
            "but it still draws its objects: is 1"
        );
        assert!(
            !s.contains(&BlockStep::UpdateCell(culled)),
            "and pass A skips it"
        );
    }

    /// Oracle: block draw's two passes — pass A runs over the **whole** `draw_array` before pass B
    /// starts, because the shadow-part sort of a later cell must not run between two earlier draws.
    #[test]
    fn pass_a_completes_before_pass_b_begins() {
        let draw_array = dereth_terrain::land::order::cell_draw_order(
            4,
            dereth_terrain::land::mesh::Direction::North,
            (0, 0),
        );
        let s = block_steps(&draw_array, &|_| true, &|_| true);
        let last_a = s
            .iter()
            .rposition(|x| matches!(x, BlockStep::UpdateCell(_)))
            .expect("pass A ran");
        let first_b = s
            .iter()
            .position(|x| matches!(x, BlockStep::LandCell(_)))
            .expect("pass B ran");
        assert!(last_a < first_b);
        // Both passes visit the cells in the same far-to-near order.
        let a: Vec<u16> = s
            .iter()
            .filter_map(|x| match x {
                BlockStep::UpdateCell(c) => Some(*c),
                _ => None,
            })
            .collect();
        let b: Vec<u16> = s
            .iter()
            .filter_map(|x| match x {
                BlockStep::LandCell(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(a, draw_array);
        assert_eq!(b, draw_array);
    }

    /// Oracle: the block draw — "if  (0.75) < 1: flush the alpha list at 0.75", per sort cell.
    /// The partial flush is a shipped behaviour, and it is what bounds the alpha list's growth
    /// across a large window.
    #[test]
    fn each_sort_cell_is_followed_by_a_partial_alpha_flush() {
        let draw_array = vec![0u16, 1, 2];
        let s = block_steps(&draw_array, &|_| true, &|_| false);
        let pairs: Vec<&BlockStep> = s.iter().collect();
        for w in pairs.windows(2) {
            if matches!(w[0], BlockStep::SortCell(_)) {
                assert_eq!(*w[1], BlockStep::FlushPartial);
            }
        }
        assert_eq!(
            s.iter().filter(|x| **x == BlockStep::FlushPartial).count(),
            3
        );
        assert_eq!(dereth_terrain::consts::ALPHA_FLUSH_MIN_Z, 0.75);
    }

    /// Oracle: indoor rendering — the frame takes the indoor path, and it still runs the
    /// outdoor path first, clipped to the portal polygons.
    #[test]
    fn the_indoor_path_is_selected_and_still_runs_the_outdoor_pass() {
        let view = ConstructedView {
            cell_draw_list: vec![dereth_primitives::CellId(0x0001_0100)],
            outside_view_count: 1,
            ..ConstructedView::default()
        };
        let steps = frame_steps(true, &view, &[], &|_| true, true);
        let FrameSteps::Indoor(s) = steps else {
            panic!("expected the indoor path")
        };
        assert_eq!(s[0], IndoorStep::OutdoorsThroughPortals);
        assert_eq!(*s.last().expect("non-empty"), IndoorStep::FlushAlphaList);
        // Outdoors the same call gives the outdoor sequence.
        let steps = frame_steps(false, &view, &[0], &|_| true, false);
        assert!(matches!(steps, FrameSteps::Outdoor(_)));
    }
}
