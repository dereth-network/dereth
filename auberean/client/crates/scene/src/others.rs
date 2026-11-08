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
//! Each body is made through the client, which names the physics record it is made from
//! ([`Others::make_physics_obj`]).
//!
//! Only the cells of the landblocks around the player's (the one it stands in and the eight
//! beside it) are loaded; with no player, none is. An object whose position (or creation) lies in
//! a cell not loaded leaves the world: it is kept, with that position, in the list of the objects
//! lost in that cell, and neither moves nor is drawn; when the cell is loaded again, every object
//! lost in it enters the world where it was last put. A body in the world leaves it where it
//! stands when the cell it stands in is let go of, unless it never moves
//! ([`Others::leave_visibility`], asked through the client).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use auberean_animation::body::Body as Figure;
use auberean_animation::host::Completion;
use auberean_animation::CachedRecords;
use auberean_client_sdk::model::model::Place;
use auberean_client_sdk::model::{Model, Object};
use auberean_client_sdk::protocol::types::{VisualDesc as WireVisual, WeenieDesc};
use auberean_client_sdk::runtime::world::PhysicsObjOrder;
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_draw::systems::ParticleSystems;
use auberean_physics::body::missile::{ObjectTable, Objects};
use auberean_physics::body::placing::error as place_error;
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

/// Whether `at` is past the physics range of a player at `player`.
// D2-06344
#[must_use]
pub fn out_of_physics_range(player: &Position, at: &Position) -> bool {
    let o = geom::offset(player, at);
    (o.x * o.x + o.y * o.y + o.z * o.z).sqrt() > PHYSICS_RANGE
}

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

/// Whether a cell is loaded while the player stands in cell `around` (none is, with no player:
/// the client holds no cells then): a land cell when `land` holds its landblock (the near
/// window's), an interior cell when its landblock is the player's or one beside it.
// D2-10776
#[must_use]
pub fn is_loaded(cell: u32, around: Option<u32>, land: &Land) -> bool {
    let Some(at) = around else {
        return false;
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

/// The lost cells: the objects lost in each cell not loaded, and the cell each object is lost in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LostCells {
    cells: BTreeMap<u32, LostCell>,
    of: BTreeMap<u64, u32>,
}

impl LostCells {
    /// The objects lost in cell `cell`, made when first asked for.
    // D2-05694
    fn lost_cell(&mut self, cell: u32) -> &mut LostCell {
        self.cells
            .entry(cell)
            .or_insert_with(|| LostCell::new(cell))
    }

    /// Takes object `id` out of the cell it is lost in. Whether it was lost in one.
    // D2-05695, D2-05621
    pub fn remove(&mut self, id: u64) -> bool {
        let Some(cell) = self.of.remove(&id) else {
            return false;
        };
        if let Some(l) = self.cells.get_mut(&cell) {
            l.remove(id);
        }
        true
    }

    /// Object `id` is lost in cell `cell`: out of any cell it was lost in first, then into
    /// `cell`'s list, unless it hangs from another (`has_parent`), which is lost in none. Whether
    /// it went into the list.
    // D2-05708, D2-05620
    pub fn goto(&mut self, id: u64, cell: u32, has_parent: bool) -> bool {
        if cell == 0 {
            return false;
        }
        self.remove(id);
        if has_parent {
            return false;
        }
        self.of.insert(id, cell);
        self.lost_cell(cell).add(id);
        true
    }

    /// Cell `cell` is loaded: its list goes, and the objects that were lost in it are given back,
    /// in the list's order.
    // D2-05681, D2-05624
    pub fn take(&mut self, cell: u32) -> Vec<u64> {
        let Some(l) = self.cells.remove(&cell) else {
            return Vec::new();
        };
        for id in &l.objects {
            self.of.remove(id);
        }
        l.objects
    }

    /// Forgets every lost cell.
    // D2-05674, D2-16422
    pub fn flush(&mut self) {
        self.cells.clear();
        self.of.clear();
    }

    /// The cell object `id` is lost in (0 for none).
    #[must_use]
    pub fn cell_of(&self, id: u64) -> u32 {
        self.of.get(&id).copied().unwrap_or(0)
    }

    /// The cells objects are lost in.
    pub fn cells(&self) -> impl Iterator<Item = u32> + '_ {
        self.cells.keys().copied()
    }

    /// How many objects are lost.
    #[must_use]
    pub fn len(&self) -> usize {
        self.of.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.of.is_empty()
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

/// The squared distance beyond which a position is taken at once, not followed: 15 m.
const FAR_SQUARED: f32 = 225.0;

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

    /// What its figure set off this step: its effects go among the figure's events, the kinds of
    /// its hooks are handed back, and a jump its animation reached wakes the body, which jumps if
    /// it may.
    // D2-06252
    pub fn take_figure_triggers(&mut self, figure: &mut Figure) -> Vec<u32> {
        let (hooks, impulses) = crate::take_triggers(figure);
        if impulses.iter().any(|i| i.jump) {
            self.body.set_movement_active(true);
            self.body.jump();
        }
        hooks
    }

    /// Whether the body is in the world (not lost in a cell not loaded).
    #[must_use]
    pub fn in_world(&self) -> bool {
        self.away.is_none()
    }

    /// Puts the body in the world's table as `shape` (its look's), standing where the body
    /// stands and solid at the body's whole scale: its look's times its own.
    pub fn stand_in_table(&self, world: &mut World, land: &Land, shape: Arc<Shape>) -> BodyKey {
        let k = world.insert(
            land,
            self.id,
            &WorldObject {
                shape,
                position: self.body.position,
                is_static: true,
                passable: false,
            },
        );
        world.set_scale(k, self.body.total_scale(None));
        k
    }

    /// The body, new in the world's table, is placed where it stands.
    // D2-05660
    pub fn settle(&mut self, land: &Land, world: &mut World) {
        let at = self.body.position;
        self.put(land, world, at);
    }

    /// The body leaves the world, to enter it again at `p`.
    // D2-06057, D2-06161, D2-06158
    pub fn leave_visibility(&mut self, p: Position) {
        self.body.stop_interpolating();
        self.body.in_world = false;
        self.away = Some(p);
    }

    /// The body enters the world again where it was last put.
    // D2-06058, D2-06059
    pub fn re_enter_visibility(&mut self, land: &Land, world: &mut World) {
        if let Some(p) = self.away.take() {
            self.body.in_world = true;
            self.put(land, world, p);
        }
    }

    /// A creation says the body stands at `p`: there at once, and in the world again if it was
    /// lost in a cell not loaded.
    // D2-05660, D2-06368, D2-06268
    pub fn put_by_creation(&mut self, land: &Land, world: &mut World, p: Position) {
        if self.away.take().is_some() {
            self.body.in_world = true;
        }
        self.put(land, world, p);
    }

    /// Put the body at `p` at once, with nothing left to follow and nothing to draw it moving
    /// from. Once it stands in the world's table it is placed: it takes a free spot nearby and
    /// touches only what is there, and with no room stays where it was.
    // D2-06369
    pub fn put(&mut self, land: &Land, world: &mut World, p: Position) {
        self.body.stop_interpolating();
        let heading = geom::frame_heading(&p.frame).degrees();
        let in_table = self.body.key().is_some_and(|k| world.position(k).is_some());
        if !in_table {
            self.body.place(p, heading);
        } else if self.body.set_position_simple(land, world, p, true) == place_error::OK {
            let at = self.body.position;
            self.body.place(at, heading);
        }
        self.body.velocity = dereth_primitives::Vec3::ZERO;
        self.body.turn_rate = 0.0;
        self.body.set_placement_position(p);
    }

    /// A position the server sent for this object, `player` where the player is, `loaded` whether
    /// the position's cell is loaded.
    // D2-06372, D2-06103, D2-06102
    pub fn receive(
        &mut self,
        place: &Place,
        player: Option<&Position>,
        loaded: bool,
        land: &Land,
        world: &mut World,
    ) -> Received {
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
            self.put(land, world, p);
            self.stamps = (place.position_stamp, place.teleport_stamp);
            return Received::Taken;
        }
        let mut out = false;
        if let Some(me) = player {
            out = out_of_physics_range(me, &p);
            self.in_range = !out;
            // In range the body moves and animates; out of it, neither.
            self.body.set_out_of_physics_range(out);
        }
        let heading = Heading::new(place.heading());
        let to = geom::offset(&self.body.position, &p);
        let far = to.x * to.x + to.y * to.y + to.z * to.z > FAR_SQUARED;
        if is_newer(self.stamps.1, place.teleport_stamp) || out || far {
            self.put(land, world, p);
        } else if !place.jump() && !place.impulse() {
            // Nearer than 15 m, a place is followed.
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

    /// The body's movement looks at the player's range, as it does every tick while it runs: past
    /// it, the body stops where it is until a position for it comes.
    // D2-06344, D2-06102
    pub fn check_range(&mut self, player: &Position) {
        if self.body.movement_checks_range() && out_of_physics_range(player, &self.body.position) {
            self.in_range = false;
            self.body.set_out_of_physics_range(true);
        }
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
    /// The object's count of descriptions sent, at the last one taken.
    visual_updates: u32,
    /// Its own descriptions (none when its look is a stored record's).
    dress: Option<ObjectLook>,
    figure: Option<Figure>,
    body: Option<OtherBody>,
    scale: Vec3,
    seen: bool,
    /// The physics record its physics object was made from, as the client named it (0 for
    /// none): its entry in the world's table takes that record's properties.
    record: u32,
    /// Just made: its creation's place, mode and sliders are still to be taken.
    fresh: bool,
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
    /// The objects the server described since last asked.
    described: Vec<u64>,
    lost: LostCells,
    /// The sounds of the effects of objects that went, not yet taken.
    parting_sounds: Vec<(u64, auberean_draw::effects::FxSound)>,
    /// The bodies that left the world (`false`) or entered it again (`true`) since last taken.
    world_changes: Vec<(u64, bool)>,
    /// What each figure finished and the hooks it fired since last taken, by body.
    heard: Vec<(u64, Vec<Completion>, Vec<u32>)>,
    /// The speed, acceleration and jump scales of the bodies that went while their objects stayed,
    /// with the incarnation they were for: the client's physics object outlives its body here.
    kept: BTreeMap<u64, (u16, [f32; 3])>,
}

impl std::fmt::Debug for Others {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Others")
            .field("bodies", &self.bodies.len())
            .finish()
    }
}

/// Whether the scene draws object `o` as a body of its own: in the world with a place, hanging from
/// nothing and in no container.
pub(crate) fn drawn(o: &Object) -> bool {
    o.in_world && o.place.is_some() && o.parent.is_none() && o.container.is_none()
}

/// The heading a rotation faces, degrees clockwise from north.
fn heading_of(q: Quat) -> f32 {
    let a = 2.0 * math::atan2f(q.z, q.w);
    (-a.to_degrees()).rem_euclid(360.0)
}

impl Others {
    /// The sounds every body's effects played or stopped since last taken, each with its object.
    pub(crate) fn take_sounds(&mut self, out: &mut Vec<(u64, auberean_draw::effects::FxSound)>) {
        out.append(&mut self.parting_sounds);
        for fx in self.bodies.values_mut().filter_map(|b| b.effects.as_mut()) {
            out.extend(fx.sounds.drain(..).map(|s| (fx.id, s)));
        }
    }

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
    /// The objects the server described since last asked (their creations): the scene brings each
    /// body up to its description.
    pub(crate) fn take_described(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.described)
    }

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
    /// is and how it moves, or, for an object that does not move, where it stands; and whether its
    /// game object is a player or a creature, as `player_or_creature` answers.
    pub fn note_objects(
        &self,
        table: &mut ObjectTable,
        mut player_or_creature: impl FnMut(u64) -> bool,
    ) {
        for (id, b) in &self.bodies {
            let Some(ob) = b.body.as_ref().filter(|ob| ob.in_world()) else {
                continue;
            };
            let asked = player_or_creature(*id);
            if b.shape.is_some() {
                let s = ob.body.total_scale(None);
                table.note_still(*id, &ob.body, s, asked);
            } else {
                table.note(*id, &ob.body, asked);
            }
        }
    }

    /// Object `id`'s entry in the world's table, if it stands in it.
    pub(crate) fn key_of(&self, id: u64) -> Option<auberean_physics::collision::manager::BodyKey> {
        let o = self.bodies.get(&id)?;
        o.key.or_else(|| o.body.as_ref().and_then(|b| b.body.key()))
    }

    /// Object `id`'s body goes: out of the cell it is lost in and out of the world's table.
    // D2-05725, D2-05638
    // D2-06178, D2-06138
    pub(crate) fn destroy_object(&mut self, id: u64, world: &mut World) {
        self.lost.remove(id);
        if let Some(k) = self.bodies.get(&id).and_then(|b| b.key) {
            world.remove(k);
        }
        if let Some(mut b) = self.bodies.remove(&id) {
            if let Some(fx) = b.effects.as_mut() {
                fx.fx.destroy();
                self.parting_sounds
                    .extend(fx.fx.take_sounds().into_iter().map(|s| (id, s)));
                self.parting_sounds
                    .extend(fx.sounds.drain(..).map(|s| (id, s)));
            }
            if let Some(ob) = b.body {
                let s = &ob.body;
                self.kept.insert(
                    id,
                    (
                        b.instance,
                        [s.velocity_scale(), s.acceleration_scale(), s.jump_scale()],
                    ),
                );
            }
        }
    }

    /// Puts an object the scene has in the list of those lost in `cell`, out of any other; one
    /// that hangs from another (`has_parent`) is lost in none. Whether it went.
    fn goto_lost_cell(&mut self, id: u64, cell: u32, has_parent: bool) -> bool {
        self.bodies.contains_key(&id) && self.lost.goto(id, cell, has_parent)
    }

    /// Cell `cell` is loaded: every object lost in it enters the world.
    fn init_obj_cell(&mut self, cell: u32, land: &Land, world: &mut World) {
        for id in self.lost.take(cell) {
            if let Some(ob) = self.bodies.get_mut(&id).and_then(|b| b.body.as_mut()) {
                ob.re_enter_visibility(land, world);
            }
        }
    }

    /// Forgets every lost cell.
    pub fn flush_lost_cells(&mut self) {
        self.lost.flush();
    }

    /// Object `id`'s body leaves the world where it stands, as its cell is let go of: it is lost in
    /// that cell (unless it hangs from another, `has_parent`) and out of the world's table until the
    /// cell is loaded again. Whether it was in the world.
    pub(crate) fn leave_visibility(
        &mut self,
        id: u64,
        has_parent: bool,
        world: &mut World,
    ) -> bool {
        let Some(b) = self.bodies.get_mut(&id) else {
            return false;
        };
        let Some(ob) = b.body.as_mut().filter(|ob| ob.in_world()) else {
            return false;
        };
        let at = ob.body.position;
        ob.leave_visibility(at);
        if let Some(k) = b.key.take() {
            world.remove(k);
        }
        self.goto_lost_cell(id, at.cell.0, has_parent);
        self.world_changes.push((id, false));
        true
    }

    /// Block `block` went among the free blocks, or came back: the static bodies standing in it
    /// are told.
    // D2-03634, D2-03633
    pub(crate) fn set_block_on_free_list(&mut self, block: (u8, u8), on: bool) {
        for b in self.bodies.values_mut() {
            let Some(ob) = b.body.as_mut().filter(|ob| ob.body.is_static()) else {
                continue;
            };
            let cell = ob.body.position.cell.0;
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the two high bytes of a cell id are its block.
            if ((cell >> 24) as u8, (cell >> 16) as u8) == block {
                ob.body.set_cell_on_free_list(on);
            }
        }
    }

    /// Whether object `id`'s body never moves (static).
    #[must_use]
    pub fn is_static(&self, id: u64) -> bool {
        self.bodies
            .get(&id)
            .and_then(|b| b.body.as_ref())
            .is_some_and(|ob| ob.body.is_static())
    }

    /// The cell each body stands in (`None` for one out of the world), by object.
    pub(crate) fn cells(&self) -> impl Iterator<Item = (u64, Option<u32>)> + '_ {
        self.bodies.iter().filter_map(|(id, b)| {
            let ob = b.body.as_ref()?;
            Some((*id, ob.in_world().then_some(ob.body.position.cell.0)))
        })
    }

    /// The cell object `id` is lost in (0 for none).
    #[must_use]
    pub fn lost_cell_of(&self, id: u64) -> u32 {
        self.lost.cell_of(id)
    }

    /// The physics record object `id`'s physics object was made from (0 for none); `None` when
    /// the scene has none for it.
    #[must_use]
    pub fn physics_record(&self, id: u64) -> Option<u32> {
        self.bodies.get(&id).map(|b| b.record)
    }

    /// How many objects are lost in cells not loaded.
    #[must_use]
    pub fn lost(&self) -> usize {
        self.lost.len()
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
    /// through the frame's physics ticks against `land` and, when the frame has an animation step
    /// (`anim`, in seconds), animated by how it moved; `player` is the player's id and where its
    /// body is.
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
        anim: Option<f32>,
        look: &mut dyn FnMut(u64, &Body, &mut Figure, f32),
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
            .cells()
            .filter(|c| is_loaded(*c, around, land))
            .collect();
        for c in found {
            self.init_obj_cell(c, land, world);
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
            let solid = ethereality(&o.weenie);
            // Its physics object was made through the client ([`Self::make_physics_obj`]).
            let Some(b) = self
                .bodies
                .get_mut(&id)
                .filter(|b| b.instance == o.instance_stamp)
            else {
                continue;
            };
            let o_desc = b.record;
            b.seen = true;
            let new = std::mem::take(&mut b.fresh);
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
                        ob.put_by_creation(land, world, at);
                        ob.stamps = (c.stamps[0], c.stamps[1]);
                    }
                }
            }
            if sent_again(b.visual.as_ref(), b.visual_updates, o) {
                // The description it has, sent again: its parts stay, and its effects play again.
                let look = look_after(records, anims, &o.visual, &mut b.dress);
                crate::takes_look(anims, &mut b.figure, &mut b.effects, look.as_ref(), o.id);
            }
            b.visual_updates = o.visual_updates;
            if b.visual.as_ref() != Some(&o.visual) {
                let looks = Looks {
                    records,
                    anims,
                    world: &mut *world,
                    trees: &mut *trees,
                };
                take_look(b, &o.visual, o, &place, &mut self.kept, looks);
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
                }
                self.described.push(id);
            }
            b.standing = b.body.is_none() && is_loaded(place.cell, around, land);
            let Some(ob) = b.body.as_mut() else { continue };
            let mut lost_in = None;
            for r in &reports {
                match ob.receive(
                    r,
                    player_at.as_ref(),
                    is_loaded(r.cell, around, land),
                    land,
                    world,
                ) {
                    Received::Lost(p) => lost_in = Some(p.cell.0),
                    Received::Taken => lost_in = None,
                    Received::Ignored => {}
                }
            }
            // A body its creation puts in a cell not loaded leaves the world where it is. (One
            // already in the world leaves it when its cell is let go of: [`Self::leave_visibility`].)
            if created && ob.in_world() && !is_loaded(ob.body.position.cell.0, around, land) {
                let at = ob.body.position;
                ob.leave_visibility(at);
                lost_in = Some(at.cell.0);
            }
            let in_world = ob.in_world();
            let was_lost = self.lost.cell_of(id);
            if !in_world {
                if let Some(k) = b.key.take() {
                    world.remove(k);
                }
            }
            if let Some(cell) = lost_in {
                if was_lost != cell {
                    self.goto_lost_cell(id, cell, o.has_parent());
                }
                continue;
            }
            if was_lost != 0 && in_world {
                self.lost.remove(id);
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
                    let k = ob.stand_in_table(world, land, Arc::clone(shape));
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
                    ob.settle(land, world);
                }
                (None, Some(_)) => {}
            }
            if let Some((_, me)) = player.as_ref().filter(|_| ticks > 0 && b.shape.is_none()) {
                ob.check_range(me);
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
                let frozen = ob.body.is_animation_frozen();
                if let (Some(f), Some(step)) = (&mut b.figure, anim.filter(|_| !frozen)) {
                    // Feet that match how the body moved over its last tick.
                    let (moved, turn, size) = crate::figure_motion(&mut ob.body, step);
                    f.set_motion(moved, turn, size);
                    crate::forward_behaviors(&ob.body, &mut b.shown, f, anims);
                    f.ground = crate::ground_of(&ob.body);
                    look(id, &ob.body, f, step);
                    f.advance(anims, step);
                    ob.body.figure_stands_in(f.modes.element());
                    let hooks = ob.take_figure_triggers(f);
                    self.heard.push((id, f.take_completions(), hooks));
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
        // An object the client forgot takes its physics object with it.
        self.kept.retain(|id, _| model.object(*id).is_some());
    }

    /// Whether object `o` (one the scene draws) wants its physics object made: the scene has none
    /// for it, or one for an older incarnation.
    pub(crate) fn wants_physics_obj(&self, o: &Object) -> bool {
        self.bodies
            .get(&o.id)
            .is_none_or(|b| b.instance != o.instance_stamp)
    }

    /// The physics object `order` describes, for the client's object `o` (which the scene draws):
    /// its figure and body from the order's visual description, standing where `o` was created
    /// (or last placed), its entry in the world's table to take the properties of the order's
    /// physics record. A newer incarnation of an object the scene has takes the place of the one
    /// it had. Whether it has a body: an object whose look makes none is kept, drawn by nothing.
    // D2-05731, D2-05737
    pub(crate) fn make_physics_obj(
        &mut self,
        order: &PhysicsObjOrder,
        o: &Object,
        looks: Looks<'_, '_>,
    ) -> bool {
        let Some(place) = o.place.filter(|_| order.id == o.id) else {
            return false;
        };
        if self.bodies.contains_key(&o.id) {
            self.destroy_object(o.id, looks.world);
        }
        let mut b = Other {
            visual: None,
            visual_updates: o.visual_updates,
            dress: None,
            figure: None,
            body: None,
            scale: Vec3::new(1.0, 1.0, 1.0),
            seen: true,
            record: order.physics_record,
            fresh: true,
            shown: BTreeSet::new(),
            key: None,
            shape: None,
            effects: None,
            instance: o.instance_stamp,
            creations: o.creations,
            standing: false,
        };
        take_look(&mut b, &order.visual, o, &place, &mut self.kept, looks);
        let made = b.body.is_some();
        self.bodies.insert(o.id, b);
        made
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
            let (place, rel, heading) = drawn_place(ob, alpha, (ox, oy));
            let poses = f.pose(anims);
            let start = out.len();
            drawer.next_body(ob.body.is_mobile());
            out.extend(drawer.batches(records, f, &poses, &place));
            noted.push((*id, start..out.len(), rel, heading));
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
        let Some(b) = self.bodies.get(&id) else {
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
        let b = self.bodies.get(&id)?;
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
        for b in self.bodies.values_mut() {
            if let (Some(fx), Some(ob)) = (&mut b.effects, &b.body) {
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
        for b in self.bodies.values_mut() {
            let (Some(fx), Some(ob)) = (&mut b.effects, &mut b.body) else {
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
        for (id, b) in &mut self.bodies {
            if let Some(fx) = b.effects.as_mut() {
                for (on, location) in fx.fx.take_streak_asks() {
                    fx.fx.turn_streaks(on);
                    out.push((*id, on, location));
                }
            }
        }
        out
    }

    /// The objects with a body here.
    pub(crate) fn ids(&self) -> Vec<u64> {
        self.bodies.keys().copied().collect()
    }

    /// The effects body `id` plays, when it has any.
    pub(crate) fn effects_of(&self, id: u64) -> Option<&crate::fx::BodyFx> {
        self.bodies.get(&id)?.effects.as_ref()
    }

    /// The effects body `id` plays, when it has any.
    pub(crate) fn effects_mut(&mut self, id: u64) -> Option<&mut crate::fx::BodyFx> {
        self.bodies.get_mut(&id)?.effects.as_mut()
    }

    /// The streaks the bodies' own effects made, for frame `frame`: each placed where its body is
    /// drawn this frame (`alpha` of the way between its last two ticks), when it is drawn (in the
    /// world within the detailed blocks around the viewer, and not told not to be), run, and drawn.
    pub(crate) fn streaks(&mut self, frame: &mut crate::held::StreakFrame<'_>, alpha: f32) {
        for b in self.bodies.values_mut() {
            let (Some(fx), Some(ob)) = (b.effects.as_mut(), b.body.as_ref()) else {
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
        for b in self.bodies.values() {
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
        for b in self.bodies.values() {
            let (Some(fx), Some(ob)) = (&b.effects, &b.body) else {
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

/// What making a body's look needs of the world.
pub(crate) struct Looks<'a, 'w> {
    pub records: &'a WorldRecords,
    pub anims: &'a CachedRecords<'w>,
    pub world: &'a mut World,
    pub trees: &'a mut Trees,
}

/// Whether the server has sent `o` the description already taken again, since the count `updates`.
#[must_use]
pub fn sent_again(taken: Option<&WireVisual>, updates: u32, o: &Object) -> bool {
    updates != o.visual_updates && taken == Some(&o.visual)
}

/// Object `o`'s entry takes the look of `visual`: its figure brought up to date (made when it has
/// none), its effects made again (its look's startup effects begun when it is next drawn), and its
/// body made anew for the look, standing where the object was last placed or else where it was
/// created (`place`), its scales those its old body had, or those `kept` for this incarnation.
fn take_look(
    b: &mut Other,
    visual: &WireVisual,
    o: &Object,
    place: &Place,
    kept: &mut BTreeMap<u64, (u16, [f32; 3])>,
    looks: Looks<'_, '_>,
) {
    let Looks {
        records,
        anims,
        world,
        trees,
    } = looks;
    let id = o.id;
    b.visual = Some(visual.clone());
    let look = look_after(records, anims, visual, &mut b.dress);
    b.scale = look.as_ref().map_or(Vec3::new(1.0, 1.0, 1.0), |l| l.scale);
    crate::takes_look(anims, &mut b.figure, &mut b.effects, look.as_ref(), id);
    let config = look
        .as_ref()
        .zip(b.figure.as_ref())
        .and_then(|(l, f)| crate::body_config(records, l, f, &o.physics));
    let (at, heading) = match &b.body {
        Some(old) => (old.body.position, old.body.heading),
        None => creation_place(o, place),
    };
    let stamps = b
        .body
        .as_ref()
        .map_or((place.position_stamp, place.teleport_stamp), |old| {
            old.stamps
        });
    // The scales the server gave its physics object, which outlives a change of look and its body
    // going while the object stays.
    let scales = b
        .body
        .as_ref()
        .map(|old| {
            [
                old.body.velocity_scale(),
                old.body.acceleration_scale(),
                old.body.jump_scale(),
            ]
        })
        .or_else(|| {
            kept.remove(&id)
                .filter(|(instance, _)| *instance == o.instance_stamp)
                .map(|(_, s)| s)
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
        if let Some(l) = look.as_ref() {
            crate::give_parts(&mut ob.body, records, trees, l);
        }
        if let Some(f) = b.figure.as_ref() {
            ob.body.set_mode(f.mode);
        }
        crate::profile::init_physics(records, o, &mut ob.body);
        if let Some([v, a, j]) = scales {
            ob.body.set_velocity_scale(v);
            ob.body.set_acceleration_scale(a);
            ob.body.set_jump_scale(j);
        }
        ob
    });
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
    auberean_physics::solid::Ethereality::of_numbers(number)
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
