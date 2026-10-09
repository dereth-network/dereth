//! The objects the server puts in the world.
//!
//! **This module wires; it does not implement.** What each message *means* belongs elsewhere and
//! is called from here by name:
//!
//! | Decision | Whose |
//! |---|---|
//! | the object tables, the create/merge/recreate rules, the destruction queue | [`dereth_client_model::World`] |
//! | the create/remove stream's own gate | [`dereth_client_net::client_session`]'s world-object dispatch |
//! | decoding `0xF745`, `0xF747`, `0xF748`, `0xF74C` and the movement buffer | [`dereth_protocol`] |
//! | what a movement buffer does to an object's animation | [`dereth_animation`]'s `MotionInterp` |
//!
//! What is this module's own is this file: the **loop** that drains the session, hands each decoded
//! message to the crate that owns it, and closes [`dereth_client_net::client_session::Session::object_arrived`] — the
//! seam whose caller is the application that owns both halves.
//!
//! # The seam
//!
//! `0x0013 Login_PlayerDescription` and every ordered event about an object arrive **parked on that
//! object**, and are released only when the object exists. The releasing call is
//! [`dereth_client_net::client_session::Session::object_arrived`], and it can only be called for a
//! real object if `0xF745` for an *unknown* object takes the **create path** rather than going
//! through the instance gate, which would park it. The session dispatch handles that case, so no
//! stand-in for the player alone is needed.
//!
//! # What this deliberately does not do
//!
//! Remote objects follow server positions rather than client prediction. Their physics bodies are
//! still synchronized and stepped so that queued interpolation, cell membership, and collision
//! state remain current. Their *animation* also advances as local playback of a server-chosen
//! motion rather than prediction of position. See `crate::world_state::WorldState`'s object step for
//! both updates.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod physics_setup;

use dereth_client_model::World;
use dereth_client_net::client_session::{SessionEvent, SessionState};
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LandblockId, LocalTime, ObjectId, Position, ServerTime};
use dereth_protocol::movement::{
    MovementBody, MovementBuffer, MovementPositionAndMovementEvent, MovementPositionEvent,
    MovementSetObjectMovement, MovementVectorUpdate,
};
use dereth_protocol::objects::{
    is_newer, EffectsPlayScriptId, EffectsPlayScriptType, EffectsSoundEvent, InventoryPickupEvent,
    ItemCreateObject, ItemDeleteObject, ItemObjDescEvent, ItemParentEvent, ItemSetState,
    ItemUpdateObject,
};
use dereth_protocol::{read_body_padded, Opcode, Reader};

/// One server-ordered physics-script trigger, waiting for the frame that holds the part arrays.
///
/// The two opcodes reach the *same physics body* through different entry
/// points: one names a physics-script **DataID** and consults no table at all,
/// while the other names a script type and intensity that must be resolved against the object's own
/// physics-script table — which the object may not have. Collapsing the two into one id at this
/// seam would need the table here, and the table is the renderer's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptEvent {
    /// `0xF755 Effects_PlayScriptType` names a script type and intensity.
    Type {
        id: ObjectId,
        script_type: i32,
        intensity: f32,
    },
    /// `0xF754 Effects_PlayScriptID` names a script `DataID` directly.
    Id { id: ObjectId, script: DataId },
}

/// What the object stream has done, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectStats {
    /// Null placeholders holding parked WorldObjects blobs that
    /// object maintenance destroyed at their scheduled
    /// deadline, because the object they were waiting for never arrived.
    ///
    /// A non-zero value is normal and is the bound working: it is one per id the server spoke
    /// about and then stopped (ACE's silent `handle_visible_obj` expiry, a landblock reload's new
    /// guids, a body this client culled while the server was still broadcasting). It is counted
    /// rather than hidden because the alternative reading — a create that was on its way and was
    /// thrown out — is the same event seen from the other side, and only a number can tell how
    /// often it happens.
    pub parked_blob_owners_destroyed: u64,
    /// `0xF745` that created an object.
    pub creates: u64,
    /// `0xF745` for an object already present — the in-place merge.
    pub merges: u64,
    /// `0xF7DB`.
    pub recreates: u64,
    /// A create the instance gate refused as older than what we hold.
    pub stale_instances: u64,
    /// `0xF747`.
    pub removes: u64,
    /// `0xF748` accepted by the position gate and not rolled back for an older teleport stamp.
    pub position_updates: u64,
    /// `0xF748`s that took an object **out of a container** -- it had neither a
    /// position nor a parent before the message and has a position after it -- and were therefore
    /// re-offered to the renderer through [`ObjectStream::take_created`].
    ///
    /// This is a world **drop**, and in the recorded corpus it is 0: no capture contains one.
    /// The counter exists so that the arm can be seen to have been taken rather than assumed;
    /// a zero count alone does not establish coverage.
    pub container_exits_offered: u64,
    /// `0xF748` refused by the `POSITION_TS` gate itself.
    pub stale_positions: u64,
    /// `0xF748` whose newer `POSITION_TS` was restored because `TELEPORT_TS` was older.
    /// Separate from stale positions: rejecting this packet must leave its position stamp reusable.
    pub position_teleport_rollbacks: u64,
    /// `0xF748`s that advanced `update_times[TELEPORT_TS]`, i.e. that moved the
    /// teleport echo every subsequent `0xF61C` and `0xF753` carries. The denominator is
    /// [`Self::position_updates`].
    pub teleport_stamps: u64,
    /// `0xF748`s that advanced the **player's** force-position stamp, `update_times[6]`.
    /// **0 across the whole corpus**, and that zero is the measurement rather than an absence: the
    /// echo is 0 in all 1,827 recorded bodies precisely because nothing ever advanced it.
    pub force_position_stamps: u64,
    /// `0xF748`s about the **player**, whatever they then do — the denominator of
    /// [`Self::player_teleports`], counted at the same place so the two cannot drift apart.
    pub player_positions: u64,
    /// `0xF748`s that advanced the **player's** `update_times[TELEPORT_TS]` and so
    /// triggered player teleport — the server moving the player's own body.
    ///
    /// The denominator is every `0xF748` about the player: **1,826** of the corpus's 2,254, of
    /// which exactly **6** are teleports (early-inventory-and-casting 1, long-solo-play 4,
    /// short-play-with-training 1), each preceded in the same millisecond by a `0xF751
    /// Effects_PlayerTeleport` carrying the same new sequence. So this is an **edge**, not a
    /// per-message fact, which is why a build that ignored it looked healthy for 1,820 messages out
    /// of 1,826.
    pub player_teleports: u64,
    /// `0xF74C` whose buffer decoded.
    pub movement_updates: u64,
    /// `0xF74C` whose buffer did not decode. A non-zero count is a layout bug, not bad input.
    pub movement_undecodable: u64,
    /// `0xF74C` refused because `MOVEMENT_TS` was not newer than what we hold.
    pub movement_stale: u64,
    /// `0xF74C` refused because our server-controlled-move stamp is ahead of the message's.
    pub movement_old_control: u64,
    /// `0xF74C` about the player, marked autonomous: the server echoing what he just sent.
    pub movement_own_echo: u64,
    /// `0xF74E Movement_VectorUpdate` that passed the vector-update
    /// `VECTOR_TS` gate and reached `set_velocity`/`set_omega`.
    pub vector_updates: u64,
    /// `0xF74E` for an object this table does not hold. The session's gate parks a vector update
    /// on an unknown object exactly as the missing-object branch queues its blob for the
    /// object, so this only counts an object the world tables refused.
    pub vector_unknown: u64,
    /// `0xF74E` refused because the incoming vector-update stamp was not newer than
    /// the retained stamp. **Equality is stale**: the test is `is_newer`, not `>=`.
    pub vector_stale: u64,
    /// `0xF74E` **about the player**, refused because position-from-server is false —
    /// `autonomy_level == 2`, which is the constructor's value at
    /// construction and therefore the state a client is in unless a command list has taken control.
    ///
    /// The identity comparison runs this arm **only** for the player, and note
    /// where it sits: **after** the timestamp gate has already written the stamp. So a refused player
    /// vector update still advances `VECTOR_TS`, and the next one is measured against it.
    pub vector_player_autonomous: u64,
    /// `0xF619 Movement_PositionAndMovementEvent` whose position half was applied
    /// and whose movement half was handed on — the whole message, both limbs.
    pub position_and_movement_events: u64,
    /// `0xF619` whose trailing movement buffer did not decode. A layout bug, not bad input — and
    /// the one number that would show the layout boundary to be wrong.
    pub position_and_movement_undecodable: u64,
    /// Movement buffers carried **inside** a create's `PhysicsDesc` rather than in a `0xF74C`.
    /// **509 of the corpus's 837 creates** have a non-empty one and all 509 decode and consume
    /// exactly; it is the object's opening animation. Measured over the whole-message opcode space
    /// of the seven-session corpus (`fixtures/message-corpus/…`, 11,245 blobs) and asserted by
    /// the client-net session tests. (The three-session corpus gave 219 of 375.)
    pub create_movements: u64,
    /// A create's embedded buffer that did not decode. A layout bug, not bad input.
    pub create_movement_undecodable: u64,
    /// `0xF625 Item_ObjDescEvent` — the server changing an object's appearance after the create.
    ///
    /// **Never exercised by the corpus**: none of `fixtures/packet-captures`' three sessions equips,
    /// unequips or dyes anything, so all 328 descriptions arrive on a `0xF745` and this stays 0.
    /// The appearance-event handler is the other half of
    /// the same seam and leaving it out would make the first dye in play a silent no-op; it has no
    /// oracle behind it and says so.
    pub objdesc_events: u64,
    /// A `0xF625` for an object the tables do not hold. The session's gate parks one for an
    /// unknown object, so this counts only what the world tables refused.
    pub objdesc_events_unknown: u64,
    /// `0xF749 Item_ParentEvent` applied — a creature picked an item up into a hand.
    pub parent_events: u64,
    /// A `0xF749` refused by the parent-event gate: the item's `POSITION_TS` is
    /// not older than the pack's second `u16`.
    pub parent_events_stale: u64,
    /// A `0xF749` naming an item the tables do not hold. Parent-event handling queues the
    /// blob on the **item** id in that case, and the session's gate parks only on the
    /// leading guid, which is the creature — so this is the arm the session cannot cover.
    pub parent_events_unknown_item: u64,
    /// A `0xF749` naming a creature the tables do not hold. Normally the session's gate catches
    /// this first, so a non-zero count means the world tables refused the creature.
    pub parent_events_unknown_creature: u64,
    /// `0xF74A Inventory_PickupEvent` applied — the server saying the object has left the 3-D
    /// world. Pickup handling passed its `POSITION_TS` gate:
    /// `unset_parent` then `leave_world`, with the object *staying* in the tables.
    pub pickup_events: u64,
    /// A `0xF74A` refused by the pickup-event gate — the pack's second `u16` is
    /// not newer than the object's `POSITION_TS` under `is_newer`.
    pub pickup_events_stale: u64,
    /// A `0xF74A` naming an object the tables do not hold. Pickup-event handling queues
    /// the blob on that id and the session's gate does the same, so a non-zero count means the world
    /// tables refused an object the session let through.
    pub pickup_events_unknown: u64,
    /// A child attached while applying a **holder's** create listing:
    /// a child that has already arrived.
    pub children_attached: u64,
    /// A descriptor child entry naming an object the tables do not hold. Applying the child
    /// list records a null placeholder edge so the holder's declared child order survives; the
    /// child's own create later replaces that placeholder with the live edge.
    pub children_unknown: u64,
    /// A child detached by the child-unparenting pass, which child-list replacement runs
    /// **before** it reads the descriptor's list.
    pub children_unparented: u64,
    /// A parent assignment this stream **refused**, because child attachment could not
    /// validate the holding location against the parent's part array.
    ///
    /// A stream built by [`ObjectStream::new`] has no [`RetailDatStore`], so
    /// setup lookup answers `None` for every setup and *every* link lands here.
    /// That is the assetless contract, and it is silent: the objects still arrive, the inventory
    /// lists still fill, and only the physics parent edge is missing. This once reduced three
    /// capture-replay suites to their denominator guards, so it is counted: a replay that expects
    /// held items and reads a non-zero here is holding an
    /// assetless stream and wants [`ObjectStream::with_store`].
    pub parent_links_refused: u64,
    /// `0xF750 Effects_SoundEvent` messages decoded and parked.
    pub sound_events: u64,
    /// `0xF755 Effects_PlayScriptType` messages decoded and parked.
    pub script_type_events: u64,
    /// `0xF754 Effects_PlayScriptID` messages decoded and parked.
    pub script_id_events: u64,
    /// `0xF74B Item_SetState` applied to a presence.
    ///
    /// State updates accepted past the `update_times[STATE_TS]` gate, which is what
    /// changes a door from solid to `ETHEREAL_PS` and back, and what hides an object the server
    /// wants hidden. The corpus carries **57**, of which **35** are about an object other than the
    /// player.
    pub state_events: u64,
    /// A `0xF74B` refused by the state update's `STATE_TS` gate — the stamp is not newer
    /// than the one we hold, under `is_newer`'s half-period rule.
    pub state_events_stale: u64,
    /// A `0xF74B` naming an object the tables do not hold. State-event handling queues the
    /// blob on that id, and the session's gate does exactly that, so a non-zero count here means
    /// the world tables refused the object after the session let it through.
    pub state_events_unknown: u64,
    /// A `0xF74B` this table applied whose object `dereth_client_model`'s physics table does not hold, so
    /// the single source of the state word (`PhysicsPresence::state`) took no write.
    ///
    /// It has its own counter rather than being folded into `state_events` because the whole
    /// point of the wire is that the message reaches the table the visible-object list update reads: a
    /// build where every `0xF74B` landed here and nowhere else would look identical in
    /// `state_events` and would still be broken. Non-zero means the two tables
    /// disagree about which objects exist — `create_or_merge`'s `StaleInstance` refusal is the
    /// one path that can produce that.
    pub state_events_without_physics: u64,
    /// A WorldObjects message this stream has no handler for. Counted rather than logged: the
    /// remaining effects and script messages are handled elsewhere.
    pub unhandled: u64,
    /// `0xF6EA Object_SendForceObjdesc` sent by descriptor recovery or by
    /// the object manager's 20-second null-table sweep.
    ///
    /// It has its own counter rather than sharing `unhandled` because it is the client's only
    /// recovery path for a dangling object reference, and a build in which it is silently zero
    /// looks exactly like a build with nothing to recover. The corpus's own answer is **35** over seven sessions.
    pub objdesc_asks: u64,
    /// One the sender refused (no session, or an encode failure). Non-zero means the seam is
    /// broken again.
    pub objdesc_asks_undeliverable: u64,
    /// Landblocks whose interior cells were flushed by the streaming window —
    /// driven from `WorldScene::release_block_interiors`.
    pub blocks_flushed: u64,
    /// Objects that left visibility because the cell they stood in unloaded.
    /// Leaving object visibility releases the object's cell.
    ///
    /// It has its own counter rather than sharing [`Self::removes`] because the two are different
    /// events with different causes: a `0xF747` is the *server* taking an object away, and this is
    /// the *client* unloading the ground it stood on. A build where this is permanently zero while
    /// blocks are flushed has the release path unreachable, and it looks exactly like a build with
    /// nothing to release.
    pub objects_left_visibility: u64,
}

/// The facts about one object that the renderer needs and that no crate holds.
///
/// `dereth_client_model::PhysicsPresence` keeps the cell, the state word and the parent — the *gameplay*
/// facts — while the synchronized physics body holds pose and collision state. `Presence` is the
/// rendering projection of descriptor fields plus the `update_times[]` slots that gate the
/// messages this stream handles.
///
/// **There is no `state` field here, and that is the point.**
///
/// The original client has exactly one state word per object; state updates and all other consumers
/// read and write that word. This build splits the object across two crates and `dereth-client-model` cannot
/// depend on `dereth-client`, so the word has to live where **both** sides can reach it: it is
/// [`dereth_client_model::objects::PhysicsPresence::state`], and every consumer in this crate reads it
/// through [`ObjectStream::physics_state`] or straight off [`ObjectStream::world`].
///
/// Do not add a *projection* here — a second field written only by reading the game table back
/// out. A projection cannot be proved to stay a projection by a test that only samples the
/// stations it happens to know about; not having the storage is the assertion.
#[derive(Debug, Clone)]
pub struct Presence {
    /// `PhysicsDesc.setup_id`, the setup record the object draws with.
    pub setup_id: Option<DataId>,
    /// `update_times[STATE_TS]` — `[2]`.
    ///
    /// Description setup closes by copying **all nine** of the
    /// descriptor's timestamps into `update_times`, so a create seeds this exactly as it seeds
    /// [`Self::position_ts`]; the handler then compares an incoming
    /// `0xF74B`'s second `u16` against it, writes the stamp, and
    /// applies the word only when the message wins.
    pub state_ts: u16,
    /// `PhysicsDesc.mtable_id`. Without one the object stands in its placement frame for ever,
    /// which is correct for a chest and wrong for a mosswart.
    pub mtable_id: Option<DataId>,
    /// `PhysicsDesc.stable_id` — the sound table the **server** put on this object.
    ///
    /// Sound playback resolves a `SoundType` against the object's sound table.
    /// Description setup prefers the descriptor's `stable_id` over the setup's default
    /// whenever the descriptor carries one.
    ///
    /// It is not a corner case. Over the seven recorded captures, **all 38 creates of the 34
    /// objects the server ever sends a `0xF750` about carry a `stable_id`**, as the
    /// client-net session tests assert. A build that resolved the table from the setup
    /// alone would be reading the wrong table for every sounded object in the corpus and would
    /// still pass a test that only counted triggers.
    pub sound_table: Option<DataId>,
    /// `PhysicsDesc.phstable_id` — the physics-script table the **server** put on this object.
    ///
    /// The exact analogue of [`Self::sound_table`] one field up, and it matters more:
    /// description setup releases and reloads *both* tables, but
    /// only **6 of the 5,935 shipped setup records** carry a non-zero `default_phstable_id`, and the
    /// Aluvian male body — the player — is not one of them. So for essentially every object the
    /// script table comes from the wire and nowhere else, and without this field
    /// playing a script returns 0 for want of a table no matter how correct everything downstream
    /// of it is.
    pub phs_table: Option<DataId>,
    /// Which of the setup's placement frames the object is posed by.
    ///
    /// Part-array setup and final object initialization install
    /// [`crate::models::PLACEMENT_RESTING`] (`0x65`), which is the default here, but the placement
    /// is **not** fixed for the life of an object: the placement-frame setter
    /// installs whatever id it is handed, and the server hands it one from two of the three
    /// messages measured against the recorded corpus:
    ///
    /// * a create's `PhysicsDesc` **animframe** field (`0x00020000`) — description setup reads it,
    ///   but only when the
    ///   descriptor carried no movement buffer, which is the same `if` the two wire fields are
    ///   mutually exclusive under;
    /// * the position packet's placement identifier (`HAS_PLACEMENT_ID 0x0002`) — received-position handling
    ///   installs it after `unset_parent` and only when the object has no animations playing.
    ///
    /// The third is `0xF749 Item_ParentEvent`, whose placement this build does not apply.
    pub placement: u32,
    /// A placement a **position event** named, waiting for the draw to test whether any animation
    /// is playing.
    ///
    /// The two messages do not agree on their guard and the difference is retail's:
    /// `set_description`'s only condition is "no movement buffer", while
    /// received-position handling checks whether the animation list is empty and installs the id
    /// **only when it is empty**,
    /// because the placement frame is what the sequence controller uses when nothing is playing.
    ///
    /// That list lives on the part array, which this crate does not hold: a remote object's
    /// `MotionDriver` belongs to the renderer. So the id is parked here and the draw applies the
    /// test, exactly as [`Self::pending_movement`] parks a movement buffer for the same reason.
    pub pending_placement: Option<u32>,
    /// from `PhysicsDesc.object_scale`.
    pub scale: f32,
    /// **The object's one position.**
    ///
    /// `None` for a contained or wielded object, which has no position of its own —
    /// `pd.position.objcell_id == 0` is the pickup case, and leaving the world zeroes
    /// `position.objcell_id` for exactly that.
    ///
    /// # Why this is the body's position and not the wire's
    ///
    /// In retail there is **one** position per object, and every consumer reads it.
    /// Selection and radar conversion use the object and player positions, checking the
    /// object's cell for the missing-position case. The selection range watch uses those
    /// same two positions. Distance calculations with object radii and sound emission use
    /// the body's position too, and drawing copies its frame to the part array.
    ///
    /// **And the `0xF748` handler stores the wire position nowhere.** It builds the position as a
    /// temporary from the cell id and frame, hands that to
    /// move-or-teleport, and then reads the **body** back for the one
    /// thing it does next:
    ///
    /// If move-or-teleport returns zero, the handler stops. Otherwise it constrains the
    /// physics object using the body's current position, rather than the wire local.
    ///
    /// So the wire position dies with the function and the only position a later reader can see is
    /// `position`. Selection keeps no position either: it
    /// writes the selected id, the previously selected id and the weenie's selected flag and
    /// nothing else.
    ///
    /// This field is therefore the **achieved** position: seeded from the wire by the create and
    /// by `0xF748` (which is what `position` is until physics runs), and then republished from
    /// the live physics body by [`ObjectStream::publish_physics_cells`] on every frame the body
    /// steps. The last position the *wire* named is [`Self::server_position`], which is a
    /// **target** and not a second copy of this.
    pub position: Option<Position>,
    /// The last position the **wire** named — `MoveOrTeleport`'s argument, i.e. the *target*.
    ///
    /// Retail's copy of this is the stack local quoted on [`Self::position`] plus, once
    /// `MoveOrTeleport`'s `InterpolateTo` arm has run, the node
    /// interpolation queued — never a field on the object. Here it has to
    /// outlive the message because [`crate::object_physics::ObjectPhysics::sync`] runs a frame
    /// later than `apply_event` and completes the client's received-position handling: it is the only
    /// reader, it uses it for `move_or_teleport`'s destination and for the `placed` memo that
    /// suppresses a redundant second `InterpolateTo`, and nothing else in the client may read it.
    ///
    /// It is the same split `crate::world::SceneObject::server_position` already carries for the
    /// scene object, for the same reason and under the same name.
    pub server_position: Option<Position>,
    /// `PhysicsDesc.parent` — the object this one is attached to, and the **`ParentLocation`** it
    /// is attached at. The create path enters the world only for
    /// `pd.parent_id == 0`, and collision checking steps over a shadow
    /// whose physics-body parent is non-null, so a held object is not solid on its own account
    /// either way.
    ///
    /// The location word is the key requested from the **holder's** setup, and
    /// without it a held object cannot be placed.
    /// Object creation reads both off the descriptor and calls
    /// `set_parent(obj, parent, location)`, so a wielded item is attached by its own
    /// create and not only by a later `0xF749`.
    pub parent: Option<(ObjectId, u32)>,
    /// `update_times[INSTANCE_TS]`.
    pub instance: u16,
    /// `update_times[POSITION_TS]`.
    pub position_ts: u16,
    /// `update_times[MOVEMENT_TS]`.
    pub movement_ts: u16,
    /// The server-controlled-move stamp, `update_times[5]`.
    pub server_control_ts: u16,
    /// The vector-update stamp — timestamp slot `[3]`
    /// used by the vector update.
    ///
    /// Seeded from the descriptor by the create, like the other eight: create handling copies all
    /// nine slots, and `PhysicsDesc` carries
    /// this one as [`dereth_protocol::types::PhysicsTimestamps::vector`]. Without this field the
    /// slot would be decoded on every create and dropped.
    pub vector_ts: u16,
    /// The velocity and angular velocity a `0xF74E` asked for and the renderer has not applied
    /// yet, in the shape [`Self::pending_movement`] has and for the same reason: velocity and
    /// angular-velocity setters belong to the physics object, which lives in
    /// `dereth_physics`'s world and not in this table.
    pub pending_vector: Option<(dereth_primitives::Vec3, dereth_primitives::Vec3)>,
    /// `update_times[TELEPORT_TS]` — `[4]`.
    ///
    /// This is the **fourth** of the four sequences every client-to-server movement pack echoes
    /// back; every movement-event sender passes
    /// `update_times[8], [5], [4], [6]`, in that argument order). Measured over the 1,827
    /// recorded `0xF61C`/`0xF753` bodies, `teleport` is **non-zero in 1,481** and runs 0..=4, so
    /// a producer that hard-wired it to 0 would echo a value the client does not hold in four
    /// position reports out of five.
    ///
    /// Its writers are the two call sites plus the create:
    /// creation seeds it from the descriptor along with all nine slots, and a `0xF748` position
    /// update advances it. The `0xF751` teleport handler does **not** write it; it only compares
    /// against it and
    /// sets `waiting_for_teleport`, so the teleport announcement never advances the echo.
    pub teleport_ts: u16,
    /// The force-position stamp, `update_times[6]`.
    ///
    /// The third of the four echoes. Forced-position handling runs
    /// `newer_event(player, 6, ts)` on this slot **before** the `POSITION_TS` gate and only for
    /// the player, which is the "blip" arm: a force-position the client has not seen yet is
    /// applied without interpolation and answered with a position event of its own.
    ///
    /// It is 0 in **all 1,827** recorded bodies, so a hard-wired 0 would pass the corpus for
    /// this half — which is exactly why it is worth wiring rather than leaving: an echo that is
    /// accidentally correct on the corpus is indistinguishable from one that is correct, and this
    /// field is the control that tells the two apart when the other one moves.
    pub force_position_ts: u16,
    /// **The two facts move-or-teleport branches on.**
    ///
    /// Received-position handling supplies the destination, teleport decision, contact flag, and
    /// velocity for a non-player object, and the
    /// callee's first two tests are the ones this pair carries: whether the update advanced
    /// `TELEPORT_TS`, and `PositionPack`'s `has_contact` flag (`(flags >> 2) & 1`,
    /// [`dereth_protocol::movement`]). Both describe [`Self::position`] —
    /// the update it arrived with — so they are replaced by every accepted `0xF748` and never
    /// need clearing.
    ///
    /// The teleport half is the *answer* `received_position` already computes for the player's
    /// player-teleport arm; it is stored rather than re-derived because the physics body keeps no
    /// timestamp table for a remote object and `newer_event` cannot be asked twice.
    pub teleported: bool,
    /// The `has_contact` half of the pair above.
    pub contact: bool,
    /// Whether [`Self::position`] was written by a create — a `0xF745` for an object the client
    /// did not hold, a newer instance, or a `0xF7DB` — rather than by received-position handling.
    ///
    /// Create handling is the only caller of world entry. An equal-instance re-create is not a
    /// create here: the client already holds the object, and create handling hands the
    /// description's position to received-position handling, exactly as a `0xF748`'s is handed,
    /// so a returning player's re-create — which is how ACE re-tracks somebody who walked back
    /// into view — reaches move-or-teleport and is never placed again.
    pub position_from_create: bool,
    /// The last movement buffer the server sent that the renderer has not applied yet.
    pub pending_movement: Option<MovementBuffer>,
    /// The object description — part swaps, texture-map swaps, and the shift palette.
    ///
    /// Object-description unpacking opens by clearing the description, so this always *replaces* rather than
    /// merges, whether it arrived on a `0xF745` or on a `0xF625`.
    pub objdesc: dereth_protocol::types::ObjDesc,
}

/// One position as received-position handling takes it: the destination, the placement to
/// install when no animation is playing, the ground-contact flag move-or-teleport reads, and the
/// three stamps the handler gates on.
///
/// A `0xF748` supplies all of it. An equal-instance `0xF745` re-create supplies its
/// description's position, its placement, the description's three position stamps and contact
/// set, and `from_create` says which of the two it was.
#[derive(Debug, Clone, Copy)]
struct ReceivedPosition {
    id: ObjectId,
    destination: Position,
    placement_id: Option<u32>,
    contact: bool,
    position_ts: u16,
    teleport_ts: u16,
    force_position_ts: u16,
    from_create: bool,
}

/// A presence that has heard nothing but its own creation: every field zero **except**
/// `placement`, which starts at the id installed by part-array setup and final object
/// initialization. A default of `0` here would be
/// `Placement.Default`, a pose the client does *not* draw at.
impl Presence {
    /// A position the **wire** named, which in retail is one write and here is two fields.
    ///
    /// Position-event handling and object creation both name
    /// a position that is at that instant both the target physics is asked for *and* the object's
    /// `position` — nothing has stepped yet, so there is nothing for the two to disagree about.
    /// Every such site goes through here so that they can only ever be seeded together, and the
    /// one writer of [`Self::position`] alone afterwards is
    /// [`ObjectStream::publish_physics_cells`], which republishes it off the live body.
    fn set_wire_position(&mut self, at: Option<Position>) {
        self.position = at;
        self.server_position = at;
    }
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            setup_id: None,
            state_ts: 0,
            mtable_id: None,
            sound_table: None,
            phs_table: None,
            placement: crate::models::PLACEMENT_RESTING,
            pending_placement: None,
            scale: 0.0,
            position: None,
            server_position: None,
            parent: None,
            instance: 0,
            position_ts: 0,
            movement_ts: 0,
            server_control_ts: 0,
            vector_ts: 0,
            pending_vector: None,
            teleport_ts: 0,
            force_position_ts: 0,
            teleported: false,
            contact: false,
            position_from_create: false,
            pending_movement: None,
            objdesc: dereth_protocol::types::ObjDesc::default(),
        }
    }
}

/// [`dereth_client_model::World`] plus the render facts, driven by the session's event stream.
#[derive(Debug)]
pub struct ObjectStream {
    /// The world tables. Public because the frame loop and the tests read them, exactly as the
    /// client's object-maintenance tables are globally reachable.
    pub world: World,
    store: Option<Arc<RetailDatStore>>,
    presences: BTreeMap<ObjectId, Presence>,
    /// Objects created since the renderer last synchronised, in arrival order.
    created: Vec<ObjectId>,
    /// Objects removed since then.
    removed: Vec<ObjectId>,
    /// Broadcasts for the owning interaction/UI subscribers, drained even without a connection.
    notices: Vec<dereth_client_model::Notice>,
    /// Requests synchronously raised while one admitted object message is applied. App drains
    /// these before the next message, matching the direct description-update send.
    pending_requests: Vec<dereth_client_model::Request>,
    /// Old-instance Session cleanup edges. Pump drains each edge before the next arrival.
    session_deleted: Vec<ObjectId>,
    /// Failed physical initialization retires only physical parked bytes, not the new Weenie.
    session_physics_deleted: Vec<ObjectId>,
    /// Explicit accepted remote `0xF748` owner edges, not POSITION_TS changes: parent/pickup
    /// share that slot. Per-message production sync consumes the marker once on the actual
    /// surviving body; coarse snapshot callers retain their existing batching limitation.
    position_entries: BTreeSet<ObjectId>,
    /// `0xF750 Effects_SoundEvent`s decoded since the frame loop last drained them, in arrival
    /// order.
    ///
    /// Parked rather than played for the same reason [`Presence::pending_movement`] is: the sound
    /// plays at the emitting object's own `position`, which is the renderer's, and this crate
    /// holds no rendered frame. [`ObjectStream::take_sound_events`] is the drain.
    sound_events: Vec<EffectsSoundEvent>,
    /// `0xF755` and `0xF754`s decoded since the frame loop last drained them, in arrival order.
    /// [`ObjectStream::take_script_events`] is the drain.
    ///
    /// Parked rather than played for the same reason [`Self::sound_events`] is, and for a second
    /// one: playing a script adds it to the script manager, whose particle-creation hook
    /// builds a particle emitter **on the object's
    /// part array** — and a remote object's part array is the renderer's `MotionDriver`, not
    /// anything this crate holds. `WorldScene::play_script_type` / `play_script_id` are the halves
    /// that do hold it.
    script_events: Vec<ScriptEvent>,
    /// Accepted player movement/teleport calls, in dispatch order. Retail applies F74C during
    /// smart-box event dispatch and player teleport during position-event handling,
    /// rather than in two independent end-of-batch passes.
    /// App drains this before its later scene snapshot synchronization.
    player_motion_dispatches: Vec<PlayerMotionDispatch>,
    /// `autonomy_level != 2`, the one
    /// question vector-update handling asks of the command interpreter.
    ///
    /// A pushed-in global, shaped like `Interaction::note_player_physics`, because the original
    /// client routes vector updates through world-object dispatch to the command interpreter, while this table
    /// has no interpreter at all. **`false` is retail's own default**, not a convenience:
    /// The command interpreter starts with `autonomy_level = 2`, and server-position acceptance
    /// tests for inequality with 2, so a client that has never taken control answers *false*
    /// and drops every vector update about its own body.
    use_position_from_server: bool,
    pub stats: ObjectStats,
    /// The physics bodies of the same objects; see [`crate::object_physics`].
    pub physics: crate::object_physics::ObjectPhysics,
    /// The numbering the server's motion commands arrive in: the world files', as the client of
    /// their day numbered its messages. The final numbering without a store.
    command_numbering: dereth_world_data::command_numbering::CommandNumbering,
}

/// Calls accepted for the local body. This is a Rust ownership seam, not a new wire message.
/// Teleport's timestamp snapshot belongs to that accepted edge: a later F74C/F748 in the same
/// network drain must not change PlayerTeleported's outgoing destination or echoed stamps.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerMotionDispatch {
    Movement(MovementBuffer),
    Teleport {
        position: Position,
        timestamps: dereth_protocol::movement::MoveTimestamps,
    },
}

/// A `dereth_client_model::World` with the host's text conversion on it.
///
/// World construction defaults to this workspace's windows-1252 table, because `dereth-client-model` has no
/// platform to read an ANSI code page from. The client does, so every `World` it builds gets
/// `CP_ACP` -- the live OS code page -- and
/// `@log`'s taboo narrowing, the Turbine callbacks and the chat line all keep the NLS path.
fn new_world() -> World {
    let mut world = World::new();
    crate::platform::text::install(&mut world);
    world
}

impl Default for ObjectStream {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectStream {
    /// An **assetless** stream: model objects arrive, but no parent link can ever be accepted.
    ///
    /// `add_child` validates the holding location against the parent's part array, so
    /// parenting needs `PhysicsSetupFacts` resolved out of the dats, and this
    /// constructor has no store to resolve them from. Every `set_parent` is therefore refused and
    /// counted in [`ObjectStats::parent_links_refused`]. Retail always has the dats open; a replay
    /// that measures anything a creature *holds* -- a container's contents on the body, a hidden
    /// holder's children, what a walk draws -- wants [`ObjectStream::with_store`] and not this.
    #[must_use]
    pub fn new() -> Self {
        Self {
            world: new_world(),
            store: None,
            presences: BTreeMap::new(),
            created: Vec::new(),
            removed: Vec::new(),
            notices: Vec::new(),
            pending_requests: Vec::new(),
            session_deleted: Vec::new(),
            session_physics_deleted: Vec::new(),
            position_entries: BTreeSet::new(),
            sound_events: Vec::new(),
            script_events: Vec::new(),
            player_motion_dispatches: Vec::new(),
            // `autonomy_level = 2` initially, so using the server position is false.
            use_position_from_server: false,
            stats: ObjectStats::default(),
            physics: crate::object_physics::ObjectPhysics::new(),
            command_numbering: dereth_world_data::command_numbering::CommandNumbering::Final,
        }
    }

    /// Production constructor. Assetless streams retain coarse model objects but cannot
    /// validate a part array or accept a parent link. App shares its already-open store.
    pub fn with_store(store: Arc<RetailDatStore>) -> Self {
        Self {
            command_numbering: dereth_world_data::command_numbering::of_store(&store),
            store: Some(store),
            ..Self::new()
        }
    }

    /// Read the data files through `store` from here on: the files reopened after a patch.
    pub fn set_store(&mut self, store: Arc<RetailDatStore>) {
        self.command_numbering = dereth_world_data::command_numbering::of_store(&store);
        self.store = Some(store);
    }

    /// The numbering the server's motion commands arrive in, and the client's go out in: the
    /// world files' ([`dereth_world_data::command_numbering`]).
    #[must_use]
    pub fn command_numbering(&self) -> dereth_world_data::command_numbering::CommandNumbering {
        self.command_numbering
    }

    fn resolve_physics_setup(&mut self, setup: u32) {
        if self.world.physics_setup_facts(setup).is_some() {
            return;
        }
        if let Some(store) = &self.store {
            self.world
                .register_physics_setup(setup, physics_setup::resolve(store, setup));
        }
    }

    fn project_parent(&mut self, id: ObjectId) {
        if let Some(p) = self.presences.get_mut(&id) {
            p.parent = self.world.physics_parent(id);
            if p.parent.is_some() {
                p.set_wire_position(None);
            }
        }
    }

    /// Give every object the server placed a body in `world`, and take away the body
    /// of every object it has removed.
    ///
    /// Called from [`crate::app::App::sync_objects`] and, when accepted player dispatch needs
    /// newly created targets, its earlier preparation stage. Synchronization advances no
    /// simulation, but an actual entry samples `update_time` at the source preparation point.
    /// The fourth create-object step both draws and places the object.
    ///
    /// The player is excluded because `world` is [`crate::character::Character`]'s and already
    /// holds his body: the client has one physics body for the player, not two.
    pub fn sync_physics(
        &mut self,
        store: &RetailDatStore,
        world: &mut dereth_physics::PhysicsWorld,
    ) {
        // Non-App adapter: a caller without the current frame time uses the most recent
        // physics sample. Production App supplies its current frame sample through `_at`.
        self.sync_physics_at(store, world, LocalTime(world.last_physics_time()));
    }

    pub fn sync_physics_at(
        &mut self,
        store: &RetailDatStore,
        world: &mut dereth_physics::PhysicsWorld,
        now: LocalTime,
    ) {
        // Initializing a loaded interior consumes its lost-object list before each
        // visibility reentry. Do not infer availability from a saved wire position.
        let loaded: Vec<_> = self
            .world
            .tables
            .lost_cells
            .iter()
            .filter_map(|(cell, _)| {
                (!cell.is_outdoor() && world.land().env_cell(cell).is_some()).then_some(cell)
            })
            .collect();
        let mut reentering = Vec::new();
        for cell in loaded {
            for id in self.world.init_obj_cell(cell) {
                if self.physics.prepare_reentry(id, world, now.0) {
                    self.world.prepare_physics_reentry(id);
                    reentering.push(id);
                } else {
                    // Assetless/bodyless adapters have no actual surviving object to reenter.
                    self.world.goto_lost_cell(id, cell);
                }
            }
        }
        // Dynamic-object initialization for a land block initializes
        // objects in a newly loaded outdoor cell too. The remote
        // bodies below already retry ordinary placement from their saved Presence; the local
        // player is intentionally excluded from that sync and therefore needs the matching
        // cell-initialization visibility consumer here. Require both the real
        // lost-cell membership and the land source's resident slot -- a saved outdoor position
        // alone does not mean its cell is available.
        let local_outdoor_reentry = self.world.player.and_then(|player| {
            self.world
                .tables
                .lost_cells
                .iter()
                .find_map(|(cell, lost)| {
                    (cell.is_outdoor()
                        && lost.objects.contains(&player)
                        && world.land().landblock_resident(cell.landblock()))
                    .then_some(player)
                })
        });
        if let Some(player) = local_outdoor_reentry {
            if self.physics.prepare_reentry(player, world, now.0) {
                self.world.prepare_physics_reentry(player);
                reentering.push(player);
            }
        }
        for id in std::mem::take(&mut self.position_entries) {
            // Move-or-teleport enters position setting when this real remote object has no
            // cell. Internal position setting prepares entry before placement, even when the
            // destination is a different loaded cell from its old lost-cell registration.
            // Mere shared-stamp changes, absent bodies and later pickup/parent snapshots do
            // not authorize this tail; player placement remains its separate owner.
            let eligible = self.world.player != Some(id)
                && self
                    .presences
                    .get(&id)
                    .is_some_and(|p| p.parent.is_none() && p.position.is_some())
                && self
                    .physics
                    .handle(id)
                    .and_then(|h| world.get(h))
                    .is_some_and(|body| body.cell.is_none());
            if eligible
                && !reentering.contains(&id)
                && self.physics.prepare_reentry(id, world, now.0)
            {
                self.world.prepare_physics_reentry(id);
                reentering.push(id);
            }
        }
        // `ObjectPhysics::sync` below excludes the local player so it cannot create a second
        // body. Complete only the re-entry prepared from an actual loaded lost-cell row, using
        // the same surviving body and saved physical pose. On failure leave the id in
        // `reentering`; the common failure tail below restores its lost row and deadline.
        if let Some(player) = self.world.player.filter(|id| reentering.contains(id)) {
            if self.physics.reenter_surviving_body(player, world) {
                reentering.retain(|id| *id != player);
            }
        }
        let Self {
            physics,
            presences,
            world: game,
            ..
        } = self;
        physics.sync(store, world, presences, game, game.player);
        // The tail of smart-box create-object handling.
        // An object whose `position.objcell_id` is non-zero and whose
        // `cell` is null after the placement goes on the object manager's destruction queue
        // (scheduled for current time + 25.0 s), and comes back off it
        // when it has a cell again. `ObjectPhysics` raises the edge because it owns
        // the placement; the queue is `dereth_client_model`'s, exactly as the reentry failure below is.
        //
        // Without it an object the client can never place — a Holtburg house door, created at
        // the **outdoor** land cell while standing inside the house, where the enclosed volume is
        // solid and answers `COLLIDED_TS` — is retried every
        // frame for ever and drawn from its wire position the whole time.
        for (id, doomed) in physics.take_placement_verdicts() {
            if !game.tables.physics.contains_key(id) {
                continue;
            }
            if doomed {
                // The failed placement of a body that had a
                // cell is the internal-positioning null-cell arm. That queue edge dooms
                // the object **and every physical child**. A teleporting player's survivors get
                // his `0xF748` to an unloaded landblock and nothing else (ACE sends no `0xF747`
                // on an adjacency move), so without the child loop his wielded items would outlive
                // their culled wielder as parentless, positionless orphans for the session.
                game.schedule_destroy(id, ServerTime(now.0));
                for child in game.physics_children(id) {
                    game.schedule_destroy(child, ServerTime(now.0));
                }
            } else {
                // A released outdoor cell
                // returns through this generic placement retry rather than the interior
                // cell-initialization scan above, and so does a body the doomed arm above parked
                // without a lost row. Both are placing a body
                // whose `cell == NULL`, and its first step is
                // the prepare-to-enter-world step: the lost row goes and the deadline is
                // cancelled for the parent **and every physical child**.
                // Cancelling only the parent when no lost row exists would be wrong: the
                // parent-only create tail is what *schedules*, but the placement that rescues
                // always runs the prepare-to-enter-world step first, and the children the doomed
                // arm queues must leave the queue on the same edge.
                game.prepare_physics_reentry(id);
            }
        }
        for id in reentering {
            if let Some(body) = physics
                .handle(id)
                .or_else(|| world.by_object_id(id))
                .and_then(|h| world.get(h))
            {
                if body.cell.is_none() {
                    // Internal position setting: a lost resolver takes prepare-to-leave,
                    // saved-position/go-to-lost-cell and clear-active, not successful entry.
                    let saved = presences
                        .get(&id)
                        .and_then(|p| p.position)
                        .unwrap_or(body.position);
                    physics.leave_visibility(id, world);
                    game.failed_physics_reentry(id, saved.cell, ServerTime(now.0));
                }
            }
        }
        self.publish_physics_cells(world);
    }

    /// One publication seam for loaded cells and the object's has-cell flag: immediately after registration
    /// or accepted placement, and again after the actual physics step. Read the body's cell membership,
    /// never Position.objcell_id. Held children follow model enter/leave-cell transitions;
    /// they deliberately do not need independent ObjectPhysics arena handles.
    ///
    /// **It publishes here too, and that is what makes the client's readers agree with
    /// retail's.**
    ///
    /// Every original-client consumer of an object's position reads the single position on the
    /// physics body — see [`Presence::position`] for the six observed readers — and
    /// the `0xF748` handler stores the position it was sent nowhere at all. Here the wire's word
    /// and the body's pose are two fields on two sides of a crate boundary, and with the
    /// no-contact arm (which moves nothing) and the queued interpolation (which moves
    /// the body over the following sub-steps) they can differ by the whole correction: without
    /// this, a body could stand still at 3 m for six "walks" out to 90 m
    /// while the record — and therefore `SceneSelectionPhysics`, the radar and every name — said
    /// it had arrived.
    ///
    /// So the record follows the body, on the same seam and in the same pass as the cell, which is
    /// the one place in this crate that holds a `&PhysicsWorld` and every object's presence at
    /// once. [`crate::app::App::frame`] calls it immediately after `WorldScene::update` — the
    /// physics step and `finish_object_physics`, which republishes the same `position` as the
    /// *drawn* position — so the selection distance, the range watch, the radar blip, the name and
    /// the drawn object are one position on one frame, as they are in retail.
    ///
    /// The **cell** is deliberately not consulted: storing a position writes `position`
    /// even on the internal position setter's null-cell arm, so retail's
    /// `position` is always the destination and `cell == NULL` is a separate fact — the one this
    /// function's other half publishes, and the one player-space conversion tests.
    ///
    /// `pub` rather than `pub(crate)` because it is one of the three steps
    /// [`crate::app::App::frame`] runs per frame, and a station that has to show a reader and the
    /// body agreeing *on one frame* has to be able to run that frame in the production order.
    pub fn publish_physics_cells(&mut self, world: &dereth_physics::PhysicsWorld) {
        let cells: Vec<_> = self
            .world
            .tables
            .physics
            .iter()
            .filter_map(|(id, p)| {
                if p.parent.is_some() {
                    return None;
                }
                let handle = self.physics.handle(id).or_else(|| {
                    (self.world.player == Some(id))
                        .then(|| world.by_object_id(id))
                        .flatten()
                })?;
                let body = world.get(handle)?;
                Some((id, body.cell, body.position))
            })
            .collect();
        for (id, cell, position) in cells {
            self.world.publish_physics_cell(id, cell);
            if let Some(p) = self.presences.get_mut(&id) {
                p.position = Some(position);
            }
        }
    }

    #[must_use]
    pub fn presence(&self, id: ObjectId) -> Option<&Presence> {
        self.presences.get(&id)
    }

    /// The whole physics state word, from the one place
    /// it is stored.
    ///
    /// `None` for an object [`Self::world`]'s physics table does not hold, which is not the same
    /// question as [`Self::presence`]: `create_or_merge`'s `StaleInstance` refusal can leave this
    /// table holding a presence the game table does not, and
    /// [`ObjectStats::state_events_without_physics`] counts the `0xF74B` arm of exactly that.
    /// Callers that need a word rather than an answer use `.unwrap_or(0)`, which is
    /// `PhysicsPresence::default()`'s own word and therefore the same value a freshly created
    /// object would have carried.
    #[must_use]
    pub fn physics_state(&self, id: ObjectId) -> Option<u32> {
        self.world.physics(id).map(|p| p.state)
    }

    /// One landblock's interior cells have unloaded: release every object standing in them.
    ///
    /// This is the client end of cell release: it handles every cell a departing block
    /// hands back. It
    /// is called from `crate::world_objects::sync_objects` for each block
    /// `WorldScene::release_block_interiors` queued. Without that caller an
    /// object in a departed cell would keep its cell, stay in the visible-object table for ever
    /// and never be scheduled for destruction.
    ///
    /// Model-only adapter. Production uses `release_block_obj_cells_with_physics` to remove
    /// the surviving body's real cell/shadow membership too; retaining an arena handle does
    /// not permit retaining a pointer to the freed cell. Presence keeps its saved position.
    ///
    /// Returns how many objects left visibility.
    pub fn release_block_obj_cells(&mut self, block: LandblockId, now: LocalTime) -> usize {
        self.flush_block_obj_cells(block, now).len()
    }

    fn flush_block_obj_cells(&mut self, block: LandblockId, now: LocalTime) -> Vec<ObjectId> {
        // In the client's own order:
        // release every land cell first, then
        // the enclosed-cell half. Cell object release is reached from three
        // places, and landblock object release is the
        // third. Without the land-cell half an object on the **terrain** of a departed block is
        // never released — it keeps its cell, stays in the visible table for ever and is never
        // scheduled for destruction, which is what leaves a portal hanging over a block the
        // player has walked out of.
        let mut left = self.world.release_land_cells(block, ServerTime(now.0));
        left.extend(self.world.flush_cells(block, ServerTime(now.0)));
        self.stats.blocks_flushed += 1;
        self.stats.objects_left_visibility += left.len() as u64;
        left
    }

    /// Production release owner, including surviving remote physical bodies.
    pub fn release_block_obj_cells_with_physics(
        &mut self,
        block: LandblockId,
        now: LocalTime,
        physics: &mut dereth_physics::PhysicsWorld,
    ) -> usize {
        let left = self.flush_block_obj_cells(block, now);
        for id in &left {
            // leave_visibility's store_position owns the current physical pose, which may
            // have advanced since the last wire packet. leave_cell preserves position.
            //
            // It also re-seeds the *target* as well as the position, which is
            // why it goes through `set_wire_position`. Storing the body's position writes
            // `position` and there is no newer word from the server, so the pose the body
            // actually reached is both where it is and where the reentry must put it back;
            // `ObjectPhysics::sync`'s retry has nothing else to aim at.
            if let Some(position) = self
                .physics
                .handle(*id)
                .or_else(|| physics.by_object_id(*id))
                .and_then(|h| physics.get(h))
                .map(|body| body.position)
            {
                if let Some(p) = self.presences.get_mut(id) {
                    p.set_wire_position(Some(position));
                }
            }
            self.physics.leave_visibility(*id, physics);
        }
        left.len()
    }

    /// Every object the server has put in the world, in id order.
    pub fn presences(&self) -> impl Iterator<Item = (ObjectId, &Presence)> {
        self.presences.iter().map(|(k, v)| (*k, v))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.presences.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.presences.is_empty()
    }

    /// `position_entries` — the accepted-remote-`0xF748` owner markers waiting to be consumed by
    /// the next per-message physics sync. Instrumentation.
    ///
    /// It is a set keyed by object, so an id that leaves the world without its marker being
    /// consumed leaves a row behind for the rest of the session. A long-session station asserts
    /// this is back at its baseline after every leave/return cycle.
    #[must_use]
    pub fn position_entry_count(&self) -> usize {
        self.position_entries.len()
    }

    /// Everything this stream is holding for a consumer that has not drained it yet, summed.
    /// Instrumentation.
    ///
    /// The nine queues are `created`, `removed`, `notices`, `pending_requests`,
    /// `session_deleted`, `session_physics_deleted`, `sound_events`, `script_events` and
    /// `player_motion_dispatches`. Every one of them is drained by some phase of `App::frame`,
    /// so a settled frame must read zero; a number that climbs cycle over cycle is a drain that
    /// stopped running, which is a leak *and* a dropped edge. Summed rather than reported nine
    /// ways because the station's question is "did anything stay behind", and the failure
    /// message prints the total beside its baseline.
    #[must_use]
    pub fn queued_edge_count(&self) -> usize {
        self.created.len()
            + self.removed.len()
            + self.notices.len()
            + self.pending_requests.len()
            + self.session_deleted.len()
            + self.session_physics_deleted.len()
            + self.sound_events.len()
            + self.script_events.len()
            + self.player_motion_dispatches.len()
    }

    /// The player's own object, once `0xF746` and its `0xF745` have both arrived.
    #[must_use]
    pub fn player(&self) -> Option<ObjectId> {
        self.world.player
    }

    /// Take the objects created since the last call. The renderer builds geometry for these.
    pub fn take_created(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.created)
    }

    /// Take the objects removed since the last call.
    pub fn take_removed(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.removed)
    }

    pub fn take_notices(&mut self) -> Vec<dereth_client_model::Notice> {
        std::mem::take(&mut self.notices)
    }

    fn apply_notice_projection(
        &mut self,
        notices: dereth_client_model::RecordingSink,
        queue_ui: bool,
    ) {
        for notice in notices.0 {
            if let dereth_client_model::Notice::ObjectDeleted(id) = &notice {
                let id = *id;
                // Object removal unsets the parent and unparents children before deleting the old body.
                let children: Vec<_> = self
                    .presences
                    .iter()
                    .filter_map(|(child, p)| {
                        p.parent
                            .is_some_and(|(parent, _)| parent == id)
                            .then_some(*child)
                    })
                    .collect();
                // The model already retired the old edge. Project, never mutate a newly
                // created same-id model object while draining its predecessor's notice.
                for child in children {
                    self.project_parent(child);
                }
                self.presences.remove(&id);
                self.position_entries.remove(&id);
                self.created.retain(|created| *created != id);
                self.physics.retire(id);
                self.removed.push(id);
                self.session_deleted.push(id);
                self.stats.removes += 1;
                if self.world.player == Some(id) {
                    self.player_motion_dispatches.clear();
                }
            }
            if queue_ui {
                self.notices.push(notice);
            }
        }
    }

    pub fn retire_session_instances<T: dereth_primitives::Transport>(
        &mut self,
        session: Option<&mut dereth_client_net::client_session::Session<T>>,
    ) {
        let deleted = std::mem::take(&mut self.session_deleted);
        let physics_deleted = std::mem::take(&mut self.session_physics_deleted);
        if let Some(session) = session {
            for id in deleted {
                session.object_deleted(id);
            }
            for id in physics_deleted {
                session.physics_object_deleted(id);
            }
        }
    }

    /// Send requests raised by the just-applied object message before another one is admitted.
    /// A component with no session consumes them as undeliverable rather than replaying stale
    /// requests into a later character session.
    pub fn drain_pending_requests<T: dereth_primitives::Transport>(
        &mut self,
        mut session: Option<&mut dereth_client_net::client_session::Session<T>>,
    ) {
        for request in std::mem::take(&mut self.pending_requests) {
            if session
                .as_deref_mut()
                .is_some_and(|s| crate::requests::send_request(s, &request))
            {
                self.stats.objdesc_asks += 1;
            } else {
                self.stats.objdesc_asks_undeliverable += 1;
            }
        }
    }

    fn discard_pending_requests(&mut self) {
        self.stats.objdesc_asks_undeliverable +=
            u64::try_from(self.pending_requests.len()).unwrap_or(u64::MAX);
        self.pending_requests.clear();
    }

    /// Non-cell-blocked world maintenance, using the current frame time. Not part of network
    /// pumping: a frame with no packets (or no link) must still expire objects and deliver notices.
    pub fn use_time<T: dereth_primitives::Transport>(
        &mut self,
        now: ServerTime,
        session: Option<&mut dereth_client_net::client_session::Session<T>>,
    ) {
        self.use_time_dispatch(now, session, None);
    }

    pub fn use_time_with_dispatch<T: dereth_primitives::Transport>(
        &mut self,
        now: ServerTime,
        session: Option<&mut dereth_client_net::client_session::Session<T>>,
        dispatch: &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
    ) {
        self.use_time_dispatch(now, session, Some(dispatch));
    }

    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn use_time_dispatch<T: dereth_primitives::Transport>(
        &mut self,
        now: ServerTime,
        session: Option<&mut dereth_client_net::client_session::Session<T>>,
        mut dispatch: Option<
            &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
        >,
    ) {
        let mut notices = dereth_client_model::RecordingSink::default();
        let mut asks = dereth_client_model::RecordingRequests::default();
        self.world.use_time_with_dispatch(
            now,
            &mut |world, notice| {
                if let Some(callback) = dispatch.as_mut() {
                    callback(world, notice.clone());
                }
                notices.0.push(notice);
            },
            &mut asks,
        );
        self.apply_notice_projection(notices, dispatch.is_none());
        let mut session = session;
        self.retire_session_instances(session.as_deref_mut());
        // The other half of the object-maintenance deadline pass:
        // the **null placeholders** that hold WorldObjects blobs parked on objects the client does
        // not have. Queuing a blob creates a null placeholder and schedules its destruction
        // in the same operation, so a parked blob has a 25-second life in the client whether or not its
        // object ever arrives. In this build that table lives in `dereth_client_net::client_session`;
        // without this deadline every WorldObjects message about an id this very loop had just
        // culled would be retained for the session. See `dereth_client_net::client_session::ordering::ParkedBlobs`.
        if let Some(session) = session.as_deref_mut() {
            self.stats.parked_blob_owners_destroyed +=
                u64::try_from(session.destroy_expired_parked_blobs(now.0)).unwrap_or(u64::MAX);
            // The same placeholders make the client ask the server to describe
            // the object: 20 s after the first blob parks on an unknown id, then every 20 s
            // while blobs keep it alive, until it is created or its deadline passes. The game
            // world's own null-object table is the same table in the client, so an id it already
            // holds (a child referenced before it arrived) is left to the world's sweep and not
            // asked about twice.
            for id in session.take_force_objdesc_asks(now.0) {
                if self.world.tables.null_physics.contains_key(id) {
                    continue;
                }
                asks.0.push(dereth_client_model::Request::ForceObjdesc(
                    dereth_protocol::objects::ObjectSendForceObjdesc { id },
                ));
            }
        }
        for request in asks.0 {
            if session
                .as_deref_mut()
                .is_some_and(|s| crate::requests::send_request(s, &request))
            {
                self.stats.objdesc_asks += 1;
            } else {
                self.stats.objdesc_asks_undeliverable += 1;
            }
        }
    }

    /// Take the server's sound events since the last call, in arrival order.
    ///
    /// The caller owes them a position and a table: sound playback uses
    /// the object's own `position` and resolves the `SoundType` against the object's own
    /// sound table, which is [`Presence::sound_table`] when the descriptor carried one and
    /// the setup's default sound table otherwise. [`crate::audio::world_use_time`] is that caller.
    pub fn take_sound_events(&mut self) -> Vec<EffectsSoundEvent> {
        std::mem::take(&mut self.sound_events)
    }

    /// Take the server's physics-script triggers since the last call, in arrival order.
    ///
    /// The caller owes them the object's physics body, which in this build is the scene's:
    /// `crate::world_state::WorldState::play_script_type` and `play_script_id` are the tails of
    /// the smart box's play-script-type and play-script-identifier handlers, and
    /// `crate::world_objects::sync_objects` is the drain's one production caller — reached
    /// from `App::frame` through `Renderer::sync_objects`.
    pub fn take_script_events(&mut self) -> Vec<ScriptEvent> {
        std::mem::take(&mut self.script_events)
    }

    /// Take the player teleport a `0xF748` asked for, if one is waiting.
    ///
    /// Applying a received player position after the `POSITION_TS` gate: when
    /// `newer_event(obj, TELEPORT_TS, <message's teleport stamp>)` holds, retail teleports
    /// the player (a simple position set with the teleport flag), then constrains the body to
    /// the position, sets its velocity and returns; otherwise it applies only the every-message
    /// constrain correction.
    ///
    /// **Only the player-teleport arm is offered here.** Constrain and interpolate are the
    /// gentle per-message correction of a body the client is already simulating; they need
    /// the start/max constraint-distance queries and the command interpreter's two
    /// answers for them, none of which this build has, and getting them wrong would fight the local
    /// simulation on every one of the corpus's 1,820 non-teleport player position events. The
    /// teleport arm is unconditional and is the one the server uses to *move* the player.
    ///
    /// The caller owes it the player-teleport arm's own null guard — it does nothing when there is no
    /// player body. The fixture helper `app::apply_player_teleport` supplies that guard.
    pub fn take_player_teleport(&mut self) -> Option<Position> {
        // Isolated teleport/component adapter: retain its historical latest-position contract.
        // Full App dispatch uses take_player_motion_dispatches instead, preserving every call.
        let mut last = None;
        self.player_motion_dispatches.retain(|event| {
            if let PlayerMotionDispatch::Teleport { position, .. } = event {
                last = Some(*position);
                false
            } else {
                true
            }
        });
        last
    }

    /// Consume accepted local-body calls exactly once, including the latest movement snapshot
    /// that WorldScene's legacy/bodyless component interface would otherwise apply again.
    pub fn take_player_motion_dispatches(&mut self) -> Vec<PlayerMotionDispatch> {
        let events = std::mem::take(&mut self.player_motion_dispatches);
        if let Some(p) = self.world.player.and_then(|id| self.presences.get_mut(&id)) {
            // Both local producers (F74C and embedded descriptor movement) journal their write
            // to this latest snapshot. Drain it structurally, not by comparing floating payloads.
            // A bodyless adapter may have kept an older snapshot with no queued movement; a
            // teleport-only journal must leave that initial/viewer snapshot alone.
            if events
                .iter()
                .any(|event| matches!(event, PlayerMotionDispatch::Movement(_)))
            {
                p.pending_movement = None;
            }
        }
        events
    }

    /// Whether App needs the scene's create/geometry prefix before ordered player dispatch.
    pub fn has_player_motion_dispatches(&self) -> bool {
        !self.player_motion_dispatches.is_empty()
    }

    /// A bodyless viewer already reads the latest position and movement snapshots. It cannot
    /// execute a local-body completion tail, and must not retain an unconsumed dispatch journal.
    pub fn discard_bodyless_player_dispatches(&mut self) -> u64 {
        let teleports = self
            .player_motion_dispatches
            .iter()
            .filter(|e| matches!(e, PlayerMotionDispatch::Teleport { .. }))
            .count() as u64;
        self.player_motion_dispatches.clear();
        teleports
    }

    /// Take the placement a position event named for `id`, if one is waiting.
    ///
    /// The caller must install it only when the object's sequence has
    /// no animations, matching the position-event handler's guard.
    pub fn take_pending_placement(&mut self, id: ObjectId) -> Option<u32> {
        self.presences.get_mut(&id)?.pending_placement.take()
    }

    /// Record that the draw accepted a placement, so it is not offered again.
    pub fn placement_installed(&mut self, id: ObjectId, placement: u32) {
        if let Some(e) = self.presences.get_mut(&id) {
            e.placement = placement;
        }
    }

    /// The command interpreter uses the position supplied by the server.
    ///
    /// `App::interaction_use_time`'s neighbour reads `MovementCommands::lists.autonomy_level` and
    /// hands the answer here, once a frame, before the network drain — see
    /// [`Self::use_position_from_server`] for why the default is `false`.
    pub fn note_use_position_from_server(&mut self, on: bool) {
        self.use_position_from_server = on;
    }

    /// Take the `(velocity, omega)` a `0xF74E` asked for, if one is waiting.
    ///
    /// Vector updating sets velocity and then angular velocity, in that order.
    /// `WorldScene::sync_objects` drains these arguments because the physics object they
    /// are called on is `dereth_physics`'s and lives in the scene's world.
    pub fn take_vector_update(
        &mut self,
        id: ObjectId,
    ) -> Option<(dereth_primitives::Vec3, dereth_primitives::Vec3)> {
        self.presences.get_mut(&id)?.pending_vector.take()
    }

    /// Put a `(velocity, omega)` back because there was no physics body to apply it to yet.
    ///
    /// In the client the two are inseparable: object creation makes the
    /// body and *then* applies its description, so `set_velocity` is never reached
    /// without one. This build inverts that order — `WorldScene::sync_objects` (the drain) runs
    /// before `ObjectStream::sync_physics` (the spawn) in `App::sync_objects` — so a create's own
    /// velocity would be taken on the frame the body does not exist and lost for ever. Re-parking
    /// keeps the retail ordering without moving either half.
    ///
    /// It does not overwrite a newer pair: a `0xF74E` that arrived in the meantime is the later
    /// word and the vector-update stamp gate has already ruled on it — so "a newer one is already
    /// waiting" answers `true` as well, because in neither case was anything lost.
    ///
    /// `false` means the object has left the table between the take and the put-back, which is the
    /// one case where a `(velocity, omega)` is genuinely dropped.
    pub fn park_vector_update(
        &mut self,
        id: ObjectId,
        pair: (dereth_primitives::Vec3, dereth_primitives::Vec3),
    ) -> bool {
        let Some(e) = self.presences.get_mut(&id) else {
            return false;
        };
        if e.pending_vector.is_none() {
            e.pending_vector = Some(pair);
        }
        true
    }

    /// Take the movement buffer the server sent for `id`, if one is waiting.
    pub fn take_movement(&mut self, id: ObjectId) -> Option<MovementBuffer> {
        let pending = self.presences.get_mut(&id)?.pending_movement.take();
        if self.world.player == Some(id) && pending.is_some() {
            // The existing component/viewer API consumes the latest snapshot; do not replay
            // those same commands if its caller subsequently hands the stream back to App.
            self.player_motion_dispatches
                .retain(|e| !matches!(e, PlayerMotionDispatch::Movement(_)));
        }
        pending
    }

    /// Apply one session event, returning Weenie/create arrivals. Callers must consult
    /// World.physics before publishing a physics INSTANCE_TS or draining physical netblobs:
    /// CreateObject can succeed for the Weenie despite failed physical initialization.
    ///
    /// Compatibility/component adapter. The App uses `apply_event_with_dispatch` at each
    /// owning Session phase so old-owner subscribers execute synchronously.
    pub fn apply_event(&mut self, e: &SessionEvent, now: LocalTime) -> Vec<(ObjectId, u16)> {
        let mut arrived = Vec::new();
        self.apply(e, now, &mut arrived);
        // This compatibility adapter has no Session. A request raised here belongs to this
        // message boundary and must not wait for an unrelated future session.
        self.discard_pending_requests();
        arrived
    }

    /// Production one-message boundary. Old-Weenie subscribers finish before a replacement
    /// is instantiated; returned arrivals must release parked callbacks before another message.
    pub fn apply_event_with_dispatch(
        &mut self,
        e: &SessionEvent,
        now: LocalTime,
        dispatch: &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
    ) -> Vec<(ObjectId, u16)> {
        let mut arrived = Vec::new();
        self.apply_with_dispatch(e, now, &mut arrived, Some(dispatch));
        arrived
    }

    /// Drain the session until it is quiet, applying everything to the tables.
    ///
    /// The loop is what closes [`dereth_client_net::client_session::Session::object_arrived`]: releasing what was parked
    /// on an object produces more events — the `0x0013` that makes the session playable among them
    /// — and those may themselves create objects. Each pass drains, applies, and then tells the
    /// session which objects now exist; the pass that produces no events ends it.
    ///
    /// Returns everything decoded, in arrival order, for the caller to log and act on.
    pub fn pump(
        &mut self,
        net: &mut crate::net::ClientNetwork,
        now: LocalTime,
    ) -> Vec<SessionEvent> {
        self.pump_session(&mut net.session, now)
    }

    /// Compatibility coarse drain for component callers. It does not implement App's source
    /// frame phases; network-event draining simply collects the session's event drain.
    ///
    /// **Measured against a finer-grained drain and found identical**, on
    /// `early-inventory-and-casting`, `short-second-connection`, `long-solo-play` and
    /// `short-play-with-training`: same event count, same presences, same physics state words, same
    /// inventory lists at every datagram. The coarseness is real -- one `Session::object_arrived`
    /// where `App` runs `begin_weenie_arrival` / `begin_object_arrival` / `process_next_object_ui`
    /// / `take_object_message` in the client's object-blob processing order -- but it is not a
    /// behavioural divergence on the recorded corpus. A replay suite reduced to its denominator
    /// guards is caused by [`ObjectStream::new`] having no dat store, not by this loop.
    pub fn pump_session<T: dereth_primitives::Transport>(
        &mut self,
        session: &mut dereth_client_net::client_session::Session<T>,
        now: LocalTime,
    ) -> Vec<SessionEvent> {
        let mut all = Vec::new();
        self.retire_session_instances(Some(session));
        loop {
            let batch: Vec<_> = session.drain_events().collect();
            if batch.is_empty() {
                break;
            }
            for event in batch {
                let e = match event {
                    SessionEvent::WorldObject { opcode, body } => {
                        let Some(accepted) = session.recheck_world_object_delivery(opcode, &body)
                        else {
                            continue;
                        };
                        accepted
                    }
                    other => other,
                };
                let mut arrived = Vec::new();
                self.apply_with_dispatch(&e, now, &mut arrived, None);
                self.drain_pending_requests(Some(session));
                // Never defer an id-only cleanup past a later replacement in this batch.
                self.retire_session_instances(Some(session));
                for (id, instance) in arrived {
                    if self.world.physics(id).is_some() {
                        session.object_arrived(id, instance);
                    }
                }
                all.push(e);
            }
        }
        // Maintenance is now the WorldObjects frame's responsibility, not a second network tick.
        all
    }

    /// One event.
    fn apply(&mut self, e: &SessionEvent, now: LocalTime, arrived: &mut Vec<(ObjectId, u16)>) {
        self.apply_with_dispatch(e, now, arrived, None);
    }

    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn apply_with_dispatch(
        &mut self,
        e: &SessionEvent,
        now: LocalTime,
        arrived: &mut Vec<(ObjectId, u16)>,
        dispatch: Option<
            &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
        >,
    ) {
        match e {
            // The player identifier arrives before the object.
            SessionEvent::PlayerCreated(id) => {
                self.world.set_player(*id);
            }
            SessionEvent::WorldObject { opcode, body } => {
                self.world_view(*opcode, body, now, arrived, dispatch);
            }
            // Ending the character session tears the whole object model down with the session.
            //
            // **`WorldReset` is the arm that matters.** The three
            // others are *endings*; retail's object teardown is keyed on the **beginning** of the
            // next session, when its second login phase resets the session. Without it, an ending
            // this client never sees (a transport drop, the 110 s timeout) leaves every object of
            // the last session in these tables, and `load_pending_scene` then rebuilds the
            // landscape around the *old* player's position because `ObjectStream::player` still
            // answers.
            SessionEvent::WorldReset
            | SessionEvent::StateChanged(
                SessionState::CharacterSelect | SessionState::Disconnected(_),
            )
            | SessionEvent::LoggedOff => {
                self.reset();
            }
            _ => {}
        }
    }

    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn world_view(
        &mut self,
        op: Opcode,
        body: &[u8],
        now: LocalTime,
        arrived: &mut Vec<(ObjectId, u16)>,
        dispatch: Option<
            &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
        >,
    ) {
        match op {
            Opcode::ITEM_CREATE_OBJECT => {
                let Ok(m) = read_body_padded::<ItemCreateObject>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.create(&m.0, false, now, arrived, dispatch);
            }
            Opcode::ITEM_UPDATE_OBJECT => {
                let Ok(m) = read_body_padded::<ItemUpdateObject>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.create(&m.0, true, now, arrived, dispatch);
            }
            // a new appearance for an object that
            // already exists, which is what a dye, an equip or an unequip produces.
            Opcode::ITEM_OBJ_DESC_EVENT => {
                let Ok(m) = read_body_padded::<ItemObjDescEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                match self.presences.get_mut(&m.id) {
                    Some(e) => {
                        e.objdesc = m.objdesc;
                        self.stats.objdesc_events += 1;
                        // The renderer keys geometry on the appearance, so a changed one is a
                        // rebuild: offer the object again exactly as a recreate does.
                        self.created.push(m.id);
                    }
                    None => self.stats.objdesc_events_unknown += 1,
                }
            }
            Opcode::ITEM_DELETE_OBJECT => {
                let Ok(m) = read_body_padded::<ItemDeleteObject>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                // Delete-object handling rejects the player and accepts only the matching
                // existing physics instance. Session owns parking of unknown/newer packets.
                if self.world.player != Some(m.id)
                    && self
                        .presence(m.id)
                        .is_some_and(|p| p.instance == m.instance_sequence)
                {
                    self.remove(m.id, now, dispatch);
                }
            }
            // Smart-box parent-event handling dispatches the parent event.
            Opcode::ITEM_PARENT_EVENT => {
                let Ok(m) = read_body_padded::<ItemParentEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.parent_event(&m, now);
            }
            Opcode::MOVEMENT_POSITION_EVENT => {
                let Ok(m) = read_body_padded::<MovementPositionEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.received_position(&m, now);
            }
            Opcode::MOVEMENT_SET_OBJECT_MOVEMENT => {
                let Ok(m) = read_body_padded::<MovementSetObjectMovement>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                let Ok(buf) = m.decoded_movement() else {
                    self.stats.movement_undecodable += 1;
                    return;
                };
                self.set_object_movement(m.id, buf);
            }
            // Physics dispatch sends the vector update through smart-box handling
            // and then applies it.
            //
            // The unpacker consults no state: it rechecks the opcode, then reads the id,
            // two vectors and a timestamp, which is exactly this decode.
            //
            // Vector-update handling first looks up the object with a failure arm, then performs
            // three-way `INSTANCE_TS` comparison, whose two failure arms are "park" and
            // "return 2", the old-instance result.
            // `instance_sequence` answers `Some(timestamps.instance)` for this opcode, so the gate
            // has already run and the receiver owes none of it. **This is where `0xF74E` differs
            // from the two script arms**, whose `instance_sequence` is `None`: those owed only
            // existence, this one has a second stamp underneath.
            Opcode::MOVEMENT_VECTOR_UPDATE => {
                let Ok(m) = read_body_padded::<MovementVectorUpdate>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.vector_update(&m);
            }
            // The `0xF619` arm is inlined into smart-box event dispatch and has no
            // function of its own.
            //
            // Read the object id, then unpack the position using an in/out cursor that starts
            // after the id. Result 4 queues the blob and returns 4; result 2 returns without
            // applying movement. Otherwise look up the object again and apply movement using
            // the cursor advanced past the position pack. A zero movement result stops here;
            // after a successful movement result, control is relinquished to the server.
            //
            // **Two things the shape alone would lose.** Position-event unpacking returns 1
            // **unconditionally** after applying the received position, which itself returns
            // void — so a position the `POSITION_TS` gate refused still lets the
            // movement half run. The cursor is re-read *after* the call, so the movement buffer
            // begins exactly where position unpacking stopped:
            // that is `r.rest()` in `dereth-protocol`, which settles the boundary.
            // Its **alignment origin is not `0xF74C`'s**: a
            // `PositionPack` is always a whole number of dwords, so the buffer starts at a blob
            // offset that is `0 (mod 4)` rather than `0xF74C`'s 10, and reading it at
            // the bare movement packet's blob origin would pad by the wrong amount.
            //
            // The corpus has **zero** of these, so nothing below is measured against recorded
            // traffic.
            Opcode::MOVEMENT_POSITION_AND_MOVEMENT_EVENT => {
                let Ok(m) = read_body_padded::<MovementPositionAndMovementEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.position_and_movement_event(&m, now);
            }
            // Smart-box state handling applies the state.
            //
            // The state handler's own two tests — "does the object exist" and "is the instance
            // sequence the one we hold" — are `dereth_client_net::client_session`'s dispatch, which parks the blob on
            // the object id and answers `OldInstance` exactly as the original handler does. What is left is
            // its `STATE_TS` gate and the word itself.
            Opcode::ITEM_SET_STATE => {
                let Ok(m) = read_body_padded::<ItemSetState>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.set_state(&m);
            }
            // Smart-box sound-event handling asks the physics object to play the sound.
            // There is no sequence check at all (`dereth_client_net::client_session`'s dispatch says
            // so and its own test asserts it), and the object is guaranteed to exist: the
            // session's gate parks the blob on the leading guid until it does, which is exactly
            // the sound handler's queue-on-missing-object arm.
            //
            // Nothing is played here. The message is parked for the frame loop the same way a
            // movement buffer is, because the position it plays at is the object's *rendered*
            // `position` and this crate does not draw.
            // Smart-box pickup-event handling applies the pickup event.
            // The lookup and the instance test are `dereth_client_net::client_session`'s dispatch, which parks
            // the blob on the object id and answers `OldInstance` exactly as the original handler does; what
            // is left is the pickup event's `POSITION_TS` gate and its two calls.
            Opcode::INVENTORY_PICKUP_EVENT => {
                let Ok(m) = read_body_padded::<InventoryPickupEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.pickup_event(&m, now);
            }
            Opcode::EFFECTS_SOUND_EVENT => {
                let Ok(m) = read_body_padded::<EffectsSoundEvent>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.stats.sound_events += 1;
                if crate::trace::notice() {
                    // `0xF750` as received; `0x94` is `OpenFailDueToLock`.
                    tracing::debug!(
                        target: "dereth::trace::notice",
                        "notice-trace 0xF750 Sound object={:?} sound_type={:#x} volume={}",
                        m.id,
                        m.sound_type,
                        m.volume
                    );
                }
                self.sound_events.push(m);
            }
            // The type form resolves through the object's script table.
            //
            // Spelled out, because the shape of the handler is the
            // whole of what this arm owes and a summary loses it:
            //
            // Look up the object by id. A missing object queues the blob and returns 4.
            // A found object plays the requested script type with the supplied intensity.
            //
            // There is **no instance-sequence test at all** — the only gate is "does the object
            // exist", and its failure arm queues the blob for the object, not a drop. `dereth_client_net::client_session`'s
            // dispatch is that gate (`instance_sequence` answers `None` for `0xF750`, `0xF754` and
            // `0xF755`, and the `None` arm parks on `table.knows(id)`), so by the time a body
            // reaches here the object is known and nothing more is owed.
            //
            // Nothing is played here, for the reason [`Self::script_events`] gives.
            Opcode::EFFECTS_PLAY_SCRIPT_TYPE => {
                let Ok(m) = read_body_padded::<EffectsPlayScriptType>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.stats.script_type_events += 1;
                self.script_events.push(ScriptEvent::Type {
                    id: m.id,
                    script_type: m.script_type,
                    intensity: m.intensity,
                });
            }
            // The ID form names the script directly. The same handler as above with one
            // argument fewer and a call to direct script playback,
            // which consults **no physics-script table**, because the
            // message names the script outright.
            Opcode::EFFECTS_PLAY_SCRIPT_ID => {
                let Ok(m) = read_body_padded::<EffectsPlayScriptId>(body) else {
                    self.stats.unhandled += 1;
                    return;
                };
                self.stats.script_id_events += 1;
                self.script_events.push(ScriptEvent::Id {
                    id: m.id,
                    script: DataId(m.script_id),
                });
            }
            // `stats.unhandled` says *how many*, not *which*, and "which" is the whole
            // question when chasing a dropped message. The ledger names the opcode once, in
            // the log, the first time it is dropped.
            _ => {
                crate::dropped::record(crate::dropped::Site::WorldObjects, op);
                self.stats.unhandled += 1;
            }
        }
    }

    /// `0xF745` and `0xF7DB`, through the world tables' own two entry points.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn create(
        &mut self,
        p: &dereth_protocol::objects::ObjectCreatePayload,
        recreate: bool,
        now: LocalTime,
        arrived: &mut Vec<(ObjectId, u16)>,
        mut dispatch: Option<
            &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
        >,
    ) {
        let id = p.id;
        let instance = p.physicsdesc.timestamps.instance;
        self.resolve_physics_setup(p.physicsdesc.setup_id.unwrap_or(0));
        let old_position_ts = self.presences.get(&id).map(|p| p.position_ts);
        // Vector updating compares against the stamp the object
        // *already holds*, so the merge arm below needs it before the descriptor overwrites it.
        let old_vector_ts = self.presences.get(&id).map(|p| p.vector_ts);
        let is_player = self.world.is_the_player(id);
        let use_position_from_server = self.use_position_from_server;
        let mut notices = dereth_client_model::RecordingSink::default();
        let queue_ui = dispatch.is_none();
        let mut callback = |world: &mut dereth_client_model::World,
                            notice: dereth_client_model::Notice| {
            if let Some(callback) = dispatch.as_mut() {
                callback(world, notice.clone());
            }
            notices.0.push(notice);
        };
        let mut requests = dereth_client_model::RecordingRequests::default();
        let r = if recreate {
            self.world
                .recreate_with_dispatch(p, ServerTime(now.0), &mut callback)
        } else {
            self.world.create_or_merge_with_dispatch_and_requests(
                p,
                ServerTime(now.0),
                &mut callback,
                &mut requests,
            )
        };
        self.pending_requests.extend(requests.0);
        self.apply_notice_projection(notices, queue_ui);
        match r {
            Ok(_) => {
                if recreate {
                    self.stats.recreates += 1;
                } else {
                    self.stats.creates += 1;
                }
            }
            Err(dereth_client_model::GameError::DuplicateCreate(_)) => self.stats.merges += 1,
            Err(dereth_client_model::GameError::StaleInstance { .. }) => {
                // The world tables refused it, so nothing about the object changes and the renderer is not
                // told. The session's gate has the same rule and normally catches this first.
                self.stats.stale_instances += 1;
                return;
            }
            Err(_) => {
                self.stats.unhandled += 1;
                return;
            }
        }

        // Object creation may construct and announce the Weenie even when physical or placeholder
        // initialization fails. Do not invent a render/physical presence or replay admission.
        if self.world.physics(id).is_none() {
            self.presences.remove(&id);
            self.position_entries.remove(&id);
            self.session_physics_deleted.push(id);
            if self.world.weenie(id).is_some() {
                arrived.push((id, instance));
            }
            return;
        }

        let existed = self.presences.contains_key(&id);
        // The equal-instance merge: the client already holds this very object, so the description
        // is applied to it piecewise rather than building it again. A newer instance, which the
        // world tables have just deleted and built again (its presence went with the delete), and
        // a `0xF7DB` are creates.
        let merged = existed && !recreate;
        let e = self.presences.entry(id).or_default();
        let d = &p.physicsdesc;
        e.instance = instance;
        // **The state word is not written here at all, because it is not stored
        // here.** Game-side creation has already put `physicsdesc.state` into
        // [`dereth_client_model::objects::PhysicsPresence::state`] on both the create and the merge path,
        // and that is the only copy. See [`Presence`] and [`ObjectStream::physics_state`].
        e.scale = d.object_scale.unwrap_or(1.0);
        if let Some(s) = d.setup_id {
            e.setup_id = Some(DataId(s));
        }
        if let Some(mt) = d.mtable_id {
            e.mtable_id = Some(DataId(mt));
        }
        // Same shape as the two above: description setup only writes the field
        // when its flag is set, and an equal-instance `0xF745` merge that omits it keeps what it
        // had. `0xF7DB` is forced recreation and starts with a fresh Presence.
        if let Some(st) = d.stable_id {
            e.sound_table = Some(DataId(st));
        }
        // The same description-setup step that installs the sound table
        // one line up: the two are released and re-`Get`d together, so they take the same rule.
        if let Some(pt) = d.phstable_id {
            e.phs_table = Some(DataId(pt));
        }
        // When the movement buffer is null,
        // it installs the descriptor's `0x00020000` animframe field as the placement frame.
        // ACE writes `(uint)(Placement ?? Placement.Default)` into that field;
        // `dereth_protocol` decodes it as `animframe_id`. Object creation is the only caller
        // and applies the description unconditionally, so **the descriptor always decides**.
        //
        // The `unwrap_or` is not a convenience: descriptor unpacking zeroes the
        // animframe field when **neither** flag is set, so a descriptor that names
        // no movement and no placement installs `Placement.Default` — overriding the `0x65` that
        // setup creation and final object initialization put there. 20 of the corpus's 839 creates take that
        // arm. A create that *does* carry a movement buffer takes the other arm of the same `if`
        // and never touches the placement, so it keeps what was installed.
        //
        // The merge does not set up a description, so it installs no placement here: its
        // placement goes with its position, parent or pickup below.
        if d.movement.is_none() && !merged {
            e.placement = d.animframe_id.unwrap_or(crate::models::PLACEMENT_DEFAULT);
        }
        // Create-object handling enters an object into the world when it has a
        // position and no parent; `objcell_id == 0` means it is carried, not placed.
        //
        // **Only a create stores the description's position here.** The merge hands it to
        // received-position handling below, as a `0xF748` is handed, and the stamps the object
        // already holds decide whether it moves.
        if let Some(pos) = d.position.filter(|_| !merged) {
            if pos.objcell_id != 0 {
                e.set_wire_position(Some(pos.into()));
            } else {
                e.set_wire_position(None);
            }
            // This position came from the create-object handler, the only
            // caller of world entry. A position arriving this way must therefore enter the world
            // rather than use move-or-teleport, which belongs to received-position handling.
            e.position_from_create = true;
        }
        // Object creation applies the whole descriptor, including the object description;
        // the equal-instance merge path re-applies it too, which is why this is unconditional.
        e.objdesc = p.objdesc.clone();
        // The three position stamps are the received-position handler's on the merge, which
        // advances each one only past its own gate.
        if !merged {
            e.position_ts = d.timestamps.position;
        }
        e.movement_ts = d.timestamps.movement;
        // Description setup closes with a loop over **all nine**
        // `PhysicsTimeStamp` slots (`POSITION_TS` through `INSTANCE_TS`), so the descriptor seeds
        // `update_times[STATE_TS]` exactly as it seeds the three beside it. Without this a create
        // that carried a non-zero state stamp would leave the gate at 0 and accept a `0xF74B` the
        // client would have refused.
        e.state_ts = d.timestamps.state;
        e.server_control_ts = d.timestamps.server_controlled_move;
        // The same "all nine slots" loop as the line above it. Both are 0 on every
        // player create in the corpus, so the seed is not what makes the echo right — but a create
        // that carried a non-zero stamp would leave `newer_event` comparing against 0 and accept a
        // `0xF748` the client would have refused, the same failure as for `STATE_TS` above.
        if !merged {
            e.teleport_ts = d.timestamps.teleport;
            e.force_position_ts = d.timestamps.force_position;
        }
        // **The merge's position, parent or pickup** — whichever one its description names, and
        // before its movement, state and vector, as the merge applies them:
        //
        // * a parent: the parent event, gated on `POSITION_TS`, installs the description's
        //   placement and attaches it to its holder;
        // * a position in a cell: received-position handling, handed the description's
        //   placement, its three position stamps and ground contact set. It is gated on the
        //   stamps the object already holds and rolls back for an older teleport stamp, and a
        //   remote object it accepts then takes move-or-teleport rather than being placed again;
        // * no position, or a zero cell: the pickup event, gated on `POSITION_TS`.
        if merged {
            let placement = d.animframe_id.unwrap_or(crate::models::PLACEMENT_DEFAULT);
            let newer = old_position_ts.is_some_and(|old| is_newer(old, d.timestamps.position));
            if let Some((holder, location)) = d.parent.filter(|(id, _)| id.0 != 0) {
                if newer {
                    e.position_ts = d.timestamps.position;
                    e.placement = placement;
                    e.pending_placement = None;
                    self.set_parent(id, holder, location, now);
                } else {
                    self.project_parent(id);
                }
            } else if let Some(pos) = d.position.filter(|pos| pos.objcell_id != 0) {
                self.apply_received_position(
                    &ReceivedPosition {
                        id,
                        destination: pos.into(),
                        placement_id: Some(placement),
                        contact: true,
                        position_ts: d.timestamps.position,
                        teleport_ts: d.timestamps.teleport,
                        force_position_ts: d.timestamps.force_position,
                        from_create: true,
                    },
                    now,
                );
            } else if newer {
                e.position_ts = d.timestamps.position;
                self.unset_parent(id, now);
                self.world.leave_physics_world(id);
                if let Some(e) = self.presences.get_mut(&id) {
                    e.set_wire_position(None);
                }
            } else {
                self.project_parent(id);
            }
        }
        let e = self.presences.entry(id).or_default();
        // The ninth slot of the same loop. The equal-instance merge path closes by applying
        // the velocity pair under `ts[VECTOR_TS]`, which is the *same*
        // function the `0xF74E` receiver below calls, so seeding the stamp here is what stops a
        // create's own velocity and a vector update that crosses it from being applied twice in
        // the wrong order.
        //
        // **The stamp alone is not enough: the velocity it guards must be applied too.** A spell
        // projectile is an object the server creates *with a velocity in its descriptor* and the
        // client integrates; with the descriptor's velocity dropped, the bolt is created at the
        // caster and stays there.
        //
        // Retail has **two** arms and they are not the same, so both are here:
        //
        // * **the create arm** — applies the description to the body it just made, then
        //   passes all three velocity components to the velocity setter with notification
        //   enabled. It assigns the angular velocity directly.
        //
        //   There is **no stamp gate and no non-zero test**: every fresh create applies its
        //   descriptor's velocity, and the nine-stamp loop that seeds the line above runs *after*
        //   it. The update-object handler invokes creation with recreation enabled, and
        //   that argument skips the merge test entirely, so a
        //   `0xF7DB` is a destroy-and-recreate and takes this arm too.
        //
        //   The two arms write angular velocity the same way: description setup
        //   assigns it directly, and vector-update handling calls the angular-velocity setter,
        //   whose entire body is that assignment.
        //
        //   `set_omega` does **not** mark the object active: the angular-velocity setter makes no
        //   calls. That matters on the angular-velocity animation hook, where `set_omega` is the
        //   only call made; a `set_active` there would be a divergence from retail. The physics
        //   crate's tests pin this.
        //
        // * **the merge arm** — closes its equal-instance path by applying
        //   `(pd.velocity, pd.omega)` under `ts[VECTOR_TS]`, using
        //   the gate [`Self::vector_update`] transcribes: `is_newer` (an equal stamp is stale), the
        //   stamp written *before* the player test, and the `UsePositionFromServer` arm that makes
        //   an autonomous client ignore a vector update about its own body.
        let created = recreate || !existed;
        if created || old_vector_ts.is_some_and(|old| is_newer(old, d.timestamps.vector)) {
            e.vector_ts = d.timestamps.vector;
            // **No deviation here: this was measured.**
            //
            // Position-description unpacking leaves the vector zeroed when its flag is clear, so
            // retail's create calls `set_velocity((0,0,0), TRUE)` for a descriptor that declares
            // no velocity — and this is not a no-op:
            // the vector inequality test branches **to** the state update rather than past it,
            // and the second argument is never read at all (only the vector is). So every
            // non-static object the client creates really is ACTIVE from birth, which puts every
            // created object on the integrating path with gravity.
            //
            // **The object-update pass makes this activation redundant:**
            //
            // Beyond 96 m, clear ACTIVE_TS before activating the object. Within 96 m,
            // preserve that flag and proceed directly to activation.
            //
            // The physics tick sweeps **every** object into `update_object`, and
            // `update_object` has no `ACTIVE_TS` gate of its own — the gate lives
            // one level down, in the internal object update, and by the time control gets
            // there `set_active(1)` has already run for anything inside the 96 m activity radius.
            // So the create's activation is redundant on the very next tick, and for an object
            // *outside* 96 m the same sweep clears `ACTIVE_TS` again before anything integrates.
            // The window in which the two behaviours differ is between the create and the first
            // `UseTime`, and the only reader of `ACTIVE_TS` in that window is `update_object`
            // itself, which overwrites it. Measured rather than argued: a client test records
            // that a gravity-carrying create falls the same 5.0 m to the ground either way.
            //
            // Transcribed, therefore, rather than deviated from: a create parks a pair whether or
            // not the descriptor declared one, exactly as applying the original description does.
            let autonomy_refuses = !created && is_player && !use_position_from_server;
            if !autonomy_refuses {
                e.pending_vector = Some((
                    d.velocity.map_or(dereth_primitives::Vec3::ZERO, Into::into),
                    d.omega.map_or(dereth_primitives::Vec3::ZERO, Into::into),
                ));
            }
        }

        // The create path applies the whole descriptor, including its movement buffer.
        // The merge path invokes the same movement setter with `(phys, buffer, len, ts[MOVEMENT_TS],
        // ts[5], autonomous)` (slot 5 is the server-controlled-move stamp). The three header fields
        // normally peeled from a movement packet come from the **descriptor**
        // here, and the buffer starts directly at its body.
        //
        // Its alignment origin is the blob, like every other buffer, and here the two conventions
        // agree: the object description ends 4-aligned and `bitfield`, `state`, and length occupy
        // 12 bytes, so the buffer always begins on a multiple of four. All 219 embedded buffers
        // in the seven-session corpus consume exactly under that reading.
        if let Some((buf, autonomous)) = d.movement.clone() {
            if !buf.is_empty() {
                let mut r = Reader::new(&buf);
                match MovementBody::read(&mut r).and_then(|b| r.expect_exhausted().map(|()| b)) {
                    Ok(body) => {
                        let movement = MovementBuffer {
                            movement_timestamp: d.timestamps.movement,
                            server_control_timestamp: d.timestamps.server_controlled_move,
                            autonomous: autonomous != 0,
                            body,
                        };
                        // The descriptor dispatches through the same movement consumer. Without
                        // this edge, an embedded approach followed by `0xF748` would be installed by
                        // the later snapshot sync after teleport_hook had already canceled it.
                        if self.world.player == Some(id) {
                            self.player_motion_dispatches
                                .push(PlayerMotionDispatch::Movement(movement.clone()));
                        }
                        e.pending_movement = Some(movement);
                        self.stats.create_movements += 1;
                    }
                    Err(_) => self.stats.create_movement_undecodable += 1,
                }
            }
        }
        // Object-creation order: apply description and appearance changes,
        // attach the children from the descriptor's child list, and only **then** attach the
        // object to its descriptor parent and location.
        //
        // **Only on the create path.** The equal-instance *merge* arm -- the one
        // taken when the object already exists and the instance sequences are equal -- does not
        // go through fresh object creation at all. It applies the visual description first; then
        // exactly one applicable parent, pickup, or received-position branch; then movement,
        // state, and vector updates. Those three position branches are alternatives, not a sequence.
        // The merge never touches the child list.
        // Running `unparent_children` on a merge detaches held items the descriptor's list is not
        // obliged to re-name, and over the corpus that would make one of the 36 pickups that
        // arrive on a held object stop seeing one.
        if !merged {
            self.project_created_children(id, d);
            // Placeholder initialization leaves the old parent intact when the descriptor has none.
            // The merge's parent, position or pickup was applied above.
            self.project_parent(id);
        }

        // A recreate deletes and rebuilds, so its geometry is rebuilt too; a merge changes nothing
        // the renderer has to be told about.
        if !existed || recreate {
            self.created.push(id);
        }

        // The seam: the object exists now, so replay whatever was parked on it.
        arrived.push((id, instance));
    }

    /// `0xF747 Item_DeleteObject` — smart-box delete handling removes the object.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn remove(
        &mut self,
        id: ObjectId,
        now: LocalTime,
        mut dispatch: Option<
            &mut dyn FnMut(&mut dereth_client_model::World, dereth_client_model::Notice),
        >,
    ) {
        let mut notices = dereth_client_model::RecordingSink::default();
        self.world
            .delete_object_with_dispatch(id, ServerTime(now.0), &mut |world, notice| {
                if let Some(callback) = dispatch.as_mut() {
                    callback(world, notice.clone());
                }
                notices.0.push(notice);
            });
        self.apply_notice_projection(notices, dispatch.is_none());
    }

    /// Apply a state event after the handler's object and instance lookups.
    ///
    /// When the event stamp is newer than the object's `STATE_TS` stamp, the stamp is stored, the
    /// object's state is set with notification enabled, and -- if the object is the player, a
    /// teleport is awaited and the new state lacks `0x4000` -- the teleport wait is cleared.
    ///
    /// The gate is the whole handler: `set_state` writes the **whole** word, so a stale message
    /// applied out of order would not merge wrongly, it would reinstate an entire earlier state —
    /// a door that re-closes because its "open" arrived late.
    ///
    /// **The `waiting_for_teleport` clause is not reproduced here and the reason is structural.**
    /// It is about the *player's* body, and this build's player is [`crate::character::Character`],
    /// which [`crate::object_physics::ObjectPhysics::sync`] excludes by construction so that the
    /// client keeps one physics body for him rather than two. **22 of the corpus's 57 `0xF74B`
    /// are about the player** — the teleport hide (`HIDDEN_PS | IGNORE_COLLISIONS_PS`) and the
    /// unhide that ends it — and they land in this table but not on his body.
    fn set_state(&mut self, m: &ItemSetState) {
        let Some(e) = self.presences.get_mut(&m.id) else {
            // The session's gate parks a `0xF74B` for an unknown object on that object's id, so
            // this arm is only reachable when the world tables refused an object the session
            // accepted.
            self.stats.state_events_unknown += 1;
            return;
        };
        // Test and advance the event stamp in `update_times[STATE_TS]`, which is
        // `update_times[2]` in the state handler's own indexing.
        if !is_newer(e.state_ts, m.timestamps.event) {
            self.stats.state_events_stale += 1;
            return;
        }
        e.state_ts = m.timestamps.event;
        // **One word, written once and stored once.** State handling
        // reaches exactly one state word, and the object manager's two readers of bit 0
        // (visible-list update and cell release) read that same
        // word. Here the object tables live in `dereth_client_model`, which cannot see this crate, so the
        // single source is [`dereth_client_model::objects::PhysicsPresence::state`] and this is the only
        // write. Without it a server-side `STATIC_PS` change would move retail's visible list and
        // not ours.
        //
        // The write can land on nothing: `dereth_client_model::World` refuses a create whose instance
        // sequence is stale while this table still holds the presence, so the two can disagree
        // about whether the object exists. Counted rather than assumed — and it is now the *only*
        // signal, because there is no second copy of the word left to inspect.
        if !self.world.set_physics_state(m.id, m.state) {
            self.stats.state_events_without_physics += 1;
        }
        self.stats.state_events += 1;
    }

    /// A `0xF748`'s position, through received-position handling.
    fn received_position(&mut self, m: &MovementPositionEvent, now: LocalTime) {
        self.apply_received_position(
            &ReceivedPosition {
                id: m.id,
                destination: Position::new(
                    dereth_primitives::CellId(m.position.origin.objcell_id),
                    dereth_primitives::Frame::new(
                        m.position.origin.origin.into(),
                        m.position.orientation.into(),
                    ),
                ),
                placement_id: m.position.placement_id,
                contact: m.position.has_contact(),
                position_ts: m.position.position_timestamp,
                teleport_ts: m.position.teleport_timestamp,
                force_position_ts: m.position.force_position_timestamp,
                from_create: false,
            },
            now,
        );
    }

    /// Apply a received position, reduced to what a viewer needs.
    ///
    /// The full handler also runs the player's force-position "blip" and answers it with a
    /// position event of its own; that is the *player's* arm and belongs with the local player
    /// body, which this build keeps outside this table. The ordinary path reproduces the `POSITION_TS` gate and
    /// older-`TELEPORT_TS` rollback before changing position, parent, placement or completion.
    ///
    /// Two messages come here: a `0xF748` (and the position half of a `0xF619`), and an
    /// equal-instance `0xF745` re-create, whose description hands the same handler its position,
    /// its placement and its three position stamps with ground contact set. The `0xF748`
    /// counters in [`ObjectStats`] count only the first.
    fn apply_received_position(&mut self, m: &ReceivedPosition, now: LocalTime) {
        let counted = !m.from_create;
        let is_player = self.world.player == Some(m.id);
        if is_player && counted {
            self.stats.player_positions += 1;
        }
        let Some(e) = self.presences.get_mut(&m.id) else {
            // The session's gate parks a position event for an unknown object, so this only
            // happens for an object the world tables refused.
            self.stats.unhandled += 1;
            return;
        };
        // Whether this object is **in a container** as the message arrives, read
        // before either of the two writes below changes it. The object manager's meaning is
        // exactly "neither a position of its own nor a holder": leaving the world has taken it
        // out of every cell list and zeroed `position.objcell_id`. See
        // the tail of this function for what it is for.
        let was_in_a_container = e.position.is_none() && e.parent.is_none();
        // **The force-position stamp comes first, and only for the player.**
        //
        // Received-position handling opens, for the player only, with a `newer_event` test of
        // the held force-position stamp (slot 6) against the message's, **outside** and
        // **before** the `POSITION_TS` gate below, so the stamp advances even on the arm that
        // returns early. Its body-side half — the force-position "blip": set the heading
        // from the object's heading getter,
        // `update_times[0] = ts`, and answer with the command interpreter's own
        // position event — is the *player body's* arm, which this build's `ObjectStream`
        // does not own; what is reproduced here is the sequence, because that is what goes back
        // out on every `0xF61C` and `0xF753`.
        //
        // **The blip is unobserved in the corpus, and a test pins its count as a literal.**
        // `force_position_ts` is **never advanced once across all 8,617 server blobs** in the
        // corpus — the branch below has fired **0** times in seven captures — which is why
        // every one of the 1,827 recorded outbound position bodies and both recorded `0xF61B`
        // jump bodies carry `force_position = 0`. That zero is currently right *because
        // nothing in the recording advances the sequence*, not because a producer decided it,
        // and the difference matters: the blip is the arm that would make it non-zero, and it
        // has no producer here. Replacing the pin with an oracle needs a capture of a GM
        // teleport or an anti-cheat correction of the recording character.
        if is_player && is_newer(e.force_position_ts, m.force_position_ts) {
            e.force_position_ts = m.force_position_ts;
            if counted {
                self.stats.force_position_stamps += 1;
            }
        }
        // apply and store only when strictly newer.
        if !is_newer(e.position_ts, m.position_ts) {
            if counted {
                self.stats.stale_positions += 1;
            }
            return;
        }
        let previous_position_ts = e.position_ts;
        e.position_ts = m.position_ts;
        // Save POSITION_TS before newer_event, then restore
        // it and return when TELEPORT_TS is older.
        // The reversed is_newer arguments also retain retail's exact 0x8000 half-window rule.
        // This is after the player-only `force_position_ts` update: that earlier accepted stamp
        // survives rejection. No parent/placement/position/teleport callback may run below it.
        if is_newer(m.teleport_ts, e.teleport_ts) {
            e.position_ts = previous_position_ts;
            if counted {
                self.stats.position_teleport_rollbacks += 1;
            }
            return;
        }
        // **Past the gate, `TELEPORT_TS`.**
        //
        // Both arms of received-position handling advance it, and neither is conditional on
        // anything this build lacks: the player's is a bare `newer_event` test of `TELEPORT_TS`
        // against the message's teleport stamp, whose *answer* chooses the player teleport
        // over the constrain correction, and a remote object's is the same call inside
        // the remote-object arm. So the write is common and only what follows
        // it differs — and what follows is the body's, not the table's.
        let teleported = is_newer(e.teleport_ts, m.teleport_ts);
        if teleported {
            e.teleport_ts = m.teleport_ts;
            if counted {
                self.stats.teleport_stamps += 1;
            }
        }
        let destination = m.destination;
        e.set_wire_position(Some(destination));
        // The position-event handler passes these two arguments to
        // move-or-teleport for a non-player object. They are recorded beside the
        // position they describe. `teleported` is the `newer_event(TELEPORT_TS)` answer above,
        // which is the same test `MoveOrTeleport` reruns in the client and which nothing on this
        // build's physics body could answer a second time.
        e.teleported = teleported;
        e.contact = m.contact;
        e.position_from_create = false;
        // **The server moving the player's own body.**
        //
        // Past this gate, position-event handling branches on `obj == player`, and the
        // player's arm teleports the body directly to the supplied position
        // whenever `newer_event(obj, TELEPORT_TS, ts)` answers true. Without this arm
        // `Presence::position` moves but the *body* does not, and the player walks out of portal
        // space standing exactly where he entered it while the server has already put him at the
        // destination. `ObjectPhysics::sync` excludes the player by design (the client has one
        // physics body for him, not two), and the only other call to `Character::teleport` on the
        // connected path is `App::load_pending_scene`'s one-shot at world entry — so this is the
        // only thing after the first frame that can move the player's own body.
        //
        // It is parked rather than applied because the body is `Character`'s; see
        // [`Self::take_player_teleport`] for the arm that is deliberately *not* reproduced.
        if is_player && teleported {
            self.player_motion_dispatches
                .push(PlayerMotionDispatch::Teleport {
                    position: destination,
                    timestamps: dereth_protocol::movement::MoveTimestamps {
                        instance: e.instance,
                        server_control: e.server_control_ts,
                        teleport: e.teleport_ts,
                        force_position: e.force_position_ts,
                    },
                });
            if counted {
                self.stats.player_teleports += 1;
            }
        }
        // Past the gate, position-event handling does `unset_parent`, then
        // tests whether animations are active, and installs the placement only when none are.
        // The placement installed is the third argument — the placement id passed in — not the
        // `Position *`.
        //
        // The active-animation test is the renderer's to answer, because the sequence it
        // asks about is on the `MotionDriver` the renderer owns; the id is parked for it.
        //
        // `unset_parent` comes **before** both, unconditionally: an object the server gives a
        // position of its own to is no longer held by anything. Without it `parent` would be
        // written only by the create and never cleared, so a dropped item would stay attached
        // for ever.
        //
        // This is the shared `unset_parent` helper, which also **clears**
        // `NODRAW_PS` when the holder it is leaving carries `HIDDEN_PS`; it is the only path
        // in the client that does.
        let pid = m.placement_id;
        self.unset_parent(m.id, now);
        if let Some(pid) = pid {
            if let Some(e) = self.presences.get_mut(&m.id) {
                e.pending_placement = Some(pid);
            }
        }
        if !is_player {
            self.position_entries.insert(m.id);
        }
        // **The world drop: a transition out of a container that must reach the renderer.**
        //
        // Without this, a dropped item never appears in the world yet can be picked back up
        // immediately: the object is in the model but has no drawable at all.
        //
        // A successful drop is answered with an inventory-move event and a position update,
        // and **no create**:
        // the dropping client already knows the item, because it was in his pack. So this
        // function is the whole of the client's physical half of a drop.
        //
        // Retail needs nothing more here, because it never deferred the drawable.
        // Object maintenance creates the physics body and its part array
        // on **every** create, before it has looked at the
        // descriptor's parent id at all; an object in a container simply is not in any cell's
        // object list, so cell drawing never reaches the part array that is sitting there ready.
        // Received-position handling unparents it, moves or teleports it into a cell,
        // and it draws on the next frame.
        //
        // This build **does** defer it: `WorldScene::prepare_object_dispatch` skips an object with
        // neither a position nor a parent, which keeps a hundred pack items out of the GPU. That
        // is a defensible deferral and it stays. What it needs is a wake-up -- and this is the
        // moment retail's already-built part array enters the world, so this is where the deferred
        // build has to be completed. The idiom is `parent_event`'s, four screens down, which
        // pushes the same queue for the same reason: *the attachment changes where the object is
        // drawn from, which is what the create path decides.*
        //
        // **Guarded, and the guard is not an optimisation.** `0xF748` is the most common message
        // in the game -- every walking creature sends several a second -- and `take_created`
        // *rebuilds* a `SceneObject`, motion driver and all. Offering an object that already has
        // one would reset its animation every frame. `was_in_a_container` is true only for an
        // object that had no position and no holder, which is precisely the set
        // `prepare_object_dispatch` skipped.
        //
        // The player is excluded because he has no `SceneObject` in the first place: with a local
        // body `Character` draws him, and his arm above is the teleport dispatch.
        //
        // **One bounded deviation, named rather than left to be found.** An object that was in
        // the world, was picked up (`0xF74A` -> `pickup_event`, which leaves exactly this state)
        // and is now dropped again already *has* a `SceneObject`, and the create loop replaces it
        // rather than reusing it -- retail keeps the one part array object creation built. The cost
        // is one `AppearanceKey` cache hit and a fresh `MotionDriver`; the thing being re-made is
        // a loose item on the ground with nothing playing on it. It happens once per drop, not
        // once per frame, which is what the guard above is for.
        if was_in_a_container && !is_player {
            self.created.push(m.id);
            if counted {
                self.stats.container_exits_offered += 1;
            }
        }
        if counted {
            self.stats.position_updates += 1;
        }
    }

    /// Smart-box pickup application, after the handler's lookups.
    ///
    /// When the event stamp is newer than the object's `POSITION_TS` stamp, the stamp is stored,
    /// the object's parent is cleared and the object is removed from the world.
    ///
    /// This is the server saying "that object is in a container now". It is **not** a delete: the
    /// object stays in `object_table` -- leaving the world only takes it out of the cells
    /// and zeroes `position.objcell_id`, which is [`Presence::position`] here -- so a later
    /// `0xF749` still finds it. Measured over the seven recorded captures: **52 pickups, 52 of
    /// them for an object the tables already hold, 36 of them for an object attached to a holder
    /// at that moment** as a build with no handler sees it -- 35 with this one live, the
    /// difference being one repeat pickup on an item the previous one had already un-handed --
    /// and only **4** are ever followed by a `0xF747` for the same object. Without this handler
    /// all 52 would fall through to `stats.unhandled` and those items would stay drawn in the
    /// hand they had just left.
    fn pickup_event(&mut self, m: &InventoryPickupEvent, now: LocalTime) {
        let Some(e) = self.presences.get_mut(&m.id) else {
            self.stats.pickup_events_unknown += 1;
            return;
        };
        // Test and advance `update_times[POSITION_TS]`, the same slot and
        // the same test `0xF748` and `0xF749` are gated on.
        if !is_newer(e.position_ts, m.timestamps.event) {
            self.stats.pickup_events_stale += 1;
            return;
        }
        e.position_ts = m.timestamps.event;
        self.unset_parent(m.id, now);
        self.world.leave_physics_world(m.id);
        // `leave_world`: `report_collision_end`, out of every cell list, and
        // `position.objcell_id = 0`. The cell lists are [`crate::world_state::WorldState`]'s and the
        // objcell is this field; an object with neither a position nor a holder is exactly the
        // "in a container" case the create path already refuses to put on screen.
        if let Some(e) = self.presences.get_mut(&m.id) {
            e.set_wire_position(None);
        }
        self.stats.pickup_events += 1;
    }

    /// Detach an object from its holder in this build's terms.
    ///
    /// ```text
    /// if the object has a parent:
    ///     remove this object from its parent's child list
    ///     if the parent's state has HIDDEN_PS:
    ///         clear NODRAW_PS from this object's state
    ///         make this object's part array drawable
    ///     clear parent; stamp update time; clear transient states
    /// ```
    ///
    /// **The `HIDDEN_PS` arm belongs to the scene, not here.** A child of a hidden holder carries
    /// `NODRAW_PS` for as long as it is attached and only this function takes it off again -- but
    /// [`dereth_client_model::objects::PhysicsPresence::state`] is *the word the server sent*, and
    /// the scene keeps the live word on
    /// `SceneObject::state` with a `wire_state` latch precisely so that a `NODRAW_PS` the client
    /// forces on a child cannot be mistaken for a new word from the server. Writing the bit here
    /// would be a second transcription of `reparent_nodraw`, fighting the latch. This function
    /// moves the edge; `WorldScene::sync_objects` watches the edge change and applies the bit.
    ///
    /// World owns real/null child edges and the null update-time edge; Presence only projects
    /// it. Ordered CHILDLIST traversal and the live body's transient-state tail remain outside
    /// this model seam.
    fn unset_parent(&mut self, id: ObjectId, now: LocalTime) {
        self.world.unset_physics_parent(id, ServerTime(now.0));
        self.project_parent(id);
    }

    /// Attach an object to a holder in this build's terms.
    ///
    /// ```text
    /// if attachment validation fails, return 0
    /// detach from the old parent; leave world cells; store the new parent
    /// ... update the cell, child pose, and cross-cell membership ...
    /// if the new parent's state has HIDDEN_PS:
    ///     set NODRAW_PS and hide this object's part array
    /// ```
    ///
    /// Child attachment is validated by World using App-resolved DAT part-array facts.
    /// Refusal occurs before changing the old edge, position or deadline, including physical
    /// nulls. The renderer consumes the accepted edge, not an unvalidated descriptor link.
    /// The existing HIDDEN_PS/live-body tail is described in [`Self::unset_parent`].
    fn set_parent(&mut self, id: ObjectId, holder: ObjectId, location: u32, now: LocalTime) {
        if !self
            .world
            .set_physics_parent(id, holder, location, ServerTime(now.0))
        {
            self.stats.parent_links_refused += 1;
        }
        self.project_parent(id);
    }

    /// Project a **holder's** descriptor listing what it holds.
    ///
    /// ```text
    /// detach the holder's existing children
    /// for i in 0 .. pd.num_children:
    ///     child = find real object by id or find its placeholder
    ///     attach child at the descriptor's requested location
    /// ```
    ///
    /// Object creation calls it on **every** create, after
    /// description setup and appearance changes and **before** the holder's own parent assignment.
    /// `unparent_children` is unconditional, so a create for a holder whose descriptor lists
    /// nothing detaches everything it was holding.
    ///
    /// In the recorded corpus the list is redundant -- 18 of 889 creates carry one, 18 links, and
    /// all 10 creates that arrive for a holder with children already attached carry a list, so the
    /// detach can never strand anything there. What the message exists for is the **ordering**
    /// it handles: a holder whose create lists a child that arrived
    /// first, where the child's own descriptor named a parent that did not exist yet.
    ///
    /// [`dereth_client_model::World`] has already applied the child list, including placeholder attachment.
    /// This projects real descriptions only. The unknown counter identifies a physical null,
    /// not a dropped link. Later placeholder initialization promotes that same accepted edge.
    fn project_created_children(
        &mut self,
        holder: ObjectId,
        d: &dereth_protocol::types::PhysicsDesc,
    ) {
        // Project final edges; do not count these as a second unparent_children call. A fresh
        // holder has no children before child-list replacement (recreation retired the old one).
        let current: Vec<ObjectId> = self
            .presences
            .iter()
            .filter(|(_, e)| e.parent.is_some_and(|(h, _)| h == holder))
            .map(|(id, _)| *id)
            .collect();
        for child in current {
            self.project_parent(child);
        }
        let Some(links) = d.children.clone() else {
            return;
        };
        for l in links {
            if !self.presences.contains_key(&l.child_id) {
                self.stats.children_unknown += 1;
                continue;
            }
            self.project_parent(l.child_id);
            if self.world.physics_parent(l.child_id) == Some((holder, l.location_id)) {
                self.stats.children_attached += 1;
            }
            // The attachment changes where the child is drawn from, which is what the create path
            // decides -- offered again exactly as a `0xF749` offers it.
            self.created.push(l.child_id);
        }
    }

    /// Smart-box parent-event application, after the handler's lookups.
    ///
    /// 1. Find the creature (the first guid); if it is missing, or `ts.instance` is newer than
    ///    its `INSTANCE_TS` stamp, queue on the creature's guid.
    /// 2. Find the item (the second guid); if it is missing, queue on the item's guid.
    /// 3. If the creature's `INSTANCE_TS` stamp differs from `ts.instance`: `OLD_INSTANCE`.
    /// 4. Apply the parent event to item, creature, location, placement and event stamp: return
    ///    unless `ts.event` is newer than the item's `POSITION_TS` stamp; store the stamp;
    ///    `set_parent(item, creature, location)`; install the requested placement frame.
    ///
    /// **The argument order matters:** the placement installed is the fourth argument (the
    /// placement), not the third (the parent location), and `set_parent` receives the parent
    /// location as shown above. See [`dereth_protocol::objects::ItemParentEvent`] for the guid order.
    ///
    /// There is **no active-animation guard here**, unlike received-position handling: a parent event
    /// installs the placement whatever the item is doing.
    ///
    /// The renderer is offered the item again exactly as a `0xF625` offers it — the attachment
    /// changes where the object is drawn from, which is what the create path decides.
    fn parent_event(&mut self, m: &ItemParentEvent, now: LocalTime) {
        if !self.presences.contains_key(&m.creature) {
            self.stats.parent_events_unknown_creature += 1;
            return;
        }
        let Some(item) = self.presences.get_mut(&m.item) else {
            // the blob is re-queued on the **item's** id, which the session's gate —
            // keyed on the leading guid — cannot do. Counted rather than silently dropped.
            self.stats.parent_events_unknown_item += 1;
            return;
        };
        // Test and advance the item's `POSITION_TS`, exactly as `0xF748`
        // is gated: apply and store only when strictly newer.
        if !is_newer(item.position_ts, m.timestamps.event) {
            self.stats.parent_events_stale += 1;
            return;
        }
        item.position_ts = m.timestamps.event;
        item.placement = m.placement_frame;
        // `set_parent(item, creature, location)` -- the shared helper, so a wield onto a
        // **hidden** holder takes the `NODRAW_PS` arm that the original parenting operation
        // applies.
        self.set_parent(m.item, m.creature, m.location, now);
        let item = self.presences.get_mut(&m.item).expect("checked above");
        // A parked id from an earlier `0xF748` would otherwise re-pose the item behind this one's
        // back on the next frame; this placement write is the later word.
        item.pending_placement = None;
        self.stats.parent_events += 1;
        self.created.push(m.item);
    }

    /// Apply the movement update's two stamp gates, then hand the buffer to the
    /// renderer, which owns the movement interpreter attached to the physics body.
    ///
    /// Takes the id and the decoded buffer rather than a `0xF74C`, because
    /// the smart-box dispatch arm calls the same movement setter with a buffer it found
    /// after a `PositionPack` instead of after a bare `u16`. Returns retail's own return value —
    /// non-zero when the movement was applied — which is what the dispatch arm tests before
    /// making the command interpreter relinquish control to the server.
    fn set_object_movement(&mut self, id: ObjectId, buf: MovementBuffer) -> bool {
        let Some(e) = self.presences.get_mut(&id) else {
            self.stats.unhandled += 1;
            return false;
        };
        // "if (!is_newer(update_times[MOVEMENT_TS], movement_ts)) return 0" — a stale movement is
        // ignored outright.
        if !is_newer(e.movement_ts, buf.movement_timestamp) {
            self.stats.movement_stale += 1;
            return false;
        }
        e.movement_ts = buf.movement_timestamp;
        // "if (is_newer(server_ts, update_times[5])) return 0", slot 5 being the
        // server-controlled-move stamp — note the arguments are the other way round from the
        // line above; a *newer* stored stamp wins.
        //
        // Follows retail's movement-event handler and is **never exercised by
        // the corpus**: all 304 recorded
        // buffers carry a server-control stamp at or ahead of the object's, so this arm has no
        // oracle behind it. The counter exists so that the first traffic that trips it is visible
        // rather than silent.
        if is_newer(buf.server_control_timestamp, e.server_control_ts) {
            self.stats.movement_old_control += 1;
            return false;
        }
        e.server_control_ts = buf.server_control_timestamp;
        // "if (autonomous && is_the_player) return 0" — the server echoing what we just sent.
        if buf.autonomous && self.world.is_the_player(id) {
            self.stats.movement_own_echo += 1;
            return false;
        }
        if self.world.is_the_player(id) {
            self.player_motion_dispatches
                .push(PlayerMotionDispatch::Movement(buf.clone()));
        }
        // `self.presences` was borrowed above; re-take it so the two `is_the_player` reads above
        // could use `self.world`. The entry cannot have gone away in between.
        if let Some(e) = self.presences.get_mut(&id) {
            e.pending_movement = Some(buf);
        }
        self.stats.movement_updates += 1;
        true
    }

    /// The complete smart-box vector-update path.
    ///
    /// Compare the stored vector stamp with the message's event stamp using the `0x7FFF`
    /// half-window. A stale event writes nothing. A newer event writes the stamp first,
    /// then checks whether the object is the player. Non-player updates proceed directly;
    /// player updates proceed only if server position is accepted. Finally set velocity
    /// and angular velocity with notification enabled.
    ///
    /// Three orderings that a paraphrase loses and that this transcription keeps:
    ///
    /// * the stamp is written **before** the player test, so a message the autonomy
    ///   arm throws away still advances `VECTOR_TS`;
    /// * the stale branch uses equality after `is_newer`, so an **equal** stamp is stale — the same strictness
    ///   the state update's `STATE_TS` gate has;
    /// * `UsePositionFromServer` is `autonomy_level != 2` and
    ///   the command interpreter starts at 2, so the **default** answer
    ///   is *false*: a client that has never taken control ignores every vector update about its
    ///   own body. [`Self::note_use_position_from_server`] is what makes that answer live, and its
    ///   default here is retail's.
    ///
    /// `set_velocity`/`set_omega` are not called from here for the same reason a movement buffer
    /// is not applied here: they belong to the `dereth_physics` body, which this table does
    /// not hold. The pair is parked and `crate::world_objects::sync_objects` applies it.
    fn vector_update(&mut self, m: &MovementVectorUpdate) {
        let is_player = self.world.is_the_player(m.id);
        let use_position_from_server = self.use_position_from_server;
        let Some(e) = self.presences.get_mut(&m.id) else {
            // The original receiver parks updates for missing objects. The session gate
            // already handles that parking, so reaching here means gameplay refused the object.
            self.stats.vector_unknown += 1;
            return;
        };
        if !is_newer(e.vector_ts, m.timestamps.event) {
            self.stats.vector_stale += 1;
            return;
        }
        e.vector_ts = m.timestamps.event;
        if is_player && !use_position_from_server {
            self.stats.vector_player_autonomous += 1;
            return;
        }
        e.pending_vector = Some((m.velocity.into(), m.omega.into()));
        self.stats.vector_updates += 1;
    }

    /// The composite `0xF619` position/movement event, inlined into movement-event dispatch.
    /// See the match arm in the WorldObjects dispatch for the ordering.
    ///
    /// It is a composition of two handlers this file already has — received-position handling
    /// through [`Self::received_position`], then movement handling through
    /// [`Self::set_object_movement`] — and the composition, not either half,
    /// is what this adds. The position half runs first and its verdict does **not**
    /// gate the movement half: the composite handler returns `1` regardless of what the position
    /// half did with the stamps, because that function
    /// returns void.
    ///
    /// The movement half's non-zero return is the lose-control-to-server edge, and it reaches
    /// `App` by the route `0xF74C` already uses: `PlayerMotionDispatch::Movement` ->
    /// `WorldScene::apply_player_movement` -> `take_player_movement_applied` ->
    /// `command_interpreter_control_transfer`. So nothing new is owed there either.
    fn position_and_movement_event(
        &mut self,
        m: &MovementPositionAndMovementEvent,
        now: LocalTime,
    ) {
        // Position unpacking leaves the cursor here; dispatch rereads it. A
        // `PositionPack` is a whole number of dwords, so the buffer's blob offset is `0 (mod 4)`
        // and the origin is **not** the bare movement packet's blob origin (10).
        let mut r = Reader::with_origin(&m.movement, 0);
        let buf = MovementBuffer::read(&mut r)
            .ok()
            .filter(|_| r.expect_exhausted().is_ok());
        let Some(buf) = buf else {
            self.stats.position_and_movement_undecodable += 1;
            return;
        };
        // Apply position through the shared `0xF748` unpacking path,
        // so the position half is literally the same code and cannot drift from it.
        self.received_position(
            &MovementPositionEvent {
                id: m.id,
                position: m.position,
            },
            now,
        );
        // Apply movement unconditionally, regardless of the position result above.
        self.set_object_movement(m.id, buf);
        self.stats.position_and_movement_events += 1;
    }

    /// Character-session end. The counters survive, because they are this run's log line.
    fn reset(&mut self) {
        // A request names the old character session and must never be replayed after reset.
        self.pending_requests.clear();
        if self.presences.is_empty() && self.world.player.is_none() {
            return;
        }
        self.removed.extend(self.presences.keys().copied());
        for id in self.presences.keys().copied() {
            self.physics.retire(id);
        }
        self.presences.clear();
        self.created.clear();
        self.notices.clear();
        self.session_deleted.clear();
        self.session_physics_deleted.clear();
        self.position_entries.clear();
        // A sound event for an object the session just tore down has nothing to play
        // on and nothing to play it at, so it goes with the tables it named.
        self.sound_events.clear();
        // Same rule for a script trigger -- it names an object that no longer exists.
        self.script_events.clear();
        // A teleport destination whose session has been torn down names a body that no
        // longer exists.
        self.player_motion_dispatches.clear();
        let mut fresh = new_world();
        fresh.magic.preserve_receipt_serials_from(&self.world.magic);
        self.world.journal.set_identity(None);
        fresh.journal = std::mem::take(&mut self.world.journal);
        // The local chat DLL and communication-system rooms/spam bucket are process-owned,
        // not character-owned. Character-session end only unregisters its quality watch.
        fresh
            .chat
            .preserve_turbine_provider_from(&mut self.world.chat);
        // The chat log file is likewise process-owned. Log closing occurs only at
        // shutdown, output replacement, and replacement open; character-session
        // teardown must not silently stop a typed `@log`.
        fresh.scroll.preserve_output_from(&self.world.scroll);
        fresh.preserve_material_names_from(&mut self.world);
        fresh.preserve_vital_formulas_from(&mut self.world);
        // The world's rules are the world's, not the character's.
        fresh.world_rules = std::mem::take(&mut self.world.world_rules);
        self.world = fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    use dereth_protocol::{write_body, Message};

    fn create_blob(id: u32, instance: u16, setup: u32, position: Option<u32>) -> Vec<u8> {
        let mut physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP,
            setup_id: Some(setup),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        };
        if let Some(cell) = position {
            physicsdesc.bitfield |= flags::POSITION;
            physicsdesc.position = Some(dereth_protocol::types::PositionWire {
                objcell_id: cell,
                ..dereth_protocol::types::PositionWire::default()
            });
        }
        write_body(&ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: ObjectId(id),
                objdesc: ObjDesc::default(),
                physicsdesc,
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode")
    }

    fn ev(op: Opcode, body: Vec<u8>) -> SessionEvent {
        SessionEvent::WorldObject { opcode: op, body }
    }

    /// Behaviour: none (actual session reset preserves only spell receipt counters).
    #[test]
    fn character_session_reset_discards_spell_receipts_but_not_their_identity_sequence() {
        let mut objects = ObjectStream::new();
        objects.world.player = Some(ObjectId(1));
        objects.world.research_spell_update(7, true);
        assert_eq!(objects.world.magic.last_learned_spell, Some((1, 7)));
        objects.reset();
        assert_eq!(objects.world.magic.last_learned_spell, None);
        assert_eq!(objects.world.magic.research_success, None);
        objects.world.player = Some(ObjectId(2));
        objects.world.research_spell_update(8, true);
        assert_eq!(objects.world.magic.last_learned_spell, Some((2, 8)));
    }

    #[test]
    fn character_session_reset_preserves_the_process_chat_output_handle() {
        let dir =
            std::env::temp_dir().join(format!("dereth-object-reset-log-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
        }
        std::fs::create_dir_all(&dir).expect("create disposable output directory");
        let path = dir.join("chat.txt");
        let mut objects = ObjectStream::new();
        assert!(
            objects
                .world
                .scroll
                .start_copy_output_to_file(&path.to_string_lossy(), 0, || {
                    crate::platform::text::open_chat_log(&path)
                }),
            "open process chat output"
        );
        objects.world.player = Some(ObjectId(0x5000_0001));
        objects.reset();
        objects
            .world
            .scroll
            .add_text_to_scroll("after reset", 0, true, 0);
        assert!(
            objects.world.scroll.close_log_file(0),
            "reset retained the open handle"
        );
        let text = std::fs::read_to_string(&path).expect("read reset output");
        assert!(
            text.starts_with("after reset\r\n"),
            "fresh World lost the old append handle"
        );
        assert!(
            text.ends_with(&format!("Chat log {} closed.\r\n", path.display())),
            "fresh World lost the native log name used at close"
        );
        std::fs::remove_dir_all(&dir).expect("remove disposable output directory");
    }

    #[test]
    fn character_session_reset_preserves_material_names() {
        let mut objects = ObjectStream::new();
        objects
            .world
            .install_material_names(std::collections::BTreeMap::from([(0x3A, "Bronze".into())]));
        objects.world.player = Some(ObjectId(0x5000_0001));

        objects.reset();

        assert_eq!(objects.world.material_name(0x3A), Some("Bronze"));
        assert_eq!(
            objects.world.material_name(0x3B),
            None,
            "a missing mapper row stays missing"
        );
    }

    fn create_blob_full(
        id: u32,
        setup: u32,
        animframe: Option<u32>,
        mtable: Option<u32>,
        movement: Option<Vec<u8>>,
    ) -> Vec<u8> {
        let mut physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP | flags::POSITION,
            setup_id: Some(setup),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: 0x00A9_B401,
                ..dereth_protocol::types::PositionWire::default()
            }),
            ..PhysicsDesc::default()
        };
        if let Some(mt) = mtable {
            physicsdesc.bitfield |= flags::MTABLE;
            physicsdesc.mtable_id = Some(mt);
        }
        // Physics-description unpacking reads the movement buffer **or** the animframe, never
        // both: they share one `if`/`else if`, which is exactly why description setup
        // can use the absence of a buffer as its test for "the placement field is meaningful".
        if let Some(buf) = movement {
            physicsdesc.bitfield |= flags::MOVEMENT;
            physicsdesc.movement = Some((buf, 0));
        } else if let Some(a) = animframe {
            physicsdesc.bitfield |= flags::ANIMFRAME;
            physicsdesc.animframe_id = Some(a);
        }
        write_body(&ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: ObjectId(id),
                objdesc: ObjDesc::default(),
                physicsdesc,
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode")
    }

    fn position_blob(id: u32, ts: u16, placement: Option<u32>) -> Vec<u8> {
        use dereth_protocol::movement::position_flags;
        let mut pp = dereth_protocol::movement::PositionPack {
            origin: dereth_protocol::types::Origin {
                objcell_id: 0x00A9_B401,
                origin: dereth_protocol::types::Vec3::default(),
            },
            position_timestamp: ts,
            ..dereth_protocol::movement::PositionPack::default()
        };
        if let Some(pid) = placement {
            pp.flags |= position_flags::HAS_PLACEMENT_ID;
            pp.placement_id = Some(pid);
        }
        write_body(&MovementPositionEvent {
            id: ObjectId(id),
            position: pp,
        })
        .expect("encode")
    }

    /// Synthetic wire-only marker lifetime controls; no body placement is asserted here.
    #[test]
    fn accepted_remote_position_marker_dies_with_its_object_or_session_not_with_shared_stamp() {
        let id = ObjectId(0x37);
        let mut s = ObjectStream::new();
        let spawn = |instance| {
            ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob(id.0, instance, 0x0200_0124, Some(0x00A9_B401)),
            )
        };
        let pos = || {
            ev(
                Opcode::MOVEMENT_POSITION_EVENT,
                position_blob(id.0, 1, None),
            )
        };
        s.apply_event(&spawn(0), LocalTime(1.0));
        assert!(
            s.position_entries.is_empty(),
            "create placement is a different owner"
        );
        s.apply_event(&pos(), LocalTime(2.0));
        assert_eq!(s.position_entries, BTreeSet::from([id]));
        let delete = ItemDeleteObject {
            id,
            instance_sequence: 0,
        };
        s.apply_event(
            &ev(ItemDeleteObject::OPCODE, write_body(&delete).unwrap()),
            LocalTime(3.0),
        );
        assert!(s.position_entries.is_empty());
        s.apply_event(&spawn(0), LocalTime(4.0));
        assert!(
            s.position_entries.is_empty(),
            "same-id successor cannot inherit acceptance"
        );
        s.apply_event(&pos(), LocalTime(5.0));
        assert_eq!(s.position_entries, BTreeSet::from([id]));
        s.apply_event(&spawn(1), LocalTime(6.0));
        assert!(
            s.position_entries.is_empty(),
            "implicit newer-instance retirement also clears"
        );
        s.apply_event(&pos(), LocalTime(7.0));
        assert_eq!(s.position_entries, BTreeSet::from([id]));
        s.reset();
        assert!(s.position_entries.is_empty());
        s.apply_event(&SessionEvent::PlayerCreated(id), LocalTime(8.0));
        s.apply_event(&spawn(0), LocalTime(8.0));
        s.apply_event(&pos(), LocalTime(9.0));
        assert!(
            s.position_entries.is_empty(),
            "local player retains its separate position owner"
        );
    }

    /// Synthetic producer/lifetime controls: local descriptor motion uses the ordered path;
    /// remote and bodyless consumers retain their documented latest-snapshot interface.
    #[test]
    fn player_dispatch_journal_preserves_create_autonomy_and_bodyless_reset_lifetimes() {
        let player = ObjectId(0x30);
        let mut writer = dereth_protocol::Writer::new();
        MovementBody {
            interpreted: Some(Default::default()),
            ..Default::default()
        }
        .write(&mut writer)
        .unwrap();
        let motion = writer.into_inner();
        for autonomous in [0, 1] {
            let mut s = ObjectStream::new();
            s.apply_event(&SessionEvent::PlayerCreated(player), LocalTime(0.0));
            let blob = create_blob_full(player.0, 0x0200_0124, None, None, Some(motion.clone()));
            let mut create = ItemCreateObject::read(&mut Reader::new(&blob)).unwrap();
            create.0.physicsdesc.movement.as_mut().unwrap().1 = autonomous;
            s.apply_event(
                &ev(Opcode::ITEM_CREATE_OBJECT, write_body(&create).unwrap()),
                LocalTime(0.0),
            );
            assert!(
                matches!(&s.player_motion_dispatches[..], [PlayerMotionDispatch::Movement(m)]
                if m.autonomous == (autonomous != 0))
            );
            let snapshot = s
                .presence(player)
                .unwrap()
                .pending_movement
                .clone()
                .unwrap();
            assert_eq!(s.discard_bodyless_player_dispatches(), 0);
            assert!(!s.has_player_motion_dispatches());
            assert_eq!(
                s.take_movement(player),
                Some(snapshot),
                "bodyless latest motion survives"
            );
            assert!(s.take_movement(player).is_none());

            // A remote create is not a player dispatch, and no-motion local creates do not
            // manufacture an entry. Both still follow the ordinary create path.
            create.0.id = ObjectId(0x31);
            s.apply_event(
                &ev(Opcode::ITEM_CREATE_OBJECT, write_body(&create).unwrap()),
                LocalTime(0.0),
            );
            assert!(!s.has_player_motion_dispatches());
            assert!(s.take_movement(create.0.id).is_some());
            create.0.id = player;
            create.0.physicsdesc.movement = None;
            create.0.physicsdesc.bitfield &= !flags::MOVEMENT;
            s.apply_event(
                &ev(Opcode::ITEM_CREATE_OBJECT, write_body(&create).unwrap()),
                LocalTime(0.0),
            );
            assert!(!s.has_player_motion_dispatches());

            let mut pos =
                MovementPositionEvent::read(&mut Reader::new(&position_blob(player.0, 1, None)))
                    .unwrap();
            pos.position.teleport_timestamp = 1;
            s.apply_event(
                &ev(Opcode::MOVEMENT_POSITION_EVENT, write_body(&pos).unwrap()),
                LocalTime(0.0),
            );
            assert!(s.has_player_motion_dispatches());
            assert_eq!(s.discard_bodyless_player_dispatches(), 1);
            assert_eq!(s.discard_bodyless_player_dispatches(), 0);
            pos.position.position_timestamp = 2;
            pos.position.teleport_timestamp = 2;
            s.apply_event(
                &ev(Opcode::MOVEMENT_POSITION_EVENT, write_body(&pos).unwrap()),
                LocalTime(0.0),
            );
            assert!(s.has_player_motion_dispatches());
            s.reset();
            assert!(!s.has_player_motion_dispatches());
            assert!(s.take_player_teleport().is_none());
        }
    }

    /// A create naming neither a buffer nor a placement installs default.
    #[test]
    fn a_create_naming_neither_a_buffer_nor_a_placement_installs_default() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        s.apply(
            &ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob(0x30, 1, 0x0200_0124, Some(0x00A9_B401)),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(
            s.presence(ObjectId(0x30)).expect("a presence").placement,
            crate::models::PLACEMENT_DEFAULT,
            "the descriptor's zeroed animframe field wins over CreateSetup's 0x65"
        );
    }

    /// A presence with no descriptor yet is at resting.
    #[test]
    fn a_presence_with_no_descriptor_yet_is_at_resting() {
        assert_eq!(
            Presence::default().placement,
            crate::models::PLACEMENT_RESTING
        );
        assert_eq!(Presence::default().pending_placement, None);
    }

    /// **A create that names one is posed by it.**
    ///
    /// Oracle: the packet corpus. All 21 of its drawable non-`Resting` creates are setup
    /// `0x02000124` at `MissileFlight (52)` — a missile in flight — and that setup does carry a
    /// key 52 (`dereth/client/tests/dat/objects/server_placement.rs`). The id travels in `PhysicsDesc`'s `0x00020000`
    /// "animframe" field, which loads from `[desc + 0x14]`.
    #[test]
    fn a_create_that_names_a_placement_is_posed_by_it() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        s.apply(
            &ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob_full(0x31, 0x0200_0124, Some(52), None, None),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(
            s.presence(ObjectId(0x31)).expect("a presence").placement,
            52
        );
    }

    /// **A create that carries a movement buffer instead keeps the installed placement.**
    ///
    /// Description application chooses between a movement buffer and a placement: it reads
    /// the placement only when the buffer pointer is null. 502 of the corpus's 839 creates
    /// take the movement-buffer arm.
    #[test]
    fn a_create_with_a_movement_buffer_keeps_the_installed_placement() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        s.apply(
            &ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob_full(0x32, 0x0200_0124, Some(52), None, Some(vec![0xFF; 8])),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(
            s.presence(ObjectId(0x32)).expect("a presence").placement,
            crate::models::PLACEMENT_RESTING,
            "the animframe field is not even present on the wire when a buffer is"
        );
    }

    /// **A `0xF748` that names a placement parks it for the draw**, and one that does not leaves
    /// the object alone. The placement-setting argument is the third stack word, not the
    /// `Position *`.
    ///
    /// It is *parked* rather than installed because the client first checks for active
    /// animations, and the sequence it asks about belongs to the renderer.
    #[test]
    fn a_position_event_that_names_a_placement_parks_it_for_the_draw() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        s.apply(
            &ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob_full(0x33, 0x0200_0124, None, None, None),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        // The create named neither a buffer nor a placement, so `set_description` installed
        // `Placement.Default`; what matters below is that the position event moves it off that.
        assert_eq!(
            s.presence(ObjectId(0x33)).unwrap().placement,
            crate::models::PLACEMENT_DEFAULT
        );
        assert_eq!(s.presence(ObjectId(0x33)).unwrap().pending_placement, None);

        s.apply(
            &ev(
                Opcode::MOVEMENT_POSITION_EVENT,
                position_blob(0x33, 5, Some(3)),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(s.stats.position_updates, 1);
        assert_eq!(
            s.presence(ObjectId(0x33)).unwrap().pending_placement,
            Some(3),
            "LeftHand, named by the server and waiting for the HasAnims test"
        );

        // The draw takes it once and it does not come back.
        assert_eq!(s.take_pending_placement(ObjectId(0x33)), Some(3));
        assert_eq!(s.take_pending_placement(ObjectId(0x33)), None);
        s.placement_installed(ObjectId(0x33), 3);
        assert_eq!(s.presence(ObjectId(0x33)).unwrap().placement, 3);

        // A later event that names no placement must not park anything. Placement changes only
        // when the flag was set.
        s.apply(
            &ev(
                Opcode::MOVEMENT_POSITION_EVENT,
                position_blob(0x33, 6, None),
            ),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(s.stats.position_updates, 2);
        assert_eq!(s.presence(ObjectId(0x33)).unwrap().pending_placement, None);
        assert_eq!(s.presence(ObjectId(0x33)).unwrap().placement, 3);
    }

    /// A create reaches both halves and is offered to the renderer once.
    #[test]
    fn a_create_reaches_both_halves_and_is_offered_to_the_renderer_once() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        let blob = create_blob(0x5000_0001, 9, 0x0200_0001, Some(0x00A9_B401));
        s.apply(
            &ev(Opcode::ITEM_CREATE_OBJECT, blob.clone()),
            LocalTime(0.0),
            &mut arrived,
        );

        assert_eq!(s.stats.creates, 1);
        assert_eq!(
            arrived,
            vec![(ObjectId(0x5000_0001), 9)],
            "the object_arrived seam"
        );
        let p = s.presence(ObjectId(0x5000_0001)).expect("a presence");
        assert_eq!(p.setup_id, Some(DataId(0x0200_0001)));
        assert_eq!(p.position.map(|q| q.cell.0), Some(0x00A9_B401));
        assert!(
            s.world.weenie(ObjectId(0x5000_0001)).is_some(),
            "The object-type table"
        );
        assert_eq!(s.take_created().len(), 1);

        // The same create again is the in-place merge, and offers nothing new to draw.
        s.apply(
            &ev(Opcode::ITEM_CREATE_OBJECT, blob),
            LocalTime(0.0),
            &mut arrived,
        );
        assert_eq!(s.stats.merges, 1);
        assert!(s.take_created().is_empty());
    }

    /// A create at `x` along the cell, with its position and teleport stamps.
    fn stamped_create(
        id: u32,
        instance: u16,
        position_ts: u16,
        teleport_ts: u16,
        x: f32,
    ) -> SessionEvent {
        let physicsdesc = PhysicsDesc {
            bitfield: flags::SETUP | flags::POSITION | flags::ANIMFRAME,
            setup_id: Some(0x0200_0124),
            animframe_id: Some(0x65),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: 0x00A9_B401,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_protocol::types::Vec3 { x, y: 4.0, z: 0.0 },
                    ..dereth_protocol::types::Frame::default()
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance,
                position: position_ts,
                teleport: teleport_ts,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        };
        ev(
            Opcode::ITEM_CREATE_OBJECT,
            write_body(&ItemCreateObject(
                dereth_protocol::objects::ObjectCreatePayload {
                    id: ObjectId(id),
                    objdesc: ObjDesc::default(),
                    physicsdesc,
                    wdesc: PublicWeenieDesc::default(),
                },
            ))
            .expect("encode"),
        )
    }

    /// An equal-instance re-create hands its position to received-position handling: it is not
    /// a create's position, it says the object is on the ground, it is gated and rolled back by
    /// the stamps the object already holds, and its placement is parked for the draw rather than
    /// installed. None of it is counted as a `0xF748`.
    #[test]
    fn an_equal_instance_re_create_moves_the_object_as_a_position_event_does() {
        let id = ObjectId(0x38);
        let mut s = ObjectStream::new();
        let x = |s: &ObjectStream| {
            s.presence(id)
                .and_then(|p| p.position)
                .map(|p| p.frame.origin.x)
        };
        s.apply_event(&stamped_create(id.0, 1, 5, 5, 0.0), LocalTime(0.0));
        let p = s.presence(id).expect("created");
        assert!(
            p.position_from_create,
            "a create's position is the create's"
        );
        assert_eq!(
            (p.position_ts, p.teleport_ts),
            (5, 5),
            "a create seeds the stamps"
        );
        assert_eq!(p.placement, 0x65, "a create installs its placement");
        s.placement_installed(id, 0x15);

        // Newer: it moves, as an update with contact, and parks its placement.
        s.apply_event(&stamped_create(id.0, 1, 6, 5, 3.0), LocalTime(1.0));
        let p = s.presence(id).expect("merged");
        assert_eq!(s.stats.merges, 1);
        assert_eq!(x(&s), Some(3.0));
        assert!(
            !p.position_from_create,
            "a re-create's position is a received position"
        );
        assert!(p.contact && !p.teleported);
        assert_eq!(p.position_ts, 6);
        assert_eq!(
            p.placement, 0x15,
            "the merge installs no placement of its own"
        );
        assert_eq!(
            p.pending_placement,
            Some(0x65),
            "it parks the description's for the draw"
        );
        assert!(s.position_entries.contains(&id));

        // Not newer: nothing moves.
        s.apply_event(&stamped_create(id.0, 1, 6, 5, 9.0), LocalTime(2.0));
        assert_eq!(x(&s), Some(3.0), "an equal position stamp moves nothing");
        // Newer, but with an older teleport stamp: rolled back, and the stamp can be used again.
        s.apply_event(&stamped_create(id.0, 1, 7, 4, 9.0), LocalTime(3.0));
        assert_eq!(x(&s), Some(3.0), "an older teleport stamp moves nothing");
        assert_eq!(s.presence(id).expect("merged").position_ts, 6);
        // A newer teleport stamp is a teleport.
        s.apply_event(&stamped_create(id.0, 1, 7, 6, 12.0), LocalTime(4.0));
        let p = s.presence(id).expect("merged");
        assert_eq!(x(&s), Some(12.0));
        assert!(p.teleported && p.contact);
        assert_eq!((p.position_ts, p.teleport_ts), (7, 6));

        assert_eq!(
            (
                s.stats.position_updates,
                s.stats.stale_positions,
                s.stats.position_teleport_rollbacks
            ),
            (0, 0, 0),
            "the 0xF748 counters count no re-create"
        );
        assert_eq!(s.stats.merges, 4);
    }

    /// The `POSITION_TS` gate: forwards moves, backwards does not.
    #[test]
    fn a_stale_position_event_is_ignored() {
        let mut s = ObjectStream::new();
        let mut arrived = Vec::new();
        s.apply(
            &ev(
                Opcode::ITEM_CREATE_OBJECT,
                create_blob(7, 0, 0x0200_0001, Some(0x00A9_B401)),
            ),
            LocalTime(0.0),
            &mut arrived,
        );

        let at = |ts: u16, x: f32| {
            let m = MovementPositionEvent {
                id: ObjectId(7),
                position: dereth_protocol::movement::PositionPack {
                    origin: dereth_protocol::types::Origin {
                        objcell_id: 0x00A9_B401,
                        origin: dereth_protocol::types::Vec3 { x, y: 0.0, z: 0.0 },
                    },
                    position_timestamp: ts,
                    ..dereth_protocol::movement::PositionPack::default()
                },
            };
            ev(
                MovementPositionEvent::OPCODE,
                write_body(&m).expect("encode"),
            )
        };

        s.apply(&at(5, 10.0), LocalTime(0.0), &mut arrived);
        assert_eq!(s.stats.position_updates, 1);
        assert!(
            (s.presence(ObjectId(7))
                .unwrap()
                .position
                .unwrap()
                .frame
                .origin
                .x
                - 10.0)
                .abs()
                < 1e-6
        );

        s.apply(&at(3, 99.0), LocalTime(0.0), &mut arrived);
        assert_eq!(s.stats.stale_positions, 1, "an older stamp is dropped");
        assert!(
            (s.presence(ObjectId(7))
                .unwrap()
                .position
                .unwrap()
                .frame
                .origin
                .x
                - 10.0)
                .abs()
                < 1e-6
        );
    }

    /// An update for an unknown object asks for it every twenty seconds until created or dropped.
    #[test]
    fn an_update_for_an_unknown_object_asks_for_it_every_twenty_seconds_until_created_or_dropped() {
        use dereth_client_net::client_session::testing::MockTransport;
        use dereth_client_net::client_session::Session;
        use dereth_primitives::NetQueue;
        use dereth_protocol::movement::MovementPositionEvent;

        fn asks(s: &Session<MockTransport>) -> Vec<u32> {
            s.transport
                .sent_on(NetQueue::Control)
                .iter()
                .filter(|b| b.get(..4) == Some([0xEA, 0xF6, 0x00, 0x00].as_slice()))
                .map(|b| u32::from_le_bytes(b[4..8].try_into().expect("an id after the opcode")))
                .collect()
        }
        fn update(s: &mut Session<MockTransport>, id: u32, t: f64) {
            let blob = dereth_protocol::write_blob(&MovementPositionEvent {
                id: ObjectId(id),
                ..MovementPositionEvent::default()
            })
            .expect("encode");
            s.transport.deliver_blob(NetQueue::WorldObjects, &blob);
            s.tick(LocalTime(t));
            s.drain_events().count();
        }
        let mut s = Session::new(MockTransport::new());
        let mut objects = ObjectStream::new();
        let (unknown, known, created) = (0x8000_0301_u32, 0x8000_0302_u32, 0x8000_0303_u32);
        s.object_arrived(ObjectId(known), 0);

        // A pass every 0.5 s for 100 s; an update for each id every 9 s up to 54 s (`created`
        // only up to 18 s, when it is created).
        let mut sent = Vec::new();
        for step in 0..=200_u32 {
            let t = f64::from(step) * 0.5;
            if step % 18 == 0 && t <= 60.0 {
                update(&mut s, unknown, t);
                update(&mut s, known, t);
                if t <= 18.0 {
                    update(&mut s, created, t);
                }
            }
            if step == 36 {
                s.object_arrived(ObjectId(created), 0);
            }
            objects.use_time(ServerTime(t), Some(&mut s));
            let now = asks(&s);
            for id in &now[sent.len()..] {
                assert_eq!(*id, unknown, "asked about 0x{id:08X} at {t} s");
            }
            if now.len() > sent.len() {
                sent.push(t);
            }
            assert_eq!(now.len(), sent.len(), "one ask per pass at most, at {t} s");
        }
        // First park at 0 s, and the age test is strict: asks on the first passes past 20, 40.5
        // and 61 s. The last update, at 54 s, keeps the placeholder until 79 s, so none at 81.5 s.
        assert_eq!(
            sent,
            vec![20.5, 41.0, 61.5],
            "asks at retail's cadence, and none after the hold expires"
        );
        assert_eq!(objects.stats.objdesc_asks, 3);
        assert_eq!(objects.stats.objdesc_asks_undeliverable, 0);
    }
}
