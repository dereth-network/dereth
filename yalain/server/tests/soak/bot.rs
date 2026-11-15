//! A player of the soak: the test kit's client ([`Bot`]) on a UDP socket of its own, logging in and
//! playing a few scripted behaviors, mixed by a seed.
//!
//! Each player logs in, enters the world and tells the server its scene is drawn, wields the sword
//! its recipe gave it, and from then on plays one behavior at a time, drawing the next every half
//! minute to a minute and a half by the mix's weights:
//!
//! - **idle:** it stands about, reporting a place a step from its own every second, as a client
//!   standing does;
//! - **wander:** it walks to points within 12 m of where it was made, a report every half second;
//! - **chat:** it stands about and says a line every 8 to 15 seconds;
//! - **fight:** it starts attacking the nearest creature within 30 m and walks up to it; when the
//!   creature dies it walks to the corpse, opens it and takes everything out;
//! - **portal:** it walks to the nearest portal within 80 m of it in its landblock and uses it (a
//!   trip through portal space), and goes on from where it lands; with none near it stands about.
//!
//! What the server tells it of its own place (a report not taken, a trip's end) it takes, and
//! reports from there with the stamps the server gave.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

use auberean_primitives::Vec3;
use auberean_protocol::login::{DataHeld, Empty, PlayCharacter};
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::text::NarrowString;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use dereth_primitives::num::math;
use yalain_net::LocalTime;
use yalain_testkit::bot::Bot;
use yalain_testkit::play::{server_call, use_of};
use yalain_world::graph::{tag, Builder, Slot};
use yalain_world::session::function::SHOW_TEXT;
use yalain_world::world::{motion, IDLE_MOTION};

use crate::server::{Place, SWORD};

/// A player's behaviors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Idle,
    Wander,
    Chat,
    Fight,
    Portal,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Idle,
        Kind::Wander,
        Kind::Chat,
        Kind::Fight,
        Kind::Portal,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Idle => "idle",
            Kind::Wander => "wander",
            Kind::Chat => "chat",
            Kind::Fight => "fight",
            Kind::Portal => "portal",
        }
    }
}

/// The behaviors' weights, by name: `idle=1,wander=3,chat=1,fight=4,portal=1` (the default).
#[derive(Debug, Clone, PartialEq)]
pub struct Mix(pub Vec<(Kind, u32)>);

impl Mix {
    /// Read a mix; a behavior it does not name has no weight.
    ///
    /// # Panics
    /// A part that is not `name=weight` with a known name.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let mut out = Vec::new();
        for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (name, weight) = part.split_once('=').expect("name=weight");
            let kind = Kind::ALL
                .into_iter()
                .find(|k| k.name() == name.trim())
                .unwrap_or_else(|| panic!("no behavior {name}"));
            out.push((kind, weight.trim().parse().expect("a weight")));
        }
        assert!(out.iter().any(|(_, w)| *w > 0), "a mix of nothing");
        Self(out)
    }

    fn draw(&self, rng: &mut Rng) -> Kind {
        let total: u32 = self.0.iter().map(|(_, w)| w).sum();
        let mut n = rng.below(u64::from(total));
        for (k, w) in &self.0 {
            if n < u64::from(*w) {
                return *k;
            }
            n -= u64::from(*w);
        }
        self.0[0].0
    }

    /// The mix as it reads.
    #[must_use]
    pub fn text(&self) -> String {
        self.0
            .iter()
            .map(|(k, w)| format!("{}={w}", k.name()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// A small generator of the player's own (splitmix64).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    /// A number from `lo` to `hi`.
    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * unit
    }
}

/// Where a player is in its login.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Waiting,
    LoggingIn,
    Listing,
    Entering,
    InWorld,
}

/// What the player saw of an object.
#[derive(Debug, Clone, Copy)]
struct Seen {
    template: u32,
    cell: u32,
    at: [f32; 3],
}

/// What a player did, counted.
#[derive(Debug, Clone, Default)]
pub struct Counts {
    /// Messages on its world connection, all of them and by message type.
    pub world_messages: u64,
    pub by_type: BTreeMap<u32, u64>,
    /// Time in the world under each behavior, in seconds.
    pub time_in: BTreeMap<Kind, f64>,
    pub reports: u64,
    pub told_back: u64,
    pub said: u64,
    pub attacks: u64,
    pub kills_seen: u64,
    pub corpses_opened: u64,
    pub takes: u64,
    pub portals_used: u64,
    pub no_portal: u64,
    pub trips: u64,
}

/// The request to take everything out of a container.
fn take_all(container: u64, me: u64) -> Arg {
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| {
        [
            Slot::Tagged(tag::LONG, v as u32),
            Slot::Tagged(tag::HIGH, (v >> 32) as u32),
        ]
    };
    let mut slots = vec![
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(1),
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(0),
    ];
    slots.extend(long(container));
    slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
    slots.extend(long(me));
    let mut g = Builder::new();
    g.object(yalain_world::pack::TAKE_ALL_DESC, &slots);
    Arg::Object(g.graph())
}

/// A line said, as the client sends it.
fn literal(text: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        106,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(text.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    Arg::Object(b.graph())
}

/// The lines players say.
const LINES: [&str; 5] = [
    "Well met.",
    "Anyone seen the drudges about?",
    "Heading to the lifestone.",
    "That was a good fight.",
    "Looking for a fellowship.",
];

/// How far a player goes to a portal, in meters.
const PORTAL_REACH: f32 = 80.0;
/// The request to start attacking.
const START_ATTACK: u32 = 0x8204_0008;
/// How near a player walks to what it attacks or loots, in meters.
const NEAR: f32 = 1.4;
/// How far a player walks in half a second, in meters.
const STRIDE: f32 = 3.0;

/// A player.
pub struct Player {
    pub bot: Bot,
    socket: UdpSocket,
    pub account: String,
    pub stage: Stage,
    login_at: f64,
    pub character: u64,
    cell: u32,
    at: [f32; 3],
    /// Where it was made, which it wanders about.
    home: (u32, [f32; 3]),
    teleport_stamp: u16,
    force_stamp: u16,
    next_report: f64,
    rng: Rng,
    mix: Mix,
    pub kind: Kind,
    kind_since: f64,
    next_kind: f64,
    next_say: f64,
    goal: Option<[f32; 3]>,
    sword: Option<u64>,
    wielded: bool,
    target: Option<u64>,
    target_at: Option<(u32, [f32; 3])>,
    corpse: Option<(u64, f64)>,
    opened: BTreeSet<u64>,
    next_attack: f64,
    portal: Option<(u64, u32, [f32; 3])>,
    objects: BTreeMap<u64, Seen>,
    creatures: std::sync::Arc<BTreeSet<u32>>,
    portals: std::sync::Arc<Vec<(u64, u32, [f32; 3])>>,
    pub counts: Counts,
}

impl Player {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        i: usize,
        server: SocketAddr,
        place: Place,
        login_at: f64,
        seed: u64,
        mix: Mix,
        creatures: std::sync::Arc<BTreeSet<u32>>,
        portals: std::sync::Arc<Vec<(u64, u32, [f32; 3])>>,
    ) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a socket");
        socket.set_nonblocking(true).expect("non-blocking");
        let addr = socket.local_addr().expect("its address");
        Self {
            bot: Bot::new(addr, server),
            socket,
            account: format!("soak{i}"),
            stage: Stage::Waiting,
            login_at,
            character: 0,
            cell: place.cell,
            at: place.at,
            home: (place.cell, place.at),
            teleport_stamp: 0,
            force_stamp: 0,
            next_report: 0.0,
            rng: Rng::new(seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
            mix,
            kind: Kind::Idle,
            kind_since: 0.0,
            next_kind: 0.0,
            next_say: 0.0,
            goal: None,
            sword: None,
            wielded: false,
            target: None,
            target_at: None,
            corpse: None,
            opened: BTreeSet::new(),
            next_attack: 0.0,
            portal: None,
            objects: BTreeMap::new(),
            creatures,
            portals,
            counts: Counts::default(),
        }
    }

    /// Take what arrived, answer what the login asks, and act.
    pub fn turn(&mut self, now: LocalTime) {
        let mut buf = [0u8; 4096];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            self.bot.on_datagram(from, &buf[..n], now);
        }
        let messages: Vec<(bool, ServerMessage)> = self
            .bot
            .inbox
            .drain(..)
            .filter_map(|r| r.message().map(|m| (r.world, m)))
            .collect();
        for (world, m) in messages {
            if world {
                self.counts.world_messages += 1;
                *self.counts.by_type.entry(m.message_type()).or_default() += 1;
            }
            self.heard(world, &m, now);
        }
        match self.stage {
            Stage::Waiting if now.0 >= self.login_at => {
                self.bot.log_in(&self.account, b"soak", now);
                self.stage = Stage::LoggingIn;
            }
            Stage::InWorld => self.play(now.0),
            _ => {}
        }
        for (to, bytes) in self.bot.tick(now) {
            let _ = self.socket.send_to(&bytes, to);
        }
    }

    fn send(&mut self, m: &ClientMessage) {
        self.bot.send(true, queue::WEENIE, m);
    }

    fn report(&mut self, moving: bool) {
        self.counts.reports += 1;
        let m = ClientMessage::Position(ClientPosition {
            time: 0.0,
            cell: self.cell,
            origin: Vec3::new(self.at[0], self.at[1], self.at[2]),
            heading_motion: if moving {
                motion(127, 255, 127)
            } else {
                IDLE_MOTION
            },
            flags: 4,
            instance_stamp: 1,
            teleport_stamp: self.teleport_stamp,
            force_position_stamp: self.force_stamp,
            move_to_stamp: 0,
            jump_velocity: Vec3::ZERO,
        });
        self.send(&m);
    }

    /// One step toward a point in the player's own cell's landblock frame; whether it is there.
    fn step_to(&mut self, goal: [f32; 3], near: f32) -> bool {
        let (dx, dy) = (goal[0] - self.at[0], goal[1] - self.at[1]);
        let d = (dx * dx + dy * dy).sqrt();
        if d <= near {
            return true;
        }
        let go = (d - near).min(STRIDE) / d;
        self.at = [self.at[0] + dx * go, self.at[1] + dy * go, goal[2]];
        false
    }

    fn play(&mut self, now: f64) {
        if !self.wielded {
            if let Some(sword) = self.sword {
                self.wielded = true;
                let me = self.character;
                self.send(&server_call(
                    yalain_world::requests::function::USE,
                    &[use_of(sword, me)],
                ));
            }
        }
        if now >= self.next_kind {
            *self.counts.time_in.entry(self.kind).or_default() += now - self.kind_since;
            self.kind = self.mix.draw(&mut self.rng);
            self.kind_since = now;
            self.next_kind = now + self.rng.between(30.0, 90.0);
            self.goal = None;
            self.target = None;
            self.corpse = None;
            self.portal = None;
            if self.kind == Kind::Portal {
                let here = (self.cell, self.at);
                self.portal = self
                    .portals
                    .iter()
                    .filter(|(_, cell, _)| cell >> 16 == here.0 >> 16)
                    .map(|p| (*p, flat(p.2, here.1)))
                    .filter(|(_, d)| *d < PORTAL_REACH)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(p, _)| p);
                if self.portal.is_none() {
                    self.counts.no_portal += 1;
                }
            }
        }
        if now < self.next_report {
            return;
        }
        match self.kind {
            Kind::Portal if self.portal.is_some() => {
                self.next_report = now + 0.5;
                if let Some((id, _, at)) = self.portal {
                    let there = self.step_to(at, NEAR);
                    self.report(!there);
                    if there {
                        self.portal = None;
                        self.counts.portals_used += 1;
                        let me = self.character;
                        self.send(&server_call(
                            yalain_world::requests::function::USE,
                            &[use_of(id, me)],
                        ));
                    }
                }
            }
            Kind::Idle | Kind::Chat | Kind::Portal => {
                self.next_report = now + 1.0;
                if self.kind == Kind::Chat && now >= self.next_say {
                    self.next_say = now + self.rng.between(8.0, 15.0);
                    self.counts.said += 1;
                    #[allow(clippy::cast_possible_truncation)]
                    let line = LINES[self.rng.below(LINES.len() as u64) as usize];
                    self.send(&server_call(
                        yalain_world::requests::function::SAY,
                        &[Arg::Int(0), literal(line)],
                    ));
                }
                let side = if self.counts.reports.is_multiple_of(2) {
                    0.5
                } else {
                    -0.5
                };
                self.at[0] += side;
                self.report(false);
            }
            Kind::Wander => {
                self.next_report = now + 0.5;
                let goal = *self.goal.get_or_insert_with(|| {
                    let (_, h) = self.home;
                    #[allow(clippy::cast_possible_truncation)]
                    let (a, r) = (
                        self.rng.between(0.0, std::f64::consts::TAU) as f32,
                        self.rng.between(2.0, 12.0) as f32,
                    );
                    [h[0] + r * math::cosf(a), h[1] + r * math::sinf(a), h[2]]
                });
                if self.step_to(goal, 0.5) {
                    self.goal = None;
                }
                self.report(true);
            }
            Kind::Fight => {
                self.next_report = now + 0.5;
                self.fight(now);
            }
        }
    }

    fn fight(&mut self, now: f64) {
        let me = self.character;
        // A corpse to loot first.
        if let Some((corpse, since)) = self.corpse {
            match self.objects.get(&corpse).copied() {
                Some(c) if c.cell >> 16 == self.cell >> 16 => {
                    let there = self.step_to(c.at, NEAR);
                    self.report(!there);
                    if there && !self.opened.contains(&corpse) {
                        self.opened.insert(corpse);
                        self.counts.corpses_opened += 1;
                        self.send(&server_call(
                            yalain_world::requests::function::USE,
                            &[use_of(corpse, me)],
                        ));
                        self.corpse = Some((corpse, now));
                    } else if there && now - since >= 2.0 {
                        self.counts.takes += 1;
                        self.send(&server_call(
                            yalain_world::pack::function::TAKE_ALL,
                            &[take_all(corpse, me)],
                        ));
                        self.corpse = None;
                    }
                }
                _ => self.corpse = None,
            }
            return;
        }
        // The target, or the nearest creature within 30 m to take on.
        if self.target.is_none_or(|t| !self.objects.contains_key(&t)) {
            if let Some(t) = self.target.take() {
                self.counts.kills_seen += 1;
                // Its corpse, where it fell.
                if let Some((cell, at)) = self.target_at {
                    let corpse = self
                        .objects
                        .iter()
                        .filter(|(id, o)| {
                            o.template == yalain_world::death::CORPSE_TEMPLATE
                                && o.cell >> 16 == cell >> 16
                                && !self.opened.contains(id)
                        })
                        .map(|(id, o)| (*id, flat(o.at, at)))
                        .filter(|(_, d)| *d < 6.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((c, _)) = corpse {
                        self.corpse = Some((c, now));
                    }
                }
                let _ = t;
            }
            let here = (self.cell, self.at);
            self.target = self
                .objects
                .iter()
                .filter(|(_, o)| {
                    self.creatures.contains(&o.template) && o.cell >> 16 == here.0 >> 16
                })
                .map(|(id, o)| (*id, flat(o.at, here.1)))
                .filter(|(_, d)| *d < 30.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id);
            if let Some(t) = self.target {
                self.counts.attacks += 1;
                self.next_attack = now + 5.0;
                self.send(&server_call(START_ATTACK, &[Arg::LongInt(t)]));
            }
        }
        match self.target.and_then(|t| self.objects.get(&t).copied()) {
            Some(o) => {
                self.target_at = Some((o.cell, o.at));
                let there = self.step_to(o.at, NEAR);
                self.report(!there);
                // An attack the server ended (the target out of reach a while) is asked again.
                if now >= self.next_attack {
                    self.next_attack = now + 5.0;
                    if let Some(t) = self.target {
                        self.send(&server_call(START_ATTACK, &[Arg::LongInt(t)]));
                    }
                }
            }
            None => {
                let side = if self.counts.reports.is_multiple_of(2) {
                    0.5
                } else {
                    -0.5
                };
                self.at[0] += side;
                self.report(false);
            }
        }
    }

    fn heard(&mut self, world: bool, m: &ServerMessage, now: LocalTime) {
        match (self.stage, m) {
            (Stage::LoggingIn, ServerMessage::DataQuery(_)) => {
                let held = ClientMessage::DataHeld(DataHeld {
                    language: 0x8000_0001,
                    files: Vec::new(),
                });
                self.bot.send(false, queue::DATABASE, &held);
                self.stage = Stage::Listing;
            }
            (Stage::Listing, ServerMessage::CharacterList(list)) => {
                let c = list.characters.first().expect("the recipe's character");
                self.character = c.id;
                let play = ClientMessage::PlayCharacter(PlayCharacter {
                    character: c.id,
                    account: NarrowString::from(self.account.as_str()),
                });
                self.bot.send(false, queue::LOGON, &play);
                self.stage = Stage::Entering;
            }
            (Stage::Entering, ServerMessage::PlayerCall(c))
                if world && c.function.0 == SHOW_TEXT =>
            {
                self.stage = Stage::InWorld;
                self.next_report = now.0 + 2.0;
                self.kind_since = now.0;
                self.next_kind = now.0 + 2.0;
                self.send(&ClientMessage::SceneRenderingComplete(Empty));
            }
            (_, ServerMessage::CreateObject(o)) => {
                if o.weenie.entity_did == SWORD && self.sword.is_none() && o.id != self.character {
                    self.sword = Some(o.id);
                }
                self.objects.insert(
                    o.id,
                    Seen {
                        template: o.weenie.entity_did,
                        cell: o.physics.position.cell,
                        at: [
                            o.physics.position.frame.origin.x,
                            o.physics.position.frame.origin.y,
                            o.physics.position.frame.origin.z,
                        ],
                    },
                );
            }
            (_, ServerMessage::Position(p) | ServerMessage::PositionCell(p)) => {
                if p.sender.id == self.character {
                    // Where the server put the player, or would not let it go: it goes on from there.
                    if p.teleport_stamp != self.teleport_stamp {
                        self.counts.trips += 1;
                        self.home = (p.cell, [p.origin.x, p.origin.y, p.origin.z]);
                    }
                    if p.force_position_stamp != self.force_stamp {
                        self.counts.told_back += 1;
                    }
                    if p.teleport_stamp != self.teleport_stamp
                        || p.force_position_stamp != self.force_stamp
                    {
                        self.teleport_stamp = p.teleport_stamp;
                        self.force_stamp = p.force_position_stamp;
                        self.cell = p.cell;
                        self.at = [p.origin.x, p.origin.y, p.origin.z];
                        self.goal = None;
                    }
                } else if let Some(o) = self.objects.get_mut(&p.sender.id) {
                    o.cell = p.cell;
                    o.at = [p.origin.x, p.origin.y, p.origin.z];
                }
            }
            (_, ServerMessage::ObjectLeaves(e)) => {
                self.objects.remove(&e.sender.id);
            }
            (_, ServerMessage::ForgetObject(x)) => {
                self.objects.remove(&x.0.id);
            }
            _ => {}
        }
    }

    /// Close the time spent under the behavior it plays now.
    pub fn close(&mut self, now: f64) {
        if self.stage == Stage::InWorld {
            *self.counts.time_in.entry(self.kind).or_default() += now - self.kind_since;
            self.kind_since = now;
        }
    }
}

/// The distance between two points across the ground.
fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}
