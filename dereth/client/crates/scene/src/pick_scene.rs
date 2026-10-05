//! `impl PickScene for WorldScene` -- the scene side of picking.
//!
//! The sweep and its trait are [`dereth_client_runtime::pick`]; `WorldScene` is this crate's, so
//! its implementation of the runtime's trait lives here.
//!
//! The `#[cfg(gpu)]` gate is on this
//! module's declaration in `lib.rs` -- the same gate `WorldScene` itself carries.

use dereth_client_runtime::pick::{LocalBody, LocalBodyPart, PickScene};
use dereth_primitives::{Frame, ObjectId};

use {
    crate::world_scene::SceneHalves, crate::world_scene::WorldScene,
    crate::world_scene::WorldSceneMut, crate::world_scene::WorldSceneRef,
};

/// One impl text over the three ways the scene is held: whole, and as the shared
/// or mutable view of the application's world state and the presentation's drawing half.
macro_rules! impl_pick_scene {
    ($ty:ty) => {
        impl PickScene for $ty {
            fn viewer(&self) -> Frame {
                self.halves().0.camera.frame()
            }

            fn object_frame(&self, id: ObjectId) -> Option<Frame> {
                self.halves().0.server_object_frame(id)
            }

            fn fov_y_rad(&self, screen: (u32, u32)) -> f32 {
                let (ws, draw) = self.halves();
                draw.view_params(ws, screen.0, screen.1).fov_y_rad
            }

            fn local_body(&self) -> Option<LocalBody> {
                let c = self.halves().0.character.as_ref()?;
                let d = c.driver();
                Some(LocalBody {
                    id: c.object_id(),
                    setup: c.setup_id(),
                    // Setup creation selects placement frame `0x65`; the body's own
                    // placement never comes from the wire (its `0xF745` is consumed by `Character`), and
                    // the live part frames below are what the sweep places by anyway.
                    placement: dereth_client_runtime::models::PLACEMENT_RESTING,
                    parts: d
                        .part_array
                        .parts
                        .iter()
                        .map(|p| LocalBodyPart {
                            pos: p.pos,
                            gfxobj_scale: p.gfxobj_scale,
                            no_draw: p.no_draw(),
                        })
                        .collect(),
                })
            }

            fn drawn_cells(&self) -> Option<std::collections::BTreeSet<u32>> {
                self.halves().1.drawn_cells()
            }

            fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
                self.halves().1.drawn_objects()
            }

            fn object_part_frames(&self, id: ObjectId) -> Option<Vec<Frame>> {
                // `WorldScene`'s own `o.driver.borrow().part_array.parts[i].pos`, written by
                // `driver.update_parts(&o.frame)` in the same statement that sets `o.frame` — so these are
                // in the same viewer-block-relative space as `object_frame` and need no recomposition.
                self.halves().0.server_object_part_frames(id)
            }
        }
    };
}

impl_pick_scene!(WorldScene);
impl_pick_scene!(WorldSceneRef<'_>);
impl_pick_scene!(WorldSceneMut<'_>);
