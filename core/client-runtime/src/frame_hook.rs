//! The frame's one extension point: a step the product installs, run once per frame after the
//! frame's inbound messages have been applied and before the UI step.
//!
//! The runtime names nothing that plugs in here. A product installs a [`FrameHook`](crate::frame_hook::FrameHook)
//! ([`crate::app::App::install_frame_hook`]); each frame the hook is handed a [`HookFrame`](crate::frame_hook::HookFrame): the
//! game view and the object model as they stand, the chat lines this frame's messages produced
//! (not yet shown), and the extension commands the player typed since the last step. It answers
//! through the same frame: lines to hide or to print, requests for the interaction step (the same
//! requests the UI makes), and the command words it claims.
//!
//! Running here means the hook always sees a model consistent with the lines it is handed, what
//! it prints and hides is settled before the chat windows take the frame's lines, and what it asks
//! for is acted on in this frame's interaction step, as a click in the UI step would be.
//!
//! The hook also answers the server's plugin audit, `Admin_QueryPluginList` (`0x02AE`) and
//! `Admin_QueryPlugin` (`0x02B1`). With no hook installed, or a hook that declines, the client
//! answers as retail does when no third-party interface is running.

use std::any::Any;

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::{GameView, UiRequest};
use dereth_client_model::cmd::CommandInterp;

/// A command line the player typed whose verb an installed hook claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCommand {
    /// The verb, as the hook registered it (lowercase).
    pub verb: String,
    /// Everything after the verb, with the player's own spacing, trimmed.
    pub args: String,
    /// The arguments split into words on spaces and tabs.
    pub words: Vec<String>,
}

/// The answer to one `Admin_QueryPlugin` (`0x02B1`): `Admin_QueryPluginResponse` (`0x02B2`)'s
/// fields after its context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginQueryAnswer {
    pub success: bool,
    pub name: String,
    pub author: String,
    pub email: String,
    pub webpage: String,
}

/// The list answer when no third-party interface is running.
pub const PLUGIN_LIST_NOT_IN_USE: &str = "3rd party API not in use.";

/// What a single plugin query leaves in a field it has nothing for.
pub const PLUGIN_QUERY_UNKNOWN: &str = "Unknown";

impl PluginQueryAnswer {
    /// The answer when no third-party interface is running: failure, the asked name, and
    /// `Unknown` in the other three fields.
    #[must_use]
    pub fn not_in_use(asked: &str) -> Self {
        Self {
            success: false,
            name: asked.to_owned(),
            author: PLUGIN_QUERY_UNKNOWN.to_owned(),
            email: PLUGIN_QUERY_UNKNOWN.to_owned(),
            webpage: PLUGIN_QUERY_UNKNOWN.to_owned(),
        }
    }
}

/// A step run once per frame (see the module documentation).
pub trait FrameHook: std::fmt::Debug {
    /// This frame's step.
    fn step(&mut self, frame: &mut HookFrame<'_>);

    /// The `Admin_QueryPluginListResponse` (`0x02AF`) text, or `None` to answer as a client with
    /// no third-party interface.
    fn plugin_list(&self) -> Option<String> {
        None
    }

    /// The answer to `Admin_QueryPlugin` (`0x02B1`) for `asked`, or `None` to answer as a client
    /// with no third-party interface.
    fn plugin_query(&self, asked: &str) -> Option<PluginQueryAnswer> {
        let _ = asked;
        None
    }

    /// Whether the hook reads the frame's messages ([`HookFrame::messages`]): while it does not,
    /// the client keeps no copy of them.
    fn wants_messages(&self) -> bool {
        false
    }

    /// The hook as `Any`, so that the product that installed it can reach its own type again.
    fn as_any(&self) -> &dyn Any;

    /// The hook as `Any`, mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// One frame, as the hook sees it and answers it.
pub struct HookFrame<'a> {
    /// The read-only view the retail UI reads.
    pub view: &'a dyn GameView,
    /// The object model.
    pub world: &'a dereth_client_model::World,
    /// The client's command table, to tell the client's own commands from extension ones.
    pub commands_table: &'a CommandInterp,
    /// The world clock this frame, in seconds (the server's time once synchronized).
    pub server_time: f64,
    /// The frame clock this frame, in seconds since the client started: simulated in a
    /// fixed-step run, so the same frame of a replay always reads the same time.
    pub local_time: f64,
    /// The chat lines this frame's messages produced, in order, not yet shown.
    pub chat: &'a [ChatMessage],
    /// The extension commands typed since the last step, in order.
    pub typed: &'a [ExtensionCommand],
    /// What the server said this frame about the player's own requests, in order.
    pub outcomes: &'a [Outcome],
    /// The world's name, as the server gave it at login, once known.
    pub world_name: Option<&'a str>,
    /// The account logged in, once known.
    pub account: Option<&'a str>,
    /// The session as the pre-game screens see it: its phase and the character list.
    pub session: &'a SessionFacts,
    /// Where the player and the objects are.
    pub places: &'a dyn Places,
    /// Whether the player moved the character themselves this frame (a movement key or button,
    /// pressed or released): the player's own movement takes precedence over a hook's.
    pub player_input: bool,
    /// Whether the server took control of the character's movement since the hook last ran (to
    /// walk it to something used or attacked, or through a stance change or a teleport): a hook's
    /// movement gives way to it.
    pub server_moving: bool,
    /// The messages of this frame, while the hook asks for them ([`FrameHook::wants_messages`]):
    /// those received, in the order the client applied them, then those it sent.
    pub messages: &'a [WireMessage],
    /// The client's travel: what it told since the hook last ran, and the journey in progress.
    pub travel: &'a crate::travel::TravelStatus,
    /// This world's portal and recall knowledge, once a world is entered.
    pub travel_store: Option<&'a dereth_navigation::PortalStore>,
    hidden: Vec<usize>,
    printed: Vec<ChatMessage>,
    requests: Vec<UiRequest>,
    verbs: Option<Vec<String>>,
    nav: Vec<NavRequest>,
    session_requests: Vec<SessionRequest>,
    travel_requests: Vec<crate::travel::TravelRequest>,
}

/// No travel: a frame with no journey and nothing told.
static NO_TRAVEL: crate::travel::TravelStatus = crate::travel::TravelStatus {
    events: Vec::new(),
    trip: None,
};

/// One message as it crossed the wire: which way, and its blob, `[opcode][body]`. A received game
/// event comes without its ordered-event header (its own type first); a sent game action keeps
/// its ordered-action header, as it went to the transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireMessage {
    pub dir: dereth_protocol::codecs::Dir,
    pub blob: Vec<u8>,
}

impl WireMessage {
    /// A received message's blob from a session event: the blob the client applied, or the typed
    /// message written back out. `None` for an event that is not a message (a state change, a
    /// dropped blob, a step of the data exchange).
    #[must_use]
    pub fn received(event: &dereth_client_net::client_session::SessionEvent) -> Option<Self> {
        use dereth_client_net::client_session::SessionEvent as E;
        let write = |r: Result<Vec<u8>, dereth_protocol::MessageError>| r.ok();
        let blob = match event {
            E::UiEvent { blob, .. } => Some(blob.clone()),
            E::WorldObject { opcode, body } => {
                let mut b = opcode.0.to_le_bytes().to_vec();
                b.extend_from_slice(body);
                Some(b)
            }
            E::CharacterSet(m) => write(dereth_protocol::write_blob(m.as_ref())),
            E::PlayerDescription(m) => write(dereth_protocol::write_blob(m.as_ref())),
            E::CharGenResponse(m) => write(dereth_protocol::write_blob(m.as_ref())),
            E::WorldInfo {
                connections,
                max_connections,
                name,
            } => write(dereth_protocol::write_blob(
                &dereth_protocol::login::LoginWorldInfo {
                    connections: *connections,
                    max_connections: *max_connections,
                    world_name: name.clone(),
                },
            )),
            E::TurbineChat(body) => {
                let mut b = 0xF7DE_u32.to_le_bytes().to_vec();
                b.extend_from_slice(body);
                Some(b)
            }
            _ => None,
        }?;
        Some(Self {
            dir: dereth_protocol::codecs::Dir::S2c,
            blob,
        })
    }

    /// A blob the client sent.
    #[must_use]
    pub fn sent(blob: Vec<u8>) -> Self {
        Self {
            dir: dereth_protocol::codecs::Dir::C2s,
            blob,
        }
    }
}

/// Where the player and the objects are, for a hook that steers or marks by position.
pub trait Places {
    /// The player's position as the client simulates it, once there is a body.
    fn player(&self) -> Option<dereth_primitives::Position>;
    /// An object's last known position; `None` for one held in a container, or not placed.
    fn object(&self, id: dereth_primitives::ObjectId) -> Option<dereth_primitives::Position>;
    /// Whether the run lock (autorun) is on.
    fn autorun(&self) -> bool;
    /// Whether a straight line from `from` to `to` is clear of the world (its walls, the ground,
    /// and the objects that stop a body, creatures aside); `None` where it cannot tell.
    fn sight(
        &self,
        _from: &dereth_primitives::Position,
        _to: &dereth_primitives::Position,
    ) -> Option<bool> {
        None
    }
}

/// No positions: a frame with no world.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPlaces;

impl Places for NoPlaces {
    fn player(&self) -> Option<dereth_primitives::Position> {
        None
    }

    fn object(&self, _id: dereth_primitives::ObjectId) -> Option<dereth_primitives::Position> {
        None
    }

    fn autorun(&self) -> bool {
        false
    }
}

/// Positions read from the object stream, with the player's from its simulated body, and lines of
/// sight through the body's world where there is one.
#[derive(Debug)]
pub struct StreamPlaces<'a> {
    pub player: Option<dereth_primitives::Position>,
    pub objects: &'a crate::objects::ObjectStream,
    pub autorun: bool,
    pub sight: Option<&'a crate::sight::Sight<'a>>,
}

impl Places for StreamPlaces<'_> {
    fn player(&self) -> Option<dereth_primitives::Position> {
        self.player.or_else(|| {
            self.objects
                .player()
                .and_then(|p| self.objects.presence(p))
                .and_then(|p| p.position)
        })
    }

    fn object(&self, id: dereth_primitives::ObjectId) -> Option<dereth_primitives::Position> {
        self.objects.presence(id).and_then(|p| p.position)
    }

    fn autorun(&self) -> bool {
        self.autorun
    }

    fn sight(
        &self,
        from: &dereth_primitives::Position,
        to: &dereth_primitives::Position,
    ) -> Option<bool> {
        self.sight?.clear(from, to)
    }
}

/// Where the session is, as the pre-game screens see it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SessionPhase {
    /// No connection has been made.
    #[default]
    Offline,
    /// Connected; the character list has not arrived.
    Connecting,
    /// The character list has arrived and no character is entering.
    CharacterSelect,
    /// A character is entering the world.
    Entering,
    /// A character is in the world.
    InWorld,
    /// The connection ended.
    Disconnected,
}

impl SessionPhase {
    /// The phase's name as plugins read it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Connecting => "connecting",
            Self::CharacterSelect => "character_select",
            Self::Entering => "entering",
            Self::InWorld => "in_world",
            Self::Disconnected => "disconnected",
        }
    }

    /// The phase of a session in `state`.
    #[must_use]
    pub const fn of(state: dereth_client_net::client_session::SessionState) -> Self {
        use dereth_client_net::client_session::SessionState as S;
        match state {
            S::Connected | S::Patching => Self::Connecting,
            S::CharacterSelect => Self::CharacterSelect,
            S::EnteringWorld => Self::Entering,
            S::Playable => Self::InWorld,
            S::Disconnected(_) => Self::Disconnected,
        }
    }
}

/// The session's phase, the account's characters and whether the character is in portal space.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFacts {
    pub phase: SessionPhase,
    /// The account's characters, in the list's order: id and name.
    pub characters: Vec<(dereth_primitives::ObjectId, String)>,
    /// A teleport is in progress: from the server's teleport (or the character's arrival in the
    /// world) until the client has placed the character at its destination.
    pub portal_space: bool,
    /// The character creation screens are up (told by the interface, which knows its screens;
    /// never set where there is none).
    pub creating_character: bool,
}

/// A movement a hook asks of the player's character: what a player turning, holding the run
/// lock, holding a movement key or charging a jump does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavRequest {
    /// Turn to a heading, in degrees clockwise from north, with the client's turn-to-heading
    /// movement (the one the camera uses in first person).
    Face(f32),
    /// Set the run lock on or off.
    Autorun(bool),
    /// Press (`true`) or let go of one of the game's movement controls, as the game's own
    /// movement: held, a step or a turn goes on until it is let go; the jump charges while held
    /// and leaps when let go.
    Hold(NavKey, bool),
    /// Put down a jump being charged without leaping, as the Escape key puts one down.
    CancelJump,
}

/// The game's movement controls a hook may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavKey {
    Forward,
    Backward,
    StrafeLeft,
    StrafeRight,
    TurnLeft,
    TurnRight,
    /// The walk key: held, the character walks where it would run.
    Walk,
    /// The jump: held to charge it, let go to leap.
    Jump,
}

impl NavKey {
    /// The game's action for the control.
    #[must_use]
    pub const fn action(self) -> dereth_client_contract::actions::ActionId {
        use dereth_client_contract::actions::movement as m;
        match self {
            Self::Forward => m::MOVE_FORWARD,
            Self::Backward => m::MOVE_BACKWARD,
            Self::StrafeLeft => m::STRAFE_LEFT,
            Self::StrafeRight => m::STRAFE_RIGHT,
            Self::TurnLeft => m::TURN_LEFT,
            Self::TurnRight => m::TURN_RIGHT,
            Self::Walk => m::TOGGLE_RUN_WALK,
            Self::Jump => m::JUMP,
        }
    }
}

/// A pre-game operation a hook asks for: what the character screen's buttons and the log-out
/// ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRequest {
    /// Enter the world as this character.
    LogOn(dereth_primitives::ObjectId),
    /// Log the character in the world off, back to the character list.
    LogOff,
    /// Log off if in the world, then end the client.
    Quit,
}

static NO_PLACES: NoPlaces = NoPlaces;
static NO_SESSION: std::sync::LazyLock<SessionFacts> =
    std::sync::LazyLock::new(SessionFacts::default);

impl std::fmt::Debug for HookFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookFrame")
            .field("server_time", &self.server_time)
            .field("chat", &self.chat.len())
            .field("typed", &self.typed)
            .finish_non_exhaustive()
    }
}

/// What a step asked for, applied by the runtime after the step returns.
#[derive(Debug, Default)]
pub struct HookOutput {
    /// Indices into [`HookFrame::chat`] of lines not to show.
    pub hidden: Vec<usize>,
    /// Local lines to show, after this frame's own.
    pub printed: Vec<ChatMessage>,
    /// Requests for this frame's interaction step.
    pub requests: Vec<UiRequest>,
    /// The command verbs claimed from now on, when the step changed them.
    pub verbs: Option<Vec<String>>,
    /// Movements for the player's character, in order.
    pub nav: Vec<NavRequest>,
    /// Pre-game operations, in order.
    pub session: Vec<SessionRequest>,
    /// Journeys asked of the travel, in order.
    pub travel: Vec<crate::travel::TravelRequest>,
}

impl<'a> HookFrame<'a> {
    /// A frame over the given state, with nothing asked for yet.
    #[must_use]
    pub fn new(
        view: &'a dyn GameView,
        world: &'a dereth_client_model::World,
        commands_table: &'a CommandInterp,
        server_time: f64,
        local_time: f64,
        chat: &'a [ChatMessage],
        typed: &'a [ExtensionCommand],
    ) -> Self {
        Self {
            view,
            world,
            commands_table,
            server_time,
            local_time,
            chat,
            typed,
            outcomes: &[],
            world_name: None,
            account: None,
            session: &NO_SESSION,
            places: &NO_PLACES,
            player_input: false,
            server_moving: false,
            messages: &[],
            travel: &NO_TRAVEL,
            travel_store: None,
            hidden: Vec::new(),
            printed: Vec::new(),
            requests: Vec::new(),
            verbs: None,
            nav: Vec::new(),
            session_requests: Vec::new(),
            travel_requests: Vec::new(),
        }
    }

    /// The same frame, with the travel's news and this world's portal knowledge.
    #[must_use]
    pub fn with_travel(
        mut self,
        travel: &'a crate::travel::TravelStatus,
        store: Option<&'a dereth_navigation::PortalStore>,
    ) -> Self {
        self.travel = travel;
        self.travel_store = store;
        self
    }

    /// Ask the travel for a journey, a plan or a cancellation.
    pub fn travel(&mut self, request: crate::travel::TravelRequest) {
        self.travel_requests.push(request);
    }

    /// The same frame, with the session's phase and characters.
    #[must_use]
    pub fn with_pregame(mut self, session: &'a SessionFacts) -> Self {
        self.session = session;
        self
    }

    /// The same frame, with positions.
    #[must_use]
    pub fn with_places(mut self, places: &'a dyn Places) -> Self {
        self.places = places;
        self
    }

    /// The same frame, saying whether the player moved the character themselves and whether the
    /// server is moving it.
    #[must_use]
    pub fn with_control(mut self, player_input: bool, server_moving: bool) -> Self {
        self.player_input = player_input;
        self.server_moving = server_moving;
        self
    }

    /// The same frame, with its messages.
    #[must_use]
    pub fn with_messages(mut self, messages: &'a [WireMessage]) -> Self {
        self.messages = messages;
        self
    }

    /// Ask a movement of the player's character.
    pub fn nav(&mut self, request: NavRequest) {
        self.nav.push(request);
    }

    /// Ask a pre-game operation.
    pub fn session_request(&mut self, request: SessionRequest) {
        self.session_requests.push(request);
    }

    /// The same frame, with this frame's outcomes.
    #[must_use]
    pub fn with_outcomes(mut self, outcomes: &'a [Outcome]) -> Self {
        self.outcomes = outcomes;
        self
    }

    /// The same frame, naming the world and the account.
    #[must_use]
    pub fn with_session(mut self, world_name: Option<&'a str>, account: Option<&'a str>) -> Self {
        self.world_name = world_name;
        self.account = account;
        self
    }

    /// Do not show line `index` of [`Self::chat`].
    pub fn hide_chat(&mut self, index: usize) {
        if index < self.chat.len() && !self.hidden.contains(&index) {
            self.hidden.push(index);
        }
    }

    /// Show a local line in the chat windows. Nothing is sent to the server.
    pub fn print_chat(&mut self, line: ChatMessage) {
        self.printed.push(line);
    }

    /// Queue a request for this frame's interaction step, as the UI would.
    pub fn request(&mut self, request: UiRequest) {
        self.requests.push(request);
    }

    /// Claim these command verbs from now on (replacing the claimed set). A verb the client's own
    /// command table has is never claimed.
    pub fn claim_verbs(&mut self, verbs: Vec<String>) {
        self.verbs = Some(verbs);
    }

    /// Whether `verb` is one of the client's own commands.
    #[must_use]
    pub fn is_client_command(&self, verb: &str) -> bool {
        self.commands_table.lookup(verb).is_some()
    }

    /// What the step asked for.
    #[must_use]
    pub fn into_output(self) -> HookOutput {
        HookOutput {
            hidden: self.hidden,
            printed: self.printed,
            requests: self.requests,
            verbs: self.verbs,
            nav: self.nav,
            session: self.session_requests,
            travel: self.travel_requests,
        }
    }
}

/// What the server said about one of the player's own requests: the acknowledgements and refusals
/// the retail interface turns into chat lines or uses to end a busy state. The hook reads them to
/// learn how a request it made ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// `Item_UseDone` (`0x01C7`): a use or a cast ended; `failure` 0 is success, anything else the
    /// reason it failed.
    UseDone { failure: u32 },
    /// `Communication_WeenieError` (`0x028A`) or `Communication_WeenieErrorWithString`
    /// (`0x028B`): something the player asked for was refused.
    WeenieError { code: u32 },
    /// `Character_ServerSaysAttemptFailed` (`0x00A0`): an inventory request on `object` was
    /// refused.
    AttemptFailed {
        object: dereth_primitives::ObjectId,
        reason: u32,
    },
    /// `Trade_TradeFailure` (`0x0207`): adding `object` to a trade was refused.
    TradeFailed {
        object: dereth_primitives::ObjectId,
        reason: u32,
    },
    /// `Character_CharacterError` (`0xF659`): entering the world, or another character-list
    /// operation, failed for this reason.
    LoginFailed { code: u32 },
    /// `Combat_HandleAttackDoneEvent` (`0x01A7`): a swing of the player's attack ended with the
    /// sequence going on (`error` 0), or the attack sequence ended (`ActionCancelled`, `0x0036`,
    /// after its last swing or after a refusal), or another reason.
    AttackDone { error: u32 },
    /// `Combat_HandleCommenceAttackEvent` (`0x01B8`): the next swing of a repeating attack began.
    AttackCommenced,
}

/// The outcomes among `events`, in order. An event whose payload does not read is skipped.
#[must_use]
pub fn outcomes_of(events: &[dereth_client_net::client_session::SessionEvent]) -> Vec<Outcome> {
    use dereth_protocol::combat::CombatHandleAttackDoneEvent;
    use dereth_protocol::comms::{CommunicationWeenieError, CommunicationWeenieErrorWithString};
    use dereth_protocol::objects::{CharacterServerSaysAttemptFailed, ItemUseDone};
    use dereth_protocol::trade::TradeTradeFailure;
    use dereth_protocol::{Message as _, Opcode};
    let mut out = Vec::new();
    for e in events {
        if let dereth_client_net::client_session::SessionEvent::CharacterError(code) = e {
            out.push(Outcome::LoginFailed { code: *code });
            continue;
        }
        let dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob } = e else {
            continue;
        };
        // The blob begins with its own type dword.
        let body = blob.get(4..).unwrap_or_default();
        let mut r = dereth_protocol::archive::Reader::new(body);
        let o = match *opcode {
            Opcode::ITEM_USE_DONE => ItemUseDone::read(&mut r).ok().map(|m| Outcome::UseDone {
                failure: m.failure_type,
            }),
            Opcode::COMBAT_HANDLE_ATTACK_DONE_EVENT => CombatHandleAttackDoneEvent::read(&mut r)
                .ok()
                .map(|m| Outcome::AttackDone { error: m.error }),
            Opcode::COMBAT_HANDLE_COMMENCE_ATTACK_EVENT => Some(Outcome::AttackCommenced),
            Opcode::COMMUNICATION_WEENIE_ERROR => CommunicationWeenieError::read(&mut r)
                .ok()
                .map(|m| Outcome::WeenieError { code: m.error_type }),
            Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING => {
                CommunicationWeenieErrorWithString::read(&mut r)
                    .ok()
                    .map(|m| Outcome::WeenieError { code: m.error_type })
            }
            Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED => {
                CharacterServerSaysAttemptFailed::read(&mut r)
                    .ok()
                    .map(|m| Outcome::AttemptFailed {
                        object: m.object,
                        reason: m.reason,
                    })
            }
            Opcode::TRADE_TRADE_FAILURE => {
                TradeTradeFailure::read(&mut r)
                    .ok()
                    .map(|m| Outcome::TradeFailed {
                        object: m.item,
                        reason: m.reason,
                    })
            }
            _ => None,
        };
        out.extend(o);
    }
    out
}

/// The answer to one plugin audit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditReply {
    /// `Admin_QueryPluginListResponse` (`0x02AF`).
    List(dereth_protocol::admin::AdminQueryPluginListResponse),
    /// `Admin_QueryPluginResponse` (`0x02B2`).
    One(dereth_protocol::admin::AdminQueryPluginResponseSend),
}

/// The answer to `event` when it is `Admin_QueryPluginList` (`0x02AE`) or `Admin_QueryPlugin`
/// (`0x02B1`): `hook`'s answer, or, with no hook or one that declines, the answer of a client with
/// no third-party interface. Any other event, or one whose payload does not read, has none.
#[must_use]
pub fn audit_reply(
    event: &dereth_client_net::client_session::SessionEvent,
    hook: Option<&dyn FrameHook>,
) -> Option<AuditReply> {
    use dereth_protocol::admin::{
        AdminQueryPlugin, AdminQueryPluginList, AdminQueryPluginListResponse,
        AdminQueryPluginResponseSend,
    };
    use dereth_protocol::Message as _;
    let dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob } = event else {
        return None;
    };
    // The blob begins with its own type dword.
    let body = blob.get(4..).unwrap_or_default();
    let mut r = dereth_protocol::archive::Reader::new(body);
    if *opcode == dereth_protocol::Opcode::ADMIN_QUERY_PLUGIN_LIST {
        let q = AdminQueryPluginList::read(&mut r).ok()?;
        let plugin_list = hook
            .and_then(FrameHook::plugin_list)
            .unwrap_or_else(|| PLUGIN_LIST_NOT_IN_USE.to_owned());
        Some(AuditReply::List(AdminQueryPluginListResponse {
            context: q.context,
            plugin_list,
        }))
    } else if *opcode == dereth_protocol::Opcode::ADMIN_QUERY_PLUGIN {
        let q = AdminQueryPlugin::read(&mut r).ok()?;
        let a = hook
            .and_then(|h| h.plugin_query(&q.plugin_name))
            .unwrap_or_else(|| PluginQueryAnswer::not_in_use(&q.plugin_name));
        Some(AuditReply::One(AdminQueryPluginResponseSend {
            context: q.context,
            success: i32::from(a.success),
            name: a.name,
            author: a.author,
            email: a.email,
            webpage: a.webpage,
        }))
    } else {
        None
    }
}

/// A chat line as the client reads it once it is submitted: narrowed to the host's ANSI code page
/// (with Windows' best fit, so a full-width `／` or `ｐ` becomes `/` or `p`) and spelled back byte
/// for byte, as the client's own narrow text is. Commands are told apart in this text. `None`
/// when the line cannot be narrowed, which the client refuses to send.
#[must_use]
pub fn submitted_line(
    conversion: &dyn dereth_primitives::HostEncoding,
    text: &str,
) -> Option<String> {
    let narrow = conversion.narrow(&text.encode_utf16().collect::<Vec<_>>())?;
    Some(dereth_primitives::text::cp1252::decode(&narrow))
}

/// The command verb of a submitted line, and its arguments, as the client reads them: the line
/// (spaces, tabs and line ends around it trimmed) starts with `/` or `@`, and the verb is the
/// first word without the sigil and without trailing commas, in lowercase. `None` for a line that
/// is not a command or names no verb.
#[must_use]
pub fn command_verb(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_matches([' ', '\t', '\r', '\n']);
    let rest = trimmed
        .strip_prefix('/')
        .or_else(|| trimmed.strip_prefix('@'))?;
    let (word, args) = match rest.find([' ', '\t']) {
        Some(i) => (&rest[..i], rest[i..].trim_matches([' ', '\t'])),
        None => (rest, ""),
    };
    let verb = word.trim_end_matches(',').to_ascii_lowercase();
    (!verb.is_empty()).then_some((verb, args))
}

/// Split a typed line into an extension command if its verb ([`command_verb`]) is claimed and is
/// not the client's own, compared without case. Only a line the player typed is offered here: a
/// line sent on the player's behalf never reaches a hook as a command.
#[must_use]
pub fn claim_line(line: &str, verbs: &[String], table: &CommandInterp) -> Option<ExtensionCommand> {
    let (verb, args) = command_verb(line)?;
    if table.lookup(&verb).is_some() || !verbs.iter().any(|v| v.eq_ignore_ascii_case(&verb)) {
        return None;
    }
    Some(ExtensionCommand {
        verb,
        args: args.to_owned(),
        words: dereth_client_model::cmd::interp::find_all_words(args),
    })
}

#[cfg(test)]
mod tests {
    use super::{claim_line, command_verb, submitted_line};
    use dereth_client_model::cmd::CommandInterp;

    #[test]
    fn a_line_s_verb_is_read_after_the_line_is_narrowed_as_the_client_sends_it() {
        let cp1252 = dereth_client_model::HostText::default();
        for (wide, verb) in [
            ("\u{FF0F}plugin unload network.dereth.tank", "plugin"),
            (
                "/\u{FF50}\u{FF4C}\u{FF55}\u{FF47}\u{FF49}\u{FF4E} list",
                "plugin",
            ),
            ("\u{FF20}plugin list", "plugin"),
            ("/plugin, list", "plugin"),
            ("  @GO,,", "go"),
        ] {
            let sent = submitted_line(&*cp1252, wide).expect("narrows");
            assert_eq!(
                command_verb(&sent).map(|(v, _)| v).as_deref(),
                Some(verb),
                "{wide:?} is sent as {sent:?}"
            );
        }
        assert_eq!(command_verb("plugin list"), None, "not a command line");
        assert_eq!(command_verb("/ list"), None, "no verb");
        assert_eq!(
            command_verb("/plugin,   list  now "),
            Some(("plugin".to_owned(), "list  now"))
        );
    }

    #[test]
    fn a_claimed_verb_is_split_from_its_arguments_and_the_clients_own_commands_are_never_claimed() {
        let table = CommandInterp::new();
        let verbs = vec!["xp".to_owned(), "help".to_owned()];
        let c = claim_line("  /XP reset   now ", &verbs, &table).expect("claimed");
        assert_eq!((c.verb.as_str(), c.args.as_str()), ("xp", "reset   now"));
        assert_eq!(c.words, ["reset", "now"]);
        assert!(claim_line("@xp", &verbs, &table).is_some());
        assert!(
            claim_line("/help", &verbs, &table).is_none(),
            "the client's own command wins"
        );
        assert!(claim_line("/xpx", &verbs, &table).is_none());
        assert!(
            claim_line("xp", &verbs, &table).is_none(),
            "not a command line"
        );
        assert!(claim_line("/", &verbs, &table).is_none());
    }
}
