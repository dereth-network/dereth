//! The plugin conformance driver: a plugin host over a replayed recording or a scripted world,
//! with no data file and no device.
//!
//! The spec's conformance suite (`docs/plugins/conformance/`) is a set of test plugins, each with
//! an `expect.toml` naming its scenario and what it must produce. Any host runs them through five
//! calls, and this is this client's implementation of the five:
//!
//! | call | here |
//! |---|---|
//! | load a plugin | [`PluginRun::load`] |
//! | feed a scenario | [`PluginRun::login`] then [`PluginRun::advance`] through a recording, or [`PluginRun::scripted_world`] |
//! | advance time | [`PluginRun::advance`] (frames at 30 per second) |
//! | act on a UI node | [`PluginRun::click`], [`PluginRun::set`], [`PluginRun::type_line`] for a command, and [`PluginRun::map_click`] for a press on the game's map |
//! | collect what it emitted | [`PluginRun::console`], [`PluginRun::printed`], [`PluginRun::hidden`], [`PluginRun::ui_dump`], [`PluginRun::sent`] |
//!
//! The model is the one the `cpu` tier's scenarios use: server messages go through the real
//! session, object stream, HUD model and interaction, and the host is stepped with the same
//! [`HookFrame`] the runtime hands it. What the plugins ask for goes through the interaction step
//! as the interface's own requests do, and the messages it would send are collected
//! ([`PluginRun::sent`]).
//!
//! A recording is replayed at its own pace, so a timer and a rate read the recording's clock.
//! Nothing of a run turns on how fast the machine runs it: the time a plugin takes in its VM is
//! read from a clock on which an entry takes none (`NoVmTime`), so a plugin's events never wait
//! for a later frame nor is it suspended for its share of one, and an entry may run for
//! [`UNHURRIED_BUDGET_MS`] before its VM stops it. Only the cases about that budget itself run at
//! the host's own ([`crate::conformance::BUDGET_CASES`]).
//!
//! A **scripted world** is a character with the objects a case lists and a server that answers the
//! client's requests the way a server does ([`Scripted`]): an appraisal with the object's
//! properties, a move with the item's new place, a use with its acknowledgement (and a
//! container's contents when the object is one, or a vendor's wares), a cast with its
//! acknowledgement, a sale or a purchase at a vendor with the items and pyreals changing hands, a
//! salvage with the bags it makes, and a merge with the stacks it changes.
//!
//! In a scripted world the character and the objects have places. The character's movements are
//! carried out the way the client's body would carry them out, simplified: a turn to a heading
//! turns at [`TURN_RATE`], and while the run lock is on the character runs straight ahead at
//! [`RUN_SPEED`]. A movement control a hook holds moves it too: forward at [`RUN_SPEED`] (at
//! [`WALK_SPEED`] with the walk key), back and sideways at [`WALK_SPEED`], turns at [`TURN_RATE`];
//! while a jump charges the character stands, and when it is let go the character leaps
//! [`leap_distance`] the way the controls held say (straight up when none). A **scripted pre-game** is an account's character list; entering the world as
//! one of them enters the scripted world as that character, unless the case says the character's
//! last session is still in the world, in which case the first attempt is refused as the server
//! refuses it (`Character_CharacterError` `0x0D`). The plugins' HTTPS requests are answered from a
//! table ([`TableNet`]), and two runs can share a bus between clients ([`LocalBus`]).
//!
//! [`TableNet`]: dereth_plugin_host::TableNet
//! [`LocalBus`]: dereth_plugin_host::LocalBus

use std::collections::VecDeque;
use std::path::Path;

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_model::{Request, Weenie};
use dereth_client_net::client_session::testing::capture::{peer, Datagram};
use dereth_client_net::client_session::{SessionEvent, SessionState};
use dereth_client_runtime::frame_hook::{
    claim_line, outcomes_of, ExtensionCommand, HookFrame, NavKey, NavRequest, Outcome, Places,
    SessionFacts, SessionPhase, SessionRequest, StreamPlaces, WireMessage,
};
use dereth_client_runtime::hud::Hud;
use dereth_client_runtime::interaction::Interaction;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_plugin_api::{HostCapability, Value};
use dereth_plugin_host::{
    HostConfig, MemoryStorage, Package, PluginHost, StaticSource, UiInput, VmClock,
};
use dereth_primitives::{LocalTime, ObjectId, Position, ServerTime};
use dereth_protocol::Message;

use crate::replay::{recorded_endpoint, records};

/// Host frames per second.
pub const FRAME_RATE: f64 = 30.0;

/// How long one entry into a plugin's VM may run in a run before the VM stops it, in milliseconds:
/// far past what any entry of a test plugin's or a sample's takes on a machine however loaded. A
/// run is given it unless its configuration says otherwise, as the cases about the budget itself
/// do ([`crate::conformance::BUDGET_CASES`]).
pub const UNHURRIED_BUDGET_MS: f64 = 10_000.0;

/// The clock a run reads the plugins' time in their VMs from: an entry takes none, so a plugin's
/// share of a frame is never used up. The shares themselves are the plugin host's own tests'.
#[derive(Debug)]
struct NoVmTime;

impl VmClock for NoVmTime {
    fn now(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// How long the scripted server takes to answer, in seconds.
pub const SCRIPTED_LATENCY: f64 = 0.1;

/// How long the scripted server takes to answer an attack: the swing, in seconds.
pub const SCRIPTED_SWING: f64 = 1.0;

/// The scripted world's character's object id.
pub const SCRIPTED_PLAYER: u32 = 0x5000_0001;

/// The name of the world a run reports to the host.
pub const WORLD_NAME: &str = "Conformance";

/// How fast the scripted character runs, in metres a second.
pub const RUN_SPEED: f64 = 4.0;

/// How fast the scripted character turns, in degrees a second.
pub const TURN_RATE: f64 = 180.0;

/// How fast the scripted character walks, backs up and steps sideways, in metres a second.
pub const WALK_SPEED: f64 = 1.25;

/// How far the scripted character leaps on a jump charged `charge` seconds (at most one): a
/// metre, and three more at full charge.
#[must_use]
pub fn leap_distance(charge: f64) -> f64 {
    1.0 + 3.0 * charge.clamp(0.0, 1.0)
}

/// Where the scripted character stands when a case names no place: Holtburg's first cell.
pub const START_NS: f64 = 42.2;
pub const START_EW: f64 = 33.8;

/// The scripted world's places: the character's, with its heading and run lock, and the objects'.
#[derive(Debug, Clone, Default)]
pub struct ScriptedPlaces {
    pub player: Option<Position>,
    pub objects: std::collections::BTreeMap<u32, Position>,
    pub autorun: bool,
    /// The heading the character is turning to.
    pub turning_to: Option<f64>,
    /// The movement controls a hook holds.
    pub held: std::collections::BTreeSet<NavKey>,
    /// When the jump being charged began.
    pub charging: Option<f64>,
    /// The doors that stand closed, by id: where each stands. A closed door stops the character
    /// running into it.
    pub closed_doors: std::collections::BTreeMap<u32, Position>,
    /// The walls, each a straight stretch between two places on the ground. A wall stops the
    /// character running into it and blocks a line of sight across it.
    pub walls: Vec<(Position, Position)>,
    /// What cuts a line of sight and nothing else, each a straight stretch on the ground: the
    /// character walks across it and walking routes go straight over it, as over the edge of a
    /// ramp that hides what stands below it.
    pub screens: Vec<(Position, Position)>,
    /// The ledges, each a straight stretch on the ground and how high it stands: the character
    /// steps off one, moving across it, but no walking route takes it, and a journey that only a
    /// drop off it would carry on fails, naming the drop.
    pub ledges: Vec<(Position, Position, f64)>,
}

/// How near a closed door's middle the character comes before it stops the character, metres.
pub const DOOR_RADIUS: f64 = 1.0;

/// How near a wall the character comes before it stops the character, metres.
pub const WALL_REACH: f64 = 0.4;

/// A place's ground point in metres from the world's corner.
fn flat(p: &Position) -> (f64, f64) {
    let (x, y, _) = dereth_plugin_host::places::global(p);
    (x, y)
}

/// The distance from the point `p` to the segment `a`–`b`, on the ground.
#[must_use]
pub fn point_segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f64::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (x, y) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - x).powi(2) + (p.1 - y).powi(2)).sqrt()
}

/// Whether the segments `a`–`b` and `c`–`d` cross, on the ground.
#[must_use]
pub fn segments_cross(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let side = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// The distance between the segments `a`–`b` and `c`–`d`, on the ground: 0 where they cross.
#[must_use]
pub fn segment_distance(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> f64 {
    if segments_cross(a, b, c, d) {
        return 0.0;
    }
    point_segment_distance(a, c, d)
        .min(point_segment_distance(b, c, d))
        .min(point_segment_distance(c, a, b))
        .min(point_segment_distance(d, a, b))
}

impl Places for ScriptedPlaces {
    fn player(&self) -> Option<Position> {
        self.player
    }

    fn object(&self, id: ObjectId) -> Option<Position> {
        self.objects.get(&id.0).copied()
    }

    fn autorun(&self) -> bool {
        self.autorun
    }

    fn sight(&self, from: &Position, to: &Position) -> Option<bool> {
        let (a, b) = (flat(from), flat(to));
        let walled = self
            .walls
            .iter()
            .chain(&self.screens)
            .any(|(c, d)| segments_cross(a, b, flat(c), flat(d)));
        let door = self
            .closed_doors
            .values()
            .any(|d| point_segment_distance(flat(d), a, b) < DOOR_RADIUS);
        Some(!walled && !door)
    }
}

impl ScriptedPlaces {
    /// Whether a wall stops a move on the ground from `a` to `b` (metres from the world's corner):
    /// the move crosses it, or ends within [`WALL_REACH`] of it.
    fn walled(&self, a: (f64, f64), b: (f64, f64)) -> bool {
        self.walls.iter().any(|(c, d)| {
            let (c, d) = (flat(c), flat(d));
            segments_cross(a, b, c, d) || point_segment_distance(b, c, d) < WALL_REACH
        })
    }

    /// Whether a closed door stands at the global place `(gx, gy)` (metres), within
    /// [`DOOR_RADIUS`] of its middle.
    fn door_at(&self, gx: f64, gy: f64) -> bool {
        self.closed_doors.values().any(|d| {
            let (dx, dy, _) = dereth_plugin_host::places::global(d);
            ((dx - gx).powi(2) + (dy - gy).powi(2)).sqrt() < DOOR_RADIUS
        })
    }

    /// Move the character `d` metres toward `bearing` degrees clockwise from north, keeping its
    /// heading; a closed door in the way stops it.
    fn shift(&mut self, bearing: f64, d: f64) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let h = bearing.to_radians();
        let (gx, gy, z) = dereth_plugin_host::places::global(p);
        let from = flat(p);
        let (gx, gy) = (
            gx + dereth_primitives::num::math::sin(h) * d,
            gy + dereth_primitives::num::math::cos(h) * d,
        );
        if self.door_at(gx, gy) || self.walled(from, (gx, gy)) {
            return;
        }
        let Some(p) = self.player.as_mut() else {
            return;
        };
        let ns = gy / dereth_plugin_host::places::UNIT - dereth_plugin_host::places::ORIGIN;
        let ew = gx / dereth_plugin_host::places::UNIT - dereth_plugin_host::places::ORIGIN;
        if let Some(mut moved) = dereth_plugin_host::places::from_coords(ns, ew, z) {
            moved.frame.rotation = p.frame.rotation;
            *p = moved;
        }
    }

    /// The way the held controls move the character, as a bearing off its heading, and the pace
    /// they move it at; `None` when they move it nowhere.
    fn held_way(&self) -> Option<(f64, f64)> {
        let walk = self.held.contains(&NavKey::Walk);
        if self.held.contains(&NavKey::Forward) {
            Some((0.0, if walk { WALK_SPEED } else { RUN_SPEED }))
        } else if self.held.contains(&NavKey::Backward) {
            Some((180.0, WALK_SPEED))
        } else if self.held.contains(&NavKey::StrafeRight) {
            Some((90.0, WALK_SPEED))
        } else if self.held.contains(&NavKey::StrafeLeft) {
            Some((270.0, WALK_SPEED))
        } else {
            None
        }
    }

    /// A control pressed or let go at `now`: the answer names a leap, when the jump was let go.
    fn hold(&mut self, key: NavKey, down: bool, now: f64) -> Option<String> {
        if key == NavKey::Jump {
            if down {
                self.charging.get_or_insert(now);
                return None;
            }
            let charge = (now - self.charging.take()?).clamp(0.0, 1.0);
            let heading = self
                .player
                .as_ref()
                .map(dereth_plugin_host::places::heading)?;
            return Some(match self.held_way() {
                Some((off, _)) => {
                    self.shift(heading + off, leap_distance(charge));
                    let way = match self.held.iter().find(|k| **k != NavKey::Walk) {
                        Some(k) => nav_key_name(*k),
                        None => "up",
                    };
                    format!("leap {charge:.1} s {way}")
                }
                None => format!("leap {charge:.1} s up"),
            });
        }
        if down {
            self.held.insert(key);
        } else {
            self.held.remove(&key);
        }
        None
    }

    /// Carry the character's movement on by `dt` seconds.
    fn walk(&mut self, dt: f64) {
        if self.charging.is_none() {
            let turn = if self.held.contains(&NavKey::TurnRight) {
                TURN_RATE
            } else if self.held.contains(&NavKey::TurnLeft) {
                -TURN_RATE
            } else {
                0.0
            };
            if let Some(p) = self.player.as_mut() {
                if turn != 0.0 {
                    let h = dereth_plugin_host::places::heading(p) + turn * dt;
                    #[allow(clippy::cast_possible_truncation)]
                    dereth_primitives::frame::set_heading(&mut p.frame, h.rem_euclid(360.0) as f32);
                }
            }
            if let Some((off, pace)) = self.held_way() {
                if let Some(h) = self
                    .player
                    .as_ref()
                    .map(dereth_plugin_host::places::heading)
                {
                    self.shift(h + off, pace * dt);
                }
            }
        }
        let Some(p) = self.player.as_mut() else {
            return;
        };
        if let Some(to) = self.turning_to {
            let h = dereth_plugin_host::places::heading(p);
            let turn = dereth_plugin_host::places::turn(h, to);
            let step = TURN_RATE * dt;
            let next = if turn.abs() <= step {
                self.turning_to = None;
                to
            } else {
                h + step.copysign(turn)
            };
            #[allow(clippy::cast_possible_truncation)]
            dereth_primitives::frame::set_heading(&mut p.frame, next.rem_euclid(360.0) as f32);
        }
        if self.autorun {
            let h = dereth_plugin_host::places::heading(p).to_radians();
            let (gx, gy, z) = dereth_plugin_host::places::global(p);
            let from = flat(p);
            let (gx, gy) = (
                gx + dereth_primitives::num::math::sin(h) * RUN_SPEED * dt,
                gy + dereth_primitives::num::math::cos(h) * RUN_SPEED * dt,
            );
            // A closed door or a wall in the way stops the run.
            let blocked = self.closed_doors.values().any(|d| {
                let (dx, dy, _) = dereth_plugin_host::places::global(d);
                ((dx - gx).powi(2) + (dy - gy).powi(2)).sqrt() < DOOR_RADIUS
            }) || self.walled(from, (gx, gy));
            if blocked {
                return;
            }
            let Some(p) = self.player.as_mut() else {
                return;
            };
            let ns = gy / dereth_plugin_host::places::UNIT - dereth_plugin_host::places::ORIGIN;
            let ew = gx / dereth_plugin_host::places::UNIT - dereth_plugin_host::places::ORIGIN;
            if let Some(mut moved) = dereth_plugin_host::places::from_coords(ns, ew, z) {
                moved.frame.rotation = p.frame.rotation;
                *p = moved;
            }
        }
    }
}

/// A movement control's name in a run's list of moves.
#[must_use]
pub const fn nav_key_name(key: NavKey) -> &'static str {
    match key {
        NavKey::Forward => "forward",
        NavKey::Backward => "backward",
        NavKey::StrafeLeft => "strafe_left",
        NavKey::StrafeRight => "strafe_right",
        NavKey::TurnLeft => "turn_left",
        NavKey::TurnRight => "turn_right",
        NavKey::Walk => "walk",
        NavKey::Jump => "jump",
    }
}

/// A scripted pre-game: the account's characters, and those whose last session is still in the
/// world (the first entry of each is refused).
#[derive(Debug, Clone, Default)]
pub struct ScriptedPregame {
    pub characters: Vec<(u32, String)>,
    pub still_in_world: Vec<String>,
    /// The scripted world's objects, entered with whichever character logs on.
    pub objects: Vec<ScriptedObject>,
}

/// One object of a scripted world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptedObject {
    pub id: u32,
    pub name: String,
    /// The item-type bits.
    pub obj_type: u32,
    /// The public description's flags (a corpse, a player, …).
    pub bitfield: u32,
    /// The container it is in (the player's id for the main pack), or none (on the ground).
    pub container: Option<u32>,
    pub value: u32,
    pub stack: Option<u16>,
    /// The icon effects (1: magical).
    pub effects: Option<u32>,
    /// What an appraisal tells: integer properties and spells.
    pub ints: Vec<(u32, i32)>,
    pub spells: Vec<u32>,
    /// ... and the other property tables an appraisal carries.
    pub int64s: Vec<(u32, i64)>,
    pub floats: Vec<(u32, f64)>,
    pub strings: Vec<(u32, String)>,
    pub bools: Vec<(u32, bool)>,
    pub dids: Vec<(u32, u32)>,
    /// A creature's health, `(current, max)`, which its appraisal tells.
    pub health: Option<(u32, u32)>,
    /// Where it stands, for an object on the ground: map coordinates `(ns, ew)`.
    pub place: Option<(f64, f64)>,
    /// Where it may be worn or wielded (the equip mask), for an item a wield can take up.
    pub locations: Option<u32>,
    /// Worn or wielded by the scripted character from the start.
    pub worn: bool,
    /// A door: closed until it is used, then open, unless it is locked.
    pub door: Option<ScriptedDoor>,
    /// The most one stack of it holds, for an item that stacks.
    pub max_stack: Option<u16>,
    /// Its material (what it salvages into; a salvage bag's material).
    pub material: Option<u32>,
    /// Its workmanship (a salvage bag's: the average of what went into it).
    pub workmanship: Option<f32>,
    /// Its uses left (a salvage bag's units).
    pub structure: Option<u16>,
    /// The spell it casts when used (a wand's, a food's).
    pub spell_id: Option<u16>,
    /// How it is used (where it must be, and what it may be used on).
    pub useability: Option<u32>,
    /// The item types it may be used on (a wand's spell on a creature, a lockpick on a door).
    pub target_type: Option<u32>,
    /// A vendor: used, it shows its wares, the objects in it.
    pub vendor: Option<ScriptedVendor>,
    /// Used on with an item, it is remade as the server remakes a training weapon an oil is poured
    /// on: it and the item used are gone, and a new item of this name comes into the pack.
    pub remade_as: Option<String>,
}

/// A vendor of the scripted world: used, it shows its wares (the objects in it, each sold at its
/// value without end), and it buys what its item types cover, paying the item's value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScriptedVendor {
    /// The item types it buys.
    pub buys: u32,
}

/// A salvage bag holds at most this many units.
const BAG_UNITS: u16 = 100;

/// The player's integer property of the pyreals it carries.
const COIN_VALUE: u32 = 20;

/// A door of the scripted world: it stands closed, and stops the character running into it, until
/// it is used; then it opens, unless it is locked, when it stays closed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScriptedDoor {
    pub locked: bool,
}

/// The public description's door flag.
const DOOR_FLAG: u32 = 0x0000_1000;

/// The integer property of a lock's resistance to picking, which a door's appraisal tells.
const RESIST_LOCKPICK: u32 = 173;

/// The physics state of an open door: ethereal, walked through.
const ETHEREAL: u32 = 0x0000_0004;

/// The scripted server: the world's objects, as it answers for them.
#[derive(Debug, Clone, Default)]
pub struct Scripted {
    pub player: u32,
    pub objects: Vec<ScriptedObject>,
}

impl Scripted {
    fn object(&self, id: ObjectId) -> Option<&ScriptedObject> {
        self.objects.iter().find(|o| o.id == id.0)
    }

    /// A vendor's wares (`Vendor_VendorInfo`, `0x0062`): the objects in it, each sold without
    /// end, and what it buys. Sent when the vendor is used, and again after each sale or purchase.
    fn vendor_info(
        &self,
        vendor: ObjectId,
        v: ScriptedVendor,
    ) -> dereth_protocol::trade::VendorInfo {
        use dereth_protocol::trade::{ItemProfile, VendorInfo, VendorProfile};
        VendorInfo {
            merchant_id: vendor,
            profile: VendorProfile {
                item_types: v.buys,
                min_value: 0,
                max_value: -1,
                magic: 1,
                buy_price: 1.0,
                sell_price: 1.0,
                ..VendorProfile::default()
            },
            items: self
                .objects
                .iter()
                .filter(|o| o.container == Some(vendor.0))
                .map(|o| ItemProfile {
                    amount: -1,
                    iid: ObjectId(o.id),
                    pwd: Some(stock_pwd(o)),
                })
                .collect(),
        }
    }

    /// The server's answer to one request the client sent.
    fn answer(&self, r: &Request) -> Vec<SessionEvent> {
        use dereth_protocol::objects::{
            ItemOnViewContents, ItemServerSaysContainId, ItemSetAppraiseInfo, ItemUseDone,
        };
        use dereth_protocol::types::{AppraisalProfile, ContentProfile, PropertyTables};
        match r {
            Request::Appraise(m) => {
                let Some(o) = self.object(m.target) else {
                    return Vec::new();
                };
                use dereth_protocol::types::appraisal::flags as af;
                fn table<V: Clone>(
                    entries: &[(u32, V)],
                    flag: u32,
                    flags: &mut u32,
                ) -> Option<dereth_protocol::archive::PackedHash<u32, V>> {
                    (!entries.is_empty()).then(|| {
                        *flags |= flag;
                        dereth_protocol::archive::PackedHash {
                            table_size: 64,
                            entries: entries.to_vec(),
                        }
                    })
                }
                let mut flags = af::INT | af::SPELL_BOOK;
                let bools: Vec<(u32, i32)> =
                    o.bools.iter().map(|(k, b)| (*k, i32::from(*b))).collect();
                let tables = PropertyTables {
                    ints: Some(dereth_protocol::archive::PackedHash {
                        table_size: 64,
                        entries: o.ints.clone(),
                    }),
                    int64s: table(&o.int64s, af::INT64, &mut flags),
                    floats: table(&o.floats, af::FLOAT, &mut flags),
                    strings: table(&o.strings, af::STRING, &mut flags),
                    bools: table(&bools, af::BOOL, &mut flags),
                    dids: table(&o.dids, af::DID, &mut flags),
                    ..PropertyTables::default()
                };
                let creature_profile = o.health.map(|(health, max_health)| {
                    flags |= af::CREATURE_PROFILE;
                    dereth_protocol::types::appraisal::CreatureAppraisalProfile {
                        health,
                        max_health,
                        ..Default::default()
                    }
                });
                let profile = AppraisalProfile {
                    flags,
                    success_flag: 1,
                    spell_book: Some(o.spells.clone()),
                    tables,
                    creature_profile,
                    ..AppraisalProfile::default()
                };
                vec![ui_event(&ItemSetAppraiseInfo {
                    object: m.target,
                    profile,
                })]
            }
            Request::PutItemInContainer(m) => vec![ui_event(&ItemServerSaysContainId {
                item: m.item,
                container: m.container,
                slot: m.slot,
                container_properties: 0,
            })],
            // A gift is taken: it goes into whoever it was given to.
            Request::GiveObjectRequest(m) => vec![ui_event(&ItemServerSaysContainId {
                item: m.item,
                container: m.target,
                slot: 0,
                container_properties: 0,
            })],
            Request::UseEvent(m) => {
                let mut out = Vec::new();
                if let Some(v) = self.object(m.object).and_then(|o| o.vendor) {
                    out.push(ui_event(&self.vendor_info(m.object, v)));
                } else if self
                    .object(m.object)
                    .is_some_and(|o| o.obj_type & dereth_rules::weenie::item_type::CONTAINER != 0)
                {
                    let contents = self
                        .objects
                        .iter()
                        .filter(|o| o.container == Some(m.object.0))
                        .map(|o| ContentProfile {
                            iid: ObjectId(o.id),
                            container_properties: 0,
                        })
                        .collect();
                    out.push(ui_event(&ItemOnViewContents {
                        container: m.object,
                        contents,
                    }));
                }
                out.push(ui_event(&ItemUseDone { failure_type: 0 }));
                out
            }
            Request::CastUntargetedSpell(_)
            | Request::CastTargetedSpell(_)
            | Request::UseWithTargetEvent(_) => {
                vec![ui_event(&ItemUseDone { failure_type: 0 })]
            }
            // A wield is answered with the item worn where it was asked to go.
            Request::GetAndWieldItem(m) => {
                vec![ui_event(&dereth_protocol::objects::ItemWearItem {
                    item: m.item,
                    slot: m.slot,
                })]
            }
            _ => Vec::new(),
        }
    }
}

/// The scripted spell table: a few self buffs, a buff for another, a self heal, Revitalize and
/// Stamina to Mana, a debuff and a bolt by id and name, so that casting can be tested with no game
/// data. A heal, a restoring spell or a debuff is a Life spell, the bolt War, the rest Creature;
/// the debuff, the bolt and Strength Other I are cast on another, the rest on the caster. Every
/// spell but the debuff and the bolt is beneficial. Each is its own category but Might of the
/// Lugians, Strength Self's level 7 (its category and the shape of its description); each "… I"
/// is level 1.
pub const SCRIPTED_SPELLS: &[(u32, &str)] = &[
    (2, "Strength Other I"),
    (1, "Strength Self I"),
    (15, "Heal Self I"),
    (1133, "Armor Self I"),
    (2215, "Blade Protection Self I"),
    (23, "Imperil Other I"),
    (27, "Flame Bolt I"),
    (1177, "Revitalize Self I"),
    (1676, "Stamina to Mana Self I"),
    (2087, "Might of the Lugians"),
];

/// [`SCRIPTED_SPELLS`] as a spell table, with no components.
#[must_use]
pub fn scripted_spell_table() -> dereth_assets::tables::SpellTable {
    let spells = SCRIPTED_SPELLS
        .iter()
        .map(|(id, name)| {
            (
                *id,
                dereth_assets::tables::SpellBase {
                    name: (*name).to_owned(),
                    description: match *id {
                        1 => "Increases the caster's Strength by 10 points.".to_owned(),
                        2087 => "Increases the caster's Strength by 40 points.".to_owned(),
                        _ => String::new(),
                    },
                    school: if name.starts_with("Flame") {
                        1
                    } else if ["Heal", "Imperil", "Revitalize", "Stamina to"]
                        .iter()
                        .any(|p| name.starts_with(p))
                    {
                        2
                    } else {
                        4
                    },
                    icon: 0,
                    // Each spell its own category (family), as no two scripted spells are of one.
                    category: if *id == 2087 { 1 } else { *id },
                    bitfield: if name.starts_with("Imperil") || name.starts_with("Flame") {
                        0
                    } else if name.ends_with("Other I") {
                        dereth_client_model::magic::spell_index::BENEFICIAL
                    } else {
                        dereth_client_model::magic::spell_index::SELF_TARGETED
                            | dereth_client_model::magic::spell_index::BENEFICIAL
                    },
                    base_mana: 10,
                    base_range_constant: 0.0,
                    base_range_mod: 0.0,
                    // Might of the Lugians, the strength buff's level 7, takes a skill of 300.
                    power: if *id == 2087 { 300 } else { 1 },
                    spell_economy_mod: 1.0,
                    formula_version: 1,
                    component_loss: 0.0,
                    meta_spell_type: 1,
                    meta_spell_id: *id,
                    duration: Some((1800.0, 0.0, 0.0)),
                    portal_lifetime: None,
                    // The power component: level 1, or level 7 for Might of the Lugians.
                    raw_comps: [if *id == 2087 { 0x70 } else { 1 }, 0, 0, 0, 0, 0, 0, 0],
                    comp_key: 0,
                    comps: Vec::new(),
                    caster_effect: 0,
                    target_effect: 0,
                    fizzle_effect: 0,
                    recovery_interval: 0.0,
                    recovery_amount: 0.0,
                    display_order: 0,
                    non_component_target_type: 0,
                    mana_mod: 0,
                },
            )
        })
        .collect();
    dereth_assets::tables::SpellTable {
        id: dereth_primitives::DataId(0x0E00_000E),
        spell_buckets: 0,
        spells,
        spellset_bucket_index: 0,
        spellsets: std::collections::BTreeMap::new(),
    }
}

/// A message on the ordered event queue, as the session delivers it.
pub fn ui_event<M: Message>(m: &M) -> SessionEvent {
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob: w.into_inner(),
    }
}

/// A request the client sent, as a line: the message's name and its fields.
#[must_use]
pub fn intent_text(r: &Request) -> String {
    match r {
        Request::Appraise(m) => format!("appraise {:#x}", m.target.0),
        Request::PutItemInContainer(m) => format!(
            "put_item_in_container {:#x} {:#x} {}",
            m.item.0, m.container.0, m.slot
        ),
        Request::UseEvent(m) => format!("use {:#x}", m.object.0),
        Request::GiveObjectRequest(m) => {
            format!("give {:#x} {:#x} {}", m.item.0, m.target.0, m.amount)
        }
        Request::UseWithTargetEvent(m) => {
            format!("use_with_target {:#x} {:#x}", m.object.0, m.target.0)
        }
        Request::GetAndWieldItem(m) => format!("wield {:#x} {:#x}", m.item.0, m.slot),
        Request::CastUntargetedSpell(m) => format!("cast {}", m.spell_id),
        Request::CastTargetedSpell(m) => format!("cast {} {:#x}", m.spell_id, m.target.0),
        Request::TargetedMeleeAttack(m) => format!("melee_attack {:#x}", m.target.0),
        Request::VendorSell(m) => format!(
            "vendor_sell {:#x} {}",
            m.vendor_id.0,
            m.items
                .iter()
                .map(|i| format!("{:#x}", i.iid.0))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Request::VendorBuy(m) => format!(
            "vendor_buy {:#x} {}",
            m.vendor_id.0,
            m.items
                .iter()
                .map(|i| format!("{:#x}x{}", i.iid.0, i.amount))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Request::TradeAddToTrade(m) => format!("trade_add {:#x}", m.item.0),
        Request::TradeAcceptTrade(_) => "trade_accept".to_owned(),
        Request::Emote(m) => format!("emote {}", m.message),
        Request::TalkDirectByName(m) => format!("tell {}: {}", m.target_name, m.message),
        Request::FellowshipRecruit(m) => format!("fellowship_recruit {:#x}", m.target.0),
        other => {
            let d = format!("{other:?}");
            d.split(['(', ' ', '{'])
                .next()
                .unwrap_or_default()
                .to_owned()
        }
    }
}

/// A plugin host over a replayed recording or a scripted world.
pub struct PluginRun {
    host: PluginHost,
    objects: ObjectStream,
    hud: Hud,
    interaction: Interaction,
    net: Option<ClientNetwork>,
    rest: VecDeque<Datagram>,
    /// The run's clock: seconds since the recording's first datagram, or since the start.
    now: f64,
    /// When the player entered the world, on the run's clock.
    entered_at: f64,
    chat: Vec<ChatMessage>,
    outcomes: Vec<Outcome>,
    typed: Vec<ExtensionCommand>,
    verbs: Vec<String>,
    printed: Vec<ChatMessage>,
    hidden: Vec<ChatMessage>,
    sent: Vec<Request>,
    scripted: Option<Scripted>,
    replies: VecDeque<(f64, SessionEvent)>,
    places: ScriptedPlaces,
    session: SessionFacts,
    pregame: Option<ScriptedPregame>,
    /// Pre-game answers due: when, and what (a character to enter as, a refusal, or the list back).
    pregame_due: VecDeque<(f64, PregameStep)>,
    /// What the character's body and the session were asked to do, as lines.
    moves: Vec<String>,
    /// The scripted character's object id.
    player_id: u32,
    /// Whether the scripted character is a player killer.
    pk: bool,
    /// The codes the scripted server refuses the next attacks with, one attack each.
    attack_refusals: VecDeque<u32>,
    /// Whether the scripted server repeats attacks: a swing ends with attack-done `0` (the
    /// sequence goes on) instead of the sequence's end.
    repeat_attacks: bool,
    /// The codes the scripted server fails the next casts with, one cast each (a fizzle is
    /// `0x0402`).
    cast_failures: VecDeque<u32>,
    /// Whether a cast that ends well takes effect: a self buff's enchantment shows on the
    /// character, an item buff's on the item's appraisal.
    casts_land: bool,
    /// Item spells the scripted character knows besides [`SCRIPTED_SPELLS`]: id and name, each
    /// an Item Enchantment spell cast on an item (an aura on the character), its own category,
    /// level 1.
    item_spells: Vec<(u32, String)>,
    /// Other spells of the scripted spell table ([`PluginRun::with_spells`]): id, name, school,
    /// and whether the character knows it.
    extra_spells: Vec<(u32, String, u32, bool)>,
    /// The spells of [`Self::extra_spells`] that are buffs cast on another
    /// ([`PluginRun::with_buffs_for_others`]): beneficial, and aimed at the selection.
    buffs_for_others: Vec<u32>,
    /// Spell table categories given in place of each spell's own ([`PluginRun::with_categories`]).
    categories: Vec<(u32, u32)>,
    /// The player works a movement key at the next frame.
    player_moves: bool,
    /// The server takes control of the character's movement at the next frame.
    server_takes_control: bool,
    /// The frame's messages, received and sent, for the message stream.
    messages: Vec<WireMessage>,
    /// A session the requests sent are written through, to give the stream their bytes.
    wire: dereth_client_net::client_session::Session<Discard>,
    /// The scripted world's travel: journeys over the case's portals.
    travel: crate::scripted_travel::ScriptedTravel,
    /// Doors used and opening: when each opens.
    door_openings: VecDeque<(f64, u32)>,
    /// The id the scripted server gives the next object it makes (a purchase, a salvage bag).
    next_object: u32,
}

/// A transport that sends nothing anywhere: the requests' bytes are only kept.
#[derive(Debug, Default)]
struct Discard;

impl dereth_primitives::Transport for Discard {
    fn send(&mut self, _queue: dereth_primitives::NetQueue, _ordered: bool, _payload: &[u8]) {}
    fn poll(&mut self) -> Option<dereth_primitives::IncomingMessage> {
        None
    }
}

#[derive(Debug, Clone)]
enum PregameStep {
    Enter(String),
    Refuse(u32),
    List,
}

/// What the scripted server does to the world for a request: a sale to a vendor (its items, for
/// their value), a purchase (its stock rows and counts), a salvage (its items) or a merge (from,
/// to, how many).
#[derive(Debug, Clone)]
enum Deal {
    Sell(ObjectId, Vec<ObjectId>),
    Buy(ObjectId, Vec<(ObjectId, i32)>),
    Salvage(Vec<ObjectId>),
    Merge(ObjectId, ObjectId, i32),
    /// A lockpick used on a locked door whose lock can be picked: it is unlocked.
    Unlock(ObjectId),
    /// An item used on an object that is remade (the item used, then the object): both are gone,
    /// and the new one is in the pack.
    Remake(ObjectId, ObjectId),
}

impl std::fmt::Debug for PluginRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRun")
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

/// Read a package folder: every file under it by `/`-separated relative path.
///
/// # Panics
/// Panics when the folder cannot be read; the conformance plugins are committed to the
/// repository.
#[must_use]
pub fn read_package(dir: &Path) -> Package {
    dereth_headless::plugins::read_package(dir)
        .unwrap_or_else(|e| panic!("the test plugin at {} did not read: {e}", dir.display()))
}

/// Put scripted objects into the world's tables: each with its name, type, flags and value, in its
/// container (the main pack when it is the player's).
/// A scripted object as the world's tables keep it: its name, type, flags, value, stack, material,
/// workmanship, uses and place.
fn weenie_of(o: &ScriptedObject, player: u32) -> Weenie {
    use dereth_rules::weenie::{bitfield, item_type};
    let mut it = Weenie::new(ObjectId(o.id));
    it.pwd.name.clone_from(&o.name);
    it.pwd.obj_type = o.obj_type;
    // A vendor stands where it is: used, it shows its wares rather than being picked up.
    it.pwd.bitfield = o.bitfield
        | if o.vendor.is_some() {
            bitfield::VENDOR | bitfield::STUCK
        } else {
            0
        };
    it.pwd.value = Some(o.value);
    it.pwd.stack_size = o.stack;
    it.pwd.max_stack_size = o.max_stack;
    it.pwd.material_type = o.material;
    it.pwd.workmanship = o.workmanship;
    it.pwd.structure = o.structure;
    it.pwd.spell_id = o.spell_id;
    it.pwd.useability = o.useability;
    it.pwd.max_structure = o.structure.map(|_| BAG_UNITS);
    it.pwd.effects = o.effects;
    it.pwd.container_id = o.container.map(ObjectId);
    it.pwd.valid_locations = o.locations;
    it.pwd.target_type = o.target_type;
    if o.worn {
        it.pwd.wielder_id = Some(ObjectId(player));
        // Worn where it may be worn: the slot it occupies.
        it.pwd.location = o.locations;
    }
    if o.obj_type & item_type::CONTAINER != 0 {
        it.pwd.items_capacity = Some(120);
    }
    it
}

/// A vendor's stock item as its wares show it: its description, each field it has flagged in the
/// header as the wire carries it.
fn stock_pwd(o: &ScriptedObject) -> dereth_protocol::types::PublicWeenieDesc {
    use dereth_protocol::types::weeniedesc::header as h;
    let mut d = weenie_of(o, 0).pwd;
    d.container_id = None;
    d.wielder_id = None;
    let mut header = 0;
    for (set, bit) in [
        (d.value.is_some(), h::VALUE),
        (d.effects.is_some(), h::EFFECTS),
        (d.structure.is_some(), h::STRUCTURE),
        (d.max_structure.is_some(), h::MAX_STRUCTURE),
        (d.stack_size.is_some(), h::STACK_SIZE),
        (d.max_stack_size.is_some(), h::MAX_STACK_SIZE),
        (d.valid_locations.is_some(), h::VALID_LOCATIONS),
        (d.workmanship.is_some(), h::WORKMANSHIP),
        (d.spell_id.is_some(), h::SPELL_ID),
        (d.useability.is_some(), h::USEABILITY),
        (d.target_type.is_some(), h::TARGET_TYPE),
        (d.material_type.is_some(), h::MATERIAL_TYPE),
        (d.items_capacity.is_some(), h::ITEMS_CAPACITY),
    ] {
        if set {
            header |= bit;
        }
    }
    d.header = header;
    d
}

fn insert_objects(w: &mut dereth_client_model::World, player: u32, objects: &[ScriptedObject]) {
    use dereth_rules::weenie::item_type;
    for o in objects {
        let id = ObjectId(o.id);
        let mut it = weenie_of(o, player);
        if o.door.is_some() {
            // A door stands where it is: used, it opens rather than being picked up.
            it.pwd.bitfield |= DOOR_FLAG | dereth_rules::weenie::bitfield::STUCK;
            // Closed: not ethereal.
            w.tables.physics.insert(
                id,
                dereth_client_model::objects::PhysicsPresence {
                    cell: None,
                    state: 0,
                    parent: None,
                    setup_id: 0,
                },
            );
        }
        it.valid = true;
        // A worn item is put on as the server's move puts it on: its slot taken, so the stances and
        // the paper doll know it.
        let wear = o.worn.then_some(o.locations).flatten();
        if wear.is_some() {
            it.pwd.wielder_id = None;
            it.pwd.location = None;
        }
        w.tables.weenies.insert(id, it);
        if let Some(location) = wear {
            w.server_says_move_item(
                id,
                ObjectId(0),
                0,
                ObjectId(player),
                location,
                false,
                &mut dereth_client_model::NullSink,
            );
        }
        if o.container == Some(player) {
            let pack = o.obj_type & item_type::CONTAINER != 0;
            if let Some(inv) = w.tables.inventories.get_mut(ObjectId(player)) {
                // A side pack is in the pack's list of packs, as the client keeps it; anything
                // else in its list of items.
                if pack {
                    inv.containers.push(id);
                } else {
                    inv.items.push(id);
                }
            }
            if pack {
                // The side pack's own contents, as the client knows them from its login.
                w.tables
                    .inventories
                    .insert(id, dereth_client_model::objects::ObjectInventory::new(id));
            }
        }
    }
}

impl Default for PluginRun {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRun {
    /// A host offering what the headless client offers.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(|_| {})
    }

    /// A host offering what the headless client offers, its entries given
    /// [`UNHURRIED_BUDGET_MS`], configured by `f`.
    #[must_use]
    pub fn with_config(f: impl FnOnce(&mut HostConfig)) -> Self {
        let mut config = HostConfig::new(
            "dereth-conformance",
            vec![
                HostCapability::Core,
                HostCapability::UiWindow,
                HostCapability::UiHud,
                HostCapability::OverlayWorld,
                HostCapability::OverlayArrow,
                HostCapability::OverlayRadar,
                HostCapability::OverlayMap,
                HostCapability::MapClicks,
                HostCapability::NavMoveto,
                HostCapability::NavJourney,
                HostCapability::NavPath,
                HostCapability::SessionPregame,
                HostCapability::PeersLocal,
                HostCapability::NetHttp,
                HostCapability::Messages,
                HostCapability::UiIcons,
                HostCapability::GameData,
            ],
        );
        config.limits.call_budget_ms = UNHURRIED_BUDGET_MS;
        f(&mut config);
        Self {
            host: PluginHost::new(
                config,
                Box::new(dereth_plugin_lua::LuauEngine),
                Box::new(StaticSource::default()),
                Box::new(MemoryStorage::default()),
            )
            .with_engine(Box::new(dereth_plugin_wasm::WasmEngine::interpreted()))
            .with_vm_clock(Box::new(NoVmTime)),
            objects: ObjectStream::new(),
            hud: Hud::new(),
            interaction: Interaction::default(),
            net: None,
            rest: VecDeque::new(),
            now: 0.0,
            entered_at: 0.0,
            chat: Vec::new(),
            outcomes: Vec::new(),
            typed: Vec::new(),
            verbs: Vec::new(),
            printed: Vec::new(),
            hidden: Vec::new(),
            sent: Vec::new(),
            scripted: None,
            replies: VecDeque::new(),
            places: ScriptedPlaces::default(),
            session: SessionFacts::default(),
            pregame: None,
            pregame_due: VecDeque::new(),
            moves: Vec::new(),
            player_id: SCRIPTED_PLAYER,
            pk: false,
            attack_refusals: VecDeque::new(),
            repeat_attacks: false,
            cast_failures: VecDeque::new(),
            casts_land: false,
            item_spells: Vec::new(),
            extra_spells: Vec::new(),
            buffs_for_others: Vec::new(),
            categories: Vec::new(),
            player_moves: false,
            travel: crate::scripted_travel::ScriptedTravel::default(),
            door_openings: VecDeque::new(),
            next_object: 0x8800_0000,
            server_takes_control: false,
            messages: Vec::new(),
            wire: {
                let mut s = dereth_client_net::client_session::Session::new(Discard);
                s.keep_sent(true);
                s
            },
        }
    }

    /// The same run, its scripted character a player killer.
    #[must_use]
    pub fn with_player_killer(mut self) -> Self {
        self.pk = true;
        self
    }

    /// The same run, its scripted character with object id `id` (two runs on one bus can then see
    /// each other's characters).
    #[must_use]
    pub fn with_player_id(mut self, id: u32) -> Self {
        self.player_id = id;
        self
    }

    /// The same run, its scripted server repeating attacks: each swing ends with the sequence going
    /// on, and the client swings again by itself until the attack is cancelled.
    #[must_use]
    pub fn with_repeating_attacks(mut self) -> Self {
        self.repeat_attacks = true;
        self
    }

    /// The same run, its scripted server refusing the next attacks with `codes`, one each (a
    /// `Communication_WeenieError` and the end of the attack sequence, as the server refuses one).
    #[must_use]
    pub fn with_attack_refusals(mut self, codes: impl IntoIterator<Item = u32>) -> Self {
        self.attack_refusals = codes.into_iter().collect();
        self
    }

    /// The same run, its scripted server failing the next casts with `codes`, one each (the cast
    /// ends with that failure, and nothing takes effect).
    #[must_use]
    pub fn with_cast_failures(mut self, codes: impl IntoIterator<Item = u32>) -> Self {
        self.cast_failures = codes.into_iter().collect();
        self
    }

    /// The same run, its character knowing the item spells `spells` too (id and name): Item
    /// Enchantment spells cast on an item (an "Aura of …" one on the character), each its own
    /// category, level 1.
    #[must_use]
    pub fn with_item_spells(mut self, spells: &[(u32, &str)]) -> Self {
        self.item_spells = spells
            .iter()
            .map(|(id, name)| (*id, (*name).to_owned()))
            .collect();
        self
    }

    /// The same run, its spell table holding `spells` too (id, name and school): a spell whose name
    /// says `Self` is cast on the character, any other on another (with the target selected, as
    /// the debuff of [`SCRIPTED_SPELLS`] is); each its own category, its level the one its name's
    /// numeral says. The character knows them when `known`.
    #[must_use]
    pub fn with_spells(mut self, spells: &[(u32, &str, u32)], known: bool) -> Self {
        self.extra_spells.extend(
            spells
                .iter()
                .map(|(id, name, school)| (*id, (*name).to_owned(), *school, known)),
        );
        self
    }

    /// The same run, its character knowing `spells` too (id, name and school) as buffs cast on
    /// another: beneficial spells aimed at the selected player, each its own category, its level
    /// the one its name's numeral says.
    #[must_use]
    pub fn with_buffs_for_others(mut self, spells: &[(u32, &str, u32)]) -> Self {
        self.buffs_for_others
            .extend(spells.iter().map(|(id, _, _)| *id));
        self.with_spells(spells, true)
    }

    /// The same run, the spells `categories` names (id and category) in those spell table
    /// categories rather than each its own: kindred spells, as the game files them.
    #[must_use]
    pub fn with_categories(mut self, categories: &[(u32, u32)]) -> Self {
        self.categories.extend_from_slice(categories);
        self
    }

    /// The messages the client sent for the plugins, as the requests themselves, in order.
    #[must_use]
    pub fn requests(&self) -> &[Request] {
        &self.sent
    }

    /// The scripted server fails the next casts with `codes`, one each, from now on.
    pub fn fail_casts(&mut self, codes: impl IntoIterator<Item = u32>) {
        self.cast_failures.extend(codes);
    }

    /// Whether casts that end well take effect from now on: a self buff's enchantment shows on
    /// the character (its spell table category, for half an hour), an item buff's in the item's
    /// appraised spells (its id with the high bit set, as the server lists an enchantment).
    pub fn casts_land(&mut self, land: bool) {
        self.casts_land = land;
    }

    /// Move a scripted object to map coordinates `(ns, ew)`, as the server's position update would.
    /// The scripted world's portals and recalls, for journeys.
    pub fn scripted_portals(
        &mut self,
        portals: &[crate::scripted_travel::ScriptedPortal],
        recalls: Vec<crate::scripted_travel::ScriptedRecall>,
    ) {
        self.travel.set_world(portals, recalls);
    }

    /// A wall of the scripted world, on the ground from map coordinates `from` to `to` (each
    /// `(ns, ew)`): it stops the character and blocks a line of sight across it, and walking
    /// routes go round it.
    pub fn wall(&mut self, from: (f64, f64), to: (f64, f64)) {
        if let (Some(a), Some(b)) = (
            dereth_plugin_host::places::from_coords(from.0, from.1, 0.0),
            dereth_plugin_host::places::from_coords(to.0, to.1, 0.0),
        ) {
            self.places.walls.push((a, b));
        }
    }

    /// Something of the scripted world that cuts a line of sight and nothing else, on the ground
    /// from map coordinates `from` to `to` (each `(ns, ew)`): the character walks across it, as
    /// across the edge of a ramp that hides what stands below it.
    pub fn screen(&mut self, from: (f64, f64), to: (f64, f64)) {
        if let (Some(a), Some(b)) = (
            dereth_plugin_host::places::from_coords(from.0, from.1, 0.0),
            dereth_plugin_host::places::from_coords(to.0, to.1, 0.0),
        ) {
            self.places.screens.push((a, b));
        }
    }

    /// A ledge of the scripted world, on the ground from map coordinates `from` to `to` (each
    /// `(ns, ew)`), `height` metres high: the character steps off it, moving across it, but no
    /// walking route takes it, and a journey that only a drop off it would carry on fails, naming
    /// the drop, as the client's own journeys leave a drop to the player.
    pub fn ledge(&mut self, from: (f64, f64), to: (f64, f64), height: f64) {
        if let (Some(a), Some(b)) = (
            dereth_plugin_host::places::from_coords(from.0, from.1, 0.0),
            dereth_plugin_host::places::from_coords(to.0, to.1, 0.0),
        ) {
            self.places.ledges.push((a, b, height));
        }
    }

    pub fn move_object(&mut self, id: u32, ns: f64, ew: f64) {
        if let Some(p) = dereth_plugin_host::places::from_coords(ns, ew, 0.0) {
            self.places.objects.insert(id, p);
        }
        self.frame();
    }

    /// The server sends `events` now: they are applied as a frame applies them, then a frame runs.
    pub fn server_says(&mut self, events: &[SessionEvent]) {
        self.apply(events);
        self.frame();
    }

    /// Someone speaks to the character, as the server tells it: `kind` is `tell` (a tell to the
    /// character), `say` (speech near it) or `emote`; `from` is the speaker's name and `id` its
    /// object.
    ///
    /// # Panics
    /// Panics on a kind that is none of these.
    pub fn hear(&mut self, kind: &str, from: &str, id: u32, text: &str) {
        use dereth_client_model::chat::text_type;
        use dereth_protocol::comms::{
            CommunicationHearDirectSpeech, CommunicationHearEmote, CommunicationHearSpeech,
        };
        let event = match kind {
            "tell" => ui_event(&CommunicationHearDirectSpeech {
                message: text.into(),
                sender_name: from.into(),
                sender_id: ObjectId(id),
                target_id: ObjectId(self.player_id),
                text_type: text_type::SPEECH_DIRECT,
                secret_flags: 0,
            }),
            "say" => ui_event(&CommunicationHearSpeech {
                message: text.into(),
                sender_name: from.into(),
                sender_id: ObjectId(id),
                text_type: text_type::SPEECH,
            }),
            "emote" => ui_event(&CommunicationHearEmote {
                sender: ObjectId(id),
                sender_name: from.into(),
                text: text.into(),
            }),
            other => panic!("no speech of kind {other}: tell, say or emote"),
        };
        self.server_says(&[event]);
    }

    /// The object model, for a scenario that sets up what no scripted message carries.
    pub fn world_mut(&mut self) -> &mut dereth_client_model::World {
        &mut self.objects.world
    }

    /// The same run, reading the game's data through `data` for the plugins that ask.
    #[must_use]
    pub fn with_game_data(
        mut self,
        data: Box<dyn dereth_plugin_host::gamedata::PluginGameData>,
    ) -> Self {
        self.host = self.host.with_game_data(data);
        self
    }

    /// What plugins asked of the game's own panels, a line a panel: `panel plugin hidden=… at=…`.
    #[must_use]
    pub fn retail_ui(&self) -> Vec<String> {
        self.host
            .retail_ui()
            .into_iter()
            .map(|(panel, plugin, hidden, at)| {
                format!(
                    "{panel} {plugin} hidden={} at={}",
                    hidden.map_or("-".to_owned(), |h| h.to_string()),
                    at.map_or("-".to_owned(), |(x, y)| format!("{x},{y}"))
                )
            })
            .collect()
    }

    /// The player presses one of a plugin's hotkeys, and a frame runs.
    pub fn press_hotkey(&mut self, plugin: &str, node: &str) {
        self.host
            .press_hotkey(plugin, node)
            .unwrap_or_else(|e| panic!("hotkey: {e}"));
        self.frame();
    }

    /// The player presses the game's map where `click` says, and a frame runs.
    pub fn map_click(&mut self, click: &dereth_client_contract::panels::map::MapClick) {
        self.host.map_click(click);
        self.frame();
    }

    /// The player enters the world as `name` from the character list, as a click on the character
    /// screen would.
    pub fn log_on(&mut self, name: &str) {
        let gid = self
            .session
            .characters
            .iter()
            .find(|(_, n)| n == name)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no character {name} in the list"));
        self.carry_out(&[], &[SessionRequest::LogOn(gid)]);
        self.frame();
    }

    /// The same run, talking to other runs over `peers`.
    #[must_use]
    pub fn with_peers(mut self, peers: Box<dyn dereth_plugin_host::PluginPeers>) -> Self {
        self.host = self.host.with_peers(peers);
        self
    }

    /// The same run, offering `packages` as the plugins it found: listed, and off until the player
    /// ticks each.
    #[must_use]
    pub fn with_found(mut self, packages: Vec<Package>) -> Self {
        self.host = self.host.with_found(Box::new(StaticSource { packages }));
        self.host.load_all();
        self
    }

    /// The same run, with the plugins' data folders in `files`.
    #[must_use]
    pub fn with_files(mut self, files: Box<dyn dereth_plugin_host::PluginFiles>) -> Self {
        self.host = self.host.with_files(files);
        self
    }

    /// The same run, keeping the plugins' stores in `storage`: another run given the same
    /// storage opens what this one saved.
    #[must_use]
    pub fn with_storage(mut self, storage: Box<dyn dereth_plugin_host::PluginStorage>) -> Self {
        self.host = self.host.with_storage(storage);
        self
    }

    /// The same run, answering the plugins' HTTPS requests with `net`.
    #[must_use]
    pub fn with_net(mut self, net: Box<dyn dereth_plugin_host::PluginNet>) -> Self {
        self.host = self.host.with_net(net);
        self
    }

    /// Put up a scripted pre-game: the account's character list, as after connecting.
    pub fn scripted_pregame(&mut self, pregame: ScriptedPregame) {
        self.session = SessionFacts {
            phase: SessionPhase::CharacterSelect,
            characters: pregame
                .characters
                .iter()
                .map(|(id, n)| (ObjectId(*id), n.clone()))
                .collect(),
            portal_space: false,
            creating_character: false,
        };
        self.pregame = Some(pregame);
        self.frame();
    }

    /// The scripted character's place, as map coordinates and heading.
    #[must_use]
    pub fn player_place(&self) -> Option<(f64, f64, f64)> {
        let p = self.places.player.as_ref()?;
        let (ns, ew) = dereth_plugin_host::places::coords(p)?;
        Some((ns, ew, dereth_plugin_host::places::heading(p)))
    }

    /// Move the scripted character to map coordinates `(ns, ew)`, as a teleport would.
    pub fn place_player(&mut self, ns: f64, ew: f64) {
        self.places.player = dereth_plugin_host::places::from_coords(ns, ew, 0.0);
        self.frame();
    }

    /// The scripted character's health, stamina and mana, each `(current, maximum)`, and a frame
    /// run. (A vital's maximum is its stored level here: the scripted world's vitals formula adds
    /// nothing.)
    pub fn set_vitals(&mut self, health: (u32, u32), stamina: (u32, u32), mana: (u32, u32)) {
        use dereth_protocol::types::qualities::{Attribute, AttributeCache, SecondaryAttribute};
        let zero = dereth_assets::tables::SkillFormula {
            w: 0,
            x: 0,
            y: 0,
            z: 1,
            attr1: 0,
            attr2: 0,
        };
        if self.hud.vitals_table.is_none() {
            self.hud.vitals_table = Some(dereth_assets::tables::Attribute2ndTable {
                id: dereth_primitives::DataId(0x0E00_0003),
                health: zero,
                stamina: zero,
                mana: zero,
            });
        }
        let w = &mut self.objects.world;
        let mut desc = w.player_qualities().cloned().unwrap_or_default();
        let v = |(cur, max): (u32, u32)| {
            Some(SecondaryAttribute {
                attribute: Attribute {
                    level_from_cp: 0,
                    init_level: max,
                    cp_spent: 0,
                },
                current_level: cur,
            })
        };
        let mut cache = desc.attributes.unwrap_or(AttributeCache {
            flags: 0,
            strength: None,
            endurance: None,
            quickness: None,
            coordination: None,
            focus: None,
            self_: None,
            health: None,
            stamina: None,
            mana: None,
        });
        cache.health = v(health);
        cache.stamina = v(stamina);
        cache.mana = v(mana);
        desc.attributes = Some(cache);
        w.set_player_desc(desc);
        self.hud.player_desc_received = true;
        self.frame();
    }

    /// The scripted character's skill `name`, trained, at `current` (its base and buffed value
    /// alike), and a frame run.
    pub fn set_skill(&mut self, name: &str, current: i32) {
        use dereth_client_contract::view::SkillEntry;
        let skills = &mut self.hud.skills;
        if let Some(s) = skills.iter_mut().find(|s| s.name == name) {
            s.level = current;
            s.effective = current;
        } else {
            let id = u32::try_from(skills.len()).unwrap_or(0) + 1;
            skills.push(SkillEntry {
                id,
                name: name.to_owned(),
                icon: None,
                min_level: 0,
                sac: 2,
                level: current,
                effective: current,
                vitae: 0,
            });
        }
        self.frame();
    }

    /// A teleport of the scripted character: a second in portal space, then placed at map
    /// coordinates `(ns, ew)`.
    pub fn teleport(&mut self, ns: f64, ew: f64) {
        self.session.portal_space = true;
        self.advance(1.0);
        self.places.player = dereth_plugin_host::places::from_coords(ns, ew, 0.0);
        self.session.portal_space = false;
        self.frame();
    }

    /// The player works a movement key (the hook hears it at the next frame), and the frame runs.
    pub fn player_moves(&mut self) {
        self.player_moves = true;
        self.frame();
    }

    /// The server takes control of the character's movement (to walk it to something used, say),
    /// and the frame runs.
    pub fn server_takes_control(&mut self) {
        self.server_takes_control = true;
        self.frame();
    }

    /// Unload plugin `id` (the player's `/plugin unload`), and run the frame that does it.
    pub fn unload(&mut self, id: &str) {
        self.host.request_unload(id);
        self.frame();
    }

    /// The scripted character dies: its count of deaths (integer property 43) rises, as the
    /// server raises it.
    pub fn die(&mut self) {
        if let Some(q) = self.objects.world.player_qualities_mut() {
            let ints = q.ints.get_or_insert_with(Default::default);
            *ints.entry(43).or_insert(0) += 1;
        }
        self.frame();
    }

    /// Every marker the plugins keep, as the host lists them.
    #[must_use]
    pub fn overlays(&self) -> String {
        self.host.overlays().dump()
    }

    /// What the character's body and the session were asked to do, as lines, in order: `face
    /// <heading>`, `autorun on|off`, `log_on <name>`, `log_off`, `quit`.
    #[must_use]
    pub fn moves(&self) -> Vec<String> {
        self.moves.clone()
    }

    /// The session's phase.
    #[must_use]
    pub fn phase(&self) -> SessionPhase {
        self.session.phase
    }

    /// The host, for what the five calls do not cover.
    pub fn host(&mut self) -> &mut PluginHost {
        &mut self.host
    }

    /// The host itself, to install elsewhere or to ask as a frame hook.
    #[must_use]
    pub fn into_host(self) -> PluginHost {
        self.host
    }

    /// Load a package; it runs at the next frame.
    pub fn load(&mut self, package: Package) {
        self.host.request_load(package);
        self.frame();
    }

    /// Replay `session`'s login up to the world, selecting `character` (the first when `None`).
    ///
    /// # Panics
    /// Panics when the recording never reaches the world.
    pub fn login(&mut self, session: &str, character: Option<&str>) {
        let recs = records(session);
        let mut net = recorded_endpoint(&recs);
        let mut rest: VecDeque<Datagram> = recs.into_iter().filter(|r| !r.c2s).collect();
        let mut entered = false;
        while let Some(r) = rest.pop_front() {
            self.frames_until(r.t);
            let events = self.feed(&mut net, &r);
            for e in &events {
                if let SessionEvent::CharacterSet(set) = e {
                    if !entered {
                        let c = character
                            .and_then(|want| set.characters.iter().find(|c| c.name == want))
                            .or_else(|| set.characters.first())
                            .unwrap_or_else(|| panic!("{session}: the character set is empty"));
                        net.enter_world(c.gid, &set.account);
                        entered = true;
                    }
                }
            }
            if net.session_state() == SessionState::Playable {
                break;
            }
        }
        assert_eq!(
            net.session_state(),
            SessionState::Playable,
            "{session} never reached the world"
        );
        self.entered_at = self.now;
        self.net = Some(net);
        self.rest = rest;
        self.frame();
    }

    /// Enter a scripted world: a character named `character` holding a main pack, and `objects`
    /// wherever each says. The scripted server answers the client from then on.
    pub fn scripted_world(&mut self, character: &str, objects: Vec<ScriptedObject>) {
        self.enter_scripted(character, objects);
        self.frame();
    }

    fn enter_scripted(&mut self, character: &str, objects: Vec<ScriptedObject>) {
        use dereth_client_model::objects::ObjectInventory;
        use dereth_rules::weenie::{bitfield, item_type};
        let player = self.player_id;
        let w = &mut self.objects.world;
        let mut me = Weenie::new(ObjectId(player));
        me.pwd.name = character.to_owned();
        me.pwd.bitfield = bitfield::PLAYER | if self.pk { bitfield::PLAYER_KILLER } else { 0 };
        me.pwd.obj_type = item_type::CREATURE;
        me.pwd.items_capacity = Some(102);
        me.pwd.containers_capacity = Some(7);
        me.valid = true;
        w.tables.weenies.insert(ObjectId(player), me);
        w.player = Some(ObjectId(player));
        w.tables
            .inventories
            .insert(ObjectId(player), ObjectInventory::new(ObjectId(player)));
        insert_objects(w, player, &objects);
        // The character's description: a combat table, so it can fight (the value is any id; the
        // scripted world reads no data).
        let mut desc = dereth_client_model::Qualities::new();
        // It knows every scripted spell.
        desc.spell_book = Some(
            SCRIPTED_SPELLS
                .iter()
                .map(|(id, _)| *id)
                .chain(self.item_spells.iter().map(|(id, _)| *id))
                .chain(
                    self.extra_spells
                        .iter()
                        .filter(|s| s.3)
                        .map(|(id, _, _, _)| *id),
                )
                .map(|id| {
                    (
                        id,
                        dereth_protocol::types::qualities::SpellBookPage::default(),
                    )
                })
                .collect(),
        );
        desc.dids = Some(
            [(
                dereth_client_model::combat::COMBAT_TABLE_DID,
                dereth_primitives::DataId(0x3000_0000),
            )]
            .into_iter()
            .collect(),
        );
        w.set_player_desc(desc);
        // The description counts as received: what reads it (the enchantments among them) does.
        self.hud.player_desc_received = true;
        // The character stands in the world, so what is said and told to it is drawn.
        self.hud.player = Some(ObjectId(player));
        self.hud.player_body = true;
        let mut table = scripted_spell_table();
        // The item spells: Strength Self I's row, renamed, Item Enchantment, cast on an item.
        if let Some(base) = table.spells.get(&1).cloned() {
            for (id, name) in &self.item_spells {
                let mut b = base.clone();
                b.name.clone_from(name);
                b.description = String::new();
                b.school = 3;
                b.category = *id;
                if name.starts_with("Aura of") {
                    // An aura: cast on the character, for whatever it holds.
                    b.bitfield = dereth_client_model::magic::spell_index::SELF_TARGETED;
                } else {
                    b.bitfield = 0;
                    // A complete formula, level 1, whose target component (0x39) aims it at
                    // an enchantable item.
                    b.raw_comps = [1, 1, 1, 1, 0x39, 0, 0, 0];
                }
                b.meta_spell_id = *id;
                table.spells.insert(*id, b);
            }
            for (id, name, school, _) in &self.extra_spells {
                let mut b = base.clone();
                b.name.clone_from(name);
                b.description = String::new();
                b.school = *school;
                b.category = *id;
                b.bitfield = if name.contains(" Self") {
                    dereth_client_model::magic::spell_index::SELF_TARGETED
                } else if self.buffs_for_others.contains(id) {
                    dereth_client_model::magic::spell_index::BENEFICIAL
                } else {
                    0
                };
                b.meta_spell_id = *id;
                table.spells.insert(*id, b);
            }
        }
        for (id, category) in &self.categories {
            if let Some(b) = table.spells.get_mut(id) {
                b.category = *category;
            }
        }
        w.magic.spell_table = Some(std::sync::Arc::new(table.clone()));
        self.hud.spell_table = Some(table);
        self.places.player = dereth_plugin_host::places::from_coords(START_NS, START_EW, 0.0);
        self.places.objects = objects
            .iter()
            .filter_map(|o| {
                let (ns, ew) = o.place?;
                Some((o.id, dereth_plugin_host::places::from_coords(ns, ew, 0.0)?))
            })
            .collect();
        self.places.closed_doors = objects
            .iter()
            .filter(|o| o.door.is_some())
            .filter_map(|o| Some((o.id, *self.places.objects.get(&o.id)?)))
            .collect();
        self.scripted = Some(Scripted { player, objects });
        self.session.phase = SessionPhase::InWorld;
        self.entered_at = self.now;
    }

    /// More objects come into the scripted world (a corpse where a monster fell, say), each where
    /// it says.
    pub fn spawn(&mut self, objects: Vec<ScriptedObject>) {
        let player = self.player_id;
        insert_objects(&mut self.objects.world, player, &objects);
        for o in &objects {
            if let Some((ns, ew)) = o.place {
                if let Some(p) = dereth_plugin_host::places::from_coords(ns, ew, 0.0) {
                    self.places.objects.insert(o.id, p);
                    if o.door.is_some() {
                        self.places.closed_doors.insert(o.id, p);
                    }
                }
            }
        }
        if let Some(s) = self.scripted.as_mut() {
            s.objects.extend(objects);
        }
    }

    /// An object leaves the scripted world (a monster that died, say).
    pub fn remove(&mut self, id: u32) {
        self.objects.world.tables.weenies.remove(ObjectId(id));
        self.places.objects.remove(&id);
        if let Some(s) = self.scripted.as_mut() {
            s.objects.retain(|o| o.id != id);
        }
    }

    /// Carry on for `secs` seconds, frames at [`FRAME_RATE`] between recorded datagrams and the
    /// scripted server's answers.
    pub fn advance(&mut self, secs: f64) {
        let until = self.now + secs;
        if let Some(mut net) = self.net.take() {
            while self.rest.front().is_some_and(|r| r.t <= until) {
                let r = self.rest.pop_front().expect("front exists");
                self.frames_until(r.t);
                self.feed(&mut net, &r);
            }
            self.net = Some(net);
        }
        self.frames_until(until);
    }

    /// Seconds of run time since the player entered the world.
    #[must_use]
    pub fn since_entry(&self) -> f64 {
        self.now - self.entered_at
    }

    /// Server events through the HUD and interaction, as a frame applies them.
    fn apply(&mut self, events: &[SessionEvent]) {
        let chat = self.hud.apply_events_with_combat_mode_handler(
            events,
            &mut self.objects.world,
            &mut dereth_client_runtime::hud::NoPanels,
            None,
            &mut |_, _| {},
        );
        dereth_client_runtime::interaction::apply_events(
            &mut self.interaction,
            events,
            &mut self.objects.world,
        );
        self.chat.extend(chat);
        self.outcomes.extend(outcomes_of(events));
        self.messages
            .extend(events.iter().filter_map(WireMessage::received));
    }

    /// One recorded datagram through the session, the object stream, the HUD and interaction;
    /// answers the session's events.
    fn feed(&mut self, net: &mut ClientNetwork, r: &Datagram) -> Vec<SessionEvent> {
        let now = LocalTime(r.t);
        self.now = self.now.max(r.t);
        net.feed(&r.raw, peer(r.pair), now);
        net.tick(now);
        let _ = net.take_outgoing();
        let events = self.objects.pump(net, now);
        self.apply(&events);
        events
    }

    fn frames_until(&mut self, t: f64) {
        while self.now + 1.0 / FRAME_RATE <= t {
            self.now += 1.0 / FRAME_RATE;
            self.places.walk(1.0 / FRAME_RATE);
            self.frame();
        }
    }

    /// The pre-game's due answers.
    fn pregame_step(&mut self) {
        while self
            .pregame_due
            .front()
            .is_some_and(|(t, _)| *t <= self.now)
        {
            let Some((_, step)) = self.pregame_due.pop_front() else {
                break;
            };
            match step {
                PregameStep::Refuse(code) => {
                    self.outcomes.push(Outcome::LoginFailed { code });
                }
                PregameStep::Enter(name) => {
                    let objects = self
                        .pregame
                        .as_ref()
                        .map(|p| p.objects.clone())
                        .unwrap_or_default();
                    self.enter_scripted(&name, objects);
                }
                PregameStep::List => {
                    self.objects = ObjectStream::new();
                    self.hud = Hud::new();
                    self.scripted = None;
                    self.places = ScriptedPlaces::default();
                    self.session.phase = SessionPhase::CharacterSelect;
                }
            }
        }
    }

    /// What the frame asked of the character's body and of the session.
    fn carry_out(&mut self, nav: &[NavRequest], session: &[SessionRequest]) {
        for n in nav {
            match *n {
                NavRequest::Face(h) => {
                    self.moves.push(format!("face {h:.0}"));
                    self.places.turning_to = Some(f64::from(h));
                }
                NavRequest::Autorun(on) => {
                    self.moves
                        .push(format!("autorun {}", if on { "on" } else { "off" }));
                    self.places.autorun = on;
                }
                NavRequest::Hold(key, down) => {
                    self.moves.push(format!(
                        "{} {}",
                        if down { "hold" } else { "let go" },
                        nav_key_name(key)
                    ));
                    if let Some(leap) = self.places.hold(key, down, self.now) {
                        self.moves.push(leap);
                    }
                }
                NavRequest::CancelJump => {
                    self.moves.push("put down jump".into());
                    self.places.charging = None;
                }
            }
        }
        for s in session {
            match *s {
                SessionRequest::LogOn(gid) => {
                    let name = self
                        .session
                        .characters
                        .iter()
                        .find(|(id, _)| *id == gid)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_default();
                    self.moves.push(format!("log_on {name}"));
                    self.session.phase = SessionPhase::Entering;
                    let still = self
                        .pregame
                        .as_mut()
                        .and_then(|p| {
                            let i = p.still_in_world.iter().position(|n| *n == name)?;
                            Some(p.still_in_world.remove(i))
                        })
                        .is_some();
                    let at = self.now + SCRIPTED_LATENCY;
                    if still {
                        self.pregame_due.push_back((at, PregameStep::Refuse(0x0D)));
                        self.pregame_due.push_back((at, PregameStep::List));
                    } else {
                        self.pregame_due.push_back((at, PregameStep::Enter(name)));
                    }
                }
                SessionRequest::LogOff => {
                    self.moves.push("log_off".into());
                    self.pregame_due
                        .push_back((self.now + SCRIPTED_LATENCY, PregameStep::List));
                }
                SessionRequest::Quit => self.moves.push("quit".into()),
            }
        }
    }

    /// One host frame at the current time: the scripted server's due answers, the host's step,
    /// then the interaction step for what it asked.
    pub fn frame(&mut self) {
        let mut due = Vec::new();
        while self.replies.front().is_some_and(|(t, _)| *t <= self.now) {
            due.push(self.replies.pop_front().expect("front exists").1);
        }
        if !due.is_empty() {
            self.apply(&due);
        }
        // Doors used a moment ago open: ethereal, and walked through.
        while self
            .door_openings
            .front()
            .is_some_and(|(t, _)| *t <= self.now)
        {
            let Some((_, door)) = self.door_openings.pop_front() else {
                break;
            };
            self.objects
                .world
                .set_physics_state(ObjectId(door), ETHEREAL);
            self.places.closed_doors.remove(&door);
        }
        self.pregame_step();
        let chat = std::mem::take(&mut self.chat);
        let outcomes = std::mem::take(&mut self.outcomes);
        let typed = std::mem::take(&mut self.typed);
        let messages = std::mem::take(&mut self.messages);
        let out = {
            let view = self.hud.view(&self.objects);
            let frame = HookFrame::new(
                &view,
                &self.objects.world,
                self.interaction.command_table(),
                self.now,
                self.now,
                &chat,
                &typed,
            )
            .with_outcomes(&outcomes)
            .with_messages(&messages)
            .with_session(Some(WORLD_NAME), None)
            .with_pregame(&self.session);
            let stream = StreamPlaces {
                player: None,
                objects: &self.objects,
                autorun: false,
                sight: None,
            };
            let places: &dyn Places = if self.scripted.is_some() {
                &self.places
            } else {
                &stream
            };
            let mut frame = frame
                .with_places(places)
                .with_control(self.player_moves, self.server_takes_control)
                .with_travel(&self.travel.status, Some(&self.travel.store));
            self.host.step(&mut frame);
            frame.into_output()
        };
        self.server_takes_control = false;
        self.carry_out(&out.nav, &out.session);
        for r in out.travel {
            self.travel.request(r);
        }
        if self.scripted.is_some() {
            self.travel.step(
                self.now,
                1.0 / FRAME_RATE,
                &mut self.places,
                std::mem::take(&mut self.player_moves),
            );
            self.moves.append(&mut self.travel.moves);
        } else {
            self.player_moves = false;
        }
        self.hidden
            .extend(out.hidden.iter().map(|&i| chat[i].clone()));
        self.printed.extend(out.printed);
        if let Some(v) = out.verbs {
            // As the client does: the claimed verbs are the interaction's, for the lines that
            // reach it from now on.
            self.interaction.set_extension_verbs(v.clone());
            self.verbs = v;
        }
        if !out.requests.is_empty() {
            self.interaction.queue(Vec::new(), out.requests);
            let _ = self.interaction.run_ui_requests(
                &mut self.objects.world,
                true,
                ServerTime(self.now),
            );
        }
        // A line the client took for an extension's command reaches the plugins at the next frame.
        self.typed
            .extend(self.interaction.take_extension_commands());
        if self.scripted.is_some() {
            // The frame's combat step: the power bar that turns a click into a swing, and a combat
            // toggle asked for (with no scene there is nothing to target automatically). The
            // scripted body is always settled: no motions pending.
            let world = &mut self.objects.world;
            let attack_ready = world.ready_position_mode_arm(true, false);
            self.interaction
                .run_power_bar(world, attack_ready, LocalTime(self.now));
            let ready = world.ready_position_mode_arm(false, false);
            let phys = dereth_client_runtime::selection_geometry::SceneSelectionPhysics::new(
                None,
                &self.objects,
            );
            self.interaction.run_combat_mode_toggle(
                &mut self.objects.world,
                ready,
                &phys,
                25.0,
                LocalTime(self.now),
            );
        }
        {
            let sent = self.interaction.take_pending_requests();
            // Item buffs that took effect: the item's appraised spells gain them.
            let mut on_items: Vec<(u32, u32)> = Vec::new();
            // Doors used: each opens a moment later, unless it is locked.
            let mut opening: Vec<u32> = Vec::new();
            // Sales, purchases, salvages and merges: what each does to the world.
            let mut deals: Vec<Deal> = Vec::new();
            if let Some(s) = &self.scripted {
                for r in &sent {
                    match r {
                        Request::VendorSell(m) => deals.push(Deal::Sell(
                            m.vendor_id,
                            m.items.iter().map(|i| i.iid).collect(),
                        )),
                        Request::VendorBuy(m) => deals.push(Deal::Buy(
                            m.vendor_id,
                            m.items.iter().map(|i| (i.iid, i.amount)).collect(),
                        )),
                        Request::CreateTinkeringTool(m) => {
                            deals.push(Deal::Salvage(m.items.clone()));
                        }
                        Request::StackableMerge(m) => {
                            deals.push(Deal::Merge(m.merge_from, m.merge_to, m.amount));
                        }
                        // A lockpick used on a locked door whose appraisal tells its lock's
                        // resistance (a lock that can be picked) unlocks it.
                        Request::UseWithTargetEvent(m)
                            if s.object(m.object).is_some_and(|o| {
                                o.obj_type & dereth_rules::weenie::item_type::KEY != 0
                            }) && s.object(m.target).is_some_and(|o| {
                                o.door.is_some_and(|d| d.locked)
                                    && o.ints.iter().any(|(k, _)| *k == RESIST_LOCKPICK)
                            }) =>
                        {
                            deals.push(Deal::Unlock(m.target));
                        }
                        Request::UseWithTargetEvent(m)
                            if s.object(m.target).is_some_and(|o| o.remade_as.is_some()) =>
                        {
                            deals.push(Deal::Remake(m.object, m.target));
                        }
                        _ => {}
                    }
                    if let Request::UseEvent(m) = r {
                        if s.object(m.object)
                            .and_then(|o| o.door)
                            .is_some_and(|d| !d.locked)
                        {
                            opening.push(m.object.0);
                        }
                    }
                    let mut answers: Vec<(f64, SessionEvent)> = Vec::new();
                    let mut failed = false;
                    let delay = match r {
                        // As the server answers an attack with repeating attacks off: nothing
                        // until the swing ends, then the end of the attack sequence
                        // (`ActionCancelled`); a refused one, the refusal and that end at once.
                        Request::TargetedMeleeAttack(_) | Request::TargetedMissileAttack(_) => {
                            use dereth_protocol::combat::CombatHandleAttackDoneEvent;
                            let end = ui_event(&CombatHandleAttackDoneEvent {
                                error: if self.repeat_attacks { 0 } else { 0x0036 },
                            });
                            if let Some(code) = self.attack_refusals.pop_front() {
                                answers.push((
                                    SCRIPTED_LATENCY,
                                    ui_event(&dereth_protocol::comms::CommunicationWeenieError {
                                        error_type: code,
                                    }),
                                ));
                                answers.push((SCRIPTED_LATENCY, end));
                            } else {
                                answers.push((SCRIPTED_SWING, end));
                            }
                            SCRIPTED_LATENCY
                        }
                        Request::CastUntargetedSpell(_) | Request::CastTargetedSpell(_) => {
                            let (spell, target) = match r {
                                Request::CastTargetedSpell(m) => (m.spell_id, m.target.0),
                                Request::CastUntargetedSpell(m) => (m.spell_id, s.player),
                                _ => unreachable!(),
                            };
                            if let Some(code) = self.cast_failures.pop_front() {
                                answers.push((
                                    SCRIPTED_LATENCY,
                                    ui_event(&dereth_protocol::objects::ItemUseDone {
                                        failure_type: code,
                                    }),
                                ));
                                failed = true;
                            } else if self.casts_land && target == s.player {
                                let category = self
                                    .hud
                                    .spell_table
                                    .as_ref()
                                    .and_then(|t| t.spells.get(&spell))
                                    .map_or(0, |b| b.category);
                                answers.push((
                                    SCRIPTED_LATENCY,
                                    ui_event(&dereth_protocol::qualities::MagicUpdateEnchantment(
                                        dereth_protocol::types::qualities::Enchantment {
                                            id: spell,
                                            category_word: category,
                                            duration: 1800.0,
                                            caster: ObjectId(s.player),
                                            // An additive, beneficial raise of an attribute.
                                            smod: dereth_protocol::types::qualities::StatMod {
                                                kind: 0x0200_8001,
                                                key: 1,
                                                value: 10.0,
                                            },
                                            ..Default::default()
                                        },
                                    )),
                                ));
                            } else if self.casts_land {
                                on_items.push((target, spell));
                            }
                            SCRIPTED_LATENCY
                        }
                        _ => SCRIPTED_LATENCY,
                    };
                    if !failed {
                        answers.extend(s.answer(r).into_iter().map(|e| (delay, e)));
                    }
                    for (delay, e) in answers {
                        // In time order, after any answer due at the same time.
                        let at = self.now + delay;
                        let i = self.replies.partition_point(|(t, _)| *t <= at);
                        self.replies.insert(i, (at, e));
                    }
                }
            }
            for door in opening {
                self.door_openings
                    .push_back((self.now + SCRIPTED_LATENCY, door));
            }
            for d in deals {
                self.deal(d);
            }
            if let Some(s) = &mut self.scripted {
                for (item, spell) in on_items {
                    if let Some(o) = s.objects.iter_mut().find(|o| o.id == item) {
                        o.spells.push(spell | 0x8000_0000);
                    }
                }
            }
            for r in &sent {
                dereth_client_runtime::requests::send_request(&mut self.wire, r);
            }
            self.messages
                .extend(self.wire.take_sent().into_iter().map(WireMessage::sent));
            self.sent.extend(sent);
        }
    }

    /// The pyreals the scripted character carries.
    #[must_use]
    pub fn coins(&self) -> i32 {
        self.objects
            .world
            .player_qualities()
            .map_or(0, |q| q.inq_int(COIN_VALUE))
    }

    /// The scripted character carries `n` pyreals.
    pub fn set_coins(&mut self, n: i32) {
        self.set_player_int(COIN_VALUE, n);
    }

    /// The scripted character's integer property `key` (a game property number) made `n`: its
    /// level, say.
    pub fn set_player_int(&mut self, key: u32, n: i32) {
        use dereth_client_model::{StatKey, StatType, StatValue};
        if let Some(q) = self.objects.world.player_qualities_mut() {
            q.set(StatKey::new(StatType::Int, key), StatValue::Int(n));
        }
    }

    /// A new object of the scripted world's, in the character's pack.
    fn make(&mut self, mut o: ScriptedObject) -> u32 {
        o.id = self.next_object;
        self.next_object += 1;
        o.container = Some(self.player_id);
        o.place = None;
        let player = self.player_id;
        insert_objects(&mut self.objects.world, player, std::slice::from_ref(&o));
        let id = o.id;
        if let Some(s) = self.scripted.as_mut() {
            s.objects.push(o);
        }
        id
    }

    /// An object of the scripted world's gone (sold, salvaged, merged away).
    fn unmake(&mut self, id: ObjectId) {
        self.objects.world.server_says_remove(
            id,
            ServerTime(self.now),
            &mut dereth_client_model::NullSink,
        );
        if let Some(s) = self.scripted.as_mut() {
            s.objects.retain(|o| o.id != id.0);
        }
    }

    /// A scripted object's description changed: the world's copy takes it.
    fn remake(&mut self, o: &ScriptedObject) {
        let player = self.player_id;
        let fresh = weenie_of(o, player).pwd;
        if let Some(w) = self.objects.world.tables.weenies.get_mut(ObjectId(o.id)) {
            w.pwd.name = fresh.name;
            w.pwd.stack_size = fresh.stack_size;
            w.pwd.value = fresh.value;
            w.pwd.workmanship = fresh.workmanship;
            w.pwd.structure = fresh.structure;
        }
        if let Some(s) = self.scripted.as_mut() {
            if let Some(x) = s.objects.iter_mut().find(|x| x.id == o.id) {
                *x = o.clone();
            }
        }
    }

    fn scripted_object(&self, id: ObjectId) -> Option<ScriptedObject> {
        self.scripted.as_ref()?.object(id).cloned()
    }

    /// The scripted server's answer to a sale, a purchase, a salvage or a merge, in the world: the
    /// items and pyreals change hands (and the vendor shows its wares again, which ends the
    /// client's request), the salvaged items become bags of their materials, stacks merge.
    fn deal(&mut self, d: Deal) {
        use dereth_rules::weenie::item_type;
        match d {
            Deal::Remake(item, target) => {
                if let Some(o) = self.scripted_object(target) {
                    self.unmake(item);
                    // Gone at once, as the server deletes it: out of its slot, and nothing of it
                    // left to read.
                    self.unmake(target);
                    self.remove(target.0);
                    self.make(ScriptedObject {
                        name: o.remade_as.clone().unwrap_or_default(),
                        worn: false,
                        remade_as: None,
                        ..o
                    });
                }
            }
            Deal::Unlock(door) => {
                if let Some(s) = self.scripted.as_mut() {
                    if let Some(d) = s
                        .objects
                        .iter_mut()
                        .find(|o| o.id == door.0)
                        .and_then(|o| o.door.as_mut())
                    {
                        d.locked = false;
                    }
                }
            }
            Deal::Sell(vendor, items) => {
                let mut paid = 0i64;
                for it in items {
                    if let Some(w) = self.objects.world.weenie(it) {
                        paid += i64::from(w.pwd.value.unwrap_or(0));
                        self.unmake(it);
                    }
                }
                let coins = i64::from(self.coins()) + paid;
                self.set_coins(i32::try_from(coins).unwrap_or(i32::MAX));
                self.vendor_answers(vendor);
            }
            Deal::Buy(vendor, rows) => {
                let wanted: Vec<(ScriptedObject, i32)> = rows
                    .into_iter()
                    .filter_map(|(id, n)| Some((self.scripted_object(id)?, n)))
                    .collect();
                let cost: i64 = wanted
                    .iter()
                    .map(|(o, n)| i64::from(o.value) * i64::from(*n))
                    .sum();
                if cost <= i64::from(self.coins()) {
                    for (o, n) in wanted {
                        let stacks = o.max_stack.is_some_and(|m| m > 1);
                        let unit = o.value;
                        let copies = if stacks { 1 } else { n.max(0) };
                        for _ in 0..copies {
                            let mut item = o.clone();
                            if stacks {
                                item.stack = u16::try_from(n).ok();
                                item.value = unit * u32::try_from(n).unwrap_or(0);
                            }
                            self.make(item);
                        }
                    }
                    let left = i64::from(self.coins()) - cost;
                    self.set_coins(i32::try_from(left).unwrap_or(0));
                }
                self.vendor_answers(vendor);
            }
            Deal::Salvage(items) => {
                // Each salvage makes bags of its own, a material's units filling one before the
                // next, as the server makes them.
                let mut made = Vec::new();
                for it in items {
                    let Some(o) = self.scripted_object(it) else {
                        continue;
                    };
                    self.unmake(it);
                    let work = o.workmanship.unwrap_or(1.0);
                    let units = if o.obj_type & item_type::TINKERING_MATERIAL != 0 {
                        o.structure.unwrap_or(0)
                    } else {
                        // As many units as its workmanship, at least one.
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let u = work.round().max(1.0) as u16;
                        u
                    };
                    self.add_salvage(o.material.unwrap_or(0), units, work, &mut made);
                }
            }
            Deal::Merge(from, to, amount) => {
                if let (Some(mut a), Some(mut b)) =
                    (self.scripted_object(from), self.scripted_object(to))
                {
                    let room =
                        i32::from(b.max_stack.unwrap_or(1)) - i32::from(b.stack.unwrap_or(1));
                    let moved = amount.min(room).min(i32::from(a.stack.unwrap_or(1))).max(0);
                    let unit = a.value / u32::from(a.stack.unwrap_or(1)).max(1);
                    let moved = u16::try_from(moved).unwrap_or(0);
                    b.stack = Some(b.stack.unwrap_or(1) + moved);
                    b.value += unit * u32::from(moved);
                    self.remake(&b);
                    let left = a.stack.unwrap_or(1).saturating_sub(moved);
                    if left == 0 {
                        self.unmake(from);
                    } else {
                        a.stack = Some(left);
                        a.value = unit * u32::from(left);
                        self.remake(&a);
                    }
                }
                self.objects.world.request_lock.clear();
            }
        }
    }

    /// `units` of salvage of `material` at workmanship `work` into the pack: into the first bag
    /// of that material with room of those this salvage `made`, the rest into a new bag, a bag
    /// holding at most [`BAG_UNITS`]; a bag's workmanship is the average of its units'.
    fn add_salvage(&mut self, material: u32, mut units: u16, work: f32, made: &mut Vec<u32>) {
        use dereth_rules::weenie::item_type;
        while units > 0 {
            let bag = self.scripted.as_ref().and_then(|s| {
                s.objects
                    .iter()
                    .find(|o| {
                        made.contains(&o.id)
                            && o.material == Some(material)
                            && o.structure.unwrap_or(0) < BAG_UNITS
                    })
                    .cloned()
            });
            let mut bag = bag.unwrap_or_else(|| ScriptedObject {
                name: "Salvage".into(),
                obj_type: item_type::TINKERING_MATERIAL,
                material: Some(material),
                workmanship: Some(work),
                structure: Some(0),
                ..ScriptedObject::default()
            });
            let had = bag.structure.unwrap_or(0);
            let put = units.min(BAG_UNITS - had);
            let total = had + put;
            bag.workmanship = Some(
                (bag.workmanship.unwrap_or(work) * f32::from(had) + work * f32::from(put))
                    / f32::from(total.max(1)),
            );
            bag.structure = Some(total);
            bag.value = u32::from(total) * 10;
            bag.name = format!("Salvage ({total})");
            if bag.id == 0 {
                made.push(self.make(bag));
            } else {
                self.remake(&bag);
            }
            units -= put;
        }
    }

    /// The vendor shows its wares again, a moment later, then the use is acknowledged: the
    /// server's answer to a sale or a purchase, which ends the client's request.
    fn vendor_answers(&mut self, vendor: ObjectId) {
        let Some(s) = &self.scripted else {
            return;
        };
        let Some(v) = s.object(vendor).and_then(|o| o.vendor) else {
            return;
        };
        let at = self.now + SCRIPTED_LATENCY;
        for e in [
            ui_event(&s.vendor_info(vendor, v)),
            ui_event(&dereth_protocol::objects::ItemUseDone { failure_type: 0 }),
        ] {
            let i = self.replies.partition_point(|(t, _)| *t <= at);
            self.replies.insert(i, (at, e));
        }
    }

    /// The player types `line` into the chat bar: a claimed command reaches the plugin at the next
    /// frame; anything else is ignored here.
    pub fn type_line(&mut self, line: &str) {
        if let Some(c) = claim_line(line, &self.verbs, self.interaction.command_table()) {
            self.typed.push(c);
        }
        self.frame();
    }

    fn input(&mut self, plugin: &str, path: &str, input: impl FnOnce(String) -> UiInput) {
        let (window, node) = path
            .split_once('.')
            .unwrap_or_else(|| panic!("{path}: want window.node"));
        self.host
            .ui_input(plugin, window, input(node.to_owned()))
            .unwrap_or_else(|e| panic!("{plugin} {path}: {e}"));
        self.frame();
    }

    /// A click on a window node, `window.node`.
    ///
    /// # Panics
    /// Panics when there is no such window or node.
    pub fn click(&mut self, plugin: &str, path: &str) {
        self.input(plugin, path, |node| UiInput::Click { node });
    }

    /// A toggle or other input changed to `value`, `window.node`.
    ///
    /// # Panics
    /// Panics when there is no such window or node.
    pub fn set(&mut self, plugin: &str, path: &str, value: Value) {
        self.input(plugin, path, |node| UiInput::Change { node, value });
    }

    /// The player chooses rows of a list (from 1), or a tab of a `tabs` node, `window.node`.
    pub fn choose(&mut self, plugin: &str, path: &str, rows: Vec<i64>) {
        self.input(plugin, path, |node| UiInput::Select { node, rows });
    }

    /// The player answers the host's question, by its id: `yes` or no, with the line `pick`
    /// picked.
    ///
    /// # Panics
    /// Panics when no question is up, or the host refuses the answer.
    pub fn answer(&mut self, yes: bool, pick: Option<&str>) {
        let q = self.host.question().expect("a question is up");
        let reply = dereth_client_runtime::dialogs::HostReply {
            id: q.id,
            yes,
            picked: pick.and_then(|p| q.picks.iter().position(|l| l == p)),
        };
        self.host
            .reply(&reply)
            .unwrap_or_else(|e| panic!("the answer to {:?}: {e}", q.title));
        self.frame();
    }

    /// A plugin's console lines.
    #[must_use]
    pub fn console(&self, plugin: &str) -> Vec<String> {
        self.host.console(plugin).unwrap_or_default()
    }

    /// The local lines the plugins printed, in order.
    #[must_use]
    pub fn printed(&self) -> Vec<String> {
        self.printed.iter().map(|m| m.body.clone()).collect()
    }

    /// The incoming lines the plugins hid, in order.
    #[must_use]
    pub fn hidden(&self) -> Vec<String> {
        self.hidden.iter().map(|m| m.body.clone()).collect()
    }

    /// The messages the client sent for the plugins, as [`intent_text`] lines, in order.
    #[must_use]
    pub fn sent(&self) -> Vec<String> {
        self.sent.iter().map(intent_text).collect()
    }

    /// Every window of every plugin, as the host keeps it: `window <plugin> <id> open=<bool>` and
    /// then the tree.
    #[must_use]
    pub fn ui_dump(&self) -> String {
        let mut out = String::new();
        for (p, w, win) in self.host.windows() {
            out.push_str(&format!("window {p} {w} open={}", win.open));
            if let Some((sw, sh)) = win.size {
                out.push_str(&format!(" size={sw}x{sh}"));
            }
            out.push('\n');
            out.push_str(&win.tree.dump());
        }
        out
    }

    /// Every open window laid out as a host with no display lays it out (each character a cell), at
    /// the size its plugin keeps for it or else its natural size: `window <plugin> <window>`, then
    /// its nodes' places and sizes (`dereth_plugin_api::layout::dump`).
    #[must_use]
    pub fn layout_dump(&self) -> String {
        let mut out = String::new();
        for (p, w, win) in self.host.windows() {
            if !win.open {
                continue;
            }
            out.push_str(&format!("window {p} {w}\n"));
            out.push_str(&dereth_plugin_api::layout::reference_dump(
                &win.tree, win.size,
            ));
        }
        out
    }

    /// The plugin bar, `id\tstate\topen|closed` each.
    #[must_use]
    pub fn bar(&self) -> Vec<String> {
        self.host
            .bar()
            .into_iter()
            .map(|e| {
                format!(
                    "{}\t{}\t{}",
                    e.id,
                    e.state,
                    if e.open { "open" } else { "closed" }
                )
            })
            .collect()
    }

    /// The player clicks `plugin`'s button on the plugin bar, and a frame runs.
    pub fn bar_click(&mut self, plugin: &str) {
        if let Err(e) = self.host.toggle_window(plugin) {
            panic!("the bar click went nowhere: {e}");
        }
        self.frame();
    }

    /// The player resizes `plugin`'s `window` to `width` cells by `height` lines, and a frame runs.
    pub fn resize(&mut self, plugin: &str, window: &str, width: i64, height: i64) {
        if let Err(e) = self.host.ui_input(
            plugin,
            window,
            dereth_plugin_host::UiInput::Resize { width, height },
        ) {
            panic!("the resize went nowhere: {e}");
        }
        self.frame();
    }

    /// A plugin's status word, or `absent`.
    #[must_use]
    pub fn status(&self, plugin: &str) -> String {
        self.host
            .plugins()
            .into_iter()
            .find(|p| p.id == plugin)
            .map_or_else(|| "absent".to_owned(), |p| p.status)
    }

    /// The player uses object `id` (opens a corpse or a chest, say), as a double click on it does:
    /// the request goes to the scripted server, which answers it, and a frame runs.
    pub fn player_uses(&mut self, id: u32) {
        self.interaction.queue(
            Vec::new(),
            vec![dereth_client_contract::UiRequest::Use(ObjectId(id))],
        );
        let _ =
            self.interaction
                .run_ui_requests(&mut self.objects.world, true, ServerTime(self.now));
        self.frame();
    }

    /// Select object `id`, as a click in the world does.
    pub fn select(&mut self, id: u32) {
        self.objects.world.selected = Some(ObjectId(id));
        self.frame();
    }

    /// The items the scripted character holds, in the order its packs show them (the main pack,
    /// then each side pack): id, name and stack.
    #[must_use]
    pub fn held_items(&self) -> Vec<(u32, String, u32)> {
        let w = &self.objects.world;
        let Some(player) = w.player else {
            return Vec::new();
        };
        let mut packs = vec![player];
        let mut out = Vec::new();
        if let Some(inv) = w.tables.inventories.get(player) {
            packs.extend(inv.containers.iter().copied());
        }
        for p in packs {
            let Some(inv) = w.tables.inventories.get(p) else {
                continue;
            };
            for id in &inv.items {
                if let Some(x) = w.weenie(*id) {
                    out.push((
                        id.0,
                        x.pwd.name.clone(),
                        u32::from(x.pwd.stack_size.unwrap_or(1)),
                    ));
                }
            }
        }
        out
    }

    /// The salvage bags the scripted character holds: id, units and workmanship.
    #[must_use]
    pub fn salvage_bags(&self) -> Vec<(u32, u32, f64)> {
        let w = &self.objects.world;
        self.held_items()
            .into_iter()
            .filter_map(|(id, _, _)| {
                let x = w.weenie(ObjectId(id))?;
                (x.pwd.obj_type & dereth_rules::weenie::item_type::TINKERING_MATERIAL != 0).then(
                    || {
                        (
                            id,
                            u32::from(x.pwd.structure.unwrap_or(0)),
                            f64::from(x.pwd.workmanship.unwrap_or(0.0)),
                        )
                    },
                )
            })
            .collect()
    }

    /// Where the scripted world's item `id` is now: its container, or nil.
    #[must_use]
    pub fn container_of(&self, id: u32) -> Option<u32> {
        self.objects
            .world
            .weenie(ObjectId(id))
            .and_then(|w| w.pwd.container_id)
            .map(|c| c.0)
    }
}
