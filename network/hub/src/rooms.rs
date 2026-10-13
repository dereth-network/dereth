//! A world's rooms (`docs/network/05-hub.md` section 5.10): which of the world's own rooms carries
//! the Global room, kept for each world, and its mirrors into Discord, which follow it.
//!
//! - **Who sets it.** The world's operator, on the site (`/v1/operator/worlds/<slug>/global-room`),
//!   or the world itself, for its admins in game (`rooms.global` on its link). A world the hub holds
//!   no room for sends the room it carried the Global room in before (a seed); the hub keeps a seed
//!   only while it holds none, so once it holds one, the world's own settings change nothing. Each
//!   change is a registry event of the world's, naming who made it (`world` for the world).
//! - **The world is told** its rooms at each `welcome` and whenever they change (the routing's
//!   `rooms`, [`crate::core::Hub`]): the room that carries the Global room, and the own rooms the
//!   hub mirrors into Discord for it.
//! - **Mirrors follow it.** The room that carries the Global room is not one of the world's own
//!   rooms while it does: a mirror of it is refused, with a reason that says to mirror the Global
//!   room instead (`carries_global`). When the room becomes known or changes, the world's mirror of
//!   that room becomes its Global room's mirror if it has none, and is taken away otherwise; either
//!   way the log says so, and a registry event records it, by the hub. A room that stops carrying
//!   the Global room is an ordinary own room again, mirrored nowhere until its operator mirrors it.

use dereth_network_protocol::rooms::{is_global_room, OFF, OWN_ROOMS};
use dereth_network_protocol::wire::GLOBAL_ROOM;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::operators::{Operator, Role};
use crate::registry::{iso, Registry, RegistryError};
use crate::server::{log, Shared};

/// The schema the rooms add to the registry (its version 9): where each world carries the Global
/// room, for the worlds the hub holds one for.
pub(crate) const SCHEMA: &str = "
    CREATE TABLE world_rooms (
      server_id TEXT PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
      global_room TEXT NOT NULL,
      set_at TEXT NOT NULL);
";

/// The actor of a change a world made itself, over its link.
pub const BY_WORLD: &str = "world";

/// The actor of a mirror the hub changed itself, as a world's Global room changed.
const BY_HUB: &str = "hub";

/// What setting a world's Global room did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GlobalSet {
    /// The room changed (or became known).
    pub changed: bool,
    /// The world's mirrors changed with it.
    pub mirrors: bool,
}

/// A mirror of the room that carries the Global room on its world.
fn carries_global(room: &str) -> RegistryError {
    RegistryError {
        reason: Some("carries_global"),
        ..RegistryError::conflict(format!(
            "{room} carries the Global room on this world: mirror the Global room instead"
        ))
    }
}

impl Registry {
    /// Where the world with registry id `server_id` carries the Global room: one of its own rooms,
    /// or `off`; none while the hub holds none for it.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn global_room_of(&self, server_id: &str) -> Result<Option<String>, RegistryError> {
        Ok(self
            .db
            .query_row(
                "SELECT global_room FROM world_rooms WHERE server_id = ?",
                [server_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Where the world `slug` carries the Global room; none while the hub holds none for it.
    ///
    /// # Errors
    /// 404 for no such world; database errors.
    pub fn global_room(&self, slug: &str) -> Result<Option<String>, RegistryError> {
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        self.global_room_of(&row.id)
    }

    /// Whether `room` of the world `server_id` may not be mirrored as its own: it carries the
    /// Global room there.
    ///
    /// # Errors
    /// 409 (`carries_global`) when it does; database errors.
    pub(crate) fn check_own_room(&self, server_id: &str, room: &str) -> Result<(), RegistryError> {
        if room != GLOBAL_ROOM && self.global_room_of(server_id)?.as_deref() == Some(room) {
            return Err(carries_global(room));
        }
        Ok(())
    }

    /// The world `slug` carries the Global room in `room` (one of its own rooms, or `off`) from
    /// now on, at `actor`'s asking (an operator's id, or [`BY_WORLD`]); with `seed`, only while the
    /// hub holds none for it. A change is a registry event of the world's, and the world's mirror
    /// of the room that now carries the Global room becomes its Global room's mirror, if it has
    /// none, or goes.
    ///
    /// # Errors
    /// 400 for a room that is none of the world's own nor `off`; 404 for no such world; 409 for a
    /// locked world; database errors.
    pub fn set_global_room(
        &self,
        slug: &str,
        room: &str,
        seed: bool,
        actor: &str,
    ) -> Result<GlobalSet, RegistryError> {
        if !is_global_room(room) {
            return Err(RegistryError::bad(format!(
                "room: one of {}, or {OFF}",
                OWN_ROOMS.join(", ")
            )));
        }
        let row = self.row(slug)?.ok_or_else(RegistryError::not_found)?;
        let was = self.global_room_of(&row.id)?;
        if was.as_deref() == Some(room) || (seed && was.is_some()) {
            return Ok(GlobalSet::default());
        }
        if row.is_locked() {
            return Err(RegistryError::conflict(
                "The hub's operator has locked this world",
            ));
        }
        self.db.execute(
            "INSERT INTO world_rooms(server_id, global_room, set_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(server_id) DO UPDATE SET global_room = excluded.global_room,
               set_at = excluded.set_at",
            params![row.id, room, iso(self.now())],
        )?;
        let mut details = json!({ "room": room, "was": was });
        if seed {
            details["seed"] = json!(true);
        }
        self.event_by(&row.id, "global-room", Some(actor), &details)?;
        log(
            "info",
            &format!(
                "{slug} carries the Global room in {room} now{}",
                if seed { " (as its settings said)" } else { "" }
            ),
        );
        let mirrors = room != OFF && self.mirror_follows(&row.id, slug, room)?;
        Ok(GlobalSet {
            changed: true,
            mirrors,
        })
    }

    /// The world `server_id`'s mirror of `room`, which now carries the Global room there, becomes
    /// its Global room's mirror if it has none, and goes otherwise. Whether it had one.
    fn mirror_follows(
        &self,
        server_id: &str,
        slug: &str,
        room: &str,
    ) -> Result<bool, RegistryError> {
        let mirrors = self.mirrors_of(Some(server_id))?;
        let Some(own) = mirrors.iter().find(|m| m.room == room) else {
            return Ok(false);
        };
        if mirrors.iter().any(|m| m.room == GLOBAL_ROOM) {
            self.db.execute(
                "DELETE FROM discord_mirrors WHERE server_id = ?1 AND room = ?2",
                params![server_id, room],
            )?;
            self.event_by(
                server_id,
                "discord-mirror",
                Some(BY_HUB),
                &json!({ "room": room, "mirrored": false, "carriesGlobal": true }),
            )?;
            log(
                "info",
                &format!(
                    "{slug}'s mirror of {room} into the Discord channel {} is taken away: {room} carries the Global room there now, which it mirrors already",
                    own.placement.channel
                ),
            );
        } else {
            self.db.execute(
                "UPDATE discord_mirrors SET room = ?3 WHERE server_id = ?1 AND room = ?2",
                params![server_id, room, GLOBAL_ROOM],
            )?;
            let p = &own.placement;
            let mut details = json!({
                "room": GLOBAL_ROOM, "mirrored": true, "channel": p.channel, "server": p.server,
                "through": p.through(), "was": room,
            });
            if let Some(hook) = &p.hook {
                details["webhook"] = json!(hook.id);
            }
            self.event_by(server_id, "discord-mirror", Some(BY_HUB), &details)?;
            log(
                "info",
                &format!(
                    "{slug}'s mirror of {room} into the Discord channel {} mirrors the Global room now: {room} carries it there",
                    p.channel
                ),
            );
        }
        Ok(true)
    }
}

/// The operator route `/v1/operator/worlds/<slug>/global-room`, for the world's owner alone: where
/// the world carries the Global room (`GET`), or carrying it in another room (`PUT`, `{"room"}`),
/// at once.
///
/// # Errors
/// 404 or 403 for a world that is not the operator's; 400 for another body or room; 409 for a
/// locked world.
pub(crate) fn operator_global_room(
    shared: &Shared,
    op: &Operator,
    method: &str,
    slug: &str,
    body: &Value,
) -> Result<(u16, Value), RegistryError> {
    let (row, role) = shared.registry().operator_world(op, slug)?;
    if role != Role::Owner {
        return Err(RegistryError::forbidden(
            "Verify your claim before you manage this world",
        ));
    }
    match method {
        "GET" => {}
        "PUT" => {
            let room = body
                .as_object()
                .filter(|o| o.len() == 1)
                .and_then(|o| o.get("room"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RegistryError::bad(format!(
                        "The body is {{\"room\": one of {}, or {OFF}}}",
                        OWN_ROOMS.join(", ")
                    ))
                })?;
            let set = shared
                .registry()
                .set_global_room(&row.slug, room, false, &op.id)?;
            if set.changed {
                rooms_changed(shared, set);
            }
        }
        _ => return Err(RegistryError::new(404, "Route not found")),
    }
    let global = shared.registry().global_room_of(&row.id)?;
    Ok((200, json!({ "globalRoom": global })))
}

/// A world's rooms changed outside the routing: the mirrors are read again when they changed too,
/// and every world linked is told its rooms where they differ from what it was last told.
pub(crate) fn rooms_changed(shared: &Shared, set: GlobalSet) {
    if set.mirrors {
        crate::bridge::refresh(shared);
    }
    shared.rooms_changed();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::Placement;
    use dereth_network_protocol::Federation;

    fn registry() -> Registry {
        let r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        let registration = crate::registry::Registration::from_json(&json!({
            "slug": "coldeve", "name": "Coldeve", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        r.register(&registration).expect("registered");
        r.set_federation("coldeve", Federation::On).expect("on");
        r
    }

    fn rooms_mirrored(r: &Registry) -> Vec<(String, String)> {
        r.mirrors()
            .expect("read")
            .into_iter()
            .map(|m| (m.room, m.placement.channel))
            .collect()
    }

    /// Spec: hub: a world's Global room is one of its own rooms or off, held once its operator or
    /// the world sets it; a seed is taken only while the hub holds none; each change is an event
    /// naming who made it.
    #[test]
    fn a_worlds_global_room_is_seeded_once_and_set_by_its_operator_or_itself() {
        let r = registry();
        assert_eq!(r.global_room("coldeve"), Ok(None), "none held yet");
        assert_eq!(
            r.set_global_room("coldeve", "roleplay", true, BY_WORLD),
            Ok(GlobalSet {
                changed: true,
                mirrors: false
            })
        );
        assert_eq!(
            r.global_room("coldeve").expect("read").as_deref(),
            Some("roleplay")
        );
        assert_eq!(
            r.set_global_room("coldeve", "trade", true, BY_WORLD),
            Ok(GlobalSet::default()),
            "a seed while one is held changes nothing"
        );
        assert_eq!(
            r.global_room("coldeve").expect("read").as_deref(),
            Some("roleplay")
        );
        assert!(
            r.set_global_room("coldeve", "trade", false, "op_1")
                .expect("set")
                .changed
        );
        assert!(
            r.set_global_room("coldeve", "off", false, BY_WORLD)
                .expect("set")
                .changed
        );
        assert_eq!(
            r.global_room("coldeve").expect("read").as_deref(),
            Some("off")
        );
        assert_eq!(
            r.set_global_room("coldeve", "trade", true, BY_WORLD),
            Ok(GlobalSet::default()),
            "off is held too"
        );
        for bad in ["global", "society", "Trade", ""] {
            assert_eq!(
                r.set_global_room("coldeve", bad, false, BY_WORLD)
                    .map_err(|e| e.status),
                Err(400),
                "{bad}"
            );
        }
        let audit = r.audit("coldeve").expect("the log");
        let events: Vec<(String, Value, String)> = audit
            .as_array()
            .expect("events")
            .iter()
            .filter(|e| e["kind"] == "global-room")
            .map(|e| {
                (
                    e["actor"].as_str().unwrap_or("").to_owned(),
                    e["details"]["room"].clone(),
                    e["details"]["was"].to_string(),
                )
            })
            .collect();
        assert_eq!(
            events,
            [
                ("world".to_owned(), json!("off"), "\"trade\"".to_owned()),
                ("op_1".to_owned(), json!("trade"), "\"roleplay\"".to_owned()),
                ("world".to_owned(), json!("roleplay"), "null".to_owned()),
            ]
        );
    }

    /// Spec: hub: when a world's Global room becomes known, its mirror of that room becomes its
    /// Global room's mirror if it has none, and goes otherwise, each recorded by the hub; a room
    /// that stops carrying the Global room is mirrored nowhere until its operator mirrors it.
    #[test]
    fn a_mirror_of_the_room_that_comes_to_carry_global_becomes_the_global_mirror_or_goes() {
        let r = registry();
        let bot = |channel: &str| Placement::bot(channel, "500");
        r.set_mirror(Some("coldeve"), "roleplay", &bot("71"), Some("op_1"))
            .expect("mirrored before the hub knew roleplay carries Global");
        r.set_mirror(Some("coldeve"), "general", &bot("72"), Some("op_1"))
            .expect("mirrored");
        assert_eq!(
            r.set_global_room("coldeve", "roleplay", true, BY_WORLD),
            Ok(GlobalSet {
                changed: true,
                mirrors: true
            })
        );
        assert_eq!(
            rooms_mirrored(&r),
            [
                ("general".to_owned(), "72".to_owned()),
                ("global".to_owned(), "71".to_owned())
            ],
            "the roleplay mirror carries Global into its channel now"
        );
        let audit = r.audit("coldeve").expect("the log").to_string();
        assert!(
            audit.contains(r#""actor":"hub""#) && audit.contains(r#""was":"roleplay""#),
            "{audit}"
        );
        assert_eq!(
            r.set_global_room("coldeve", "general", false, "op_1"),
            Ok(GlobalSet {
                changed: true,
                mirrors: true
            })
        );
        assert_eq!(
            rooms_mirrored(&r),
            [("global".to_owned(), "71".to_owned())],
            "Global is mirrored already, so general's mirror goes"
        );
        assert!(r
            .audit("coldeve")
            .expect("the log")
            .to_string()
            .contains(r#""carriesGlobal":true"#));
        assert_eq!(
            r.set_global_room("coldeve", "trade", false, "op_1"),
            Ok(GlobalSet {
                changed: true,
                mirrors: false
            }),
            "general, no longer carrying Global, has no mirror"
        );
        assert_eq!(rooms_mirrored(&r), [("global".to_owned(), "71".to_owned())]);
    }

    /// Spec: hub: the room that carries a world's Global room is not one of its own rooms, so a
    /// mirror of it is refused with a reason that says to mirror Global instead.
    #[test]
    fn the_room_carrying_global_is_refused_as_an_own_rooms_mirror() {
        let r = registry();
        let id = r.row("coldeve").expect("read").expect("the world").id;
        assert_eq!(r.check_own_room(&id, "roleplay"), Ok(()), "none held yet");
        r.set_global_room("coldeve", "roleplay", false, "op_1")
            .expect("set");
        let refused = r.check_own_room(&id, "roleplay").expect_err("refused");
        assert_eq!(
            (refused.status, refused.reason),
            (409, Some("carries_global"))
        );
        assert!(refused.message.contains("mirror the Global room instead"));
        assert_eq!(r.check_own_room(&id, "general"), Ok(()));
        assert_eq!(r.check_own_room(&id, "global"), Ok(()));
    }
}
