//! The session before and around the world: the character list, entering the world as a
//! character, logging off, and why an entry failed.
//!
//! A plugin loaded before login hears `session.pregame` each time the character list is up (after
//! connecting, and after each log-off), `session.login_failed` when the server refuses an entry
//! (with the reason, for instance `character_in_world` while the character's last session is still
//! in the world), and `session.disconnect` when the connection ends. With `session.control` it can
//! choose a character and enter the world, log off, and quit, as the character screen's buttons
//! and the log-out do; each is a promise.
//!
//! | action | completes | fails |
//! |---|---|---|
//! | `session.select_character(name)` | the character is in the world | the entry's reason (`character_in_world`, …), `disconnected`, `timeout` (60 s) |
//! | `session.logout()` | the character list is back | `disconnected`, `timeout` (30 s) |
//! | `session.quit()` | at once (the client logs off and ends) | |

use dereth_client_runtime::frame_hook::{HookFrame, Outcome, SessionPhase, SessionRequest};
use dereth_plugin_api::{CallError, Event, Value};

use crate::actions::Settled;

/// How long an entry may take.
pub const ENTER_SECS: f64 = 60.0;
/// How long a log-off may take.
pub const LOGOFF_SECS: f64 = 30.0;

/// The name of a character error (`Character_CharacterError`, `0xF659`), as plugins read it.
#[must_use]
pub fn reason(code: u32) -> &'static str {
    match code {
        0x01 => "logon",
        0x03 => "account_login",
        0x04 | 0x08 => "server_crash",
        0x05 => "logoff",
        0x06 => "delete",
        0x09 => "account_invalid",
        0x0A => "account_missing",
        0x0B => "enter_game_generic",
        0x0C => "account_stressed",
        0x0D => "character_in_world",
        0x0E => "player_account_missing",
        0x0F => "character_not_owned",
        0x10 => "character_in_world_server",
        0x11 => "old_character",
        0x12 => "corrupt_character",
        0x13 => "start_server_down",
        0x14 => "could_not_place_character",
        0x15 => "server_full",
        0x17 => "character_locked",
        0x18 => "subscription_expired",
        _ => "other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    InWorld,
    CharacterList,
}

#[derive(Debug, Clone)]
struct Pending {
    plugin: String,
    id: i64,
    action: &'static str,
    want: Want,
    started: f64,
    value: Value,
}

/// The session as the host follows it.
#[derive(Debug, Default)]
pub struct Session {
    phase: SessionPhase,
    pending: Option<Pending>,
    /// Operations asked for this frame, for the frame to carry out.
    outbox: Vec<SessionRequest>,
}

impl Session {
    /// The operations asked for since the last call.
    pub fn take_requests(&mut self) -> Vec<SessionRequest> {
        std::mem::take(&mut self.outbox)
    }

    /// This frame's session events: the character list coming up, an entry refused, the
    /// connection ending.
    pub fn observe(&mut self, frame: &HookFrame<'_>) -> Vec<Event> {
        let mut out = Vec::new();
        for o in frame.outcomes {
            if let Outcome::LoginFailed { code } = *o {
                out.push(Event::new(
                    "session.login_failed",
                    Value::map().with("code", code).with("reason", reason(code)),
                ));
            }
        }
        let phase = frame.session.phase;
        if phase != self.phase {
            match phase {
                SessionPhase::CharacterSelect => {
                    out.push(Event::new("session.pregame", info(frame)));
                }
                SessionPhase::Disconnected => {
                    out.push(Event::new("session.disconnect", Value::map()));
                }
                _ => {}
            }
            self.phase = phase;
        }
        out
    }

    /// The pending operation, if this frame ended it.
    pub fn settle(&mut self, frame: &HookFrame<'_>) -> Option<Settled> {
        let p = self.pending.as_ref()?;
        let failed = frame.outcomes.iter().find_map(|o| match *o {
            Outcome::LoginFailed { code } => Some(reason(code)),
            _ => None,
        });
        let phase = frame.session.phase;
        let result = match p.want {
            Want::InWorld if phase == SessionPhase::InWorld => Some(Ok(p.value.clone())),
            Want::InWorld if failed.is_some() => failed.map(|r| Err(r.to_owned())),
            Want::CharacterList if phase == SessionPhase::CharacterSelect => Some(Ok(Value::Nil)),
            _ if phase == SessionPhase::Disconnected => Some(Err("disconnected".to_owned())),
            Want::InWorld if frame.local_time - p.started > ENTER_SECS => {
                Some(Err("timeout".to_owned()))
            }
            Want::CharacterList if frame.local_time - p.started > LOGOFF_SECS => {
                Some(Err("timeout".to_owned()))
            }
            _ => None,
        }?;
        let p = self.pending.take()?;
        Some(Settled {
            plugin: p.plugin,
            id: p.id,
            action: p.action,
            result,
        })
    }

    /// Start the operation `name` for `plugin` as action `id`.
    ///
    /// # Errors
    /// The session is not where the operation starts from, another operation is pending, or the
    /// arguments name no character.
    pub fn request(
        &mut self,
        plugin: &str,
        id: i64,
        name: &str,
        args: &Value,
        frame: &HookFrame<'_>,
    ) -> Result<Option<Settled>, CallError> {
        let phase = frame.session.phase;
        let failed = |msg: String| Err(CallError::Failed(msg));
        if self.pending.is_some() && name != "session.quit" {
            return failed(format!("{name}: another session operation is under way"));
        }
        let (action, want, value): (&'static str, Want, Value) = match name {
            "session.select_character" => {
                if phase != SessionPhase::CharacterSelect {
                    return failed(format!(
                        "{name} needs the character list up (the session is {})",
                        phase.name()
                    ));
                }
                let want = args.get("character");
                let found = frame.session.characters.iter().find(|(gid, n)| match want {
                    Value::Str(s) => n.eq_ignore_ascii_case(s),
                    v => v.as_i64() == Some(i64::from(gid.0)),
                });
                let Some((gid, n)) = found else {
                    return failed(format!("{name}: no character {want:?} on this account"));
                };
                let value = Value::map().with("id", gid.0).with("name", n.as_str());
                self.outbox.push(SessionRequest::LogOn(*gid));
                ("session.select_character", Want::InWorld, value)
            }
            "session.logout" => {
                if phase != SessionPhase::InWorld {
                    return failed(format!("{name} needs a character in the world"));
                }
                self.outbox.push(SessionRequest::LogOff);
                ("session.logout", Want::CharacterList, Value::Nil)
            }
            "session.quit" => {
                self.outbox.push(SessionRequest::Quit);
                return Ok(Some(Settled {
                    plugin: plugin.to_owned(),
                    id,
                    action: "session.quit",
                    result: Ok(Value::Nil),
                }));
            }
            other => return Err(CallError::Unknown(other.to_owned())),
        };
        self.pending = Some(Pending {
            plugin: plugin.to_owned(),
            id,
            action,
            want,
            started: frame.local_time,
            value,
        });
        Ok(None)
    }

    /// Forget `plugin`'s pending operation (it is being unloaded).
    pub fn forget(&mut self, plugin: &str) {
        if self.pending.as_ref().is_some_and(|p| p.plugin == plugin) {
            self.pending = None;
        }
    }
}

/// `session.info()`: the phase, the characters, the world, the account, portal space and the
/// context (`world`, `pregame` or `chargen`).
#[must_use]
pub fn info(frame: &HookFrame<'_>) -> Value {
    Value::map()
        .with("phase", frame.session.phase.name())
        .with(
            "characters",
            Value::List(
                frame
                    .session
                    .characters
                    .iter()
                    .map(|(id, n)| Value::map().with("id", id.0).with("name", n.as_str()))
                    .collect(),
            ),
        )
        .with("world", frame.world_name.map(str::to_owned))
        .with("account", frame.account.map(str::to_owned))
        .with("portal_space", frame.session.portal_space)
        .with("context", crate::context_of(frame).name())
}

#[cfg(test)]
mod tests {
    use super::reason;

    #[test]
    fn a_character_error_reads_as_its_reason() {
        assert_eq!(reason(0x0D), "character_in_world");
        assert_eq!(reason(0x15), "server_full");
        assert_eq!(reason(0x99), "other");
    }
}
