//! Buildings: the draw sequence and the portal pass.
//!
//! A landblock's per-cell registration (`SortCells`, the building each land cell owns, and the
//! static-object lists) is the landscape's own content in `dereth-terrain`.

use crate::cells::portal_view::{PortalMode, PortalPoly};

/// Building drawing has this three-pass structure:
///
/// ```text
/// update viewer distance for the building's first part
/// flush alpha at 0.0                  // everything queued before the building
/// draw first part in portal-only mode // pass 1: the interior
/// draw first part normally            // pass 2: the shell mesh
/// ```
///
/// The full alpha flush before the building is load-bearing: the interior seen through the
/// building's portals must not be interleaved with translucent surfaces queued outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingStep {
    UpdateViewerDistance,
    FlushAlphaList,
    /// Draw part zero in portal-only mode, which draws the interior cells.
    PortalPass,
    /// Draw part zero in normal mode to render the shell.
    ShellPass,
}

/// The building draw sequence, as data.
#[must_use]
pub fn building_steps() -> [BuildingStep; 4] {
    [
        BuildingStep::UpdateViewerDistance,
        BuildingStep::FlushAlphaList,
        BuildingStep::PortalPass,
        BuildingStep::ShellPass,
    ]
}

/// [`BuildingStep::PortalPass`] expanded into the work it actually is.
///
/// Drawing part zero in portal-only mode takes the building branch,
/// which traverses the shell mesh's drawing BSP **twice**:
///
/// ```text
/// building_view = current portal view // pin the traversal to one view polygon
/// traverse drawing_bsp in mode 1      // stamp every visible opening's depth
/// traverse drawing_bsp in mode 2      // build each interior's view and draw it
/// building_view = saved
/// ```
///
/// Both walks visit the same nodes in the same order for a fixed viewpoint, so this yields the
/// portal polygons once with [`PortalMode::StampDepth`] and then again with
/// [`PortalMode::BuildView`]. Note the pass boundary is real and not an ordering detail: **every**
/// opening's depth is reset before **any** interior is drawn, so two portals of one building
/// cannot stamp over each other's interior.
#[must_use]
pub fn portal_pass(order: &[PortalPoly]) -> Vec<(PortalMode, PortalPoly)> {
    let mut out = Vec::with_capacity(order.len() * 2);
    for mode in [PortalMode::StampDepth, PortalMode::BuildView] {
        out.extend(order.iter().map(|&p| (mode, p)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the full `FlushAlphaList(0.0)` comes
    /// **before** the building's two passes, and the portal pass comes before the shell.
    #[test]
    fn a_building_flushes_the_alpha_list_before_drawing_its_interior() {
        let s = building_steps();
        assert_eq!(
            s,
            [
                BuildingStep::UpdateViewerDistance,
                BuildingStep::FlushAlphaList,
                BuildingStep::PortalPass,
                BuildingStep::ShellPass,
            ]
        );
        let flush = s
            .iter()
            .position(|x| *x == BuildingStep::FlushAlphaList)
            .expect("a flush");
        let portal = s
            .iter()
            .position(|x| *x == BuildingStep::PortalPass)
            .expect("a portal pass");
        let shell = s
            .iter()
            .position(|x| *x == BuildingStep::ShellPass)
            .expect("a shell pass");
        assert!(flush < portal, "everything queued outside is flushed first");
        assert!(
            portal < shell,
            "the interior is drawn before the shell that occludes it"
        );
    }

    /// The portal pass stamps every opening before drawing any interior.
    #[test]
    fn the_portal_pass_stamps_every_opening_before_drawing_any_interior() {
        let order = [
            PortalPoly {
                polygon: 4,
                portal_index: 0,
            },
            PortalPoly {
                polygon: 7,
                portal_index: 1,
            },
        ];
        let pass = portal_pass(&order);
        assert_eq!(
            pass,
            vec![
                (PortalMode::StampDepth, order[0]),
                (PortalMode::StampDepth, order[1]),
                (PortalMode::BuildView, order[0]),
                (PortalMode::BuildView, order[1]),
            ]
        );
        let last_stamp = pass
            .iter()
            .rposition(|(m, _)| *m == PortalMode::StampDepth)
            .expect("a stamp");
        let first_view = pass
            .iter()
            .position(|(m, _)| *m == PortalMode::BuildView)
            .expect("an interior");
        assert!(last_stamp < first_view);
        assert!(
            portal_pass(&[]).is_empty(),
            "a building with no portals does no portal work"
        );
    }
}
