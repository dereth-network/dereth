//! A soak bot: a player that knows only what the wire tells it.
//!
//! The bot is the playable-loop scenario cut into behaviours
//! (`empyrean/testkit/tests/all/login/playable_loop.rs`): it logs in, creates a character through
//! chargen, enters the world, and then picks one behaviour at a time with its own seeded RNG:
//! wander, chat, loot (pick up food and eat it), fight (a drudge from `@create`, then loot its
//! corpse), trade (with a partner the driver pairs it with), relog (to character select and back)
//! or idle. Every behaviour has a deadline; a behaviour that misses
//! it is counted and abandoned, never asserted: the soak's pass/fail is its invariants.
//!
//! Its knowledge of the world comes only from the messages it receives: its own guid (chargen),
//! the objects it is told about (`CreateObject`, `UpdateObject`, `DeleteObject`, a server remove,
//! `ServerSaysContainId`, `PickupEvent`, `UpdatePosition`), combat and trade events. What it sends is
//! queued in [`BotIo`] as encoded messages, so the same bot runs over the in-memory wire (a
//! [`TestServer`](crate::TestServer)) and over real UDP sockets.
//!
//! The one thing the wire cannot do is the `@create` of a fight: like the playable-loop scenario,
//! the bot asks the driver (in [`BotIo::admin`]) to make its session an admin for that one command.
//! Over UDP there is no driver with world access, so the UDP smoke never fights.
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::collections::{BTreeMap, HashMap, HashSet};

use dereth_primitives::ObjectId;
use dereth_primitives::{IncomingMessage, NetQueue};
use dereth_protocol::combat::{
    CombatChangeCombatMode, CombatHandleAttackDoneEvent, CombatTargetedMeleeAttack,
    VictimNotificationSelf,
};
use dereth_protocol::comms::{
    CommunicationHearSpeech, CommunicationTalk, CommunicationWeenieError,
    CommunicationWeenieErrorWithString,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::items::{
    InventoryGetAndWieldItem, InventoryPutItemInContainer, InventoryUseEvent,
};
use dereth_protocol::login::{
    CharGenResult, CharGenVerificationResponse, CharacterLoginCompleteNotification,
    CharacterSendCharGenResult, LoginCharacterSet, LoginExecuteLogOff, LoginExecuteLogOffRequest,
    LoginSendEnterWorld, LoginSendEnterWorldRequest,
};
use dereth_protocol::movement::{
    AutonomousPosition, MoveTimestamps, MoveToStatePack, MovementAutonomousPosition,
    MovementMoveToState, MovementPositionEvent, RawMotionState,
};
use dereth_protocol::objects::{
    EffectsPlayerTeleport, InventoryPickupEvent, ItemCreateObject, ItemDeleteObject,
    ItemOnViewContents, ItemServerSaysContainId, ItemServerSaysRemove, ItemUpdateObject,
    ItemUseDone, ItemWearItem, LoginCreatePlayer, ObjectCreatePayload,
};
use dereth_protocol::trade::{
    Trade, TradeAcceptTradeRequest, TradeAddToTrade, TradeCloseTradeNegotiations,
    TradeOpenTradeNegotiations, TradeRegisterTrade,
};
use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};
use dereth_protocol::{self as proto, Message};

/// The weenies the behaviours look for (ACE's world DB).
pub const DRUDGE_SKULKER: u32 = 7;
/// The starter Training Dirk (the Heavy Weapons starter gear).
pub const TRAINING_DIRK: u32 = 12739;
/// `ItemType.Food`.
const ITEM_TYPE_FOOD: u32 = 0x0000_0020;
/// `EquipMask.MeleeWeapon`.
const MELEE_WEAPON: u32 = 0x0010_0000;
/// `CombatMode.NonCombat` and `Melee`.
const NON_COMBAT: u32 = 1;
const MELEE: u32 = 2;
/// `MotionCommand.WalkForward`, `HoldKey.None` and `HoldKey.Run`.
const WALK_FORWARD: u32 = 0x4500_0005;
const HOLD_NONE: u32 = 1;
const HOLD_RUN: u32 = 2;
/// ACE's `GameEventKillerNotification` (0x01AD; the retail name is `VictimNotificationOther`): sent
/// to the killer only.
const KILLER_NOTIFICATION: u32 = 0x01AD;
/// `WeenieError.TradeComplete`.
const TRADE_COMPLETE: u32 = 0x0529;

/// A small seeded RNG (SplitMix64), the bot's own: the world's `ThreadSafeRandom` is never drawn.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let x = (self.next_u64() >> 11) as f64;
        x / (1u64 << 53) as f64
    }

    /// A float in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

/// Where something is, as the wire said.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pos {
    pub cell: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot: Quat,
}

impl Pos {
    fn from_wire(p: &PositionWire) -> Self {
        Self {
            cell: p.objcell_id,
            x: p.frame.origin.x,
            y: p.frame.origin.y,
            z: p.frame.origin.z,
            rot: p.frame.orientation,
        }
    }

    fn wire(&self) -> PositionWire {
        PositionWire {
            objcell_id: self.cell,
            frame: Frame {
                origin: Vec3 {
                    x: self.x,
                    y: self.y,
                    z: self.z,
                },
                orientation: self.rot,
            },
        }
    }

    /// The horizontal distance to `o`.
    #[must_use]
    pub fn dist(&self, o: &Pos) -> f32 {
        empyrean_common::math::hypotf(self.x - o.x, self.y - o.y)
    }
}

/// An object the bot was told about.
#[derive(Debug, Clone)]
pub struct Known {
    pub wcid: u32,
    pub name: String,
    pub item_type: u32,
    pub pos: Option<Pos>,
    pub container: Option<u32>,
    pub wielder: Option<u32>,
    /// The bot's clock when it was first told about the object.
    pub first_seen: f64,
}

/// What the bot asks of the wire (and, for `admin`, of the in-process driver).
#[derive(Debug, Default)]
pub struct BotIo {
    /// Encoded messages to send, in order.
    pub out: Vec<(NetQueue, Vec<u8>)>,
    /// The next game action's sequence (`OrderedActionHeader.stamp`); restarts with each connection.
    pub stamp: u32,
    /// A chat line to type as an admin (the driver elevates the session for it, as the
    /// playable-loop scenario does).
    pub admin: Option<String>,
}

impl BotIo {
    pub fn action<M: Message>(&mut self, m: &M) {
        let data = proto::actions::pack_action(self.stamp, m).expect("an encodable game action");
        self.stamp = self.stamp.wrapping_add(1);
        self.out.push((NetQueue::Weenie, data));
    }

    fn message<M: Message>(&mut self, queue: NetQueue, m: &M) {
        self.out
            .push((queue, proto::write_blob(m).expect("an encodable message")));
    }
}

/// Where the bot is in its session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Waiting for the transport's handshake.
    Connecting,
    /// Waiting for the character list.
    CharacterList,
    /// Chargen sent; waiting for its answer.
    Creating { since: f64 },
    /// `EnterWorldRequest` sent at `since`; `EnterWorld` goes 0.2 s later.
    EnterRequested { since: f64 },
    /// `EnterWorld` sent; waiting for `CreatePlayer` and the character's own `CreateObject`.
    Entering { since: f64 },
    /// In the world, playing.
    InWorld,
    /// `LogOffRequest` sent; waiting for character select.
    LoggingOff { since: f64 },
    /// At character select until `until`, then enters again (unless the run is ending).
    CharacterSelect { until: f64 },
    /// Logged off for good (the end of the run), or given up.
    Done,
}

/// A behaviour.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Idle,
    Wander,
    Chat,
    Loot,
    Fight,
    Trade,
    Relog,
    Travel,
}

impl Kind {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Idle => "idle",
            Kind::Wander => "wander",
            Kind::Chat => "chat",
            Kind::Loot => "loot",
            Kind::Fight => "fight",
            Kind::Trade => "trade",
            Kind::Relog => "relog",
            Kind::Travel => "travel",
        }
    }
}

/// The trade roles the driver hands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeRole {
    Initiator,
    Responder,
}

#[derive(Debug, Clone, PartialEq)]
enum Activity {
    Idle {
        until: f64,
    },
    Wander {
        target: Pos,
        stopped: bool,
    },
    Loot {
        item: u32,
        phase: LootPhase,
        since: f64,
    },
    Fight {
        phase: FightPhase,
        since: f64,
    },
    Trade {
        partner: u32,
        role: TradeRole,
        phase: TradePhase,
        since: f64,
    },
    /// Dead: waiting for the teleport to the lifestone (`teleported` is when it came).
    Dead {
        since: f64,
        teleported: Option<f64>,
    },
    /// A hunter's `@teleloc` to its hunting ground (`teleported` is when the teleport came).
    Travel {
        since: f64,
        teleported: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum LootPhase {
    /// PutItemInContainer sent; walking to it.
    Picking,
    /// In the pack; Use (eat) sent.
    Eating,
}

#[derive(Debug, Clone, PartialEq)]
enum FightPhase {
    Wield {
        dirk: u32,
    },
    /// A hunter picks the nearest drudge it knows of (a generator's).
    Hunt,
    GoHome,
    /// `@create` typed at `at`.
    Create {
        at: f64,
    },
    Engage {
        drudge: u32,
        created: f64,
    },
    Attack {
        drudge: u32,
        created: f64,
        dones: u32,
        attacked: bool,
    },
    /// The drudge died near `at`; its corpse is one the bot first heard of after `created`.
    FindCorpse {
        at: Pos,
        created: f64,
    },
    OpenCorpse {
        corpse: u32,
    },
    Take {
        corpse: u32,
        item: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TradePhase {
    Open,
    Offered { at: f64 },
    Accepted,
}

/// Events the driver acts on (the invariant checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotEvent {
    /// `LogOffRequest` sent for this character.
    LoggingOff,
    /// Back at character select.
    LoggedOff,
    /// `EnterWorld` sent.
    Entering,
    /// In the world again (LoginComplete sent).
    Entered,
}

/// What a bot counted over its life.
#[derive(Debug, Clone, Default)]
pub struct BotStats {
    /// Messages received, by kind (the game-event type inside a `0xF7B0`, else the opcode).
    pub received: BTreeMap<u32, u64>,
    pub received_total: u64,
    pub sent_total: u64,
    /// Behaviours started, completed and abandoned (by reason), by behaviour.
    pub started: BTreeMap<&'static str, u64>,
    pub completed: BTreeMap<&'static str, u64>,
    pub abandoned: BTreeMap<String, u64>,
    pub kills: u64,
    pub deaths: u64,
    pub items_looted: u64,
    pub food_eaten: u64,
    pub trades_completed: u64,
    pub speech_heard: u64,
    pub logins: u64,
    pub reconnects: u64,
    /// `WeenieError`s received (plain and with a string), by code.
    pub weenie_errors: BTreeMap<u32, u64>,
    /// `AttackDone`s with a failure, by code.
    pub attack_errors: BTreeMap<u32, u64>,
    /// `UseDone`s with a failure, by code.
    pub use_errors: BTreeMap<u32, u64>,
}

/// A soak bot. See the module docs.
#[derive(Debug)]
pub struct Bot {
    pub index: usize,
    pub account: String,
    pub name: String,
    start_area: u32,
    rng: Rng,
    pub state: State,
    /// The character, once chargen answered.
    pub me: Option<u32>,
    pub pos: Option<Pos>,
    /// Where the character entered the world (the chargen spawn): wander and `@create` stay near it.
    pub home: Option<Pos>,
    known: HashMap<u32, Known>,
    activity: Activity,
    /// Set by the driver: the next behaviour is a trade with this partner.
    pub trade_with: Option<(u32, TradeRole)>,
    /// Set by the driver near the end of the run: log off and stay off.
    pub finish: bool,
    /// Relogs are allowed (the UDP smoke keeps its bots in the world).
    pub allow_relog: bool,
    /// Fights are allowed (they need the driver's admin elevation).
    pub allow_fight: bool,
    /// A hunter: goes to its hunting ground (`@teleloc`, as an admin for that one command) and
    /// fights the drudges the encounters there spawn, instead of `@create`d ones in the Academy.
    pub hunter: bool,
    /// The hunting ground's cell and the `@teleloc` line that goes there.
    pub hunting_ground: Option<(u32, String)>,
    /// The terrain of the hunting ground's landblock, so that its steps stay on the ground (a
    /// client knows the land from its own dats).
    pub terrain: Option<std::sync::Arc<Terrain>>,
    /// Only this behaviour (for looking at one behaviour in isolation).
    pub only: Option<Kind>,
    /// Print each change of behaviour or phase to stderr.
    pub trace: bool,
    last_traced: String,
    create_player_seen: bool,
    attack_dones: u32,
    killed: bool,
    died: bool,
    teleported: bool,
    trade_registered: bool,
    trade_complete: bool,
    view_contents: Option<(u32, Vec<u32>)>,
    worn: HashSet<u32>,
    pub stats: BotStats,
    pub events: Vec<BotEvent>,
}

/// The kind of a received message: the game-event type inside a `0xF7B0`, else the opcode.
#[must_use]
pub fn kind(m: &IncomingMessage) -> u32 {
    if m.opcode == proto::OrderedEventHeader::MAGIC {
        let mut blob = m.opcode.to_le_bytes().to_vec();
        blob.extend_from_slice(&m.body);
        if let Ok(event) = split_ui_blob(&blob) {
            return event.sub_type.0;
        }
    }
    m.opcode
}

/// Reads a received message as `M` (plain, or a game event), or `None` if it does not decode.
#[must_use]
pub fn read<M: Message>(m: &IncomingMessage) -> Option<M> {
    if m.opcode == proto::OrderedEventHeader::MAGIC {
        let mut blob = m.opcode.to_le_bytes().to_vec();
        blob.extend_from_slice(&m.body);
        let mut body = split_ui_blob(&blob).ok()?.body;
        M::read(&mut body).ok()
    } else {
        proto::read_body_padded::<M>(&m.body).ok()
    }
}

fn is<M: Message>(k: u32) -> bool {
    M::OPCODE.0 == k
}

impl Bot {
    /// A bot for `account`, whose character will be called `name` and start at `start_area`.
    #[must_use]
    pub fn new(index: usize, account: String, name: String, start_area: u32, seed: u64) -> Self {
        Self {
            index,
            account,
            name,
            start_area,
            rng: Rng::new(seed),
            state: State::Connecting,
            me: None,
            pos: None,
            home: None,
            known: HashMap::new(),
            activity: Activity::Idle { until: 0.0 },
            trade_with: None,
            finish: false,
            allow_relog: true,
            allow_fight: true,
            hunter: false,
            hunting_ground: None,
            terrain: None,
            only: None,
            trace: false,
            last_traced: String::new(),
            create_player_seen: false,
            attack_dones: 0,
            killed: false,
            died: false,
            teleported: false,
            trade_registered: false,
            trade_complete: false,
            view_contents: None,
            worn: HashSet::new(),
            stats: BotStats::default(),
            events: Vec::new(),
        }
    }

    /// Whether the bot is in the world with nothing to do (the driver pairs such bots to trade).
    #[must_use]
    pub fn is_idle_in_world(&self) -> bool {
        self.state == State::InWorld
            && matches!(self.activity, Activity::Idle { .. })
            && self.trade_with.is_none()
    }

    /// The name of the behaviour under way.
    #[must_use]
    pub fn activity_name(&self) -> &'static str {
        match &self.activity {
            Activity::Idle { .. } => "idle",
            Activity::Wander { .. } => "wander",
            Activity::Loot { .. } => "loot",
            Activity::Fight { .. } => "fight",
            Activity::Trade { .. } => "trade",
            Activity::Dead { .. } => "dead",
            Activity::Travel { .. } => "travel",
        }
    }

    /// The transport came up (again): a new connection starts its action sequence over.
    pub fn connected(&mut self, io: &mut BotIo) {
        io.stamp = 1;
        self.state = State::CharacterList;
    }

    /// The transport went down: forget the world and connect again.
    pub fn disconnected(&mut self) {
        self.stats.reconnects += 1;
        self.known.clear();
        self.state = if self.finish {
            State::Done
        } else {
            State::Connecting
        };
    }

    fn start(&mut self, kind: &Kind) {
        *self.stats.started.entry(kind.name()).or_insert(0) += 1;
    }

    fn complete(&mut self, name: &'static str, now: f64) {
        *self.stats.completed.entry(name).or_insert(0) += 1;
        self.idle(now);
    }

    /// A behaviour that could not start (nothing to do it with).
    fn skip(&mut self, name: &str, why: &str, now: f64) {
        *self
            .stats
            .abandoned
            .entry(format!("{name}: {why}"))
            .or_insert(0) += 1;
        self.idle(now);
    }

    fn abandon(&mut self, why: &str, now: f64) {
        *self
            .stats
            .abandoned
            .entry(format!("{}: {why}", self.activity_name()))
            .or_insert(0) += 1;
        self.idle(now);
    }

    fn idle(&mut self, now: f64) {
        let until = now + self.rng.range(1.0, 8.0);
        self.activity = Activity::Idle { until };
    }

    /// Reads the messages received since the last call.
    pub fn receive(&mut self, now: f64, inbox: &[IncomingMessage]) {
        for m in inbox {
            let k = kind(m);
            *self.stats.received.entry(k).or_insert(0) += 1;
            self.stats.received_total += 1;
            self.learn(now, k, m);
        }
    }

    fn remember(&mut self, now: f64, p: &ObjectCreatePayload) {
        let pos = p.physicsdesc.position.as_ref().map(Pos::from_wire);
        let first_seen = self.known.get(&p.id.0).map_or(now, |k| k.first_seen);
        let k = Known {
            wcid: p.wdesc.wcid,
            name: p.wdesc.name.clone(),
            item_type: p.wdesc.obj_type,
            pos,
            container: p.wdesc.container_id.map(|c| c.0),
            wielder: p.wdesc.wielder_id.map(|c| c.0),
            first_seen,
        };
        if Some(p.id.0) == self.me {
            if let Some(pos) = pos {
                self.pos = Some(pos);
            }
        }
        self.known.insert(p.id.0, k);
    }

    #[allow(clippy::too_many_lines)]
    fn learn(&mut self, now: f64, k: u32, m: &IncomingMessage) {
        if is::<ItemCreateObject>(k) {
            if let Some(c) = read::<ItemCreateObject>(m) {
                self.remember(now, &c.0);
            }
        } else if is::<ItemUpdateObject>(k) {
            if let Some(c) = read::<ItemUpdateObject>(m) {
                self.remember(now, &c.0);
            }
        } else if is::<ItemDeleteObject>(k) {
            if let Some(d) = read::<ItemDeleteObject>(m) {
                self.known.remove(&d.id.0);
            }
        } else if is::<ItemServerSaysRemove>(k) {
            if let Some(d) = read::<ItemServerSaysRemove>(m) {
                self.known.remove(&d.object.0);
            }
        } else if is::<MovementPositionEvent>(k) {
            if let Some(p) = read::<MovementPositionEvent>(m) {
                let pos = Pos {
                    cell: p.position.origin.objcell_id,
                    x: p.position.origin.origin.x,
                    y: p.position.origin.origin.y,
                    z: p.position.origin.origin.z,
                    rot: p.position.orientation,
                };
                if Some(p.id.0) == self.me {
                    self.pos = Some(pos);
                }
                if let Some(o) = self.known.get_mut(&p.id.0) {
                    o.pos = Some(pos);
                }
            }
        } else if is::<ItemServerSaysContainId>(k) {
            if let Some(c) = read::<ItemServerSaysContainId>(m) {
                if let Some(o) = self.known.get_mut(&c.item.0) {
                    o.container = Some(c.container.0);
                    o.wielder = None;
                    o.pos = None;
                }
            }
        } else if is::<InventoryPickupEvent>(k) {
            if let Some(p) = read::<InventoryPickupEvent>(m) {
                if let Some(o) = self.known.get_mut(&p.id.0) {
                    o.pos = None;
                }
            }
        } else if is::<ItemWearItem>(k) {
            if let Some(w) = read::<ItemWearItem>(m) {
                self.worn.insert(w.item.0);
                if let Some(o) = self.known.get_mut(&w.item.0) {
                    o.wielder = self.me;
                    o.container = None;
                }
            }
        } else if is::<CombatHandleAttackDoneEvent>(k) {
            self.attack_dones += 1;
            if let Some(e) = read::<CombatHandleAttackDoneEvent>(m).filter(|e| e.error != 0) {
                *self.stats.attack_errors.entry(e.error).or_insert(0) += 1;
            }
        } else if is::<ItemUseDone>(k) {
            if let Some(e) = read::<ItemUseDone>(m).filter(|e| e.failure_type != 0) {
                *self.stats.use_errors.entry(e.failure_type).or_insert(0) += 1;
            }
        } else if is::<CommunicationWeenieErrorWithString>(k) {
            if let Some(e) = read::<CommunicationWeenieErrorWithString>(m) {
                *self.stats.weenie_errors.entry(e.error_type).or_insert(0) += 1;
            }
        } else if is::<VictimNotificationSelf>(k) {
            self.died = true;
        } else if k == KILLER_NOTIFICATION {
            self.killed = true;
        } else if is::<EffectsPlayerTeleport>(k) {
            self.teleported = true;
        } else if is::<ItemOnViewContents>(k) {
            if let Some(v) = read::<ItemOnViewContents>(m) {
                self.view_contents =
                    Some((v.container.0, v.contents.iter().map(|c| c.iid.0).collect()));
            }
        } else if is::<TradeRegisterTrade>(k) {
            self.trade_registered = true;
        } else if is::<CommunicationWeenieError>(k) {
            if let Some(e) = read::<CommunicationWeenieError>(m) {
                *self.stats.weenie_errors.entry(e.error_type).or_insert(0) += 1;
                if e.error_type == TRADE_COMPLETE {
                    self.trade_complete = true;
                }
            }
        } else if is::<CommunicationHearSpeech>(k) {
            self.stats.speech_heard += 1;
        } else if is::<LoginCharacterSet>(k) {
            if let (State::CharacterList, Some(set)) = (self.state, read::<LoginCharacterSet>(m)) {
                if let Some(c) = set.characters.first() {
                    self.me = Some(c.gid.0);
                    self.state = State::EnterRequested { since: -1.0 };
                } else {
                    self.state = State::Creating { since: -1.0 };
                }
            }
        } else if is::<CharGenVerificationResponse>(k) {
            if let Some(r) = read::<CharGenVerificationResponse>(m) {
                if r.response_type == 1 {
                    self.me = Some(r.identity.gid.0);
                    self.state = State::EnterRequested { since: -1.0 };
                } else {
                    *self
                        .stats
                        .abandoned
                        .entry(format!("chargen: response {}", r.response_type))
                        .or_insert(0) += 1;
                    self.state = State::Done;
                }
            }
        } else if is::<LoginCreatePlayer>(k) {
            self.create_player_seen = true;
        } else if is::<LoginExecuteLogOff>(k) && matches!(self.state, State::LoggingOff { .. }) {
            self.events.push(BotEvent::LoggedOff);
            self.state = if self.finish {
                State::Done
            } else {
                State::CharacterSelect {
                    until: now + self.rng.range(5.0, 30.0),
                }
            };
        }
    }

    /// Runs the bot for one of its steps (the driver calls it every 0.1 s of virtual time).
    pub fn update(&mut self, now: f64, io: &mut BotIo) {
        match self.state {
            State::Connecting | State::CharacterList | State::Done => {}
            State::Creating { since } => {
                if since < 0.0 {
                    io.message(NetQueue::Logon, &self.chargen());
                    self.state = State::Creating { since: now };
                } else if now - since > 30.0 {
                    *self
                        .stats
                        .abandoned
                        .entry("chargen: no answer".to_owned())
                        .or_insert(0) += 1;
                    self.state = State::Done;
                }
            }
            State::EnterRequested { since } => {
                if since < 0.0 {
                    io.message(NetQueue::Logon, &LoginSendEnterWorldRequest);
                    self.state = State::EnterRequested { since: now };
                } else if now - since >= 0.2 {
                    let me = self.me.expect("a character to enter");
                    self.known.clear();
                    self.create_player_seen = false;
                    self.pos = None;
                    io.message(
                        NetQueue::Logon,
                        &LoginSendEnterWorld {
                            character: ObjectId(me),
                            account: self.account.clone(),
                        },
                    );
                    self.events.push(BotEvent::Entering);
                    self.state = State::Entering { since: now };
                }
            }
            State::Entering { since } => {
                if self.create_player_seen && self.pos.is_some() && now - since >= 0.5 {
                    io.action(&CharacterLoginCompleteNotification);
                    self.stats.logins += 1;
                    if self.home.is_none() {
                        self.home = self.pos;
                    }
                    self.died = false;
                    self.teleported = false;
                    self.events.push(BotEvent::Entered);
                    self.state = State::InWorld;
                    self.idle(now);
                } else if now - since > 60.0 {
                    *self
                        .stats
                        .abandoned
                        .entry("enter world: no CreatePlayer".to_owned())
                        .or_insert(0) += 1;
                    self.state = State::Done;
                }
            }
            State::InWorld => {
                self.play(now, io);
                if self.trace {
                    let t = format!("{:?}", self.activity);
                    let t = t.split(", since").next().unwrap_or(&t).to_owned();
                    if t != self.last_traced {
                        eprintln!("bot {} t={now:.1}: {t}", self.index);
                        self.last_traced = t;
                    }
                }
            }
            State::LoggingOff { since } => {
                if now - since > 60.0 {
                    *self
                        .stats
                        .abandoned
                        .entry("log off: no answer".to_owned())
                        .or_insert(0) += 1;
                    self.state = State::Done;
                }
            }
            State::CharacterSelect { until } => {
                if self.finish {
                    self.state = State::Done;
                } else if now >= until {
                    self.state = State::EnterRequested { since: -1.0 };
                }
            }
        }
        self.stats.sent_total += io.out.len() as u64;
    }

    fn log_off(&mut self, now: f64, io: &mut BotIo) {
        let me = self.me.expect("in the world");
        io.message(
            NetQueue::Logon,
            &LoginExecuteLogOffRequest {
                character: ObjectId(me),
            },
        );
        self.events.push(BotEvent::LoggingOff);
        self.state = State::LoggingOff { since: now };
    }

    fn play(&mut self, now: f64, io: &mut BotIo) {
        // death interrupts any behaviour
        if self.died && !matches!(self.activity, Activity::Dead { .. }) {
            self.stats.deaths += 1;
            if !matches!(self.activity, Activity::Idle { .. }) {
                self.abandon("died", now);
            }
            self.teleported = false;
            self.activity = Activity::Dead {
                since: now,
                teleported: None,
            };
        }
        match self.activity.clone() {
            Activity::Idle { until } => {
                if self.finish {
                    self.log_off(now, io);
                } else if self.hunter
                    && self.allow_fight
                    && self.hunting_ground.as_ref().is_some_and(|(cell, _)| {
                        self.pos.is_some_and(|p| p.cell >> 16 != cell >> 16)
                    })
                {
                    // to the hunting ground (again, after a death sent it to the lifestone)
                    self.start(&Kind::Travel);
                    self.teleported = false;
                    io.admin = self.hunting_ground.as_ref().map(|(_, line)| line.clone());
                    self.activity = Activity::Travel {
                        since: now,
                        teleported: None,
                    };
                } else if let Some((partner, role)) = self.trade_with {
                    self.start(&Kind::Trade);
                    self.trade_registered = false;
                    self.trade_complete = false;
                    if role == TradeRole::Initiator {
                        io.action(&TradeOpenTradeNegotiations {
                            partner: ObjectId(partner),
                        });
                    }
                    self.activity = Activity::Trade {
                        partner,
                        role,
                        phase: TradePhase::Open,
                        since: now,
                    };
                } else if now >= until {
                    self.choose(now, io);
                }
            }
            Activity::Wander { target, stopped } => self.wander(now, io, target, stopped),
            Activity::Loot { item, phase, since } => self.loot(now, io, item, phase, since),
            Activity::Fight { phase, since } => self.fight(now, io, phase, since),
            Activity::Trade {
                partner,
                role,
                phase,
                since,
            } => self.trade(now, io, partner, role, phase, since),
            Activity::Travel { since, teleported } => {
                if let Some(t) = teleported {
                    if now - t >= 1.0 {
                        // out of portal space, as a client does after a teleport
                        io.action(&CharacterLoginCompleteNotification);
                        self.home = self.pos;
                        self.complete("travel", now);
                    }
                } else if self.teleported {
                    self.activity = Activity::Travel {
                        since,
                        teleported: Some(now),
                    };
                } else if now - since > 20.0 {
                    self.abandon("no teleport", now);
                }
            }
            Activity::Dead { since, teleported } => {
                if let Some(t) = teleported {
                    if now - t >= 1.0 {
                        // the client leaves portal space
                        io.action(&CharacterLoginCompleteNotification);
                        self.died = false;
                        self.idle(now);
                    }
                } else if self.teleported {
                    self.activity = Activity::Dead {
                        since,
                        teleported: Some(now),
                    };
                } else if now - since > 60.0 {
                    self.died = false;
                    self.abandon("no teleport after death", now);
                }
            }
        }
    }

    /// Picks the next behaviour.
    fn choose(&mut self, now: f64, io: &mut BotIo) {
        // hunters fight often (their drudges are a generator's, so their number is bounded); the
        // Academy bots now and then (each `@create`d drudge that outlives its fight stays)
        let fight = match (self.allow_fight, self.hunter) {
            (false, _) => 0.0,
            (true, true) => 4.0,
            (true, false) => 0.3,
        };
        let weights: [(Kind, f64); 6] = [
            (Kind::Wander, 4.0),
            (Kind::Chat, 2.0),
            (Kind::Loot, if self.hunter { 0.5 } else { 2.0 }),
            (Kind::Fight, fight),
            (Kind::Relog, if self.allow_relog { 0.15 } else { 0.0 }),
            (Kind::Idle, 2.0),
        ];
        let total: f64 = weights.iter().map(|w| w.1).sum();
        let mut roll = self.rng.range(0.0, total);
        let mut pick = Kind::Idle;
        for (k, w) in weights {
            if roll < w {
                pick = k;
                break;
            }
            roll -= w;
        }
        if let Some(only) = &self.only {
            pick = only.clone();
        }
        self.start(&pick);
        match pick {
            Kind::Idle => self.complete("idle", now),
            Kind::Chat => {
                let line = format!("soak {} says {}", self.index, self.rng.next_u64() % 1000);
                io.action(&CommunicationTalk { message: line });
                self.complete("chat", now);
            }
            Kind::Wander => {
                let (Some(home), Some(_)) = (self.home, self.pos) else {
                    return self.skip("wander", "no position", now);
                };
                let mut target = home;
                #[allow(clippy::cast_possible_truncation)]
                {
                    target.x += self.rng.range(-2.5, 2.5) as f32;
                    target.y += self.rng.range(-2.5, 2.5) as f32;
                }
                let at = self.pos.expect("a position");
                io.action(&move_to_state(
                    &at,
                    RawMotionState {
                        current_holdkey: Some(HOLD_RUN),
                        forward_command: Some(WALK_FORWARD),
                        forward_holdkey: Some(HOLD_RUN),
                        ..RawMotionState::default()
                    },
                ));
                self.activity = Activity::Wander {
                    target,
                    stopped: false,
                };
            }
            Kind::Loot => {
                let Some(at) = self.pos else {
                    return self.skip("loot", "no position", now);
                };
                // the nearest food lying in the world (the Academy's generators keep spawning it)
                let food = self
                    .known
                    .iter()
                    .filter(|(_, o)| {
                        o.item_type == ITEM_TYPE_FOOD
                            && o.container.is_none()
                            && o.wielder.is_none()
                    })
                    .filter_map(|(g, o)| o.pos.map(|p| (*g, p.dist(&at))))
                    .filter(|(_, d)| *d < 20.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                let Some((item, _)) = food else {
                    return self.skip("loot", "no food in sight", now);
                };
                io.action(&InventoryPutItemInContainer {
                    item: ObjectId(item),
                    container: ObjectId(self.me.expect("in the world")),
                    slot: 0,
                });
                self.activity = Activity::Loot {
                    item,
                    phase: LootPhase::Picking,
                    since: now,
                };
            }
            Kind::Fight => {
                let dirk = self
                    .known
                    .iter()
                    .find(|(_, o)| {
                        o.wcid == TRAINING_DIRK && (o.container == self.me || o.wielder == self.me)
                    })
                    .map(|(g, _)| *g);
                let after_wield = if self.hunter {
                    FightPhase::Hunt
                } else {
                    FightPhase::GoHome
                };
                let phase = match dirk {
                    Some(d) if !self.worn.contains(&d) => {
                        io.action(&InventoryGetAndWieldItem {
                            item: ObjectId(d),
                            slot: MELEE_WEAPON,
                        });
                        FightPhase::Wield { dirk: d }
                    }
                    _ => after_wield,
                };
                self.activity = Activity::Fight { phase, since: now };
            }
            Kind::Relog => self.log_off(now, io),
            Kind::Trade | Kind::Travel => {}
        }
    }

    /// Steps toward `target` (0.5 m per 0.1 s, the cell kept) with an AutonomousPosition; true once
    /// within `within` metres.
    fn step_toward(&mut self, io: &mut BotIo, target: &Pos, within: f32) -> bool {
        let Some(mut at) = self.pos else { return false };
        let (dx, dy) = (target.x - at.x, target.y - at.y);
        let d = empyrean_common::math::hypotf(dx, dy);
        if d <= within + 0.01 {
            return true;
        }
        let step = (d - within).min(0.5);
        at.x += dx / d * step;
        at.y += dy / d * step;
        if at.cell & 0xFFFF < 0x100 {
            // outdoors the cell is the 24 m square under the spot
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (cx, cy) = (
                ((at.x / 24.0).floor().clamp(0.0, 7.0)) as u32,
                ((at.y / 24.0).floor().clamp(0.0, 7.0)) as u32,
            );
            at.cell = (at.cell & 0xFFFF_0000) | (cx * 8 + cy + 1);
            // on the ground, as the client's own physics would keep it
            if let Some(t) = self
                .terrain
                .as_ref()
                .filter(|t| t.landblock == at.cell >> 16)
            {
                at.z = t.height(at.x, at.y);
            }
        }
        io.action(&MovementAutonomousPosition(AutonomousPosition {
            position: at.wire(),
            timestamps: MoveTimestamps::default(),
            contact: 1,
        }));
        self.pos = Some(at);
        false
    }

    fn wander(&mut self, now: f64, io: &mut BotIo, target: Pos, stopped: bool) {
        if stopped {
            return self.complete("wander", now);
        }
        if self.step_toward(io, &target, 0.0) {
            let at = self.pos.expect("a position");
            io.action(&move_to_state(
                &at,
                RawMotionState {
                    current_holdkey: Some(HOLD_NONE),
                    ..RawMotionState::default()
                },
            ));
            self.activity = Activity::Wander {
                target,
                stopped: true,
            };
        }
    }

    fn loot(&mut self, now: f64, io: &mut BotIo, item: u32, phase: LootPhase, since: f64) {
        let me = self.me.expect("in the world");
        match phase {
            LootPhase::Picking => {
                let o = self.known.get(&item).cloned();
                match o {
                    None => self.abandon("food gone", now),
                    Some(o) if o.container == Some(me) => {
                        // eat it once the pickup is over (ACE's pickup keeps the player busy for its
                        // animation), and once more 4 s later if that was refused
                        self.stats.items_looted += 1;
                        self.activity = Activity::Loot {
                            item,
                            phase: LootPhase::Eating,
                            since: now,
                        };
                    }
                    Some(o) if o.container.is_some() => {
                        self.abandon("someone else took the food", now)
                    }
                    Some(o) => {
                        if now - since > 10.0 {
                            return self.abandon("pickup timed out", now);
                        }
                        if let Some(p) = o.pos {
                            let _ = self.step_toward(io, &p, 0.6);
                        }
                    }
                }
            }
            LootPhase::Eating => {
                let waited = now - since;
                if !self.known.contains_key(&item) {
                    self.stats.food_eaten += 1;
                    self.complete("loot", now);
                } else if waited > 12.0 {
                    self.abandon("eating timed out", now);
                } else if (2.0..2.1).contains(&waited) || (6.0..6.1).contains(&waited) {
                    io.action(&InventoryUseEvent {
                        object: ObjectId(item),
                    });
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn fight(&mut self, now: f64, io: &mut BotIo, phase: FightPhase, since: f64) {
        let me = self.me.expect("in the world");
        let set =
            |s: &mut Self, phase: FightPhase| s.activity = Activity::Fight { phase, since: now };
        match phase {
            FightPhase::Wield { dirk } => {
                if self.worn.contains(&dirk) {
                    set(
                        self,
                        if self.hunter {
                            FightPhase::Hunt
                        } else {
                            FightPhase::GoHome
                        },
                    );
                } else if now - since > 5.0 {
                    self.abandon("wield timed out", now);
                }
            }
            FightPhase::Hunt => {
                let at = self.pos.expect("a position");
                let prey = self
                    .known
                    .iter()
                    .filter(|(_, o)| {
                        o.name.starts_with("Drudge")
                            && o.pos.is_some_and(|p| {
                                p.cell >> 16 == at.cell >> 16 && p.dist(&at) < 80.0
                            })
                    })
                    .min_by(|a, b| {
                        a.1.pos
                            .map_or(f32::MAX, |p| p.dist(&at))
                            .total_cmp(&b.1.pos.map_or(f32::MAX, |p| p.dist(&at)))
                            .then(a.0.cmp(b.0))
                    })
                    .map(|(g, _)| *g);
                let Some(drudge) = prey else {
                    if self.trace {
                        let seen: Vec<_> = self
                            .known
                            .values()
                            .filter(|o| o.name.starts_with("Drudge"))
                            .map(|o| (o.name.clone(), o.pos.map(|p| (p.cell, p.dist(&at)))))
                            .collect();
                        eprintln!(
                            "bot {} at {at:?} sees no prey; drudges known: {seen:?}",
                            self.index
                        );
                    }
                    return self.abandon("no drudge in sight", now);
                };
                io.action(&CombatChangeCombatMode { combat_mode: MELEE });
                self.killed = false;
                set(
                    self,
                    FightPhase::Engage {
                        drudge,
                        created: now,
                    },
                );
            }
            FightPhase::GoHome => {
                // (an @create from elsewhere can land in another indoor cell)
                let Some(home) = self.home else {
                    return self.abandon("no home", now);
                };
                if self.step_toward(io, &home, 0.0) {
                    io.admin = Some("@create drudgeskulker".to_owned());
                    set(self, FightPhase::Create { at: now });
                } else if now - since > 30.0 {
                    self.abandon("could not walk home", now);
                }
            }
            FightPhase::Create { at: created } => {
                let at = self.pos.expect("a position");
                let new = self
                    .known
                    .iter()
                    .filter(|(_, o)| {
                        o.wcid == DRUDGE_SKULKER
                            && o.first_seen >= created
                            && o.pos.is_some_and(|p| p.dist(&at) < 5.0)
                    })
                    .map(|(g, _)| *g)
                    .min();
                if let Some(drudge) = new {
                    io.action(&CombatChangeCombatMode { combat_mode: MELEE });
                    self.killed = false;
                    set(self, FightPhase::Engage { drudge, created });
                } else if now - since > 3.0 {
                    self.abandon("@create made no drudge", now);
                }
            }
            FightPhase::Engage { drudge, created } => {
                if now - since >= 1.0 {
                    self.activity = Activity::Fight {
                        phase: FightPhase::Attack {
                            drudge,
                            created,
                            dones: self.attack_dones,
                            attacked: false,
                        },
                        since: now,
                    };
                }
            }
            FightPhase::Attack {
                drudge,
                created,
                dones,
                attacked,
            } => {
                let d = self.known.get(&drudge).cloned();
                let last = d
                    .as_ref()
                    .and_then(|d| d.pos)
                    .or(self.pos)
                    .expect("a position");
                // the kill, as the wire tells it: the killer's notification (ACE's KillerNotification,
                // 0x01AD), the drudge's corpse appearing where it stood, or the drudge deleted (which
                // does not happen while `Player.RemoveTrackedObject` is not ported)
                let corpse_seen = self.known.values().any(|o| {
                    o.name == DRUDGE_CORPSE
                        && o.first_seen >= created
                        && o.pos.is_some_and(|p| p.dist(&last) < 4.0)
                });
                if self.killed || corpse_seen || d.is_none() {
                    self.stats.kills += 1;
                    // the client is never told it is gone (`RemoveTrackedObject` is not ported), so
                    // forget it here
                    self.known.remove(&drudge);
                    return set(self, FightPhase::FindCorpse { at: last, created });
                }
                if now - since > 120.0 {
                    io.action(&CombatChangeCombatMode {
                        combat_mode: NON_COMBAT,
                    });
                    return self.abandon("the fight lasted two minutes", now);
                }
                if let Some(p) = d.and_then(|d| d.pos) {
                    let _ = self.step_toward(io, &p, 1.0);
                }
                // no auto-repeat: attack again after each AttackDone
                if !attacked || self.attack_dones > dones {
                    io.action(&CombatTargetedMeleeAttack {
                        target: ObjectId(drudge),
                        attack_height: 2,
                        power_level: 0.5,
                    });
                }
                self.activity = Activity::Fight {
                    phase: FightPhase::Attack {
                        drudge,
                        created,
                        dones: self.attack_dones,
                        attacked: true,
                    },
                    since,
                };
            }
            FightPhase::FindCorpse { at, created } => {
                let corpse = self
                    .known
                    .iter()
                    .filter(|(_, o)| {
                        o.name == DRUDGE_CORPSE
                            && o.first_seen >= created
                            && o.pos.is_some_and(|p| p.dist(&at) < 6.0)
                    })
                    .map(|(g, _)| *g)
                    .min();
                if let Some(corpse) = corpse {
                    self.view_contents = None;
                    io.action(&InventoryUseEvent {
                        object: ObjectId(corpse),
                    });
                    set(self, FightPhase::OpenCorpse { corpse });
                } else if now - since > 5.0 {
                    io.action(&CombatChangeCombatMode {
                        combat_mode: NON_COMBAT,
                    });
                    self.abandon("no corpse", now);
                }
            }
            FightPhase::OpenCorpse { corpse } => {
                if let Some((c, items)) = self.view_contents.clone() {
                    if c == corpse {
                        if let Some(&item) = items.first() {
                            io.action(&InventoryPutItemInContainer {
                                item: ObjectId(item),
                                container: ObjectId(me),
                                slot: 0,
                            });
                            return set(self, FightPhase::Take { corpse, item });
                        }
                        io.action(&CombatChangeCombatMode {
                            combat_mode: NON_COMBAT,
                        });
                        return self.complete("fight", now);
                    }
                }
                if now - since > 10.0 {
                    io.action(&CombatChangeCombatMode {
                        combat_mode: NON_COMBAT,
                    });
                    return self.abandon("corpse never opened", now);
                }
                if let Some(p) = self.known.get(&corpse).and_then(|o| o.pos) {
                    let _ = self.step_toward(io, &p, 1.0);
                }
            }
            FightPhase::Take { corpse, item } => {
                if self
                    .known
                    .get(&item)
                    .is_some_and(|o| o.container == Some(me))
                {
                    self.stats.items_looted += 1;
                    io.action(&CombatChangeCombatMode {
                        combat_mode: NON_COMBAT,
                    });
                    return self.complete("fight", now);
                }
                if now - since > 10.0 {
                    io.action(&CombatChangeCombatMode {
                        combat_mode: NON_COMBAT,
                    });
                    return self.abandon("corpse item never taken", now);
                }
                if let Some(p) = self.known.get(&corpse).and_then(|o| o.pos) {
                    let _ = self.step_toward(io, &p, 0.3);
                }
            }
        }
    }

    fn trade(
        &mut self,
        now: f64,
        io: &mut BotIo,
        partner: u32,
        role: TradeRole,
        phase: TradePhase,
        since: f64,
    ) {
        let end = |s: &mut Self, io: &mut BotIo, ok: bool, why: &str| {
            io.action(&TradeCloseTradeNegotiations);
            s.trade_with = None;
            if ok {
                s.stats.trades_completed += 1;
                s.complete("trade", now);
            } else {
                s.abandon(why, now);
            }
        };
        match phase {
            TradePhase::Open => {
                if self.trade_registered {
                    // the initiator offers one thing from its pack (not its dirk), if it has one
                    if role == TradeRole::Initiator {
                        let me = self.me;
                        let offer = self
                            .known
                            .iter()
                            .filter(|(_, o)| o.container == me && o.wcid != TRADE_KEEP_DIRK)
                            .map(|(g, _)| *g)
                            .min();
                        if let Some(item) = offer {
                            io.action(&TradeAddToTrade {
                                item: ObjectId(item),
                                slot: 0,
                            });
                        }
                    }
                    self.activity = Activity::Trade {
                        partner,
                        role,
                        phase: TradePhase::Offered { at: now },
                        since,
                    };
                } else if now - since > 20.0 {
                    end(self, io, false, "never registered");
                } else if role == TradeRole::Initiator {
                    // the server's MoveTo toward the partner: walk there, as a client does
                    if let Some(p) = self.known.get(&partner).and_then(|o| o.pos) {
                        let _ = self.step_toward(io, &p, 1.0);
                    }
                }
            }
            TradePhase::Offered { at } => {
                if now - at >= 1.0 {
                    io.action(&TradeAcceptTradeRequest(Trade::default()));
                    self.activity = Activity::Trade {
                        partner,
                        role,
                        phase: TradePhase::Accepted,
                        since,
                    };
                }
            }
            TradePhase::Accepted => {
                if self.trade_complete {
                    end(self, io, true, "");
                } else if now - since > 30.0 {
                    end(self, io, false, "never completed");
                }
            }
        }
    }

    /// An Aluvian male Soldier (template 6) with Heavy Weapons, Melee Defense, Healing, Jump and Run
    /// trained, at the bot's start area: the playable-loop scenario's character.
    fn chargen(&self) -> CharacterSendCharGenResult {
        let mut sacs = vec![0; 55];
        for skill in [44usize, 6, 21, 22, 24] {
            sacs[skill] = 2;
        }
        let mut result = CharGenResult {
            version: 1,
            heritage_group: 1,
            gender: 1,
            headgear_color: 3,
            shirt_color: 4,
            trousers_color: 5,
            footwear_color: 6,
            skin_shade: 1.0,
            hair_shade: 0.5,
            headgear_shade: 0.25,
            shirt_shade: 0.125,
            trousers_shade: 0.375,
            footwear_shade: 0.625,
            template_num: 6,
            strength: 100,
            endurance: 60,
            coordination: 100,
            quickness: 50,
            focus: 10,
            self_: 10,
            class_id: 1,
            skill_advancement_classes: sacs,
            name: self.name.clone(),
            start_area: self.start_area,
            ..CharGenResult::default()
        };
        result.checksum_value = result.checksum();
        CharacterSendCharGenResult {
            account: self.account.clone(),
            result,
        }
    }

    /// How many objects the bot knows of.
    #[must_use]
    pub fn known_count(&self) -> usize {
        self.known.len()
    }

    /// Whether the bot knows of `guid` (for the driver's trade pairing: partners must see each other).
    #[must_use]
    pub fn knows(&self, guid: u32) -> bool {
        self.known.contains_key(&guid)
    }
}

/// One landblock's terrain height, sampled every metre.
#[derive(Debug)]
pub struct Terrain {
    /// The landblock (`cell >> 16`).
    pub landblock: u32,
    /// 193 x 193 heights, x-major.
    pub heights: Vec<f32>,
}

impl Terrain {
    /// The height under `(x, y)`, interpolated between the samples.
    #[must_use]
    pub fn height(&self, x: f32, y: f32) -> f32 {
        let (x, y) = (x.clamp(0.0, 191.99), y.clamp(0.0, 191.99));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (ix, iy) = (x.floor() as usize, y.floor() as usize);
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let h = |i: usize, j: usize| self.heights[i * 193 + j];
        let a = h(ix, iy) * (1.0 - fx) + h(ix + 1, iy) * fx;
        let b = h(ix, iy + 1) * (1.0 - fx) + h(ix + 1, iy + 1) * fx;
        a * (1.0 - fy) + b * fy
    }
}

/// A drudge skulker's corpse, by name.
const DRUDGE_CORPSE: &str = "Corpse of Drudge Skulker";

/// The starter dirk stays with its owner (fights need it).
const TRADE_KEEP_DIRK: u32 = TRAINING_DIRK;

fn move_to_state(at: &Pos, raw_motion_state: RawMotionState) -> MovementMoveToState {
    MovementMoveToState(MoveToStatePack {
        raw_motion_state,
        position: at.wire(),
        timestamps: MoveTimestamps::default(),
        contact: true,
        longjump_mode: false,
    })
}
