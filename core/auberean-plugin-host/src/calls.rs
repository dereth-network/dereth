//! The host operations a plugin reaches through its bindings: one entry point
//! ([`HostCalls::call`]), each operation checking its arguments and the host's ceilings before it
//! acts.

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_contract::orbit::OrbitSettings;
use auberean_client_contract::view::View;
use auberean_client_contract::Action;
use dereth_plugin_api::{
    ApiVersion, CallError, HostCalls, HostCapability, Permission, UiNode, Value,
};

use crate::events::{id_text, Session};
use crate::slot::{SlotState, Timer, Window};
use crate::storage::PluginFiles;
use crate::HostConfig;

/// The most local lines one plugin may print in one frame.
pub const PRINTS_PER_FRAME: usize = 50;

/// The shortest period of a repeating timer, in milliseconds.
pub const MIN_PERIOD_MS: f64 = 50.0;

/// The most timers one plugin may have at once (a sleeping task holds one).
pub const MAX_TIMERS: usize = 256;

/// The most lines one plugin may send to the server within [`CHAT_SEND_WINDOW`] seconds.
pub const CHAT_SENDS: usize = 10;
pub const CHAT_SEND_WINDOW: f64 = 10.0;

/// A line for the player's chat: a plugin's (by the name it is told by) or the host's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The plugin's name; `None` for the host's own lines.
    pub from: Option<String>,
    pub text: String,
}

/// An action a plugin asked for as a promise (`act.request`), carried out by the host once the
/// entry is over.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    pub plugin: String,
    pub id: i64,
    pub action: String,
    pub args: Value,
    /// It followed the player's gesture on the plugin's own surface.
    pub gesture: bool,
}

/// How an asked action ended, for its plugin's promise.
#[derive(Debug, Clone, PartialEq)]
pub struct Settled {
    pub plugin: String,
    pub id: i64,
    pub action: String,
    pub result: Result<Value, String>,
}

/// A call of one plugin's service by another, delivered to the provider at the next frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCall {
    pub id: i64,
    pub caller: String,
    pub provider: String,
    pub service: String,
    pub args: Value,
    pub gesture: bool,
}

/// What a frame's calls make beyond one plugin's own state.
#[derive(Debug, Default)]
pub struct Shared {
    /// The frame clock, seconds.
    pub now: f64,
    /// The wall clock at the frame clock's zero, Unix seconds (`time.unix`).
    pub unix_at_zero: f64,
    pub next_timer: i64,
    pub next_action: i64,
    /// An event answering one of the player's gestures is being delivered.
    pub answering: bool,
    /// Lines for the player's chat, in order, until the product takes them.
    pub printed: Vec<Line>,
    /// Every claimed verb and its plugin.
    pub verbs: BTreeMap<String, String>,
    /// The actions for the client, carried out as the frame ends.
    pub actions: Vec<Action>,
    /// The actions asked as promises in this entry.
    pub asked: Vec<Asked>,
    /// Answers to service calls, by call id.
    pub replies: Vec<(i64, Result<Value, String>)>,
    /// Every running plugin's services as the frame began: provider, service, the callers it
    /// allows.
    pub service_index: Vec<(String, String, Option<BTreeSet<String>>)>,
    /// The session as the events told it.
    pub session: Session,
    /// The world and the account, as the product said (the store scopes' keys).
    pub world: Option<String>,
    pub account: Option<String>,
    /// The camera a plugin last chose, until the product takes it: the orbit camera's settings,
    /// or the game's own (`None`).
    pub camera: Option<Option<OrbitSettings>>,
}

impl Shared {
    /// The store scopes open now: always the global one; the world's once the product named the
    /// world; the character's once a character is entering or in it.
    #[must_use]
    pub fn scope_keys(&self) -> Vec<String> {
        let mut keys = vec!["global".to_owned()];
        if let Some(w) = &self.world {
            keys.push(format!("world:{w}"));
            if let Some(c) = self.session.playing_name() {
                let account = self.account.as_deref().unwrap_or("");
                keys.push(format!("character:{w}:{account}:{c}"));
            }
        }
        keys
    }

    /// The key of the open scope a plugin names (`global`, `world`, `character`).
    fn scope_key(&self, name: &str) -> Result<String, CallError> {
        let keys = self.scope_keys();
        let prefix = match name {
            "global" => return Ok("global".to_owned()),
            "world" => "world:",
            "character" => "character:",
            other => {
                return Err(bad(format!(
                    "no store scope {other:?} (global, world or character)"
                )))
            }
        };
        keys.into_iter()
            .find(|k| k.starts_with(prefix))
            .ok_or_else(|| {
                CallError::Failed(format!(
                    "the {name} store opens once the host knows the {name}"
                ))
            })
    }
}

/// One plugin's entry into its VM: the host lends this as the plugin's [`HostCalls`].
#[derive(Debug)]
pub struct Call<'a> {
    pub slot: &'a mut SlotState,
    pub shared: &'a mut Shared,
    pub config: &'a HostConfig,
    pub files: &'a mut dyn PluginFiles,
    /// The game's view this frame, when the host has one.
    pub view: Option<&'a View>,
}

/// A wrong argument.
pub(crate) fn bad(s: impl Into<String>) -> CallError {
    CallError::BadArgs(s.into())
}

fn arg_str<'v>(args: &'v [Value], i: usize, what: &str) -> Result<&'v str, CallError> {
    args.get(i)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{what} must be a string")))
}

impl Call<'_> {
    fn me(&self) -> String {
        self.slot.manifest.id.to_string()
    }

    fn window_mut(&mut self, args: &[Value]) -> Result<&mut Window, CallError> {
        let id = arg_str(args, 0, "the window id")?;
        self.slot
            .windows
            .get_mut(id)
            .ok_or_else(|| bad(format!("no window {id:?}")))
    }

    fn declare(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        let tree = UiNode::window_from_value(args.first().unwrap_or(&Value::Nil))
            .map_err(|e| bad(e.to_string()))?;
        let hud = op == "ui.hud";
        let capability = if hud {
            HostCapability::UiHud
        } else {
            HostCapability::UiWindow
        };
        if !self.config.capabilities.contains(&capability) {
            return Err(CallError::Failed(format!(
                "this host ({}) has no {capability}",
                self.config.name
            )));
        }
        if hud != (tree.kind == dereth_plugin_api::NodeKind::Hud) {
            return Err(bad(format!(
                "{op} declares a {}",
                if hud { "hud" } else { "window" }
            )));
        }
        let id = tree.id.clone().unwrap_or_default();
        let w = self.slot.windows.entry(id).or_insert_with(|| Window {
            tree: tree.clone(),
            open: false,
            revision: 0,
        });
        w.tree = tree;
        w.revision += 1;
        Ok(Value::Nil)
    }

    fn store(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        let key = self
            .shared
            .scope_key(arg_str(args, 0, "the store scope")?)?;
        let name = arg_str(args, 1, "the store key")?.to_owned();
        let scope = self.slot.stores.entry(key).or_default();
        if op == "store.get" {
            return Ok(scope.values.get(&name).cloned().unwrap_or_default());
        }
        let value = args.get(2).cloned().unwrap_or_default();
        let old = scope
            .values
            .get(&name)
            .map_or(0, |v| crate::slot::entry_size(&name, v));
        let new = if value.is_nil() {
            0
        } else {
            crate::slot::entry_size(&name, &value)
        };
        let size = scope.size - old + new;
        if size > self.config.store_quota && new > old {
            return Err(CallError::Failed(format!(
                "a store scope is limited to {} bytes",
                self.config.store_quota
            )));
        }
        if value.is_nil() {
            scope.values.remove(&name);
        } else {
            scope.values.insert(name, value);
        }
        scope.size = size;
        scope.dirty = true;
        Ok(Value::Nil)
    }

    fn session_info(&self) -> Value {
        let s = &self.shared.session;
        let phase = match s.phase {
            None | Some(auberean_client_contract::session::Phase::Disconnected) => "disconnected",
            Some(auberean_client_contract::session::Phase::Connecting) => "connecting",
            Some(auberean_client_contract::session::Phase::Characters) => "characters",
            Some(auberean_client_contract::session::Phase::Entering) => "entering",
            Some(auberean_client_contract::session::Phase::World) => "world",
        };
        let characters: Vec<Value> = s
            .characters
            .iter()
            .map(|(c, n)| {
                Value::map()
                    .with("id", id_text(*c))
                    .with("name", n.as_str())
            })
            .collect();
        Value::map()
            .with("phase", phase)
            .with("world", self.shared.world.clone())
            .with("account", self.shared.account.clone())
            .with("character", s.playing_name().map(str::to_owned))
            .with("id", s.playing.map(id_text))
            .with("characters", characters)
    }

    fn chat_send(&mut self, text: &str) -> Result<Value, CallError> {
        let now = self.shared.now;
        let sends = &mut self.slot.chat_sends;
        while sends.front().is_some_and(|t| now - t > CHAT_SEND_WINDOW) {
            sends.pop_front();
        }
        if sends.len() >= CHAT_SENDS {
            return Err(CallError::Failed(format!(
                "a plugin may send at most {CHAT_SENDS} lines within {CHAT_SEND_WINDOW} seconds"
            )));
        }
        sends.push_back(now);
        self.shared.actions.push(Action::Say {
            text: text.to_owned(),
        });
        Ok(Value::Nil)
    }

    fn print(&mut self, text: &str) -> Result<Value, CallError> {
        if self.slot.prints >= PRINTS_PER_FRAME {
            return Err(CallError::Failed(format!(
                "a plugin may print at most {PRINTS_PER_FRAME} lines a frame"
            )));
        }
        self.slot.prints += 1;
        self.shared.printed.push(Line {
            from: Some(self.slot.told_name()),
            text: dereth_plugin_api::engine::console_line(text).into_owned(),
        });
        Ok(Value::Nil)
    }

    fn timer(&mut self, args: &[Value]) -> Result<Value, CallError> {
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
                due: self.shared.now + ms / 1000.0,
                every: repeat.then_some(ms / 1000.0),
            },
        );
        Ok(Value::Int(id))
    }

    fn command_add(&mut self, args: &[Value]) -> Result<Value, CallError> {
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
        let me = self.me();
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
        Ok(Value::Nil)
    }

    fn services(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        match op {
            "services.offer" => {
                let name = arg_str(args, 0, "the service name")?.to_owned();
                let allow = match args.get(1) {
                    None | Some(Value::Nil) => None,
                    Some(Value::List(l)) => Some(
                        l.iter()
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<BTreeSet<String>>>()
                            .ok_or_else(|| bad("allow is a list of plugin ids"))?,
                    ),
                    Some(_) => return Err(bad("allow is a list of plugin ids")),
                };
                self.slot.services.insert(name, allow);
                Ok(Value::Nil)
            }
            "services.withdraw" => {
                self.slot
                    .services
                    .remove(arg_str(args, 0, "the service name")?);
                Ok(Value::Nil)
            }
            "services.list" => {
                let me = self.me();
                Ok(Value::List(
                    self.shared
                        .service_index
                        .iter()
                        .filter(|(_, _, allow)| allow.as_ref().is_none_or(|a| a.contains(&me)))
                        .map(|(p, s, _)| {
                            Value::map()
                                .with("plugin", p.as_str())
                                .with("service", s.as_str())
                        })
                        .collect(),
                ))
            }
            "services.reply" => {
                let id = args
                    .first()
                    .and_then(Value::as_i64)
                    .ok_or_else(|| bad("services.reply takes the call's id"))?;
                let result = match args.get(2).and_then(Value::as_str) {
                    Some(err) => Err(err.to_owned()),
                    None => Ok(args.get(1).cloned().unwrap_or_default()),
                };
                self.shared.replies.push((id, result));
                Ok(Value::Nil)
            }
            _ => Err(CallError::Unknown(op.to_owned())),
        }
    }

    fn files(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        let me = self.me();
        match op {
            "files.package" => Ok(Value::from(
                self.slot.package.text(arg_str(args, 0, "the path")?),
            )),
            "files.package_list" => Ok(Value::List(
                self.slot
                    .package
                    .files
                    .keys()
                    .map(|k| Value::from(k.as_str()))
                    .collect(),
            )),
            "files.data" => {
                let path = arg_str(args, 0, "the path")?;
                Ok(Value::from(
                    self.files
                        .read(&me, path)
                        .map(|b| String::from_utf8_lossy(&b).into_owned()),
                ))
            }
            "files.data_list" => Ok(Value::List(
                self.files.list(&me).into_iter().map(Value::from).collect(),
            )),
            "files.save_data" => {
                let path = arg_str(args, 0, "the path")?;
                if !dereth_plugin_api::manifest::is_package_path(path) {
                    return Err(bad(format!("{path:?} is not a path in the data folder")));
                }
                let text = arg_str(args, 1, "the text")?;
                self.files
                    .write(&me, path, text.as_bytes())
                    .map(|()| Value::Nil)
                    .map_err(CallError::Failed)
            }
            _ => Err(CallError::Unknown(op.to_owned())),
        }
    }

    fn read(&self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        let Some(view) = self.view else {
            return Err(CallError::Failed(
                "this host gives no view of the game yet".into(),
            ));
        };
        Ok(match op {
            "game.player" => crate::reads::player(view),
            "game.objects" => crate::reads::objects(view),
            "game.object" => {
                let id = args
                    .first()
                    .and_then(crate::events::parse_id)
                    .ok_or_else(|| bad("an object is named by its id (\"0x…\")"))?;
                crate::reads::object(view, id)
            }
            "game.selection" => crate::reads::selection(view),
            "game.inventory" => crate::reads::inventory(view),
            "game.effects" => crate::reads::effects(view),
            "game.skills" => crate::reads::skills(view),
            "game.clock" => crate::reads::clock(view),
            "game.questions" => crate::reads::questions(view),
            "game.place" => crate::reads::place_line(view),
            "game.social" => crate::reads::social(view),
            "game.journal" => crate::reads::journal(view),
            "game.markers" => crate::reads::markers(view),
            "game.pets" => crate::reads::pets(view),
            "game.toolbar" => crate::reads::toolbar(view),
            "game.craft" => crate::reads::craft(view),
            _ => return Err(CallError::Unknown(op.to_owned())),
        })
    }

    fn ui(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        match op {
            "ui.window" | "ui.hud" => self.declare(op, args),
            "ui.get" => {
                let node = arg_str(args, 1, "the node id")?.to_owned();
                let key = arg_str(args, 2, "the property")?.to_owned();
                let w = self.window_mut(args)?;
                let n = w
                    .tree
                    .find(&node)
                    .ok_or_else(|| bad(format!("no node {node:?}")))?;
                Ok(n.props.get(&key).cloned().unwrap_or_default())
            }
            "ui.set" => {
                let node = arg_str(args, 1, "the node id")?.to_owned();
                let key = arg_str(args, 2, "the property")?.to_owned();
                let value = args.get(3).cloned().unwrap_or_default();
                let w = self.window_mut(args)?;
                w.tree
                    .set_prop(&node, &key, value)
                    .map_err(|e| bad(e.to_string()))?;
                w.revision += 1;
                Ok(Value::Nil)
            }
            "ui.close" => {
                let w = self.window_mut(args)?;
                if w.open {
                    w.open = false;
                    w.revision += 1;
                }
                Ok(Value::Nil)
            }
            "ui.show" => {
                let answering = self.shared.answering;
                let w = self.window_mut(args)?;
                if answering {
                    if !w.open {
                        w.open = true;
                        w.revision += 1;
                    }
                } else {
                    self.slot.log(
                        "a window opens only when the player asks: ui.show was not answering them",
                    );
                }
                Ok(Value::Nil)
            }
            "ui.toast" => {
                let text = arg_str(args, 0, "the text")?.to_owned();
                self.print(&text)
            }
            "ui.status" => {
                let s = arg_str(args, 0, "the status")?;
                if !["running", "paused", "idle"].contains(&s) {
                    return Err(bad("a status is running, paused or idle"));
                }
                s.clone_into(&mut self.slot.bar_state);
                Ok(Value::Nil)
            }
            _ => Err(CallError::Unknown(op.to_owned())),
        }
    }
}

impl HostCalls for Call<'_> {
    fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        if let Some(actions) = crate::ops::actions(op, args) {
            let actions = actions.map_err(CallError::BadArgs)?;
            self.shared.actions.extend(actions);
            return Ok(Value::Nil);
        }
        let (ns, _) = op.split_once('.').unwrap_or((op, ""));
        match op {
            "events.subscribe" => {
                let name = arg_str(args, 0, "the event name")?;
                if !crate::events::is_known(name) {
                    return Err(bad(format!(
                        "there is no event {name:?} in this version of Auberean's API"
                    )));
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
            "console.print" => {
                let text = arg_str(args, 0, "the text")?.to_owned();
                self.slot.log(text.clone());
                self.print(&text)
            }
            "chat.print" => {
                let text = arg_str(args, 0, "the text")?.to_owned();
                self.print(&text)
            }
            "chat.send" => {
                let text = arg_str(args, 0, "the text")?.to_owned();
                self.chat_send(&text)
            }
            "chat.hide" => Err(CallError::Failed(
                "this host does not hide the chat's lines yet".into(),
            )),
            "commands.add" => self.command_add(args),
            "commands.remove" => {
                let name = arg_str(args, 0, "the command name")?.to_ascii_lowercase();
                if self.slot.commands.remove(&name).is_some() {
                    self.shared.verbs.remove(&name);
                }
                Ok(Value::Nil)
            }
            "task.timer" => self.timer(args),
            // Long work between its pieces: here it goes on in the next frame.
            "task.soon" => self.timer(&[Value::Num(0.0)]),
            "task.cancel" => {
                if let Some(id) = args.first().and_then(Value::as_i64) {
                    self.slot.timers.remove(&id);
                }
                Ok(Value::Nil)
            }
            "task.error" => {
                let line = format!("error in a task: {}", arg_str(args, 0, "the error")?);
                self.shared.printed.push(Line {
                    from: Some(self.slot.told_name()),
                    text: line.lines().next().unwrap_or_default().to_owned(),
                });
                self.slot.log(line);
                self.slot.errors.push_back(self.shared.now);
                Ok(Value::Nil)
            }
            "store.get" | "store.set" => self.store(op, args),
            "act.request" => {
                let action = arg_str(args, 0, "the action")?.to_owned();
                self.shared.next_action += 1;
                let id = self.shared.next_action;
                self.shared.asked.push(Asked {
                    plugin: self.me(),
                    id,
                    action,
                    args: args.get(1).cloned().unwrap_or_default(),
                    gesture: self.slot.activation_until >= self.shared.now,
                });
                Ok(Value::Int(id))
            }
            "game.camera" => {
                self.shared.camera = Some(crate::ops::camera(args).map_err(CallError::BadArgs)?);
                Ok(Value::Nil)
            }
            "session.info" => Ok(self.session_info()),
            "peers.send" => Err(CallError::Failed(
                "this host has no other clients to send to".into(),
            )),
            "policy.info" => Ok(Value::map()
                .with("published", false)
                .with("rules", Value::map())),
            "permissions.has" => {
                let name = arg_str(args, 0, "the permission")?;
                let p = Permission::parse(name)
                    .ok_or_else(|| bad(format!("no permission {name:?}")))?;
                Ok(Value::Bool(self.slot.has(p)))
            }
            "permissions.activation" => Ok(Value::Num(
                (self.slot.activation_until - self.shared.now).max(0.0),
            )),
            "host.info" => Ok(Value::map()
                .with("api", ApiVersion::CURRENT.to_string())
                .with("name", self.config.name.as_str())
                .with("version", self.config.version.as_str())
                .with("game", dereth_plugin_api::Game::Auberean.name())
                .with("messages", dereth_plugin_api::event::MESSAGES_VERSION)),
            "host.has" => {
                let name = arg_str(args, 0, "the capability")?;
                Ok(Value::Bool(
                    self.config
                        .capabilities
                        .iter()
                        .any(|c| c.to_string() == name),
                ))
            }
            "time.clock" => Ok(Value::Num(self.shared.now - self.slot.loaded_at)),
            #[allow(clippy::cast_possible_truncation)]
            "time.unix" => Ok(Value::Int(
                (self.shared.unix_at_zero + self.shared.now).floor() as i64,
            )),
            "module.source" => Ok(Value::from(self.slot.package.module(arg_str(
                args,
                0,
                "the module",
            )?))),
            _ if ns == "services" => self.services(op, args),
            _ if ns == "files" => self.files(op, args),
            _ if ns == "ui" => self.ui(op, args),
            _ if ns == "game" => self.read(op, args),
            _ => Err(CallError::Unknown(op.to_owned())),
        }
    }

    fn console(&mut self, line: &str) {
        self.slot.log(line.to_owned());
        let _ = self.print(line);
    }

    fn note(&mut self, line: &str) {
        self.slot.log(line.to_owned());
    }

    fn module_source(&mut self, name: &str) -> Option<String> {
        self.slot.package.module(name)
    }
}
