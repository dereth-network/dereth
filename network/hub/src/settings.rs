//! The hub's settings, which its admins change while it runs (`docs/network/05-hub.md` section
//! 2.14): the hub's ceiling on worlds approved onto the network a day, how old a Discord account
//! must be to register or claim a world, and how long a registered world may stay `pending`.
//!
//! - **Kept in the registry.** A setting the registry holds no value of is its default. Wherever a
//!   setting is used, it is read from the registry then, so a change holds at once, for the
//!   serving hub and for the command line alike.
//! - **Each change is an event** of the hub's own settings log, with who made it: the admin, or
//!   the hub's operator for a flag that seeded it.
//! - **Flags at start.** The hub's operator once gave these as flags of `serve`. A flag given still
//!   seeds its setting while the registry holds no value of it; once the registry holds one, a
//!   flag that differs is ignored, and the log names both values ([`Registry::seed_settings`]).

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::operators::Operator;
use crate::registry::{iso, Registry, RegistryError};

type Result<T> = std::result::Result<T, RegistryError>;

/// The schema the settings add to the registry (its version 8): the value of each setting set,
/// and the hub's own settings log.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE hub_settings (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
    CREATE TABLE settings_events (
      id INTEGER PRIMARY KEY,
      kind TEXT NOT NULL, at TEXT NOT NULL, actor TEXT, details TEXT NOT NULL);
";

/// One of the hub's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Setting {
    /// Worlds approved onto the network a day, for every operator together: a ceiling above what
    /// operators' own allowances reach together, so that a few accounts cannot spend it for all.
    ApprovalsPerDay,
    /// How old a Discord account must be, in days, to register or claim a world.
    MinAccountAgeDays,
    /// How long a registered world may stay `pending`, in days, before it is removed and its slug
    /// freed.
    PendingDays,
}

impl Setting {
    /// Every setting, in the order the routes answer them.
    pub const ALL: [Self; 3] = [
        Self::ApprovalsPerDay,
        Self::MinAccountAgeDays,
        Self::PendingDays,
    ];

    /// Its name in the admin routes, and in its events.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ApprovalsPerDay => "approvalsPerDay",
            Self::MinAccountAgeDays => "minAccountAgeDays",
            Self::PendingDays => "pendingDays",
        }
    }

    /// The flag of `serve` that seeds it.
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::ApprovalsPerDay => "--approvals-per-day",
            Self::MinAccountAgeDays => "--min-account-age-days",
            Self::PendingDays => "--pending-days",
        }
    }

    /// Its value while the registry holds none.
    #[must_use]
    pub fn default_value(self) -> u64 {
        match self {
            Self::ApprovalsPerDay => 500,
            Self::MinAccountAgeDays => 30,
            Self::PendingDays => 14,
        }
    }

    /// The least and the most it may be. A world must have at least a day to be verified in.
    #[must_use]
    pub fn bounds(self) -> (u64, u64) {
        match self {
            Self::ApprovalsPerDay => (0, 100_000),
            Self::MinAccountAgeDays => (0, 3_650),
            Self::PendingDays => (1, 365),
        }
    }

    /// The setting named `name` in the admin routes.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// What it may be, in words.
    fn range_text(self) -> String {
        let (least, most) = self.bounds();
        format!("a whole number from {least} to {most}")
    }

    /// `value`, when it is within the setting's bounds.
    ///
    /// # Errors
    /// A message naming the setting and its bounds.
    pub fn check(self, value: u64) -> std::result::Result<u64, String> {
        let (least, most) = self.bounds();
        if (least..=most).contains(&value) {
            Ok(value)
        } else {
            Err(format!("{}: {}", self.name(), self.range_text()))
        }
    }

    /// The value its flag was given as `text`.
    ///
    /// # Errors
    /// A message naming the flag and its bounds, for anything but a whole number within them.
    pub fn parse_flag(self, text: &str) -> std::result::Result<u64, String> {
        text.trim()
            .parse::<u64>()
            .ok()
            .and_then(|v| self.check(v).ok())
            .ok_or_else(|| format!("{} {text}: {}", self.flag(), self.range_text()))
    }
}

/// What a `PUT /v1/admin/settings` body sets: one or more settings by name, each a whole number.
///
/// # Errors
/// 400, naming the setting, for a value that is not a whole number within its bounds; 400 for a
/// name that is not a setting, or a body that names none.
pub fn changes_of(body: &Value) -> Result<Vec<(Setting, u64)>> {
    let names = Setting::ALL.map(Setting::name).join(", ");
    let o = body.as_object().filter(|o| !o.is_empty()).ok_or_else(|| {
        RegistryError::bad(format!(
            "The body names one or more of the hub's settings ({names}), each a whole number"
        ))
    })?;
    let mut changes = Vec::new();
    for (name, value) in o {
        let setting = Setting::named(name).ok_or_else(|| {
            RegistryError::bad(format!("{name}: not one of the hub's settings ({names})"))
        })?;
        let value = value
            .as_u64()
            .ok_or_else(|| RegistryError::bad(format!("{name}: {}", setting.range_text())))?;
        changes.push((setting, setting.check(value).map_err(RegistryError::bad)?));
    }
    Ok(changes)
}

/// What the log says of a flag given at start: its level and its line.
pub type SeedLine = (&'static str, String);

impl Registry {
    /// The value the registry holds of `setting`, if it holds one.
    fn setting_held(&self, setting: Setting) -> Result<Option<u64>> {
        let v: Option<i64> = self
            .db
            .query_row(
                "SELECT value FROM hub_settings WHERE name = ?",
                [setting.name()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.and_then(|v| u64::try_from(v).ok()))
    }

    /// The value of `setting` in effect: the registry's, or its default while it holds none.
    ///
    /// # Errors
    /// Database errors only.
    pub fn setting(&self, setting: Setting) -> Result<u64> {
        Ok(self
            .setting_held(setting)?
            .unwrap_or(setting.default_value()))
    }

    /// Keeps `value` for `setting` as an event of the hub's settings log by `actor` (the hub's
    /// operator for none), with `more` in its details; nothing when the registry holds it already.
    fn keep(
        &self,
        setting: Setting,
        value: u64,
        actor: Option<&str>,
        more: &[(&str, Value)],
    ) -> Result<()> {
        if self.setting_held(setting)? == Some(value) {
            return Ok(());
        }
        let was = self.setting(setting)?;
        self.db.execute(
            "INSERT INTO hub_settings(name, value) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET value = excluded.value",
            params![setting.name(), i64::try_from(value).unwrap_or(i64::MAX)],
        )?;
        let mut details = Map::new();
        details.insert("name".into(), json!(setting.name()));
        details.insert("value".into(), json!(value));
        details.insert("was".into(), json!(was));
        for (k, v) in more {
            details.insert((*k).to_owned(), v.clone());
        }
        self.db.execute(
            "INSERT INTO settings_events(kind, at, actor, details) VALUES ('setting', ?, ?, ?)",
            params![iso(self.now()), actor, Value::Object(details).to_string()],
        )?;
        Ok(())
    }

    /// Sets each of `changes` at `actor`'s asking (the hub's operator for none), all of them or
    /// none: each is checked before any is kept, and each value the registry did not hold already
    /// is an event of the hub's settings log. It holds at once.
    ///
    /// # Errors
    /// 400, naming the setting, for a value outside its bounds; database errors.
    pub fn set_settings(&self, changes: &[(Setting, u64)], actor: Option<&str>) -> Result<()> {
        for &(setting, value) in changes {
            setting.check(value).map_err(RegistryError::bad)?;
        }
        let tx = self.db.unchecked_transaction()?;
        for &(setting, value) in changes {
            self.keep(setting, value, actor, &[])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Seeds each setting a flag gave at start, `given`, while the registry holds no value of it,
    /// as an event of the hub's operator's. What the log says of each flag: that it seeded its
    /// setting, or, when the registry holds another value, that it is ignored, naming both.
    ///
    /// # Errors
    /// 400 for a value outside its setting's bounds; database errors.
    pub fn seed_settings(&self, given: &[(Setting, u64)]) -> Result<Vec<SeedLine>> {
        let mut lines = Vec::new();
        for &(setting, value) in given {
            setting.check(value).map_err(RegistryError::bad)?;
            let flag = setting.flag();
            match self.setting_held(setting)? {
                None => {
                    self.keep(setting, value, None, &[("flag", json!(flag))])?;
                    lines.push((
                        "info",
                        format!(
                            "{flag} {value} is now the hub's setting {}: admins change it on the site, and the flag is no longer needed",
                            setting.name()
                        ),
                    ));
                }
                Some(held) if held != value => lines.push((
                    "warn",
                    format!(
                        "{flag} {value} is ignored: the registry holds {held} for {}, which admins change on the site",
                        setting.name()
                    ),
                )),
                Some(_) => {}
            }
        }
        Ok(lines)
    }

    /// The settings as the admin routes answer them: each one's value in effect, its default and
    /// its bounds.
    ///
    /// # Errors
    /// Database errors only.
    pub fn settings_view(&self) -> Result<Value> {
        let mut out = Map::new();
        for setting in Setting::ALL {
            let (least, most) = setting.bounds();
            out.insert(
                setting.name().to_owned(),
                json!({
                    "value": self.setting(setting)?,
                    "default": setting.default_value(),
                    "min": least,
                    "max": most,
                }),
            );
        }
        Ok(Value::Object(out))
    }

    /// The hub's settings log, newest first, at most `limit` events.
    ///
    /// # Errors
    /// Database errors only.
    pub fn settings_log(&self, limit: usize) -> Result<Vec<Value>> {
        let mut stmt = self.db.prepare(
            "SELECT kind, at, actor, details FROM settings_events ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt
            .query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |r| {
                let details: String = r.get(3)?;
                let actor: Option<String> = r.get(2)?;
                Ok(json!({
                    "kind": r.get::<_, String>(0)?,
                    "at": r.get::<_, String>(1)?,
                    "actor": actor.unwrap_or_else(|| "hub-operator".to_owned()),
                    "details": serde_json::from_str::<Value>(&details).unwrap_or(Value::Null),
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// The admin routes under `/v1/admin/settings`: the settings (`GET`), a change to any of them
/// (`PUT`), and the hub's settings log (`GET log`).
///
/// # Errors
/// 400 as [`changes_of`]; 404 for no such route.
pub(crate) fn admin(
    registry: &Registry,
    op: &Operator,
    method: &str,
    rest: &[&str],
    body: &Value,
) -> Result<(u16, Value)> {
    match (method, rest) {
        ("GET", []) => Ok((200, registry.settings_view()?)),
        ("PUT", []) => {
            registry.set_settings(&changes_of(body)?, Some(&op.id))?;
            Ok((200, registry.settings_view()?))
        }
        ("GET", ["log"]) => Ok((200, json!({ "events": registry.settings_log(200)? }))),
        _ => Err(RegistryError::new(404, "Route not found")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry")
    }

    /// Spec: hub: a setting the registry holds no value of is its default; each is set within its
    /// bounds only, all of a change or none of it, and each value set is an event of the hub's
    /// settings log naming who set it and what it was.
    #[test]
    fn a_setting_is_its_default_until_set_within_its_bounds() {
        let r = registry();
        for s in Setting::ALL {
            assert_eq!(r.setting(s), Ok(s.default_value()), "{}", s.name());
        }
        assert_eq!(
            r.set_settings(
                &[(Setting::ApprovalsPerDay, 40), (Setting::PendingDays, 0)],
                Some("op_admin"),
            )
            .map_err(|e| (e.status, e.message)),
            Err((400, "pendingDays: a whole number from 1 to 365".to_owned()))
        );
        assert_eq!(r.setting(Setting::ApprovalsPerDay), Ok(500), "none of it");
        assert!(r.settings_log(10).expect("the log").is_empty());
        r.set_settings(&[(Setting::ApprovalsPerDay, 40)], Some("op_admin"))
            .expect("set");
        r.set_settings(&[(Setting::ApprovalsPerDay, 40)], Some("op_admin"))
            .expect("asked again, nothing changes");
        r.set_settings(&[(Setting::PendingDays, 14)], None)
            .expect("its default, now the registry's");
        assert_eq!(r.setting(Setting::ApprovalsPerDay), Ok(40));
        let log = r.settings_log(10).expect("the log");
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(log[1]["actor"], "op_admin");
        assert_eq!(
            log[1]["details"],
            json!({ "name": "approvalsPerDay", "value": 40, "was": 500 })
        );
        assert_eq!(log[0]["actor"], "hub-operator");
    }

    /// Spec: hub: a flag given at start seeds its setting while the registry holds no value of it;
    /// once it holds one, the registry's value holds, and a flag that differs is ignored with a
    /// line naming both values.
    #[test]
    fn a_flag_seeds_its_setting_only_while_the_registry_holds_none() {
        let r = registry();
        let lines = r
            .seed_settings(&[(Setting::ApprovalsPerDay, 400)])
            .expect("seeded");
        assert_eq!(r.setting(Setting::ApprovalsPerDay), Ok(400));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "info");
        let seeded = &r.settings_log(10).expect("the log")[0];
        assert_eq!(
            (&seeded["kind"], &seeded["actor"]),
            (&json!("setting"), &json!("hub-operator"))
        );
        assert_eq!(
            seeded["details"],
            json!({ "name": "approvalsPerDay", "value": 400, "was": 500,
                    "flag": "--approvals-per-day" })
        );
        r.set_settings(&[(Setting::ApprovalsPerDay, 300)], Some("op_admin"))
            .expect("an admin's change");
        let lines = r
            .seed_settings(&[(Setting::ApprovalsPerDay, 400), (Setting::PendingDays, 21)])
            .expect("read");
        assert_eq!(
            r.setting(Setting::ApprovalsPerDay),
            Ok(300),
            "the registry's holds"
        );
        assert_eq!(
            r.setting(Setting::PendingDays),
            Ok(21),
            "the other is seeded"
        );
        assert_eq!(
            lines[0],
            (
                "warn",
                "--approvals-per-day 400 is ignored: the registry holds 300 for approvalsPerDay, which admins change on the site"
                    .to_owned()
            )
        );
        assert_eq!(
            r.seed_settings(&[(Setting::ApprovalsPerDay, 300)]),
            Ok(Vec::new()),
            "a flag that agrees says nothing"
        );
        assert_eq!(r.settings_log(10).expect("the log").len(), 3);
        assert_eq!(
            r.seed_settings(&[(Setting::PendingDays, 0)])
                .map_err(|e| e.status),
            Err(400)
        );
    }

    /// Spec: hub: a change to the settings names each setting by its name and gives a whole
    /// number within its bounds; anything else is refused, naming the setting.
    #[test]
    fn a_change_names_settings_and_whole_numbers_within_their_bounds() {
        assert_eq!(
            changes_of(&json!({ "approvalsPerDay": 0, "pendingDays": 365 })),
            Ok(vec![
                (Setting::ApprovalsPerDay, 0),
                (Setting::PendingDays, 365)
            ])
        );
        for (body, names) in [
            (json!({ "approvalsPerDay": 100_001 }), "approvalsPerDay"),
            (json!({ "approvalsPerDay": -1 }), "approvalsPerDay"),
            (json!({ "minAccountAgeDays": 3_651 }), "minAccountAgeDays"),
            (json!({ "minAccountAgeDays": 1.5 }), "minAccountAgeDays"),
            (json!({ "pendingDays": 0 }), "pendingDays"),
            (json!({ "pendingDays": 366 }), "pendingDays"),
            (json!({ "pendingDays": "14" }), "pendingDays"),
            (json!({ "pendingDays": null }), "pendingDays"),
            (json!({ "pendingdays": 14 }), "pendingdays"),
            (json!({}), "approvalsPerDay, minAccountAgeDays, pendingDays"),
            (json!([]), "approvalsPerDay, minAccountAgeDays, pendingDays"),
        ] {
            let e = changes_of(&body).expect_err("refused");
            assert_eq!(e.status, 400, "{body}");
            assert!(e.message.contains(names), "{body}: {}", e.message);
        }
        assert_eq!(Setting::PendingDays.parse_flag("21"), Ok(21));
        for bad in ["0", "366", "-1", "x", ""] {
            let e = Setting::PendingDays.parse_flag(bad).expect_err("refused");
            assert!(e.starts_with("--pending-days"), "{e}");
        }
    }
}
