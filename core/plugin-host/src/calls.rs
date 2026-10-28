//! The host operations a plugin reaches through its bindings: one per API function, each checking
//! its arguments, the world's policy and the rate ceilings before it acts.

use std::collections::BTreeMap;

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::view::Vital;
use dereth_client_runtime::frame_hook::{HookFrame, NavKey};
use dereth_client_runtime::travel::{TravelAllow, TravelTarget};
use dereth_plugin_api::event;
use dereth_plugin_api::{
    ApiVersion, AutoDomain, CallError, HostCalls, HostCapability, Permission, UiNode, Value,
};
use dereth_primitives::{ObjectId, Position};

use crate::observe::chat_type;
use crate::places::{raised, SIGHT_HEIGHT};
use crate::slot::{SlotState, Timer, Window};
use crate::HostConfig;

/// What a plugin asked of one of the game's panels: the plugin, whether it is hidden, and where
/// its top left is to be (screen pixels).
pub type RetailPanel = (String, Option<bool>, Option<(i64, i64)>);

/// One panel's request as [`crate::PluginHost::retail_ui`] lists it: the panel's name first.
pub type RetailRequest = (String, String, Option<bool>, Option<(i64, i64)>);

/// The game's own panels a plugin may hide or move (`retail_ui`).
pub const RETAIL_PANELS: &[&str] = &[
    "chat",
    "vitals",
    "side_vitals",
    "toolbar",
    "radar",
    "panels",
    "environment",
    "combat",
    "power_bar",
    "examination",
];

/// The physics state's ethereal bit: set on a door while it stands open.
const ETHEREAL: u32 = 0x0000_0004;

/// The most local chat lines one plugin may print in one frame.
pub const PRINTS_PER_FRAME: usize = 50;

/// The shortest period of a repeating timer: the 20 Hz tick.
pub const MIN_PERIOD_MS: f64 = 50.0;

/// The most timers one plugin may have at once (a sleeping task holds one).
pub const MAX_TIMERS: usize = 256;

/// The most bytes one store scope may hold, as JSON.
pub const SCOPE_QUOTA: usize = 16 << 20;

/// What a frame's calls produce beyond one plugin's own state.
#[derive(Debug, Default)]
pub struct Shared {
    pub next_timer: i64,
    /// An event answering one of the player's gestures is being delivered: a window the plugin
    /// opens now is the player's.
    pub answering: bool,
    /// The interface's remembered places and sizes for the character in the world (the plugin
    /// bar's place, each window's size and place), by key; `layout_dirty` when it is to be saved.
    pub layout: BTreeMap<String, Value>,
    pub layout_dirty: bool,
    /// Local lines to print, in order.
    pub printed: Vec<ChatMessage>,
    /// This frame's chat lines to hide, by index.
    pub hidden: Vec<usize>,
    /// Every claimed verb and its plugin, to refuse a second claim.
    pub verbs: BTreeMap<String, String>,
    pub verbs_changed: bool,
    /// The character whose store scope is open.
    pub character: Option<String>,
    /// The world and the account logged in, for the store scopes.
    pub world: Option<String>,
    pub account: Option<String>,
    /// Every plugin's actions.
    pub actions: crate::actions::ActionQueue,
    /// The movement in progress.
    pub nav: crate::nav::Nav,
    /// The session before and around the world.
    pub session: crate::session::Session,
    /// Messages for the other clients, sent at the end of the frame.
    pub peers_out: Vec<crate::peers::PeerMessage>,
    /// Requests to start at the end of the frame.
    pub net_out: Vec<(i64, crate::net::HttpRequest)>,
    /// Requests outstanding: the asking plugin, and whether the answer is an image.
    pub net_pending: BTreeMap<i64, (String, bool)>,
    pub next_marker: i64,
    pub next_image: i64,
    /// Lines to send, for this frame's interaction step.
    pub chat_out: Vec<String>,
    /// An attack to stop, for this frame's interaction step.
    pub abort_attack: bool,
    /// An attack to stop if the client is still attacking: an unloaded or suspended plugin's.
    pub abort_attack_if_attacking: bool,
    /// When each object's last appraisal arrived, on the frame clock, with its delivery number;
    /// and the appraisal count last looked at.
    pub appraised_at: BTreeMap<u32, (u64, f64)>,
    pub appraisals_seen: u64,
    /// The toasts showing: the plugin's name, the text, and when it goes (frame clock).
    pub toasts: Vec<(String, String, f64)>,
    pub toasts_changed: bool,
    /// The files plugins asked the player for this frame.
    pub import_requests: Vec<crate::import::Request>,
    /// What plugins asked of the game's own panels: panel to (plugin, hidden, place).
    pub retail_ui: BTreeMap<String, RetailPanel>,
    /// Service calls made this frame, delivered at the next.
    pub service_calls: Vec<crate::services::ServiceCall>,
    /// Calls delivered and not yet answered, by id.
    pub service_pending: BTreeMap<i64, crate::services::Pending>,
    /// Every running, active plugin's services as they stood at the frame's start: provider,
    /// service and the callers it allows.
    pub service_index: Vec<(String, String, Option<std::collections::BTreeSet<String>>)>,
}

/// The most toasts one plugin shows at once, and the longest a toast shows, in seconds.
pub const TOASTS_PER_PLUGIN: usize = 3;
pub const TOAST_MAX_SECS: f64 = 30.0;

impl Shared {
    /// The store scopes open now: everywhere, then the world and the character once logged in.
    #[must_use]
    pub fn scope_keys(&self) -> Vec<String> {
        let mut keys = vec!["global".to_owned()];
        if let Some(w) = &self.world {
            keys.push(format!("world:{w}"));
            if let Some(c) = &self.character {
                let account = self.account.as_deref().unwrap_or("");
                keys.push(format!("character:{w}:{account}:{c}"));
            }
        }
        keys
    }
}

/// One plugin's entry into its VM: the host lends this as the plugin's [`HostCalls`].
#[derive(Debug)]
pub struct Call<'a, 'f> {
    pub slot: &'a mut SlotState,
    pub frame: Option<&'a HookFrame<'f>>,
    pub shared: &'a mut Shared,
    pub config: &'a HostConfig,
    pub prints: usize,
    /// The number of this frame's chat lines, for `chat.hide`.
    pub chat_lines: usize,
    /// The plugins' data folders.
    pub files: &'a mut dyn crate::storage::PluginFiles,
    /// The game's data files.
    pub data: &'a dyn crate::gamedata::PluginGameData,
}

/// The most bytes one data file a plugin saves may hold, and the most files its data folder may.
pub const DATA_FILE_MAX: usize = 4 << 20;
pub const DATA_FILES_MAX: usize = 1000;

fn bad(msg: impl Into<String>) -> CallError {
    CallError::BadArgs(msg.into())
}

fn arg_str<'v>(args: &'v [Value], i: usize, what: &str) -> Result<&'v str, CallError> {
    args.get(i)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{what} must be a string")))
}

/// A duration given in milliseconds, as seconds within `min`..=`max` seconds.
fn millis(v: &Value, min: f64, max: f64, what: &str) -> Result<f64, CallError> {
    v.as_f64()
        .filter(|ms| ms.is_finite() && *ms / 1000.0 >= min && *ms / 1000.0 <= max)
        .map(|ms| ms / 1000.0)
        .ok_or_else(|| {
            bad(format!(
                "{what} is from {} to {} milliseconds",
                min * 1000.0,
                max * 1000.0
            ))
        })
}

/// A flag that may be left out (false).
fn flag(v: &Value, what: &str) -> Result<bool, CallError> {
    match v {
        Value::Nil => Ok(false),
        v => v
            .as_bool()
            .ok_or_else(|| bad(format!("{what} is true or false"))),
    }
}

fn arg_id(args: &[Value], i: usize) -> Result<ObjectId, CallError> {
    args.get(i)
        .and_then(Value::as_i64)
        .and_then(|v| u32::try_from(v).ok())
        .map(ObjectId)
        .ok_or_else(|| bad("an object id must be a whole number"))
}

impl Call<'_, '_> {
    fn frame(&self, op: &str) -> Result<&HookFrame<'_>, CallError> {
        self.frame.ok_or_else(|| {
            CallError::Failed(format!(
                "{op} is not available while the plugin loads outside the game"
            ))
        })
    }

    fn now(&self) -> f64 {
        self.frame.map_or(0.0, |f| f.local_time)
    }

    fn player_get(&self, key: &str) -> Result<Value, CallError> {
        let f = self.frame("game.player")?;
        let view = f.view;
        let Some(id) = view.player() else {
            return Ok(Value::Nil);
        };
        let vital = |v: Vital| {
            view.vital(id, v).map_or(Value::Nil, |(cur, max)| {
                Value::map().with("current", cur).with("max", max)
            })
        };
        Ok(match key {
            "id" => Value::from(id.0),
            "name" => Value::from(view.character_name().map(str::to_owned)),
            "level" => f
                .world
                .player_qualities()
                .map_or(Value::Nil, |q| Value::from(q.inq_int(25))),
            "health" => vital(Vital::Health),
            "stamina" => vital(Vital::Stamina),
            "mana" => vital(Vital::Mana),
            "xp" => {
                crate::observe::experience(f.world).map_or(Value::Nil, |(total, unassigned)| {
                    Value::map()
                        .with("total", total)
                        .with("unassigned", unassigned)
                        .with(
                            "to_level",
                            view.experience_header()
                                .map(|h| i64::try_from(h.to_level).unwrap_or(i64::MAX)),
                        )
                })
            }
            "luminance" => {
                let (earned, max) = view.luminance();
                Value::map().with("earned", earned).with("max", max)
            }
            "burden" => Value::from(view.load()),
            "coords" => view
                .player_coords()
                .map_or(Value::Nil, |(x, y)| Value::map().with("x", x).with("y", y)),
            "heading" => f
                .places
                .player()
                .map_or(Value::from(view.player_heading()), |p| {
                    Value::from((crate::places::heading(&p) * 10.0).round() / 10.0)
                }),
            "position" => f
                .places
                .player()
                .map_or(Value::Nil, |p| crate::places::to_value(&p)),
            "autorun" => Value::from(f.places.autorun()),
            "outside" => Value::from(view.player_outside()),
            "combat_mode" => Value::from(view.combat_mode()),
            "vitae" => Value::from(view.vitae()),
            "skills" => Value::List(
                view.skills()
                    .iter()
                    .map(|s| {
                        Value::map()
                            .with("id", s.id)
                            .with("name", s.name.as_str())
                            .with(
                                "training",
                                match s.sac {
                                    2 => "trained",
                                    3 => "specialized",
                                    _ => "untrained",
                                },
                            )
                            .with("base", s.level)
                            .with("current", s.effective)
                    })
                    .collect(),
            ),
            "enchantments" => Value::List(
                view.active_effects()
                    .into_iter()
                    .map(|e| {
                        Value::map()
                            .with("spell", e.spell)
                            .with("name", e.name.as_str())
                            .with("beneficial", e.beneficial)
                            .with("remaining", (!e.permanent).then_some(e.remaining))
                            .with("category", view.spell_category(e.spell))
                    })
                    .collect(),
            ),
            other => crate::character::field(f, other).unwrap_or_default(),
        })
    }

    fn object_get(&self, id: ObjectId, key: &str) -> Result<Value, CallError> {
        let f = self.frame("an object handle")?;
        let Some(w) = f.world.weenie(id) else {
            return Ok(Value::Nil);
        };
        let d = &w.pwd;
        Ok(match key {
            "id" => Value::from(id.0),
            "name" => Value::from(d.name.as_str()),
            "icon" => Value::from(d.icon_id),
            "type" => Value::from(d.obj_type),
            "wcid" => Value::from(d.wcid),
            "container" => Value::from(d.container_id.map(|c| c.0)),
            "wielder" => Value::from(d.wielder_id.map(|c| c.0)),
            "stack" => Value::from(d.stack_size.map(u32::from)),
            "value" => Value::from(d.value),
            "burden" => Value::from(d.burden.map(u32::from)),
            "selected" => Value::from(w.selected),
            "position" => f
                .places
                .object(id)
                .map_or(Value::Nil, |p| crate::places::to_value(&p)),
            "class" => Value::from(crate::reads::class_of(w).1),
            "class_id" => Value::from(crate::reads::class_of(w).0),
            "uses" => Value::from(d.structure.map(u32::from)),
            "max_uses" => Value::from(d.max_structure.map(u32::from)),
            "max_stack" => Value::from(d.max_stack_size.map(u32::from)),
            "slot" => Value::from(d.location),
            "material" => Value::from(d.material_type),
            "workmanship" => Value::from(d.workmanship.map(f64::from)),
            "plural" => Value::from(d.plural_name.clone()),
            "monarch" => Value::from(d.monarch.map(|c| c.0)),
            "spells" => Value::List(
                crate::reads::spells(f.world, id)
                    .into_iter()
                    .map(Value::from)
                    .collect(),
            ),
            "appraised" => Value::from(f.world.appraisal.get(id).is_some()),
            "appraised_at" => Value::from(
                self.shared
                    .appraised_at
                    .get(&id.0)
                    .map(|(_, t)| *t - self.slot.loaded_at),
            ),
            "open" => Value::from(f.world.physics(id).map(|p| p.state & ETHEREAL != 0)),
            _ => Value::Nil,
        })
    }

    fn world_find(&self, filter: &Value) -> Result<Value, CallError> {
        let f = self.frame("game.world:find")?;
        let name = filter.get("name").as_str();
        let contains = filter.get("name_contains").as_str().map(str::to_lowercase);
        let container = filter.get("container").as_i64();
        let kind = filter.get("type").as_i64();
        let wcid = filter.get("wcid").as_i64();
        let wielder = filter.get("wielder").as_i64();
        let class = filter.get("class");
        let conditions = crate::reads::Condition::list(filter.get("where")).map_err(bad)?;
        let limit = match filter.get("limit") {
            Value::Nil => None,
            v => Some(
                v.as_i64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| bad("limit is a whole number"))?,
            ),
        };
        let nearest_first = match filter.get("sort") {
            Value::Nil => false,
            v if v.as_str() == Some("distance") => true,
            _ => {
                return Err(bad(
                    "sort is \"distance\" (nearest first), or absent (id order)",
                ))
            }
        };
        let within = match filter.get("within") {
            Value::Nil => None,
            v => Some(
                v.as_f64()
                    .filter(|d| d.is_finite() && *d >= 0.0)
                    .ok_or_else(|| bad("within is a number of metres"))?,
            ),
        };
        let monster = filter.get("monster").as_bool();
        for k in filter.as_map().into_iter().flatten().map(|(k, _)| k) {
            if ![
                "name",
                "name_contains",
                "container",
                "type",
                "within",
                "monster",
                "wcid",
                "wielder",
                "class",
                "where",
                "limit",
                "sort",
            ]
            .contains(&k.as_str())
            {
                return Err(bad(format!("game.world:find has no filter {k:?}")));
            }
        }
        let me = f.places.player();
        let player = f.world.player;
        // A world that asks for parity: only what the radar shows, and what the player holds.
        let parity: Option<std::collections::BTreeSet<ObjectId>> =
            self.config.policy.read_parity.then(|| {
                f.view
                    .radar_objects()
                    .iter()
                    .map(|r| r.id)
                    .chain(crate::observe::held_items(f).into_keys().map(ObjectId))
                    .collect()
            });
        let mut ids: Vec<u32> = f
            .world
            .tables
            .weenies
            .iter()
            .filter(|(_, w)| {
                let d = &w.pwd;
                name.is_none_or(|n| d.name == n)
                    && contains
                        .as_deref()
                        .is_none_or(|c| d.name.to_lowercase().contains(c))
                    && container.is_none_or(|c| d.container_id.map(|x| i64::from(x.0)) == Some(c))
                    && kind.is_none_or(|t| i64::from(d.obj_type) == t)
                    && wcid.is_none_or(|c| i64::from(d.wcid) == c)
                    && wielder.is_none_or(|c| d.wielder_id.map(|x| i64::from(x.0)) == Some(c))
                    && match class {
                        Value::Nil => true,
                        Value::Str(s) => crate::reads::class_of(w).1.eq_ignore_ascii_case(s),
                        c => c.as_i64() == Some(i64::from(crate::reads::class_of(w).0)),
                    }
                    && monster.is_none_or(|m| {
                        // A creature the character can attack that is not a player: not the
                        // town's people.
                        use dereth_rules::weenie::bitfield;
                        let is = d.obj_type & dereth_rules::weenie::item_type::CREATURE != 0
                            && d.bitfield & bitfield::PLAYER == 0
                            && d.bitfield & bitfield::ATTACKABLE != 0;
                        is == m
                    })
            })
            .filter(|(k, _)| Some(*k) != player || monster.is_none())
            .filter(|(k, _)| parity.as_ref().is_none_or(|p| p.contains(k)))
            .filter(|(k, _)| {
                within.is_none_or(|r| {
                    me.zip(f.places.object(*k))
                        .is_some_and(|(a, b)| crate::places::distance(&a, &b) <= r)
                })
            })
            .filter(|(k, _)| conditions.iter().all(|c| c.holds(f.world, *k)))
            .map(|(k, _)| k.0)
            .collect();
        ids.sort_unstable();
        if nearest_first {
            let away = |id: u32| {
                me.zip(f.places.object(ObjectId(id)))
                    .map_or(f64::INFINITY, |(a, b)| crate::places::distance(&a, &b))
            };
            ids.sort_by(|a, b| away(*a).total_cmp(&away(*b)).then(a.cmp(b)));
        }
        if let Some(n) = limit {
            ids.truncate(n);
        }
        Ok(Value::List(ids.into_iter().map(Value::from).collect()))
    }

    fn store_scope(&self, args: &[Value]) -> Result<String, CallError> {
        let keys = self.shared.scope_keys();
        let find = |prefix: &str| keys.iter().find(|k| k.starts_with(prefix)).cloned();
        match arg_str(args, 0, "the store scope")? {
            "global" => Ok("global".to_owned()),
            "world" => find("world:").ok_or_else(|| {
                CallError::Failed("store.world is available once a character is logged in".into())
            }),
            "character" => find("character:").ok_or_else(|| {
                CallError::Failed(
                    "store.character is available once a character is logged in".into(),
                )
            }),
            other => Err(bad(format!(
                "store.{other} is not in this version of the API"
            ))),
        }
    }

    /// Whether the world stops automation now because the character is a player killer (its
    /// policy's `pk_automation = false`).
    fn pk_blocked(&self) -> bool {
        !self.config.policy.pk_automation
            && self
                .frame
                .is_some_and(|f| f.view.pk_status() != dereth_client_contract::view::PkStatus::Npk)
    }

    /// Whether an action in `domain` may run now: `Ok(None)` to go ahead, or `Ok(Some(id))` when
    /// the world forbids it (the id of its promise, settled failed at the next frame).
    fn gate(&mut self, name: &'static str, domain: AutoDomain) -> Result<Option<i64>, CallError> {
        let interactive = self.slot.activation_until >= self.now();
        if self.config.policy.allows(domain, interactive) && (interactive || !self.pk_blocked()) {
            Ok(None)
        } else {
            let me = self.slot.manifest.id.to_string();
            Ok(Some(self.shared.actions.refuse(
                &me,
                name,
                "denied_by_world",
            )))
        }
    }

    /// A movement no gesture asked for, just after the player moved the character themselves:
    /// refused (`player_moving`), since the player's own movement comes first.
    fn yield_to_player(&mut self, name: &'static str) -> Option<i64> {
        let now = self.now();
        let interactive = self.slot.activation_until >= now;
        (!interactive && self.shared.nav.player_recently_moved(now)).then(|| {
            let me = self.slot.manifest.id.to_string();
            self.shared.actions.refuse(&me, name, "player_moving")
        })
    }

    fn capability(&self, op: &str, cap: HostCapability) -> Result<(), CallError> {
        if self.config.capabilities.contains(&cap) {
            Ok(())
        } else {
            Err(CallError::Failed(format!("this host has no {cap} ({op})")))
        }
    }

    /// A movement of the player's character (see [`crate::nav`]): answers its promise's id.
    fn nav(&mut self, name: &str, args: &Value) -> Result<Value, CallError> {
        if name == "nav.path" {
            return self.path(args);
        }
        self.capability(name, HostCapability::NavMoveto)?;
        let range = |d: f64| -> Result<f64, CallError> {
            match args.get("range") {
                Value::Nil => Ok(d),
                v => v
                    .as_f64()
                    .filter(|r| r.is_finite() && *r >= 0.0)
                    .ok_or_else(|| bad("a range is a number of metres")),
            }
        };
        let goal = match name {
            "nav.stop" => {
                let me = self.slot.manifest.id.to_string();
                if let Some(s) = self.shared.nav.stop(&me) {
                    self.shared.actions.settle_later(s);
                }
                return Ok(Value::Nil);
            }
            "nav.autorun" => {
                let on = args
                    .get("on")
                    .as_bool()
                    .ok_or_else(|| bad("nav.autorun takes true or false"))?;
                // Letting go of the run lock is always allowed (it touches only the lock this
                // plugin set); setting it is an action like any other.
                if on {
                    if let Some(refused) = self.gate("nav.autorun", AutoDomain::Nav)? {
                        return Ok(Value::Int(refused));
                    }
                    if let Some(refused) = self.yield_to_player("nav.autorun") {
                        return Ok(Value::Int(refused));
                    }
                }
                let me = self.slot.manifest.id.to_string();
                self.shared.nav.ask_run(&me, on);
                let id = self.shared.actions.allocate();
                self.shared.actions.settle_later(crate::actions::Settled {
                    plugin: me,
                    id,
                    action: "nav.autorun",
                    result: Ok(Value::Bool(on)),
                });
                return Ok(Value::Int(id));
            }
            "nav.path_to" | "nav.plan" => return self.journey(name, args),
            "nav.move_to" => {
                let target = match (args.get("object"), args.get("position")) {
                    (o, Value::Nil) if !o.is_nil() => {
                        crate::overlay::Anchor::Object(arg_id(std::slice::from_ref(o), 0)?)
                    }
                    (Value::Nil, p) if !p.is_nil() => {
                        crate::overlay::Anchor::Place(crate::places::from_value(p)?)
                    }
                    _ => return Err(bad("nav.move_to goes to an object or a position")),
                };
                crate::nav::Goal::MoveTo {
                    target,
                    range: range(2.0)?,
                }
            }
            "nav.follow" => crate::nav::Goal::Follow {
                object: arg_id(std::slice::from_ref(args.get("object")), 0)?,
                range: range(3.0)?,
            },
            "nav.face" => crate::nav::Goal::Face {
                heading: args
                    .get("heading")
                    .as_f64()
                    .filter(|h| h.is_finite())
                    .ok_or_else(|| bad("nav.face takes a heading in degrees"))?
                    .rem_euclid(360.0),
            },
            "nav.jump" => crate::nav::Goal::Jump {
                heading: match args.get("heading") {
                    Value::Nil => None,
                    v => Some(
                        v.as_f64()
                            .filter(|h| h.is_finite())
                            .ok_or_else(|| bad("a jump's heading is in degrees"))?
                            .rem_euclid(360.0),
                    ),
                },
                hold: millis(args.get("ms"), 0.0, crate::nav::JUMP_MAX_SECS, "a jump's ms")?,
                walk: flag(args.get("walk"), "a jump's walk")?,
                direction: match args.get("direction") {
                    Value::Nil => None,
                    v => Some(match v.as_str() {
                        Some("forward") => NavKey::Forward,
                        Some("backward") => NavKey::Backward,
                        Some("strafe_left") => NavKey::StrafeLeft,
                        Some("strafe_right") => NavKey::StrafeRight,
                        _ => {
                            return Err(bad(
                                "a jump's direction is forward, backward, strafe_left or strafe_right",
                            ))
                        }
                    }),
                },
            },
            "nav.motion" => crate::nav::Goal::Motion {
                key: match args.get("kind").as_str() {
                    Some("forward") => NavKey::Forward,
                    Some("backward") => NavKey::Backward,
                    Some("strafe_left") => NavKey::StrafeLeft,
                    Some("strafe_right") => NavKey::StrafeRight,
                    Some("turn_left") => NavKey::TurnLeft,
                    Some("turn_right") => NavKey::TurnRight,
                    _ => {
                        return Err(bad(
                            "a motion is forward, backward, strafe_left, strafe_right, turn_left or turn_right",
                        ))
                    }
                },
                secs: millis(args.get("ms"), 0.05, crate::nav::MOTION_MAX_SECS, "a motion's ms")?,
                walk: flag(args.get("walk"), "a motion's walk")?,
            },
            other => return Err(CallError::Unknown(other.to_owned())),
        };
        if let Some(refused) = self.gate(goal.action(), AutoDomain::Nav)? {
            return Ok(Value::Int(refused));
        }
        if let Some(refused) = self.yield_to_player(goal.action()) {
            return Ok(Value::Int(refused));
        }
        let id = self.shared.actions.allocate();
        let me = self.slot.manifest.id.to_string();
        if let Some(s) = self.shared.nav.start(&me, id, goal, self.now()) {
            self.shared.actions.settle_later(s);
        }
        Ok(Value::Int(id))
    }

    /// Whether the world lets an action in `domain` run now: what a journey asks to know before
    /// it plans legs of that domain.
    fn may(&self, domain: AutoDomain) -> bool {
        let interactive = self.slot.activation_until >= self.now();
        self.config.policy.allows(domain, interactive) && (interactive || !self.pk_blocked())
    }

    /// Where a journey goes: an object, a place, a cell, or a place this world's store learned (a
    /// portal's destination by its name, or a recall's landing).
    fn travel_target(&self, args: &Value) -> Result<TravelTarget, CallError> {
        let store = self.frame.and_then(|f| f.travel_store);
        if !args.get("object").is_nil() {
            return Ok(TravelTarget::Object(arg_id(
                std::slice::from_ref(args.get("object")),
                0,
            )?));
        }
        if !args.get("position").is_nil() {
            return Ok(TravelTarget::Place(crate::places::from_value(
                args.get("position"),
            )?));
        }
        if let Some(cell) = args.get("cell").as_i64() {
            return u32::try_from(cell)
                .map(|c| TravelTarget::Cell(dereth_primitives::CellId(c)))
                .map_err(|_| bad("a cell is a cell id"));
        }
        if let Some(name) = args.get("portal").as_str() {
            let store = store.ok_or_else(|| bad("no world's portals are known yet"))?;
            return store
                .portals()
                .iter()
                .filter(|e| e.name.eq_ignore_ascii_case(name) && e.retired.is_none())
                .max_by_key(|e| (e.source, e.verified))
                .and_then(|e| e.to)
                .map(TravelTarget::Place)
                .ok_or_else(|| bad(format!("no portal {name:?} with a known destination")));
        }
        if !args.get("recall").is_nil() {
            let store = store.ok_or_else(|| bad("no world's recalls are known yet"))?;
            let recall = recall_of(args.get("recall"))?;
            let character = self.character_name();
            return store
                .recall_destination(&character, recall)
                .map(TravelTarget::Place)
                .ok_or_else(|| bad(format!("{} has not been used yet", recall.word())));
        }
        Err(bad(
            "a journey goes to an object, a position, a cell, a portal's destination or a recall's",
        ))
    }

    /// The character's name, as its weenie names it (the store keys recalls by it).
    fn character_name(&self) -> String {
        self.frame
            .and_then(|f| f.world.player.and_then(|p| f.world.weenie(p)))
            .map(|w| w.pwd.name.clone())
            .unwrap_or_default()
    }

    /// The walking route to a target, planned by the client and not walked (`nav.path`):
    /// answers the promise's id. Planning moves nothing, so it needs no domain.
    fn path(&mut self, args: &Value) -> Result<Value, CallError> {
        self.capability("nav.path", HostCapability::NavPath)?;
        let target = self.travel_target(args.get("target"))?;
        let me = self.slot.manifest.id.to_string();
        let id = self.shared.actions.allocate();
        self.shared.nav.route(&me, id, target);
        Ok(Value::Int(id))
    }

    /// Whether a straight line from the character (or `from`) to `target` is clear of what stands
    /// in the world (`nav.sight`): each end a metre above the ground the place or object stands
    /// on. Nil where the host cannot tell.
    fn sight(&self, args: &[Value]) -> Result<Value, CallError> {
        self.capability("nav.sight", HostCapability::NavPath)?;
        let f = self.frame("nav.sight")?;
        let end = |v: &Value| -> Result<Option<Position>, CallError> {
            if let Some(id) = v.as_i64().or_else(|| v.get("id").as_i64()) {
                let id = u32::try_from(id).map_err(|_| bad("an object id is a whole number"))?;
                return Ok(f.places.object(dereth_primitives::ObjectId(id)));
            }
            crate::places::from_value(v).map(Some)
        };
        let to = match args.first() {
            Some(v) if !v.is_nil() => end(v)?,
            _ => return Err(bad("nav.sight looks at an object or a place")),
        };
        let from = match args.get(1) {
            Some(v) if !v.is_nil() => end(v)?,
            _ => f.places.player(),
        };
        let (Some(from), Some(to)) = (from, to) else {
            return Ok(Value::Nil);
        };
        Ok(f.places
            .sight(&raised(&from, SIGHT_HEIGHT), &raised(&to, SIGHT_HEIGHT))
            .map_or(Value::Nil, Value::Bool))
    }

    /// A journey (`nav.path_to`) or its plan (`nav.plan`): answers the promise's id.
    fn journey(&mut self, name: &str, args: &Value) -> Result<Value, CallError> {
        self.capability(name, HostCapability::NavJourney)?;
        let target = self.travel_target(args.get("target"))?;
        let opts = args.get("opts");
        let flag = |k: &str| match opts.get(k) {
            Value::Nil => Ok(true),
            v => v
                .as_bool()
                .ok_or_else(|| bad(format!("a journey's {k} is true or false"))),
        };
        let (portals, recalls, show) = (flag("portals")?, flag("recalls")?, flag("show")?);
        // Each kind of leg is an action of its own domain: a portal used, a recall item used and
        // a recall by command are travel (`nav`); a recall cast is a spell on oneself (`buff`).
        let allow = TravelAllow {
            portals,
            spells: recalls && self.may(AutoDomain::Buff),
            items: recalls,
            commands: recalls,
        };
        let me = self.slot.manifest.id.to_string();
        if name == "nav.plan" {
            let id = self.shared.actions.allocate();
            self.shared.nav.plan(&me, id, target, allow);
            return Ok(Value::Int(id));
        }
        if let Some(refused) = self.gate("nav.path_to", AutoDomain::Nav)? {
            return Ok(Value::Int(refused));
        }
        if let Some(refused) = self.yield_to_player("nav.path_to") {
            return Ok(Value::Int(refused));
        }
        let id = self.shared.actions.allocate();
        for s in self.shared.nav.start_journey(&me, id, target, allow, show) {
            self.shared.actions.settle_later(s);
        }
        Ok(Value::Int(id))
    }

    /// Stop attacking: the attack being charged, sent or repeated is cancelled at once, not
    /// behind the queue, and this plugin's attacks in the queue or in flight fail as `stopped`.
    /// Answers its promise's id, settled at the next frame.
    ///
    /// Stopping its own attack is always allowed, with no gesture, permission or leave of the
    /// world: a plugin never leaves the character fighting. Cancelling an attack it did not start
    /// (the player's) is an action in combat like any other.
    fn combat_stop(&mut self) -> Result<Value, CallError> {
        self.frame("combat.stop")?;
        let me = self.slot.manifest.id.to_string();
        let stopped = self.shared.actions.stop_attacks(&me);
        let own = !stopped.is_empty() || self.shared.actions.is_last_attacker(&me);
        if !own {
            if let Some(refused) = self.gate("combat.stop", AutoDomain::Combat)? {
                return Ok(Value::Int(refused));
            }
        }
        for s in stopped {
            self.shared.actions.settle_later(s);
        }
        self.shared.abort_attack = true;
        let id = self.shared.actions.allocate();
        self.shared.actions.settle_later(crate::actions::Settled {
            plugin: me,
            id,
            action: "combat.stop",
            result: Ok(Value::Nil),
        });
        Ok(Value::Int(id))
    }

    /// A pre-game operation (see [`crate::session`]): answers its promise's id.
    fn session_op(&mut self, name: &str, args: &Value) -> Result<Value, CallError> {
        self.capability(name, HostCapability::SessionPregame)?;
        self.frame(name)?;
        let Some(frame) = self.frame else {
            return Ok(Value::Nil);
        };
        let interactive = self.slot.activation_until >= frame.local_time;
        let me = self.slot.manifest.id.to_string();
        let action: &'static str = match name {
            "session.select_character" => "session.select_character",
            "session.logout" => "session.logout",
            "session.quit" => "session.quit",
            other => return Err(CallError::Unknown(other.to_owned())),
        };
        if !self.config.policy.allows_session(interactive) {
            return Ok(Value::Int(self.shared.actions.refuse(
                &me,
                action,
                "denied_by_world",
            )));
        }
        let id = self.shared.actions.allocate();
        if let Some(s) = self.shared.session.request(&me, id, name, args, frame)? {
            self.shared.actions.settle_later(s);
        }
        Ok(Value::Int(id))
    }

    /// A message to this plugin's copies in the other clients (see [`crate::peers`]).
    fn peers_send(&mut self, args: &[Value]) -> Result<Value, CallError> {
        self.capability("peers.send", HostCapability::PeersLocal)?;
        if self.config.policy.peers_local == Some(0) {
            return Err(CallError::Failed(
                "this world allows no coordination between clients".into(),
            ));
        }
        let channel = arg_str(args, 0, "the channel")?.to_owned();
        let data = args.get(1).cloned().unwrap_or_default().to_json();
        if data.len() > crate::peers::MAX_MESSAGE {
            return Err(CallError::Failed(format!(
                "a message is limited to {} bytes",
                crate::peers::MAX_MESSAGE
            )));
        }
        let now = self.now();
        let sends = &mut self.slot.peer_sends;
        while sends.front().is_some_and(|t| now - t >= 1.0) {
            sends.pop_front();
        }
        if sends.len() >= crate::peers::SENDS_PER_SECOND {
            return Err(CallError::Failed(format!(
                "a plugin may send at most {} messages a second",
                crate::peers::SENDS_PER_SECOND
            )));
        }
        sends.push_back(now);
        self.shared.peers_out.push(crate::peers::PeerMessage {
            from: self.shared.character.clone().unwrap_or_default(),
            plugin: self.slot.manifest.id.to_string(),
            channel,
            data,
        });
        Ok(Value::Nil)
    }

    /// A file the player picks (see [`crate::import`]): answers its promise's id.
    fn files_import(&mut self, args: &Value) -> Result<Value, CallError> {
        let extensions = match args.get("extensions") {
            Value::Nil => Vec::new(),
            v => v
                .as_list()
                .ok_or_else(|| bad("extensions is a list of file extensions"))?
                .iter()
                .filter_map(Value::as_str)
                .map(|e| e.trim_start_matches('.').to_owned())
                .collect(),
        };
        let id = self.shared.actions.allocate();
        self.shared.import_requests.push(crate::import::Request {
            plugin: self.slot.manifest.id.to_string(),
            name: self.slot.manifest.name.clone(),
            id,
            prompt: args.get("prompt").as_str().unwrap_or("a file").to_owned(),
            extensions,
        });
        Ok(Value::Int(id))
    }

    /// A call to another plugin's service (see [`crate::services`]): answers its promise's id.
    fn services_call(&mut self, args: &Value) -> Result<Value, CallError> {
        let plugin = args
            .get("plugin")
            .as_str()
            .ok_or_else(|| bad("services.call names the plugin"))?
            .to_owned();
        let service = args
            .get("service")
            .as_str()
            .ok_or_else(|| bad("services.call names the service"))?
            .to_owned();
        let payload = args.get("args").clone();
        if payload.to_json().len() > crate::services::MAX_PAYLOAD {
            return Err(CallError::Failed(format!(
                "a call's arguments are at most {} bytes",
                crate::services::MAX_PAYLOAD
            )));
        }
        let now = self.now();
        let calls = &mut self.slot.service_calls;
        while calls.front().is_some_and(|t| now - t >= 1.0) {
            calls.pop_front();
        }
        if calls.len() >= crate::services::CALLS_PER_SECOND {
            return Err(CallError::Failed(format!(
                "a plugin may call at most {} services a second",
                crate::services::CALLS_PER_SECOND
            )));
        }
        calls.push_back(now);
        let id = self.shared.actions.allocate();
        self.shared
            .service_calls
            .push(crate::services::ServiceCall {
                id,
                caller: self.slot.manifest.id.to_string(),
                provider: plugin,
                service,
                args: payload,
                gesture: self.slot.activation_until >= now,
            });
        Ok(Value::Int(id))
    }

    /// An HTTPS request (see [`crate::net`]): answers its promise's id. A request goes out only
    /// when the manifest names `net.http` and lists the request's domain; otherwise its promise
    /// fails, saying which is missing.
    fn net_fetch(&mut self, args: &Value) -> Result<Value, CallError> {
        self.capability("net.fetch", HostCapability::NetHttp)?;
        let (request, image) = crate::net::request_of(args)?;
        let (host, port) = crate::net::https_authority(&request.url)
            .ok_or_else(|| bad("net.fetch reaches https:// URLs only, to a host name"))?;
        let missing = if !self.slot.has(Permission::NetHttp) {
            Some("this plugin's manifest does not name net.http".to_owned())
        } else if !self.slot.manifest.admits(host, port) {
            Some(format!(
                "{host}:{port} is not among the domains this plugin's manifest lists"
            ))
        } else {
            None
        };
        if let Some(why) = missing {
            let me = self.slot.manifest.id.to_string();
            return Ok(Value::Int(self.shared.actions.refuse(
                &me,
                "net.fetch",
                &why,
            )));
        }
        if self.slot.net_in_flight >= crate::net::IN_FLIGHT {
            return Err(CallError::Failed(format!(
                "a plugin may have at most {} requests outstanding",
                crate::net::IN_FLIGHT
            )));
        }
        self.slot.net_in_flight += 1;
        let id = self.shared.actions.allocate();
        self.shared
            .net_pending
            .insert(id, (self.slot.manifest.id.to_string(), image));
        self.shared.net_out.push((id, request));
        Ok(Value::Int(id))
    }

    /// A marker (see [`crate::overlay`]): answers its id.
    fn overlay_add(&mut self, args: &[Value]) -> Result<Value, CallError> {
        let kind = crate::overlay::Kind::parse(arg_str(args, 0, "the marker kind")?)
            .ok_or_else(|| bad("a marker is world, radar, map, arrow, shape or nametags"))?;
        self.capability("markers", kind.capability())?;
        let m = crate::overlay::Marker::from_value(
            kind,
            args.get(1).unwrap_or(&Value::Nil),
            self.now(),
        )?;
        // One arrow and one nametags layer a plugin: a new one replaces the old.
        if matches!(
            kind,
            crate::overlay::Kind::Arrow | crate::overlay::Kind::Nametags
        ) {
            self.slot.markers.retain(|_, m| m.kind != kind);
        }
        if self.slot.markers.len() >= crate::overlay::MAX_MARKERS {
            return Err(CallError::Failed(format!(
                "a plugin may keep at most {} markers",
                crate::overlay::MAX_MARKERS
            )));
        }
        self.shared.next_marker += 1;
        let id = self.shared.next_marker;
        self.slot.markers.insert(id, m);
        Ok(Value::Int(id))
    }

    /// Queue the action `name` for this plugin (see [`crate::actions`]): answers its id.
    fn act(&mut self, name: &str, args: &Value) -> Result<Value, CallError> {
        let frame = self.frame(name)?;
        let now = frame.local_time;
        let interactive = self.slot.activation_until >= now;
        if !interactive && crate::actions::gesture_only(name) {
            return Err(CallError::Failed(format!(
                "{name} can only follow the player's click or command"
            )));
        }
        let plan = crate::actions::plan(name, args, frame)?;
        let me = self.slot.manifest.id.to_string();
        if interactive {
            // A gesture is behind it: the world's rule for a gesture decides, below.
        } else if let Some(d) = plan.domain {
            if !self.config.policy.allows(d, false) || self.pk_blocked() {
                let id = self
                    .shared
                    .actions
                    .refuse(&me, plan.kind.name(), "denied_by_world");
                return Ok(Value::Int(id));
            }
        }
        if let (true, Some(d)) = (interactive, plan.domain) {
            if !self.config.policy.allows(d, true) {
                let id = self
                    .shared
                    .actions
                    .refuse(&me, plan.kind.name(), "denied_by_world");
                return Ok(Value::Int(id));
            }
        }
        self.shared.actions.push(&me, plan, now).map(Value::Int)
    }

    /// What the loot matcher needs to know about the character, with the names of `item`'s
    /// spells.
    fn loot_character(&self, frame: &HookFrame<'_>, item: ObjectId) -> crate::loot::Character {
        let view = frame.view;
        let mut ch = crate::loot::Character {
            level: frame
                .world
                .player_qualities()
                .map_or(0, |q| i64::from(q.inq_int(25))),
            ..crate::loot::Character::default()
        };
        for s in view.skills() {
            ch.skills
                .insert(s.id, (i64::from(s.effective), i64::from(s.level)));
        }
        if let Some(p) = frame.world.player {
            let cap = view.items_capacity(p).unwrap_or(102);
            let used = i32::try_from(view.container_contents(p).len()).unwrap_or(i32::MAX);
            ch.main_pack_free = i64::from(cap.saturating_sub(used).max(0));
        }
        for s in crate::reads::spells(frame.world, item) {
            if let Some(e) = view.spell(s) {
                ch.spell_names.insert(s, e.name);
            }
        }
        ch
    }

    fn loot_profile(&self, args: &[Value]) -> Result<&crate::loot::Profile, CallError> {
        args.first()
            .and_then(Value::as_i64)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.slot.loot_profiles.get(i))
            .ok_or_else(|| bad("not a loot profile of this plugin"))
    }

    fn window_mut(&mut self, args: &[Value]) -> Result<&mut Window, CallError> {
        let id = arg_str(args, 0, "the window id")?;
        self.slot
            .windows
            .get_mut(id)
            .ok_or_else(|| bad(format!("no window {id:?}")))
    }
}

impl HostCalls for Call<'_, '_> {
    #[allow(clippy::too_many_lines)]
    fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        match op {
            "events.subscribe" => {
                let name = arg_str(args, 0, "the event name")?;
                if name.starts_with(event::MESSAGES_PREFIX) {
                    self.capability(op, HostCapability::Messages)?;
                    if !crate::messages::is_message_event(name) {
                        return Err(bad(format!(
                            "there is no message {name:?}: a message is named as the opcode table \
                             names it, or by its type (messages.0x02CD), or messages.any"
                        )));
                    }
                } else if !event::is_known(name) {
                    return Err(bad(format!(
                        "there is no event {name:?} in this version of the API"
                    )));
                } else if name == "map.clicked" {
                    // Heard only where the host tells the plugins of presses on its map.
                    self.capability(op, HostCapability::MapClicks)?;
                }
                self.slot.subscriptions.insert(name.to_owned());
                Ok(Value::Nil)
            }
            "events.unsubscribe" => {
                self.slot
                    .subscriptions
                    .remove(arg_str(args, 0, "the event name")?);
                Ok(Value::Nil)
            }
            "chat.print" => {
                let text = arg_str(args, 0, "the text")?;
                let ty = match args.get(1).and_then(Value::as_str) {
                    None => 0,
                    Some(style) => {
                        chat_type(style).ok_or_else(|| bad(format!("no chat style {style:?}")))?
                    }
                };
                if self.prints >= PRINTS_PER_FRAME {
                    return Err(CallError::Failed(format!(
                        "a plugin may print at most {PRINTS_PER_FRAME} lines a frame"
                    )));
                }
                self.prints += 1;
                self.shared.printed.push(ChatMessage {
                    feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                    ty,
                    body: text.to_owned(),
                    prefix: None,
                    window: 0,
                });
                Ok(Value::Nil)
            }
            "chat.hide" => {
                let line = args
                    .first()
                    .and_then(Value::as_i64)
                    .and_then(|i| usize::try_from(i).ok());
                match line {
                    Some(i) if i < self.chat_lines => {
                        if !self.shared.hidden.contains(&i) {
                            self.shared.hidden.push(i);
                        }
                        Ok(Value::Nil)
                    }
                    _ => Err(bad(
                        "chat.hide takes a line of this frame (the event's line field)",
                    )),
                }
            }
            "commands.add" => {
                let name = arg_str(args, 0, "the command name")?.to_ascii_lowercase();
                if name.is_empty()
                    || name.len() > 32
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                {
                    return Err(bad(format!(
                        "{name:?} is not a command name (letters, digits, _ and -)"
                    )));
                }
                if name == crate::HOST_VERB {
                    return Err(bad(format!("/{name} is the host's own command")));
                }
                if let Some(f) = self.frame {
                    if f.is_client_command(&name) {
                        return Err(bad(format!("/{name} is one of the client's own commands")));
                    }
                }
                let me = self.slot.manifest.id.to_string();
                if let Some(owner) = self.shared.verbs.get(&name) {
                    if *owner != me {
                        return Err(bad(format!("/{name} is already claimed by {owner}")));
                    }
                }
                let help = args
                    .get(1)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.slot.commands.insert(name.clone(), help);
                self.shared.verbs.insert(name, me);
                self.shared.verbs_changed = true;
                Ok(Value::Nil)
            }
            "commands.remove" => {
                let name = arg_str(args, 0, "the command name")?.to_ascii_lowercase();
                if self.slot.commands.remove(&name).is_some() {
                    self.shared.verbs.remove(&name);
                    self.shared.verbs_changed = true;
                }
                Ok(Value::Nil)
            }
            "task.timer" => {
                let ms = args
                    .first()
                    .and_then(Value::as_f64)
                    .filter(|m| m.is_finite() && *m >= 0.0)
                    .ok_or_else(|| bad("a timer's delay must be a number of milliseconds"))?;
                let repeat = args.get(1).and_then(Value::as_bool).unwrap_or(false);
                if self.slot.timers.len() >= MAX_TIMERS {
                    return Err(CallError::Failed(format!(
                        "a plugin may have at most {MAX_TIMERS} timers at once"
                    )));
                }
                let ms = if repeat { ms.max(MIN_PERIOD_MS) } else { ms };
                self.shared.next_timer += 1;
                let id = self.shared.next_timer;
                self.slot.timers.insert(
                    id,
                    Timer {
                        due: self.now() + ms / 1000.0,
                        every: repeat.then_some(ms / 1000.0),
                    },
                );
                Ok(Value::Int(id))
            }
            "task.cancel" => {
                if let Some(id) = args.first().and_then(Value::as_i64) {
                    self.slot.timers.remove(&id);
                }
                Ok(Value::Nil)
            }
            "task.error" => {
                let msg = arg_str(args, 0, "the error")?;
                let line = format!("error in a task: {msg}");
                self.shared
                    .printed
                    .push(crate::error_line(&self.slot.manifest, &line, false));
                self.slot.log(line);
                let now = self.now();
                self.slot.errors.push_back(now);
                Ok(Value::Nil)
            }
            "store.get" => {
                let scope = self.store_scope(args)?;
                let key = arg_str(args, 1, "the store key")?;
                Ok(self
                    .slot
                    .stores
                    .get(&scope)
                    .and_then(|s| s.values.get(key))
                    .cloned()
                    .unwrap_or_default())
            }
            "store.set" => {
                let scope = self.store_scope(args)?;
                let key = arg_str(args, 1, "the store key")?.to_owned();
                let value = args.get(2).cloned().unwrap_or_default();
                let s = self.slot.stores.entry(scope).or_default();
                let old = s
                    .values
                    .get(&key)
                    .map_or(0, |v| crate::slot::entry_size(&key, v));
                let new = if value.is_nil() {
                    0
                } else {
                    crate::slot::entry_size(&key, &value)
                };
                let size = s.size - old + new;
                let quota = self.config.store_quota;
                if size > quota && new > old {
                    return Err(CallError::Failed(format!(
                        "the store is limited to {quota} bytes"
                    )));
                }
                if value.is_nil() {
                    s.values.remove(&key);
                } else {
                    s.values.insert(key, value);
                }
                s.size = size;
                s.dirty = true;
                Ok(Value::Nil)
            }
            "player.get" => self.player_get(arg_str(args, 0, "the field")?),
            "player.int" => {
                let key = args
                    .first()
                    .and_then(Value::as_i64)
                    .and_then(|k| u32::try_from(k).ok())
                    .ok_or_else(|| bad("an integer property's number"))?;
                Ok(Value::from(
                    self.frame(op)?
                        .world
                        .player_qualities()
                        .map(|q| q.inq_int(key)),
                ))
            }
            "object.exists" => {
                let id = arg_id(args, 0)?;
                Ok(Value::Bool(
                    self.frame("game.world:get")?.world.weenie(id).is_some(),
                ))
            }
            "object.get" => {
                let id = arg_id(args, 0)?;
                self.object_get(id, arg_str(args, 1, "the field")?)
            }
            "object.distance" => {
                let id = arg_id(args, 0)?;
                let places = self.frame(op)?.places;
                Ok(match (places.player(), places.object(id)) {
                    (Some(me), Some(at)) => {
                        Value::from((crate::places::distance(&me, &at) * 100.0).round() / 100.0)
                    }
                    _ => Value::Nil,
                })
            }
            "object.int64" | "object.did" | "object.iid" => {
                let id = arg_id(args, 0)?;
                let key = args
                    .get(1)
                    .and_then(Value::as_i64)
                    .and_then(|k| u32::try_from(k).ok())
                    .ok_or_else(|| bad("a property key must be a whole number"))?;
                let w = self.frame(op)?.world;
                Ok(match op {
                    "object.int64" => Value::from(crate::reads::int64(w, id, key)),
                    "object.did" => Value::from(crate::reads::did(w, id, key)),
                    _ => Value::from(crate::reads::iid(w, id, key)),
                })
            }
            "object.health" => {
                let id = arg_id(args, 0)?;
                let f = self.frame(op)?;
                Ok(crate::reads::health(f.world, f.view, id))
            }
            "object.int" | "object.float" | "object.string" | "object.bool" => {
                let id = arg_id(args, 0)?;
                let key = args
                    .get(1)
                    .and_then(Value::as_i64)
                    .and_then(|k| u32::try_from(k).ok())
                    .ok_or_else(|| bad("a property key must be a whole number"))?;
                let w = self.frame(op)?.world;
                Ok(match op {
                    "object.int" => Value::from(crate::reads::int(w, id, key)),
                    "object.float" => Value::from(crate::reads::float(w, id, key)),
                    "object.string" => Value::from(crate::reads::string(w, id, key)),
                    _ => Value::from(crate::reads::boolean(w, id, key)),
                })
            }
            "object.appraisal" => {
                let id = arg_id(args, 0)?;
                Ok(crate::reads::appraisal(self.frame(op)?.world, id))
            }
            "object.properties" => {
                let id = arg_id(args, 0)?;
                let only = match args.get(1) {
                    None | Some(Value::Nil) => None,
                    Some(v) => Some(
                        v.as_str()
                            .filter(|t| crate::reads::PROPERTY_TYPES.contains(t))
                            .ok_or_else(|| {
                                bad("a property type is int, int64, float, string, bool, did or iid")
                            })?,
                    ),
                };
                Ok(crate::reads::properties(self.frame(op)?.world, id, only))
            }
            "inventory.items" => {
                let f = self.frame(op)?;
                Ok(Value::List(
                    crate::observe::held_items(f)
                        .into_keys()
                        .map(Value::from)
                        .collect(),
                ))
            }
            "inventory.open_container" => {
                let w = self.frame(op)?.world;
                Ok(Value::from(
                    w.ground_object
                        .filter(|c| w.inventory(*c).is_some())
                        .map(|c| c.0),
                ))
            }
            "inventory.packs" => {
                let f = self.frame(op)?;
                let Some(p) = f.world.player else {
                    return Ok(Value::Nil);
                };
                let pack = |id: ObjectId| {
                    let used = f.view.container_contents(id).len();
                    Value::map()
                        .with("id", id.0)
                        .with(
                            "name",
                            f.world
                                .weenie(id)
                                .map(|w| w.pwd.name.clone())
                                .unwrap_or_default(),
                        )
                        .with("capacity", f.view.items_capacity(id).unwrap_or(0))
                        .with("used", i64::try_from(used).unwrap_or(i64::MAX))
                };
                let mut packs = vec![pack(p)];
                packs.extend(f.view.contained_containers(p).iter().copied().map(pack));
                Ok(Value::List(packs))
            }
            "inventory.free_slots" => {
                let f = self.frame(op)?;
                let Some(p) = f.world.player else {
                    return Ok(Value::Nil);
                };
                let cap = f.view.items_capacity(p).unwrap_or(0);
                let used = i32::try_from(f.view.container_contents(p).len()).unwrap_or(i32::MAX);
                Ok(Value::from(cap.saturating_sub(used).max(0)))
            }
            "vendor.info" => {
                let f = self.frame(op)?;
                let shop = f.view.shop();
                if !shop.open {
                    return Ok(Value::Nil);
                }
                let rows = |rows: &[dereth_client_contract::view::ShopRow]| {
                    Value::List(
                        rows.iter()
                            .map(|r| {
                                Value::map()
                                    .with("id", r.item.0)
                                    .with("name", r.name.as_str())
                                    .with("price", r.price)
                                    .with("amount", r.amount)
                            })
                            .collect(),
                    )
                };
                Ok(Value::map()
                    .with("id", shop.vendor.map(|v| v.0))
                    .with("stock", rows(&shop.stock))
                    .with("buy_list", rows(&shop.buy_list))
                    .with("sell_list", rows(&shop.sell_list)))
            }
            "trade.info" => {
                let t = self.frame(op)?.view.trade();
                if !t.open {
                    return Ok(Value::Nil);
                }
                let side = |rows: &[dereth_client_contract::view::TradeRow]| {
                    Value::List(rows.iter().map(|r| Value::from(r.item.0)).collect())
                };
                Ok(Value::map()
                    .with("partner", t.partner.map(|p| p.0))
                    .with("partner_name", t.partner_name.as_str())
                    .with("mine", side(&t.self_rows))
                    .with("theirs", side(&t.partner_rows))
                    .with("accepted", t.accepted)
                    .with("partner_accepted", t.partner_accepted))
            }
            "magic.spell" => {
                let id = args
                    .first()
                    .and_then(Value::as_i64)
                    .and_then(|i| u32::try_from(i).ok())
                    .ok_or_else(|| bad("a spell id must be a whole number"))?;
                let view = self.frame(op)?.view;
                Ok(view.spell(id).map_or(Value::Nil, |s| {
                    Value::map()
                        .with("id", id)
                        .with("name", s.name.as_str())
                        .with("icon", s.icon.map(|d| d.0))
                        .with("known", view.is_spell_known(id))
                        .with("school", s.school)
                        .with("level", s.level)
                        .with("category", view.spell_category(id))
                        .with("description", view.spell_description(id))
                        .with("difficulty", view.spell_cost(id).map(|c| c.0))
                        .with("mana", view.spell_cost(id).map(|c| c.1))
                        .with("duration", view.spell_duration(id))
                }))
            }
            "magic.known" => {
                // The book itself (the character's qualities), each spell looked up in the table;
                // a page of it when asked (a character can know thousands).
                let whole = |i: usize| {
                    args.get(i)
                        .filter(|v| !matches!(v, Value::Nil))
                        .map(|v| {
                            v.as_i64()
                                .and_then(|n| usize::try_from(n).ok())
                                .ok_or_else(|| bad("from and count must be whole numbers"))
                        })
                        .transpose()
                };
                let from = whole(0)?.unwrap_or(1).max(1);
                let count = whole(1)?.unwrap_or(usize::MAX);
                let f = self.frame(op)?;
                let mut ids: Vec<u32> = f
                    .world
                    .player_qualities()
                    .and_then(|q| q.spell_book.as_ref())
                    .map(|b| b.keys().copied().collect())
                    .unwrap_or_default();
                ids.sort_unstable();
                Ok(Value::List(
                    ids.into_iter()
                        // A page's rows only are built: the ones before it are only counted.
                        .filter(|id| f.view.has_spell(*id))
                        .skip(from - 1)
                        .take(count)
                        .filter_map(|id| f.view.spell(id))
                        .map(|s| {
                            Value::map()
                                .with("id", s.id)
                                .with("name", s.name.as_str())
                                .with("school", s.school)
                                .with("level", s.level)
                                .with("category", f.view.spell_category(s.id))
                                .with("difficulty", f.view.spell_cost(s.id).map(|c| c.0))
                                .with("mana", f.view.spell_cost(s.id).map(|c| c.1))
                        })
                        .collect(),
                ))
            }
            "files.package" => {
                let path = arg_str(args, 0, "the file's path")?;
                Ok(Value::from(self.slot.package.text(path)))
            }
            "files.package_list" => Ok(Value::List(
                self.slot
                    .package
                    .files
                    .keys()
                    .map(|k| Value::from(k.as_str()))
                    .collect(),
            )),
            "act.request" => {
                let name = arg_str(args, 0, "the action")?.to_owned();
                if let Some(action) = self.config.unavailable.iter().find(|a| **a == name) {
                    let me = self.slot.manifest.id.to_string();
                    return Ok(Value::Int(self.shared.actions.refuse(
                        &me,
                        action,
                        "unavailable",
                    )));
                }
                if let Some((action, why)) =
                    self.config.unavailable_now.iter().find(|(a, _)| *a == name)
                {
                    let me = self.slot.manifest.id.to_string();
                    return Ok(Value::Int(self.shared.actions.refuse(
                        &me,
                        action,
                        &format!("unavailable ({why})"),
                    )));
                }
                let a = args.get(1).cloned().unwrap_or_else(Value::map);
                if name.starts_with("nav.") {
                    return self.nav(&name, &a);
                }
                if name.starts_with("session.") {
                    return self.session_op(&name, &a);
                }
                if name == "net.fetch" {
                    return self.net_fetch(&a);
                }
                if name == "combat.stop" {
                    return self.combat_stop();
                }
                if name == "services.call" {
                    return self.services_call(&a);
                }
                if name == "files.import" {
                    return self.files_import(&a);
                }
                self.act(&name, &a)
            }
            "nav.owner" => Ok(Value::from(self.shared.nav.owner().map(str::to_owned))),
            "nav.sight" => self.sight(args),
            "nav.journey" => Ok(self
                .frame
                .and_then(|f| f.travel.trip.as_ref())
                .filter(|_| self.shared.nav.owner() == Some(self.slot.manifest.id.as_str()))
                .map_or(Value::Nil, trip_value)),
            "nav.show_journey" => {
                let on = args
                    .first()
                    .and_then(Value::as_bool)
                    .ok_or_else(|| bad("nav.show_journey takes true or false"))?;
                let me = self.slot.manifest.id.to_string();
                Ok(Value::Bool(self.shared.nav.show_journey(&me, on)))
            }
            "nav.known_portals" => Ok(self
                .frame
                .and_then(|f| f.travel_store)
                .map_or_else(|| Value::List(Vec::new()), portals_value)),
            "nav.recalls" => {
                let character = self.character_name();
                Ok(self
                    .frame
                    .and_then(|f| f.travel_store)
                    .map_or_else(|| Value::List(Vec::new()), |s| recalls_value(s, &character)))
            }
            "meta.parse" => crate::metfile::parse(arg_str(args, 0, "the meta's text")?)
                .map_err(CallError::Failed),
            "files.data" => {
                let path = arg_str(args, 0, "the file's path")?;
                if !dereth_plugin_api::manifest::is_package_path(path) {
                    return Err(bad(format!(
                        "{path:?} is not a path inside the data folder"
                    )));
                }
                let id = self.slot.manifest.id.to_string();
                Ok(self.files.read(&id, path).map_or(Value::Nil, |b| {
                    Value::from(String::from_utf8_lossy(&b).into_owned())
                }))
            }
            "files.data_list" => {
                let id = self.slot.manifest.id.to_string();
                let mut list = self.files.list(&id);
                list.sort();
                Ok(Value::List(list.into_iter().map(Value::from).collect()))
            }
            "files.save_data" => {
                let path = arg_str(args, 0, "the file's path")?;
                let text = arg_str(args, 1, "the file's text")?;
                if !dereth_plugin_api::manifest::is_package_path(path) {
                    return Err(bad(format!(
                        "{path:?} is not a path inside the data folder"
                    )));
                }
                if text.len() > DATA_FILE_MAX {
                    return Err(CallError::Failed(format!(
                        "a data file is at most {DATA_FILE_MAX} bytes"
                    )));
                }
                let id = self.slot.manifest.id.to_string();
                let known = self.files.list(&id);
                if !known.iter().any(|k| k == path) && known.len() >= DATA_FILES_MAX {
                    return Err(CallError::Failed(format!(
                        "a data folder holds at most {DATA_FILES_MAX} files"
                    )));
                }
                self.files
                    .write(&id, path, text.as_bytes())
                    .map(|()| Value::Bool(true))
                    .map_err(CallError::Failed)
            }
            "profile.usd" => crate::usdfile::parse(arg_str(args, 0, "the profile's text")?)
                .map_err(CallError::Failed),
            "text.match" => {
                let pattern = arg_str(args, 0, "the pattern")?;
                let text = arg_str(args, 1, "the text")?;
                let ignore_case = args.get(2).and_then(Value::as_bool).unwrap_or(false);
                let re = self
                    .slot
                    .patterns
                    .get(pattern, ignore_case)
                    .map_err(CallError::Failed)?;
                Ok(crate::pattern::first_match(re, text))
            }
            "nav.parse" => crate::navfile::parse(arg_str(args, 0, "the route's text")?)
                .map_err(CallError::Failed),
            "chat.send" => {
                let text = arg_str(args, 0, "the line")?.trim().to_owned();
                if text.is_empty() || text.len() > 400 || text.contains(['\n', '\r']) {
                    return Err(bad("a chat line is one line of at most 400 characters"));
                }
                // The verb as the client will read it: after the line is narrowed to the host's
                // code page (where a full-width `／` is a `/`) and with its trailing commas gone.
                let default = dereth_client_model::HostText::default();
                let conversion = self
                    .frame
                    .map_or(&*default, |f| &*f.world.chat.text_conversion);
                let Some(sent) =
                    dereth_client_runtime::frame_hook::submitted_line(conversion, &text)
                else {
                    return Err(bad("the line has characters the client cannot send"));
                };
                if let Some((verb, _)) = dereth_client_runtime::frame_hook::command_verb(&sent) {
                    if verb == crate::HOST_VERB || self.shared.verbs.contains_key(&verb) {
                        return Err(bad(format!(
                            "/{verb} is a plugin's command; call it directly"
                        )));
                    }
                }
                let now = self.now();
                let sends = &mut self.slot.chat_sends;
                while sends.front().is_some_and(|t| now - t >= 3.0) {
                    sends.pop_front();
                }
                // Three lines in any three seconds: one a second, with a burst of three.
                if sends.len() >= 3 {
                    return Err(CallError::Failed(
                        "a plugin may send one chat line a second (three at once)".into(),
                    ));
                }
                sends.push_back(now);
                self.shared.chat_out.push(text);
                Ok(Value::Nil)
            }
            "session.info" => Ok(crate::session::info(self.frame(op)?)),
            "peers.send" => self.peers_send(args),
            "overlay.add" => self.overlay_add(args),
            "overlay.remove" => {
                let id = args
                    .first()
                    .and_then(Value::as_i64)
                    .ok_or_else(|| bad("a marker's id"))?;
                Ok(Value::Bool(self.slot.markers.remove(&id).is_some()))
            }
            "overlay.clear" => {
                let kind = match args.first().and_then(Value::as_str) {
                    None => None,
                    Some(k) => Some(
                        crate::overlay::Kind::parse(k)
                            .ok_or_else(|| bad("a marker is world, radar, map or arrow"))?,
                    ),
                };
                self.slot
                    .markers
                    .retain(|_, m| kind.is_some_and(|k| m.kind != k));
                Ok(Value::Nil)
            }
            "policy.info" => {
                let p = &self.config.policy;
                let mut all = Value::map();
                for d in dereth_plugin_api::AutoDomain::ALL {
                    all = all.with(d.name(), p.rule(d).name());
                }
                Ok(Value::map()
                    .with("published", p.published)
                    .with("rules", all.with("session", p.session.name()))
                    .with("peers_local", p.peers_local)
                    .with("notes", p.notes.clone()))
            }
            "loot.load" => {
                let text = arg_str(args, 0, "the profile's text")?;
                let p = crate::loot::Profile::read(text)
                    .map_err(|e| CallError::Failed(e.to_string()))?;
                if self.slot.loot_profiles.len() >= 64 {
                    return Err(CallError::Failed(
                        "a plugin may read at most 64 loot profiles".into(),
                    ));
                }
                self.slot.loot_profiles.push(p);
                Ok(Value::Int(
                    i64::try_from(self.slot.loot_profiles.len() - 1).unwrap_or(0),
                ))
            }
            "loot.rules" => {
                let p = self.loot_profile(args)?;
                Ok(Value::List(
                    p.rules
                        .iter()
                        .map(|r| {
                            let mut v = Value::map()
                                .with("name", r.name.as_str())
                                .with("action", r.action.name())
                                .with(
                                    "conditions",
                                    r.conditions()
                                        .into_iter()
                                        .map(Value::from)
                                        .collect::<Vec<_>>(),
                                );
                            if let crate::loot::LootAction::KeepUpTo(n) = r.action {
                                v = v.with("count", i64::from(n));
                            }
                            v
                        })
                        .collect(),
                ))
            }
            "loot.salvage_combine" => {
                let p = self.loot_profile(args)?;
                let ranges = |rs: &[crate::loot::WorkRange]| {
                    Value::List(
                        rs.iter()
                            .map(|(a, b)| Value::List(vec![Value::Int(*a), Value::Int(*b)]))
                            .collect(),
                    )
                };
                Ok(p.salvage_combine.as_ref().map_or(Value::Nil, |s| {
                    Value::map().with("default", ranges(&s.default)).with(
                        "materials",
                        Value::List(
                            s.materials
                                .iter()
                                .map(|(m, rs)| {
                                    Value::map().with("material", *m).with("ranges", ranges(rs))
                                })
                                .collect(),
                        ),
                    )
                }))
            }
            "loot.classify" | "loot.needs_appraisal" => {
                let item = arg_id(args, 1)?;
                let f = self.frame(op)?;
                let ch = self.loot_character(f, item);
                let p = self.loot_profile(args)?;
                if op == "loot.needs_appraisal" {
                    return Ok(Value::Bool(p.needs_appraisal(f.world, &ch, item)));
                }
                Ok(p.classify(f.world, &ch, item).map_or(Value::Nil, |r| {
                    let v = Value::map()
                        .with("name", r.name.as_str())
                        .with("action", r.action.name())
                        .with(
                            "keep",
                            matches!(
                                r.action,
                                crate::loot::LootAction::Keep
                                    | crate::loot::LootAction::KeepUpTo(_)
                            ),
                        );
                    match r.action {
                        crate::loot::LootAction::KeepUpTo(n) => v.with("count", n),
                        _ => v,
                    }
                }))
            }
            "permissions.activation" => {
                let now = self.now();
                Ok(Value::from((self.slot.activation_until - now).max(0.0)))
            }
            "world.find" => self.world_find(args.first().unwrap_or(&Value::Nil)),
            "world.selection" => Ok(Value::from(
                self.frame("game.selection")?.world.selected.map(|s| s.0),
            )),
            "ui.window" | "ui.hud" => {
                let (cap, kind) = if op == "ui.hud" {
                    (HostCapability::UiHud, dereth_plugin_api::NodeKind::Hud)
                } else {
                    (
                        HostCapability::UiWindow,
                        dereth_plugin_api::NodeKind::Window,
                    )
                };
                if !self.config.capabilities.contains(&cap) {
                    return Err(CallError::Failed(format!("this host has no {cap} ({op})")));
                }
                let tree = UiNode::window_from_value(args.first().unwrap_or(&Value::Nil))
                    .map_err(|e| bad(e.to_string()))?;
                if tree.kind != kind {
                    return Err(bad(format!("{op} declares a {}", kind.name())));
                }
                let id = tree.id.clone().unwrap_or_default();
                if self.slot.windows.len() >= 32 && !self.slot.windows.contains_key(&id) {
                    return Err(CallError::Failed(
                        "a plugin may declare at most 32 windows".into(),
                    ));
                }
                let revision = self.slot.windows.get(&id).map_or(1, |w| w.revision + 1);
                // A size the player gave the window stays, this session or a remembered one.
                let size = self.slot.windows.get(&id).and_then(|w| w.size).or_else(|| {
                    let v = self
                        .shared
                        .layout
                        .get(&format!("size/{}/{id}", self.slot.manifest.id))?;
                    Some((v.get("width").as_i64()?, v.get("height").as_i64()?))
                });
                let size = size
                    .filter(|_| tree.props.get("resizable").and_then(Value::as_bool) == Some(true));
                // A window opens only when the player asks. Declared again, it stays open or
                // closed as it was (open while answering the player); declared anew, it is closed
                // unless the plugin is answering the player, or the player had it open when the
                // plugin was reloaded.
                let was = self.slot.windows.get(&id).map(|w| (w.open, w.by_player));
                let reopened = was.is_none() && self.slot.reopen.remove(&id);
                let (open, by_player) = match was {
                    Some((true, by_player)) => (true, by_player),
                    _ => {
                        let open = self.shared.answering || reopened;
                        (open, open)
                    }
                };
                self.slot.windows.insert(
                    id,
                    Window {
                        tree,
                        open,
                        revision,
                        size,
                        by_player,
                    },
                );
                Ok(Value::Nil)
            }
            "services.offer" => {
                let name = arg_str(args, 0, "the service's name")?;
                crate::services::check_name(name).map_err(bad)?;
                let allow = match args.get(1) {
                    None | Some(Value::Nil) => None,
                    Some(v) => Some(
                        v.as_list()
                            .ok_or_else(|| bad("allow is a list of plugin ids"))?
                            .iter()
                            .map(|p| {
                                p.as_str()
                                    .map(str::to_owned)
                                    .ok_or_else(|| bad("allow is a list of plugin ids"))
                            })
                            .collect::<Result<_, _>>()?,
                    ),
                };
                if !self.slot.services.contains_key(name)
                    && self.slot.services.len() >= crate::services::MAX_SERVICES
                {
                    return Err(CallError::Failed(format!(
                        "a plugin may offer at most {} services",
                        crate::services::MAX_SERVICES
                    )));
                }
                self.slot.services.insert(name.to_owned(), allow);
                Ok(Value::Nil)
            }
            "services.withdraw" => {
                self.slot
                    .services
                    .remove(arg_str(args, 0, "the service's name")?);
                Ok(Value::Nil)
            }
            "services.reply" => {
                let id = args
                    .first()
                    .and_then(Value::as_i64)
                    .ok_or_else(|| bad("a reply names its call's id"))?;
                let me = self.slot.manifest.id.to_string();
                let Some(p) = self.shared.service_pending.get(&id) else {
                    return Ok(Value::Bool(false));
                };
                if p.provider != me {
                    return Err(bad("a plugin answers only the calls made to it"));
                }
                let p = self.shared.service_pending.remove(&id).expect("just found");
                let result = match args.get(2).and_then(Value::as_str) {
                    Some(err) => Err(format!("{}: {err}", p.provider)),
                    None => {
                        let v = args.get(1).cloned().unwrap_or_default();
                        if v.to_json().len() > crate::services::MAX_PAYLOAD {
                            Err("the answer is over the size a service may answer".to_owned())
                        } else {
                            Ok(v)
                        }
                    }
                };
                self.shared.actions.settle_later(crate::actions::Settled {
                    plugin: p.caller,
                    id,
                    action: "services.call",
                    result,
                });
                Ok(Value::Bool(true))
            }
            "data.landblock" | "data.cell" => {
                self.capability(op, HostCapability::GameData)?;
                let id = args
                    .first()
                    .and_then(Value::as_i64)
                    .and_then(|i| u32::try_from(i).ok())
                    .ok_or_else(|| bad("a landblock or cell id"))?;
                Ok(if op == "data.landblock" {
                    crate::gamedata::landblock(self.data, id)
                } else {
                    crate::gamedata::cell(self.data, id)
                })
            }
            "retail_ui.set" | "retail_ui.reset" => {
                self.capability(op, HostCapability::RetailUi)?;
                let panel = arg_str(args, 0, "the panel")?;
                if !RETAIL_PANELS.contains(&panel) {
                    return Err(bad(format!("the panels are {}", RETAIL_PANELS.join(", "))));
                }
                let me = self.slot.manifest.id.to_string();
                if op == "retail_ui.reset" {
                    if self
                        .shared
                        .retail_ui
                        .get(panel)
                        .is_some_and(|(p, ..)| *p == me)
                    {
                        self.shared.retail_ui.remove(panel);
                    }
                    return Ok(Value::Nil);
                }
                let v = args.get(1).cloned().unwrap_or_default();
                let hidden = v.get("hidden").as_bool();
                let at = match (v.get("x").as_i64(), v.get("y").as_i64()) {
                    (Some(x), Some(y)) => Some((x, y)),
                    (None, None) => None,
                    _ => return Err(bad("a panel's place is both x and y")),
                };
                self.shared
                    .retail_ui
                    .insert(panel.to_owned(), (me, hidden, at));
                Ok(Value::Nil)
            }
            "services.list" => {
                let me = self.slot.manifest.id.to_string();
                Ok(Value::List(
                    self.shared
                        .service_index
                        .iter()
                        .filter(|(p, _, allow)| {
                            *p != me && allow.as_ref().is_none_or(|a| a.contains(&me))
                        })
                        .map(|(p, s, _)| {
                            Value::map()
                                .with("plugin", p.as_str())
                                .with("service", s.as_str())
                        })
                        .collect(),
                ))
            }
            "ui.toast" => {
                self.capability(op, HostCapability::UiHud)?;
                let text = arg_str(args, 0, "the toast's text")?;
                if text.is_empty() || text.len() > 200 || text.contains(['\n', '\r']) {
                    return Err(bad("a toast is one line of at most 200 characters"));
                }
                let secs = args
                    .get(1)
                    .and_then(Value::as_f64)
                    .unwrap_or(5.0)
                    .clamp(1.0, TOAST_MAX_SECS);
                let name = self.slot.manifest.name.clone();
                let now = self.now();
                let mine: Vec<usize> = self
                    .shared
                    .toasts
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.0 == name)
                    .map(|(i, _)| i)
                    .collect();
                // A plugin's oldest toast gives way to its newest.
                if mine.len() >= TOASTS_PER_PLUGIN {
                    self.shared.toasts.remove(mine[0]);
                }
                self.shared.toasts.push((name, text.to_owned(), now + secs));
                self.shared.toasts_changed = true;
                Ok(Value::Nil)
            }
            "ui.status" => {
                let s = arg_str(args, 0, "the status")?;
                if !["running", "paused", "idle"].contains(&s) {
                    return Err(bad(format!(
                        "a plugin's status is running, paused or idle, not {s:?}"
                    )));
                }
                self.slot.bar_state = s.to_owned();
                Ok(Value::Nil)
            }
            "ui.get" => {
                let w = self.window_mut(args)?;
                let node = arg_str(args, 1, "the node id")?;
                let prop = arg_str(args, 2, "the property")?;
                let n = w
                    .tree
                    .find(node)
                    .ok_or_else(|| bad(format!("no node {node:?}")))?;
                Ok(match prop {
                    "id" => Value::from(node),
                    "kind" => Value::from(n.kind.name()),
                    _ => n.props.get(prop).cloned().unwrap_or_default(),
                })
            }
            "ui.set" => {
                let node = arg_str(args, 1, "the node id")?.to_owned();
                let prop = arg_str(args, 2, "the property")?.to_owned();
                let value = args.get(3).cloned().unwrap_or_default();
                let w = self.window_mut(args)?;
                w.tree
                    .set_prop(&node, &prop, value)
                    .map_err(|e| bad(e.to_string()))?;
                w.revision += 1;
                Ok(Value::Nil)
            }
            "ui.close" | "ui.show" => {
                let open = op == "ui.show";
                let answering = self.shared.answering;
                let w = self.window_mut(args)?;
                // A window opens only when the player asks: shown of the plugin's own accord (as
                // it loads, on a timer, on a game event), it stays closed.
                if w.open != open && (answering || !open) {
                    w.open = open;
                    w.by_player = open;
                    w.revision += 1;
                }
                Ok(Value::Nil)
            }
            "permissions.has" => {
                let name = arg_str(args, 0, "the permission")?;
                let p = Permission::parse(name)
                    .ok_or_else(|| bad(format!("no permission {name:?}")))?;
                Ok(Value::Bool(self.slot.has(p)))
            }
            "host.info" => Ok(Value::map()
                .with("api", ApiVersion::CURRENT.to_string())
                .with("messages", event::MESSAGES_VERSION)
                .with("name", self.config.name.as_str())
                .with("version", self.config.version.as_str())),
            "host.has" => {
                let name = arg_str(args, 0, "the capability")?;
                Ok(Value::Bool(
                    HostCapability::parse(name)
                        .is_some_and(|c| self.config.capabilities.contains(&c)),
                ))
            }
            "time.clock" => Ok(Value::Num(self.now() - self.slot.loaded_at)),
            "time.unix" => Ok(Value::Int(
                self.frame.map_or(0, |f| f.world.scroll.now_unix),
            )),
            _ => Err(CallError::Unknown(op.to_owned())),
        }
    }

    fn console(&mut self, line: &str) {
        self.slot.log(line);
        // Told in the chat under the plugin's name, within its lines a frame; past them it is in
        // the log alone.
        if self.prints < PRINTS_PER_FRAME {
            self.prints += 1;
            self.shared
                .printed
                .push(crate::printed_line(&self.slot.manifest, line));
        } else {
            tracing::info!(target: "dereth::plugin", "{}: {line}", self.slot.manifest.id);
        }
    }

    fn note(&mut self, line: &str) {
        self.slot.log(line);
    }

    fn module_source(&mut self, name: &str) -> Option<String> {
        self.slot.package.module(name)
    }
}

/// A recall a plugin named: `"lifestone"`, `"marketplace"`, or a recall spell's id.
fn recall_of(v: &Value) -> Result<dereth_navigation::Recall, CallError> {
    use dereth_navigation::Recall;
    match (v.as_str(), v.as_i64()) {
        (Some("lifestone"), _) => Ok(Recall::Lifestone),
        (Some("marketplace"), _) => Ok(Recall::Marketplace),
        (_, Some(n)) => u32::try_from(n)
            .map(Recall::Spell)
            .map_err(|_| bad("a recall spell is a spell id")),
        _ => Err(bad(
            "a recall is \"lifestone\", \"marketplace\" or a recall spell's id",
        )),
    }
}

/// The journey in progress as plugins read it.
fn trip_value(t: &dereth_client_runtime::travel::TripView) -> Value {
    let place = crate::places::to_value;
    let mut v = Value::map()
        .with(
            "legs",
            Value::List(t.legs.iter().map(crate::nav::leg_value).collect()),
        )
        .with("leg", i64::try_from(t.leg).unwrap_or(0) + 1)
        .with("goal", place(&t.goal))
        .with("route", Value::List(t.route.iter().map(place).collect()));
    if let Some(target) = &t.target {
        v = v.with("target", place(target));
    }
    v
}

/// The portals this world's store knows, as plugins read them.
fn portals_value(store: &dereth_navigation::PortalStore) -> Value {
    Value::List(
        store
            .portals()
            .iter()
            .map(|e| {
                let mut v = Value::map()
                    .with("name", e.name.as_str())
                    .with("source", e.source.word())
                    .with("from", crate::places::to_value(&e.from))
                    .with("uses", e.uses)
                    .with("failures", e.failures);
                if let Some(w) = e.wcid {
                    v = v.with("class", w);
                }
                if let Some(t) = &e.to {
                    v = v.with("to", crate::places::to_value(t));
                }
                if let Some(l) = e.levels.0 {
                    v = v.with("min_level", l);
                }
                if let Some(l) = e.levels.1 {
                    v = v.with("max_level", l);
                }
                if let Some(r) = &e.retired {
                    v = v.with("retired", r.as_str());
                }
                if e.verified != 0 {
                    v = v.with("verified", i64::try_from(e.verified).unwrap_or(i64::MAX));
                }
                v
            })
            .collect(),
    )
}

/// The character's learned recalls, as plugins read them.
fn recalls_value(store: &dereth_navigation::PortalStore, character: &str) -> Value {
    Value::List(
        store
            .recalls()
            .iter()
            .filter(|r| r.character == character)
            .map(|r| {
                Value::map()
                    .with("recall", r.recall.word())
                    .with("to", crate::places::to_value(&r.to))
                    .with("uses", r.uses)
                    .with("verified", i64::try_from(r.verified).unwrap_or(i64::MAX))
            })
            .collect(),
    )
}
