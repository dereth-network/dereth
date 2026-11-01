//! The bodies of everything the client knows in the world besides the player: each object with a
//! place and a visual description, put together once (again whenever its description changes),
//! with a physics body of its own that the positions the server sends for it steer.
//!
//! A position the server sends for another object ([`OtherBody::receive`]) is taken only when its
//! position stamp is newer than the last one taken. Within 96 m of the player the body is in
//! physics range: it moves and animates. A newer teleport stamp, or a body out of range, puts the
//! body at the sent place at once; otherwise the place becomes the next keyframe of the path the
//! body follows (or of a jump or push the position carries), unless it is more than 15 m from the
//! body, which puts it there. Every position also tethers the body near it.
//!
//! Every body in range runs the physics ticks of the frame against the world around the player,
//! steered along its path by its motion interpreter; every body is drawn between its last two
//! ticks, and animated by how it moved over the last one.
//!
//! An object whose look plays startup effects begins them the first time it is drawn after it is
//! put together, and they move with it.
//!
//! Only the cells of the landblocks around the player's (the one it stands in and the eight
//! beside it) are loaded. An object whose position lies in a cell not loaded leaves the world: it
//! is kept, with that position, in the list of the objects lost in that cell, and neither moves
//! nor is drawn; when the cell is loaded again, every object lost in it enters the world where it
//! was last put.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use auberean_animation::body::Body as Figure;
use auberean_animation::host::Completion;
use auberean_animation::CachedRecords;
use auberean_client_sdk::model::model::Place;
use auberean_client_sdk::model::{Model, Object};
use auberean_client_sdk::protocol::types::{VisualDesc as WireVisual, WeenieDesc};
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_draw::systems::ParticleSystems;
use auberean_physics::body::missile::{ObjectTable, Objects};
use auberean_physics::body::{Body, BodyConfig, Notice, TICK};
use auberean_physics::geom::{self, Heading};
use auberean_physics::land::Land;
use auberean_primitives::{Quat, Vec3};
use auberean_world::look::ObjectLook;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame as PFrame, Position};

use crate::figures::look_after;
use crate::BLOCK_LENGTH;
use auberean_physics::collision::manager::BodyKey;
use auberean_physics::mesh::Trees;
use auberean_physics::object::Shape;
use auberean_physics::world::{NoGameObjects, World, WorldObject};

/// Beyond this distance from the player a body is out of physics range: it neither moves nor
/// animates, and every position puts it in place, meters.
pub const PHYSICS_RANGE: f32 = 96.0;

/// Whether stamp `b` is newer than stamp `a`, the stamps counting round through 65536.
// D2-05665
#[must_use]
pub fn is_newer(a: u16, b: u16) -> bool {
    if a == b {
        return false;
    }
    let (gap, mut sign) = if b < a { (a - b, -1) } else { (b - a, 1) };
    if gap > 0x7FFF {
        sign = -sign;
    }
    sign > 0
}

/// The place a position names, facing its heading.
#[must_use]
pub fn position_of(place: &Place) -> Position {
    let o = place.origin;
    let mut p = Position::new(
        CellId(place.cell),
        PFrame::new(
            dereth_primitives::Vec3::new(o.x, o.y, o.z),
            dereth_primitives::Quat::IDENTITY,
        ),
    );
    geom::set_heading(&mut p.frame, Heading::new(place.heading()));
    p
}

/// How many landblocks around the player's each way have their cells loaded.
pub const NEAR_RADIUS: u32 = 1;

/// Whether a cell is loaded while the player stands in cell `around` (everything is, with no
/// player): a land cell when `land` holds its landblock (the near window's), an interior cell when
/// its landblock is the player's or one beside it.
// D2-10776
#[must_use]
pub fn is_loaded(cell: u32, around: Option<u32>, land: &Land) -> bool {
    let Some(at) = around else {
        return true;
    };
    let (x, y) = ((cell >> 24) & 0xFF, (cell >> 16) & 0xFF);
    if cell & 0xFFFF < 0x100 {
        return land
            .block(u8::try_from(x).unwrap_or(0), u8::try_from(y).unwrap_or(0))
            .is_some();
    }
    let (px, py) = ((at >> 24) & 0xFF, (at >> 16) & 0xFF);
    x.abs_diff(px) <= NEAR_RADIUS && y.abs_diff(py) <= NEAR_RADIUS
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

/// The objects lost in one cell not loaded, by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LostCell {
    pub cell: u32,
    pub objects: Vec<u64>,
}

impl LostCell {
    // D2-05745
    #[must_use]
    pub fn new(cell: u32) -> Self {
        Self {
            cell,
            objects: Vec::new(),
        }
    }

    // D2-05747
    pub fn add(&mut self, id: u64) {
        self.objects.push(id);
    }

    /// Takes an object out; the last one takes its place. Whether it was there.
    // D2-05746
    pub fn remove(&mut self, id: u64) -> bool {
        match self.objects.iter().position(|o| *o == id) {
            Some(i) => {
                self.objects.swap_remove(i);
                true
            }
            None => false,
        }
    }
}

/// What a position did to a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Received {
    /// Not newer than the last taken.
    Ignored,
    /// Taken.
    Taken,
    /// Its cell is not loaded: the body left the world, to enter it there when the cell is.
    Lost(Position),
}

/// Another object's physics body, steered by the positions the server sends for it.
#[derive(Debug)]
pub struct OtherBody {
    /// The object's id.
    pub id: u64,
    pub body: Body,
    /// The position and teleport stamps of the last position taken.
    pub stamps: (u16, u16),
    /// Within physics range of the player: it moves and animates.
    pub in_range: bool,
    /// Out of the world, and where it enters it again.
    pub away: Option<Position>,
}

fn pv(v: Vec3) -> dereth_primitives::Vec3 {
    dereth_primitives::Vec3::new(v.x, v.y, v.z)
}

impl OtherBody {
    /// Object `id`'s body of `config` standing at `at` facing `heading`, its stamps `stamps`.
    #[must_use]
    pub fn new(
        id: u64,
        config: BodyConfig,
        at: Position,
        heading: f32,
        stamps: (u16, u16),
    ) -> Self {
        let mut body = Body::new(config, at, heading);
        body.is_player = false;
        Self {
            id,
            body,
            stamps,
            in_range: true,
            away: None,
        }
    }

    /// Whether the body is in the world (not lost in a cell not loaded).
    #[must_use]
    pub fn in_world(&self) -> bool {
        self.away.is_none()
    }

    /// The body leaves the world, to enter it again at `p`.
    // D2-06057, D2-06161
    pub fn leave_visibility(&mut self, p: Position) {
        self.body.stop_interpolating();
        self.body.in_world = false;
        self.away = Some(p);
    }

    /// The body enters the world again where it was last put.
    // D2-06058, D2-06059
    pub fn re_enter_visibility(&mut self) {
        if let Some(p) = self.away.take() {
            self.body.in_world = true;
            self.put(p);
        }
    }

    /// Put the body at `p` at once, with nothing left to follow and nothing to draw it moving
    /// from.
    // D2-06369
    pub fn put(&mut self, p: Position) {
        self.body.stop_interpolating();
        let heading = geom::frame_heading(&p.frame).degrees();
        self.body.place(p, heading);
        self.body.velocity = dereth_primitives::Vec3::ZERO;
        self.body.turn_rate = 0.0;
    }

    /// A position the server sent for this object, `player` where the player is, `loaded` whether
    /// the position's cell is loaded.
    // D2-06372, D2-06103, D2-06102
    pub fn receive(&mut self, place: &Place, player: Option<&Position>, loaded: bool) -> Received {
        if !is_newer(self.stamps.0, place.position_stamp) || place.cell == 0 {
            return Received::Ignored;
        }
        let p = position_of(place);
        self.body.minterp.constrain_to(self.id, false, p);
        if !loaded {
            self.leave_visibility(p);
            self.stamps = (place.position_stamp, place.teleport_stamp);
            return Received::Lost(p);
        }
        if self.away.is_some() {
            // In a loaded cell again: there at once.
            self.away = None;
            self.body.in_world = true;
            self.put(p);
            self.stamps = (place.position_stamp, place.teleport_stamp);
            return Received::Taken;
        }
        let mut out = false;
        if let Some(me) = player {
            let o = geom::offset(me, &p);
            let d = (o.x * o.x + o.y * o.y + o.z * o.z).sqrt();
            self.in_range = d <= PHYSICS_RANGE;
            out = !self.in_range;
            // In range the body moves and animates; out of it, neither.
            self.body.set_movement_active(self.in_range);
            self.body.set_animation_active(self.in_range);
        }
        let heading = Heading::new(place.heading());
        if is_newer(self.stamps.1, place.teleport_stamp) || out {
            self.put(p);
        } else if !place.jump() && !place.impulse() {
            // A place more than 15 m away is taken at once; nearer, it is followed.
            self.body
                .interpolate_to(place.time, p, pv(place.motion()), heading, place.contact());
        } else {
            self.body.interpolate_and_impulse(
                place.time,
                p,
                pv(place.motion()),
                heading,
                pv(place.velocity),
                place.jump(),
            );
        }
        self.stamps = (place.position_stamp, place.teleport_stamp);
        Received::Taken
    }

    /// One physics tick against `land` and the world's other objects, with the game clock at
    /// `game_now`, as the physics update runs it: it moves while its movement runs, and its other
    /// updates (its detections, its collisions reported) run while woken.
    pub fn tick(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        game_now: f64,
    ) {
        #[allow(clippy::cast_possible_truncation)]
        let dt = TICK as f32;
        // The body's clock is the game's: the positions it follows carry the server's.
        self.body.clock = game_now - TICK;
        self.body
            .update_in(land, world, objects, &NoGameObjects, dt);
    }

    /// Where the body is drawn `alpha` of a tick past its last tick, on one grid across every
    /// block, and its heading.
    #[must_use]
    pub fn drawn(&self, alpha: f32) -> (Vec3, f32) {
        let (p, h) = self.body.drawn(alpha);
        let c = p.cell.0;
        let o = p.frame.origin;
        (crate::figures::global(c, Vec3::new(o.x, o.y, o.z)), h)
    }
}

struct Other {
    /// The description the figure was put together from.
    visual: Option<WireVisual>,
    /// Its own descriptions (none when its look is a stored record's).
    dress: Option<ObjectLook>,
    figure: Option<Figure>,
    body: Option<OtherBody>,
    scale: Vec3,
    seen: bool,
    /// The cell it is lost in (0 for none).
    lost_cell: u32,
    /// The behaviors its figure was last told the body shows.
    shown: BTreeSet<u32>,
    /// Its handle in the world's table while it is in the world.
    key: Option<BodyKey>,
    /// The shape it collides as, for an object with no collision spheres (a sign, a statue): it
    /// stands where it is put rather than moving as a body.
    shape: Option<Arc<Shape>>,
    /// The effects it plays.
    effects: Option<crate::fx::BodyFx>,
    /// The incarnation it was made for (the object's instance stamp), and how many of that
    /// incarnation's creations it has taken.
    instance: u16,
    creations: u32,
    /// With no body, whether it stands in the world: its place's cell is loaded.
    standing: bool,
}

/// The other bodies, by object id, and the objects lost in cells not loaded.
#[derive(Default)]
pub struct Others {
    bodies: BTreeMap<u64, Other>,
    lost: BTreeMap<u32, LostCell>,
    /// The bodies that left the world (`false`) or entered it again (`true`) since last taken.
    world_changes: Vec<(u64, bool)>,
    /// What each figure finished and the hooks it fired since last taken, by body.
    heard: Vec<(u64, Vec<Completion>, Vec<u32>)>,
}

impl std::fmt::Debug for Others {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Others")
            .field("bodies", &self.bodies.len())
            .finish()
    }
}

fn drawn(o: &Object) -> bool {
    o.in_world && o.place.is_some() && o.parent.is_none() && o.container.is_none()
}

/// The heading a rotation faces, degrees clockwise from north.
fn heading_of(q: Quat) -> f32 {
    let a = 2.0 * math::atan2f(q.z, q.w);
    (-a.to_degrees()).rem_euclid(360.0)
}

impl Others {
    /// How many bodies are drawn.
    #[must_use]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.bodies
            .values()
            .filter(|b| b.figure.is_some() && b.body.as_ref().is_some_and(OtherBody::in_world))
            .count()
    }

    /// The figure of object `id`, when it has one.
    pub fn figure_mut(&mut self, id: u64) -> Option<&mut Figure> {
        self.bodies.get_mut(&id)?.figure.as_mut()
    }

    /// Object `id` as a holder: its body, its figure and its look's scale, when it has both.
    pub(crate) fn holder(&self, id: u64) -> Option<(&Body, &Figure, Vec3)> {
        let o = self.bodies.get(&id)?;
        Some((&o.body.as_ref()?.body, o.figure.as_ref()?, o.scale))
    }

    /// Object `id` as a holder taking something up: its body and its figure.
    pub(crate) fn holder_mut(&mut self, id: u64) -> Option<(&mut Body, &Figure)> {
        let o = self.bodies.get_mut(&id)?;
        Some((&mut o.body.as_mut()?.body, o.figure.as_ref()?))
    }

    /// What each figure finished and the hooks it fired since last taken, by body.
    pub(crate) fn take_heard(&mut self) -> Vec<(u64, Vec<Completion>, Vec<u32>)> {
        std::mem::take(&mut self.heard)
    }

    /// The bodies that left the world (`false`) or entered it again (`true`) since last taken.
    /// Object `id`'s figure, when it has one.
    #[must_use]
    pub fn figure(&self, id: u64) -> Option<&Figure> {
        self.bodies.get(&id)?.figure.as_ref()
    }

    /// Whether object `id` stands in the world here: its body is in it, or, with no body, its
    /// place's cell is loaded. An object not drawn here (held, contained, with no place) does not.
    #[must_use]
    pub fn stands(&self, id: u64) -> bool {
        self.bodies
            .get(&id)
            .is_some_and(|b| b.body.as_ref().map_or(b.standing, OtherBody::in_world))
    }

    pub(crate) fn take_world_changes(&mut self) -> Vec<(u64, bool)> {
        std::mem::take(&mut self.world_changes)
    }

    /// What every body's figure asked beyond its animation since last taken, with its id.
    pub(crate) fn take_figure_events(&mut self) -> Vec<(u64, auberean_animation::host::Event)> {
        let mut out = Vec::new();
        for (id, b) in &mut self.bodies {
            if let Some(f) = &mut b.figure {
                out.extend(f.take_events().into_iter().map(|e| (*id, e)));
            }
        }
        out
    }

    /// The physics body of object `id`, when it has one in the world.
    #[must_use]
    pub fn body(&self, id: u64) -> Option<&Body> {
        self.bodies
            .get(&id)?
            .body
            .as_ref()
            .filter(|b| b.in_world())
            .map(|b| &b.body)
    }

    /// The physics body of object `id`, when it has one.
    pub fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
        self.bodies.get_mut(&id)?.body.as_mut().map(|b| &mut b.body)
    }

    /// What every body asked of the others, its figure and its game object since last taken,
    /// each with the id of the body that asked.
    pub fn take_notices(&mut self) -> Vec<(u64, Notice)> {
        let mut out = Vec::new();
        for (id, b) in &mut self.bodies {
            if let Some(ob) = b.body.as_mut() {
                out.extend(ob.body.take_requests().into_iter().map(|n| (*id, n)));
            }
        }
        out
    }

    /// Notes every body in the world in `table`, as the bodies' guidance sees the others: where it
    /// is and how it moves, or, for an object that does not move, where it stands.
    pub fn note_objects(&self, table: &mut ObjectTable) {
        for (id, b) in &self.bodies {
            let Some(ob) = b.body.as_ref().filter(|ob| ob.in_world()) else {
                continue;
            };
            if b.shape.is_some() {
                let s = b.scale;
                table.note_still(*id, ob.body.position, ob.body.height(), pv(s));
            } else {
                table.note(
                    *id,
                    &ob.body,
                    auberean_client_sdk::model::ids::is_player(*id),
                );
            }
        }
    }

    /// Object `id`'s body goes: out of the cell it is lost in and out of the world's table.
    // D2-05725, D2-05638
    fn destroy_object(&mut self, id: u64, world: &mut World) {
        self.remove_from_lost_cell(id);
        if let Some(k) = self.bodies.get(&id).and_then(|b| b.key) {
            world.remove(k);
        }
        self.bodies.remove(&id);
    }

    /// The objects lost in cell `cell`, made when first asked for.
    // D2-05694
    fn lost_cell(&mut self, cell: u32) -> &mut LostCell {
        self.lost.entry(cell).or_insert_with(|| LostCell::new(cell))
    }

    /// Takes an object out of the cell it is lost in. Whether it could.
    // D2-05695, D2-05621
    fn remove_from_lost_cell(&mut self, id: u64) -> bool {
        let Some(b) = self.bodies.get_mut(&id) else {
            return false;
        };
        let cell = std::mem::take(&mut b.lost_cell);
        if cell != 0 {
            if let Some(l) = self.lost.get_mut(&cell) {
                l.remove(id);
            }
        }
        true
    }

    /// Puts an object in the list of those lost in `cell`, out of any other. Whether it went.
    // D2-05708, D2-05620
    fn goto_lost_cell(&mut self, id: u64, cell: u32) -> bool {
        if cell == 0 || !self.bodies.contains_key(&id) {
            return false;
        }
        self.remove_from_lost_cell(id);
        if let Some(b) = self.bodies.get_mut(&id) {
            b.lost_cell = cell;
        }
        self.lost_cell(cell).add(id);
        true
    }

    /// Cell `cell` is loaded: every object lost in it enters the world.
    // D2-05681, D2-05624
    fn init_obj_cell(&mut self, cell: u32) {
        let Some(l) = self.lost.remove(&cell) else {
            return;
        };
        for id in l.objects {
            if let Some(b) = self.bodies.get_mut(&id) {
                b.lost_cell = 0;
                if let Some(ob) = b.body.as_mut() {
                    ob.re_enter_visibility();
                }
            }
        }
    }

    /// Forgets every lost cell.
    // D2-05674, D2-16422
    pub fn flush_lost_cells(&mut self) {
        self.lost.clear();
        for b in self.bodies.values_mut() {
            b.lost_cell = 0;
        }
    }

    /// How many objects are lost in cells not loaded.
    #[must_use]
    pub fn lost(&self) -> usize {
        self.lost.values().map(|l| l.objects.len()).sum()
    }

    /// The drawn bodies nearest `at` (on the one grid across every block), each where it is drawn
    /// `alpha` of a tick past its last tick: their ids, distances and bearings (degrees clockwise
    /// from north), nearest first.
    #[must_use]
    pub fn nearest(&self, at: Vec3, n: usize, alpha: f32) -> Vec<(u64, f32, f32)> {
        let mut v: Vec<(u64, f32, f32)> = self
            .bodies
            .iter()
            .filter(|(_, b)| b.figure.is_some())
            .filter_map(|(id, b)| {
                let ob = b.body.as_ref().filter(|ob| ob.in_world())?;
                let (p, _) = ob.drawn(alpha);
                let (dx, dy) = (p.x - at.x, p.y - at.y);
                let d = (dx * dx + dy * dy).sqrt();
                Some((*id, d, math::atan2f(dx, dy).to_degrees().rem_euclid(360.0)))
            })
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.truncate(n);
        v
    }

    /// Follow the model at time `now` (the game clock at `game_now`): new objects put together,
    /// gone ones dropped, the positions heard since the last frame taken, every body in range run
    /// through the frame's physics ticks against `land` and animated by how it moved, over `dt`
    /// seconds; `player` is the player's id and where its body is.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        model: &mut Model,
        land: &Land,
        world: &mut World,
        trees: &mut Trees,
        player: Option<(u64, Position)>,
        objects: &mut dyn Objects,
        ticks: u32,
        game_now: f64,
        dt: f32,
    ) {
        for b in self.bodies.values_mut() {
            b.seen = false;
        }
        let was_in_world: BTreeMap<u64, bool> = self
            .bodies
            .iter()
            .filter_map(|(id, b)| b.body.as_ref().map(|ob| (*id, ob.in_world())))
            .collect();
        let player_id = player.map(|p| p.0);
        let player_at = player.map(|p| p.1);
        let around = player_at.map(|p| p.cell.0);
        // The cells loaded again take back the objects lost in them.
        let found: Vec<u32> = self
            .lost
            .keys()
            .copied()
            .filter(|c| is_loaded(*c, around, land))
            .collect();
        for c in found {
            self.init_obj_cell(c);
        }
        let ids: Vec<u64> = model.objects().map(|o| o.id).collect();
        for id in ids {
            if Some(id) == player_id {
                continue;
            }
            let Some(o) = model.object_mut(id) else {
                continue;
            };
            let reports = std::mem::take(&mut o.reports);
            if !drawn(o) {
                continue;
            }
            let Some(place) = o.place else { continue };
            let o = &*o;
            // A newer incarnation of an object the client has: the one it had goes, and the new
            // one is made where it was created.
            // D2-05731, D2-05737
            if self
                .bodies
                .get(&id)
                .is_some_and(|b| b.instance != o.instance_stamp)
            {
                self.destroy_object(id, world);
            }
            let solid = ethereality(&o.weenie);
            let o_desc = physics_description(o);
            let new = !self.bodies.contains_key(&id);
            let b = self.bodies.entry(id).or_insert_with(|| Other {
                visual: None,
                dress: None,
                figure: None,
                body: None,
                scale: Vec3::new(1.0, 1.0, 1.0),
                seen: true,
                lost_cell: 0,
                shown: BTreeSet::new(),
                key: None,
                shape: None,
                effects: None,
                instance: o.instance_stamp,
                creations: o.creations,
                standing: false,
            });
            b.seen = true;
            // A creation of the incarnation it has: put where the creation says it stands, its
            // stamps the creation's (an object hanging from another is not drawn here).
            // D2-05660
            let created = new || b.creations != o.creations;
            if created {
                b.creations = o.creations;
                if let Some(ob) = b.body.as_mut() {
                    let c = &o.physics;
                    if c.bits & auberean_client_sdk::protocol::types::PhysicsDesc::POSITION != 0
                        && c.position.cell != 0
                    {
                        let (at, _) = creation_place(o, &place);
                        ob.put(at);
                        ob.stamps = (c.stamps[0], c.stamps[1]);
                    }
                }
            }
            if b.visual.as_ref() != Some(&o.visual) {
                b.visual = Some(o.visual.clone());
                let look = look_after(records, anims, &o.visual, &mut b.dress);
                b.scale = look.as_ref().map_or(Vec3::new(1.0, 1.0, 1.0), |l| l.scale);
                // Its figure brought up to date (made when it has none), its effects made again
                // (its look's startup effects begun when it is next drawn).
                crate::takes_look(anims, &mut b.figure, &mut b.effects, look.as_ref(), id);
                let config = look
                    .as_ref()
                    .zip(b.figure.as_ref())
                    .and_then(|(l, f)| crate::body_config(records, l, f, &o.physics));
                // The body stands where the object was last placed, or where it was created.
                let (at, heading) = match &b.body {
                    Some(old) => (old.body.position, old.body.heading),
                    None => creation_place(o, &place),
                };
                let stamps = b
                    .body
                    .as_ref()
                    .map_or((place.position_stamp, place.teleport_stamp), |old| {
                        old.stamps
                    });
                if let Some(k) = b.key.take() {
                    world.remove(k);
                }
                b.shape = config
                    .as_ref()
                    .filter(|(c, _)| c.spheres.is_empty())
                    .and(look.as_ref())
                    .and_then(|l| Shape::of_look(records, l, trees))
                    .map(Arc::new);
                b.body = config.map(|(c, _)| {
                    let mut ob = OtherBody::new(id, c, at, heading, stamps);
                    if let Some(k) = crate::velocity_scale(&o.physics) {
                        ob.body.minterp.velocity_scale = k;
                    }
                    ob.body.minterp.scale = pv(b.scale);
                    if let Some(l) = look.as_ref() {
                        crate::give_parts(&mut ob.body, records, trees, l);
                    }
                    crate::profile::init_physics(records, o, &mut ob.body);
                    ob
                });
            }
            // A creation's mode and sliders, once its figure is made: the figure stands in that
            // mode at once (a corpse lying dead) and its sliders go to their values.
            if created {
                if let Some(f) = b.figure.as_mut() {
                    let c = &o.physics;
                    use auberean_client_sdk::protocol::types::PhysicsDesc as Pd;
                    if c.bits & Pd::MODE != 0 && c.mode != 0 {
                        f.set_mode(anims, c.mode);
                    }
                    if c.bits & Pd::SLIDERS != 0 {
                        for sl in &c.sliders {
                            f.sliders.do_slider(0, sl.slider, sl.value, sl.time);
                        }
                    }
                }
            }
            b.standing = b.body.is_none() && is_loaded(place.cell, around, land);
            let Some(ob) = b.body.as_mut() else { continue };
            let mut lost_in = None;
            for r in &reports {
                match ob.receive(r, player_at.as_ref(), is_loaded(r.cell, around, land)) {
                    Received::Lost(p) => lost_in = Some(p.cell.0),
                    Received::Taken => lost_in = None,
                    Received::Ignored => {}
                }
            }
            // A body in the world whose cell is no longer loaded leaves it where it is.
            if ob.in_world() && !is_loaded(ob.body.position.cell.0, around, land) {
                let at = ob.body.position;
                ob.leave_visibility(at);
                lost_in = Some(at.cell.0);
            }
            let in_world = ob.in_world();
            let was_lost = b.lost_cell;
            if !in_world {
                if let Some(k) = b.key.take() {
                    world.remove(k);
                }
            }
            if let Some(cell) = lost_in {
                if was_lost != cell {
                    self.goto_lost_cell(id, cell);
                }
                continue;
            }
            if was_lost != 0 && in_world {
                self.remove_from_lost_cell(id);
            }
            let Some(b) = self.bodies.get_mut(&id) else {
                continue;
            };
            let Some(ob) = b.body.as_mut() else { continue };
            if !ob.in_world() {
                continue;
            }
            // In the world's table: as a body that moves, or as a shape put where it stands.
            match (&b.shape, b.key) {
                (Some(shape), None) => {
                    let k = world.insert(
                        land,
                        id,
                        &WorldObject {
                            shape: Arc::clone(shape),
                            position: ob.body.position,
                            is_static: true,
                            passable: false,
                        },
                    );
                    world.set_ethereality(k, solid);
                    world.set_size(k, ob.body.radius(), ob.body.height());
                    apply_description(records, world, k, o_desc);
                    b.key = Some(k);
                }
                (Some(_), Some(k)) => {
                    if world.position(k) != Some(ob.body.position) {
                        world.place(land, k, ob.body.position);
                    }
                }
                (None, None) => {
                    let k = world.insert_body(land, id, &mut ob.body);
                    world.set_ethereality(k, solid);
                    apply_description(records, world, k, o_desc);
                    b.key = Some(k);
                }
                (None, Some(_)) => {}
            }
            if ob.in_range {
                if b.shape.is_none() {
                    for _ in 0..ticks {
                        ob.tick(land, world, objects, game_now);
                    }
                    // What its collisions began and ended goes to its game object, which is the
                    // server's here: only noted.
                    if let Some(k) = b.key {
                        for report in world.take_reports(k) {
                            tracing::debug!(
                                id = format_args!("{id:#x}"),
                                ?report,
                                "a body's collisions reported"
                            );
                        }
                    }
                }
                if let Some(f) = &mut b.figure {
                    // Feet that match how the body moved over its last tick.
                    let v = ob.body.local_velocity();
                    f.set_motion(Vec3::new(v.x, v.y, v.z), ob.body.turn_rate);
                    crate::forward_behaviors(&ob.body, &mut b.shown, f, anims);
                    f.ground = crate::ground_of(&ob.body);
                    f.advance(anims, dt);
                    self.heard
                        .push((id, f.take_completions(), crate::take_triggers(f).0));
                }
            }
        }
        for (id, b) in &self.bodies {
            if let (Some(ob), Some(was)) = (b.body.as_ref(), was_in_world.get(id)) {
                if ob.in_world() != *was {
                    self.world_changes.push((*id, ob.in_world()));
                }
            }
        }
        let gone: Vec<u64> = self
            .bodies
            .iter()
            .filter(|(_, b)| !b.seen)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.destroy_object(id, world);
        }
        if player.is_none() {
            self.flush_lost_cells();
        }
    }

    /// The bodies as batches, placed in block `block`'s frame.
    pub fn draw(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        alpha: f32,
        out: &mut Vec<ObjectBatch>,
    ) {
        self.draw_noting(records, anims, drawer, block, alpha, out, &mut Vec::new());
    }

    /// The bodies as batches, as [`Self::draw`], each body drawn noted: its id, the range of its
    /// batches in `out`, where it stands in the block's frame and its heading.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_noting(
        &self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        drawer: &mut BodyDrawer,
        block: (i32, i32),
        alpha: f32,
        out: &mut Vec<ObjectBatch>,
        noted: &mut Vec<(u64, std::ops::Range<usize>, Vec3, f32)>,
    ) {
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (block.0 as f32 * BLOCK_LENGTH, block.1 as f32 * BLOCK_LENGTH);
        for (id, b) in &self.bodies {
            let (Some(f), Some(ob)) = (&b.figure, &b.body) else {
                continue;
            };
            // Only the bodies in the world within the detailed blocks around the viewer, and not
            // those told not to be drawn.
            if !ob.in_world() || !in_detail(ob.body.position.cell.0, block) || ob.body.is_no_draw()
            {
                continue;
            }
            // Drawn between its last two ticks, at its whole scale.
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
            let poses = f.pose(anims);
            let start = out.len();
            out.extend(drawer.batches(records, f, &poses, &place));
            noted.push((*id, start..out.len(), rel, heading));
        }
    }

    /// Run the effects the bodies about block `block` play to `now` (the client's running
    /// time), each body where it is drawn `alpha` of a tick past its last tick, lit by `light`,
    /// their systems kept and drawn by `systems`, the water each stands over as `land` has it. A
    /// body's effects begin the first time it is drawn after it is put together. A body whose
    /// effects changed its appearance takes the new look (its parts' meshes read through `trees`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_effects(
        &mut self,
        systems: &ParticleSystems,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        trees: &mut Trees,
        land: &auberean_physics::land::Land,
        block: (i32, i32),
        alpha: f32,
        now: f64,
        light: auberean_world::color::Color,
        night: bool,
    ) {
        for b in self.bodies.values_mut() {
            let (Some(fx), Some(ob)) = (&mut b.effects, &mut b.body) else {
                continue;
            };
            // Only those in the world within the detailed blocks around the viewer.
            if !ob.in_world() || !in_detail(ob.body.position.cell.0, block) {
                continue;
            }
            let (at, heading) = ob.drawn(alpha);
            let who = auberean_draw::effects::FxWho {
                player: false,
                night,
            };
            // Its repulsors take the speed its physics moves it at.
            let v = ob.body.velocity;
            fx.fx
                .set_velocity(auberean_primitives::Vec3::new(v.x, v.y, v.z));
            let water = land.water_plane(ob.body.position.cell).map(|p| -p.d);
            let changed = fx.think(
                systems,
                records,
                anims,
                at,
                heading,
                b.scale,
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
                ob.body.minterp.scale = crate::pv(look.scale);
            }
        }
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
        for b in self.bodies.values() {
            let (Some(fx), Some(ob)) = (&b.effects, &b.body) else {
                continue;
            };
            if !ob.in_world() || !in_detail(ob.body.position.cell.0, block) {
                continue;
            }
            let (at, heading) = ob.drawn(alpha);
            out.extend(fx.lights(at, heading, ob.body.position.cell.0, now));
        }
        out
    }

    /// Every other body, by object.
    pub(crate) fn bodies(&self) -> impl Iterator<Item = (u64, &Body)> {
        self.bodies
            .iter()
            .filter_map(|(id, o)| o.body.as_ref().map(|b| (*id, &b.body)))
    }

    /// Object `id`'s own descriptions, when its body keeps them.
    pub(crate) fn dress(&self, id: u64) -> Option<&ObjectLook> {
        self.bodies.get(&id)?.dress.as_ref()
    }

    /// Asks object `id`'s body to play effect `effect` at a strength (or, with none, to stop it)
    /// the next time it stands somewhere; whether it has effects to play it with.
    pub(crate) fn ask_fx(&mut self, id: u64, effect: u32, play: Option<f32>) -> bool {
        match self.bodies.get_mut(&id).and_then(|b| b.effects.as_mut()) {
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

/// Where an object first stands: its last position when it has had one, else where it was
/// created, facing the creation's rotation.
fn creation_place(o: &Object, place: &Place) -> (Position, f32) {
    let created = o.physics.position.cell == place.cell
        && o.physics.position.frame.origin == place.origin
        && place.time == 0.0;
    let heading = if created {
        heading_of(o.physics.position.frame.rotation)
    } else {
        place.heading()
    };
    let mut p = position_of(place);
    geom::set_heading(&mut p.frame, Heading::new(heading));
    (p, heading)
}

/// What an object's description says it is and passes through, as the object's script applies it
/// when the object is made: each mask from its two halves, or nothing when either is missing.
pub(crate) fn ethereality(w: &WeenieDesc) -> auberean_physics::solid::Ethereality {
    let number = |bit: u32| {
        WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .filter(|_| w.mask & bit != 0)
            .map(|i| w.numbers[i])
    };
    let mask = |low, high| auberean_physics::solid::Ethereality::mask(number(low), number(high));
    auberean_physics::solid::Ethereality {
        physics_type: mask(0x8000, 0x1_0000),
        movement: mask(0x2_0000, 0x4_0000),
        placement: mask(0x8_0000, 0x10_0000),
    }
}

/// The quality naming the record an object's physics object is made from.
const PHYSICS_DESCRIPTION: u32 = 1;

/// The record object `o`'s physics object is made from, as its qualities name it (0: none).
pub(crate) fn physics_description(o: &Object) -> u32 {
    o.qualities
        .data_ids
        .get(&PHYSICS_DESCRIPTION)
        .copied()
        .unwrap_or(0)
}

/// Entry `key` takes the properties of the entity record `desc` its object is made from (an
/// object made from no entity takes none).
pub(crate) fn apply_description(
    records: &WorldRecords,
    world: &mut World,
    key: BodyKey,
    desc: u32,
) {
    use auberean_world::place::PlaceRecords;
    if desc == 0 {
        return;
    }
    let lookup = |id: u32| records.entity(auberean_primitives::DataId(id));
    world.apply_entity_properties(key, desc, &lookup);
}
