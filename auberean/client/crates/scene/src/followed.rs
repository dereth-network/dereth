//! The objects the camera may follow, hang from or look at, as the scene has them this frame: the
//! player, the camera's two stand-ins hung from the player, and every other body, each where it is
//! drawn, in the frame of the player's landblock.
//!
//! The stand-ins are made with the player when its setup has their holding locations (one the
//! camera hangs from, one it looks at): the player's animations move them, and with them the
//! camera, while a behavior asks the camera to ride them.

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_client_sdk::model::ids;
use auberean_physics::body::parent::{ChildInfo, ChildKind};
use auberean_physics::body::Body;
use auberean_world::math::Mat4;
use dereth_primitives::{Frame, Position, Quat, Vec3};

use crate::camera::{Facts, Objects, Viewer};
use crate::others::Others;
use crate::{figures, held};

/// The holding locations of the player the camera's stand-ins hang from: the one it hangs from,
/// then the one it looks at.
pub(crate) const STAND_IN_LOCATIONS: [u32; 2] = [0x4000_0004, 0x4000_0005];

/// The player as the camera reads it.
pub(crate) struct Player<'a> {
    pub id: u64,
    pub body: &'a Body,
    pub figure: &'a Figure,
    /// Its height (its setup's times its height scale).
    pub height: f32,
    /// Where it is drawn this frame.
    pub drawn: Position,
}

/// The objects the camera reads this frame.
pub(crate) struct Followed<'a, 'w> {
    pub player: Option<Player<'a>>,
    pub others: &'a Others,
    pub anims: &'a CachedRecords<'w>,
    pub alpha: f32,
    /// Which of the camera's stand-ins there are: the one it hangs from, the one it looks at.
    pub stand_ins: [bool; 2],
}

/// A place as the camera reads it: the point, and the frame's forward axis.
fn viewer_of(p: &Position, base: Vec3) -> Viewer {
    let m = auberean_physics::geom::rotation_axes(p.frame.rotation);
    Viewer {
        origin: Vec3::new(
            p.frame.origin.x + base.x,
            p.frame.origin.y + base.y,
            p.frame.origin.z + base.z,
        ),
        look: Vec3::new(m[3], m[4], m[5]),
        rotation: None,
    }
}

impl Followed<'_, '_> {
    /// From the frame of `cell`'s landblock to the player's.
    fn base(&self, cell: u32) -> Vec3 {
        let Some(p) = &self.player else {
            return Vec3::ZERO;
        };
        let to = |c: u32| figures::global(c, auberean_primitives::Vec3::ZERO);
        let (a, b) = (to(cell), to(p.drawn.cell.0));
        Vec3::new(a.x - b.x, a.y - b.y, 0.0)
    }

    /// Where `body` (with `figure`) stands as drawn, or at its bone of kind `bone`: where it
    /// stands by physics when it has no such bone.
    fn body_place(&self, body: &Body, figure: Option<&Figure>, bone: u32) -> Position {
        let (drawn, _) = body.drawn(self.alpha);
        if bone == 0 {
            return drawn;
        }
        figure
            .and_then(|f| {
                held::bone_position(
                    f,
                    self.anims,
                    &drawn,
                    body.total_scale(None),
                    bone,
                    &Mat4::IDENTITY,
                )
            })
            .unwrap_or(body.position)
    }

    /// The stand-in `which` (0 hung from, 1 looked at) where it hangs from the player as drawn.
    fn stand_in(&self, which: usize) -> Option<Position> {
        let p = self.player.as_ref()?;
        if !self.stand_ins[which] {
            return None;
        }
        let info = ChildInfo {
            id: 0,
            kind: ChildKind::HoldingLocation,
            location: STAND_IN_LOCATIONS[which],
            offset: Frame::new(Vec3::ZERO, Quat::IDENTITY),
            remove_rotation: false,
        };
        let m = held::hanging(p.figure, self.anims, &info);
        let whole = p.body.total_scale(None);
        Some(Body::child_place(&p.drawn, whole, &info, m.as_ref()))
    }

    fn which_stand_in(id: u64) -> Option<usize> {
        if id == ids::camera_parent() {
            Some(0)
        } else if id == ids::camera_target() {
            Some(1)
        } else {
            None
        }
    }
}

impl Objects for Followed<'_, '_> {
    fn place(&self, id: u64, bone: u32) -> Option<Viewer> {
        if let Some(p) = self.player.as_ref().filter(|p| p.id == id) {
            let at = self.body_place(p.body, Some(p.figure), bone);
            return Some(viewer_of(&at, self.base(at.cell.0)));
        }
        if let Some(w) = Self::which_stand_in(id) {
            let at = self.stand_in(w)?;
            return Some(viewer_of(&at, self.base(at.cell.0)));
        }
        let (body, figure) = match self.others.holder(id) {
            Some((b, f, _)) => (b, Some(f)),
            None => (self.others.body(id)?, None),
        };
        let at = self.body_place(body, figure, bone);
        Some(viewer_of(&at, self.base(at.cell.0)))
    }

    fn facts(&self, id: u64) -> Option<Facts> {
        if let Some(p) = self.player.as_ref().filter(|p| p.id == id) {
            return Some(Facts {
                is_player: true,
                is_dummy: false,
                no_draw: p.body.is_no_draw(),
                height: p.height,
                scale_z: p.body.total_scale(None).z,
                local_velocity: p.body.local_velocity(),
                element: p.body.element(),
            });
        }
        if let Some(w) = Self::which_stand_in(id) {
            let p = self.player.as_ref().filter(|_| self.stand_ins[w])?;
            let whole = p.body.total_scale(None);
            return Some(Facts {
                is_dummy: true,
                scale_z: whole.z * p.body.child_scale,
                ..Facts::default()
            });
        }
        let body = match self.others.holder(id) {
            Some((b, _, _)) => b,
            None => self.others.body(id)?,
        };
        Some(Facts {
            is_player: false,
            is_dummy: false,
            no_draw: body.is_no_draw(),
            height: body.height(),
            scale_z: body.total_scale(None).z,
            local_velocity: body.local_velocity(),
            element: body.element(),
        })
    }
}
