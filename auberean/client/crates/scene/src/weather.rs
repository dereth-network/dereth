//! The weather the camera can see: how hard it rains and which way the wind blows at the moment,
//! the rain's streaks, its three particle systems standing at the viewer's block, and what rain and
//! water throw off the player's body.

use auberean_animation::body::{Body as Figure, PartPose};
use auberean_draw::rain::{
    body_effects, Bone, Ground, Rain, RainWorld, Thrown, BUBBLES, MAPPING, RIPPLES, SHOWS_OVER,
    VAPOR,
};
use auberean_draw::scene::{ObjectBatch, SceneBuilder};
use auberean_draw::systems::{ParticleSystems, SystemRef};
use auberean_physics::body::GRAVITY;
use auberean_physics::land::{Land, CELL_LENGTH};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::{add, normalize, Mat4};
use auberean_world::records::WorldRecords;
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::LandblockId;

/// Land cells on a block's side.
const CELLS_PER_BLOCK: i32 = 8;

/// Where the camera stands this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Viewer {
    /// The block whose frame it stands in.
    pub home: (i32, i32),
    pub eye: Vec3,
    /// The camera's right.
    pub right: Vec3,
    /// How high the water of its land cell lies.
    pub water: Option<f32>,
    pub now: f64,
}

/// The player's skeleton where it stands.
#[derive(Clone, Debug)]
pub(crate) struct BodyBones {
    pub bones: Vec<Bone>,
    pub place: Mat4,
    /// How high the water of its land cell lies.
    pub water: Option<f32>,
}

/// A figure's first group's bones that have parents, in the body's frame.
// D2-06657
pub(crate) fn bones_of(figure: &Figure, poses: &[PartPose]) -> Vec<Bone> {
    let (Some(part), Some(pose)) = (figure.parts.first(), poses.first()) else {
        return Vec::new();
    };
    let at = |k: usize| {
        pose.bones
            .get(k)
            .map(|b| pose.world.point(b.point(Vec3::ZERO)))
    };
    part.setup
        .bones
        .iter()
        .enumerate()
        .filter_map(|(k, b)| {
            let parent = usize::try_from(b.parent)
                .ok()
                .filter(|_| b.parent != u32::MAX)?;
            Some(Bone {
                at: at(k)?,
                parent: at(parent)?,
            })
        })
        .collect()
}

/// The weather the camera can see.
#[derive(Default)]
pub(crate) struct Weather {
    rain: Rain,
    systems: Option<[Option<SystemRef>; 3]>,
    heard: f32,
}

impl std::fmt::Debug for Weather {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Weather")
            .field("heard", &self.heard)
            .finish_non_exhaustive()
    }
}

impl Weather {
    /// How hard the rain the camera can see falls.
    pub(crate) fn heard(&self) -> f32 {
        self.heard
    }

    /// The world starts afresh, or the camera can see nothing of the outdoors.
    // D2-11023
    pub(crate) fn indoors(&mut self) {
        self.heard = 0.0;
    }

    /// The frame's turn (with what the world's start made), given the moment's rain and wind: the streaks moved on, what they
    /// and the player's body throw off, the rain's systems queued at the viewer; the draws of the streaks, when it rains hard
    /// enough to show them.
    // D2-11029, D2-10935, D2-11000
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame(
        &mut self,
        records: &WorldRecords,
        builder: &mut SceneBuilder<'_>,
        systems: &ParticleSystems,
        land: &Land,
        (amount, wind): (f32, Vec3),
        v: &Viewer,
        player: Option<&BodyBones>,
    ) -> Vec<ObjectBatch> {
        let dice = systems.dice();
        self.heard = amount;
        let made = self.systems.get_or_insert_with(|| {
            [VAPOR, BUBBLES, RIPPLES].map(|value| {
                let id = records.data_id_by_enum(value, MAPPING);
                let made = systems.create(
                    records,
                    id,
                    false,
                    v.home,
                    Mat4::IDENTITY,
                    Vec3::new(1.0, 1.0, 1.0),
                    1.0,
                    v.now,
                );
                if let Some(s) = &made {
                    systems.set_no_fade(s);
                }
                made
            })
        });
        let [vapor, bubbles, ripples] = &*made;
        let push = add(wind, Vec3::new(0.0, 0.0, GRAVITY));
        let cell = (
            v.home.0 * CELLS_PER_BLOCK + floor_to_i32(v.eye.x / CELL_LENGTH),
            v.home.1 * CELLS_PER_BLOCK + floor_to_i32(v.eye.y / CELL_LENGTH),
        );
        let world = RainWorld {
            cell,
            viewer: v.eye,
            right: v.right,
            push,
            water: v.water,
            now: v.now,
            cells_per_block: CELLS_PER_BLOCK,
            cell_length: CELL_LENGTH,
        };
        self.rain.update(
            amount,
            &world,
            &dice,
            &mut |block, p| {
                let Some(id) = block_id(block) else {
                    return Ground::At(None);
                };
                let (x, y) = (id.x(), id.y());
                if land.block(x, y).is_none() {
                    return Ground::Unknown;
                }
                Ground::At(
                    land.ground(id, dereth_primitives::Vec3::new(p.x, p.y, p.z))
                        .map(|(h, _)| h),
                )
            },
            &mut |thrown, at| {
                let s = match thrown {
                    Thrown::Vapor => vapor,
                    Thrown::Ripple => ripples,
                    Thrown::Bubbles => bubbles,
                };
                if let Some(s) = s {
                    systems.discharge(s, 1, v.now, at);
                }
            },
        );
        let shows = amount > SHOWS_OVER;
        let out = if shows {
            builder.rain_batches(&self.rain)
        } else {
            Vec::new()
        };
        if let Some(b) = player {
            body_effects(
                &b.bones,
                &b.place,
                shows.then(|| normalize(push)),
                b.water,
                &dice,
                &mut |thrown, at| {
                    let s = match thrown {
                        Thrown::Vapor => vapor,
                        Thrown::Bubbles => bubbles,
                        Thrown::Ripple => ripples,
                    };
                    if let Some(s) = s {
                        systems.discharge(s, 0, v.now, at);
                    }
                },
            );
        }
        for s in queued(shows, [vapor, bubbles, ripples])
            .into_iter()
            .flatten()
        {
            systems.queue(
                s,
                v.home,
                Mat4::IDENTITY,
                Vec3::new(1.0, 1.0, 1.0),
                1.0,
                Color::WHITE,
                v.now,
            );
        }
        out
    }
}

/// The rain's systems drawn this frame: its vapor only while enough rain falls to show, its bubbles
/// and ripples always.
// D2-11042
fn queued<T: Copy>(shows: bool, [vapor, bubbles, ripples]: [T; 3]) -> Vec<T> {
    let mut out = Vec::with_capacity(3);
    if shows {
        out.push(vapor);
    }
    out.extend([bubbles, ripples]);
    out
}

fn block_id(block: (i32, i32)) -> Option<LandblockId> {
    let x = u8::try_from(block.0).ok()?;
    let y = u8::try_from(block.1).ok()?;
    Some(LandblockId::new(x, y))
}

impl BodyBones {
    /// The body's bones for the rain and the water to touch, when it stands outdoors in cell
    /// `cell`; inside they touch it nowhere, whatever the camera sees.
    // D2-11036
    pub(crate) fn outdoors(cell: u32, bones: impl FnOnce() -> Self) -> Option<Self> {
        (!crate::is_interior(cell)).then(bones)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (which of the rain's systems a frame draws; the rain itself is the draw crate's)
    use super::{queued, BodyBones};
    use auberean_world::math::Mat4;

    #[test]
    fn rain_touches_a_body_only_outdoors() {
        let bones = || BodyBones {
            bones: Vec::new(),
            place: Mat4::IDENTITY,
            water: None,
        };
        assert!(BodyBones::outdoors(0xC156_0035, bones).is_some());
        assert!(BodyBones::outdoors(0x0298_0121, bones).is_none());
    }

    #[test]
    fn the_rains_vapor_is_drawn_only_while_enough_rain_falls_to_show() {
        assert_eq!(
            queued(true, ["vapor", "bubbles", "ripples"]),
            ["vapor", "bubbles", "ripples"]
        );
        assert_eq!(
            queued(false, ["vapor", "bubbles", "ripples"]),
            ["bubbles", "ripples"]
        );
    }
}
