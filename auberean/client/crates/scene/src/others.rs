//! The other bodies as a window draws them: the bodies of everything else the client knows in the
//! world ([`auberean_client_sdk::sim::others`], which moves them), each with the effects it plays,
//! drawn where it stands between its last two ticks with its effects, its lights and its streaks.
//!
//! Only the bodies in the world within the detailed blocks around the viewer's are drawn (an
//! interior's cells all belong to its landblock, however far their places reach past its edges),
//! and not those told not to be drawn or whose drawing properties keep them out of the game. An
//! object whose look plays startup effects begins them the first time it is drawn after it is put
//! together, and they move with it; the effects of a body that goes end with it.

use auberean_animation::CachedRecords;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_draw::systems::ParticleSystems;
use auberean_physics::mesh::Trees;
use auberean_physics::world::World;
use auberean_primitives::{Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;
use dereth_primitives::Position;

pub use auberean_client_sdk::sim::others::{
    drawn, is_loaded, is_newer, out_of_physics_range, position_of, sent_again, Front, Looks,
    LostCell, LostCells, OtherBody, Received, NEAR_RADIUS, PHYSICS_RANGE,
};

use crate::BLOCK_LENGTH;

impl Front for crate::fx::BodyFx {
    fn again(old: Option<Self>, look: &auberean_animation::look::Look, id: u64) -> Option<Self> {
        Self::again(old, look, id)
    }
}

/// How many landblocks around the viewer's each way a frame draws bodies in.
const DETAIL_RADIUS: i32 = 2;

/// Whether a body standing in cell `cell` is drawn in a frame of block `block`: its cell's
/// landblock is within the detailed blocks around the viewer's. An interior's cells all belong to
/// its landblock, however far their places reach past the landblock's edges (the human training
/// hall's run more than 500 m north of its corner).
fn in_detail(cell: u32, block: (i32, i32)) -> bool {
    let (x, y) = (
        i32::try_from(cell >> 24).unwrap_or(0),
        i32::try_from((cell >> 16) & 0xFF).unwrap_or(0),
    );
    (x - block.0).abs() <= DETAIL_RADIUS && (y - block.1).abs() <= DETAIL_RADIUS
}

/// The other bodies, each with the effects it plays: the world without a window's
/// ([`auberean_client_sdk::sim::others::Others`]), which it derefs to, and their drawing.
#[derive(Debug, Default)]
pub(crate) struct Others(auberean_client_sdk::sim::others::Others<crate::fx::BodyFx>);

impl core::ops::Deref for Others {
    type Target = auberean_client_sdk::sim::others::Others<crate::fx::BodyFx>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for Others {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Others {
    /// The sounds every body's effects played or stopped since last taken, each with its object:
    /// first those of the bodies that went, whose effects end as they go, in the order they went.
    pub(crate) fn take_sounds(&mut self, out: &mut Vec<(u64, auberean_draw::effects::FxSound)>) {
        for (id, mut fx) in self.0.take_parted() {
            fx.fx.destroy();
            out.extend(fx.fx.take_sounds().into_iter().map(|s| (id, s)));
            out.extend(fx.sounds.drain(..).map(|s| (id, s)));
        }
        for fx in self.0.entries_mut().filter_map(|(_, b)| b.front.as_mut()) {
            out.extend(fx.sounds.drain(..).map(|s| (fx.id, s)));
        }
    }

    /// The bodies as batches, placed in block `block`'s frame, those `table` keeps out of the game
    /// left out, each body drawn noted: its id, the range of its batches in `out`, where it stands
    /// in the block's frame and its heading. A body left out is not noted, so it casts no shadow.
    // D2-06218, D2-06015
    #[allow(clippy::too_many_arguments)]
    pub fn draw_noting(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        table: &World,
        block: (i32, i32),
        alpha: f32,
        out: &mut Vec<ObjectBatch>,
        noted: &mut Vec<(u64, std::ops::Range<usize>, Vec3, f32)>,
    ) {
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        for (id, b) in self.0.entries() {
            let (Some(f), Some(ob)) = (&b.figure, &b.body) else {
                continue;
            };
            // Only the bodies in the world within the detailed blocks around the viewer, and not
            // those told not to be drawn or whose drawing properties keep them out of the game.
            if !ob.in_world()
                || !in_detail(ob.body.position.cell.0, block)
                || ob.body.is_no_draw()
                || table.find(id).and_then(|k| table.game_draw(k)) == Some(false)
            {
                continue;
            }
            let (place, rel, heading) = drawn_place(ob, alpha, (ox, oy));
            let poses = f.pose(anims);
            let start = out.len();
            drawer.next_body(ob.body.is_mobile());
            out.extend(drawer.batches(records, f, &poses, &place));
            noted.push((id, start..out.len(), rel, heading));
        }
    }

    /// Whether body `id` has a part past its visible distance as it stands now.
    pub(crate) fn leaves_out_a_part(
        &self,
        id: u64,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        alpha: f32,
    ) -> bool {
        let Some(b) = self.0.entry(id) else {
            return false;
        };
        let (Some(f), Some(ob)) = (&b.figure, &b.body) else {
            return false;
        };
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        let (place, _, _) = drawn_place(ob, alpha, (ox, oy));
        drawer.leaves_out_a_part(f, &place, ob.body.is_mobile())
    }

    /// Body `id`'s draws, wherever it stands from the eye: what it would be drawn as, for the box
    /// it fills past its visible distance. `None` when it has no body.
    pub(crate) fn draws_at_any_distance(
        &self,
        id: u64,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        alpha: f32,
    ) -> Option<Vec<ObjectBatch>> {
        let b = self.0.entry(id)?;
        let (f, ob) = (b.figure.as_ref()?, b.body.as_ref()?);
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        let (place, _, _) = drawn_place(ob, alpha, (ox, oy));
        let poses = f.pose(anims);
        let eye = drawer.eye.take();
        let draws = drawer.batches(records, f, &poses, &place);
        drawer.eye = eye;
        Some(draws)
    }

    /// Run the effects the bodies about block `block` play to `now` (the client's running
    /// time), each body where it is drawn `alpha` of a tick past its last tick, lit by `light`,
    /// their systems kept and drawn by `systems`, the water each stands over as `land` has it. A
    /// body's effects begin the first time it is drawn after it is put together. A body whose
    /// effects changed its appearance takes the new look (its parts' meshes read through `trees`).
    /// The decals the bodies' effects laid since last asked, each with where its body stands.
    pub(crate) fn take_decals(
        &mut self,
        out: &mut Vec<(Position, auberean_draw::effects::FxDecal)>,
    ) {
        for b in self.0.entries_mut().map(|(_, b)| b) {
            if let (Some(fx), Some(ob)) = (&mut b.front, &b.body) {
                let place = ob.body.position;
                out.extend(fx.fx.take_decals().into_iter().map(|d| (place, d)));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_effects(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        trees: &mut Trees,
        land: &auberean_physics::land::Land,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        alpha: f32,
        now: f64,
        light: auberean_world::color::Color,
        night: bool,
    ) {
        for b in self.0.entries_mut().map(|(_, b)| b) {
            let (Some(fx), Some(ob)) = (&mut b.front, &mut b.body) else {
                continue;
            };
            // Only those in the world within the detailed blocks around the viewer.
            if !ob.in_world() || !in_detail(ob.body.position.cell.0, block) {
                continue;
            }
            let (at, heading) = ob.drawn(alpha);
            let whole = ob.body.total_scale(None);
            let who = auberean_draw::effects::FxWho {
                player: false,
                night,
            };
            // Its repulsors take the speed its physics moves it at.
            let v = ob.body.velocity;
            fx.fx
                .set_velocity(auberean_primitives::Vec3::new(v.x, v.y, v.z));
            // Its systems are queued as it is drawn: not when told not to be, nor past its
            // visible distance.
            #[allow(clippy::cast_precision_loss)]
            let origin = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
            let (place, _, _) = drawn_place(ob, alpha, origin);
            let past = b
                .figure
                .as_ref()
                .is_some_and(|f| drawer.leaves_out_a_part(f, &place, ob.body.is_mobile()));
            fx.fx.set_drawn(!ob.body.is_no_draw() && !past);
            let water = land.water_plane(ob.body.position.cell).map(|p| -p.d);
            let changed = fx.think(
                systems,
                records,
                anims,
                at,
                heading,
                Vec3::new(whole.x, whole.y, whole.z),
                light,
                now,
                who,
                water,
                b.figure.as_mut(),
                b.dress.as_mut(),
            );
            if let Some(look) = changed {
                b.scale = look.scale;
                crate::body_takes_look(&mut ob.body, records, trees, &look);
            }
        }
    }

    /// What the bodies' effects asked of streaks since last taken: each body's own are turned on or
    /// off at once; each ask comes back with its body (for what it holds) and the holding location
    /// it means.
    // D2-06197, D2-06198
    pub(crate) fn take_streak_asks(&mut self) -> Vec<(u64, bool, u32)> {
        let mut out = Vec::new();
        for (id, b) in self.0.entries_mut() {
            if let Some(fx) = b.front.as_mut() {
                for (on, location) in fx.fx.take_streak_asks() {
                    fx.fx.turn_streaks(on);
                    out.push((id, on, location));
                }
            }
        }
        out
    }

    /// The effects body `id` plays, when it has any.
    pub(crate) fn effects_of(&self, id: u64) -> Option<&crate::fx::BodyFx> {
        self.0.entry(id)?.front.as_ref()
    }

    /// The effects body `id` plays, when it has any.
    pub(crate) fn effects_mut(&mut self, id: u64) -> Option<&mut crate::fx::BodyFx> {
        self.0.entry_mut(id)?.front.as_mut()
    }

    /// The streaks the bodies' own effects made, for frame `frame`: each placed where its body is
    /// drawn this frame (`alpha` of the way between its last two ticks), when it is drawn (in the
    /// world within the detailed blocks around the viewer, and not told not to be), run, and drawn.
    pub(crate) fn streaks(&mut self, frame: &mut crate::held::StreakFrame<'_>, alpha: f32) {
        for b in self.0.entries_mut().map(|(_, b)| b) {
            let (Some(fx), Some(ob)) = (b.front.as_mut(), b.body.as_ref()) else {
                continue;
            };
            let drawn = b.figure.is_some()
                && ob.in_world()
                && in_detail(ob.body.position.cell.0, frame.block)
                && !ob.body.is_no_draw();
            let at = drawn.then(|| (ob.body.drawn(alpha).0, ob.body.total_scale(None)));
            crate::held::object_streaks(&mut fx.fx, at.as_ref().map(|(p, s)| (p, *s)), frame);
        }
    }

    /// The bodies standing outdoors within the detailed blocks around the viewer (block `block`)
    /// whose objects cast on the land, as they print on the sun's shadow map now.
    // D2-03691, D2-06218
    pub fn land_casters(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        world: &World,
        block: (i32, i32),
        alpha: f32,
    ) -> Vec<ObjectBatch> {
        use auberean_physics::world::graphics;
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        let mut out = Vec::new();
        for b in self.0.entries().map(|(_, b)| b) {
            let (Some(f), Some(ob), Some(k)) = (&b.figure, &b.body, b.key) else {
                continue;
            };
            let cell = ob.body.position.cell.0;
            if !ob.in_world()
                || !in_detail(cell, block)
                || crate::is_interior(cell)
                || ob.body.is_no_draw()
                || !world.graphics_state(k, graphics::LANDSCAPE_SHADOW)
            {
                continue;
            }
            let place = drawn_place(ob, alpha, (ox, oy)).0;
            out.extend(drawer.land_caster_batches(records, f, &f.pose(anims), &place));
        }
        out
    }

    /// The lights the effects of the bodies in the world within the detailed blocks around the
    /// viewer (block `block`) placed on them, as they stand `now` with each body where it is
    /// drawn (`alpha` of the way between its last two ticks): each with the block its place is in.
    pub(crate) fn effect_lights(
        &self,
        block: (i32, i32),
        alpha: f32,
        now: f64,
    ) -> Vec<(auberean_draw::effects::PlacedLight, (i32, i32))> {
        let mut out = Vec::new();
        for b in self.0.entries().map(|(_, b)| b) {
            let (Some(fx), Some(ob)) = (&b.front, &b.body) else {
                continue;
            };
            if !ob.in_world() || !in_detail(ob.body.position.cell.0, block) {
                continue;
            }
            let (at, heading) = ob.drawn(alpha);
            let opacity = b.figure.as_ref().map_or(1.0, |f| f.sliders.opacity());
            out.extend(fx.lights(at, heading, ob.body.position.cell.0, now, opacity));
        }
        out
    }

    /// Asks object `id`'s body to play effect `effect` at a strength (or, with none, to stop it)
    /// the next time it stands somewhere; whether it has effects to play it with.
    pub(crate) fn ask_fx(&mut self, id: u64, effect: u32, play: Option<f32>) -> bool {
        match self.0.entry_mut(id).and_then(|b| b.front.as_mut()) {
            Some(fx) => {
                fx.asked.push((effect, play));
                true
            }
            None => false,
        }
    }
}

/// The block a point on the one grid lies in.
pub(crate) fn block_of_global(at: Vec3) -> (i32, i32) {
    (
        dereth_primitives::num::floor_to_i32(at.x / BLOCK_LENGTH),
        dereth_primitives::num::floor_to_i32(at.y / BLOCK_LENGTH),
    )
}

/// A body's place (its origin and turn, no scale) at `at` on the one grid facing `heading`, in
/// block `home`'s frame.
pub(crate) fn place_in(at: Vec3, heading: f32, home: (i32, i32)) -> Mat4 {
    #[allow(clippy::cast_precision_loss)]
    let rel = Vec3::new(
        at.x - home.0 as f32 * BLOCK_LENGTH,
        at.y - home.1 as f32 * BLOCK_LENGTH,
        at.z,
    );
    let rot = auberean_physics::body::heading_rotation(heading);
    Mat4::from_parts(
        rel,
        Quat {
            w: rot.w,
            x: rot.x,
            y: rot.y,
            z: rot.z,
        },
        Vec3::new(1.0, 1.0, 1.0),
    )
}

/// Where a body is drawn between its last two ticks, at its whole scale, in the frame of the block
/// whose corner is at `(ox, oy)`: its place, where it stands and its heading.
fn drawn_place(ob: &OtherBody, alpha: f32, (ox, oy): (f32, f32)) -> (Mat4, Vec3, f32) {
    let (at, heading) = ob.drawn(alpha);
    let rel = Vec3::new(at.x - ox, at.y - oy, at.z);
    let rot = auberean_physics::body::heading_rotation(heading);
    let s = ob.body.total_scale(None);
    let place = Mat4::from_parts(
        rel,
        Quat {
            w: rot.w,
            x: rot.x,
            y: rot.y,
            z: rot.z,
        },
        Vec3::new(s.x, s.y, s.z),
    );
    (place, rel, heading)
}
