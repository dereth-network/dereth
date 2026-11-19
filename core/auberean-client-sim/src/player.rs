//! The player's body: its physics against the world and its figure, as the client keeps them.
//!
//! - **Made where the server put it.** When the client has the player's object and a place for
//!   it, the body is made ([`Player::make`]): the figure its visual description puts together, the
//!   collision spheres and step heights of its setup, its motion values, the profile its qualities
//!   give; its entry in the world's table takes the properties of the physics record the client
//!   named. Whenever the server puts the player somewhere (its creation, a teleport, a forced
//!   position) the body goes there ([`Player::take_server_place`]), and once the cells it was put
//!   in are in it is stood where there is room for it nearest that place ([`Player::settle`]).
//! - **Steered by the controls.** Each frame the player's controls steer it ([`Player::steer`]):
//!   none while the client froze it or a behavior locks its actions; the camera's heading turns it
//!   at once while the mouse steers. A jump asked plays the figure's lead-in when the figure can,
//!   which sets the jump off; one it cannot play jumps at once.
//! - **Moved in ticks** of a tenth of a second against the ground, the solid objects and the
//!   interior cells around it ([`Player::tick`]); its collisions go to its scripts and the client
//!   is told where it is, the motion it was asked and its heading, and reports them to the server
//!   by its own rule ([`Player::report`]).
//! - **Its figure animates** with it ([`Player::animate`]): told how the body moved over its last
//!   tick, the behaviors its motion started or stopped, and the ground it stands on; a jump its
//!   lead-in's animation sets off is jumped and reported.

use std::collections::BTreeSet;

use auberean_animation::body::{Body as Figure, AIR, EARTH};
use auberean_animation::host::{Completion, Event};
use auberean_animation::look::{Look, ObjectLook};
use auberean_animation::turns::Ground as Standing;
use auberean_animation::CachedRecords;
use auberean_assets::animation::{MotionDesc, MotionParams};
use auberean_client_contract::control::{movement, ControlId};
use auberean_client_contract::{Action, Motion};
use auberean_client_model::Object;
use auberean_client_runtime::physics_object::BodyRequest;
use auberean_client_runtime::Body as ClientBody;
use auberean_client_runtime::Client;
use auberean_physics::body::missile::ObjectTable;
use auberean_physics::body::placing::error as place_error;
use auberean_physics::body::{setup_spheres, Body, BodyConfig, JUMP_BEHAVIOR, TICK};
use auberean_physics::collision::manager::BodyKey;
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use auberean_physics::world::{NoGameObjects, World};
use auberean_primitives::Vec3;
use auberean_protocol::types::{PhysicsDesc, VisualDesc, WeenieDesc};
use auberean_world::records::WorldRecords;
use dereth_primitives::{CellId, Frame as PFrame, Position};

use crate::ground::Ground;
use crate::{figures, is_interior, profile, WorldData};

/// The motion description bodies of the player's kind move by, when the object names none.
pub const PLAYER_MOTION: u32 = 0x3200_0000;

/// The player's body: its physics, its figure, and what it last took from the client. `F` is what
/// a front end keeps with it (a window's: the effects it plays).
// D2-09633, D2-09634, D2-09647
pub struct Player<F = ()> {
    pub id: u64,
    /// The description it was last sent, and its own descriptions (none when its look is a stored
    /// record's).
    pub visual: VisualDesc,
    /// The object's count of descriptions sent, at the last one taken.
    pub visual_updates: u32,
    pub dress: Option<ObjectLook>,
    pub body: Body,
    pub figure: Figure,
    pub placements: u32,
    /// How many times the server's throws of it have been taken.
    pub thrown: u32,
    /// How many times the server's forcing of its place has been taken.
    pub forced: u32,
    pub height: f32,
    /// Its setup's radius.
    pub radius: f32,
    pub scale: Vec3,
    /// Just put somewhere by the server: stood on the ground once the ground there is loaded.
    pub settle: bool,
    /// Made, or teleported, and not yet in the world.
    pub arriving: bool,
    /// The last teleport of the server's it has taken.
    pub teleport_stamp: u16,
    /// Arriving at a place with no room for it: out of the world, and trying again each frame.
    pub no_room: bool,
    /// The behaviors its figure was last told the body shows.
    pub shown: BTreeSet<u32>,
    /// What the front end keeps with it.
    pub front: F,
    /// The motion its interpreter was asked at its last tick (sideways, forward, up): what its
    /// reports carry while a move the server ordered steers it.
    pub ticked: dereth_primitives::Vec3,
    /// The physics record its physics object was made from, as the client named it.
    pub record: u32,
    /// A behavior that locks the player's actions is playing: what its keys ask of the body waits.
    pub actions_locked: bool,
}

impl<F> core::fmt::Debug for Player<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Player")
            .field("id", &format_args!("{:#x}", self.id))
            .field("position", &self.body.position)
            .field("heading", &self.body.heading)
            .finish_non_exhaustive()
    }
}

/// What the player asks of its body this frame: the movement its controls sum to, a jump, and the
/// camera's part (the heading the mouse steers to, and whether the mouse steers at all).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steer {
    /// Ahead positive, -1 to 1.
    pub forward: f32,
    /// Right positive, -1 to 1.
    pub sideways: f32,
    /// Left positive, -1 to 1.
    pub turn: f32,
    /// Jump this frame.
    pub jump: bool,
    /// The heading the camera steers the body to (mouse look steering), if it does this frame.
    pub heading: Option<f32>,
    /// The camera steers: the keys' turn is left out.
    pub camera_steers: bool,
}

/// What the figure did this frame beyond its animation: whether the player stopped looking at
/// what it looked at, what it finished, the kinds of the hooks it fired, and what it asked.
#[derive(Debug, Default)]
pub struct Animated {
    pub stop_looking: bool,
    pub completions: Vec<Completion>,
    pub hooks: Vec<u32>,
    pub events: Vec<Event>,
}

impl<F> Player<F> {
    /// The player's object `o` made where the client placed its body (`b`): its body, its figure,
    /// and `front`'s part made from its look; its entry in the ground's table takes the properties
    /// of the physics record `record` the client named. None when its look or its setup does not
    /// read.
    // D2-10663, D2-10661
    pub fn make(
        world: &WorldData,
        anims: &CachedRecords<'_>,
        ground: &mut Ground,
        o: &Object,
        record: u32,
        b: &ClientBody,
        front: impl FnOnce(&Look) -> F,
    ) -> Option<Self> {
        let records = &world.records;
        let mut dress = None;
        let look = figures::look_after(records, anims, &o.visual, &mut dress)?;
        let mut figure = Figure::from_look(anims, &look)?;
        // What its behaviors ask only of the player (the action lock, the camera) is asked.
        figure.is_player = true;
        let scale = look.scale;
        let (config, height) = body_config(records, &look, &figure, &o.physics)?;
        let start = Position::new(
            CellId(b.cell),
            PFrame::new(pv(b.origin), dereth_primitives::Quat::IDENTITY),
        );
        let mut body = Body::new(config, start, b.heading);
        give_parts(&mut body, records, &mut ground.trees, &look);
        body.set_mode(figure.mode);
        profile::init_physics(records, o, &mut body);
        let p = &o.physics;
        if let Some(k) = velocity_scale(p) {
            body.minterp.velocity_scale = k;
        }
        if p.bits & PhysicsDesc::JUMP_SCALE != 0 && p.jump_scale > 0.0 {
            body.jump_scale = p.jump_scale;
        }
        // The player's body in the world's table, under its own handle.
        if let Some(k) = ground.table.find(o.id) {
            ground.table.remove(k);
        }
        let key = ground.table.insert_body(&ground.land, o.id, &mut body);
        ground.table.set_ethereality(key, ethereality(&o.weenie));
        apply_description(records, &mut ground.table, key, record);
        let radius = figure.parts.first().map_or(0.0, |p| p.setup.radius);
        Some(Self {
            id: o.id,
            visual: o.visual.clone(),
            visual_updates: o.visual_updates,
            dress,
            front: front(&look),
            body,
            figure,
            placements: b.placements(),
            thrown: b.thrown().0,
            forced: b.forced_moves(),
            height,
            radius,
            scale,
            settle: true,
            arriving: true,
            teleport_stamp: b.teleport_stamp,
            no_room: false,
            shown: BTreeSet::new(),
            ticked: dereth_primitives::Vec3::ZERO,
            record,
            actions_locked: false,
        })
    }

    /// The player's object was sent a new description: its own descriptions take it (the
    /// appearance its effects lay kept over it), and its figure (keeping what it plays) and body's
    /// parts are brought up to date with the total; its body stays where it is. Answers the total
    /// look when the figure took it, for what the front end keeps of it.
    // D2-08142
    pub fn take_visual(
        &mut self,
        records: &WorldRecords,
        anims: &CachedRecords<'_>,
        trees: &mut Trees,
        o: &Object,
    ) -> Option<Look> {
        self.visual = o.visual.clone();
        self.visual_updates = o.visual_updates;
        let look = figures::look_after(records, anims, &o.visual, &mut self.dress)?;
        if !self.figure.compare_and_modify(anims, &look) {
            return None;
        }
        self.scale = look.scale;
        body_takes_look(&mut self.body, records, trees, &look);
        Some(look)
    }

    /// The server's word on the player's place, as the client keeps it (`b`): a forced move taken
    /// (answered, for the camera to follow); a placement puts the body there at rest, to be stood
    /// on the ground once the cells there are in, and a teleport has it arrive again, its motion
    /// having left the world on the way, as the client takes the player out of the world for a
    /// teleport: the move it was making on its own (walking up to what it attacked) ends, and it
    /// arrives standing. Answers whether the server forced the player elsewhere, and whether it
    /// placed it.
    // D2-10665
    pub fn take_server_place(&mut self, b: &ClientBody) -> (bool, bool) {
        let forced = b.forced_moves() != self.forced;
        self.forced = b.forced_moves();
        if b.placements() == self.placements {
            return (forced, false);
        }
        self.placements = b.placements();
        self.body.place(
            Position::new(
                CellId(b.cell),
                PFrame::new(
                    pv(b.origin),
                    auberean_physics::body::heading_rotation(b.heading),
                ),
            ),
            b.heading,
        );
        self.body.velocity = dereth_primitives::Vec3::ZERO;
        self.body.contact = true;
        self.body.on_walkable = true;
        self.body.contact_plane = None;
        self.settle = true;
        if b.teleport_stamp != self.teleport_stamp {
            self.teleport_stamp = b.teleport_stamp;
            self.arriving = true;
            self.body.motion_leaves_world();
        }
        (forced, true)
    }

    /// A place under the ground outdoors is lifted onto it, as a placement is; then the body
    /// stands where there is room for it nearest that place, as the client puts a body the server
    /// placed (beside the start hall's fir it rises 0.69 m and steps 0.45 m aside): once the cells
    /// it was put in are in (`cells_in`). A body arriving where there is no room stays out of the
    /// world, and tries again next frame. Answers whether it was stood somewhere.
    // D2-10657, D2-06059
    pub fn settle(&mut self, ground: &mut Ground, cells_in: bool) -> bool {
        if !cells_in || !core::mem::take(&mut self.settle) {
            return false;
        }
        let b = &mut self.body;
        if !is_interior(b.position.cell.0) {
            let at = b.position.frame.origin;
            if let Some((z, _)) = ground.land.ground(b.position.cell.landblock(), at) {
                if z > at.z {
                    let mut lifted = b.position;
                    lifted.frame.origin.z = z;
                    b.place(lifted, b.heading);
                }
            }
        }
        let at = b.position;
        let r = b.set_position_simple(&ground.land, &mut ground.table, at, true);
        self.no_room =
            self.arriving && matches!(r, place_error::NO_VALID_POSITION | place_error::COLLIDED);
        self.settle = self.no_room;
        if self.no_room {
            return false;
        }
        self.arriving = false;
        self.body.set_placement_position(at);
        true
    }

    /// A throw of the server's, once the body is not waiting to be stood somewhere: it comes after
    /// the place it came with.
    pub fn take_throw(&mut self, b: &ClientBody) {
        if self.settle {
            return;
        }
        let (count, velocity) = b.thrown();
        if count != self.thrown {
            self.thrown = count;
            auberean_physics::motion::Owner::apply_impulse(&mut self.body, pv(velocity));
        }
    }

    /// The player's controls steer the body this frame (`steer`), as the client froze it and the
    /// behaviors playing lock its actions; a jump asked is set off and reported. Answers the
    /// movement the controls asked, which [`Self::report`] tells the client.
    // D2-09649
    pub fn steer(
        &mut self,
        client: &mut Client,
        anims: &CachedRecords<'_>,
        steer: &Steer,
    ) -> MotionInput {
        let frozen = client.body.frozen;
        let locked = self.actions_locked;
        let input = if frozen || locked {
            MotionInput::default()
        } else {
            MotionInput {
                sideways: steer.sideways,
                forward: steer.forward,
                turn: steer.turn,
            }
        };
        // Mouse look steering sets the heading from the camera's direction and leaves no turn to
        // the keys.
        if !frozen && !locked {
            if let Some(heading) = steer.heading {
                let by = (heading - self.body.heading + 540.0).rem_euclid(360.0) - 180.0;
                if by != 0.0 {
                    self.body.turn_at_once(by);
                }
            }
        }
        if !locked {
            self.body.steer(if steer.camera_steers {
                MotionInput { turn: 0.0, ..input }
            } else {
                input
            });
        }
        // The jump asked: standing, or moving slowly, the figure plays the jump's lead-in, whose
        // animation sets the jump off; one it cannot play, or moving faster, jumps at once.
        let jumps = self.body.jumps;
        if steer.jump
            && !frozen
            && !locked
            && self.body.jump_logic()
            && self.body.behaviors.contains(&JUMP_BEHAVIOR)
        {
            if self.figure.do_autonomous_behavior(anims, JUMP_BEHAVIOR) {
                self.shown.insert(JUMP_BEHAVIOR);
            } else {
                self.body.behaviors.remove(&JUMP_BEHAVIOR);
                self.body.jump();
            }
        }
        self.report_jump(client, jumps);
        input
    }

    /// The body moved `ticks` ticks against the ground at the game's time `game_time`, with the
    /// other bodies as `objects` sees them; after each tick, `each` (where a window places its
    /// camera). Each tick the body moves while its movement runs, then its other updates run while
    /// woken: its detections checked, what its collisions began and ended reported. A body with no
    /// room where it arrived does not move.
    pub fn tick(
        &mut self,
        ground: &mut Ground,
        objects: &mut ObjectTable,
        game_time: f64,
        ticks: u32,
        mut each: impl FnMut(&Self, &mut Ground),
    ) {
        #[allow(clippy::cast_possible_truncation)]
        let tick = TICK as f32;
        ground.table.set_now(game_time);
        ground.table.file_those_waiting(&ground.land);
        for _ in 0..if self.no_room { 0 } else { ticks } {
            self.body.update_in(
                &ground.land,
                &mut ground.table,
                objects,
                &NoGameObjects,
                tick,
            );
            self.ticked = self.body.minterp.asked;
            each(self, ground);
        }
    }

    /// The frame's movement told: the body's collisions to its scripts; the client told the
    /// motion the interpreter was asked (a diagonal cut to length 1; the move the server ordered's
    /// while it steers the body), whether the controls asked any (`input`), its heading, where it
    /// stands and where it is turning to, which the client reports to the server by its own rule.
    pub fn report(&mut self, client: &mut Client, ground: &mut Ground, input: &MotionInput) {
        if let Some(k) = self.body.key() {
            for report in ground.table.take_reports(k) {
                use auberean_physics::pack::Pack;
                let mut w = auberean_physics::pack::Writer::new();
                report.pack(&mut w);
                let _ = client.handle_collision(self.id, w.finish());
            }
        }
        let active = input.forward != 0.0 || input.sideways != 0.0 || input.turn != 0.0;
        client.body.driven = true;
        let asked = if self.body.is_moving_to() {
            self.ticked
        } else {
            self.body.minterp.asked
        };
        client.act(Action::Move(Motion {
            forward: asked.y,
            sideways: asked.x,
            heading: self.body.heading,
            active,
        }));
        let pos = self.body.position;
        client
            .body
            .follow(pos.cell.0, dv(pos.frame.origin), self.body.heading);
        client.body.turning_to(
            self.body
                .minterp
                .turning_to
                .then(|| self.body.minterp.turn_to.degrees()),
        );
    }

    /// One animation step of `step` seconds when one is due (and the body's animation is not
    /// frozen): the figure told how the body moved over its last tick, the behaviors its motion
    /// started or stopped and the ground it stands on; `look` turns its head (answering whether the
    /// player stops looking at what it looked at), then the figure advances and the body stands in
    /// the element the figure's mode says. Then, due or not, what the figure finished, the hooks it
    /// fired and what it asked are handed back, and a jump its lead-in's animation set off is
    /// jumped and reported; the lead-in over, the body no longer shows it.
    pub fn animate(
        &mut self,
        client: &mut Client,
        anims: &CachedRecords<'_>,
        step: Option<f32>,
        look: impl FnOnce(&Body, &mut Figure, f32) -> bool,
    ) -> Animated {
        let mut out = Animated::default();
        if let Some(step) = step.filter(|_| !self.body.is_animation_frozen()) {
            let (moved, turn, size) = figure_motion(&mut self.body, step);
            self.figure.set_motion(moved, turn, size);
            forward_behaviors(&self.body, &mut self.shown, &mut self.figure, anims);
            self.figure.ground = ground_of(&self.body);
            out.stop_looking = look(&self.body, &mut self.figure, step);
            self.figure.advance(anims, step);
            self.body.figure_stands_in(self.figure.modes.element());
        }
        // What the figure finished and the hooks it fired go to its story; what it finished goes
        // nowhere else (the client's scripts make no waiting calls).
        let (hooks, impulses) = take_triggers(&mut self.figure);
        out.hooks = hooks;
        out.completions = self.figure.take_completions();
        out.events = self.figure.take_events();
        let jumps = self.body.jumps;
        for i in impulses {
            if i.jump {
                self.body.jump();
            }
        }
        if self.body.behaviors.contains(&JUMP_BEHAVIOR)
            && !self.figure.is_doing_behavior(JUMP_BEHAVIOR)
        {
            self.body.behaviors.remove(&JUMP_BEHAVIOR);
            self.shown.remove(&JUMP_BEHAVIOR);
        }
        self.report_jump(client, jumps);
        out
    }

    /// A jump the body made since it had made `jumps`, told to the client with the velocity it left
    /// the ground at.
    fn report_jump(&self, client: &mut Client, jumps: u32) {
        if self.body.jumps != jumps {
            let v = self.body.velocity;
            client.act(Action::Jump {
                velocity: Vec3::new(v.x, v.y, v.z),
            });
        }
    }
}

/// The movement controls the player began this frame (`started`): each interrupts the move the
/// player's body is making on its own (walking up to what it attacks or uses), and a turn also
/// stops an attack the scripts started while it moved so.
// D2-09645, D2-09646
pub fn start_movement_actions<F>(
    mut player: Option<&mut Player<F>>,
    client: &mut Client,
    started: &[ControlId],
) {
    for action in started {
        if let Some(p) = player.as_deref_mut() {
            p.body.interrupt_move_to();
        }
        if matches!(*action, movement::TURN_LEFT | movement::TURN_RIGHT) {
            client.stop_attacking_if_auto_moving();
        }
    }
}

/// The object a script's request for a body is for.
#[must_use]
pub const fn request_object(r: &BodyRequest) -> u64 {
    match *r {
        BodyRequest::VelocityScale { object, .. }
        | BodyRequest::AccelerationScale { object, .. }
        | BodyRequest::Cloaked { object, .. }
        | BodyRequest::ReportCollisions { object, .. }
        | BodyRequest::IgnoreCollisions { object, .. }
        | BodyRequest::LookAt { object, .. }
        | BodyRequest::Impulse { object, .. }
        | BodyRequest::TargetedImpulse { object, .. }
        | BodyRequest::EndLayer { object, .. } => object,
    }
}

/// A script's request carried out on body `b` as its physics object does: a speed or
/// acceleration scale; drawing it cloaked (which also stops it being drawn); reporting or ignoring
/// collisions; a push in the world's frame or its own, which puts it in the air at that velocity; a
/// push away from another object, which stands at `from` (none when that object has no place here,
/// or is the body itself: no push); stopping a layer of its animation (its figure stops it this
/// frame). Turning to look at an object is the front end's (its head), and is not carried out here.
// D2-06067, D2-06068, D2-06018, D2-06249, D2-06014, D2-06124, D2-05879, D2-05880, D2-06125
// D2-05890
pub fn carry_out_request(b: &mut Body, r: &BodyRequest, from: Option<Position>) {
    match *r {
        BodyRequest::VelocityScale { scale, .. } => b.set_velocity_scale(scale),
        BodyRequest::AccelerationScale { scale, .. } => b.set_acceleration_scale(scale),
        BodyRequest::Cloaked { on, .. } => b.set_cloaked(on),
        BodyRequest::ReportCollisions { on, .. } => b.set_report_collisions(on),
        BodyRequest::IgnoreCollisions { on, .. } => b.set_ignore_collisions(on),
        BodyRequest::Impulse { vector, local, .. } => {
            let v = dereth_primitives::Vec3::new(vector[0], vector[1], vector[2]);
            if local {
                b.add_impulse_local(v);
            } else {
                b.add_impulse(v);
            }
        }
        BodyRequest::TargetedImpulse {
            target,
            speed,
            flags,
            extra,
            ..
        } => {
            let Some(at) = from else { return };
            // The body measures the push from where the object stands now; what it kept of that
            // object for a move stays as it was.
            let kept = b.targets.insert(
                target,
                auberean_physics::motion::moveto::TargetBody {
                    position: at,
                    radius: 0.0,
                    height: 0.0,
                },
            );
            b.add_targeted_impulse(target, speed, flags != 0, extra);
            match kept {
                Some(k) => {
                    b.targets.insert(target, k);
                }
                None => {
                    b.targets.remove(&target);
                }
            }
        }
        BodyRequest::EndLayer { layer, .. } => b.stop_layer(layer),
        BodyRequest::LookAt { .. } => {}
    }
}

/// Whether object `id`'s game object is a player or a creature, what a missile that goes off on a
/// miss goes off on. An object the client knows nothing of is neither, and neither is any without
/// the client's scripts, whose records say what an object is.
pub fn is_player_or_creature(client: &mut Client, id: u64) -> bool {
    use auberean_client_runtime::repository::{is_creature, is_player};
    client
        .with_host(|h| is_player(h, id) || is_creature(h, id))
        .unwrap_or(false)
}

/// What a body says of itself when the scripts ask, read for them after the frame's ticks.
#[must_use]
pub fn reading_of(b: &Body, f: Option<&Figure>) -> auberean_client_runtime::body::BodyReading {
    let m = b.minterp.asked;
    let mode = f.map_or(0, |f| f.modes.desired_mode);
    auberean_client_runtime::body::BodyReading {
        element: b.element(),
        last_motion: [m.x, m.y, m.z],
        velocity_scale: b.velocity_scale(),
        acceleration_scale: b.acceleration_scale(),
        jump_scale: b.jump_scale(),
        radius: b.radius(),
        height: b.height(),
        mode,
        desired_mode: mode,
        remote_motion: b.has_remote_do_motion_values().then(|| {
            let v = b.remote_do_motion_values();
            [v.x, v.y, v.z]
        }),
        place: (b.position.cell.0 != 0).then(|| {
            let f = b.position.frame;
            let (o, q) = (f.origin, f.rotation);
            (b.position.cell.0, [o.x, o.y, o.z], [q.w, q.x, q.y, q.z])
        }),
    }
}

/// Body `b` of object `id` (its figure `f`, if it has one) noted for the scripts: what it says of
/// itself ([`reading_of`]), and the material it touches, read again only when it changes cell,
/// which is when it changes.
pub fn note_reading(
    readings: &mut auberean_client_runtime::body::Readings,
    materials: &mut auberean_client_runtime::body::Materials,
    id: u64,
    b: &Body,
    f: Option<&Figure>,
) {
    readings.insert(id, reading_of(b, f));
    if let (Some(cell), Some(properties)) = (b.material_cell(), b.current_properties()) {
        if materials.get(&id).is_none_or(|m| m.cell != cell.0) {
            let properties = properties.clone();
            materials.insert(
                id,
                auberean_client_runtime::body::BodyMaterial {
                    cell: cell.0,
                    properties,
                },
            );
        }
    }
}

/// What a body's figure is told of it for an animation step of `dt` seconds: how fast the body
/// moved over its last tick, in its own frame (x to its right, y ahead, z up), with its
/// acceleration's share of the step; its rate of turn; and its size (its look's times its own).
/// Not the velocity the body asks for: a body held against a wall or at a ledge stands while it
/// still asks to run, and a step up or down shows in its vertical speed for the tick it took.
pub fn figure_motion(body: &mut Body, dt: f32) -> (Vec3, f32, Vec3) {
    body.update_animation(dt);
    (
        dv(body.anim_velocity),
        body.turn_rate,
        dv(body.total_scale(None)),
    )
}

/// Where a body stands, as its figure's slope lean reads it: in touch or not, sliding or not,
/// and the plane it stands on turned into its own frame.
#[must_use]
pub fn ground_of(body: &Body) -> Standing {
    let normal = body.contact_plane.map_or(Vec3::ZERO, |(plane, _)| {
        dv(auberean_physics::geom::global_to_local(
            &body.position.frame,
            plane.normal,
        ))
    });
    Standing {
        contact: body.contact,
        sliding: body.is_sliding_down(),
        normal,
        placed: true,
    }
}

/// The behaviors the body's motion interpreter started or stopped over the frame's ticks, played
/// or stopped on its figure, as the client hands its physics object's behaviors to its animation.
pub fn forward_behaviors(
    body: &Body,
    shown: &mut BTreeSet<u32>,
    figure: &mut Figure,
    anims: &CachedRecords<'_>,
) {
    let started: Vec<u32> = body.behaviors.difference(shown).copied().collect();
    let stopped: Vec<u32> = shown.difference(&body.behaviors).copied().collect();
    for b in started {
        figure.do_autonomous_behavior(anims, b);
    }
    for b in stopped {
        figure.stop_behavior(anims, b);
    }
    shown.clone_from(&body.behaviors);
}

/// What a figure's animations set off this frame, played as the client plays a body's trigger
/// update after its animation: the effects played once and those starting are asked of the body
/// (or stopped, for a stopping trigger) among the figure's events; the kinds of its hooks, in
/// order, and its impulses are handed back; the effects stopping are stopped; then the update is
/// cleaned up, what started counting as running from then on.
pub fn take_triggers(figure: &mut Figure) -> (Vec<u32>, Vec<auberean_assets::animation::Impulse>) {
    let fx = |t: &auberean_animation::triggers::Trigger| {
        if t.stop {
            Event::StopFx { fx: t.effect }
        } else {
            Event::DoFx {
                fx: t.effect,
                intensity: t.intensity,
            }
        }
    };
    let t = &mut figure.triggers;
    let mut events: Vec<Event> = t.once.iter().map(|(_, tr)| fx(tr)).collect();
    t.once.clear();
    events.extend(
        t.starting
            .iter()
            .filter(|(_, tr)| tr.play)
            .map(|(_, tr)| fx(tr)),
    );
    let hooks = core::mem::take(&mut t.hooks)
        .into_iter()
        .map(|h| h.kind)
        .collect();
    let impulses = core::mem::take(&mut t.impulses);
    events.extend(t.stopping.iter().map(|(k, _)| Event::StopFx { fx: k }));
    t.clean_up();
    figure.events.extend(events);
    (hooks, impulses)
}

/// The motion values a motion description gives a body in `element` and `mode`.
#[must_use]
pub fn motion_values(md: &MotionDesc, element: u32, mode: u32) -> Option<MotionParams> {
    let (_, set) = md
        .set_of
        .iter()
        .find(|(k, _)| k.element == element && k.mode == mode)?;
    md.sets.get(*set as usize).map(|s| s.default)
}

/// What a body of `look` put together as `figure` is for physics: the collision spheres and step
/// heights of its setup, its scale, and its motion description's values on the ground and in the
/// air in the figure's mode, the defaults when it lists none for that mode (a building's mode is
/// its own); and its setup's height.
#[must_use]
pub fn body_config(
    records: &WorldRecords,
    look: &Look,
    figure: &Figure,
    _physics: &PhysicsDesc,
) -> Option<(BodyConfig, f32)> {
    let setup = figure.parts.first()?.setup.clone();
    let motion = if look.motion_desc.is_none() {
        PLAYER_MOTION
    } else {
        look.motion_desc.0
    };
    let md: Option<MotionDesc> = records.get(motion);
    let values = |element| {
        md.as_ref()
            .and_then(|md| motion_values(md, element, figure.mode))
    };
    let ground = values(EARTH).unwrap_or_else(auberean_physics::motion::values::default_values);
    let air = values(AIR).unwrap_or(ground);
    Some((
        BodyConfig {
            spheres: setup_spheres(&setup),
            step_up: setup.step_up_height,
            step_down: setup.step_down_height,
            scale: look.scale.z.max(0.01),
            ground,
            air,
        },
        setup.height,
    ))
}

/// `body` takes the part group `look` puts together (each mesh read once through `trees`) and ends
/// its making with it, the scale its look gives it and the scale its look gives what it holds; and
/// the whole of the motion description its look names, every element's and mode's values: water's,
/// the sea floor's and each kind of ground's as well as the earth's and the air's.
// D2-06230, D2-06411, D2-06354, D2-06355, D2-06367, D2-06370, D2-06177, D2-05837, D2-06553, D2-06554
// D2-06555
pub fn give_parts(body: &mut Body, records: &WorldRecords, trees: &mut Trees, look: &Look) {
    let motion = if look.motion_desc.is_none() {
        PLAYER_MOTION
    } else {
        look.motion_desc.0
    };
    if let Some(md) = records.get::<MotionDesc>(motion) {
        body.minterp
            .set_description(motion, std::sync::Arc::new(md));
    }
    let mut parts = auberean_physics::parts::PartGroup::default();
    if parts.init_look(
        &auberean_physics::parts::CachedParts::new(records, trees),
        look,
    ) {
        body.set_parts(parts);
        body.init_object_end();
    }
    body.look_scale = pv(look.scale);
    body.child_scale = look.child_scale;
}

/// `body` brought up to date with its object's changed look: its part group compared with the
/// look's groups and changed where they differ (each mesh read once through `trees`), the scale
/// its look gives it and what it holds, and the end of its making again.
// D2-06551
pub fn body_takes_look(body: &mut Body, records: &WorldRecords, trees: &mut Trees, look: &Look) {
    body.update_parts_to_look(
        &auberean_physics::parts::CachedParts::new(records, trees),
        look,
    );
    body.look_scale = pv(look.scale);
    body.child_scale = look.child_scale;
    body.init_object_end();
}

/// The speed scale the server gave an object's body, if any.
#[must_use]
pub fn velocity_scale(p: &PhysicsDesc) -> Option<f32> {
    (p.bits & PhysicsDesc::VELOCITY_SCALE != 0 && p.velocity_scale > 0.0)
        .then_some(p.velocity_scale)
}

/// What an object's description says it is and passes through, as the object's script applies it
/// when the object is made: each mask from its two halves, or nothing when either is missing.
#[must_use]
pub fn ethereality(w: &WeenieDesc) -> auberean_physics::solid::Ethereality {
    let number = |bit: u32| {
        WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .filter(|_| w.mask & bit != 0)
            .map(|i| w.numbers[i])
    };
    auberean_physics::solid::Ethereality::of_numbers(number)
}

/// Entry `key` takes the properties of the entity record `desc` its object is made from (an
/// object made from no entity takes none).
pub fn apply_description(records: &WorldRecords, world: &mut World, key: BodyKey, desc: u32) {
    use auberean_world::place::PlaceRecords;
    if desc == 0 {
        return;
    }
    let lookup = |id: u32| records.entity(auberean_primitives::DataId(id));
    world.apply_entity_properties(key, desc, &lookup);
}

/// A physics vector as the client's.
#[must_use]
pub fn dv(v: dereth_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// The client's vector as physics takes it.
#[must_use]
pub fn pv(v: Vec3) -> dereth_primitives::Vec3 {
    dereth_primitives::Vec3::new(v.x, v.y, v.z)
}
