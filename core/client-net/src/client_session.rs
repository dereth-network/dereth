//! The client session: the client-side state above the transport.
//!
//! It is `client_session` rather than `session` because [`dereth_transport::session`] is already
//! the role-neutral transport's per-connection receive and retransmit state.
//!
//! This module owns the connection state machine, the four queue dispatchers, the ordering machinery
//! above the blob, the per-property sequence gate and the outbound game-action counter. It does
//! **not** own what a decoded message means: the weenie/physics/cell triple, qualities, inventory
//! trees and chat state belong to `dereth-client-model`, and this module hands them decoded bytes through
//! `SessionEvent`.
//!
//! # The isolation rule
//!
//! This module names nothing else in `dereth-client-net`: not the socket, not [`crate::net::Net`], not the
//! wire types. Everything reaches the network through [`dereth_primitives::Transport`], and every test
//! runs against `testing::MockTransport`. That is not a temporary scaffold — it is how the
//! session's tests are defined, and it stays in the tree. It is checked by the `cpu` test
//! `client_session::isolation`, which reads these sources.
//!
//! # The four things this module exists to get right
//!
//! * **The queue map is a contract.** Queues 4, 5 and 8 go to the login server, everything else to
//!   the world server; against ACE both ids are the same, so a hard-coded recipient passes every
//!   local test and breaks on a split deployment.
//! * **Do not merge the queues.** The ordering rules, the crucial-events gate and the object-blocked
//!   replay all differ per queue.
//! * **`0x0013` is what makes the client "in world"**, not the transport's connection and not
//!   `0xF7DF`.
//! * **The game-action counter is global and must roll back**, or the server drops everything after
//!   the first failed send.

pub mod dispatch;
pub mod flow;
pub mod ordering;
pub mod outbound;
pub mod phases;
pub mod position;
/// Recording values and parsers supplied with bytes by their host.
pub mod recording;
pub mod stamper;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

use dereth_primitives::{
    IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId, Transport,
};
use dereth_protocol::admin::DddInterrogationResponse;
use dereth_protocol::login::{
    CharGenVerificationResponse, LoginAccountBanned, LoginAccountBooted, LoginCharacterSet,
    LoginExecuteLogOffRequest, LoginPlayerDescription, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::{read_body, Message, MessageError, Opcode};

pub use dispatch::database::{DddEvent, DddState};
pub use dispatch::ui::UiOrdering;
pub use dispatch::world_objects::InstanceTable;
pub use flow::{DisconnectReason, Flow, FlowAction, SessionState, LOGON_TIMEOUT_SECONDS};
pub use ordering::{ParkedBlobs, StampResult, StampWindow};
pub use outbound::{ActionCounter, OutboundBlob};
pub use position::{
    ContactPlane, PlayerMotion, PositionReporter, PositionReporterStats, MIN_JUMP_EXTENT,
    TIME_BETWEEN_POSITION_EVENTS,
};
pub use stamper::PropertySequenceGate;

/// `CG_VERIFICATION_RESPONSE_OK`, the `case 1:` of the char-gen verification response's
/// switch.
///
/// The literal is pinned here because a test that reads a constant through the same symbol it
/// writes through cannot detect a wrong constant. The full enum, taken
/// from the same switch, is `UNDEF = 0, OK = 1, PENDING = 2, NAME_IN_USE = 3, NAME_BANNED = 4,
/// CORRUPT = 5, DATABASE_DOWN = 6, ADMIN_PRIVILEGE_DENIED = 7`; ACE's
/// `CharacterGenerationVerificationResponse` agrees value for value.
pub const CG_VERIFICATION_RESPONSE_OK: u32 = 1;

/// Why a blob was not acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReason {
    /// Queue 4's consumer looks at nothing but `0xF7DE`.
    LoginQueueIgnoresEverythingButTurbineChat,
    /// The queue's switch has no arm for this opcode.
    NoHandler,
    /// The blob is shorter than 4 bytes, or a leading field would not fit.
    ShortBuffer,
    /// The body did not decode. The client's unpacking would leave the object half-populated; this
    /// crate stops and names the field instead.
    Malformed(MessageError),
    /// `SequenceGate` returned 3: the stamp is at or below the highest already delivered.
    StaleOrderedStamp,
    /// A second `0xF746 Login_CreatePlayer`, which the smart-box handler ignores.
    PlayerAlreadyCreated,
}

/// Everything the session decoded this tick, in arrival order.
///
/// The game layer consumes these; the session never reaches into the game state. Messages the session
/// itself acts on get a named variant; everything else is handed over with its ordering already
/// resolved, so the consumer never has to think about `OrderedEventHeader` again.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// `0xF658` — the character-select list.
    CharacterSet(Box<LoginCharacterSet>),
    /// `0xF7E1`.
    WorldInfo {
        connections: i32,
        max_connections: i32,
        name: String,
    },
    /// `0xF659`. The English text comes from string table `0x10000002`, not from this message.
    CharacterError(u32),
    /// `0xF65A`: the character screen's message, as its box shows it.
    CharacterScreenMessage(String),
    /// `0xF7DC`. An absent reason means the client substitutes
    /// `" for Code of Conduct Violations"`.
    AccountBooted(Option<String>),
    /// `0xF7C1`. `expiry <= 0` means permanent.
    AccountBanned { expiry: i32, reason: String },
    /// `0xF651`.
    SubscriptionExpiresIn { minutes: u32 },
    /// `0xF643`.
    CharGenResponse(Box<CharGenVerificationResponse>),
    /// `0xF655` received: the slot was deleted.
    CharacterDeleted,
    /// `0xF653` received.
    LoggedOff,
    /// `0xF7DF`.
    EnterWorldReady,
    /// `0x0013` — the message that makes the client "in world".
    PlayerDescription(Box<LoginPlayerDescription>),
    /// `0xF746` — the player's own object id, which arrives before its `0xF745`.
    PlayerCreated(ObjectId),
    /// `0xF7DE`, opaque because the login queue forwards its body without decoding it.
    TurbineChat(Vec<u8>),
    /// A DDD exchange step.
    Ddd(DddEvent),
    /// A UI-queue message with its ordering resolved: the blob **without** any `OrderedEventHeader`, so it
    /// begins with its own type dword.
    UiEvent { opcode: Opcode, blob: Vec<u8> },
    /// A WorldObjects message that passed the instance gate.
    WorldObject { opcode: Opcode, body: Vec<u8> },
    /// Reset the smart box with argument 1 during character log-on **phase 2**, immediately before
    /// sending enter-world.
    ///
    /// The world teardown, which in retail is keyed on *entering* the world and not on logging
    /// out. Its consumers are the object model (which walks the player too) and the renderer
    /// (which releases the landscape and flushes the cells). It arrives before
    /// the new session's `0xF746`, so a consumer needs no object-lifetime guard.
    WorldReset,
    /// The state machine moved.
    StateChanged(SessionState),
    /// A blob that was not acted on, and why.
    Dropped {
        queue: NetQueue,
        opcode: Opcode,
        reason: DropReason,
    },
}

impl SessionEvent {
    /// Borrow a UI message's opcode and body, excluding its leading type dword.
    /// A short UI message has an empty body; other event kinds have no UI body.
    #[must_use]
    pub fn ui_body(&self) -> Option<(Opcode, &[u8])> {
        match self {
            Self::UiEvent { opcode, blob } => Some((*opcode, blob.get(4..).unwrap_or_default())),
            _ => None,
        }
    }
}

/// The client-side session.
#[derive(Debug)]
pub struct Session<T: Transport> {
    /// Public so a replay harness can inspect what was sent. The client's UI protocol is likewise a
    /// singleton anything can reach.
    pub transport: T,
    flow: Flow,
    counter: ActionCounter,
    ui: UiOrdering,
    /// The physics object's parked list, drained by the smart box's object-blob replay.
    ///
    /// It is deliberately separate from the per-object windows in `UiOrdering`, which are the
    /// per-weenie blob queue's *ordered* list: the client keeps two lists on two different objects and replays
    /// them through two different dispatchers. Sharing one list would send replayed WorldObjects
    /// blobs into the UI-queue switch, where they have no arm and are dropped.
    parked_world_objects: ParkedBlobs,
    instances: InstanceTable,
    /// UI ordering belongs to a live Weenie even when the physics placeholder failed to initialise.
    weenies: std::collections::HashSet<ObjectId>,
    ddd: DddState,
    stampers: std::collections::HashMap<ObjectId, PropertySequenceGate>,
    account: String,
    world_name: Option<String>,
    characters: LoginCharacterSet,
    player_id: Option<ObjectId>,
    /// Whether character generation is awaiting a verification response; this is the
    /// branch selector in the char-gen verification response.
    ///
    /// The char-gen wizard's finish sets `PENDING` on the line **before** sending its result, so the
    /// flag and the `0xF656` are raised in the same
    /// instant; [`Session::create_character`] is that instant here. It matters because the same
    /// opcode carries ACE's `GameMessageCharacterRestore`, whose reply must **overwrite** an
    /// existing slot rather than append a new identity — see the arm's comment.
    chargen_pending: bool,
    /// The character-generation slot, as far as the restore arm of the verification response needs
    /// it.
    ///
    /// Retail sets the slot on *selection*: the character screen looks up the selected character's
    /// slot, records it for the response, and later puts it back to `-1`. This crate has no
    /// selection edge, so it is recorded by
    /// [`Session::restore_character`] instead, which is the only reachable use of the value and is
    /// strictly narrower: *Restore* is only offered for the selected character, so the slot the
    /// reply lands in is the same one either way.
    ///
    /// It is taken, not read, when the `0xF643` arrives: retail resetting the verification
    /// state and clearing the slot to -1 on every failing arm both mean one reply consumes one request.
    restore_slot: Option<i32>,
    events: Vec<SessionEvent>,
    /// The clock the last [`Session::tick`] ran at. The client reads a global clock; a
    /// caller-supplied clock is the testable equivalent, and the handlers that stamp a
    /// deadline need the same value the tick was given.
    now: LocalTime,
    /// Raw network cadence is independent of the prior/current simulated UI clock.
    network_now: LocalTime,
    queues: phases::IncomingQueues,
    /// The exit-world disconnects owed to the transport, and the enter-world calls
    /// likewise.
    ///
    /// The client reaches its transport through a global and calls both
    /// outright; this crate reaches it through `dereth_primitives::Transport`, which is
    /// deliberately two methods wide -- "the whole of what `dereth-protocol` and the client session are
    /// allowed to know about" the transport. So the *session* half of each call is performed here and
    /// the *transport* half is counted and handed to the host, which owns both objects. They are
    /// counters rather than `SessionEvent`s so the host can apply them **before** the next
    /// `Net::tick` turns a queued blob into a datagram, which is the ordering the client has; an
    /// event would arrive a drain later.
    exit_world_disconnects: u32,
    enter_worlds: u32,
}

impl<T: Transport> Session<T> {
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            flow: Flow::new(),
            counter: ActionCounter::new(),
            ui: UiOrdering::new(),
            parked_world_objects: ParkedBlobs::new(),
            instances: InstanceTable::new(),
            weenies: std::collections::HashSet::new(),
            ddd: DddState::Idle,
            stampers: std::collections::HashMap::new(),
            account: String::new(),
            world_name: None,
            characters: LoginCharacterSet::default(),
            player_id: None,
            chargen_pending: false,
            restore_slot: None,
            events: Vec::new(),
            now: LocalTime(0.0),
            network_now: LocalTime(0.0),
            queues: phases::IncomingQueues::default(),
            exit_world_disconnects: 0,
            enter_worlds: 0,
        }
    }

    /// How many exit-world disconnect and enter-world calls the transport is owed; both counters
    /// are zeroed by the read.
    ///
    /// The host must apply these to `dereth_client_net::Net` before it next asks the transport to build
    /// datagrams. See the field comment for why this is not a `SessionEvent`.
    pub fn take_transport_calls(&mut self) -> (u32, u32) {
        (
            std::mem::take(&mut self.exit_world_disconnects),
            std::mem::take(&mut self.enter_worlds),
        )
    }

    #[must_use]
    pub fn state(&self) -> SessionState {
        self.flow.state()
    }

    #[must_use]
    pub fn characters(&self) -> &LoginCharacterSet {
        &self.characters
    }

    #[must_use]
    pub fn world_name(&self) -> Option<&str> {
        self.world_name.as_deref()
    }

    #[must_use]
    pub fn player_id(&self) -> Option<ObjectId> {
        self.player_id
    }

    #[must_use]
    pub fn account(&self) -> &str {
        &self.account
    }

    // ---------------------------------------------------------------------------------------
    // **Instrumentation.** The four per-object containers a create fills and
    // [`Self::object_deleted`] / [`Self::physics_object_deleted`] are supposed to empty, plus
    // the undrained-event queue. Gauges, read by the long-session growth station only.
    //
    // They matter because this crate's tables are keyed by object id and an id is never reused:
    // a session that walks past ten thousand creatures and loses one row per cull grows for as
    // long as it runs, with nothing visible on screen to say so.
    // ---------------------------------------------------------------------------------------

    /// Rows of `InstanceTable` — objects whose instance sequence the gate can answer for.
    /// Instrumentation.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Objects the session believes have a live Weenie, i.e. the UI-ordering owners.
    /// Instrumentation.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn weenie_count(&self) -> usize {
        self.weenies.len()
    }

    /// `PropertySequenceGate`s, one per object [`Self::stamper`] has ever been asked for.
    /// Instrumentation.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn stamper_count(&self) -> usize {
        self.stampers.len()
    }

    /// Blobs parked on objects by the physics layer and not yet replayed.
    /// A blob parked on an id that never arrives is held for the rest of the session.
    /// Instrumentation.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn parked_blob_count(&self) -> usize {
        self.parked_world_objects.total()
    }

    /// Events raised and not yet taken by [`Self::drain_events`]. Instrumentation.
    #[must_use]
    #[cfg(any(test, feature = "test-support"))]
    pub fn pending_event_count(&self) -> usize {
        self.events.len()
    }

    /// The per-object `PropertySequenceGate`s, keyed by object. Object setup creates one lazily; so does
    /// this.
    #[cfg(any(test, feature = "test-support"))]
    pub fn stamper(&mut self, id: ObjectId) -> &mut PropertySequenceGate {
        self.stampers.entry(id).or_default()
    }

    /// The instance-sequence table the WorldObjects gate reads. The game layer updates it when it
    /// creates or destroys an object.
    #[cfg(any(test, feature = "test-support"))]
    pub fn instances_mut(&mut self) -> &mut InstanceTable {
        &mut self.instances
    }

    /// Retire an old object instance before a later arrival with the same id.
    /// The instance timestamp lives on the physics object. Destruction releases its parked blobs,
    /// removes its stamper and destroys its timestamp-ordered receive queue.
    /// The global UI stream/crucial gate and other objects do not belong to this instance.
    pub fn object_deleted(&mut self, id: ObjectId) {
        self.physics_object_deleted(id);
        self.weenies.remove(&id);
        self.stampers.remove(&id);
        self.ui.forget_object(id);
        self.queues.forget_object(id);
    }

    /// The object maintainer's pass over the **null placeholders** that hold parked
    /// WorldObjects blobs: destroy every one whose deadline has
    /// passed, and answer how many.
    ///
    /// `now` is the same clock used to stamp the deadline. The caller is
    /// `dereth_client_runtime::objects::ObjectStream::use_time`, this build's maintenance sweep; see
    /// [`crate::client_session::ordering::ParkedBlobs`] for
    /// what went unbounded without it.
    pub fn destroy_expired_parked_blobs(&mut self, now: f64) -> usize {
        self.parked_world_objects.destroy_expired(now)
    }

    /// The ask half of the same pass: the ids of the placeholders holding parked WorldObjects
    /// blobs whose ask stamp is more than 20 s old, restamped to `now`. The caller sends one
    /// `0xF6EA` for each. Call it after [`Self::destroy_expired_parked_blobs`] with the same
    /// clock. See [`crate::client_session::ordering::ParkedBlobs::take_force_objdesc_asks`].
    pub fn take_force_objdesc_asks(&mut self, now: f64) -> Vec<ObjectId> {
        self.parked_world_objects.take_force_objdesc_asks(now)
    }

    /// Physical object deletion/destruction only. A failed placeholder init can delete
    /// the physical parked owner while an independent Weenie/UI ordering owner survives.
    /// Raw owning-queue entries have not been parked on that object and are untouched.
    pub fn physics_object_deleted(&mut self, id: ObjectId) {
        self.instances.remove(id);
        drop(self.parked_world_objects.release(id));
    }

    /// Recheck an already emitted WorldObjects event against the instance that exists at actual
    /// object delivery. The coarse tick adapter may have decoded an entire batch before a delete
    /// or replacement was applied. Accepted WorldObjects dispatch is read-only; only its queued
    /// branch parks bytes. This never replays UI ordering/property stamps or F746 PlayerCreated.
    /// It does not repair the separate UI-at-frame-entry versus WorldObjects frame-phase schedule.
    pub fn recheck_world_object_delivery(
        &mut self,
        opcode: Opcode,
        body: &[u8],
    ) -> Option<SessionEvent> {
        if opcode == Opcode::LOGIN_CREATE_PLAYER {
            // F746's authoritative producer already emitted PlayerCreated, not WorldObjects.
            return None;
        }
        let message = IncomingMessage {
            opcode: opcode.0,
            queue: NetQueue::WorldObjects,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body: body.to_vec(),
        };
        self.parked_world_objects.set_time(self.now.0);
        dispatch::world_objects::dispatch(
            &mut self.instances,
            &mut self.parked_world_objects,
            self.player_id,
            &message,
        )
        .event
    }

    /// Tell the session an object now exists, so anything parked on it can be replayed.
    ///
    /// This is the layer boundary: the session owns the *ordering*, while the game owns the objects.
    pub fn object_arrived(&mut self, id: ObjectId, instance_sequence: u16) {
        self.weenies.insert(id);
        self.instances.set(id, instance_sequence);
        // The smart box's object-blob replay — the physics list, back through the
        // WorldObjects dispatcher it was parked by. A replayed create can itself unpark more, so the
        // caller's own loop (drain, apply, `object_arrived`, drain again) is what terminates this.
        for blob in self.parked_world_objects.release(id) {
            self.deliver_world_object_blob(&blob);
        }
        // The weenie's blob replay — the ordered UI list, replayed **through
        // the object's own `StampWindow`**, which is where `UiOrdering::route` parked it.
        let now = self.now;
        for blob in self.ui.object_arrived(id, now) {
            self.deliver_ui_blob(&strip_order_header(&blob));
        }
    }

    /// Re-enter [`dispatch::world_objects::dispatch`] with a blob that was parked on an object.
    ///
    /// The parked copy is `[opcode][body]`, which is the blob as the physics object stored it; the
    /// queue is queue 10 by construction, because nothing else parks there.
    fn deliver_world_object_blob(&mut self, blob: &[u8]) {
        let Some(op) = leading_opcode(blob) else {
            self.events.push(SessionEvent::Dropped {
                queue: NetQueue::WorldObjects,
                opcode: Opcode(0),
                reason: DropReason::ShortBuffer,
            });
            return;
        };
        let m = IncomingMessage {
            opcode: op.0,
            queue: NetQueue::WorldObjects,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body: blob[4..].to_vec(),
        };
        self.parked_world_objects.set_time(self.now.0);
        let d = dispatch::world_objects::dispatch(
            &mut self.instances,
            &mut self.parked_world_objects,
            self.player_id,
            &m,
        );
        if let Some(e) = d.event {
            if let SessionEvent::PlayerCreated(id) = e {
                self.player_id = Some(id);
            }
            self.events.push(e);
        }
    }

    /// Coarse compatibility adapter for existing standalone replay callers. This predecodes
    /// queues before the host sees object lifetime changes; it is NOT the retail App frame.
    /// Production uses the explicit phase methods in `phases` and admits once at delivery.
    pub fn tick(&mut self, now: LocalTime) {
        self.now = now;
        self.network_now = now;
        // Receive phase: fill the per-queue buckets, exactly as the client's receive-queue insert does.
        let mut logon = Vec::new();
        let mut database = Vec::new();
        let mut ui = Vec::new();
        let mut world_objects = Vec::new();
        while let Some(m) = self.transport.poll() {
            match m.queue {
                NetQueue::Logon => logon.push(m),
                NetQueue::ClientCache => database.push(m),
                NetQueue::UiQueue => ui.push(m),
                NetQueue::WorldObjects => world_objects.push(m),
                // Queues 1, 2, 3, 6, 7, 8 and 11 are unregistered or outbound-only, and
                // the client's receive-queue insert silently discards anything that arrives on them.
                other => self.events.push(SessionEvent::Dropped {
                    queue: other,
                    opcode: Opcode(m.opcode),
                    reason: DropReason::NoHandler,
                }),
            }
        }

        for m in &logon {
            let e = dispatch::logon::dispatch(m);
            self.events.push(e);
        }
        for m in &database {
            let d = dispatch::database::dispatch(&mut self.ddd, m);
            if d.send_end {
                self.send_bare(&dereth_protocol::admin::DddEndDdd);
            }
            self.on_ddd(&d.event);
            self.events.push(d.event);
        }
        for m in &ui {
            // "Does the client know this object?" is the object model's question; the session
            // answers it
            // from the instance table, which `Session::object_arrived` keeps.
            let d = {
                let instances = &self.instances;
                self.ui.route(m, now, &|id| instances.knows(id))
            };
            self.events.extend(d.events);
            for blob in d.ready {
                self.deliver_ui_blob(&blob);
            }
        }
        self.parked_world_objects.set_time(now.0);
        for m in &world_objects {
            let d = dispatch::world_objects::dispatch(
                &mut self.instances,
                &mut self.parked_world_objects,
                self.player_id,
                m,
            );
            if let Some(e) = d.event {
                if let SessionEvent::PlayerCreated(id) = e {
                    self.player_id = Some(id);
                }
                self.events.push(e);
            }
        }

        // The flow machine's own tick: the two-phase login and the 110-second timeout.
        let mut actions = Vec::new();
        self.flow.tick(now, &mut actions);
        self.run_actions(actions);
    }

    /// Start the two-step enter-world exchange.
    pub fn enter_world(&mut self, character: ObjectId, account: &str) {
        self.account = account.to_string();
        let mut actions = Vec::new();
        self.flow.enter_world(character, &mut actions);
        self.run_actions(actions);
    }

    /// Report server death from transport connection removal.
    ///
    /// The flow already owns the same state edge for its 110-second world-entry timeout; the
    /// transport calls this adapter rather than constructing a `SessionEvent` alongside it, so
    /// [`Session::state`] and the event stream cannot disagree.
    pub fn server_died(&mut self) {
        let mut actions = Vec::new();
        self.flow
            .disconnect(DisconnectReason::ServerDied, &mut actions);
        self.run_actions(actions);
    }

    /// Deleting a character — identified by **slot**, not by id.
    pub fn delete_character(&mut self, character: ObjectId) {
        let Some(slot) = self.slot_of(character) else {
            return;
        };
        self.send_bare(&dereth_protocol::login::CharacterDeleteRequest {
            account: self.account.clone(),
            slot_index: slot,
        });
    }

    /// `0xF656`, on the Logon queue.
    ///
    /// The account name is the one `0xF658` delivered, passed through unchanged in the character
    /// creation request.
    pub fn create_character(&mut self, result: dereth_protocol::login::CharGenResult) {
        // Character generation's finish step marks verification pending just before the send.
        // This is what tells the `0xF643` arm that the reply is a *creation* and its
        // identity must be appended, rather than a restore whose identity replaces a slot.
        self.chargen_pending = true;
        self.send_bare(&dereth_protocol::login::CharacterSendCharGenResult {
            account: self.account.clone(),
            result,
        });
    }

    /// Restoring a character — `0xF7D9
    /// Admin_SendAdminRestoreCharacter`, on the **Control** queue.
    ///
    /// The character screen's *Restore* button (`CharacterAction::Restore`) sends through this.
    /// The client passes the character's `ObjectID` and **two empty strings** — its restore
    /// command has no other caller and never fills either,
    /// which is why the message's two narrow-string fields exist and are always empty on the
    /// wire.
    ///
    /// It is a *bare* send and not a game action: `0xF7D9` has no `OrderedActionHeader`
    /// (`dereth_protocol::opcodes`' table gives it `send_queue: Control` and no stamp).
    pub fn restore_character(&mut self, character: ObjectId) {
        // The character screen's select is where
        // retail stamps the character-generation slot, and the `0xF643` that answers this send reads it to
        // decide *which row to overwrite*. There is no selection edge in this crate, so it is
        // stamped here: `Restore` is only offered for the selected character, so the slot is the
        // same one either way. Without it the reply has nowhere to land and the restore does not
        // finish -- see the `0xF643` arm.
        self.restore_slot = self.slot_of(character);
        self.send_bare(&dereth_protocol::admin::AdminSendAdminRestoreCharacter {
            iid: character,
            restored_char_name: String::new(),
            account_to_restore_to: String::new(),
        });
    }

    /// `0xF6EA`, eight bytes, on the **Control** queue.
    ///
    /// The opcode as a `u32`, then the object id as a `u32`.
    ///
    /// Ask the server to re-send an object's appearance.
    ///
    /// **There are three callers, not one**: the object maintainer's weenie-desc merge path
    /// desync check, and **twice** in the object-maintenance time step, which sweeps
    /// `null_object_table` and `null_weenie_object_table` and re-asks for every entry older than
    /// **20.0 s**.
    ///
    /// **All 35 recorded asks are the sweep**, measured rather than assumed: four object ids, 31 of
    /// 31 consecutive gaps at 20.00 s to within a millisecond, no `0xF745` for that id in the 10 s
    /// before any of them, 32 of 35 answered by a `0xF625` within 21 ms. So the corpus contains no
    /// example of the merge path at all.
    ///
    /// It is not a game action — `outbound_kind` puts `0xF6EA` on the Control queue with no
    /// `OrderedActionHeader` — so [`Session::send_action`] refuses it by design; this is the bare
    /// sender. The corpus replay exercises it: all 35 recorded blobs in `long-solo-play` come back
    /// byte for byte.
    ///
    /// The game world's time step runs `sweep_null_tables` (the time-step caller above) and
    /// `set_weenie_desc` step 5 is the merge path; both raise a `Request::ForceObjdesc`, which the
    /// host routes here.
    ///
    /// **Known gap, upstream of this seam:** `create_or_merge` passes `is_update = false` on the
    /// merge path, so `set_weenie_desc`'s step 5 is unreachable, and
    /// `queue_blob_for_weenie_object` declines to make a placeholder for an object that already
    /// exists where the client makes one unconditionally — which is why replaying `long-solo-play`
    /// through this build asks 0 times where retail asked 35.
    pub fn send_force_objdesc(&mut self, id: ObjectId) {
        self.send_bare(&dereth_protocol::objects::ObjectSendForceObjdesc { id });
    }

    /// The server-version request — the four literal bytes
    /// `[0xCC, 0xF7, 0, 0]` on the Control queue, without an `OrderedActionHeader` or action stamp.
    pub fn send_admin_get_server_version(
        &mut self,
        m: &dereth_protocol::admin::AdminSendAdminGetServerVersion,
    ) {
        self.send_bare(m);
    }

    /// Send friends command `cmd` for `player` as `0xF7CD`, and **not** as a game action.
    ///
    /// The same shape as `send_force_objdesc` above, for the same reason:
    /// `dereth_protocol::opcodes` puts this one on `NetQueue::Control`, so `outbound::build` frames it
    /// as a bare `[opcode][body]` with no `OrderedActionHeader` and no counter allocation, and
    /// `send_action` would refuse it outright. `@friends old` is its only caller in retail
    /// (the communication system's friends command), and it always passes `cmd = 0`.
    pub fn send_friends_command(&mut self, m: &dereth_protocol::social::SocialSendFriendsCommand) {
        self.send_bare(m);
    }

    /// Logging the character off.
    pub fn log_off(&mut self) {
        let mut actions = Vec::new();
        self.flow.log_off(&mut actions);
        self.run_actions(actions);
    }

    /// The single outbound game-action path.
    ///
    /// Allocates the next counter value, writes `[0xF7B1][stamp][sub-type][payload]`, sends it on
    /// the Weenie queue, and **rolls the counter back** if the transport reports failure.
    ///
    /// The `Transport` trait's `send` has no return value — the transport's flow queue decides
    /// whether a blob went out, and it decides after the call — so a failure is reported back
    /// through [`Session::report_send_failed`], which is what performs the rollback. There is no
    /// path by which `send_action` can learn of the failure itself, and pretending otherwise would
    /// hide the fact that the caller must wire it up.
    pub fn send_action<M: Message>(&mut self, m: &M) -> Result<u32, MessageError> {
        let blob = outbound::build(&mut self.counter, m)?;
        let stamp = blob.stamp.ok_or(MessageError::Unencodable {
            field: "send_action",
            reason: "this opcode is not a game action; use the bare senders",
        })?;
        self.transport.send(blob.queue, blob.ordered, &blob.payload);
        Ok(stamp)
    }

    /// Tell the session the last send did not go out.
    ///
    /// Call this from the transport's failure path. The counter is rolled back so the client's
    /// action sequence has no holes; the server drops everything after a hole.
    pub fn report_send_failed(&mut self) {
        self.counter.rollback();
    }

    /// The value the next game action will carry.
    #[must_use]
    pub fn next_action_stamp(&self) -> u32 {
        self.counter.peek()
    }

    /// Everything decoded since the last drain, in arrival order.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, SessionEvent> {
        self.events.drain(..)
    }

    /// Send a message that is not a game action: no `OrderedActionHeader`, on its own queue.
    fn send_bare<M: Message>(&mut self, m: &M) {
        if let Ok(blob) = outbound::build(&mut self.counter, m) {
            self.transport.send(blob.queue, blob.ordered, &blob.payload);
        }
    }

    /// The retail chat event's send path.
    /// Queue4, opcode+length+DLL bytes, no OrderedActionHeader and no game-action counter allocation.
    pub fn send_turbine_chat(
        &mut self,
        message: &dereth_protocol::turbine::SendToRoomById,
    ) -> Result<(), MessageError> {
        let blob = outbound::build(&mut self.counter, message)?;
        self.transport.send(blob.queue, blob.ordered, &blob.payload);
        Ok(())
    }

    fn slot_of(&self, character: ObjectId) -> Option<i32> {
        self.characters
            .characters
            .iter()
            .position(|c| c.gid == character)
            .and_then(|i| i32::try_from(i).ok())
    }

    /// Reset the smart box with argument 1 — the session's half.
    ///
    /// Everything here is owned by `WorldObjects` or object maintenance, and retail clears all of it on
    /// the **enter-world** edge rather than on any ending:
    ///
    /// * `self.player_id` — zeroed by the reset. This is the field the create-player handler tests
    ///   before accepting a `0xF746`
    ///   (a load, a test and a `return 3`), so
    ///   zeroing it here is what makes the next session's player creatable **without weakening the
    ///   duplicate check**. The check stays exactly as it is.
    /// * `self.parked_world_objects` — the queued-blob destruction.
    /// * `self.instances`, `self.weenies`, `self.stampers` and the object-resume continuation —
    ///   the object maintainer's destroy pass, which walks the whole object hash and destroys
    ///   every entry.
    /// * `self.ui` — the per-object ordered lists those destroyed weenies owned
    ///   (the per-weenie blob queues).
    ///
    /// **Why it is on the enter-world edge.** Cleared only in the `0xF653` arm, a session that
    /// ended any other way — a transport drop, a `0xF659`, `0xF7DC`, the 110 s `ServerDied` — would
    /// leave `player_id` set. The next entry's `0xF746` would then be refused as a duplicate, no
    /// `SessionEvent::PlayerCreated` raised, no player body would exist, and nothing that keys off
    /// the player would run: the login tunnel, and then the *departure* sequence when that session
    /// in turn logged out, so the departure animation would not play on the second logout.
    fn reset_world_view(&mut self) {
        self.ui.reset();
        self.parked_world_objects = ParkedBlobs::new();
        self.instances = InstanceTable::new();
        self.weenies.clear();
        self.player_id = None;
        self.stampers.clear();
        self.queues.forget_all_object_resumes();
    }

    fn run_actions(&mut self, actions: Vec<FlowAction>) {
        for a in actions {
            match a {
                FlowAction::SendEnterWorldRequest => self.send_bare(&LoginSendEnterWorldRequest),
                FlowAction::SendEnterWorld(gid) => {
                    let account = self.account.clone();
                    self.send_bare(&LoginSendEnterWorld {
                        character: gid,
                        account,
                    });
                    self.flow.enter_world_sent(self.network_now);
                }
                FlowAction::SendLogOff(gid) => {
                    self.send_bare(&LoginExecuteLogOffRequest { character: gid });
                }
                // The exit-world disconnect's last act resets the event counter to 0 — the session's
                // half of the teardown. `early-inventory-and-casting` of the corpus is where it
                // shows: the stamp climbs to 289, the player logs off, and the next action carries
                // **1**.
                //
                // It is here and not in the `0xF653` arm because `0xF653` is only *one* of the
                // three edges player state runs the teardown on: the character log-on's phase 1
                // and the server-ready handler are the other two, and they
                // are what make a second world entry start from the same counter as the first
                // even when no clean log-off preceded it. The reset must not fire twice; this is
                // the one site that performs it.
                FlowAction::ExitWorldDisconnect => {
                    self.counter.set(0);
                    self.exit_world_disconnects += 1;
                }
                // Entering the world.
                FlowAction::EnterWorld => self.enter_worlds += 1,
                // An event rather than a counter, unlike the two above: those are
                // calls the *host* owes the transport before the next datagram is built, and they
                // must not wait for a drain. This one is consumed by the object model and the
                // renderer, which read the event stream, and its position **in** that stream is
                // the whole point — everything after it belongs to the new session and everything
                // before it to the old.
                FlowAction::WorldReset => {
                    self.reset_world_view();
                    self.events.push(SessionEvent::WorldReset);
                }
                FlowAction::StateChanged(s) => self.events.push(SessionEvent::StateChanged(s)),
            }
        }
    }

    fn on_ddd(&mut self, e: &SessionEvent) {
        let mut actions = Vec::new();
        match e {
            SessionEvent::Ddd(DddEvent::Begin(_)) => self.flow.patching_started(&mut actions),
            SessionEvent::Ddd(DddEvent::End) => self.flow.patching_finished(&mut actions),
            _ => {}
        }
        self.run_actions(actions);
    }

    /// The UI queue's opcode switch.
    ///
    /// The blob begins with its own type dword; the `OrderedEventHeader` has already been stripped by the
    /// router. Every arm goes through [`dereth_protocol::read_checked`], which is the **second** half of
    /// the double opcode check: a message whose body disagrees with the arm it was routed to is
    /// dropped silently, not panicked on. The check is not redundant with the switch, because
    /// replayed blobs re-enter here from the parked list and from the crucial-events replay.
    #[allow(clippy::too_many_lines)] // one arm per message; splitting it would hide the switch
    fn deliver_ui_blob(&mut self, blob: &[u8]) {
        let Some(op) = leading_opcode(blob) else {
            self.events.push(SessionEvent::Dropped {
                queue: NetQueue::UiQueue,
                opcode: Opcode(0),
                reason: DropReason::ShortBuffer,
            });
            return;
        };
        let body = &blob[4..];
        let mut actions = Vec::new();

        macro_rules! decode {
            ($t:ty) => {
                match read_body::<$t>(body) {
                    Ok(v) => v,
                    Err(e) => {
                        self.events.push(SessionEvent::Dropped {
                            queue: NetQueue::UiQueue,
                            opcode: op,
                            reason: DropReason::Malformed(e),
                        });
                        return;
                    }
                }
            };
        }

        match op {
            Opcode::LOGIN_LOGIN_CHARACTER_SET => {
                let set = decode!(LoginCharacterSet);
                self.account.clone_from(&set.account);
                self.characters = set.clone();
                self.flow.character_set_received(&mut actions);
                self.events.push(SessionEvent::CharacterSet(Box::new(set)));
            }
            Opcode::LOGIN_WORLD_INFO => {
                let w = decode!(dereth_protocol::login::LoginWorldInfo);
                self.world_name = Some(w.world_name.clone());
                self.events.push(SessionEvent::WorldInfo {
                    connections: w.connections,
                    max_connections: w.max_connections,
                    name: w.world_name,
                });
            }
            Opcode::LOGIN_ENTER_GAME_SERVER_READY => {
                self.flow.server_ready(&mut actions);
                self.events.push(SessionEvent::EnterWorldReady);
            }
            Opcode::LOGIN_PLAYER_DESCRIPTION => {
                let d = decode!(LoginPlayerDescription);
                self.flow.player_description_received(&mut actions);
                self.events
                    .push(SessionEvent::PlayerDescription(Box::new(d)));
                // The crucial-events gate opens — replay everything held, in order.
                for held in self.ui.crucial_events_received() {
                    self.deliver_ui_blob(&strip_order_header(&held));
                }
            }
            Opcode::LOGIN_CHARACTER_SCREEN_MESSAGE => {
                let m = decode!(dereth_protocol::login::LoginCharacterScreenMessage);
                self.events
                    .push(SessionEvent::CharacterScreenMessage(m.shown()));
            }
            Opcode::CHARACTER_CHARACTER_ERROR => {
                let e = decode!(dereth_protocol::login::CharacterError);
                self.flow.character_error(e.char_error, &mut actions);
                self.events.push(SessionEvent::CharacterError(e.char_error));
            }
            Opcode::LOGIN_ACCOUNT_BOOTED => {
                let b = decode!(LoginAccountBooted);
                self.flow
                    .disconnect(DisconnectReason::AccountBooted, &mut actions);
                self.events.push(SessionEvent::AccountBooted(b.reason));
            }
            Opcode::LOGIN_ACCOUNT_BANNED => {
                let b = decode!(LoginAccountBanned);
                self.flow
                    .disconnect(DisconnectReason::AccountBanned, &mut actions);
                self.events.push(SessionEvent::AccountBanned {
                    expiry: b.expiry,
                    reason: b.reason,
                });
            }
            Opcode::LOGIN_AWAITING_SUBSCRIPTION_EXPIRATION => {
                let s = decode!(dereth_protocol::login::LoginAwaitingSubscriptionExpiration);
                self.events
                    .push(SessionEvent::SubscriptionExpiresIn { minutes: s.minutes });
            }
            Opcode::LOGIN_EXECUTE_LOG_OFF => {
                // The received form's body is only the opcode; anything else is not this message.
                decode!(dereth_protocol::login::LoginExecuteLogOff);
                self.flow.log_off_received(&mut actions);
                // **The rest of this clear is on the enter-world edge.** The log-off execution is
                // eight stores long and every one of them is a scalar on player state itself: two
                // login flags, the log-off time, the exit-world disconnect, two player flags and
                // **the player id**. It clears no ordering state, no
                // parked list, no instance table and no stamper — those belong to `WorldObjects` and
                // object maintenance, and the smart-box reset with argument 1 clears them on
                // the *next* entry. See [`Session::reset_world_view`].
                //
                // The one store kept here is the player id, because this is the handler that
                // zeroes it. It is **not** the duplicate guard: that guard is
                // the smart box's player id (the load, the test and the `return 3`), a different
                // field cleared inside the reset's flag-gated arm. This build has one field for both,
                // so it is cleared in both places — clearing it *here alone* would leave it set
                // after any ending that is not a clean `0xF653`, and the next session's `0xF746`
                // would then be refused as a duplicate.
                self.player_id = None;
                // The event-counter reset is not open-coded here: it is
                // [`crate::client_session::FlowAction::ExitWorldDisconnect`], which
                // the log-off execution raises here **and** which the two
                // enter-world edges raise as well — so the reset happens on all three of the
                // client's edges, and exactly once here.
                // `early-inventory-and-casting` of the corpus is the oracle: the stamp
                // climbs to 289, the player logs off, and the next action carries **1**.
                self.events.push(SessionEvent::LoggedOff);
            }
            Opcode::CHARACTER_CHARACTER_DELETE => {
                decode!(dereth_protocol::login::CharacterDeleteAck);
                self.events.push(SessionEvent::CharacterDeleted);
            }
            Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE => {
                let r = decode!(CharGenVerificationResponse);
                // The char-gen verification response, case
                // `CG_VERIFICATION_RESPONSE_OK` (1):
                //
                // ```text
                // char_set = the UI flow's persistent character set; added = false
                // if the char-gen verification state is PENDING:
                //     unpack a CharacterIdentity from the rest of the payload
                //     added = add_identity(char_set, id)
                // else:                                                      // the restore arm
                //     id = the identity at the char-gen state's slot
                //     if there is one: added = unpack the rest of the payload over it
                // raise the char-gen verification-response notice (OK)
                // set the verification state to UNDEF
                // if added: raise the character-set notice
                // ```
                //
                // **The identity must not be dropped.** The client does **not** wait for the server to re-send `0xF658`
                // after a creation: it builds the new list entry out of `0xF643`'s own
                // `CharacterIdentity` and re-raises the character-set notice itself. ACE agrees —
                // `CharacterHandler.CharacterCreateEx` sends `GameMessageCharacterCreateResponse`
                // (guid, name, 0) and **no** `GameMessageCharacterList`, which is only sent on
                // authenticate, on delete and on log-off (`Session.cs:273`,
                // `CharacterHandler.cs:322`, `AuthenticationHandler.cs:258`).
                //
                // The order below is the client's: the char-gen notice first (so the wizard has
                // its awaiting-character-set flag set), then the character-set notice, which is what
                // the char-gen wizard's per-frame step acts on.
                // **The `else` branch is the whole of RESTORE.**
                //
                // A restore is answered by `0xF643`, decoded like a creation's reply; what makes
                // the restore finish is this handler's **second branch**.
                //
                // ACE answers `0xF7D9 CharacterRestore` (`CharacterHandler.cs:379`) with
                // `GameMessageCharacterRestore`, whose opcode is `0xF643` — the *same* opcode a
                // creation is answered with, and its own enum comment says so
                // (`GameMessageOpcode.cs:42`, *"This is a duplicate…"*). It sends **no**
                // `0xF658 CharacterList` afterwards.
                //
                // The char-gen verification response separates the two using the verification
                // state. Only a creation sets it to PENDING:
                //
                // ```c
                // if (state == PENDING) { CharacterIdentity id; unpack(&id, ..);
                //                         added = add_identity(char_set, &id); }
                // else                  { id = identity_at(char_set, state.slot);
                //                         if (id) added = unpack(id, ..); }   // the restore arm
                // ```
                //
                // Without the `else`, a restore computes
                // `added = ok && pending && ..` = **false** with `pending` false, raises no
                // `CharacterSet`, and leaves the list showing the character still greyed out —
                // *and the please-wait modal still up*, because the character screen's per-frame
                // step's tail is the only thing that closes it and it runs on the arrival
                // of a character set (`screens/charmgmt.rs`). A wait dialog has no
                // buttons, so the restore would never finish.
                let ok = r.response_type == CG_VERIFICATION_RESPONSE_OK;
                let pending = std::mem::take(&mut self.chargen_pending);
                let slot = std::mem::take(&mut self.restore_slot);
                let added = if !ok {
                    // Results 3 and 4 both clear the slot to -1, and the default arm does too.
                    false
                } else if pending {
                    self.characters.add_identity(&r.identity)
                } else {
                    // Look up the identity at the slot and unpack over it: the row is **replaced**,
                    // which is
                    // what clears `seconds_greyed_out` and takes the character out of the
                    // pending-delete state the *Restore* button is offered for.
                    slot.is_some_and(|slot| self.characters.replace_identity(slot, &r.identity))
                };
                self.events.push(SessionEvent::CharGenResponse(Box::new(r)));
                if added {
                    self.events.push(SessionEvent::CharacterSet(Box::new(
                        self.characters.clone(),
                    )));
                }
            }
            _ => {
                // Everything else is the game layer's: hand it over with its ordering resolved.
                self.events.push(SessionEvent::UiEvent {
                    opcode: op,
                    blob: blob.to_vec(),
                });
            }
        }
        self.run_actions(actions);
    }
}

impl<T: Transport> Session<T> {
    /// Build the DDD interrogation response the caller owes, and send it on queue 5.
    ///
    /// The iteration lists come from the caller's dat files; this crate cannot see them.
    pub fn answer_ddd_interrogation(&mut self, response: &DddInterrogationResponse) {
        self.send_bare(response);
    }

    /// This sends `0xF7EA DDD_OnEndDDD` — "I have everything you promised".
    ///
    /// The client sends this from two places, both of which are the patcher's rather than the
    /// dispatcher's: the begin-DDD completion when the pending-download list was empty to
    /// begin with, and the pending-download removal when the last download
    /// lands. The third, the end-of-DDD handler's reply to a received `0xF7EA`, is
    /// [`crate::client_session::dispatch::database::dispatch`]'s `send_end` and needs no caller.
    pub fn send_ddd_end(&mut self) {
        self.send_bare(&dereth_protocol::admin::DddEndDdd);
    }

    /// The cache's async get from other sources -- ask the server for one resource the dat
    /// files do not hold, as `0xF7E3 DDD_RequestDataMessage` on queue 5.
    ///
    /// This is the client's *only* producer of `0xF7E3`, and it has nothing to do with the
    /// interrogation: it fires when a database-object cache read misses on disk at run time, which is why
    /// ACE's handler for it is registered on `SessionState.WorldConnected` and answers only
    /// `LandBlock`, `LandBlockInfo` and `EnvCell`
    /// (ACE's `Source/ACE.Server/Network/Handlers/DDDHandler.cs`).
    ///
    /// It writes `0xF7E3` as the event type, the qualified data id's type and id, and delivers the
    /// message.
    pub fn request_ddd_data(&mut self, resource_type: u32, resource_id: u32) {
        self.send_bare(&dereth_protocol::admin::DddRequestData {
            resource_type,
            resource_id,
        });
    }
}

fn leading_opcode(blob: &[u8]) -> Option<Opcode> {
    if blob.len() < 4 {
        return None;
    }
    Some(Opcode(u32::from_le_bytes([
        blob[0], blob[1], blob[2], blob[3],
    ])))
}

/// Strip a `OrderedEventHeader` if one is present, as the UI dispatch path does.
fn strip_order_header(blob: &[u8]) -> Vec<u8> {
    if blob.len() >= dereth_protocol::OrderedEventHeader::PACK_SIZE
        && leading_opcode(blob).map(|o| o.0) == Some(dereth_protocol::OrderedEventHeader::MAGIC)
    {
        blob[dereth_protocol::OrderedEventHeader::PACK_SIZE..].to_vec()
    } else {
        blob.to_vec()
    }
}
