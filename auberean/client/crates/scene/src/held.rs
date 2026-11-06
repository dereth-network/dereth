//! What the bodies hold: every object the server says another holds (a wielded sword, a shield
//! on a back) is put together as its figure with a physics body of its own, taken up by its
//! holder's body ([`set_parent`]) and drawn each frame where it hangs from its holder's figure.
//!
//! [`set_parent`]: auberean_physics::body::parent::set_parent
//!
//! An object the server stops calling held is let go of. A holder put together again (its look
//! changed) has an empty list, so what it held is taken up again.

use std::collections::BTreeMap;

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_assets::animation::MotionParams;
use auberean_client_sdk::model::Object;
use auberean_client_sdk::protocol::types::VisualDesc as WireVisual;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_physics::body::parent::{ChildInfo, ChildKind, OWN_FRAME};
use auberean_physics::body::{Body, BodyConfig, Notice};
use auberean_physics::motion::values::default_values;
use auberean_primitives::{Quat, Vec3};
use auberean_world::look::ObjectLook;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;
use dereth_primitives::Position;

use crate::figures::look_after;
use crate::BLOCK_LENGTH;

/// One held object.
pub(crate) struct Held {
    visual: Option<WireVisual>,
    /// Its own descriptions (none when its look is a stored record's).
    dress: Option<ObjectLook>,
    pub(crate) figure: Option<Figure>,
    pub(crate) body: Body,
    scale: Vec3,
    /// The holder and the holding location it is taken up at, once it is.
    pub(crate) holder: Option<(u64, u32)>,
    /// The effects it plays.
    pub(crate) effects: Option<crate::fx::BodyFx>,
}

/// The held objects, by id.
#[derive(Default)]
pub struct HeldObjects {
    held: BTreeMap<u64, Held>,
}

impl std::fmt::Debug for HeldObjects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeldObjects")
            .field("held", &self.held.len())
            .finish()
    }
}

/// A held object's body: no collision spheres of its own, standing at `at`.
fn held_body(id: u64, at: Position, scale: f32) -> Body {
    let still: MotionParams = default_values();
    let mut b = Body::new(
        BodyConfig {
            spheres: Vec::new(),
            step_up: 0.0,
            step_down: 0.0,
            scale,
            ground: still,
            air: still,
        },
        at,
        0.0,
    );
    b.id = id;
    b.is_player = false;
    b
}

/// The matrix of child `info`'s hanging point on `figure`, in the holder's frame: a holding
/// location's own transform (with its scale) then its bone's; a bone of a kind; a bone of the
/// root group by index; none for the holder's own frame or a point the figure lacks.
pub(crate) fn hanging(
    figure: &Figure,
    anims: &CachedRecords<'_>,
    info: &ChildInfo,
) -> Option<[[f32; 4]; 4]> {
    match info.kind {
        ChildKind::HoldingLocation if info.location == OWN_FRAME => None,
        ChildKind::HoldingLocation => {
            let (_, location, bone) = figure.holding_location(anims, info.location)?;
            let t = location.transform;
            Some(
                Mat4::from_parts(t.offset, t.rotation, t.scale)
                    .then(&bone)
                    .0,
            )
        }
        ChildKind::BoneName => figure.bone_type_matrix(anims, info.location).map(|m| m.0),
        ChildKind::BoneIndex => figure
            .pose(anims)
            .first()
            .and_then(|p| p.bones.get(info.location as usize))
            .map(|m| m.0),
        ChildKind::None => None,
    }
}

/// Where `figure`'s first bone of kind `kind` stands, moved first by `offset` in the bone's own
/// frame, the figure drawn at `drawn` with `scale`: the bone's frame in the figure's, its place
/// scaled, then the figure's place. None when the figure has no bone of that kind.
// D2-06166
pub(crate) fn bone_position(
    figure: &Figure,
    anims: &CachedRecords<'_>,
    drawn: &Position,
    scale: dereth_primitives::Vec3,
    kind: u32,
    offset: &Mat4,
) -> Option<Position> {
    use auberean_physics::body::parent::{combine_scaled, frame_from_matrix};
    let bone = figure.bone_type_matrix(anims, kind)?;
    let at = frame_from_matrix(&offset.then(&bone).0);
    Some(Position::new(
        drawn.cell,
        combine_scaled(&drawn.frame, &at, scale),
    ))
}

impl HeldObjects {
    /// The decals the bodies' effects laid since last asked, each with where its body stands.
    pub(crate) fn take_decals(
        &mut self,
        out: &mut Vec<(Position, auberean_draw::effects::FxDecal)>,
    ) {
        for b in self.held.values_mut() {
            if let Some(fx) = b.effects.as_mut() {
                let place = b.body.position;
                out.extend(fx.fx.take_decals().into_iter().map(|d| (place, d)));
            }
        }
    }

    /// The sounds every body's effects played or stopped since last taken, each with its object.
    pub(crate) fn take_sounds(&mut self, out: &mut Vec<(u64, auberean_draw::effects::FxSound)>) {
        for fx in self.held.values_mut().filter_map(|b| b.effects.as_mut()) {
            out.extend(fx.sounds.drain(..).map(|s| (fx.id, s)));
        }
    }

    /// Held object `o`, made (or put together again when its look changed) from its visual
    /// description, standing at `at` until it is taken up. Its body takes the profile its
    /// qualities give when it is made, as every object's does.
    pub(crate) fn ensure(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        o: &Object,
        at: Position,
    ) -> &mut Held {
        let (id, visual) = (o.id, &o.visual);
        let h = self.held.entry(id).or_insert_with(|| {
            let mut body = held_body(id, at, 1.0);
            crate::profile::init_physics(records, o, &mut body);
            Held {
                visual: None,
                dress: None,
                figure: None,
                body,
                scale: Vec3::new(1.0, 1.0, 1.0),
                holder: None,
                effects: None,
            }
        });
        if h.visual.as_ref() != Some(visual) {
            h.visual = Some(visual.clone());
            let look = look_after(records, anims, visual, &mut h.dress);
            h.scale = look.as_ref().map_or(Vec3::new(1.0, 1.0, 1.0), |l| l.scale);
            crate::takes_look(anims, &mut h.figure, &mut h.effects, look.as_ref(), id);
            if let Some(l) = &look {
                let mut parts = auberean_physics::parts::PartGroup::default();
                if parts.init_look(records, l) {
                    h.body.set_parts(parts);
                }
                h.body.look_scale = crate::pv(l.scale);
                h.body.child_scale = l.child_scale;
            }
        }
        h
    }

    /// Held object `id`'s effects and figure, when it has effects.
    pub(crate) fn effects_mut(
        &mut self,
        id: u64,
    ) -> Option<(&mut crate::fx::BodyFx, Option<&mut Figure>)> {
        let h = self.held.get_mut(&id)?;
        Some((h.effects.as_mut()?, h.figure.as_mut()))
    }

    /// Held object `id`'s effects think where it is drawn (`drawn`: where on the one grid, its
    /// heading and scale), as [`crate::fx::BodyFx::think`] does; when they changed its appearance,
    /// its body's parts take the new look (each mesh read once through `trees`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn think_effects(
        &mut self,
        id: u64,
        systems: &auberean_draw::systems::ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        trees: &mut auberean_physics::mesh::Trees,
        drawn: (Vec3, f32, Vec3),
        light: auberean_world::color::Color,
        now: f64,
        who: auberean_draw::effects::FxWho,
        water: Option<f32>,
    ) {
        let Some(h) = self.held.get_mut(&id) else {
            return;
        };
        let Some(fx) = h.effects.as_mut() else {
            return;
        };
        let (at, heading, scale) = drawn;
        let changed = fx.think(
            systems,
            records,
            anims,
            at,
            heading,
            scale,
            light,
            now,
            who,
            water,
            h.figure.as_mut(),
            h.dress.as_mut(),
        );
        if let Some(look) = changed {
            h.scale = look.scale;
            crate::body_takes_look(&mut h.body, records, trees, &look);
        }
    }

    /// The effects held object `id` plays, when it plays any.
    pub(crate) fn effects_of(&self, id: u64) -> Option<&crate::fx::BodyFx> {
        self.held.get(&id)?.effects.as_ref()
    }

    /// Held object `id`'s own descriptions, when it keeps them.
    pub(crate) fn dress(&self, id: u64) -> Option<&ObjectLook> {
        self.held.get(&id)?.dress.as_ref()
    }

    /// The ids held.
    pub(crate) fn ids(&self) -> Vec<u64> {
        self.held.keys().copied().collect()
    }

    /// Lets go of object `id`: what its body asks (its holder letting go of it) is returned.
    pub(crate) fn let_go(&mut self, id: u64) -> Vec<Notice> {
        let Some(mut h) = self.held.remove(&id) else {
            return Vec::new();
        };
        h.body.unset_parent();
        h.body.take_requests()
    }

    /// The physics body of held object `id`.
    pub(crate) fn figure(&self, id: u64) -> Option<&Figure> {
        self.held.get(&id)?.figure.as_ref()
    }

    pub(crate) fn body(&self, id: u64) -> Option<&Body> {
        self.held.get(&id).map(|h| &h.body)
    }

    /// The physics body of held object `id`, to change it.
    pub fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
        self.held.get_mut(&id).map(|h| &mut h.body)
    }

    /// The figure of held object `id`.
    pub fn figure_mut(&mut self, id: u64) -> Option<&mut Figure> {
        self.held.get_mut(&id)?.figure.as_mut()
    }

    /// What every held object's body asked since last taken, with its id.
    pub fn take_notices(&mut self) -> Vec<(u64, Notice)> {
        let mut out = Vec::new();
        for (id, h) in &mut self.held {
            out.extend(h.body.take_requests().into_iter().map(|n| (*id, n)));
        }
        out
    }

    /// What every held object's figure asked beyond its animation since last taken, with its id.
    pub(crate) fn take_figure_events(&mut self) -> Vec<(u64, auberean_animation::host::Event)> {
        let mut out = Vec::new();
        for (id, h) in &mut self.held {
            if let Some(f) = &mut h.figure {
                out.extend(f.take_events().into_iter().map(|e| (*id, e)));
            }
        }
        out
    }

    /// The holder of each held object that is taken up: (object, holder).
    pub(crate) fn holders(&self) -> Vec<(u64, u64)> {
        self.held
            .iter()
            .filter_map(|(id, h)| h.holder.map(|(p, _)| (*id, p)))
            .collect()
    }

    /// Every figure animated over `dt` seconds.
    pub(crate) fn advance(&mut self, anims: &CachedRecords<'_>, dt: f32) {
        for h in self.held.values_mut() {
            if let Some(f) = &mut h.figure {
                f.advance(anims, dt);
                drop(f.take_completions());
                // A held object plays no story: its hooks go nowhere; its effects are played.
                drop(crate::take_triggers(f));
            }
        }
    }

    /// What the held objects' effects asked of streaks since last taken: each object's own are
    /// turned on or off at once; each ask comes back with its object (for what it holds in turn)
    /// and the holding location it means.
    pub(crate) fn take_streak_asks(&mut self) -> Vec<(u64, bool, u32)> {
        let mut out = Vec::new();
        for (id, h) in &mut self.held {
            if let Some(fx) = h.effects.as_mut() {
                for (on, location) in fx.fx.take_streak_asks() {
                    fx.fx.turn_streaks(on);
                    out.push((*id, on, location));
                }
            }
        }
        out
    }

    /// Turn on (`on`) or off the streaks of what `holder` holds at holding location `location`
    /// (all it holds for 0), and of what those hold in turn.
    // D2-06197, D2-06198
    pub(crate) fn turn_streaks(&mut self, holder: u64, location: u32, on: bool) {
        let mine: Vec<u64> = self
            .held
            .iter()
            .filter(|(_, h)| {
                h.holder
                    .is_some_and(|(p, at)| p == holder && (location == 0 || at == location))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in mine {
            if let Some(fx) = self.held.get_mut(&id).and_then(|h| h.effects.as_mut()) {
                fx.fx.turn_streaks(on);
            }
            self.turn_streaks(id, location, on);
        }
    }

    /// The streaks the held objects' effects made, for the frame `frame`: each placed where its
    /// object hangs this frame (`placed`), run, and drawn ([`object_streaks`]).
    // D2-10330, D2-10331
    pub(crate) fn streaks(
        &mut self,
        frame: &mut StreakFrame<'_>,
        placed: &[(u64, Position, dereth_primitives::Vec3)],
    ) {
        for (id, at, scale) in placed {
            let Some(h) = self.held.get_mut(id) else {
                continue;
            };
            let drawn = !h.body.is_no_draw();
            let Some(fx) = h.effects.as_mut() else {
                continue;
            };
            object_streaks(&mut fx.fx, drawn.then_some((at, *scale)), frame);
        }
    }

    /// The held objects as batches in block `block`'s frame, each at its place in `placed`
    /// (where it hangs this frame, with its heading), noted as [`crate::others::Others::draw_noting`]
    /// notes the bodies.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        placed: &[(u64, Position, dereth_primitives::Vec3)],
        out: &mut Vec<ObjectBatch>,
        noted: &mut Vec<(u64, std::ops::Range<usize>, Vec3, f32)>,
    ) {
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        for (id, at, scale) in placed {
            let Some(f) = self
                .held
                .get(id)
                .filter(|h| !h.body.is_no_draw())
                .and_then(|h| h.figure.as_ref())
            else {
                continue;
            };
            let o = at.frame.origin;
            let g = crate::figures::global(at.cell.0, Vec3::new(o.x, o.y, o.z));
            let rel = Vec3::new(g.x - ox, g.y - oy, g.z);
            let place = place_in_block(at, *scale, block);
            let poses = f.pose(anims);
            let start = out.len();
            drawer.next_body(false);
            out.extend(drawer.batches(records, f, &poses, &place));
            let heading = auberean_physics::geom::frame_heading(&at.frame).degrees();
            noted.push((*id, start..out.len(), rel, heading));
        }
    }
}

/// Where an object hanging at `at` with `scale` stands in block `block`'s frame: its place, turn
/// and scale.
fn place_in_block(at: &Position, scale: dereth_primitives::Vec3, block: (i32, i32)) -> Mat4 {
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
    let o = at.frame.origin;
    let g = crate::figures::global(at.cell.0, Vec3::new(o.x, o.y, o.z));
    let q = at.frame.rotation;
    Mat4::from_parts(
        Vec3::new(g.x - ox, g.y - oy, g.z),
        Quat {
            w: q.w,
            x: q.x,
            y: q.y,
            z: q.z,
        },
        Vec3::new(scale.x, scale.y, scale.z),
    )
}

/// One frame's streaks: its stamp, the block its world is drawn in, the client's running time,
/// each streak material's layers, and the room the frame's streaks have taken.
pub(crate) struct StreakFrame<'a> {
    pub stamp: u64,
    pub block: (i32, i32),
    pub now: f64,
    pub layers: &'a mut dyn FnMut(u32) -> Vec<auberean_draw::material::DrawLayer>,
    pub used: usize,
    pub out: Vec<ObjectBatch>,
}

/// The streaks one object's effects made, for frame `frame`: each placed where the object is
/// drawn this frame, when it is (`at`: its place and its scales; the place a frame without its
/// scale, which the streak takes as the largest of them), run, and its ribbon drawn in its
/// material's layers while the frame's streaks have room.
// D2-10325, D2-10327, D2-10318
pub(crate) fn object_streaks(
    fx: &mut auberean_draw::effects::ObjectEffects,
    at: Option<(&Position, dereth_primitives::Vec3)>,
    frame: &mut StreakFrame<'_>,
) {
    #[allow(clippy::cast_sign_loss)]
    let home = ((frame.block.0 as u32) << 24) | ((frame.block.1 as u32 & 0xFF) << 16) | 1;
    let place = at.map(|(p, scale)| {
        let one = dereth_primitives::Vec3::new(1.0, 1.0, 1.0);
        (
            place_in_block(p, one, frame.block),
            scale.x.max(scale.y).max(scale.z),
        )
    });
    for s in fx.streaks_mut() {
        if let Some((m, largest)) = place {
            s.set_position(frame.stamp, home, m, largest);
        }
        s.think(frame.now);
        let layers = (frame.layers)(s.material());
        frame
            .out
            .extend(s.batches(frame.stamp, &mut frame.used, &layers, home));
    }
}
