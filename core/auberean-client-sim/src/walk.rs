//! The world walked through with no window: the ground, the player's body and the bodies of
//! everything else the client knows put together, one frame at a time, for a program that drives
//! the game by its controls (a headless client, a plugin's host, a test).
//!
//! Each frame ([`Walk::update`]) runs the bodies' part of the window's scene, in its order: the
//! cells asked and heard; the player's body made when the client has the player's object and a
//! place for it (through the client, which asks for its physics object), its look kept up to date,
//! and let go of with the world when the player goes; the server's word on its place taken, the
//! cells about it followed and the body stood on the ground once they are in; a throw taken; what
//! the server asked of the bodies played on them ([`crate::events`]: the moves it orders the
//! player's body to make, its pushes, its scales); then the frame's controls steer the body,
//! physics moves it in ticks of a tenth of a second (none while the cells the server put it in are
//! asked for) against the ground and the other bodies, the client is told where it is and what was
//! asked of it, and its figure animates. After the ticks the cells follow the body again, the
//! client hears whether the world waits for its cells, and everything else is run
//! ([`crate::others`]): the physics objects the client asks for made, the positions the server sent
//! for them taken, each run through the frame's ticks, and what the server asked of a body made
//! just now played on it. Then the scripts' freezing and requests for the player's body are carried
//! out, what it says of itself is noted for the scripts, the client is told the cell each body
//! stands in, which left the world or entered it again and which stand in it, and what the bodies
//! asked of each other and of their figures is handed on ([`crate::notices`]).
//!
//! The same controls on the same clock move the body exactly as the window's scene moves it,
//! which runs the same steps among its own.
//!
//! With the orbit camera chosen ([`Walk::set_scheme`]), the frame's controls steer the body
//! through it, as the window's scene does: camera- or character-based ([`crate::orbit`]). With no
//! mouse here, the camera turns by the camera's rotate and tilt controls, zooms by the zoom
//! controls, and is otherwise turned by whoever drives the walk ([`Walk::orbit_mut`]).
//!
//! Not here yet: what the bodies hold and the objects the client makes itself (the stories'
//! missiles), which the scene keeps; what the server asks of the bodies' figures (their behaviors
//! and modes), their effects and their stories, which go; the scripts' freezing and requests for
//! the other bodies; and behaviors the controls ask the player's figure to play.

use std::collections::BTreeMap;

use auberean_animation::CachedRecords;
use auberean_client_contract::control::ControlFrame;
use auberean_client_model::Object;
use auberean_client_runtime::world::{self, PhysicsObjOrder, WorldScene};
use auberean_client_runtime::Body as ClientBody;
use auberean_client_runtime::Client;
use auberean_physics::body::missile::ObjectTable;
use auberean_physics::body::{AnimationClock, Ticker};
use auberean_protocol::ServerMessage;

use crate::ground::Ground;
use crate::orbit::{OrbitCamera, OrbitInput, OrbitSettings};
use crate::others::{self, Looks, Others};
use crate::player::{self, Player, Steer};
use crate::{events, notices, WorldData};

/// The interface's function the player's scripts preload what they keep about the player with.
pub const PRELOAD_SINGLETONS: u32 = (0x7F << 16) | 0x5C;

/// The player's body walked through the world with no window (see the module documentation).
pub struct Walk<'w> {
    world: &'w WorldData,
    anims: CachedRecords<'w>,
    /// The world's physics about the player.
    pub ground: Ground,
    player: Option<Player>,
    /// When physics ticks.
    ticker: Ticker,
    /// When the figure animates: once at least a thirtieth of a second has passed.
    anim_clock: AnimationClock,
    /// The bodies as the player's guidance sees them, refreshed each frame.
    objects: ObjectTable,
    /// The bodies of everything else the client knows in the world.
    pub others: Others,
    /// The client's object whose physics object the client is asking for, and for the player, the
    /// body the client placed.
    creating: Option<(Object, Option<ClientBody>)>,
    /// The cell the client was last told the player stands in (0 for none).
    reported: BTreeMap<u64, u32>,
    /// The host's running time at the frame being run, seconds.
    clock: f64,
    /// The orbit camera, while one is chosen.
    orbit: Option<OrbitCamera>,
}

impl core::fmt::Debug for Walk<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Walk")
            .field("player", &self.player)
            .field("ground", &self.ground)
            .finish_non_exhaustive()
    }
}

impl WorldScene for Walk<'_> {
    fn make_physics_obj(&mut self, order: &PhysicsObjOrder) -> bool {
        let Some((o, b)) = self.creating.take().filter(|(o, _)| o.id == order.id) else {
            return false;
        };
        let Some(b) = b else {
            let looks = Looks {
                records: &self.world.records,
                anims: &self.anims,
                world: &mut self.ground.table,
                trees: &mut self.ground.trees,
            };
            return self.others.make_physics_obj(order, &o, looks);
        };
        self.player = Player::make(
            self.world,
            &self.anims,
            &mut self.ground,
            &o,
            order.physics_record,
            &b,
            |_| (),
        );
        self.player.is_some()
    }

    fn is_static(&self, id: u64) -> bool {
        match &self.player {
            Some(p) if p.id == id => p.body.is_static(),
            _ => self.others.is_static(id),
        }
    }

    fn has_parent(&self, _id: u64) -> bool {
        false
    }

    fn leave_visibility(&mut self, id: u64) {
        self.others
            .leave_visibility(id, false, &mut self.ground.table);
    }

    fn refresh_landscape(&mut self) -> bool {
        true
    }
}

impl<'w> Walk<'w> {
    /// Nothing walked yet, in `world`.
    #[must_use]
    pub fn new(world: &'w WorldData) -> Self {
        Self {
            world,
            anims: CachedRecords::new(&world.records),
            ground: Ground::new(world),
            player: None,
            ticker: Ticker::new(0.0),
            anim_clock: AnimationClock::new(0.0),
            objects: ObjectTable::default(),
            others: Others::default(),
            creating: None,
            reported: BTreeMap::new(),
            clock: 0.0,
            orbit: None,
        }
    }

    /// The orbit camera with these settings from now on, or with none the game's own steering. An
    /// orbit camera already chosen keeps its place and takes the settings.
    pub fn set_scheme(&mut self, settings: Option<OrbitSettings>) {
        match (settings, &mut self.orbit) {
            (None, _) => self.orbit = None,
            (Some(s), Some(o)) => o.apply_settings(s),
            (Some(s), None) => self.orbit = Some(OrbitCamera::new(s)),
        }
    }

    /// The orbit camera, while one is chosen.
    #[must_use]
    pub const fn orbit(&self) -> Option<&OrbitCamera> {
        self.orbit.as_ref()
    }

    /// The orbit camera, to turn it, while one is chosen.
    pub fn orbit_mut(&mut self) -> Option<&mut OrbitCamera> {
        self.orbit.as_mut()
    }

    /// The player's body, once made.
    #[must_use]
    pub fn player(&self) -> Option<&Player> {
        self.player.as_ref()
    }

    /// One frame at `now` on the host's running clock, seconds, with the controls in force
    /// `controls` (the client's control frame): see the module documentation.
    pub fn update(&mut self, client: &mut Client, controls: &ControlFrame, now: f64) {
        #[allow(clippy::cast_possible_truncation)] // a frame's seconds
        let dt = (now - self.clock).clamp(0.0, 0.25) as f32;
        self.clock = now;
        self.ground.attach_data_cache(client, self.world);
        self.ground.hear_cells(client);
        if client.world.take_region_change().is_some() {
            self.ground.near.notify_change_region();
        }
        self.take_player(client);
        if let Some(p) = &mut self.player {
            p.take_server_place(&client.body);
        }
        if let Some(placed) = self.player.as_ref().map(|p| p.settle) {
            self.follow(client, placed);
        }
        let cells_in = !self.ground.cells.pending;
        if let Some(p) = &mut self.player {
            // Stood somewhere new: the orbit camera is set behind it afresh.
            if p.settle(&mut self.ground, cells_in) {
                if let Some(o) = &mut self.orbit {
                    o.place_anew();
                }
            }
        }
        client.body.out_of_world = self.player.as_ref().is_none_or(|p| p.arriving);
        if let Some(p) = &mut self.player {
            p.take_throw(&client.body);
        }
        // What the server asked of the bodies since the last frame; a body made this frame hears it
        // once it is made.
        let events = core::mem::take(&mut client.physics);
        let waiting = self.play_server_events(events);
        // D2-10801
        let still = self.ground.cells.blocking;
        let (due, step) = (self.ticker.ticks(now), self.anim_clock.step(now));
        let ticks = if still { 0 } else { due };
        let anim = step.filter(|_| !still);
        self.objects.clear();
        if let Some(p) = &self.player {
            let asked = player::is_player_or_creature(client, p.id);
            self.objects.note(p.id, &p.body, asked);
        }
        self.others.note_objects(&mut self.objects, |id| {
            player::is_player_or_creature(client, id)
        });
        player::start_movement_actions(self.player.as_mut(), client, &controls.started);
        let others = &self.others;
        if let Some(p) = self.player.as_mut() {
            p.keep_move_target(|id| others.body(id));
        }
        if let Some(p) = &mut self.player {
            let mut steer = Steer {
                forward: controls.forward,
                sideways: controls.sideways,
                turn: controls.turn,
                jump: controls.jump,
                heading: None,
                camera_steers: false,
            };
            if let Some(o) = &mut self.orbit {
                let input = OrbitInput {
                    zoom: zoom_steps(controls),
                    ..OrbitInput::default()
                };
                o.step(controls.camera, &input, &steer, p.body.heading, dt);
                steer = o.steer(steer, &input, p.body.is_moving_to());
            }
            let input = p.steer(client, &self.anims, &steer);
            let game_time = client.game_time();
            p.tick(
                &mut self.ground,
                &mut self.objects,
                game_time,
                ticks,
                |_, _| {},
            );
            p.report(client, &mut self.ground, &input);
            p.animate(client, &self.anims, anim, |_, _, _| false);
        }
        if self.player.is_some() {
            self.follow(client, false);
        }
        client.world.blocking_for_cells = self.ground.cells.blocking;
        // Everything else: made through the client, its positions taken, and run through the
        // frame's ticks after the player's body, as the window's scene runs them.
        self.update_others(client, ticks, anim);
        // What the server asked of a body not here even now goes.
        drop(self.play_server_events(waiting));
        self.carry_out_scripts(client);
        self.note_reading(client);
        self.report_cell(client);
        let player = self.player.as_ref().map(|p| p.id);
        others::book_world_changes(client, player, self.others.take_world_changes());
        others::tell_lives(client, |id| match &self.player {
            Some(p) if p.id == id => p.body.in_world,
            _ => self.others.stands(id),
        });
        self.route_notices();
    }

    /// The bodies of everything else the client knows: the physics objects the client asks for
    /// made, the model followed (their positions taken, every body in range run through the
    /// frame's `ticks` and its figure animated by `anim`), and each body the server described
    /// brought up to its description. What their figures finished and asked, and the effects of
    /// those that went, are a window's to play: here they go.
    fn update_others(&mut self, client: &mut Client, ticks: u32, anim: Option<f32>) {
        self.create_physics_objs(client);
        let player = self.player.as_ref().map(|p| (p.id, p.body.position));
        let game_now = client.game_time();
        self.others.update(
            &self.world.records,
            &self.anims,
            &mut client.model,
            &self.ground.land,
            &mut self.ground.table,
            &mut self.ground.trees,
            player,
            &mut self.objects,
            ticks,
            game_now,
            anim,
            &mut |_, _, _, _| {},
        );
        for id in self.others.take_described() {
            let described = client.model.object(id).map(|o| o.physics.clone());
            if let (Some(c), Some(b)) = (described, self.others.body_mut(id)) {
                others::describe_body(b, &c);
            }
        }
        drop(self.others.take_heard());
        drop(self.others.take_figure_events());
        drop(self.others.take_parted());
    }

    /// What the server asked of the bodies, played in order on those here ([`events`]): the moves it
    /// orders the player's body to make, its pushes, the missiles it launches, and what each body
    /// takes by itself. What is for a body not here yet comes back, to be played once it is made.
    /// What it asks of the figures, their effects and their stories is a window's to play: here it
    /// goes.
    fn play_server_events(&mut self, asked: Vec<ServerMessage>) -> Vec<ServerMessage> {
        let mut waiting = Vec::new();
        for m in asked {
            if matches!(m, ServerMessage::PrivateFx(_)) {
                continue;
            }
            let Some(id) = auberean_client_model::events::subject(&m).map(|s| s.id) else {
                continue;
            };
            let here =
                self.player.as_ref().is_some_and(|p| p.id == id) || self.others.body(id).is_some();
            if !here {
                waiting.push(m);
                continue;
            }
            match &m {
                ServerMessage::MoveTo(e) => {
                    if let Some(p) = self.player.as_mut().filter(|p| p.id == id) {
                        p.receive_move_to(&self.anims, &e.action);
                    }
                }
                ServerMessage::PushToward(e) => {
                    let at = match &self.player {
                        Some(p) if p.id == e.action.target => Some(p.body.position),
                        _ => self.others.body(e.action.target).map(|b| b.position),
                    };
                    let b = body_mut(&mut self.player, &mut self.others, id);
                    if let (Some(at), Some(b)) = (at, b) {
                        events::push_toward(b, at, &e.action);
                    }
                }
                ServerMessage::InitializeMissile(e) => {
                    if let Some(b) = body_mut(&mut self.player, &mut self.others, id) {
                        let (land, table) = (&self.ground.land, &mut self.ground.table);
                        events::initialize_missile(b, land, table, &e.action);
                    }
                }
                m => {
                    if let Some(b) = body_mut(&mut self.player, &mut self.others, id) {
                        events::play_on_body(&self.world.records, b, m);
                    }
                }
            }
        }
        waiting
    }

    /// The physics objects of the client's objects with a body of their own and none here (or one
    /// for an older incarnation), each made as the client asks.
    fn create_physics_objs(&mut self, client: &mut Client) {
        let player = self.player.as_ref().map(|p| p.id);
        let wanted: Vec<u64> = client
            .model
            .objects()
            .filter(|o| Some(o.id) != player && others::drawn(o))
            .filter(|o| self.others.wants_physics_obj(o))
            .map(|o| o.id)
            .collect();
        for id in wanted {
            let Some(object) = client.model.object(id).cloned() else {
                continue;
            };
            self.creating = Some((object, None));
            client.create_physics_obj(id, self);
            self.creating = None;
        }
    }

    /// What the bodies asked of each other after the frame: what the cells' watchers were told of
    /// objects entering and leaving them, to the watchers; then, a few rounds over, what each
    /// asked of another, to that body, and what each asked of its own figure, to the figure (the
    /// effects they ask are a window's to play: here they go).
    fn route_notices(&mut self) {
        for (watcher, info) in self.ground.table.take_detection_updates() {
            if let Some(b) = body_mut(&mut self.player, &mut self.others, watcher) {
                b.receive_detection_update(&info);
            }
        }
        for _ in 0..notices::ROUNDS {
            let mut asked: Vec<(u64, auberean_physics::body::Notice)> = Vec::new();
            if let Some(p) = &mut self.player {
                asked.extend(p.body.take_requests().into_iter().map(|n| (p.id, n)));
            }
            asked.extend(self.others.take_notices());
            if asked.is_empty() {
                break;
            }
            for (from, n) in asked {
                if let Some(to) = n.addressee() {
                    if let Some(b) = body_mut(&mut self.player, &mut self.others, to) {
                        b.deliver(&n);
                    }
                    continue;
                }
                if matches!(
                    n,
                    auberean_physics::body::Notice::Animate(
                        auberean_physics::body::Animate::Fx { .. }
                    ) | auberean_physics::body::Notice::TerrainType(_)
                        | auberean_physics::body::Notice::StreakOn
                        | auberean_physics::body::Notice::StreakOff
                ) {
                    continue;
                }
                let figure = match &mut self.player {
                    Some(p) if p.id == from => Some(&mut p.figure),
                    _ => self.others.figure_mut(from),
                };
                notices::hand_on(n, figure, &self.anims);
            }
        }
    }

    /// The player's body follows the client's player object: made when the client has the object
    /// and a place for it, its look kept up to date, and let go of with the world when it goes.
    fn take_player(&mut self, client: &mut Client) {
        let wanted = client
            .model
            .player_object()
            .filter(|_| client.body.placed())
            .cloned();
        match (&wanted, &self.player) {
            (None, _) => {
                if let Some(k) = self
                    .player
                    .as_ref()
                    .and_then(|p| self.ground.table.find(p.id))
                {
                    self.ground.table.remove(k);
                }
                if self.player.take().is_some() {
                    self.ground.reset_world_state(client);
                    self.release_objects(client);
                    self.ground.near.notify_change_region();
                }
            }
            (Some(o), Some(p)) if p.id == o.id => {
                if p.visual != o.visual || p.visual_updates != o.visual_updates {
                    if let Some(p) = &mut self.player {
                        p.take_visual(&self.world.records, &self.anims, &mut self.ground.trees, o);
                    }
                }
            }
            (Some(o), _) => {
                self.player = None;
                self.creating = Some((o.clone(), Some(client.body.clone())));
                client.create_physics_obj(o.id, self);
                self.creating = None;
                if self.player.is_some() {
                    // D2-04688
                    let _ = client.interface(PRELOAD_SINGLETONS, &[]);
                    self.follow(client, true);
                }
            }
        }
    }

    /// The cells about the player follow its body (`placed`: the server just put it there), and the
    /// objects standing in the cells let go of leave the world as the client says.
    fn follow(&mut self, client: &mut Client, placed: bool) {
        let at = self.player.as_ref().map(|p| p.body.position);
        self.ground
            .follow(client, self.world, at, placed, self.clock);
        self.release_objects(client);
    }

    /// The player's cell told to the client, then the cells let go of: the objects standing in
    /// each leave the world as the client says.
    fn release_objects(&mut self, client: &mut Client) {
        self.report_player_cell(client);
        let cell = self.player.as_ref().map(|p| p.body.position.cell.0);
        let go = self.ground.cells_let_go(cell, &self.reported);
        for block in go.released {
            self.others.set_block_on_free_list(block, true);
        }
        for block in go.handed_out {
            self.others.set_block_on_free_list(block, false);
        }
        for cell in go.cells {
            world::release_obj_cell(&client.queue, &client.model, cell, self);
            self.ground
                .table
                .release_cell_watchers(dereth_primitives::CellId(cell));
        }
    }

    /// The player's cell told to the client where that changed since it was last told, before any
    /// cell is let go of.
    fn report_player_cell(&mut self, client: &mut Client) {
        let Some(p) = &self.player else {
            return;
        };
        let cell = p.body.position.cell.0;
        if self.reported.get(&p.id) != Some(&cell) {
            client.queue.change_cell(p.id, Some(cell));
            self.reported.insert(p.id, cell);
        }
    }

    /// The client told the cell each body stands in where that changed since it was last told
    /// (none for a body out of the world, or one gone while its object stays).
    fn report_cell(&mut self, client: &mut Client) {
        let mut now: BTreeMap<u64, u32> = self
            .player
            .iter()
            .map(|p| (p.id, p.body.position.cell.0))
            .collect();
        for (id, cell) in self.others.cells() {
            now.insert(id, cell.unwrap_or(0));
        }
        for (id, cell) in &now {
            if self.reported.get(id) != Some(cell) {
                client.queue.change_cell(*id, (*cell != 0).then_some(*cell));
            }
        }
        for (id, cell) in &self.reported {
            if *cell != 0 && !now.contains_key(id) {
                client.queue.change_cell(*id, None);
            }
        }
        self.reported = now;
    }

    /// What the scripts asked of the player's body: frozen or thawed, its movement and its
    /// animation; and the requests for it carried out in order (those for other bodies dropped).
    // D2-06029, D2-06030
    fn carry_out_scripts(&mut self, client: &mut Client) {
        let Some(scripts) = client.scripts.as_mut() else {
            return;
        };
        let asked = core::mem::take(&mut scripts.host.body_requests);
        let Some(p) = &mut self.player else {
            return;
        };
        if let Some((movement, animation)) = scripts.host.frozen.get(&p.id) {
            let b = &mut p.body;
            if b.is_movement_frozen() && !*movement {
                b.set_movement_active(true);
            }
            b.set_movement_frozen(*movement);
            b.set_animation_frozen(*animation);
        }
        for r in asked {
            if player::request_object(&r) == p.id {
                player::carry_out_request(&mut p.body, &r, None);
            }
        }
    }

    /// What the player's body says of itself, noted for the scripts.
    fn note_reading(&self, client: &mut Client) {
        let Some(scripts) = client.scripts.as_mut() else {
            return;
        };
        let host = &mut scripts.host;
        host.readings.clear();
        if let Some(p) = &self.player {
            player::note_reading(
                &mut host.readings,
                &mut host.materials,
                p.id,
                &p.body,
                Some(&p.figure),
            );
        }
        let readings = &host.readings;
        host.materials.retain(|id, _| readings.contains_key(id));
    }
}

/// The zoom steps the controls pressed this frame: positive toward the player.
fn zoom_steps(controls: &ControlFrame) -> i32 {
    controls
        .presses
        .iter()
        .map(|p| match p {
            auberean_client_contract::control::CameraPress::Zoom(n) => *n,
            _ => 0,
        })
        .sum()
}

/// The body of object `id`: the player's or another's.
fn body_mut<'a>(
    player: &'a mut Option<Player>,
    others: &'a mut Others,
    id: u64,
) -> Option<&'a mut auberean_physics::body::Body> {
    match player {
        Some(p) if p.id == id => Some(&mut p.body),
        _ => others.body_mut(id),
    }
}
