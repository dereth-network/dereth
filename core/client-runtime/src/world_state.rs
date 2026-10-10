//! The world as the simulation holds it.
//!
//! `WorldState` is the simulation and residency half of the drawn world, beside
//! `dereth_scene::world_scene::WorldScene`: the local body, every server object's simulation record,
//! the landblock residency window, the chase camera, the calendar clock and its tick schedule, the
//! environment options, and the queues the frame drains (sounds, house-barrier effects, the
//! movement latch). Nothing in it names a device: the drawing half -- meshes, textures, the
//! baked landscape, the lights and the per-frame draw records -- stays in `WorldScene`, which owns
//! a `WorldState` and draws from it.
//!
//! Every server object is one `WorldObject` here, keyed by id; its geometry is the drawing
//! half's, in a map keyed the same way, so the two walk in the same id order.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_physics::obj::PhysicsState;
use dereth_primitives::{CellId, DataId, Frame, ObjectId, Vec3};

use crate::anim_hooks::HookObject;
use crate::audio::SoundTrigger;
use crate::camera::FreeCamera;
use crate::character::{Character, RenderSpace};
use crate::environment::EnvironmentOverrideState;
use crate::object_step::{AsObjectSim, ObjectSim};
use crate::objects::ObjectStream;
use dereth_world_data::anim_assets::DatAnimAssets;

/// One object the server put in the world.
///
/// It is deliberately **not** a `Character`: a remote object has no local physics in this
/// build. The native client would integrate its velocity and collide it
/// against the landscape between server corrections; running that here would be dead reckoning,
/// and the client is a viewer. What is kept is the animation half — the
/// part array and the motion table — because playing back a motion the *server* chose is not a
/// prediction of anything.
///
/// The simulation half of the scene's object record; the drawing half (the
/// part geometry and the per-frame level choices) is `WorldScene`'s, keyed by the same id.
#[derive(Debug)]
pub struct WorldObject {
    /// The simulation half — driver, body handle, position, parent, state, clock and
    /// dimensions (`crate::object_step::ObjectSim`); the fields kept
    /// here are the ones that draw this object.
    pub sim: ObjectSim,
    /// the physics-object id matches the viewer id. Needed every frame rather than only at
    /// bake time, because it is the viewer-distance update's own guard: **the local player
    /// never degrades**.
    pub is_player: bool,
    /// The last wire word applied through `WorldScene::apply_object_state`, so that a
    /// `NODRAW_PS` this client forced onto a hidden holder's child does not read as a new word
    /// from the server on the next frame. `crate::object_physics::ObjectPhysics` keeps the
    /// same latch for the same reason (`stated`), for the same message.
    pub wire_state: u32,
    /// Whether [`Self::frame`] is a real place this frame.
    ///
    /// Always true for an object with a server position. For a **held** one it is true only
    /// once its holder has been placed and the holder's setup carries a holding
    /// location at the named `ParentLocation`. Otherwise attachment fails before any
    /// leave-world or enter-world transition,
    /// so the client has the object in its tables and **not** in a cell: not drawn, and not
    /// pickable, because "only objects that are actually drawn can be picked".
    ///
    /// Without this an unattachable child would be drawn at the identity, which in render
    /// space is the south-west corner of the viewer's landblock — a weapon lying on the ground
    /// a hundred metres away, which is worse than no weapon.
    pub drawn: bool,
    /// The object's position in **viewer-block-relative** render space, for this frame.
    pub frame: Frame,
    /// The placement id the part array is currently posed by.
    ///
    /// Setup creation installs `0x65`, immediately overwritten by the object's descriptor,
    /// so this starts at whatever
    /// `crate::objects::Presence::placement` holds and is re-applied when the server names a
    /// different one on a later `0xF748`.
    pub placement: u32,
    /// The sound table a `SoundTable` animation hook resolves
    /// its `SoundType` against, so the same animation gives a drudge and a lugian different
    /// voices.
    ///
    /// **The setup's table, not the server override.** The override reaches the client
    /// on the create and `crate::objects::Presence` does not keep it, so this takes the
    /// setup's; the gap is a known one.
    pub sound_table: Option<DataId>,
}

impl AsObjectSim for WorldObject {
    fn sim(&self) -> &ObjectSim {
        &self.sim
    }
    fn sim_mut(&mut self) -> &mut ObjectSim {
        &mut self.sim
    }
}

impl HookObject for WorldObject {
    /// The **drawn** origin, which is where the client's sound playback takes its position from
    /// and the space the listener works in.
    fn sound_origin(&self) -> Vec3 {
        self.frame.origin
    }
    fn sound_table(&self) -> Option<DataId> {
        self.sound_table
    }
}

/// The simulation and residency half of the scene. See the module header.
///
/// Every field is public: the drawing half in `dereth_client` reads and writes it directly.
#[derive(Debug)]
pub struct WorldState {
    pub camera: FreeCamera,
    /// The residency window and the viewer's cell within its own
    /// block, as the `crate::world_stream::WorldStreamer` the scene owns and drives.
    pub streamer: crate::world_stream::WorldStreamer,
    /// Landblocks whose interior cells `WorldScene::release_block_interiors` has
    /// handed back, waiting for `WorldScene::sync_objects` to run the *object* half of the same
    /// teardown, releasing the objects of every flushed cell.
    ///
    /// It is a queue for the same reason `WorldScene::released_blocks` is one:
    /// `release_block_interiors` is reached from `WorldScene::queue`, which holds neither the
    /// `ObjectStream` nor a clock, and `sync_objects` holds both. Retail releases on the
    /// spot; here the edge is
    /// recorded and settled at the next `sync_objects`, which is the same frame step in
    /// `App::frame` and at most one frame later.
    pub released_interiors: Vec<dereth_primitives::LandblockId>,
    /// Objects `WorldScene::prepare_object_dispatch` has just built and that
    /// `WorldScene::sync_objects` still owes to.
    ///
    /// A queue rather than an immediate call because retail runs `enter_world` *after*
    /// object creation's `set_description` has applied the descriptor's
    /// movement buffer, and in this build that buffer is parked on the `Presence` and drained
    /// by `sync_objects`'s loop -- one function later. `App::frame` may also run
    /// `prepare_object_dispatch` on its own for the ordered player dispatch, so the ids have
    /// to survive until the loop that drains them.
    pub entering_world: Vec<ObjectId>,
    /// `CameraControl::stats.sweeps` as it stood at the top of the previous
    /// `WorldScene::update`, so that "no sweep happened between the last two updates" is a
    /// number this scene reports rather than a symptom another test trips over twenty frames
    /// later. `None` before the first update. See `SceneStats::updates_without_a_sweep`.
    pub sweeps_seen: Option<u64>,
    /// The local body. `None` leaves a free camera over the static scene.
    pub character: Option<Character>,
    /// The physics bodies of the objects every resident interior cell bakes in.
    /// It lives here rather than beside the
    /// body because a cell belongs to a landblock and this is what owns the resident blocks.
    pub cell_static_objects: dereth_world_data::env_cells::CellStaticObjects,
    /// How many cells this scene has fenced with a house restriction,
    /// cumulative over the run. Reported the way the cell-static count is, for the same
    /// reason: "the world looks right and nothing stops you" is a failure no picture shows.
    pub restricted_cells: usize,
    /// Everything the server put in the world, in id order.
    ///
    /// Each object's drawing half lives with the renderer under the same id, and
    /// the renderer's object dispatch is the one place that adds or removes either. Nothing else
    /// may insert into or remove from this map: the renderer checks in debug builds that the two
    /// agree, and it cannot draw an object whose drawing half is missing.
    pub objects: BTreeMap<ObjectId, WorldObject>,
    /// latched by `WorldScene::sync_objects`.
    ///
    /// The player is the one holder with **no** `SceneObject`, so a weapon wielded by him
    /// has to hang off `Character`'s part array instead. Kept here because
    /// `advance_objects` does not have the `crate::objects::ObjectStream` in hand.
    pub player_object: Option<ObjectId>,
    /// The body's own physics-state word, as far as
    /// `WorldScene::apply_character_state` has applied it.
    ///
    /// Every other object's copy is `SceneObject::wire_state`. The player has no
    /// `SceneObject` (see the note above), so his lives here; without it **his state word
    /// would reach nothing that draws him**, and 22 of the corpus's 57 `0xF74B` are about the
    /// player. `PhysicsState::DEFAULT` is the physics body's initial value, which carries
    /// neither `NODRAW_PS` nor `HIDDEN_PS`, so the first word the server sends can only
    /// report a change that really is one.
    pub character_state: PhysicsState,
    /// The `AnimAssets` every remote object's `MotionDriver` reads through, built once.
    pub anim_assets: Option<Arc<DatAnimAssets>>,
    /// The body's own default sound-table id.
    pub character_sound_table: Option<DataId>,
    /// the body translucency the chase camera **last applied**.
    ///
    /// The client keeps this global for one reason and it is not caching:
    /// The camera resets hierarchical player translucency to zero
    /// only when the last applied value was positive. Setting translucency clears
    /// the `NoDraw` bit on every call that is not exactly 1.0. Without the latch the camera
    /// would un-hide, every frame, whatever an animation's `NoDraw` hook had hidden.
    pub camera_translucency: f32,
    pub clock: crate::game_clock::GameClock,
    /// The next environment and lighting ticks. The region overrides their periods
    /// to 0.8 s and 15 s.
    pub next_tick: f64,
    pub next_light_tick: f64,
    /// Administrative environment overrides, whose sole writer is the environment command.
    /// The two tick consumers deliberately share its
    /// one transition value.
    pub environment_override: EnvironmentOverrideState,
    /// The `PersistentAtDay` character option, disabled by default.
    /// Written only by `WorldScene::set_always_daylight` and read by `WorldScene::apply_lighting`,
    /// matching the client's option update and lighting paths (also controlled by `/day`).
    pub always_daylight: bool,
    /// The chase camera's yaw **offset from the character's own heading**, and its pitch.
    pub camera_orbit: (f32, f32),
    /// How far behind the character the chase camera sits.
    pub camera_distance: f32,
    /// The sound triggers `WorldScene::process_hooks` took off the drivers, waiting
    /// for `WorldScene::take_sound_events` to collect them.
    ///
    /// The queue exists because there is exactly **one** drain and there are two
    /// consumers. `MotionDriver::take_events` empties the whole `Vec`, so without it whichever
    /// consumer ran first would get every event and the other none — and the audio consumer
    /// runs only when there **is** an audio device, which would make a door's collision state
    /// depend on whether the machine had sound.
    pub pending_sound: Vec<SoundTrigger>,
    /// House barriers the body ran into this frame, waiting for the drain
    /// in `WorldScene::sync_objects` that can see `dereth_client_model::World`: the body, the house, and the
    /// PlayScript intensity. The house's public description carries the script type, and
    /// only `dereth_client_model` holds that description.
    pub pending_restriction_effects: Vec<(ObjectId, ObjectId, f32)>,
    /// Current time as `WorldScene::advance_objects` last saw it.
    ///
    /// Starting a stick stamps `sticky_timeout_time = cur_time + 1.0`, and
    /// `WorldScene::sync_objects` — where a `0xF74C` is unpacked — is handed no clock at all. It
    /// is at most one frame stale, and only until the first `TargetInfo`, which re-stamps the
    /// timeout from `MotionEnv::cur_time` (which `MotionDriver::tick_movement` sets from the
    /// real `now`).
    pub last_object_time: f64,
    /// The render-only scene's copy of the last physics-update time.
    ///
    /// With a character present, `Character::update` supplies the real
    /// physics tick result and this field is unused. A bodyless
    /// viewer has no `PhysicsWorld`, but its server objects still belong to the same native
    /// physics-update outer gate; this clock preserves that gate instead of
    /// letting `WorldScene::advance_objects`'s fallback ladder run at the display rate. It is
    /// separate from `WorldScene::last_object_time`, which must continue to follow every display
    /// frame for movement-command ingress.
    pub last_bodyless_tick: f64,
    /// Movement-buffer application's **return value**, latched
    /// for `WorldScene::take_player_movement_applied`.
    ///
    /// That function unpacks the buffer only when `(autonomous == 0 || not-the-player)` and
    /// sets its result flag only when the weenie's player-object answer is non-null, i.e. only for the
    /// **player** — so the two together are *non-autonomous* **and** *the player*, which is
    /// exactly the arm `apply_player_movement` is. The `0xF74C` dispatch hands control of
    /// the player to the server when that result is 1; losing control marks the interpreter as
    /// server-controlled and disables autorun.
    ///
    /// It is a **latch** rather than the call itself because the interpreter is not here:
    /// `crate::character::MovementCommands` and `crate::character::CharacterInput` live on
    /// `crate::app::App`, and this arm runs inside `WorldScene::sync_objects`, which is handed
    /// neither. A value the frame drains is the shape `WorldScene::take_sound_events` already uses
    /// for the same reason. **The consumer is `App::frame`, immediately after
    /// `sync_objects`**, so the retail order — dispatch, then —
    /// holds inside one frame.
    pub player_movement_applied: bool,
    /// Every animated body is drawn between its animation's keyframes, a presentation's choice
    /// (`dereth_animation::seq::between`); off, each part is drawn at the keyframe its animation
    /// stands at, as the game draws it. Only where the parts are drawn changes: the animations
    /// are advanced, fire their hooks and move the bodies exactly as they do with it off, and
    /// what the bodies collide with is posed at the keyframes either way.
    pub smooth_animation: bool,
}

/// One object's **pose**, as [`WorldState::server_object_pose`] reports it: the animation ids
/// the sequence is holding, the node it is playing, the node it wraps back to, and the frame it
/// is on.
pub type ObjectPose = (Vec<DataId>, Option<usize>, Option<usize>, i32);

impl WorldState {
    /// A point inside one resident interior cell that a body can stand at, in that cell's own
    /// block-local space, for `--start-cell` and for the tests.
    ///
    /// The cell's **own** geometry decides, not this function: `cell_bsp` through
    ///  says whether a point is in the room, and
    /// `physics_bsp` says whether it is inside the masonry. Sampling is only how the two are
    /// asked; the answer is the client's.
    #[must_use]
    pub fn standable_point(&self, cell: CellId) -> Option<Vec3> {
        crate::object_step::standable_point(self.character.as_ref()?, cell)
    }

    /// For the tests: what made of the resident
    /// cells, and the physics handles one cell's objects hold.
    #[must_use]
    pub fn cell_static_stats(&self) -> dereth_world_data::env_cells::CellStaticStats {
        self.cell_static_objects.stats
    }

    /// The bodies one interior cell's baked objects hold, for the tests.
    #[must_use]
    pub fn cell_static_handles(&self, cell: CellId) -> &[dereth_physics::PhysHandle] {
        self.cell_static_objects.handles(cell)
    }

    /// How many physics bodies the resident interior cells' baked objects hold **right now**.
    /// `cell_statics.created - cell_statics.destroyed`, counted from the table
    /// rather than from the counters so that the two can be compared.
    #[must_use]
    pub fn cell_static_bodies(&self) -> usize {
        self.cell_static_objects.len()
    }

    /// Which interior cells hold those bodies. A count cannot say which *blocks*
    /// a residency spans, and that is the question block release has to answer.
    #[must_use]
    pub fn cell_static_cells(&self) -> Vec<CellId> {
        self.cell_static_objects.registered_cells()
    }

    pub fn cell_seen_outside(&self, cell: CellId) -> bool {
        crate::world_stream::cell_seen_outside(&self.character, cell)
    }

    pub fn choose_viewer_block(&mut self, block: (i32, i32)) -> RenderSpace {
        crate::world_stream::choose_viewer_block(&mut self.character, block)
    }

    pub fn update_viewer_cell(&mut self) {
        let Self {
            streamer,
            character,
            camera,
            ..
        } = self;
        streamer.update_viewer_cell(character, camera);
    }

    /// The block the window is centred on, for the tests and the log lines.
    #[must_use]
    pub fn viewer_block(&self) -> Option<(i32, i32)> {
        self.streamer.window.viewer_block()
    }

    /// Landblocks waiting for the object half of their interior release
    /// (`released_interiors`). Instrumentation.
    #[must_use]
    pub fn released_interior_count(&self) -> usize {
        self.released_interiors.len()
    }

    /// Objects still owed their enter-world (`entering_world`). Instrumentation.
    #[must_use]
    pub fn entering_world_count(&self) -> usize {
        self.entering_world.len()
    }

    /// Sound triggers taken off the drivers and not yet collected. Instrumentation.
    #[must_use]
    pub fn pending_sound_count(&self) -> usize {
        self.pending_sound.len()
    }

    /// House-barrier contacts waiting for the drain in `Self::sync_objects`. Instrumentation.
    #[must_use]
    pub fn pending_restriction_count(&self) -> usize {
        self.pending_restriction_effects.len()
    }

    /// The landscape draw radius currently live — what the last preference update set.
    /// So a test can tell the ring apart from the preference that asks for it.
    #[must_use]
    pub fn mid_radius(&self) -> u32 {
        self.streamer.window.mid_radius()
    }

    /// Cache actual body dimensions before target lookup. App also calls this after preparing
    /// new bodies for ordered player dispatch, not one frame after an F74C used zero bounds.
    pub fn refresh_object_geometry(&mut self, stream: &ObjectStream) {
        for (id, object) in &mut self.objects {
            let (radius, height) = stream
                .physics
                .handle(*id)
                .and_then(|h| self.character.as_ref().and_then(|c| c.world.get(h)))
                .map_or((0.0, 0.0), |body| (body.radius(), body.height()));
            object.sim.radius = radius;
            object.sim.height = height;
        }
    }

    /// The body's own sound table, which is what a `SoundTable` animation hook resolves its
    /// `SoundType` against.
    #[must_use]
    pub fn character_sound_table(&self) -> Option<DataId> {
        self.character_sound_table
    }

    /// The listener derived from the active viewer.
    ///
    /// **This is the camera, not the player.** Viewer setup passes
    /// a reference to the position the camera produced — the same value used for
    /// sky positioning and render-camera setup. Only the origin and the yaw are
    /// read; pitch and roll never affect the mix, so looking up or down changes nothing.
    ///
    /// The space is the renderer's viewer-block-relative one, which is also the space
    /// `Self::terrain_neighbourhood` reports its cells in, so the distances
    /// `dereth_audio::atten` computes are right without any landblock arithmetic.
    #[must_use]
    pub fn listener(&self) -> dereth_audio::Listener {
        let f = self.camera.forward();
        dereth_audio::Listener {
            pos: self.camera.position,
            // Heading is taken from the frame's forward vector,
            // which is what is measured against.
            heading: dereth_physics::math::vector_get_heading(f),
        }
    }

    /// Dispatch a script type — the scene half of the `0xF755` Effects_PlayScriptType message.
    ///
    /// The script-effect handler looks up the physics object and plays `(type, mod)` on
    /// whatever it found; this is the
    /// second half, because the scene is where the client's one physics body per id lives.
    /// There is exactly one per id: the player is drawn from `Character` and **not** from
    /// `self.objects`, so his id has to be routed to the body or a level-up would resolve
    /// against nothing.
    ///
    /// Returns whether the physics object accepted the script, which is not "did anything
    /// appear": no cell is success and no `PhysicsScriptTable` is failure. A `true` here means the script was
    /// queued, and the emitters it creates arrive on the next
    /// `dereth_animation::MotionDriver::update_scripts`, not now.
    pub fn play_script_type(&mut self, id: ObjectId, script_type: u32, intensity: f32) -> bool {
        if self.player_object == Some(id) {
            if let Some(c) = self.character.as_ref() {
                return c.driver_mut().play_script_type(script_type, intensity);
            }
        }
        match self.objects.get(&id) {
            Some(o) => o
                .sim
                .driver
                .borrow_mut()
                .play_script_type(script_type, intensity),
            None => false,
        }
    }

    /// Play the default script at the tail of collision handling,
    /// which is the client's *whole* impact
    /// effect. The body is `crate::anim_hooks::play_default_script`.
    pub fn play_default_script(&mut self, id: ObjectId) -> bool {
        crate::anim_hooks::play_default_script(
            &self.objects,
            &self.character,
            self.player_object,
            id,
        )
    }

    /// Dispatch a script id — the scene half of the `0xF754` Effects_PlayScriptID message,
    /// which names the `PhysicsScript` outright and so consults no table.
    pub fn play_script_id(&mut self, id: ObjectId, script: DataId) -> bool {
        if self.player_object == Some(id) {
            if let Some(c) = self.character.as_ref() {
                return c.driver_mut().play_script_id(script);
            }
        }
        match self.objects.get(&id) {
            Some(o) => o.sim.driver.borrow_mut().play_script_id(script),
            None => false,
        }
    }

    /// The Empyrean appearance Apply prelude:
    /// install the selected floating/standing table on the existing local setup before
    /// generating and sending the appearance. The unchanged-setup arm deliberately avoids a
    /// part-array rebuild; `Character::set_setup_id` applies the independently authored
    /// motion table and recreates only its movement manager.
    pub fn replace_player_motion_table(&mut self, motion_table: DataId) -> bool {
        let Some(character) = self.character.as_mut() else {
            return false;
        };
        let setup = character.setup_id();
        if character.set_setup_id(setup, Some(motion_table)).is_err() {
            return false;
        }
        character.motion_table_id() == motion_table
    }

    /// Whether applying object movement succeeded since the
    /// last call — i.e. whether the server moved the *player* with a *non-autonomous* buffer.
    /// See `Self::player_movement_applied`.
    ///
    /// The caller is `App::frame`, and what it owes on a `true` is
    /// `MovementCommands::lose_control_to_server`. It is a **take** rather than a read so that
    /// two frames cannot both act on one dispatch: losing control to the server is an edge,
    /// and an edge sampled as a state fires twice.
    pub fn take_player_movement_applied(&mut self) -> bool {
        std::mem::take(&mut self.player_movement_applied)
    }

    /// How many of the drawn objects the server owns.
    #[must_use]
    pub fn server_object_count(&self) -> usize {
        self.objects.len()
    }

    /// One object's current render frame, for the tests — and for the pick, which reads it
    /// through `crate::pick::PickScene`.
    ///
    /// `None` for an object the scene did not draw this frame, which includes
    /// a held object whose holder refused the attachment. Only objects actually drawn can be picked.
    #[must_use]
    pub fn server_object_frame(&self, id: ObjectId) -> Option<Frame> {
        self.objects.get(&id).filter(|o| o.drawn).map(|o| o.frame)
    }

    /// The cell this object is drawn **in**, as
    /// `Self::object_draw_cell` resolves it for both the object pass and
    /// `Self::collect_particles`.
    ///
    /// `None` is the native "in no cell list at all" state that visibility departure leaves an
    /// object in, and it is
    /// the whole of what keeps a teleported-away body — and anything its `ParticleManager`
    /// is still emitting — off this client's screen. Published because the cell and the frame
    /// are separately observable.
    #[must_use]
    pub fn server_object_draw_cell(&self, id: ObjectId) -> Option<CellId> {
        self.object_draw_cell(self.objects.get(&id)?)
    }

    /// The cell whose native object-list walk owns this object.
    ///
    /// A placed object carries its own cell. A held object deliberately has no wire position
    /// here, so follow its holder chain until reaching a positioned remote object or the local
    /// character. `Self::place_held_objects` supports the same nested chains. The bounded
    /// walk leaves a malformed parent cycle unresolved instead of looping forever.
    pub fn object_draw_cell(&self, object: &WorldObject) -> Option<CellId> {
        if let Some(position) = object.sim.position {
            return Some(position.cell);
        }
        let mut parent = object.sim.parent.map(|(id, _)| id);
        for _ in 0..=self.objects.len() {
            let id = parent?;
            if self.player_object == Some(id) {
                // **The local body's cell, but only when there *is* a local body.**
                //
                // Retail reads the holder's own object and nothing else: assign the parent,
                // read the holder's cell, and change the child's cell only when that pointer
                // is non-null. The draw reads that same cell field,
                // and there is no player arm on that path because retail has exactly **one**
                // physics body per object, the player's included.
                //
                // This branch exists only because the player's body is a
                // `Character` that is *not* in `self.objects`, so his cell has to be
                // fetched from there instead. On the `--no-character` flycam
                // (`SceneConfig::character`) there is no `Character` and the player is an
                // ordinary `SceneObject` carrying his own position. Returning `None` there
                // would mean "in no cell at all", and since interior eligibility depends on the
                // draw cell, `None` is neither outdoors nor a reached interior cell, so
                // **everything the player's server object holds would stop being drawn**.
                // Falling through to his own `SceneObject` below is the native behavior for
                // that configuration.
                if let Some(c) = self.character.as_ref() {
                    return Some(c.position().cell);
                }
            }
            let holder = self.objects.get(&id)?;
            if let Some(position) = holder.sim.position {
                return Some(position.cell);
            }
            parent = holder.sim.parent.map(|(id, _)| id);
        }
        None
    }

    /// One object's part frames in render space, for the tests — what part updating last wrote.
    /// The placement tests assert against these
    /// because the placement id changes where the parts sit and nothing else about the object.
    #[must_use]
    pub fn server_object_part_frames(&self, id: ObjectId) -> Option<Vec<Frame>> {
        let o = self.objects.get(&id)?;
        Some(
            o.sim
                .driver
                .borrow()
                .part_array
                .parts
                .iter()
                .map(|p| p.pos)
                .collect(),
        )
    }

    /// One object's part graphics-object ids and each part's maximum degrade distance,
    /// for the tests.
    ///
    /// A successful part replacement writes both halves; a failed replacement
    /// leaves both alone, so a test that reads only the id cannot tell a
    /// swap that kept the *limb* from one that kept the limb and silently took the new
    /// object's LOD table. `max_degrade_distance` is **100.0** for a part with no record, so a
    /// pair of objects with different records is the only assertion that cannot pass by both
    /// being absent.
    #[must_use]
    pub fn server_object_part_lod(&self, id: ObjectId) -> Option<Vec<(DataId, f32)>> {
        let o = self.objects.get(&id)?;
        Some(
            o.sim
                .driver
                .borrow()
                .part_array
                .parts
                .iter()
                .map(|p| (p.gfxobj_id, p.max_degrade_distance()))
                .collect(),
        )
    }

    /// The local body's, likewise — he has no `SceneObject`.
    #[must_use]
    pub fn character_part_lod(&self) -> Option<Vec<(DataId, f32)>> {
        let c = self.character.as_ref()?;
        let d = c.driver();
        Some(
            d.part_array
                .parts
                .iter()
                .map(|p| (p.gfxobj_id, p.max_degrade_distance()))
                .collect(),
        )
    }

    /// The local body's root frame and its part frames in render space, for the tests — the
    /// two inputs `crate::models::child_frame` takes when the **player** is the holder.
    ///
    /// The player has no `SceneObject`, so `Self::server_object_part_frames` cannot answer
    /// for him.
    #[must_use]
    pub fn character_frames(&self) -> Option<(Frame, Vec<Frame>)> {
        let c = self.character.as_ref()?;
        let d = c.driver();
        Some((
            c.render_frame(),
            d.part_array.parts.iter().map(|p| p.pos).collect(),
        ))
    }

    /// The **local body's** animation state, for the tests: whether anything is playing, the
    /// current sequence frame, and the forward command its motion interpolation is in.
    ///
    /// `server_object_motion` below cannot answer for the player, who has no `SceneObject`.
    /// This exists because a rebuilt body has to end up animating *where it already was*: a
    /// `set_motion_table` on an unchanged table id runs `enter_default_state`, whose
    /// `initialize_state` restarts the sequence, and two bodies at different frames of the
    /// same idle differ on screen by **pose** while every part-id assertion still passes.
    /// A part-id-only assertion therefore misses this visible pose difference.
    ///
    /// `has_anims` is the third state: the sequence falls back
    /// to the **placement frame** when its node list is empty, so a body with no animation at
    /// all is a body standing in the setup record's reference pose.
    #[must_use]
    pub fn character_motion(&self) -> Option<(bool, i32, MotionCommand)> {
        let c = self.character.as_ref()?;
        let d = c.driver();
        Some((
            d.sequence.has_anims(),
            d.sequence.curr_frame_number(),
            d.movement.interp.interpreted_state.forward_command,
        ))
    }

    /// One object's **stance**: the stored current movement style that the movement decoder
    /// compares its buffer's style word against and then writes.
    ///
    /// `Self::character_motion` and `Self::server_object_motion` both report the *forward*
    /// command, which no style word carries; this is the other half, and on a
    /// `MoveTo`/`TurnTo` buffer it is the only thing in the buffer that can move at all. The
    /// local body's equivalent is read through `Character::driver` by the player-movement
    /// tests, which is why there is no `character_style` beside it.
    #[must_use]
    pub fn server_object_style(&self, id: ObjectId) -> Option<MotionCommand> {
        let o = self.objects.get(&id)?;
        Some(
            o.sim
                .driver
                .borrow()
                .movement
                .interp
                .interpreted_state
                .current_style,
        )
    }

    /// One object's **pose**: the animations its sequence is actually playing, the node it is
    /// on, and the frame it is at.
    ///
    /// The observable is *"a creature's pose follows the style word"*, and the stance
    /// word alone cannot say that: performing the motion writes `interpreted_state.current_style`
    /// **and** asks the motion table for the new stance's cycle, and a build that wrote the
    /// word without reaching the table would satisfy `Self::server_object_style` and leave
    /// the creature standing in the animation it had. These are the ids the parts are posed
    /// from — reads `nodes[floor(frame_number)]` —
    /// so a change here is a change on screen.
    #[must_use]
    pub fn server_object_pose(&self, id: ObjectId) -> Option<ObjectPose> {
        let driver = self.objects.get(&id)?.sim.driver.borrow();
        let seq = &driver.sequence;
        Some((
            seq.nodes().iter().map(|n| n.anim_id).collect(),
            seq.curr(),
            seq.first_cyclic(),
            seq.curr_frame_number(),
        ))
    }

    /// Read-only snapshot of a remote animation clock/root-motion source, for parity probes.
    /// The live driver remains owned by the scene and its attached physics body.
    #[must_use]
    pub fn server_object_sequence(&self, id: ObjectId) -> Option<dereth_animation::seq::Sequence> {
        Some(self.objects.get(&id)?.sim.driver.borrow().sequence.clone())
    }

    /// Non-owning lifecycle probe: neither renderer nor physical adapter may retain an old
    /// instance after deletion. Holding this does not keep its motion/target state alive.
    #[must_use]
    pub fn server_object_motion_lifetime(
        &self,
        id: ObjectId,
    ) -> Option<std::rc::Weak<std::cell::RefCell<dereth_animation::MotionDriver>>> {
        Some(Rc::downgrade(&self.objects.get(&id)?.sim.driver))
    }

    /// One object's animation state, for the tests: the current sequence frame and the forward
    /// command its motion interpolation is in.
    #[must_use]
    pub fn server_object_motion(&self, id: ObjectId) -> Option<(i32, MotionCommand)> {
        let o = self.objects.get(&id)?;
        let driver = o.sim.driver.borrow();
        Some((
            driver.sequence.curr_frame_number(),
            driver.movement.interp.interpreted_state.forward_command,
        ))
    }

    /// Which cell of its own block the viewer stands in, for the tests and the log lines —
    /// what `Self::update_viewer_cell` last computed.
    #[must_use]
    pub fn viewer_draw_cell(&self) -> (u8, u8) {
        self.streamer.viewer_cell
    }

    /// update `n` — the overhead and map camera modes force
    /// every object to degrade level 0.
    ///
    /// The look-down camera toggle (only while map mode is on) and the map-mode setter are
    /// what raise it, and `crate::camera::CameraEffects` is where the camera parks the request
    /// rather than dropping it. This is the other end of that wire.
    #[must_use]
    pub fn degrades_disabled(&self) -> bool {
        self.character
            .as_ref()
            .is_some_and(|c| c.camera.set.effects.degrades_disabled)
    }

    /// The always-daylight flag as it stands.
    #[must_use]
    pub fn always_daylight(&self) -> bool {
        self.always_daylight
    }

    /// The native process-global transition, exposed with the other landscape diagnostics so
    /// a scene-replacement test can prove that installing a new scene did not reset it.
    #[must_use]
    pub fn environment_override_transition(&self) -> f32 {
        self.environment_override.snapshot().transition
    }

    /// The current in-game time, for the log line and the tests: `(year, day, time of day)`.
    #[must_use]
    pub fn game_time(&self) -> (u32, u32, f32) {
        (
            self.clock.current_year,
            self.clock.current_day,
            self.clock.present_time_of_day,
        )
    }

    /// Format date and time from the one `GameTime` this process has — the
    /// **same** object whose `present_time_of_day` drives the sky and the landscape lighting.
    ///
    /// The map's date and the day/night cycle share exactly one source, here.
    #[must_use]
    pub fn game_date_time(&self) -> Option<(String, String)> {
        self.clock.date_time_strings()
    }

    pub fn resolve_object_targets(&mut self, now: dereth_primitives::LocalTime) {
        crate::object_step::resolve_object_targets(
            &mut self.objects,
            &mut self.character,
            self.player_object,
            now,
        );
    }

    /// The object a server object is stuck to, if any, for a test.
    #[must_use]
    pub fn server_object_sticky(&self, id: ObjectId) -> Option<ObjectId> {
        crate::object_step::server_object_sticky(&self.objects, id)
    }

    /// Whether a server object's stick has had a `TargetInfo` yet.
    #[must_use]
    pub fn server_object_sticky_initialized(&self, id: ObjectId) -> bool {
        crate::object_step::server_object_sticky_initialized(&self.objects, id)
    }

    /// The movement target and the arm it is running.
    #[must_use]
    pub fn server_object_move_to(
        &self,
        id: ObjectId,
    ) -> Option<(bool, dereth_animation::table::MovementType, ObjectId)> {
        crate::object_step::server_object_move_to(&self.objects, id)
    }

    /// What a server object's move-to manager is **doing**.
    #[must_use]
    pub fn server_object_move_to_state(
        &self,
        id: ObjectId,
    ) -> Option<(MotionCommand, MotionCommand, usize, bool)> {
        crate::object_step::server_object_move_to_state(&self.objects, id)
    }

    /// The **head** of a server object's move-to path.
    #[must_use]
    pub fn server_object_move_to_node(
        &self,
        id: ObjectId,
    ) -> Option<(dereth_animation::table::MovementType, f32)> {
        crate::object_step::server_object_move_to_node(&self.objects, id)
    }

    /// Everything the move-to manager aims at.
    #[must_use]
    pub fn server_object_move_to_aim(
        &self,
        id: ObjectId,
    ) -> Option<(dereth_primitives::Position, bool, f32, f32)> {
        crate::object_step::server_object_move_to_aim(&self.objects, id)
    }

    /// The extrapolation lead held for a server object's target.
    #[must_use]
    pub fn server_object_target_quantum(&self, id: ObjectId) -> Option<f32> {
        crate::object_step::server_object_target_quantum(&self.objects, id)
    }

    /// The last `TargetInfo` delivered to a server object's target-update handler.
    #[must_use]
    pub fn server_object_last_target_info(
        &self,
        id: ObjectId,
    ) -> Option<dereth_animation::motion::moveto::TargetInfo> {
        crate::object_step::server_object_last_target_info(&self.objects, id)
    }

    /// Target tracking's tick and delivery counts.
    #[must_use]
    pub fn server_object_targetting(&self, id: ObjectId) -> Option<(u32, u32)> {
        crate::object_step::server_object_targetting(&self.objects, id)
    }

    /// The number of motions pending for a server object.
    #[must_use]
    pub fn server_object_motions_pending(&self, id: ObjectId) -> Option<usize> {
        crate::object_step::server_object_motions_pending(&self.objects, id)
    }

    /// The parallel part-array table ledger.
    #[must_use]
    pub fn server_object_table_motions_pending(&self, id: ObjectId) -> Option<usize> {
        crate::object_step::server_object_table_motions_pending(&self.objects, id)
    }

    /// A server object's current turn command.
    #[must_use]
    pub fn server_object_turn(&self, id: ObjectId) -> Option<MotionCommand> {
        crate::object_step::server_object_turn(&self.objects, id)
    }

    /// The target a server object is watching.
    #[must_use]
    pub fn server_object_target(&self, id: ObjectId) -> Option<ObjectId> {
        crate::object_step::server_object_target(&self.objects, id)
    }

    /// Whether a server object is in a standing long jump.
    #[must_use]
    pub fn server_object_standing_longjump(&self, id: ObjectId) -> bool {
        crate::object_step::server_object_standing_longjump(&self.objects, id)
    }

    /// Where the scene currently has a server object.
    #[must_use]
    pub fn server_object_position(&self, id: ObjectId) -> Option<dereth_primitives::Position> {
        crate::object_step::server_object_position(&self.objects, id)
    }

    /// The last position the **server** named for an object.
    #[must_use]
    pub fn server_object_reported_position(
        &self,
        id: ObjectId,
    ) -> Option<dereth_primitives::Position> {
        crate::object_step::server_object_reported_position(&self.objects, id)
    }

    /// as `Self::sync_objects` last saw it.
    pub fn player_object_id(&self) -> Option<ObjectId> {
        self.player_object
    }

    /// The one composition, for one child: fed with the
    /// holder's live part frames.
    pub fn held_object_frame(
        &self,
        holder: ObjectId,
        location: u32,
        from_player: bool,
    ) -> Option<Frame> {
        if from_player {
            let c = self.character.as_ref()?;
            let d = c.driver();
            let holding = *d
                .part_array
                .setup
                .as_ref()?
                .holding_locations
                .get(&location)?;
            let parts: Vec<Frame> = d.part_array.parts.iter().map(|p| p.pos).collect();
            return Some(crate::models::child_frame(
                &c.render_frame(),
                &parts,
                &holding,
            ));
        }
        let h = self.objects.get(&holder)?;
        let holding = *h
            .sim
            .driver
            .borrow()
            .part_array
            .setup
            .as_ref()?
            .holding_locations
            .get(&location)?;
        let parts: Vec<Frame> = h
            .sim
            .driver
            .borrow()
            .part_array
            .parts
            .iter()
            .map(|p| p.pos)
            .collect();
        Some(crate::models::child_frame(&h.frame, &parts, &holding))
    }

    /// Mouse-look. With a body it orbits the chase camera; without one it is the flycam's own.
    pub fn look(&mut self, dx: f32, dy: f32) {
        if self.character.is_some() {
            self.camera_orbit.0 -= dx * crate::camera::MOUSE_SENSITIVITY;
            self.camera_orbit.1 = (self.camera_orbit.1 - dy * crate::camera::MOUSE_SENSITIVITY)
                .clamp(-crate::camera::PITCH_LIMIT, crate::camera::PITCH_LIMIT);
            self.follow_character();
        } else {
            self.camera.look(dx, dy);
        }
    }

    /// Snap the chase camera onto the body now — used after the body is teleported.
    /// How far behind the body the chase camera sits — `CameraState`'s viewer offset, whose
    /// default here is 4.5 m.
    ///
    /// **This is the *debug* chase camera, and it needs no production writer.** Nothing in
    /// the workspace writes `camera_distance`, and the tempting conclusion is that the
    /// client's zoom is unwired. It is not: `crate::camera::update_viewer` runs immediately
    /// after `WorldScene::update` in `App::frame` and **overwrites `self.camera` outright**
    /// with `CameraState`'s own frame, so in a running client with a body this field never
    /// reaches the drawn frame at all; and `CameraCommand::Closer` / `Farther` /
    /// `SetDefaultOffsets` have producers. What remains is a fixed 4.5 m
    /// placement used before the real camera replaces it, by the body-less flycam path, and
    /// by headless tests -- the degrade-exemption test pulls the camera back through here
    /// because it drives `WorldScene` directly and never calls `update_viewer`.
    ///
    /// The general caution: **"no writer" is a claim about one field, not about the behaviour
    /// that field appears to name.** The behaviour has a writer one layer up.
    pub fn set_camera_distance(&mut self, d: f32) {
        self.camera_distance = d;
    }

    /// The current chase distance.
    #[must_use]
    pub fn camera_distance(&self) -> f32 {
        self.camera_distance
    }

    /// Put the debug chase camera on the body and re-read the viewer's cell, for a caller
    /// that has just moved the body outside `Self::update` -- `App::load_pending_scene`
    /// after `Character::teleport`, and `App::player_teleport_use_time` after
    /// `apply_player_teleport`.
    ///
    /// # **Deliberately un-recentred, because a re-centre could not change the answer.**
    ///
    /// This is the second of the two arms that reach `Self::update_viewer_cell` with no
    /// `Self::recenter` before it; the other is the flycam placement in `Self::load`,
    /// where the same three counts are written out in full. In this arm the first count is
    /// even more direct than it is there: with a body, `Self::viewpoint_cell_id` reads
    /// `Character::camera.viewer`, which is a `Position` -- a cell id and a **cell-local**
    /// frame -- and `recenter` does not touch it. There is no block in that derivation for the
    /// re-centre to move. (The bodiless fallback is block-independent for the arithmetic
    /// reason given at `Self::load`.) The load-time viewpoint tests measure both.
    ///
    /// The second count is this arm's own: `recenter` is **not** a read-only step. It mints
    /// the frame's `RenderSpace`, shifts `self.camera.position` by the block delta and
    /// queues `LandblockWindow::update_block`'s streaming actions. A caller outside
    /// `Self::update` that re-centred would choose a *second* viewer block for the frame
    /// with nothing consuming its token -- neither `Self::place_local_body` nor
    /// `Self::advance_objects` runs here -- which is precisely the shape
    /// `Self::place_local_body` guards against: the body's parts left expressed in the block
    /// the frame is leaving. Choosing a viewer block is what supplies the placement token.
    ///
    /// And the value is superseded before it is drawn in any case. `App::frame` runs
    /// `player_teleport_use_time` and `load_pending_scene` inside its `WorldViewStep` step
    /// and `world.update` later in the *same* step, so the ordered pair
    /// `recenter(); ..; update_viewer_cell()` recomputes the index before `DrawWorld`
    /// (`crate::frame::FrameStep::ORDER`, asserted by `dereth/testkit/tests/dat/frame.rs`). By count 1 it
    /// recomputes the same number -- which is what makes this a null with a mechanism rather
    /// than a coincidence.
    ///
    /// The frame loop's own order is a **separate** question, answered and pinned at
    /// `Self::viewpoint`; nothing here re-opens it.
    pub fn follow_character_now(&mut self) {
        self.follow_character();
        self.update_viewer_cell();
    }

    /// Place the chase camera behind and above the body.
    ///
    /// This intermediate debug chase-camera construction is derived from the body and is unswept.
    /// The helper returns without a character; the frame loop calls it in its character-present arm.
    /// The production viewer then overwrites it with the swept sphere, smoothing and mouse-look
    /// dead zone from `crate::camera::update_viewer`, without inheriting this camera's wall crossing.
    pub fn follow_character(&mut self) {
        let Some(c) = self.character.as_ref() else {
            return;
        };
        let f = c.render_frame();
        // Frame heading is degrees, 0 = north (+y), increasing **clockwise**;
        // the debug camera's yaw is radians counter-clockwise from north, so it negates.
        let heading = dereth_animation::frame::get_heading(&f).to_radians();
        let yaw = self.camera_orbit.0 - heading;
        let pitch = self.camera_orbit.1;
        // Eye height: the setup's own `height` (1.835 m for the Aluvian male), so the target
        // is the head rather than the feet.
        let eye = c.height() * 0.9;
        let cam = FreeCamera::new(Vec3::ZERO, yaw, pitch);
        let d = cam.forward();
        self.camera = FreeCamera::new(
            Vec3::new(
                f.origin.x - d.x * self.camera_distance,
                f.origin.y - d.y * self.camera_distance,
                f.origin.z + eye - d.z * self.camera_distance,
            ),
            yaw,
            pitch,
        );
    }

    /// The local body's half of the re-centre: the chase camera **re-derived** in the new
    /// space and the body's parts **re-placed** in it. **It must run after
    /// `Self::recenter`.**
    ///
    /// # Why the local body needs this and the client does not
    ///
    /// Part updates apply the part array to the object's own position frame -- the
    /// object's own **cell-local** frame. Drawing applies the viewer-block origin by
    /// pushing a `Position` carrying the block's cell id and an identity frame through
    /// the position initializer `(3, ..)`. Nothing a part placement writes is expressed
    /// relative to the viewer's block, so re-centering can run in either order with it.
    ///
    /// **This build folds that origin into the part frames instead** (so that the
    /// vertices stay block-local): `Character::render_frame` and `Self::render_frame_of`
    /// both add `(block - viewer) * BLOCK_LENGTH`. That is what creates an ordering
    /// constraint the client does not have -- anything writing a part frame or a camera has to
    /// run **after** the re-centre, because the space it writes in is the one the re-centre
    /// moves.
    ///
    /// `Self::advance_objects` is on the correct side of it and says so at its own
    /// re-derivation. The local body needs this: `Character::update` runs *before*
    /// `recenter`, so parts placed there would, on a landblock crossing, sit one landblock per
    /// axis of shift behind the camera -- exactly `(-192 * dx, -192 * dy, 0)`, 192 m at a
    /// single crossing, which is off screen -- and the player's own body would vanish for one
    /// frame at every boundary.
    ///
    /// **The camera has the same problem.** `Self::follow_character` runs before `recenter`
    /// too, and patching its result by subtracting the block shift is not exact:
    /// `(v + 192k) - 192k` is not the identity in `f32`: at a four-block hop the
    /// addend is 768, where the spacing is 6.1e-5 against 7.6e-6 at the camera's own
    /// magnitude, and three low bits are lost -- a frame-wide sub-LSB haze. Re-deriving here
    /// costs one `follow_character` and removes the subtraction from the answer. (`recenter`
    /// still shifts the camera, because the body-less flycam owns its own position and has
    /// nothing to be re-derived from.)
    ///
    /// **Unconditional, like `advance_objects`, and not gated on a non-zero shift.** A gate
    /// would leave the correcting path unexercised on every frame but a crossing, which is the
    /// shape that rots; and the placement is idempotent, so re-running it on a frame that did
    /// not move the window writes the same frames back.
    ///
    /// **It is the only part placement of the body per frame.** `Character::update` does not
    /// place the parts: the placement assigns `parts[i].pos` outright from the frame it is
    /// handed, so an earlier write would be dead on every frame and *wrong* on a crossing one.
    /// `Character::place_parts` is what a `Character` driven on its own has to call instead.
    ///
    /// **It takes the frame's `RenderSpace`.** The token can only have come from
    /// `Character::set_viewer_block`, which is `Self::recenter`'s own act, so this function
    /// cannot be called before the re-centre — not by convention, by the type.
    pub fn place_local_body(&mut self, space: RenderSpace) {
        if self.character.is_none() {
            return;
        }
        self.follow_character();
        let Some(c) = self.character.as_ref() else {
            return;
        };
        // Re-derived rather than stored, for the reason `advance_objects` gives at its own
        // loop: a window scroll moves the origin of the space these frames are expressed in.
        if self.smooth_animation {
            c.place_parts_between(space);
        } else {
            c.place_parts(space);
        }
    }

    /// Apply-lighting's object half:
    /// look up the physics object by id, and on a hit apply the
    /// `dereth_animation::parts::LightingMode` switch onto that physics body's part array
    /// (`dereth_animation::parts::PartArray::apply_lighting`). The local body is in the
    /// object table too — it is a physics body like any other — so a click on
    /// yourself lights the body. Answers whether an object was found.
    pub fn apply_object_lighting(
        &mut self,
        id: ObjectId,
        mode: dereth_animation::parts::LightingMode,
    ) -> bool {
        if let Some(o) = self.objects.get(&id) {
            o.sim.driver.borrow_mut().part_array.apply_lighting(mode);
            return true;
        }
        if let Some(c) = self.character.as_ref().filter(|c| c.object_id() == id) {
            c.driver_mut().part_array.apply_lighting(mode);
            return true;
        }
        false
    }

    /// Each part's `(luminosity, diffuse)` of one object, as `draw_part` will bind it this
    /// frame — `None` for an id the scene does not draw. A probe for tests.
    #[must_use]
    pub fn object_part_lighting(&self, id: ObjectId) -> Option<Vec<(f32, f32)>> {
        if let Some(o) = self.objects.get(&id) {
            return Some(o.sim.driver.borrow().part_array.part_lighting());
        }
        self.character
            .as_ref()
            .filter(|c| c.object_id() == id)
            .map(|c| c.driver().part_array.part_lighting())
    }

    /// The cell the viewer is standing in, interior or not — for the log line.
    #[must_use]
    pub fn viewer_cell_id(&self) -> Option<CellId> {
        self.character.as_ref().map(|c| c.position().cell)
    }

    /// The cell the viewer is standing in, when it is an interior one.
    ///
    /// The outdoor test is `(viewer.objcell_id & 0xFFFF) < 0x100`, and `viewer` means the
    /// **camera**, not the body. This therefore reads the camera cell written by the swept-sphere
    /// update.
    ///
    /// Reading the *body's* cell instead is wrong now that the camera is swept against the
    /// walls, and the difference is not cosmetic here: which portals open, and which
    /// interior is drawn through them, is a question about where the *camera* is. The two
    /// genuinely differ at exactly the boundaries the portal machinery is about — the camera in
    /// the room while the body stands in the doorway, or the camera outside a wall the body is
    /// inside.
    ///
    /// **A null outside-view pointer falls back to the body's cell, and that is the
    /// client's own fallback rather than a convenience.** `update_viewer`'s last resort, when
    /// neither the sweep nor the position adjustment finds a camera position, is
    /// setting the viewer to the player's position with reset, then clearing `viewer_cell` — so
    /// the viewer *is* the body, and the normal-mode render reads the body's `objcell_id` on the
    /// very next line. A null `viewer_cell` does not skip the world; for an indoor player the
    /// body cell supplies the fallback instead.
    ///
    /// `None` therefore means "the indoor path does not run": no body at all
    /// (the `--no-character` flycam), or a viewer that is outdoors.
    #[must_use]
    pub fn viewer_cell(&self) -> Option<CellId> {
        let c = self.character.as_ref()?;
        let cell = c.camera.viewer_cell.unwrap_or_else(|| c.position().cell);
        (!dereth_physics::landdefs::is_outdoors(cell)).then_some(cell)
    }

    /// The render-space point `traversal_cells` measures every portal against -- the
    /// instrument for "the two arrivals put the viewpoint in different places".
    #[must_use]
    pub fn camera_position(&self) -> Vec3 {
        self.camera.position
    }

    /// The smart-box dispatcher's object-movement update on the local body -- the accepted
    /// player movement consumer at the application's ordered object dispatch. No physics sweep
    /// runs here; the caller consumes the control-loss latch
    /// ([`Self::take_player_movement_applied`]) straight after, before a later teleport in the
    /// same batch can disable autorun or cancel the approach. It lives here rather than on the
    /// scene because it reads and writes nothing else.
    pub fn dispatch_player_movement(
        &mut self,
        buf: &dereth_protocol::movement::MovementBuffer,
        stream: &ObjectStream,
    ) {
        if crate::movement::apply_player_movement(&self.objects, &mut self.character, buf, stream) {
            self.player_movement_applied = true;
        }
    }
}
