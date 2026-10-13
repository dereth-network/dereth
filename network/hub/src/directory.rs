//! The character directory (`docs/network/05-hub.md` section 6): the listed characters' current
//! profiles, the holds that keep a character out, and the public routes' answers.
//!
//! A world sends its own characters' profiles, and the hub takes its word for them, as it does for
//! a tell's sender. The directory keeps each listed character's current profile and nothing of its
//! past: no earlier level, no time but the hour its profile last changed and the day a world last
//! sent it.
//!
//! **What is public** is decided here and nowhere else:
//!
//! - only the listings of worlds on the network, each with the fields of a [`Profile`] and no more;
//! - a patron or a monarch only while that character is listed itself, and an address that is
//!   taken out of the directory is taken out of every other listing that named it;
//! - absence alike: a character nobody listed, one taken off the network, one taken down and an
//!   address that names no character are one answer;
//! - nothing of persons: no public answer reads the person layer. A person sees its own
//!   characters' listings on its own signed-in routes, and only there.

use std::time::Duration;

use dereth_network_protocol::address::is_character_name;
use dereth_network_protocol::directory::{
    is_shown_text, Refusal, MAX_ALLEGIANCE_NAME_CHARS, MAX_HERITAGE, MAX_LEVEL,
};
use dereth_network_protocol::{Address, Federation, ListingOutcome, OffBy, Profile};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::core::ListingAnswer;
use crate::registry::{iso, Registry, RegistryError};

type Result<T> = std::result::Result<T, RegistryError>;

/// The most listings the directory keeps for one world's characters. A world names its own
/// characters, so a world that lies is held to this whatever it sends.
pub const MAX_LISTINGS_PER_WORLD: i64 = 20_000;

/// How long a listing no world has sent again stays, in days. A world sends a listed character's
/// profile at each login, so a character played in that time keeps its listing.
pub const KEEP_DAYS: u64 = 90;

/// How long a character the hub's operator took out of the directory is held out, in days,
/// whatever its world does.
pub const HOLD_DAYS: u64 = 30;

/// The most results one search reaches, however it is paged.
pub const MAX_RESULTS: usize = 1_000;

/// The most results one page holds.
pub const MAX_PAGE: usize = 50;

const DAY: u64 = 86_400;

/// The schema the directory adds to the registry (its version 8).
pub(crate) const SCHEMA: &str = "
    CREATE TABLE directory_listings (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      name_key TEXT NOT NULL,
      world TEXT NOT NULL,
      level INTEGER NOT NULL,
      heritage INTEGER NOT NULL,
      allegiance_key TEXT,
      allegiance_name_key TEXT,
      allegiance_rank INTEGER,
      patron_key TEXT,
      monarch_key TEXT,
      profile TEXT NOT NULL,
      updated TEXT NOT NULL,
      sent_day TEXT NOT NULL);
    CREATE INDEX directory_by_world ON directory_listings(world);
    CREATE INDEX directory_by_updated ON directory_listings(updated DESC, name_key, address_key);
    CREATE INDEX directory_by_level ON directory_listings(level DESC, name_key, address_key);
    CREATE INDEX directory_by_allegiance
      ON directory_listings(world, allegiance_key, allegiance_rank DESC, name_key);
    CREATE INDEX directory_by_patron ON directory_listings(patron_key);
    CREATE INDEX directory_by_monarch ON directory_listings(monarch_key);
    CREATE INDEX directory_by_sent ON directory_listings(sent_day);
    CREATE TABLE directory_holds (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      by TEXT NOT NULL,
      owed INTEGER NOT NULL,
      until TEXT NOT NULL);
    CREATE INDEX directory_holds_by_world ON directory_holds(world, owed);
";

/// The hour `iso_time` falls in, as the directory keeps and publishes it.
fn hour_of(iso_time: &str) -> String {
    format!("{}:00:00.000Z", &iso_time[..13])
}

/// The day `iso_time` falls on.
fn day_of(iso_time: &str) -> String {
    iso_time[..10].to_owned()
}

/// Who made a hold, as the registry keeps it: the hub's operator, who holds a character out of the
/// directory for [`HOLD_DAYS`].
const BY_OPERATOR: &str = "operator";

/// A forgotten person's character, as the registry keeps it among the holds: the hub owes its world
/// telling it to take the character off the network (`listing.off`, by its person). It holds the
/// character out of the directory until its world has heard, and no longer, unless the hub's
/// operator held it out first, whose hold then runs its time.
const BY_PERSON: &str = "person";

/// The condition, in SQL, that the hub takes the character whose identity key is `key` to be on
/// the network: its world lists it in the directory, or the hub's operator holds it out of it (it
/// stays on the network). `now` names the parameter that holds the time now.
pub(crate) fn on_network_sql(key: &str, now: &str) -> String {
    format!(
        "({key} IN (SELECT address_key FROM directory_listings)
          OR {key} IN (SELECT address_key FROM directory_holds WHERE until > {now}))"
    )
}

/// `s` percent-encoded for a query: every byte but the unreserved ones.
fn encoded(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// A search's `LIKE` pattern for text that starts `prefix` or holds a word that does, with the
/// pattern's own characters taken literally (escaped with `\`).
fn word_prefix(prefix: &str) -> (String, String) {
    let literal: String = prefix
        .chars()
        .flat_map(|c| match c {
            '\\' | '%' | '_' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    (format!("{literal}%"), format!("% {literal}%"))
}

/// How a search is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// The profiles that changed most lately first (the default).
    Updated,
    /// The highest levels first.
    Level,
}

/// A search of the directory, as its query asks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    /// The start of the name, or of any word in it, in lower case.
    pub q: Option<String>,
    pub world: Option<String>,
    pub level_min: Option<u32>,
    pub level_max: Option<u32>,
    pub heritage: Option<u8>,
    /// The start of the allegiance's name, or of any word in it, in lower case.
    pub allegiance: Option<String>,
    pub sort: Sort,
    pub limit: usize,
    pub offset: usize,
}

impl Search {
    /// A search from a query's fields.
    ///
    /// # Errors
    /// 400 for a field out of its bounds, naming it.
    pub fn from_query(q: &std::collections::HashMap<String, String>) -> Result<Self> {
        let field = |k: &str| q.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());
        let number = |k: &str, max: u64| -> Result<Option<u64>> {
            field(k)
                .map(|v| {
                    v.parse::<u64>()
                        .ok()
                        .filter(|n| (1..=max).contains(n))
                        .ok_or_else(|| RegistryError::bad(format!("{k}: 1 to {max}")))
                })
                .transpose()
        };
        let name = field("q")
            .map(|v| {
                let letters = v.chars().filter(|c| c.is_alphabetic()).count();
                let ok = letters >= 2
                    && v.chars().count() <= 32
                    && v.chars().all(|c| {
                        (c.is_alphabetic() && dereth_network_protocol::text::cp1252_exact(c))
                            || matches!(c, ' ' | '\'' | '-')
                    });
                if ok {
                    Ok(v.to_lowercase())
                } else {
                    Err(RegistryError::bad(
                        "q: the start of a name, at least two letters",
                    ))
                }
            })
            .transpose()?;
        let world = field("world")
            .map(|v| {
                let v = v.to_ascii_lowercase();
                if dereth_network_protocol::address::is_slug(&v) {
                    Ok(v)
                } else {
                    Err(RegistryError::bad("world: a world's slug"))
                }
            })
            .transpose()?;
        let allegiance = field("allegiance")
            .map(|v| {
                if v.chars().count() >= 2 && is_shown_text(v, MAX_ALLEGIANCE_NAME_CHARS) {
                    Ok(v.to_lowercase())
                } else {
                    Err(RegistryError::bad(
                        "allegiance: the start of an allegiance's name, at least two characters",
                    ))
                }
            })
            .transpose()?;
        let level_min = number("level_min", u64::from(MAX_LEVEL))?;
        let level_max = number("level_max", u64::from(MAX_LEVEL))?;
        if let (Some(lo), Some(hi)) = (level_min, level_max) {
            if lo > hi {
                return Err(RegistryError::bad("level_min: at most level_max"));
            }
        }
        let heritage = number("heritage", u64::from(MAX_HERITAGE))?;
        let sort = match field("sort") {
            None | Some("updated") => Sort::Updated,
            Some("level") => Sort::Level,
            Some(_) => return Err(RegistryError::bad("sort: updated or level")),
        };
        let limit = match field("limit") {
            None => MAX_PAGE,
            Some(v) => v
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=MAX_PAGE).contains(n))
                .ok_or_else(|| RegistryError::bad(format!("limit: 1 to {MAX_PAGE}")))?,
        };
        let offset = match field("offset") {
            None => 0,
            Some(v) => v
                .parse::<usize>()
                .map_err(|_| RegistryError::bad("offset: a whole number"))?,
        };
        if offset + limit > MAX_RESULTS {
            return Err(RegistryError::bad(format!(
                "A search reaches its first {MAX_RESULTS} results: narrow it"
            )));
        }
        Ok(Self {
            q: name,
            world,
            level_min: level_min.and_then(|n| u32::try_from(n).ok()),
            level_max: level_max.and_then(|n| u32::try_from(n).ok()),
            heritage: heritage.and_then(|n| u8::try_from(n).ok()),
            allegiance,
            sort,
            limit,
            offset,
        })
    }

    /// The search's `WHERE` clause over `l` (the listings) and its values.
    fn clause(&self) -> (String, Vec<rusqlite::types::Value>) {
        use rusqlite::types::Value as V;
        let mut terms = vec!["s.federation = 'on'".to_owned()];
        let mut values = Vec::new();
        if let Some(q) = &self.q {
            let (start, word) = word_prefix(q);
            terms.push(
                "(l.name_key LIKE ? ESCAPE '\\' OR l.name_key LIKE ? ESCAPE '\\')".to_owned(),
            );
            values.extend([V::Text(start), V::Text(word)]);
        }
        if let Some(w) = &self.world {
            terms.push("l.world = ?".to_owned());
            values.push(V::Text(w.clone()));
        }
        if let Some(n) = self.level_min {
            terms.push("l.level >= ?".to_owned());
            values.push(V::Integer(i64::from(n)));
        }
        if let Some(n) = self.level_max {
            terms.push("l.level <= ?".to_owned());
            values.push(V::Integer(i64::from(n)));
        }
        if let Some(h) = self.heritage {
            terms.push("l.heritage = ?".to_owned());
            values.push(V::Integer(i64::from(h)));
        }
        if let Some(a) = &self.allegiance {
            let (start, word) = word_prefix(a);
            terms.push(
                "(l.allegiance_name_key LIKE ? ESCAPE '\\' OR l.allegiance_name_key LIKE ? ESCAPE '\\')"
                    .to_owned(),
            );
            values.extend([V::Text(start), V::Text(word)]);
        }
        (terms.join(" AND "), values)
    }
}

/// A page's fields: the page, how many matched in all, and where the next page starts.
fn page(data: Vec<Value>, total: usize, limit: usize, offset: usize) -> Value {
    let reach = total.min(MAX_RESULTS);
    json!({
        "data": data, "total": total, "limit": limit, "offset": offset,
        "nextOffset": (offset + limit < reach).then_some(offset + limit),
    })
}

/// One listing as a row of a search or an allegiance's members: who, where, and what the profile
/// says of it at a glance.
fn summary(
    address: &str,
    world: &str,
    world_name: Option<&str>,
    updated: &str,
    p: &Value,
) -> Value {
    let name = address.split('@').next().unwrap_or("");
    let mut v = json!({
        "address": address, "name": name,
        "world": { "slug": world, "name": world_name },
        "updated": updated,
        "level": p["level"], "heritage": p["heritage"], "gender": p["gender"],
    });
    if let Some(t) = p.get("title") {
        v["title"] = t.clone();
    }
    if let Some(a) = p.get("allegiance") {
        let mut al = json!({ "key": a["key"], "rank": a["rank"] });
        if let Some(n) = a.get("name") {
            al["name"] = n.clone();
        }
        v["allegiance"] = al;
    }
    v
}

impl Registry {
    fn hold_until(&self) -> String {
        iso(self.now() + Duration::from_secs(HOLD_DAYS * DAY))
    }

    /// The character's page on the site, when its address fits a page address.
    fn page_url(&self, who: &Address) -> Option<String> {
        let url = format!(
            "{}/directory/character/?a={}",
            self.site,
            encoded(&who.to_string())
        );
        dereth_network_protocol::person::is_page_url(&url).then_some(url)
    }

    /// Whether `world` is on the network.
    fn world_on(&self, world: &str) -> Result<bool> {
        Ok(self
            .federation(world)?
            .is_some_and(|(_, f)| f == Federation::On))
    }

    /// The hold on the character `key` while it lasts: whether its world still owes saying it has
    /// heard of it, and whether it is a forgotten person's character. A forgotten person's
    /// character is held only until its world has heard, unless the hub's operator's hold on it
    /// runs longer.
    fn hold_of(&self, key: &str) -> Result<Option<(bool, bool)>> {
        Ok(self
            .db
            .query_row(
                "SELECT owed, by = ?3 FROM directory_holds
                 WHERE address_key = ?1 AND (until > ?2 OR (by = ?3 AND owed = 1))",
                params![key, iso(self.now()), BY_PERSON],
                |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?)),
            )
            .optional()?)
    }

    /// Whether the hub takes the character `key` to be on the network: its world lists it, or the
    /// hub's operator holds it out of the directory. A linked character that is not pauses its
    /// link (`docs/network/05-hub.md` section 5.4).
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn on_network(&self, key: &str) -> Result<bool> {
        Ok(self.db.query_row(
            &format!("SELECT {}", on_network_sql("?1", "?2")),
            params![key, iso(self.now())],
            |r| r.get(0),
        )?)
    }

    /// The characters of the person who links the character `key`, when that is not on the network
    /// as it was (`was`): every view that names one of them may differ now. None for a character
    /// nobody links, or one whose standing did not change.
    fn standing_changed(&self, key: &str, was: bool) -> Result<Vec<Address>> {
        if self.on_network(key)? == was {
            return Ok(Vec::new());
        }
        let person: Option<String> = self
            .db
            .query_row(
                "SELECT person_id FROM person_characters WHERE address_key = ?",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        match person {
            Some(p) => self.characters_of_person(&p),
            None => Ok(Vec::new()),
        }
    }

    /// `who`'s world lists it with `profile`, or takes it out of the directory (`None`).
    ///
    /// A listing is refused while the hub's operator holds the character out, or while its world
    /// owes hearing that its forgotten person took it off the network, and a new one past the
    /// world's allowance. A profile sent unchanged changes nothing published, not even the hour.
    /// The answer names the characters of the person linking `who` when `who` came onto the
    /// network or left it, its link resuming or pausing.
    ///
    /// # Errors
    /// Database errors only.
    pub fn listing(&self, who: &Address, profile: Option<&Profile>) -> Result<ListingAnswer> {
        let key = who.key();
        let was = self.on_network(&key)?;
        let mut answer = self.apply_listing(who, profile)?;
        answer.changed = self.standing_changed(&key, was)?;
        Ok(answer)
    }

    fn apply_listing(&self, who: &Address, profile: Option<&Profile>) -> Result<ListingAnswer> {
        let answer = |outcome, reason, url| ListingAnswer {
            outcome,
            reason,
            url,
            changed: Vec::new(),
        };
        if !self.world_on(who.world())? {
            return Ok(answer(ListingOutcome::Refused, None, None));
        }
        let key = who.key();
        let hold = self.hold_of(&key)?;
        let Some(profile) = profile else {
            self.remove_listing(&key)?;
            match hold {
                // The world has heard that its forgotten person took the character off: nothing
                // holds it out any more, but a hold of the hub's operator's that runs on.
                Some((_, true)) => {
                    self.db.execute(
                        "DELETE FROM directory_holds WHERE address_key = ?1 AND until <= ?2",
                        params![key, iso(self.now())],
                    )?;
                    self.db.execute(
                        "UPDATE directory_holds SET owed = 0, by = ?2 WHERE address_key = ?1",
                        params![key, BY_OPERATOR],
                    )?;
                }
                // The world has heard that the character is held out: the hold stays its time,
                // owed nothing more.
                Some((_, false)) => {
                    self.db.execute(
                        "UPDATE directory_holds SET owed = 0 WHERE address_key = ?",
                        [&key],
                    )?;
                }
                None => {}
            }
            return Ok(answer(ListingOutcome::Ok, None, None));
        };
        if hold.is_some() {
            return Ok(answer(
                ListingOutcome::Refused,
                Some(Refusal::TakenDown),
                None,
            ));
        }
        let text = serde_json::to_string(profile)
            .map_err(|e| RegistryError::new(500, format!("a profile: {e}")))?;
        let now = iso(self.now());
        let stored: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT profile, address FROM directory_listings WHERE address_key = ?",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match stored {
            Some((p, address)) if p == text && address == who.to_string() => {
                // At most one write a day for a profile sent unchanged, as at each login.
                self.db.execute(
                    "UPDATE directory_listings SET sent_day = ?1 WHERE address_key = ?2
                     AND sent_day != ?1",
                    params![day_of(&now), key],
                )?;
            }
            stored => {
                if stored.is_none() {
                    let held: i64 = self.db.query_row(
                        "SELECT count(*) FROM directory_listings WHERE world = ?",
                        [who.world()],
                        |r| r.get(0),
                    )?;
                    if held >= MAX_LISTINGS_PER_WORLD {
                        return Ok(answer(ListingOutcome::Refused, Some(Refusal::Full), None));
                    }
                }
                let al = profile.allegiance.as_ref();
                self.db.execute(
                    "INSERT INTO directory_listings(address_key, address, name_key, world, level,
                       heritage, allegiance_key, allegiance_name_key, allegiance_rank, patron_key,
                       monarch_key, profile, updated, sent_day)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(address_key) DO UPDATE SET address = excluded.address,
                       name_key = excluded.name_key, level = excluded.level,
                       heritage = excluded.heritage, allegiance_key = excluded.allegiance_key,
                       allegiance_name_key = excluded.allegiance_name_key,
                       allegiance_rank = excluded.allegiance_rank, patron_key = excluded.patron_key,
                       monarch_key = excluded.monarch_key, profile = excluded.profile,
                       updated = excluded.updated, sent_day = excluded.sent_day",
                    params![
                        key,
                        who.to_string(),
                        who.name().to_lowercase(),
                        who.world(),
                        profile.level,
                        profile.heritage,
                        al.map(|a| a.key.clone()),
                        al.and_then(|a| a.name.as_ref()).map(|n| n.to_lowercase()),
                        al.map(|a| a.rank),
                        al.and_then(|a| a.patron.as_ref()).map(Address::key),
                        al.and_then(|a| a.monarch.as_ref()).map(Address::key),
                        text,
                        hour_of(&now),
                        day_of(&now),
                    ],
                )?;
            }
        }
        Ok(answer(ListingOutcome::Ok, None, self.page_url(who)))
    }

    /// Deletes the character `key`'s listing, and its address from every listing that names it
    /// as a patron or a monarch: true when it was listed.
    fn remove_listing(&self, key: &str) -> Result<bool> {
        let n = self.db.execute(
            "DELETE FROM directory_listings WHERE address_key = ?",
            [key],
        )?;
        self.db.execute(
            "UPDATE directory_listings
             SET profile = json_remove(profile, '$.allegiance.patron'), patron_key = NULL
             WHERE patron_key = ?",
            [key],
        )?;
        self.db.execute(
            "UPDATE directory_listings
             SET profile = json_remove(profile, '$.allegiance.monarch'), monarch_key = NULL
             WHERE monarch_key = ?",
            [key],
        )?;
        Ok(n > 0)
    }

    /// Holds `who` out of the directory for [`HOLD_DAYS`], its world owing saying it has heard.
    fn hold(&self, who: &Address) -> Result<()> {
        self.db.execute(
            "INSERT INTO directory_holds(address_key, address, world, by, owed, until)
             VALUES (?, ?, ?, ?, 1, ?)
             ON CONFLICT(address_key) DO UPDATE SET address = excluded.address,
               by = excluded.by, owed = 1, until = excluded.until",
            params![
                who.key(),
                who.to_string(),
                who.world(),
                BY_OPERATOR,
                self.hold_until()
            ],
        )?;
        Ok(())
    }

    /// The hub owes `who`'s world telling it to take `who` off the network: its person forgot
    /// itself. It is out of the directory at once, and held out until its world has heard, however
    /// long its world is away, while the world is in the registry. A hold of the hub's operator's
    /// on it runs its time after that.
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn owe_person_off(&self, who: &Address) -> Result<()> {
        self.remove_listing(&who.key())?;
        let now = iso(self.now());
        self.db.execute(
            "INSERT INTO directory_holds(address_key, address, world, by, owed, until)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(address_key) DO UPDATE SET address = excluded.address, by = excluded.by,
               owed = 1, until = CASE WHEN until > ?5 THEN until ELSE ?5 END",
            params![who.key(), who.to_string(), who.world(), BY_PERSON, now],
        )?;
        Ok(())
    }

    /// The characters of `slug` taken out of the directory whose world has not yet said it has
    /// heard, and who took each out: the hub's operator, or the character's person, forgetting
    /// itself.
    ///
    /// # Errors
    /// Database errors only.
    pub fn owed_offs(&self, slug: &str) -> Result<Vec<(Address, OffBy)>> {
        let mut q = self.db.prepare_cached(
            "SELECT address, by = ?3 FROM directory_holds
             WHERE world = ?1 AND owed = 1 AND (until > ?2 OR by = ?3)
             ORDER BY address_key",
        )?;
        let rows = q
            .query_map(params![slug, iso(self.now()), BY_PERSON], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(a, person)| {
                let by = if person {
                    OffBy::Person
                } else {
                    OffBy::Operator
                };
                Address::parse(&a).ok().map(|a| (a, by))
            })
            .collect())
    }

    /// Listings no world has sent for [`KEEP_DAYS`] go, as taking them out does, and holds past
    /// their time go.
    ///
    /// # Errors
    /// Database errors only.
    pub fn lapse_listings(&self) -> Result<()> {
        let before = day_of(&iso(self
            .now()
            .checked_sub(Duration::from_secs(KEEP_DAYS * DAY))
            .unwrap_or(std::time::UNIX_EPOCH)));
        for column in ["patron", "monarch"] {
            self.db.execute(
                &format!(
                    "UPDATE directory_listings
                     SET profile = json_remove(profile, '$.allegiance.{column}'), {column}_key = NULL
                     WHERE {column}_key IN
                       (SELECT address_key FROM directory_listings WHERE sent_day < ?)"
                ),
                [&before],
            )?;
        }
        self.db.execute(
            "DELETE FROM directory_listings WHERE sent_day < ?",
            [&before],
        )?;
        self.db.execute(
            "DELETE FROM directory_holds WHERE until <= ?1 AND NOT (by = ?2 AND owed = 1)",
            params![iso(self.now()), BY_PERSON],
        )?;
        Ok(())
    }

    /// Forgets a world's listings and holds, as the world leaves the network. What it owes hearing
    /// of forgotten persons' characters stays, for when it comes back: it goes with the world
    /// leaving the registry ([`Registry::forget_worlds_links`]).
    ///
    /// # Errors
    /// Database errors only.
    pub(crate) fn forget_worlds_listings(&self, slug: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM directory_listings WHERE world = ?", [slug])?;
        self.db.execute(
            "DELETE FROM directory_holds WHERE world = ?1 AND NOT (by = ?2 AND owed = 1)",
            params![slug, BY_PERSON],
        )?;
        Ok(())
    }

    /// The hub's operator takes a character out of the directory, and holds it out for
    /// [`HOLD_DAYS`]; its world's log says so. True when it was listed.
    ///
    /// # Errors
    /// 400 for an address that is not a character's; 404 for a world the registry does not have.
    pub fn take_down(&self, address: &str) -> Result<bool> {
        let who = Address::parse(address)
            .ok()
            .filter(|a| !a.is_channel())
            .ok_or_else(|| RegistryError::bad("Name@world: a character's address"))?;
        let row = self
            .row(who.world())?
            .ok_or_else(RegistryError::not_found)?;
        let was = self.remove_listing(&who.key())?;
        self.hold(&who)?;
        self.event(
            &row.id,
            "listing-taken-down",
            &json!({ "address": who.to_string(), "days": HOLD_DAYS }),
        )?;
        Ok(was)
    }

    /// Where the character `key` stands in the directory, for its person alone: `listed`,
    /// `taken_down` (held out by the hub's operator), or `off`.
    pub(crate) fn listing_state(&self, key: &str) -> Result<&'static str> {
        let listed: bool = self.db.query_row(
            "SELECT count(*) > 0 FROM directory_listings WHERE address_key = ?",
            [key],
            |r| r.get(0),
        )?;
        Ok(match (listed, self.hold_of(key)?) {
            (true, _) => "listed",
            (false, Some(_)) => "taken_down",
            (false, None) => "off",
        })
    }

    /// Whether the character `key` is listed, on a world on the network.
    fn is_listed(&self, key: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT count(*) > 0 FROM directory_listings l JOIN servers s ON s.slug = l.world
             WHERE l.address_key = ? AND s.federation = 'on'",
            [key],
            |r| r.get(0),
        )?)
    }

    /// `profile` as published: its patron and monarch only while each is listed.
    fn published(&self, mut profile: Value) -> Result<Value> {
        if let Some(a) = profile.get_mut("allegiance").and_then(Value::as_object_mut) {
            for field in ["patron", "monarch"] {
                let listed = match a.get(field).and_then(Value::as_str) {
                    Some(addr) => {
                        let key = Address::parse(addr).map(|x| x.key()).ok();
                        match key {
                            Some(k) => self.is_listed(&k)?,
                            None => false,
                        }
                    }
                    None => true,
                };
                if !listed {
                    a.remove(field);
                }
            }
        }
        Ok(profile)
    }

    // ---- the public routes -----------------------------------------------------------------------

    /// A page of the listings `search` matches.
    ///
    /// # Errors
    /// Database errors only.
    pub fn directory_search(&self, search: &Search) -> Result<Value> {
        let (clause, values) = search.clause();
        let total: i64 = self.db.query_row(
            &format!(
                "SELECT count(*) FROM directory_listings l JOIN servers s ON s.slug = l.world
                 WHERE {clause}"
            ),
            rusqlite::params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
        let order = match search.sort {
            Sort::Updated => "l.updated DESC, l.name_key, l.address_key",
            Sort::Level => "l.level DESC, l.name_key, l.address_key",
        };
        let mut all = values;
        all.push(rusqlite::types::Value::Integer(
            i64::try_from(search.limit).unwrap_or(0),
        ));
        all.push(rusqlite::types::Value::Integer(
            i64::try_from(search.offset).unwrap_or(0),
        ));
        let mut q = self.db.prepare(&format!(
            "SELECT l.address, l.world, json_extract(s.metadata, '$.name'), l.updated, l.profile
             FROM directory_listings l JOIN servers s ON s.slug = l.world
             WHERE {clause} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?;
        let rows = q
            .query_map(rusqlite::params_from_iter(all.iter()), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let data = rows
            .iter()
            .map(|(address, world, name, updated, p)| {
                let p: Value = serde_json::from_str(p).unwrap_or(Value::Null);
                summary(address, world, name.as_deref(), updated, &p)
            })
            .collect();
        Ok(page(
            data,
            usize::try_from(total).unwrap_or(0),
            search.limit,
            search.offset,
        ))
    }

    /// One listed character, as the public sees it: none, alike, for every other address.
    ///
    /// # Errors
    /// Database errors only.
    pub fn directory_character(&self, address: &str) -> Result<Option<Value>> {
        let Some(key) = Address::parse(address)
            .ok()
            .filter(|a| !a.is_channel() && is_character_name(a.name()))
            .map(|a| a.key())
        else {
            return Ok(None);
        };
        let row: Option<(String, String, Option<String>, String, String)> = self
            .db
            .query_row(
                "SELECT l.address, l.world, json_extract(s.metadata, '$.name'), l.updated, l.profile
                 FROM directory_listings l JOIN servers s ON s.slug = l.world
                 WHERE l.address_key = ? AND s.federation = 'on'",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let Some((address, world, world_name, updated, profile)) = row else {
            return Ok(None);
        };
        let profile = self.published(serde_json::from_str(&profile).unwrap_or(Value::Null))?;
        let mut v = json!({
            "address": address, "name": address.split('@').next().unwrap_or(""),
            "world": { "slug": world, "name": world_name }, "updated": updated,
        });
        if let (Some(o), Some(p)) = (v.as_object_mut(), profile.as_object()) {
            for (k, x) in p {
                o.insert(k.clone(), x.clone());
            }
        }
        Ok(Some(json!({ "character": v })))
    }

    /// An allegiance's listed members on `world`, highest rank first: none, alike, for an
    /// allegiance with no listed member and a key that names none.
    ///
    /// # Errors
    /// Database errors only.
    pub fn directory_allegiance(
        &self,
        world: &str,
        key: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Option<Value>> {
        let world = world.to_ascii_lowercase();
        if !dereth_network_protocol::directory::is_allegiance_key(key) {
            return Ok(None);
        }
        let total: i64 = self.db.query_row(
            "SELECT count(*) FROM directory_listings l JOIN servers s ON s.slug = l.world
             WHERE l.world = ? AND l.allegiance_key = ? AND s.federation = 'on'",
            params![world, key],
            |r| r.get(0),
        )?;
        if total == 0 {
            return Ok(None);
        }
        let mut q = self.db.prepare_cached(
            "SELECT l.address, l.world, json_extract(s.metadata, '$.name'), l.updated, l.profile
             FROM directory_listings l JOIN servers s ON s.slug = l.world
             WHERE l.world = ? AND l.allegiance_key = ? AND s.federation = 'on'
             ORDER BY l.allegiance_rank DESC, l.name_key, l.address_key LIMIT ? OFFSET ?",
        )?;
        let rows = q
            .query_map(
                params![
                    world,
                    key,
                    i64::try_from(limit).unwrap_or(0),
                    i64::try_from(offset).unwrap_or(0)
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        // The allegiance as its most lately changed listed member gives it.
        let head: Option<String> = self
            .db
            .query_row(
                "SELECT profile FROM directory_listings WHERE world = ? AND allegiance_key = ?
                 ORDER BY updated DESC, address_key LIMIT 1",
                params![world, key],
                |r| r.get(0),
            )
            .optional()?;
        let head: Value = head
            .and_then(|h| serde_json::from_str(&h).ok())
            .unwrap_or(Value::Null);
        let mut data = Vec::new();
        for (address, w, name, updated, p) in &rows {
            let p = self.published(serde_json::from_str(p).unwrap_or(Value::Null))?;
            let mut s = summary(address, w, name.as_deref(), updated, &p);
            if let Some(patron) = p["allegiance"].get("patron") {
                s["allegiance"]["patron"] = patron.clone();
            }
            data.push(s);
        }
        let world_name = rows.first().and_then(|r| r.2.clone());
        let mut allegiance = json!({
            "world": { "slug": world, "name": world_name }, "key": key,
            "members": head["allegiance"]["members"], "listed": total,
        });
        if let Some(n) = head["allegiance"].get("name") {
            allegiance["name"] = n.clone();
        }
        let mut v = page(data, usize::try_from(total).unwrap_or(0), limit, offset);
        v["allegiance"] = allegiance;
        Ok(Some(v))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use dereth_network_protocol::directory::{Allegiance, Refusal};
    use dereth_network_protocol::{Address, Federation, ListingOutcome, OffBy, Profile};
    use serde_json::{json, Value};

    use super::{Search, Sort, KEEP_DAYS, MAX_LISTINGS_PER_WORLD};
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

    /// A registry with the worlds `aa` and `bb` on the network and `dd` off it, at a quarter past
    /// an hour.
    fn fixture() -> Fixture {
        let clock = Arc::new(Mutex::new(
            UNIX_EPOCH + Duration::from_secs(1_790_000_000 + 900),
        ));
        let c = Arc::clone(&clock);
        let r = Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(move || *c.lock().expect("the clock")),
        )
        .expect("a registry");
        for slug in ["aa", "bb", "dd"] {
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

    fn profile(level: u32) -> Profile {
        Profile {
            level,
            heritage: 1,
            gender: 1,
            title: Some("the Bloodletter".into()),
            ..Profile::default()
        }
    }

    /// `profile` in the allegiance `key`, at `rank`, under `patron`.
    fn sworn(level: u32, key: &str, rank: u32, patron: Option<&str>) -> Profile {
        Profile {
            allegiance: Some(Allegiance {
                key: key.into(),
                name: Some("Order of the Dawn".into()),
                members: 41,
                rank,
                patron: patron.map(a),
                monarch: None,
            }),
            ..profile(level)
        }
    }

    fn list(f: &Fixture, who: &str, p: &Profile) -> ListingOutcome {
        f.r.listing(&a(who), Some(p)).expect("listed").outcome
    }

    fn page(f: &Fixture, who: &str) -> Option<Value> {
        f.r.directory_character(who)
            .expect("asked")
            .map(|v| v["character"].clone())
    }

    fn rows(f: &Fixture) -> i64 {
        f.r.db
            .query_row("SELECT count(*) FROM directory_listings", [], |r| r.get(0))
            .expect("counted")
    }

    fn query(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn search(f: &Fixture, pairs: &[(&str, &str)]) -> Vec<String> {
        let s = Search::from_query(&query(pairs)).expect("a search");
        f.r.directory_search(&s).expect("searched")["data"]
            .as_array()
            .expect("a page")
            .iter()
            .map(|c| c["address"].as_str().expect("an address").to_owned())
            .collect()
    }

    /// Spec: hub: a world's listing is published at once with every field of its profile and no
    /// other, its page named in the answer, and the hour it changed but never the minute.
    #[test]
    fn a_listing_is_published_at_once_with_its_page() {
        let f = fixture();
        let answer =
            f.r.listing(&a("Sir Bob@aa"), Some(&sworn(126, "dawn-key-1", 3, None)))
                .expect("listed");
        assert_eq!(answer.outcome, ListingOutcome::Ok);
        assert_eq!(
            answer.url.as_deref(),
            Some("https://dereth.network/directory/character/?a=Sir%20Bob%40aa")
        );
        let c = page(&f, "sir bob@AA").expect("published");
        assert_eq!(c["address"], "Sir Bob@aa");
        assert_eq!(c["world"], json!({ "slug": "aa", "name": "World aa" }));
        assert_eq!(c["level"], 126);
        assert_eq!(c["allegiance"]["name"], "Order of the Dawn");
        assert_eq!(c["updated"], "2026-09-21T14:00:00.000Z", "the hour alone");
        assert!(c.get("birth").is_none() && c.get("online").is_none());
    }

    /// Spec: hub: a profile sent again unchanged changes nothing published, not even the hour; a
    /// changed one is published with the hour it changed.
    #[test]
    fn an_unchanged_profile_keeps_the_hour_it_last_changed() {
        let f = fixture();
        list(&f, "Bob@aa", &profile(10));
        f.advance(2 * 3_600);
        list(&f, "Bob@aa", &profile(10));
        assert_eq!(
            page(&f, "Bob@aa").expect("listed")["updated"],
            "2026-09-21T14:00:00.000Z"
        );
        list(&f, "Bob@aa", &profile(11));
        let c = page(&f, "Bob@aa").expect("listed");
        assert_eq!(c["level"], 11);
        assert_eq!(c["updated"], "2026-09-21T16:00:00.000Z");
    }

    /// Spec: hub: taking a character out deletes its listing, and its address from every listing
    /// that named it as a patron or a monarch.
    #[test]
    fn taking_a_character_out_deletes_its_row_and_its_name_elsewhere() {
        let f = fixture();
        list(&f, "Cara@aa", &sworn(200, "dawn-key-1", 5, None));
        list(&f, "Bob@aa", &sworn(126, "dawn-key-1", 3, Some("Cara@aa")));
        assert_eq!(rows(&f), 2);
        assert_eq!(
            f.r.listing(&a("cara@aa"), None).expect("off").outcome,
            ListingOutcome::Ok
        );
        assert_eq!(rows(&f), 1, "deleted, not hidden");
        let stored: String =
            f.r.db
                .query_row("SELECT profile FROM directory_listings", [], |r| r.get(0))
                .expect("Bob's");
        assert!(!stored.to_lowercase().contains("cara"), "{stored}");
        assert!(page(&f, "Cara@aa").is_none());
    }

    /// Spec: hub: a patron or monarch is published only while that character is listed itself, and
    /// an allegiance's listed members are answered highest rank first.
    #[test]
    fn a_patron_is_published_only_while_it_is_listed() {
        let f = fixture();
        list(&f, "Bob@aa", &sworn(126, "dawn-key-1", 3, Some("Cara@aa")));
        let patron =
            |f: &Fixture| page(f, "Bob@aa").expect("listed")["allegiance"]["patron"].clone();
        assert_eq!(patron(&f), Value::Null, "Cara is not listed");
        list(&f, "Cara@aa", &sworn(200, "dawn-key-1", 5, None));
        assert_eq!(patron(&f), "Cara@aa");
        let members =
            f.r.directory_allegiance("aa", "dawn-key-1", 50, 0)
                .expect("asked")
                .expect("two listed");
        assert_eq!(members["allegiance"]["listed"], 2);
        assert_eq!(members["allegiance"]["members"], 41);
        assert_eq!(
            members["data"][0]["address"], "Cara@aa",
            "highest rank first"
        );
        assert_eq!(members["data"][1]["allegiance"]["patron"], "Cara@aa");
    }

    /// Spec: hub: absence is one answer: a character nobody listed, one taken out, one taken down,
    /// one of a world off the network, a channel and text that is no address all answer none.
    #[test]
    fn absence_is_one_answer() {
        let f = fixture();
        list(&f, "Bob@aa", &profile(10));
        list(&f, "Eve@aa", &profile(10));
        f.r.listing(&a("Bob@aa"), None).expect("off");
        f.r.take_down("Eve@aa").expect("taken down");
        assert_eq!(list(&f, "Dan@dd", &profile(10)), ListingOutcome::Refused);
        for absent in [
            "Bob@aa",
            "Eve@aa",
            "Nobody@aa",
            "Dan@dd",
            "Ghost@zz",
            "oldguard@channel",
            "not an address",
            "",
        ] {
            assert_eq!(
                f.r.directory_character(absent).expect("asked"),
                None,
                "{absent:?}"
            );
        }
        assert_eq!(
            f.r.directory_allegiance("aa", "no-such-key", 50, 0)
                .expect("asked"),
            None
        );
    }

    /// Spec: hub: a world that leaves the network, or the registry, takes its characters' listings
    /// and holds with it, deleted.
    #[test]
    fn a_world_leaving_the_network_takes_its_listings() {
        let f = fixture();
        list(&f, "Bob@aa", &profile(10));
        list(&f, "Ann@bb", &profile(10));
        f.r.take_down("Eve@aa").expect("held");
        f.r.set_federation("aa", Federation::Off).expect("off");
        assert_eq!(rows(&f), 1);
        assert!(
            f.r.owed_offs("aa").expect("asked").is_empty(),
            "its holds went too"
        );
        assert!(page(&f, "Ann@bb").is_some());
        f.r.remove("bb").expect("removed");
        assert_eq!(rows(&f), 0);
    }

    /// Spec: hub: a character the hub's operator took down is refused for its days whatever its
    /// world does; its world is owed the off until it says it has heard.
    #[test]
    fn a_hold_refuses_a_listing_for_its_days() {
        let f = fixture();
        list(&f, "Ann@aa", &profile(10));
        f.r.take_down("Ann@aa").expect("taken down");
        assert!(page(&f, "Ann@aa").is_none());
        assert_eq!(
            f.r.owed_offs("aa").expect("asked"),
            vec![(a("Ann@aa"), OffBy::Operator)]
        );
        f.r.listing(&a("Ann@aa"), None).expect("off");
        assert!(
            f.r.owed_offs("aa").expect("asked").is_empty(),
            "owed nothing more"
        );
        let refused =
            f.r.listing(&a("Ann@aa"), Some(&profile(10)))
                .expect("asked");
        assert_eq!(
            (refused.outcome, refused.reason),
            (ListingOutcome::Refused, Some(Refusal::TakenDown)),
            "its days are not over"
        );
        assert!(f
            .r
            .audit("aa")
            .expect("the log")
            .to_string()
            .contains("listing-taken-down"));
        f.advance(31 * 86_400);
        assert_eq!(list(&f, "Ann@aa", &profile(10)), ListingOutcome::Ok);
    }

    /// Spec: hub: forgetting a person takes each of its characters off the network: out of the
    /// directory at once, and its world owed the off by its person, held out until the world says
    /// it has heard, however long it is away, and through its leaving the network; then nothing
    /// holds it out, but a hold of the hub's operator's that runs on. Leaving the registry, the
    /// world takes what it was owed with it.
    #[test]
    fn forgetting_a_person_takes_its_characters_off_the_network() {
        let f = fixture();
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        for who in ["Ann@aa", "Ann@bb", "Eve@aa"] {
            list(&f, who, &profile(10));
            let (code, _) = f.r.person_code(&a(who)).expect("a code");
            f.r.redeem_link_code(&ann, &code).expect("linked");
        }
        f.r.take_down("Eve@aa").expect("taken down");
        f.r.listing(&a("Eve@aa"), None).expect("heard");
        f.r.forget_person(&ann).expect("forgotten");
        assert!(page(&f, "Ann@aa").is_none(), "out at once");
        assert!(page(&f, "Ann@bb").is_none());
        assert_eq!(
            f.r.owed_offs("aa").expect("asked"),
            vec![(a("Ann@aa"), OffBy::Person), (a("Eve@aa"), OffBy::Person)]
        );
        f.advance(40 * 86_400);
        f.r.lapse_listings().expect("lapsed");
        f.r.set_federation("aa", Federation::Off).expect("off");
        f.r.set_federation("aa", Federation::On).expect("on");
        assert_eq!(
            f.r.owed_offs("aa").expect("asked").len(),
            2,
            "owed however long the world is away"
        );
        let held =
            f.r.listing(&a("Ann@aa"), Some(&profile(10)))
                .expect("asked");
        assert_eq!(
            (held.outcome, held.reason),
            (ListingOutcome::Refused, Some(Refusal::TakenDown)),
            "held out until the world has heard"
        );
        f.r.listing(&a("Ann@aa"), None).expect("heard");
        assert_eq!(
            f.r.owed_offs("aa").expect("asked"),
            vec![(a("Eve@aa"), OffBy::Person)]
        );
        assert_eq!(list(&f, "Ann@aa", &profile(10)), ListingOutcome::Ok);
        // The operator's hold ran out while the world was away: once the world has heard, nothing
        // holds the character out.
        f.r.listing(&a("Eve@aa"), None).expect("heard");
        assert_eq!(list(&f, "Eve@aa", &profile(10)), ListingOutcome::Ok);
        f.r.forget_person(&f.r.signed_in("100", "Ann").expect("again"))
            .expect("nothing to forget");
        assert_eq!(
            f.r.owed_offs("bb").expect("asked"),
            vec![(a("Ann@bb"), OffBy::Person)]
        );
        f.r.remove("bb").expect("removed");
        assert!(f.r.owed_offs("bb").expect("asked").is_empty());
    }

    /// Spec: hub: forgetting a person while the hub's operator holds one of its characters out of
    /// the directory keeps the hold its days, once the world has heard the off.
    #[test]
    fn forgetting_a_person_keeps_the_operators_hold_its_days() {
        let f = fixture();
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        list(&f, "Ann@aa", &profile(10));
        let (code, _) = f.r.person_code(&a("Ann@aa")).expect("a code");
        f.r.redeem_link_code(&ann, &code).expect("linked");
        f.r.take_down("Ann@aa").expect("taken down");
        f.r.forget_person(&ann).expect("forgotten");
        assert_eq!(
            f.r.owed_offs("aa").expect("asked"),
            vec![(a("Ann@aa"), OffBy::Person)]
        );
        f.r.listing(&a("Ann@aa"), None).expect("heard");
        assert!(f.r.owed_offs("aa").expect("asked").is_empty());
        let held =
            f.r.listing(&a("Ann@aa"), Some(&profile(10)))
                .expect("asked");
        assert_eq!(
            (held.outcome, held.reason),
            (ListingOutcome::Refused, Some(Refusal::TakenDown))
        );
        f.advance(31 * 86_400);
        assert_eq!(list(&f, "Ann@aa", &profile(10)), ListingOutcome::Ok);
    }

    /// Spec: hub: a listing that puts a linked character on the network or takes it off names its
    /// person's characters, whose views may change; one that changes neither, a character nobody
    /// links, and the world's saying it heard of the operator's off name none.
    #[test]
    fn a_linked_character_coming_on_or_off_names_its_persons_characters() {
        let f = fixture();
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        for who in ["Ann@aa", "Ann@bb"] {
            list(&f, who, &profile(10));
            let (code, _) = f.r.person_code(&a(who)).expect("a code");
            f.r.redeem_link_code(&ann, &code).expect("linked");
        }
        let changed = |who: &str, p: Option<Profile>| -> Vec<String> {
            f.r.listing(&a(who), p.as_ref())
                .expect("asked")
                .changed
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        let both = ["Ann@aa", "Ann@bb"];
        assert!(
            changed("Ann@aa", Some(profile(11))).is_empty(),
            "on already"
        );
        assert_eq!(changed("Ann@aa", None), both, "off");
        assert!(changed("Ann@aa", None).is_empty(), "off already");
        assert_eq!(changed("Ann@aa", Some(profile(11))), both, "back on");
        assert!(
            changed("Bob@aa", Some(profile(11))).is_empty(),
            "nobody links it"
        );
        f.r.take_down("Ann@bb").expect("taken down");
        assert!(
            changed("Ann@bb", None).is_empty(),
            "held out by the operator, it stays on the network"
        );
    }

    /// Spec: hub: a person sees where each of its characters stands in the directory, and no public
    /// answer reads the person layer.
    #[test]
    fn a_person_sees_its_own_characters_listings() {
        let f = fixture();
        let ann = f.r.signed_in("100", "Ann").expect("signed in");
        for who in ["Ann@aa", "Ann@bb"] {
            let (code, _) = f.r.person_code(&a(who)).expect("a code");
            f.r.redeem_link_code(&ann, &code).expect("linked");
        }
        list(&f, "Ann@aa", &profile(10));
        let published = f.r.directory_character("Ann@aa").expect("asked");
        let states = |f: &Fixture| -> Vec<String> {
            f.r.person_json(&ann).expect("the person")["characters"]
                .as_array()
                .expect("characters")
                .iter()
                .map(|c| c["listing"].as_str().expect("a state").to_owned())
                .collect()
        };
        assert_eq!(states(&f), ["listed", "off"]);
        f.r.take_down("Ann@bb").expect("taken down");
        assert_eq!(states(&f), ["listed", "taken_down"]);
        assert!(!published.expect("published").to_string().contains("Ann@bb"));
    }

    /// Spec: hub: a world may hold at most its allowance of listings; one already listed is
    /// changed still, and another world's are its own.
    #[test]
    fn a_world_lists_no_more_than_its_allowance() {
        let f = fixture();
        list(&f, "Bob@aa", &profile(10));
        f.r.db
            .execute(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?)
                 INSERT INTO directory_listings(address_key, address, name_key, world, level,
                   heritage, profile, updated, sent_day)
                 SELECT 'x' || i || '@aa', 'X@aa', 'x', 'aa', 1, 1, '{}',
                   '2026-01-01T00:00:00.000Z', '2026-01-01' FROM n",
                [MAX_LISTINGS_PER_WORLD - 1],
            )
            .expect("filled");
        let refused =
            f.r.listing(&a("Cat@aa"), Some(&profile(10)))
                .expect("asked");
        assert_eq!(
            (refused.outcome, refused.reason),
            (ListingOutcome::Refused, Some(Refusal::Full))
        );
        assert_eq!(list(&f, "Bob@aa", &profile(11)), ListingOutcome::Ok);
        assert_eq!(
            list(&f, "Cat@bb", &profile(10)),
            ListingOutcome::Ok,
            "another world's"
        );
    }

    /// Spec: hub: a listing no world has sent for its keeping time lapses, its address with it
    /// from every other listing, and a hold past its time ends.
    #[test]
    fn a_listing_not_sent_for_its_keeping_time_lapses() {
        let f = fixture();
        list(&f, "Bob@aa", &sworn(10, "dawn-key-1", 2, Some("Cara@aa")));
        list(&f, "Cara@aa", &sworn(20, "dawn-key-1", 3, None));
        f.r.take_down("Eve@aa").expect("held");
        f.advance((KEEP_DAYS - 1) * 86_400);
        list(&f, "Bob@aa", &sworn(10, "dawn-key-1", 2, Some("Cara@aa")));
        f.advance(2 * 86_400);
        f.r.lapse_listings().expect("lapsed");
        assert!(page(&f, "Cara@aa").is_none(), "not sent for 91 days");
        let bob = page(&f, "Bob@aa").expect("sent two days ago");
        assert_eq!(bob["allegiance"].get("patron"), None);
        let stored: String =
            f.r.db
                .query_row(
                    "SELECT profile FROM directory_listings WHERE address_key = 'bob@aa'",
                    [],
                    |r| r.get(0),
                )
                .expect("Bob's");
        assert!(!stored.contains("Cara"), "{stored}");
        let holds: i64 =
            f.r.db
                .query_row("SELECT count(*) FROM directory_holds", [], |r| r.get(0))
                .expect("counted");
        assert_eq!(holds, 0, "the takedown's 30 days are over");
    }

    /// Spec: hub: a search matches the start of a name or of any word in it, and filters by world,
    /// level, heritage and allegiance; it sorts by the latest change, or by level; its fields are
    /// bounded, and it reaches its first thousand results.
    #[test]
    fn a_search_matches_name_words_and_filters_and_sorts() {
        let f = fixture();
        list(&f, "Sir Bob@aa", &sworn(126, "dawn-key-1", 3, None));
        f.advance(3_600);
        list(&f, "Bobby@bb", &profile(50));
        f.advance(3_600);
        list(
            &f,
            "Ann@aa",
            &Profile {
                heritage: 3,
                ..profile(200)
            },
        );
        assert_eq!(
            search(&f, &[]),
            ["Ann@aa", "Bobby@bb", "Sir Bob@aa"],
            "latest first"
        );
        assert_eq!(
            search(&f, &[("sort", "level")]),
            ["Ann@aa", "Sir Bob@aa", "Bobby@bb"]
        );
        assert_eq!(search(&f, &[("q", "bob")]), ["Bobby@bb", "Sir Bob@aa"]);
        assert_eq!(
            search(&f, &[("q", "ob")]),
            Vec::<String>::new(),
            "a word's start"
        );
        assert_eq!(
            search(&f, &[("world", "aa"), ("sort", "level")]),
            ["Ann@aa", "Sir Bob@aa"]
        );
        assert_eq!(
            search(&f, &[("level_min", "60"), ("level_max", "150")]),
            ["Sir Bob@aa"]
        );
        assert_eq!(search(&f, &[("heritage", "3")]), ["Ann@aa"]);
        assert_eq!(search(&f, &[("allegiance", "dawn")]), ["Sir Bob@aa"]);
        assert_eq!(
            search(&f, &[("allegiance", "d%")]).len(),
            0,
            "taken literally"
        );
        assert_eq!(
            Search::from_query(&query(&[])).expect("a search").sort,
            Sort::Updated
        );
        for bad in [
            vec![("q", "b")],
            vec![("q", "bo<b")],
            vec![("sort", "name")],
            vec![("level_min", "0")],
            vec![("level_min", "9"), ("level_max", "8")],
            vec![("heritage", "14")],
            vec![("limit", "51")],
            vec![("offset", "960")],
            vec![("world", "No World")],
        ] {
            assert_eq!(
                Search::from_query(&query(&bad)).map_err(|e| e.status),
                Err(400),
                "{bad:?}"
            );
        }
        assert!(
            Search::from_query(&query(&[("offset", "950")])).is_ok(),
            "the last page of the first thousand"
        );
    }
}
