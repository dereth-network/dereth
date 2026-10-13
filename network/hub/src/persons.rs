//! Persons: characters linked to a Discord account, and person friends (`docs/network/05-hub.md`
//! section 5).
//!
//! A person is an account of [`crate::operators`]: whoever signs in with Discord. A world asks for a
//! one-time code for one of its own characters; the player opens the link it gives on the site,
//! signed in, and the character is linked to that person. Nothing of a person is hidden from
//! another player: a person friend lists every character of its person, on any world on the
//! network.
//!
//! **What is answered to whom** is decided here and nowhere else:
//!
//! - [`Registry::person_characters`] names a person's characters, for any one of them; for every
//!   other character (nobody linked it, it does not exist) it names none, alike.
//! - [`Registry::person_view`] is what one character's world hears: whether it is linked, and the
//!   person friends of its person, each listing its handle and that handle's person's other
//!   characters. The view of a character nobody links names the friends handed back to it when it
//!   was unlinked, until it has been sent once.
//! - The site's routes show a person only itself.
//!
//! **A link pauses** while its character is off the network, as the hub takes it from the
//! directory ([`Registry::on_network`]): the person's friends' views do not name the character,
//! tells to it do not reach the person's Discord, and the person does not speak as it from
//! Discord. Its world's `listing` that puts it back on resumes it, with nothing to redo.
//!
//! A code is kept as its SHA-256; a person's events (kept for the limits) never name a character.

use std::collections::BTreeSet;
use std::time::Duration;

use dereth_network_protocol::person::{
    link_code_from, read_link_code, PersonFriend, PersonOutcome, MAX_CARRIED,
    MAX_PERSON_CHARACTERS, MAX_PERSON_FRIENDS,
};
use dereth_network_protocol::Address;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::directory::on_network_sql;
use crate::operators::{within, Operator};
use crate::registry::{digest, hex, iso, random_bytes, Registry, RegistryError};

type Result<T> = std::result::Result<T, RegistryError>;

/// How long a link code lasts.
pub const CODE_SECS: u32 = 600;

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;

/// The person layer's limits (`docs/network/05-hub.md` section 5).
pub mod limits {
    /// Codes for one character an hour.
    pub const CODES_PER_CHARACTER: usize = 5;
    /// Codes one world's characters are given in a code's lifetime, spent or not: spending a code
    /// (an unlink) frees none of the allowance.
    pub const CODES_PER_WORLD: usize = 200;
    /// Wrong codes one person may enter an hour.
    pub const WRONG_CODES_PER_HOUR: usize = 10;
    /// Characters one person may link.
    pub const CHARACTERS: usize = dereth_network_protocol::person::MAX_PERSON_CHARACTERS;
    /// Person friends one person may list, from all of its characters together, across the
    /// network. A view names at most [`dereth_network_protocol::person::MAX_PERSON_FRIENDS`] of
    /// them, as many as the final retail client's friends list holds.
    pub const FRIENDS: usize = 1_000;
}

pub use crate::core::{PersonAnswer, PersonView};

/// The account events the limits count (`docs/network/05-hub.md` sections 2.9 and 5): kept past a
/// forgotten account for a day, the longest window any of them is counted in.
const COUNTED: &[&str] = &[
    "registered",
    "claimed",
    "page-fetched",
    "approved",
    "reported",
    "wrong-code",
];

/// Who a forgotten account's counted events belong to until they lapse: a digest of its Discord
/// id, never the id itself, which a new sign-in by the same Discord account takes them back from.
pub(crate) fn forgotten_as(discord_id: &str) -> String {
    format!("gone_{}", &hex(&digest(discord_id))[..32])
}

/// The schema the person layer adds to the registry (its version 5). Its `alts` column, a setting
/// the network no longer has, goes in version 10 ([`schema_10`]).
pub(crate) const SCHEMA: &str = "
    ALTER TABLE operators ADD COLUMN alts TEXT NOT NULL DEFAULT 'hidden';
    CREATE TABLE person_characters (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      linked_at TEXT NOT NULL);
    CREATE INDEX person_characters_by_person ON person_characters(person_id);
    CREATE TABLE person_codes (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      code_hash TEXT NOT NULL UNIQUE,
      expires_at TEXT NOT NULL);
    CREATE INDEX person_codes_by_world ON person_codes(world, expires_at);
    CREATE TABLE person_code_log (address_key TEXT NOT NULL, at TEXT NOT NULL);
    CREATE INDEX person_code_log_by_key ON person_code_log(address_key, at);
    CREATE TABLE person_friends (
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      added_from TEXT NOT NULL,
      added_from_address TEXT NOT NULL,
      added_at TEXT NOT NULL,
      PRIMARY KEY (person_id, handle_key, added_from));
";

/// The person layer's second step (the registry's version 6): the code log by time, for a world's
/// allowance.
pub(crate) const SCHEMA_6: &str = "
    CREATE INDEX person_code_log_by_time ON person_code_log(at);
";

/// The person layer's third step (the registry's version 7): the friends handed back to a
/// character when it is unlinked, until its world is sent them.
pub(crate) const SCHEMA_7: &str = "
    CREATE TABLE person_returns (
      address_key TEXT NOT NULL,
      world TEXT NOT NULL,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      at TEXT NOT NULL,
      PRIMARY KEY (address_key, handle_key));
    CREATE INDEX person_returns_by_world ON person_returns(world);
    CREATE INDEX person_returns_by_time ON person_returns(at);
";

/// The person layer's fourth step (the registry's version 10): a person's setting that hid its
/// characters from each other, and the one that took tells on Discord, go; nothing of a person is
/// hidden, and a linked character's tells reach its person's Discord without a setting. A hold a
/// person put on one of its characters' listings goes too: a character is listed while it is on
/// the network. Each statement is one the registry's own code builds, so the step runs on a
/// registry that never had the second column.
pub(crate) fn schema_10(has_discord_tells: bool) -> String {
    let mut s = String::from(
        "ALTER TABLE operators DROP COLUMN alts;
         DELETE FROM directory_holds WHERE by = 'person';
         DELETE FROM operator_events WHERE kind = 'alts';",
    );
    if has_discord_tells {
        s.push_str("ALTER TABLE operators DROP COLUMN discord_tells;");
    }
    s
}

/// How long friends handed back to an unlinked character wait for it to come online.
const RETURNS_SECS: u64 = 30 * DAY;

fn address(s: &str) -> Option<Address> {
    Address::parse(s).ok()
}

/// Whether a person friend was listed.
enum Added {
    Yes,
    /// It is listed from that character already.
    Already,
    /// As many person friends as a person may list hold for that character already.
    Full,
}

/// `ok`, having changed the person whose characters are `changed`.
fn ok_changing(changed: Vec<Address>) -> PersonAnswer {
    PersonAnswer {
        outcome: PersonOutcome::Ok,
        changed,
    }
}

impl Registry {
    fn since(&self, secs: u64) -> String {
        iso(self
            .now()
            .checked_sub(Duration::from_secs(secs))
            .unwrap_or(std::time::UNIX_EPOCH))
    }

    /// The person a character is linked to.
    fn person_of(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .query_row(
                "SELECT person_id FROM person_characters WHERE address_key = ?",
                [key],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    /// The characters linked to the person signed in: for the routing, to find the views a change
    /// of the person's may have changed.
    ///
    /// # Errors
    /// Database errors only.
    pub fn characters_of(&self, op: &Operator) -> Result<Vec<Address>> {
        self.characters_of_person(&op.id)
    }

    /// A person's characters, in the order they were linked.
    pub(crate) fn characters_of_person(&self, person: &str) -> Result<Vec<Address>> {
        let mut q = self.db.prepare_cached(
            "SELECT address FROM person_characters WHERE person_id = ? ORDER BY linked_at, rowid",
        )?;
        let rows = q
            .query_map([person], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows.iter().filter_map(|a| address(a)).collect())
    }

    /// What `handle` lists as a person friend: itself, then its person's other characters on worlds
    /// on the network, while each is on the network itself (a link pauses while its character is
    /// off), in the order they were linked.
    fn listed_by(&self, handle: &Address) -> Result<Vec<Address>> {
        let mut q = self.db.prepare_cached(&format!(
            "SELECT c.address FROM person_characters h
               JOIN person_characters c ON c.person_id = h.person_id
                 AND c.address_key != h.address_key
             WHERE h.address_key = ?1
               AND c.world IN (SELECT slug FROM servers WHERE federation = 'on') AND {}
             ORDER BY c.linked_at, c.rowid",
            on_network_sql("c.address_key", "?2")
        ))?;
        let others = q
            .query_map(params![handle.key(), iso(self.now())], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out = vec![handle.clone()];
        out.extend(others.iter().filter_map(|a| address(a)));
        out.truncate(MAX_PERSON_CHARACTERS);
        Ok(out)
    }

    // ---- what worlds hear ------------------------------------------------------------------------

    /// The characters of `target`'s person on worlds on the network, `target` first; none for a
    /// character nobody linked and one that does not exist, alike.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_characters(&self, target: &Address) -> Result<Vec<Address>> {
        match self.person_of(&target.key())? {
            Some(_) => self.listed_by(target),
            None => Ok(Vec::new()),
        }
    }

    /// `who`'s person view, whole: whether it is linked, and every person friend of its person,
    /// from whichever of its characters each was added, in the order they were added, each with the
    /// characters it lists. A person may list more person friends than a view names: the routing
    /// cuts the view to its limits as it sends it, by what it knows of who is online
    /// ([`crate::core::cut_view`]).
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_view(&self, who: &Address) -> Result<PersonView> {
        let Some(person) = self.person_of(&who.key())? else {
            return Ok(PersonView {
                persons: self.returns_of(who)?,
                ..PersonView::default()
            });
        };
        // One read for every person friend: its handle, and each other character of the handle's
        // person that it lists.
        let mut q = self.db.prepare_cached(&format!(
            "SELECT f.rowid, f.handle_key, f.handle, c.address FROM person_friends f
               LEFT JOIN person_characters h ON h.address_key = f.handle_key
               LEFT JOIN person_characters c ON c.person_id = h.person_id
                 AND c.address_key != f.handle_key
                 AND c.world IN (SELECT slug FROM servers WHERE federation = 'on')
                 AND {}
             WHERE f.person_id = ?1
             ORDER BY f.added_at, f.rowid, c.linked_at, c.rowid",
            on_network_sql("c.address_key", "?2")
        ))?;
        let rows = q.query_map(params![person, iso(self.now())], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut handles = BTreeSet::new();
        let mut persons: Vec<PersonFriend> = Vec::new();
        // The person friend the rows are of now, while it is the first of its handle.
        let mut current: Option<(i64, bool)> = None;
        for row in rows {
            let (rowid, key, handle, other) = row?;
            if current.is_none_or(|(r, _)| r != rowid) {
                // Each handle once, the first time it was added.
                let first = match address(&handle) {
                    Some(handle) if handles.insert(key) => {
                        persons.push(PersonFriend {
                            characters: vec![handle.clone()],
                            handle,
                        });
                        true
                    }
                    _ => false,
                };
                current = Some((rowid, first));
            }
            if let (Some((_, true)), Some(c)) = (current, other.as_deref().and_then(address)) {
                if let Some(p) = persons.last_mut() {
                    if p.characters.len() < MAX_PERSON_CHARACTERS {
                        p.characters.push(c);
                    }
                }
            }
        }
        Ok(PersonView {
            linked: true,
            persons,
        })
    }

    /// The friends handed back to `who` when it was unlinked, each its handle alone.
    fn returns_of(&self, who: &Address) -> Result<Vec<PersonFriend>> {
        Ok(self
            .returns_of_key(&who.key())?
            .into_iter()
            .map(|handle| PersonFriend {
                characters: vec![handle.clone()],
                handle,
            })
            .collect())
    }

    /// The handles handed back to the character `key`, in the order they were.
    fn returns_of_key(&self, key: &str) -> Result<Vec<Address>> {
        let mut q = self.db.prepare_cached(
            "SELECT handle FROM person_returns WHERE address_key = ? ORDER BY at, rowid",
        )?;
        let rows = q
            .query_map([key], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows
            .iter()
            .filter_map(|h| address(h))
            .take(MAX_PERSON_FRIENDS)
            .collect())
    }

    /// `who`'s world has been sent the friends handed back to it: the hub forgets them.
    ///
    /// # Errors
    /// Database errors only.
    pub fn returns_sent(&self, who: &Address) -> Result<()> {
        self.db.execute(
            "DELETE FROM person_returns WHERE address_key = ?",
            [who.key()],
        )?;
        Ok(())
    }

    /// The handles of `person`'s person friends, in the order they were added, each once: those
    /// that hold for each of its characters.
    fn held_for(&self, person: &str) -> Result<Vec<(String, String)>> {
        let mut q = self.db.prepare_cached(
            "SELECT handle_key, handle FROM person_friends WHERE person_id = ?
             ORDER BY added_at, rowid",
        )?;
        let rows = q
            .query_map([person], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut seen = BTreeSet::new();
        Ok(rows
            .into_iter()
            .filter(|(k, _)| seen.insert(k.clone()))
            .collect())
    }

    /// The character `key` of `person` is being unlinked: the friends that hold for it now are
    /// handed back to it, to become its own friends on its world when its world is next sent its
    /// view. Friends handed back long ago and never sent are forgotten.
    fn hand_back(&self, person: &str, key: &str) -> Result<()> {
        self.db.execute(
            "DELETE FROM person_returns WHERE at < ?",
            [self.since(RETURNS_SECS)],
        )?;
        let Some(world) = self
            .db
            .query_row(
                "SELECT world FROM person_characters WHERE address_key = ?",
                [key],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        else {
            return Ok(());
        };
        let now = iso(self.now());
        for (handle_key, handle) in self.held_for(person)?.into_iter().take(MAX_PERSON_FRIENDS) {
            self.db.execute(
                "INSERT INTO person_returns(address_key, world, handle_key, handle, at)
                 VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
                params![key, world, handle_key, handle, now],
            )?;
        }
        Ok(())
    }

    /// Lists `handle` as a person friend of `person`, added from the character `who`, unless it is
    /// listed from there already or the person lists as many person friends as it may.
    fn add_friend(
        &self,
        person: &str,
        who: &Address,
        handle_key: &str,
        handle: &str,
    ) -> Result<Added> {
        let exists: bool = self.db.query_row(
            "SELECT count(*) > 0 FROM person_friends WHERE person_id = ? AND handle_key = ? AND added_from = ?",
            params![person, handle_key, who.key()],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(Added::Already);
        }
        let held: i64 = self.db.query_row(
            "SELECT count(DISTINCT handle_key) FROM person_friends
             WHERE person_id = ?1 AND handle_key != ?2",
            params![person, handle_key],
            |r| r.get(0),
        )?;
        if usize::try_from(held).unwrap_or(usize::MAX) >= limits::FRIENDS {
            return Ok(Added::Full);
        }
        self.db.execute(
            "INSERT INTO person_friends(person_id, handle_key, handle, added_from, added_from_address, added_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                person,
                handle_key,
                handle,
                who.key(),
                who.to_string(),
                iso(self.now())
            ],
        )?;
        Ok(Added::Yes)
    }

    /// A link code for `who`, which replaces any it had: the code and how long it lasts.
    ///
    /// # Errors
    /// `rate_limited` past the limits; `unavailable` for a database error.
    pub fn person_code(&self, who: &Address) -> std::result::Result<(String, u32), PersonOutcome> {
        self.issue_code(who).map_err(|e| {
            if e.status == 429 {
                PersonOutcome::RateLimited
            } else {
                crate::server::log("error", &format!("a link code: {}", e.message));
                PersonOutcome::Unavailable
            }
        })
    }

    fn issue_code(&self, who: &Address) -> Result<(String, u32)> {
        let now = self.now();
        let key = who.key();
        self.db
            .execute("DELETE FROM person_codes WHERE expires_at <= ?", [iso(now)])?;
        self.db.execute(
            "DELETE FROM person_code_log WHERE at < ?",
            [self.since(DAY)],
        )?;
        let mut q = self.db.prepare_cached(
            "SELECT at FROM person_code_log WHERE address_key = ? AND at >= ? ORDER BY at DESC",
        )?;
        let times = q
            .query_map(params![key, self.since(HOUR)], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        within(
            &times,
            limits::CODES_PER_CHARACTER,
            HOUR,
            now,
            "Too many link codes for this character this hour",
        )?;
        // Codes given to the world's characters while a code lasts, spent or not, so that unlinking,
        // which spends one, frees nothing. A key ends in `@` and the world's slug, which holds no
        // pattern character.
        let given: i64 = self.db.query_row(
            "SELECT count(*) FROM person_code_log WHERE at > ?1 AND address_key LIKE ?2",
            params![
                self.since(u64::from(CODE_SECS)),
                format!("%@{}", who.world())
            ],
            |r| r.get(0),
        )?;
        if usize::try_from(given).unwrap_or(usize::MAX) >= limits::CODES_PER_WORLD {
            return Err(RegistryError::limited(
                "This world's characters were given too many link codes lately",
                u64::from(CODE_SECS),
            ));
        }
        let code = link_code_from(random_bytes::<5>());
        self.db.execute(
            "INSERT INTO person_codes(address_key, address, world, code_hash, expires_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(address_key) DO UPDATE SET address = excluded.address,
               code_hash = excluded.code_hash, expires_at = excluded.expires_at",
            params![
                key,
                who.to_string(),
                who.world(),
                hex(&digest(&code)),
                iso(now + Duration::from_secs(u64::from(CODE_SECS)))
            ],
        )?;
        self.db.execute(
            "INSERT INTO person_code_log(address_key, at) VALUES (?, ?)",
            params![key, iso(now)],
        )?;
        Ok((code, CODE_SECS))
    }

    /// Forgets a world's characters' links to persons and their codes, and their listings, as the
    /// world leaves the registry: a world registered later under its slug inherits none of them.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn forget_worlds_links(&self, slug: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM person_characters WHERE world = ?", [slug])?;
        self.db
            .execute("DELETE FROM person_codes WHERE world = ?", [slug])?;
        self.db
            .execute("DELETE FROM person_returns WHERE world = ?", [slug])?;
        self.forget_worlds_listings(slug)?;
        self.db
            .execute("DELETE FROM directory_holds WHERE world = ?", [slug])?;
        Ok(())
    }

    /// Unlinks `who`, and spends its code. The friends that held for it are handed back to it.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_unlink(&self, who: &Address) -> Result<PersonAnswer> {
        let key = who.key();
        self.db
            .execute("DELETE FROM person_codes WHERE address_key = ?", [&key])?;
        let Some(person) = self.person_of(&key)? else {
            return Ok(PersonAnswer::unchanged(PersonOutcome::NotLinked));
        };
        let changed = self.characters_of_person(&person)?;
        self.hand_back(&person, &key)?;
        self.db.execute(
            "DELETE FROM person_characters WHERE address_key = ?",
            [&key],
        )?;
        self.operator_event(&person, "unlinked", &json!({}))?;
        Ok(ok_changing(changed))
    }

    /// What a world of the network's first version is answered when it asks to hide or show its
    /// character's person's characters: nothing of a person is hidden, so nothing changes, and it is
    /// refused (`not_linked` for a character nobody links).
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_alts(&self, who: &Address) -> Result<PersonAnswer> {
        Ok(PersonAnswer::unchanged(
            if self.person_of(&who.key())?.is_some() {
                PersonOutcome::Refused
            } else {
                PersonOutcome::NotLinked
            },
        ))
    }

    /// Lists `target`'s person as a person friend of `who`'s person, added from `who`. `ok` for any
    /// character's address, whether anyone linked it or not; `refused` when the person lists as many
    /// person friends as it may, from all of its characters together.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_befriend(&self, who: &Address, target: &Address) -> Result<PersonAnswer> {
        let Some(person) = self.person_of(&who.key())? else {
            return Ok(PersonAnswer::unchanged(PersonOutcome::NotLinked));
        };
        Ok(
            match self.add_friend(&person, who, &target.key(), &target.to_string())? {
                Added::Already => PersonAnswer::unchanged(PersonOutcome::Ok),
                Added::Full => PersonAnswer::unchanged(PersonOutcome::Refused),
                Added::Yes => ok_changing(self.characters_of_person(&person)?),
            },
        )
    }

    /// Lists each of `targets`' persons as a person friend of `who`'s person, added from `who`, in
    /// order, as [`Self::person_befriend`] does, until as many hold for `who` as a person may list:
    /// the friends `who` had before it was linked. `ok` however many were listed.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_carry(&self, who: &Address, targets: &[Address]) -> Result<PersonAnswer> {
        let Some(person) = self.person_of(&who.key())? else {
            return Ok(PersonAnswer::unchanged(PersonOutcome::NotLinked));
        };
        let mut changed = false;
        for t in targets.iter().take(MAX_CARRIED) {
            match self.add_friend(&person, who, &t.key(), &t.to_string())? {
                Added::Yes => changed = true,
                Added::Already => {}
                Added::Full => break,
            }
        }
        Ok(if changed {
            ok_changing(self.characters_of_person(&person)?)
        } else {
            PersonAnswer::unchanged(PersonOutcome::Ok)
        })
    }

    /// Removes the person friend whose handle is `target` from `who`'s person, from whichever of
    /// its characters it was added.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_unfriend(&self, who: &Address, target: &Address) -> Result<PersonAnswer> {
        let Some(person) = self.person_of(&who.key())? else {
            return Ok(PersonAnswer::unchanged(PersonOutcome::NotLinked));
        };
        let removed = self.db.execute(
            "DELETE FROM person_friends WHERE person_id = ?1 AND handle_key = ?2",
            params![person, target.key()],
        )?;
        if removed == 0 {
            return Ok(PersonAnswer::unchanged(PersonOutcome::Ok));
        }
        Ok(ok_changing(self.characters_of_person(&person)?))
    }

    // ---- the person's own routes -----------------------------------------------------------------

    /// The person signed in, as the site shows it to that person alone.
    ///
    /// # Errors
    /// Database errors only.
    pub fn person_json(&self, op: &Operator) -> Result<Value> {
        let mut q = self.db.prepare_cached(
            "SELECT address, world, linked_at FROM person_characters WHERE person_id = ? ORDER BY linked_at, rowid",
        )?;
        let rows = q
            .query_map([&op.id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut characters = Vec::new();
        for (address, world, linked_at) in rows {
            let name = self.federation(&world)?.map(|(n, _)| n);
            let listing = match Address::parse(&address) {
                Ok(a) => self.listing_state(&a.key())?,
                Err(_) => "off",
            };
            characters.push(json!({
                "address": address, "world": { "slug": world, "name": name }, "linkedAt": linked_at,
                "listing": listing,
            }));
        }
        let mut q = self.db.prepare_cached(
            "SELECT handle, added_from_address, added_at FROM person_friends WHERE person_id = ? ORDER BY added_at, rowid",
        )?;
        let friends = q
            .query_map([&op.id], |r| {
                Ok(json!({
                    "handle": r.get::<_, String>(0)?, "addedFrom": r.get::<_, String>(1)?,
                    "addedAt": r.get::<_, String>(2)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(json!({ "name": op.name, "characters": characters, "friends": friends }))
    }

    /// The person opens a link, or enters its code: the code's character is linked to it (moved
    /// from any other person), and the code is spent.
    ///
    /// # Errors
    /// 422 `no_such_code` for a code the hub never gave, a spent one and a lapsed one alike; 409
    /// past the characters a person may link; 429 past the wrong codes a person may enter.
    pub fn redeem_link_code(&self, op: &Operator, entered: &str) -> Result<Value> {
        let wrong = self.operator_event_times(Some(&op.id), "wrong-code", HOUR)?;
        within(
            &wrong,
            limits::WRONG_CODES_PER_HOUR,
            HOUR,
            self.now(),
            "Too many wrong codes this hour",
        )?;
        let now = iso(self.now());
        let found: Option<(String, String, String)> = match read_link_code(entered) {
            Some(code) => self
                .db
                .query_row(
                    "SELECT address_key, address, world FROM person_codes WHERE code_hash = ? AND expires_at > ?",
                    params![hex(&digest(&code)), now],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?,
            None => None,
        };
        let Some((key, address, world)) = found else {
            self.operator_event(&op.id, "wrong-code", &json!({}))?;
            return Err(RegistryError {
                reason: Some("no_such_code"),
                code: Some("LINK_FAILED"),
                ..RegistryError::new(
                    422,
                    "That code is not one the hub gave, or it has been used or has lapsed",
                )
            });
        };
        let held: i64 = self.db.query_row(
            "SELECT count(*) FROM person_characters WHERE person_id = ? AND address_key != ?",
            params![op.id, key],
            |r| r.get(0),
        )?;
        if usize::try_from(held).unwrap_or(usize::MAX) >= limits::CHARACTERS {
            return Err(RegistryError {
                reason: Some("too_many_characters"),
                ..RegistryError::conflict(format!(
                    "A person links at most {} characters",
                    limits::CHARACTERS
                ))
            });
        }
        self.db
            .execute("DELETE FROM person_codes WHERE address_key = ?", [&key])?;
        // The friends the character has now go with it: those that held for it under another
        // person it moves from, and those handed back to it by an unlink its world has not yet
        // been sent.
        let mut friends = match self.person_of(&key)? {
            Some(other) if other != op.id => self.held_for(&other)?,
            _ => Vec::new(),
        };
        friends.extend(
            self.returns_of_key(&key)?
                .into_iter()
                .map(|h| (h.key(), h.to_string())),
        );
        self.db
            .execute("DELETE FROM person_returns WHERE address_key = ?", [&key])?;
        self.db.execute(
            "INSERT INTO person_characters(address_key, address, world, person_id, linked_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(address_key) DO UPDATE SET address = excluded.address, world = excluded.world,
               person_id = excluded.person_id, linked_at = excluded.linked_at",
            params![key, address, world, op.id, now],
        )?;
        if let Ok(who) = Address::parse(&address) {
            for (handle_key, handle) in friends {
                if matches!(
                    self.add_friend(&op.id, &who, &handle_key, &handle)?,
                    Added::Full
                ) {
                    break;
                }
            }
        }
        // A character moved from another person is no longer that person's to speak as.
        self.identities_moved(&key, &op.id)?;
        self.operator_event(&op.id, "linked", &json!({}))?;
        let name = self.federation(&world)?.map(|(n, _)| n);
        Ok(json!({
            "character": { "address": address, "world": { "slug": world, "name": name }, "linkedAt": now },
            "person": self.person_json(op)?,
        }))
    }

    /// Unlinks one of the person's characters. The friends that held for it are handed back to it.
    ///
    /// # Errors
    /// 404 for any address that is not one of the person's, alike whether another person linked it
    /// or nobody did.
    pub fn unlink_for(&self, op: &Operator, address: &str) -> Result<()> {
        let not_yours = || RegistryError::new(404, "That is not one of your characters");
        let key = Address::parse(address).map_err(|_| not_yours())?.key();
        if let Some(person) = self.person_of(&key)?.filter(|p| *p == op.id) {
            self.hand_back(&person, &key)?;
        }
        let n = self.db.execute(
            "DELETE FROM person_characters WHERE address_key = ? AND person_id = ?",
            params![key, op.id],
        )?;
        if n == 0 {
            return Err(not_yours());
        }
        self.db
            .execute("DELETE FROM person_codes WHERE address_key = ?", [&key])?;
        self.operator_event(&op.id, "unlinked", &json!({}))?;
        Ok(())
    }

    /// Removes a person friend of the person's, whichever character it was added from.
    ///
    /// # Errors
    /// 404 for a handle it has none of.
    pub fn unfriend_for(&self, op: &Operator, handle: &str) -> Result<()> {
        let none = || RegistryError::new(404, "You list no person by that character");
        let key = Address::parse(handle).map_err(|_| none())?.key();
        let n = self.db.execute(
            "DELETE FROM person_friends WHERE person_id = ? AND handle_key = ?",
            params![op.id, key],
        )?;
        if n == 0 {
            return Err(none());
        }
        Ok(())
    }

    /// What the limits counted of a forgotten account, once a day old, is no longer needed.
    fn forget_lapsed_counts(&self) -> Result<()> {
        for kind in COUNTED {
            self.db.execute(
                "DELETE FROM operator_events WHERE kind = ?1 AND at < ?2 AND operator_id LIKE 'gone\\_%' ESCAPE '\\'",
                params![kind, self.since(DAY)],
            )?;
        }
        Ok(())
    }

    /// A new account for `discord_id` takes back what the limits counted of the one it forgot.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn take_back_counts(&self, operator: &str, discord_id: &str) -> Result<()> {
        self.forget_lapsed_counts()?;
        self.db.execute(
            "UPDATE operator_events SET operator_id = ?1 WHERE operator_id = ?2",
            params![operator, forgotten_as(discord_id)],
        )?;
        Ok(())
    }

    /// Forgets the person, as `@dn off` on every character it links and more. Each of its
    /// characters goes off the network: out of the directory at once, and its world is told to take
    /// it off (`listing.off`, by its person), over its link now or at its next link
    /// ([`Registry::owe_person_off`]). Then every link goes, each character keeping the friends that
    /// held for it, handed back to it as an unlink does, with its link code; the person friends
    /// go, and every session of the account ends. An account that is not an operator (it holds no
    /// world and no open claim) goes with it, and its reports and its claims: true when it went.
    /// The worlds' logs then name it as their owners read it, `earlier-claimant` or `hub-admin`,
    /// so nothing names the account any more. What the limits count of an account that goes stays
    /// for a day, under a digest of its Discord id ([`forgotten_as`]), so that signing in again
    /// starts none of them over. An operator keeps its account and its reports, and loses the
    /// events of its links.
    ///
    /// # Errors
    /// Database errors only.
    pub fn forget_person(&self, op: &Operator) -> Result<bool> {
        for c in self.characters_of_person(&op.id)? {
            self.owe_person_off(&c)?;
            // Each character keeps its friends, as its world's own.
            self.hand_back(&op.id, &c.key())?;
            self.db
                .execute("DELETE FROM person_codes WHERE address_key = ?", [c.key()])?;
        }
        self.db.execute(
            "DELETE FROM person_characters WHERE person_id = ?",
            [&op.id],
        )?;
        self.db
            .execute("DELETE FROM person_friends WHERE person_id = ?", [&op.id])?;
        self.db.execute(
            "DELETE FROM operator_sessions WHERE operator_id = ?",
            [&op.id],
        )?;
        // An operator keeps its account, and with it its reports; a `pending` world past its days is
        // held no longer.
        self.expire_pending()?;
        let (worlds, claims) = self.held(&op.id)?;
        if worlds + claims > 0 {
            self.db.execute(
                "DELETE FROM operator_events WHERE operator_id = ? AND kind IN ('linked', 'unlinked')",
                [&op.id],
            )?;
            self.operator_event(&op.id, "forgotten", &json!({}))?;
            return Ok(false);
        }
        // Its reports go, open or closed; a reported world's own log never said who reported it.
        self.db
            .execute("DELETE FROM reports WHERE reporter_id = ?", [&op.id])?;
        // Each event it did is named as the world's owner reads it, and its claims, none open, go.
        self.db.execute(
            "UPDATE registry_events SET actor = CASE
                 WHEN server_id IN (SELECT server_id FROM verifications WHERE operator_id = ?1)
                   OR server_id IN (SELECT server_id FROM registry_events
                                    WHERE kind = 'assigned' AND json_extract(details, '$.operator') = ?1)
                 THEN 'earlier-claimant' ELSE 'hub-admin' END
             WHERE actor = ?1",
            [&op.id],
        )?;
        self.db.execute(
            "UPDATE registry_events SET details = json_set(details, '$.operator', 'earlier-claimant')
             WHERE kind = 'assigned' AND json_extract(details, '$.operator') = ?1",
            [&op.id],
        )?;
        // The hub's own Discord log names only its admins, who act for the hub.
        self.db.execute(
            "UPDATE discord_events SET actor = 'hub-admin' WHERE actor = ?",
            [&op.id],
        )?;
        self.db
            .execute("DELETE FROM verifications WHERE operator_id = ?", [&op.id])?;
        let gone = forgotten_as(&op.discord_id);
        for kind in COUNTED {
            self.db.execute(
                "UPDATE operator_events SET operator_id = ?1, details = '{}'
                 WHERE operator_id = ?2 AND kind = ?3 AND at >= ?4",
                params![gone, op.id, kind, self.since(DAY)],
            )?;
        }
        self.db.execute(
            "DELETE FROM operator_events WHERE operator_id = ?",
            [&op.id],
        )?;
        self.forget_lapsed_counts()?;
        self.db
            .execute("DELETE FROM operators WHERE id = ?", [&op.id])?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use dereth_network_protocol::person::{MAX_CARRIED, MAX_PERSON_FRIENDS};
    use dereth_network_protocol::{Address, Federation, PersonOutcome};
    use serde_json::json;

    use super::{limits, PersonAnswer, PersonView, CODE_SECS};
    use crate::operators::Operator;
    use crate::registry::{Registration, Registry};

    struct Fixture {
        clock: Arc<Mutex<SystemTime>>,
        r: Registry,
    }

    impl Fixture {
        fn advance(&self, secs: u64) {
            *self.clock.lock().expect("the clock") += Duration::from_secs(secs);
        }
    }

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// A registry with the worlds `aa`, `bb` and `cc` on the network and `dd` off it.
    fn fixture() -> Fixture {
        let clock = Arc::new(Mutex::new(UNIX_EPOCH + Duration::from_secs(1_790_000_000)));
        let c = Arc::clone(&clock);
        let r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(move || *c.lock().expect("the clock")),
        )
        .expect("a registry");
        for slug in ["aa", "bb", "cc", "dd"] {
            let reg = Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration");
            r.register(&reg).expect("registered");
            if slug != "dd" {
                r.set_federation(slug, Federation::On).expect("on");
            }
        }
        Fixture { clock, r }
    }

    fn person(f: &Fixture, discord: &str) -> Operator {
        f.r.signed_in(discord, &format!("Player {discord}"))
            .expect("signed in")
    }

    /// `who`'s world puts it on the network, and so lists it in the directory.
    fn on(f: &Fixture, who: &str) {
        let profile = dereth_network_protocol::Profile {
            level: 20,
            heritage: 1,
            gender: 1,
            ..dereth_network_protocol::Profile::default()
        };
        f.r.listing(&a(who), Some(&profile)).expect("listed");
    }

    /// `who`'s world takes it off the network, and so out of the directory.
    fn off(f: &Fixture, who: &str) {
        f.r.listing(&a(who), None).expect("taken out");
    }

    /// Links `who` to `op` the way a player does: its world, which has it on the network, asks for
    /// a code, and the player enters it.
    fn link(f: &Fixture, op: &Operator, who: &str) {
        on(f, who);
        let (code, _) = f.r.person_code(&a(who)).expect("a code");
        f.r.redeem_link_code(op, &code).expect("linked");
    }

    fn names(list: &[Address]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    /// `who`'s view, cut as the routing cuts it while nobody is online and nobody seen.
    fn view(f: &Fixture, who: &str) -> PersonView {
        crate::core::cut_view(f.r.person_view(&a(who)).expect("a view"), |_| None)
    }

    /// A name for the number `n`, below 17,576, in three letters.
    fn letters(n: usize) -> String {
        let letter = |d: usize| char::from(b'a' + u8::try_from(d % 26).expect("small"));
        [n / 676, n / 26, n].iter().map(|&d| letter(d)).collect()
    }

    /// The handles of a view's person friends.
    fn handles(v: &PersonView) -> Vec<String> {
        v.persons.iter().map(|p| p.handle.to_string()).collect()
    }

    /// Every address a view names, handles and characters alike.
    fn named(v: &PersonView) -> Vec<String> {
        v.persons
            .iter()
            .flat_map(|p| std::iter::once(&p.handle).chain(&p.characters))
            .map(Address::key)
            .collect()
    }

    /// Spec: hub: `characters` names a person's characters for any one of them, the asked one
    /// first, and none for a character nobody linked or one that does not exist, alike.
    #[test]
    fn a_persons_characters_are_named_for_any_one_of_them() {
        let f = fixture();
        let bob = person(&f, "200");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@aa")).expect("asked")),
            ["Bob@aa", "Bob@bb"]
        );
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@bb")).expect("asked")),
            ["Bob@bb", "Bob@aa"]
        );
        let nobody = f.r.person_characters(&a("Nobody@aa")).expect("asked");
        assert!(nobody.is_empty());
        assert_eq!(
            nobody,
            f.r.person_characters(&a("Ghost@cc")).expect("asked")
        );
    }

    /// Spec: hub: every person friend of a person holds for each of its characters, from whichever
    /// of them it was added.
    #[test]
    fn every_person_friend_holds_for_each_of_the_persons_characters() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        assert_eq!(
            f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))
                .expect("ok")
                .outcome,
            PersonOutcome::Ok
        );
        assert_eq!(
            f.r.person_befriend(&a("Ann@bb"), &a("Cat@cc"))
                .expect("ok")
                .outcome,
            PersonOutcome::Ok
        );
        let both = ["bob@cc", "bob@cc", "cat@cc", "cat@cc"];
        for who in ["Ann@aa", "Ann@bb"] {
            let v = view(&f, who);
            assert!(v.linked);
            assert_eq!(named(&v), both, "{who}");
        }
    }

    /// Spec: hub: a person friend lists its handle and every other character of the handle's
    /// person; a character nobody linked, its handle alone.
    #[test]
    fn a_person_friend_lists_every_character_of_its_person() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        link(&f, &ann, "Ann@cc");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        f.r.person_befriend(&a("Ann@cc"), &a("Bob@aa"))
            .expect("befriended");
        f.r.person_befriend(&a("Ann@cc"), &a("Nobody@bb"))
            .expect("befriended");
        let v = view(&f, "Ann@cc");
        assert_eq!(names(&v.persons[0].characters), ["Bob@aa", "Bob@bb"]);
        assert_eq!(names(&v.persons[1].characters), ["Nobody@bb"]);
    }

    /// Spec: hub: a linked character off the network pauses its link: its person's friends' views
    /// name it no more, but as the handle a friend added, and its person's other characters' views
    /// keep their friends. Back on the network it is named again, with nothing to redo; held out
    /// of the directory by the hub's operator it stays on the network.
    #[test]
    fn a_character_off_the_network_pauses_its_link_until_it_is_back_on() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        link(&f, &ann, "Ann@cc");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        f.r.person_befriend(&a("Ann@cc"), &a("Bob@aa"))
            .expect("befriended");
        f.r.person_befriend(&a("Bob@bb"), &a("Ann@cc"))
            .expect("befriended");
        let of_ann = |f: &Fixture| names(&view(f, "Ann@cc").persons[0].characters);
        assert_eq!(of_ann(&f), ["Bob@aa", "Bob@bb"]);
        off(&f, "Bob@bb");
        assert_eq!(of_ann(&f), ["Bob@aa"], "off the network: not named");
        assert!(view(&f, "Bob@aa").linked, "still linked");
        assert_eq!(handles(&view(&f, "Bob@aa")), ["Ann@cc"]);
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@aa")).expect("asked")),
            ["Bob@aa"]
        );
        off(&f, "Bob@aa");
        assert_eq!(of_ann(&f), ["Bob@aa"], "a handle is named always");
        on(&f, "Bob@bb");
        on(&f, "Bob@aa");
        assert_eq!(of_ann(&f), ["Bob@aa", "Bob@bb"], "back on: named again");
        f.r.take_down("Bob@bb").expect("taken down");
        assert_eq!(of_ann(&f), ["Bob@aa", "Bob@bb"], "held out: still on");
        assert_eq!(
            names(&f.r.characters_of(&bob).expect("asked")),
            ["Bob@aa", "Bob@bb"]
        );
    }

    /// Spec: hub: an unlinked character is at once, to every answer, a character nobody linked,
    /// but for the friends handed back to it in its own view until its world is sent them.
    #[test]
    fn an_unlinked_character_is_at_once_a_character_nobody_linked() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        link(&f, &ann, "Ann@cc");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        f.r.person_befriend(&a("Ann@cc"), &a("Bob@aa"))
            .expect("befriended");
        f.r.person_befriend(&a("Bob@bb"), &a("Ann@cc"))
            .expect("befriended");
        assert_eq!(
            f.r.person_unlink(&a("Bob@bb")).expect("unlinked").outcome,
            PersonOutcome::Ok
        );
        let handed_back = view(&f, "Bob@bb");
        assert!(!handed_back.linked);
        assert_eq!(
            handles(&handed_back),
            ["Ann@cc"],
            "the friends that held for it, each its handle alone"
        );
        f.r.returns_sent(&a("Bob@bb")).expect("sent");
        assert_eq!(view(&f, "Bob@bb"), PersonView::default());
        assert!(f
            .r
            .person_characters(&a("Bob@bb"))
            .expect("asked")
            .is_empty());
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@aa")).expect("asked")),
            ["Bob@aa"]
        );
        assert_eq!(names(&view(&f, "Ann@cc").persons[0].characters), ["Bob@aa"]);
        assert_eq!(
            f.r.person_unlink(&a("Bob@bb")).expect("asked").outcome,
            PersonOutcome::NotLinked
        );
        // The person friend added from it stays the person's.
        assert_eq!(named(&view(&f, "Bob@aa")), ["ann@cc", "ann@cc"]);
    }

    /// Spec: hub: a person's characters are named only on worlds on the network; a handle always.
    #[test]
    fn a_character_on_a_world_off_the_network_is_named_only_as_a_handle() {
        let f = fixture();
        let bob = person(&f, "200");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@dd");
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@aa")).expect("asked")),
            ["Bob@aa"]
        );
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@dd")).expect("asked")),
            ["Bob@dd", "Bob@aa"]
        );
    }

    /// Spec: hub: `befriend` needs the asker linked, is `ok` for any character's address, and is
    /// refused past the person friends a person may list.
    #[test]
    fn befriending_needs_a_linked_asker_and_answers_ok_for_any_character() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        assert_eq!(
            f.r.person_befriend(&a("Ann@aa"), &a("Bob@bb"))
                .expect("asked")
                .outcome,
            PersonOutcome::NotLinked
        );
        link(&f, &ann, "Ann@aa");
        link(&f, &bob, "Bob@bb");
        for target in ["Bob@bb", "Nobody@cc", "Ghost@zz"] {
            assert_eq!(
                f.r.person_befriend(&a("Ann@aa"), &a(target))
                    .expect("asked")
                    .outcome,
                PersonOutcome::Ok,
                "{target}"
            );
        }
        for i in 3..limits::FRIENDS {
            f.r.person_befriend(&a("Ann@aa"), &a(&format!("Friend {}@cc", letters(i))))
                .expect("asked");
        }
        assert_eq!(
            f.r.person_befriend(&a("Ann@aa"), &a("One More@cc"))
                .expect("asked")
                .outcome,
            PersonOutcome::Refused
        );
        assert_eq!(
            f.r.person_befriend(&a("Ann@aa"), &a("Bob@bb"))
                .expect("asked")
                .outcome,
            PersonOutcome::Ok,
            "one it lists already"
        );
    }

    /// Spec: hub: `unfriend` from any of a person's characters removes the person friend, from
    /// whichever of them it was added.
    #[test]
    fn unfriending_from_any_character_removes_the_persons_friend() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))
            .expect("befriended");
        f.r.person_befriend(&a("Ann@bb"), &a("Bob@cc"))
            .expect("befriended");
        f.r.person_unfriend(&a("Ann@bb"), &a("Bob@cc"))
            .expect("unfriended");
        assert!(view(&f, "Ann@aa").persons.is_empty());
        assert!(view(&f, "Ann@bb").persons.is_empty());
    }

    /// Spec: hub: a code works once, for ten minutes, a new one replaces the last, and a code the
    /// hub never gave, a spent one and a lapsed one are one answer.
    #[test]
    fn a_link_code_works_once_for_ten_minutes_and_wrong_ones_are_answered_alike() {
        let f = fixture();
        let ann = person(&f, "100");
        let wrong = |code: &str| {
            let e = f.r.redeem_link_code(&ann, code).expect_err("refused");
            (e.status, e.reason, e.code, e.message)
        };
        let (first, secs) = f.r.person_code(&a("Ann@aa")).expect("a code");
        assert_eq!(secs, CODE_SECS);
        assert_eq!(CODE_SECS, 600);
        let (second, _) = f.r.person_code(&a("Ann@aa")).expect("a code");
        assert_ne!(first, second);
        let replaced = wrong(&first);
        assert_eq!(replaced.0, 422);
        assert_eq!(replaced.1, Some("no_such_code"));
        assert_eq!(replaced.2, Some("LINK_FAILED"));
        assert_eq!(wrong("0000-0000"), replaced, "never given");
        assert_eq!(wrong("not a code"), replaced, "not a code at all");
        let linked =
            f.r.redeem_link_code(&ann, &second.to_lowercase().replace('-', " "))
                .expect("typed any way");
        assert_eq!(linked["character"]["address"], "Ann@aa");
        assert_eq!(wrong(&second), replaced, "spent");
        let (late, _) = f.r.person_code(&a("Ann@bb")).expect("a code");
        f.advance(u64::from(CODE_SECS));
        assert_eq!(wrong(&late), replaced, "lapsed");
    }

    /// Spec: hub: a person may enter ten wrong codes an hour.
    #[test]
    fn wrong_codes_are_limited_for_one_person() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        for _ in 0..limits::WRONG_CODES_PER_HOUR {
            assert_eq!(
                f.r.redeem_link_code(&ann, "0000-0000")
                    .map_err(|e| e.status),
                Err(422)
            );
        }
        let (code, _) = f.r.person_code(&a("Ann@aa")).expect("a code");
        let limited = f.r.redeem_link_code(&ann, &code).expect_err("limited");
        assert_eq!(limited.status, 429);
        assert!(limited.retry_after.is_some());
        assert!(
            f.r.redeem_link_code(&bob, &code).is_ok(),
            "another person's entries are its own"
        );
        f.advance(3_601);
        assert_eq!(
            f.r.redeem_link_code(&ann, "0000-0000")
                .map_err(|e| e.status),
            Err(422)
        );
    }

    /// Spec: hub: five codes an hour for one character, and at most 200 for one world's characters
    /// while a code lasts.
    #[test]
    fn codes_are_limited_for_one_character_and_one_world() {
        let f = fixture();
        for _ in 0..limits::CODES_PER_CHARACTER {
            assert!(f.r.person_code(&a("Ann@aa")).is_ok());
        }
        assert_eq!(
            f.r.person_code(&a("Ann@aa")),
            Err(PersonOutcome::RateLimited)
        );
        assert!(f.r.person_code(&a("Ann@bb")).is_ok(), "another character");
        f.advance(3_601);
        assert!(f.r.person_code(&a("Ann@aa")).is_ok());
        let letters = |n: usize| {
            format!(
                "{}{}",
                char::from(b'a' + u8::try_from(n / 26).expect("small")),
                char::from(b'a' + u8::try_from(n % 26).expect("small"))
            )
        };
        for i in 1..limits::CODES_PER_WORLD {
            assert!(
                f.r.person_code(&a(&format!("Many {}@aa", letters(i))))
                    .is_ok(),
                "{i}"
            );
        }
        assert_eq!(
            f.r.person_code(&a("One More@aa")),
            Err(PersonOutcome::RateLimited)
        );
        assert!(f.r.person_code(&a("One More@bb")).is_ok(), "another world");
        f.advance(u64::from(CODE_SECS));
        assert!(
            f.r.person_code(&a("One More@aa")).is_ok(),
            "lapsed codes are no longer held"
        );
    }

    /// Spec: hub: spending a code, as unlinking does, frees none of a world's allowance of codes.
    #[test]
    fn spending_link_codes_frees_none_of_a_worlds_allowance() {
        let f = fixture();
        let letters = |n: usize| {
            format!(
                "{}{}",
                char::from(b'a' + u8::try_from(n / 26).expect("small")),
                char::from(b'a' + u8::try_from(n % 26).expect("small"))
            )
        };
        for i in 0..limits::CODES_PER_WORLD {
            let who = a(&format!("Many {}@aa", letters(i)));
            assert!(f.r.person_code(&who).is_ok(), "{i}");
            assert_eq!(
                f.r.person_unlink(&who).expect("asked").outcome,
                PersonOutcome::NotLinked,
                "its code spent"
            );
        }
        let held: i64 =
            f.r.db
                .query_row("SELECT count(*) FROM person_codes", [], |r| r.get(0))
                .expect("a count");
        assert_eq!(held, 0, "every code spent");
        assert_eq!(
            f.r.person_code(&a("One More@aa")),
            Err(PersonOutcome::RateLimited)
        );
        f.advance(u64::from(CODE_SECS));
        assert!(f.r.person_code(&a("One More@aa")).is_ok());
    }

    /// Spec: hub: a world removed from the registry takes its characters' links with it, so a world
    /// registered later under its slug inherits none.
    #[test]
    fn a_world_registered_under_a_removed_worlds_slug_inherits_no_link() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        let (code, _) = f.r.person_code(&a("Bob@aa")).expect("a code");
        f.r.remove("aa").expect("removed");
        let reg = Registration::from_json(&json!({
            "slug": "aa", "name": "Another aa", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.2", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        f.r.register(&reg).expect("the slug is free");
        f.r.set_federation("aa", Federation::On).expect("on");
        assert_eq!(view(&f, "Ann@aa"), PersonView::default());
        assert_eq!(
            names(&f.r.person_characters(&a("Ann@bb")).expect("asked")),
            ["Ann@bb"]
        );
        let bob = person(&f, "200");
        assert_eq!(
            f.r.redeem_link_code(&bob, &code).map_err(|e| e.reason),
            Err(Some("no_such_code")),
            "the removed world's code is gone too"
        );
    }

    /// Spec: hub: a code is kept as its digest, and a person's events name no character.
    #[test]
    fn a_code_is_kept_as_its_digest_and_events_name_no_character() {
        let f = fixture();
        let ann = person(&f, "100");
        let (code, _) = f.r.person_code(&a("Ann@aa")).expect("a code");
        let stored: String =
            f.r.db
                .query_row("SELECT code_hash FROM person_codes", [], |r| r.get(0))
                .expect("a row");
        assert!(!stored.contains(&code) && !stored.contains(&code.replace('-', "")));
        f.r.redeem_link_code(&ann, &code).expect("linked");
        f.r.person_unlink(&a("Ann@aa")).expect("unlinked");
        let mut q =
            f.r.db
                .prepare("SELECT kind, details FROM operator_events WHERE operator_id = ?")
                .expect("a query");
        let events: Vec<(String, String)> = q
            .query_map([&ann.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("rows")
            .collect::<Result<_, _>>()
            .expect("rows");
        assert!(events.iter().any(|(k, _)| k == "linked"));
        assert!(events.iter().any(|(k, _)| k == "unlinked"));
        for (_, details) in &events {
            assert!(!details.to_lowercase().contains("ann@"), "{details}");
        }
    }

    /// Spec: hub: the registry's audit log, which a world's operator reads, records nothing of
    /// persons.
    #[test]
    fn a_worlds_audit_log_records_nothing_of_its_characters_persons() {
        let f = fixture();
        let ann = person(&f, "100");
        let before = f.r.audit("aa").expect("the audit");
        link(&f, &ann, "Ann@aa");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@aa"))
            .expect("befriended");
        f.r.person_unlink(&a("Ann@aa")).expect("unlinked");
        assert_eq!(f.r.audit("aa").expect("the audit"), before);
    }

    /// Spec: hub: a character linked to another person moves to the one who entered its code.
    #[test]
    fn a_character_moves_to_the_person_who_entered_its_code() {
        let f = fixture();
        let (ann, mal) = (person(&f, "100"), person(&f, "666"));
        link(&f, &mal, "Ann@aa");
        link(&f, &ann, "Ann@aa");
        assert_eq!(
            f.r.person_json(&mal).expect("shown")["characters"],
            json!([])
        );
        assert_eq!(
            f.r.person_json(&ann).expect("shown")["characters"][0]["address"],
            "Ann@aa"
        );
    }

    /// Spec: hub: a person links at most 20 characters.
    #[test]
    fn a_person_links_at_most_twenty_characters() {
        let f = fixture();
        let ann = person(&f, "100");
        let letters = |n: usize| char::from(b'a' + u8::try_from(n).expect("small"));
        for i in 0..limits::CHARACTERS {
            link(&f, &ann, &format!("Ann {}@bb", letters(i)));
        }
        let (code, _) = f.r.person_code(&a("Ann@cc")).expect("a code");
        let e = f.r.redeem_link_code(&ann, &code).expect_err("refused");
        assert_eq!((e.status, e.reason), (409, Some("too_many_characters")));
        link(&f, &ann, &format!("Ann {}@bb", letters(0)));
    }

    /// Spec: hub: the site shows a person only itself, and a character that is not its own is
    /// answered alike whether another person linked it or nobody did.
    #[test]
    fn a_person_is_shown_only_itself() {
        let f = fixture();
        let (ann, bob) = (person(&f, "100"), person(&f, "200"));
        link(&f, &ann, "Ann@aa");
        link(&f, &bob, "Bob@bb");
        f.r.person_befriend(&a("Bob@bb"), &a("Ann@aa"))
            .expect("befriended");
        let me = f.r.person_json(&ann).expect("shown").to_string();
        assert!(!me.contains("Bob@bb") && !me.contains("200"), "{me}");
        let theirs = f.r.unlink_for(&ann, "Bob@bb").expect_err("not hers");
        let nobodys = f.r.unlink_for(&ann, "Nobody@bb").expect_err("not hers");
        assert_eq!(theirs, nobodys);
        assert_eq!(theirs.status, 404);
        assert_eq!(
            f.r.person_json(&bob).expect("shown")["characters"][0]["address"],
            "Bob@bb"
        );
        assert_eq!(
            f.r.unfriend_for(&ann, "Ann@aa").map_err(|e| e.status),
            Err(404),
            "Bob's person friend is Bob's"
        );
        f.r.unlink_for(&ann, "ann@AA")
            .expect("her own, in any case");
    }

    /// Spec: hub: forgetting a person removes its links and its person friends, and an account
    /// that holds nothing else.
    #[test]
    fn forgetting_a_person_leaves_nothing_of_it() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))
            .expect("befriended");
        assert!(f.r.redeem_link_code(&ann, "0000-0000").is_err());
        f.r.new_session(&ann).expect("a session");
        assert!(f.r.forget_person(&ann).expect("forgotten"));
        for c in ["Ann@aa", "Ann@bb"] {
            assert_eq!(handles(&view(&f, c)), ["Bob@cc"], "{c} keeps its friend");
            assert!(!view(&f, c).linked);
            f.r.returns_sent(&a(c)).expect("sent");
            assert_eq!(view(&f, c), PersonView::default());
        }
        for table in [
            "person_characters",
            "person_friends",
            "operators",
            "operator_sessions",
        ] {
            let n: i64 =
                f.r.db
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                    .expect("a count");
            assert_eq!(n, 0, "{table}");
        }
        // What a limit counts stays a day, under no account and naming nothing.
        let mut q =
            f.r.db
                .prepare("SELECT operator_id, kind, details FROM operator_events")
                .expect("a query");
        let kept: Vec<(String, String, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("rows")
            .collect::<Result<_, _>>()
            .expect("rows");
        for (who, kind, details) in &kept {
            assert_eq!(who, &super::forgotten_as(&ann.discord_id));
            assert_eq!(details, "{}", "{kind}");
        }
        assert!(
            !kept.is_empty() && kept.iter().all(|(_, kind, _)| kind == "wrong-code"),
            "{kept:?}"
        );
    }

    /// Spec: hub: forgetting a person and signing in again with the same Discord account starts
    /// none of its limits over: its wrong codes still count, for that account alone, until they
    /// lapse.
    #[test]
    fn signing_in_again_after_forgetting_starts_no_wrong_code_count_over() {
        let f = fixture();
        let ann = person(&f, "100");
        let wrong = |op: &Operator| {
            f.r.redeem_link_code(op, "0000-0000")
                .map(|_| ())
                .map_err(|e| e.status)
        };
        for _ in 0..limits::WRONG_CODES_PER_HOUR {
            assert_eq!(wrong(&ann), Err(422));
        }
        assert!(
            f.r.forget_person(&ann).expect("forgotten"),
            "the account went"
        );
        let again = person(&f, "100");
        assert_eq!(wrong(&again), Err(429), "the same Discord account");
        assert_eq!(wrong(&person(&f, "200")), Err(422), "another's are its own");
        f.advance(3_601);
        assert_eq!(wrong(&again), Err(422), "lapsed");
    }

    /// Spec: hub: forgetting an account that withdrew its worlds, and signing in again, starts no
    /// day's registrations over.
    #[test]
    fn signing_in_again_after_forgetting_starts_no_registration_count_over() {
        let f = fixture();
        let reg = |slug: &str| {
            Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "8.8.8.8", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration")
        };
        let op = person(&f, "100");
        for i in 0..crate::operators::limits::REGISTRATIONS_PER_DAY {
            let slug = format!("r{i}a");
            f.r.register_for(&op, &reg(&slug)).expect("registered");
            f.r.withdraw(&op, &slug).expect("withdrawn");
        }
        assert!(
            f.r.forget_person(&op).expect("forgotten"),
            "it held nothing"
        );
        let again = person(&f, "100");
        assert_eq!(
            f.r.register_for(&again, &reg("one-more"))
                .map(|_| ())
                .map_err(|e| e.status),
            Err(429)
        );
        assert!(
            f.r.register_for(&person(&f, "200"), &reg("theirs")).is_ok(),
            "another account's"
        );
    }

    /// Spec: hub: a person with a `pending` world is an operator, and forgetting it keeps its
    /// account until the world lapses.
    #[test]
    fn a_pending_world_keeps_an_account_until_it_lapses() {
        let f = fixture();
        let reg = Registration::from_json(&json!({
            "slug": "mine", "name": "World mine", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "8.8.8.8", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        let op = person(&f, "100");
        f.r.register_for(&op, &reg).expect("registered");
        assert!(
            !f.r.forget_person(&op).expect("forgotten"),
            "its pending world is held"
        );
        f.advance(15 * 86_400);
        assert!(
            f.r.forget_person(&op).expect("forgotten"),
            "its pending world lapsed"
        );
    }

    /// Spec: hub: forgetting a person that is not an operator removes its reports, and signing in
    /// again starts no day's reports over.
    #[test]
    fn signing_in_again_after_forgetting_starts_no_report_count_over() {
        let f = fixture();
        let reg = Registration::from_json(&json!({
            "slug": "ee", "name": "World ee", "ruleset": "PvE",
            "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
            "emulator": { "family": "ACE" },
        }))
        .expect("a registration");
        f.r.register(&reg).expect("registered");
        let reports = |op: &Operator| -> usize {
            f.r.my_reports(op).expect("its reports")["reports"]
                .as_array()
                .map_or(0, Vec::len)
        };
        let ann = person(&f, "100");
        let worlds = ["aa", "bb", "cc", "dd", "ee"];
        assert_eq!(worlds.len(), crate::operators::limits::REPORTS_PER_DAY);
        for slug in worlds {
            f.r.report(&ann, slug, "spam", "x").expect("reported");
        }
        assert!(
            f.r.forget_person(&ann).expect("forgotten"),
            "it held no world and no claim"
        );
        let again = person(&f, "100");
        assert_eq!(reports(&again), 0, "its reports went with it");
        assert_eq!(
            f.r.report(&again, "aa", "spam", "x")
                .map(|_| ())
                .map_err(|e| e.status),
            Err(429)
        );
        let bob = person(&f, "200");
        f.r.report(&bob, "aa", "spam", "x")
            .expect("another account's");
        f.advance(86_401);
        f.r.report(&again, "aa", "spam", "x").expect("lapsed");
        assert_eq!(reports(&again), 1);
    }

    /// Spec: hub: a world of the network's first version asking to hide or show its character's
    /// person's characters changes nothing, and is refused.
    #[test]
    fn a_worlds_request_to_hide_or_show_a_persons_characters_changes_nothing() {
        let f = fixture();
        let bob = person(&f, "200");
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        assert_eq!(
            f.r.person_alts(&a("Bob@aa")).expect("asked"),
            PersonAnswer::unchanged(PersonOutcome::Refused)
        );
        assert_eq!(
            names(&f.r.person_characters(&a("Bob@bb")).expect("asked")),
            ["Bob@bb", "Bob@aa"],
            "nothing hidden"
        );
        assert_eq!(
            f.r.person_alts(&a("Nobody@aa")).expect("asked").outcome,
            PersonOutcome::NotLinked
        );
    }

    /// Spec: hub: `befriend` counts every person friend of the person, from all of its characters
    /// together, and another person's are its own.
    #[test]
    fn the_friend_limit_counts_every_person_friend_of_the_person() {
        let f = fixture();
        let (bob, cat) = (person(&f, "200"), person(&f, "300"));
        link(&f, &bob, "Bob@aa");
        link(&f, &bob, "Bob@bb");
        link(&f, &cat, "Cat@bb");
        for i in 0..limits::FRIENDS {
            assert_eq!(
                f.r.person_befriend(&a("Bob@aa"), &a(&format!("Friend {}@cc", letters(i))))
                    .expect("asked")
                    .outcome,
                PersonOutcome::Ok
            );
        }
        for who in ["Bob@aa", "Bob@bb"] {
            assert_eq!(
                f.r.person_befriend(&a(who), &a("One More@cc"))
                    .expect("asked")
                    .outcome,
                PersonOutcome::Refused,
                "{who}"
            );
        }
        assert_eq!(
            f.r.person_befriend(&a("Cat@bb"), &a("Dan@cc"))
                .expect("asked")
                .outcome,
            PersonOutcome::Ok,
            "another person's"
        );
    }

    /// Spec: hub: a person request that changes nothing (a world's asking to hide its person's
    /// characters, removing a person friend that does not hold, listing one again) names no
    /// character whose views to send again; one that changes a person names all of its
    /// characters, an unlinked one among them.
    #[test]
    fn a_request_that_changes_nothing_names_no_view_to_send_again() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        let answer = |r: Result<PersonAnswer, crate::registry::RegistryError>| {
            let r = r.expect("asked");
            (r.outcome, names(&r.changed))
        };
        let none = || (PersonOutcome::Ok, Vec::<String>::new());
        let both = || {
            (
                PersonOutcome::Ok,
                vec!["Ann@aa".to_owned(), "Ann@bb".to_owned()],
            )
        };
        assert_eq!(
            answer(f.r.person_alts(&a("Ann@aa"))),
            (PersonOutcome::Refused, vec![])
        );
        assert_eq!(
            answer(f.r.person_unfriend(&a("Ann@aa"), &a("Nobody@cc"))),
            none()
        );
        assert_eq!(
            answer(f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))),
            both()
        );
        assert_eq!(
            answer(f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))),
            none(),
            "listed already"
        );
        assert_eq!(
            answer(f.r.person_unfriend(&a("Ann@bb"), &a("Bob@cc"))),
            both(),
            "from either character"
        );
        assert_eq!(
            answer(f.r.person_unfriend(&a("Ann@aa"), &a("Bob@cc"))),
            none(),
            "gone already"
        );
        assert_eq!(
            answer(f.r.person_unlink(&a("Ann@bb"))),
            both(),
            "the unlinked character among them"
        );
        assert_eq!(
            answer(f.r.person_unlink(&a("Ann@bb"))),
            (PersonOutcome::NotLinked, vec![])
        );
        assert_eq!(names(&f.r.characters_of(&ann).expect("asked")), ["Ann@aa"]);
    }

    /// Spec: hub: a person's whole view names every person friend it lists, in the order they were
    /// added; the view sent holds 100 of them, and the first 100 characters in all.
    #[test]
    fn a_view_holds_the_first_person_friends_within_its_limits_however_many_are_listed() {
        let f = fixture();
        // Ann lists 150 person friends: her whole view names them all, the view sent 100.
        let ann = person(&f, "100");
        let anns = ["Ann@aa", "Ann@bb", "Ann@cc"];
        for c in anns {
            link(&f, &ann, c);
        }
        let listed = MAX_PERSON_FRIENDS * 3 / 2;
        for i in 0..listed {
            let friend = a(&format!("Friend {}@cc", letters(i)));
            f.r.db
                .execute(
                    "INSERT INTO person_friends(person_id, handle_key, handle, added_from,
                       added_from_address, added_at) VALUES (?, ?, ?, 'ann@aa', 'Ann@aa', ?)",
                    rusqlite::params![
                        ann.id,
                        friend.key(),
                        friend.to_string(),
                        format!("2026-10-12T00:{:02}:{:02}.000Z", i / 60, i % 60)
                    ],
                )
                .expect("listed");
        }
        let whole = f.r.person_view(&a("Ann@bb")).expect("a view");
        let all: Vec<String> = (0..listed)
            .map(|j| format!("Friend {}@cc", letters(j)))
            .collect();
        assert_eq!(handles(&whole), all, "every one, in the order added");
        // Nobody seen online: the view sent holds the latest added, in the order they were added.
        let v = view(&f, "Ann@bb");
        assert_eq!(handles(&v), all[listed - MAX_PERSON_FRIENDS..]);
        // Six persons of 20 characters each: each keeps its handle, and the others take
        // what is left of the view's 100 in order.
        let cat = person(&f, "300");
        link(&f, &cat, "Cat@aa");
        let mut handles = Vec::new();
        for p in 0..6 {
            let op = person(&f, &format!("40{p}"));
            for c in 0..limits::CHARACTERS {
                let world = ["aa", "bb", "cc"][c % 3];
                link(&f, &op, &format!("P{} {}@{world}", letters(p), letters(c)));
            }
            let handle = format!("P{} {}@aa", letters(p), letters(0));
            f.r.person_befriend(&a("Cat@aa"), &a(&handle))
                .expect("asked");
            handles.push(handle);
        }
        let v = view(&f, "Cat@aa");
        let listed: Vec<String> = v.persons.iter().map(|p| p.handle.to_string()).collect();
        assert_eq!(listed, handles);
        assert_eq!(
            v.persons.iter().map(|p| p.characters.len()).sum::<usize>(),
            dereth_network_protocol::person::MAX_VIEW_CHARACTERS
        );
        assert_eq!(
            v.persons[5].characters.len(),
            1,
            "the last its handle alone"
        );
    }

    /// Spec: hub: a `carry` lists each friend a character had before it was linked as a person
    /// friend added from it, once, until the limit, and answers `ok` however many it listed.
    #[test]
    fn carrying_lists_a_characters_friends_once_until_the_limit() {
        let f = fixture();
        let ann = person(&f, "100");
        assert_eq!(
            f.r.person_carry(&a("Ann@aa"), &[a("Bob@bb")])
                .expect("asked")
                .outcome,
            PersonOutcome::NotLinked
        );
        link(&f, &ann, "Ann@aa");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@bb"))
            .expect("befriended");
        let answer =
            f.r.person_carry(&a("Ann@aa"), &[a("Bob@bb"), a("Cat@cc"), a("cat@CC")])
                .expect("carried");
        assert_eq!(answer.outcome, PersonOutcome::Ok);
        assert!(!answer.changed.is_empty());
        assert_eq!(handles(&view(&f, "Ann@aa")), ["Bob@bb", "Cat@cc"]);
        let again =
            f.r.person_carry(&a("Ann@aa"), &[a("Cat@cc")])
                .expect("carried");
        assert_eq!(again, PersonAnswer::unchanged(PersonOutcome::Ok));
        let many: Vec<Address> = (0..limits::FRIENDS)
            .map(|i| a(&format!("Friend {}@cc", letters(i))))
            .collect();
        for some in many.chunks(MAX_CARRIED) {
            let full = f.r.person_carry(&a("Ann@aa"), some).expect("carried");
            assert_eq!(full.outcome, PersonOutcome::Ok, "a full list is not told");
        }
        let me = f.r.person_json(&ann).expect("shown");
        assert_eq!(
            me["friends"].as_array().map(Vec::len),
            Some(limits::FRIENDS),
            "listed until the person's limit"
        );
        assert_eq!(view(&f, "Ann@aa").persons.len(), MAX_PERSON_FRIENDS);
    }

    /// Spec: hub: unlinking a character, in game or on the site, hands back to it the friends that
    /// held for it, to its own view alone, and the person keeps its person friends.
    #[test]
    fn unlinking_hands_a_characters_friends_back_to_it() {
        let f = fixture();
        let ann = person(&f, "100");
        link(&f, &ann, "Ann@aa");
        link(&f, &ann, "Ann@bb");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))
            .expect("befriended");
        f.r.person_befriend(&a("Ann@bb"), &a("Cat@cc"))
            .expect("befriended");
        f.r.person_unlink(&a("Ann@aa")).expect("unlinked");
        assert_eq!(
            handles(&view(&f, "Ann@aa")),
            ["Bob@cc", "Cat@cc"],
            "every friend of its person's"
        );
        assert_eq!(handles(&view(&f, "Ann@bb")), ["Bob@cc", "Cat@cc"]);
        f.r.unlink_for(&ann, "Ann@bb").expect("unlinked");
        assert_eq!(handles(&view(&f, "Ann@bb")), ["Bob@cc", "Cat@cc"]);
        let me = f.r.person_json(&ann).expect("shown");
        assert_eq!(
            me["friends"].as_array().map(Vec::len),
            Some(2),
            "the person keeps its person friends"
        );
        assert!(view(&f, "Cat@cc").persons.is_empty());
        f.advance(31 * 86_400);
        link(&f, &ann, "Ann@cc");
        f.r.person_unlink(&a("Ann@cc")).expect("unlinked");
        assert!(
            view(&f, "Ann@aa").persons.is_empty(),
            "friends never sent lapse after 30 days"
        );
    }

    /// Spec: hub: a character moved to another person, or linked again before its world was sent
    /// the friends handed back to it, takes its friends with it.
    #[test]
    fn a_character_takes_its_friends_to_the_person_it_is_linked_to() {
        let f = fixture();
        let (ann, nell) = (person(&f, "100"), person(&f, "200"));
        link(&f, &ann, "Ann@aa");
        f.r.person_befriend(&a("Ann@aa"), &a("Bob@cc"))
            .expect("befriended");
        link(&f, &nell, "Ann@aa");
        assert_eq!(handles(&view(&f, "Ann@aa")), ["Bob@cc"]);
        let nells = f.r.person_json(&nell).expect("shown");
        assert_eq!(nells["friends"][0]["handle"], "Bob@cc");
        f.r.person_unlink(&a("Ann@aa")).expect("unlinked");
        link(&f, &ann, "Ann@aa");
        let v = view(&f, "Ann@aa");
        assert!(v.linked);
        assert_eq!(handles(&v), ["Bob@cc"]);
        f.r.person_unlink(&a("Ann@aa")).expect("unlinked");
        f.r.returns_sent(&a("Ann@aa")).expect("sent");
        link(&f, &nell, "Ann@aa");
        assert_eq!(
            handles(&view(&f, "Ann@aa")),
            ["Bob@cc"],
            "added from it before, so listed once"
        );
        assert_eq!(
            f.r.person_json(&nell).expect("shown")["friends"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }
}
