//! The network's directory: this world's characters on the network, each listed with its
//! Asheron's Call 2 profile (`docs/network/02-link-protocol.md` section 7, the rules of
//! `03-world-behaviour.md` section 10 that an Asheron's Call 2 world keeps).
//!
//! The world composes each profile, since it knows the game; this half decides when it goes:
//!
//! - **On** ([`Federation::listing_on`]): at each login of a character on the network, and at
//!   `@dn on` ([`Federation::set_on`], after which the world hands the profile in:
//!   [`Federation::profiles_wanted`]), at once. A character the hub held out is asked for again
//!   then.
//! - **Changed** ([`Federation::listing_changed`]): while the character plays, at most once every
//!   [`LISTING_EVERY_SECS`], the latest profile then.
//! - **Left** ([`Federation::listing_left`]): at logout, at once, if it changed since the last.
//! - **Off**: `@dn off`, and a character deleted, at once: a listing that is not `on`, owed until
//!   the hub answers it and kept in the store ([`Federation::keep`]), so that a lost link, a
//!   restart or a crash never leaves a character in the directory its player took out. What
//!   takes characters out goes first.
//! - **The hub's `listing.off`**: by its operator, the character is held out until its next login
//!   or `@dn on`; by its person, it goes off the network. Either way the world says it heard, with
//!   a listing that is not `on`, and the player is told ([`Federation::take_notices`]).
//! - **Paced** under the hub's paces for listings, after everything else the link carries; one the
//!   hub held back goes again a minute later, the latest profile then.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dereth_network_protocol::directory::{
    ListingOutcome, OffBy, Refusal, LISTINGS_PER_CHARACTER, LISTINGS_PER_WORLD, LISTING_EVERY_SECS,
};
use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::{Address, AubereanProfile, DirectoryMessage};
use yalain_store::{Store, StoreResult};

use crate::{Federation, Friends};

/// The world's record of the listings it owes that take characters out.
pub const RECORD: &str = "dereth-network-unlistings";

/// How long a listing the hub held back to its pace waits before it goes again, in seconds.
pub const AGAIN_SECS: f64 = 60.0;

/// The line for a character the hub's operator took out of the directory, or refused to list.
pub const TAKEN_DOWN: &str =
    "The Dereth Network's operator has taken this character out of the directory for now.";

/// The line for a character the directory has no room for.
pub const FULL: &str =
    "The Dereth Network's directory holds as many of this world's characters as it may.";

/// The line for a listing on a hub that keeps no directory.
pub const NO_DIRECTORY: &str =
    "This network's hub keeps no directory, so your profile is not in one.";

/// The line for a character its player took off the network by asking the Dereth Network to forget
/// them.
pub const FORGOTTEN: &str = "You asked the Dereth Network to forget you, so this character is off the network now. Say @dn on to join it again.";

/// The most lines kept for characters not yet shown them; past it the oldest goes.
pub const MAX_NOTICES: usize = 1_000;

/// The line naming a character's page in the directory.
#[must_use]
pub fn page_line(url: &str) -> String {
    format!("Your directory page: {url}")
}

/// A tenth under the hub's own `rate`, so the two clocks never disagree at the edge.
fn under(rate: Rate) -> Rate {
    Rate {
        burst: (rate.burst * 0.9).max(1.0),
        per_sec: rate.per_sec * 0.9,
    }
}

/// What the world keeps of a listed character.
#[derive(Debug, Clone)]
struct Entry {
    /// The address it is listed under.
    who: Address,
    /// Its profile as the world last handed it in.
    profile: AubereanProfile,
    /// The profile last sent; none before the first.
    sent: Option<AubereanProfile>,
    /// When its profile goes next, if it is due.
    due: Option<f64>,
    /// It answers `@dn on`: the hub's answer is shown.
    announce: bool,
    /// The hub has not answered the profile last sent.
    owed: bool,
    /// Its page in the directory, as the hub last named it.
    page: Option<String>,
}

/// A listing handed to the link and not yet answered.
#[derive(Debug, Clone)]
struct Sent {
    /// The character it lists (its name in small letters); none for one that takes a character
    /// out.
    name: Option<String>,
    /// The identity key of the address it names.
    key: String,
    announce: bool,
    /// When it went.
    at: f64,
}

/// The directory's state on the world.
#[derive(Debug, Default)]
pub(crate) struct Directory {
    /// The listed characters, by name in small letters.
    entries: BTreeMap<String, Entry>,
    /// Characters put on the network whose profile the world has yet to hand in: their ids,
    /// names as the world writes them, and whether the hub's answer is shown.
    wanted: BTreeMap<String, (u64, String, bool)>,
    /// Characters the hub holds out, sent nothing until their next login or `@dn on`.
    held_out: BTreeSet<String>,
    /// The listings owed that take characters out, by identity key: kept in the store.
    removals: BTreeMap<String, Address>,
    /// [`Directory::removals`] changed since the store last had it.
    dirty: bool,
    /// Removals on the link and not yet answered, by identity key.
    removing: BTreeSet<String>,
    /// Removals the hub held back to its pace, and when they may go again.
    removals_wait: HashMap<String, f64>,
    /// Listings handed to the link and not yet answered, by id.
    sent: HashMap<String, Sent>,
    /// The id of each character's latest listing.
    latest: HashMap<String, String>,
    /// When each character's last listing went.
    last: HashMap<String, f64>,
    /// Each character's listings, paced as the hub paces them.
    paces: HashMap<String, Bucket>,
    /// The world's listings, paced as the hub paces them.
    pace: Option<Bucket>,
    /// Characters their person took off the network, for the store.
    offs: Vec<u64>,
    /// Lines for characters, latest last.
    notices: Vec<(String, String)>,
}

impl Directory {
    /// The removals kept in the store's record: one address a line.
    pub(crate) fn load(record: Option<&[u8]>) -> Self {
        let removals = record
            .and_then(|b| std::str::from_utf8(b).ok())
            .into_iter()
            .flat_map(str::lines)
            .filter_map(|l| Address::parse(l).ok())
            .map(|a| (a.key(), a))
            .collect();
        Self {
            removals,
            ..Self::default()
        }
    }

    fn record(&self) -> Vec<u8> {
        let mut out = String::new();
        for a in self.removals.values() {
            out.push_str(&a.to_string());
            out.push('\n');
        }
        out.into_bytes()
    }

    /// Owes the hub a listing that takes `who` out.
    fn owe_removal(&mut self, who: Address) {
        let key = who.key();
        self.removals_wait.remove(&key);
        self.removals.insert(key, who);
        self.dirty = true;
    }

    /// Marks `name`'s profile due at `at`, unless it is due sooner.
    fn due(&mut self, name: &str, at: f64, announce: bool) {
        if let Some(e) = self.entries.get_mut(name) {
            e.due = Some(e.due.map_or(at, |d| d.min(at)));
            e.announce |= announce;
        }
    }

    fn tell(&mut self, name: &str, line: &str) {
        if self.notices.len() >= MAX_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push((name.to_owned(), line.to_owned()));
    }
}

impl Federation {
    /// Whether a character of this world is listed: on the network, and not held out by the hub.
    #[must_use]
    pub fn listed(&self, name: &str) -> bool {
        self.is_on(name)
            && !self
                .directory
                .held_out
                .contains(name.to_lowercase().as_str())
    }

    /// The characters (their ids, and their names as the world writes them) put on the network
    /// since the world last handed in their profiles: it hands each in with
    /// [`Federation::listing_on`].
    #[must_use]
    pub fn profiles_wanted(&self) -> Vec<(u64, String)> {
        self.directory
            .wanted
            .values()
            .map(|(id, n, _)| (*id, n.clone()))
            .collect()
    }

    /// A character on the network entered the world, or was put on the network: it is listed
    /// with `profile` at once (the hub refuses one it holds out), its page shown to it when it was
    /// put on the network.
    pub fn listing_on(&mut self, t: f64, name: &str, profile: AubereanProfile) {
        let key = name.to_lowercase();
        let announce = self
            .directory
            .wanted
            .remove(&key)
            .is_some_and(|(_, _, a)| a);
        if !self.enabled() || !self.is_on(name) || !self.within(name, &profile) {
            return;
        }
        let Some(who) = self.on_name(name).and_then(|n| self.own_address(n)) else {
            return;
        };
        let d = &mut self.directory;
        d.held_out.remove(&key);
        // A listing supersedes one still owed that takes the character out.
        if d.removals.remove(&who.key()).is_some() {
            d.dirty = true;
        }
        let e = d.entries.entry(key.clone()).or_insert_with(|| Entry {
            who: who.clone(),
            profile: profile.clone(),
            sent: None,
            due: None,
            announce: false,
            owed: false,
            page: None,
        });
        e.who = who;
        e.profile = profile;
        d.due(&key, t, announce);
    }

    /// A listed character's profile as it is now, while it plays: a change goes once the
    /// character's last listing is [`LISTING_EVERY_SECS`] old, the latest profile then.
    pub fn listing_changed(&mut self, t: f64, name: &str, profile: AubereanProfile) {
        if !self.enabled() || !self.listed(name) || !self.within(name, &profile) {
            return;
        }
        let key = name.to_lowercase();
        let last = self.directory.last.get(&key).copied();
        let d = &mut self.directory;
        let Some(e) = d.entries.get_mut(&key) else {
            return;
        };
        if e.profile == profile {
            return;
        }
        e.profile = profile;
        if e.sent.as_ref() != Some(&e.profile) {
            let at = last.map_or(t, |l| t.max(l + LISTING_EVERY_SECS));
            d.due(&key, at, false);
        }
    }

    /// A listed character left the world, with `profile` as it left: it goes at once if it
    /// changed since the last listing.
    pub fn listing_left(&mut self, t: f64, name: &str, profile: AubereanProfile) {
        if !self.enabled() || !self.listed(name) || !self.within(name, &profile) {
            return;
        }
        let key = name.to_lowercase();
        let d = &mut self.directory;
        let Some(e) = d.entries.get_mut(&key) else {
            return;
        };
        e.profile = profile;
        if e.sent.as_ref() != Some(&e.profile) {
            d.due(&key, t, false);
        }
    }

    /// Whether `profile` is within the directory's bounds for `name`; one that is not is never
    /// sent, and logged.
    fn within(&self, name: &str, profile: &AubereanProfile) -> bool {
        let Some(who) = self.own_address(name) else {
            return false;
        };
        match profile.check(&who) {
            Ok(()) => true,
            Err(why) => {
                tracing::warn!(
                    "Dereth Network: {who}'s profile is out of the directory's bounds ({why}); \
                     it is not listed"
                );
                false
            }
        }
    }

    /// The character's page in the directory, as the hub last named it.
    #[must_use]
    pub fn listing_page(&self, name: &str) -> Option<&str> {
        self.directory
            .entries
            .get(&name.to_lowercase())
            .and_then(|e| e.page.as_deref())
    }

    /// The lines the directory has for characters (as the world writes their names), each to be
    /// shown to it, in order, for those `in_world` names; the rest wait for their next login.
    pub fn take_notices(&mut self, in_world: impl Fn(&str) -> bool) -> Vec<(String, String)> {
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.directory.notices)
            .into_iter()
            .partition(|(n, _)| in_world(n));
        self.directory.notices = later;
        now
    }

    /// Writes what the store keeps of the directory, when it changed: the listings owed that take
    /// characters out, and the characters their person took off the network.
    ///
    /// # Errors
    /// The store failed; what failed is written again at the next call.
    pub fn keep(&mut self, store: &mut impl Store) -> StoreResult<()> {
        while let Some(id) = self.directory.offs.last().copied() {
            store.set_network_on(id, false)?;
            self.directory.offs.pop();
        }
        if self.directory.dirty {
            store.set_world_record(RECORD, &self.directory.record())?;
            self.directory.dirty = false;
        }
        self.keep_rooms(store)?;
        Ok(())
    }

    /// Asks the world for `name`'s profile: it was put on the network (`@dn on`).
    pub(crate) fn want_profile(&mut self, id: u64, name: &str) {
        if self.enabled() {
            let key = name.to_lowercase();
            self.directory.held_out.remove(&key);
            self.directory
                .wanted
                .insert(key, (id, name.to_owned(), true));
        }
    }

    /// `name` went off the network (`@dn off`, or deleted): it is taken out of the directory, at
    /// once.
    pub(crate) fn listing_off(&mut self, name: &str) {
        let key = name.to_lowercase();
        let d = &mut self.directory;
        d.wanted.remove(&key);
        d.held_out.remove(&key);
        d.latest.remove(&key);
        let who = d.entries.remove(&key).map(|e| e.who);
        if !self.enabled() {
            return;
        }
        if let Some(who) = who.or_else(|| self.own_address(name)) {
            self.directory.owe_removal(who);
        }
    }

    /// A character on the network took a new name: the old one is taken out of the directory, at
    /// once, and the new one is listed as one put on the network is, its page not shown. Held out,
    /// it stays so under the new name.
    pub(crate) fn listing_renamed(&mut self, id: u64, old: &str, new: &str) {
        let (from, to) = (old.to_lowercase(), new.to_lowercase());
        let held_out = self.directory.held_out.contains(&from);
        self.listing_off(old);
        let d = &mut self.directory;
        for (name, _) in &mut d.notices {
            if name.eq_ignore_ascii_case(old) {
                new.clone_into(name);
            }
        }
        if held_out {
            d.held_out.insert(to);
        } else if self.enabled() {
            self.directory
                .wanted
                .insert(to, (id, new.to_owned(), false));
        }
    }

    /// The link is welcomed: every listed character whose latest profile the hub has not answered
    /// is due again (what takes characters out goes first, as it always does).
    pub(crate) fn directory_welcomed(&mut self, t: f64) {
        let owed: Vec<String> = self
            .directory
            .entries
            .iter()
            .filter(|(_, e)| e.owed)
            .map(|(k, _)| k.clone())
            .collect();
        for k in owed {
            self.directory.due(&k, t, false);
        }
    }

    /// The link is lost: what was on it and not yet answered is owed again, on the next link.
    pub(crate) fn directory_down(&mut self) {
        let d = &mut self.directory;
        d.sent.clear();
        d.latest.clear();
        d.removing.clear();
        d.removals_wait.clear();
        d.pace = None;
    }

    /// The paces that have filled again are forgotten; what the hub has not answered in time is
    /// owed again.
    pub(crate) fn directory_settle(&mut self, t: f64, timeout: f64) {
        let d = &mut self.directory;
        d.paces.retain(|_, b| !b.full_at(t));
        let late: Vec<String> = d
            .sent
            .iter()
            .filter(|(_, s)| t - s.at >= timeout)
            .map(|(id, _)| id.clone())
            .collect();
        for id in late {
            let Some(sent) = d.sent.remove(&id) else {
                continue;
            };
            let Some(key) = sent.name else {
                d.removing.remove(&sent.key);
                continue;
            };
            if d.latest.get(&key).is_some_and(|l| *l == id) {
                d.latest.remove(&key);
                d.due(&key, t, sent.announce);
            }
        }
    }

    /// How many listings the link holds that the hub has yet to answer.
    #[must_use]
    pub fn listings_unanswered(&self) -> usize {
        self.directory.sent.len()
    }

    /// Whether the hub's pace for this world's listings allows one more now.
    pub(crate) fn listings_paced(&mut self, t: f64) -> bool {
        self.directory
            .pace
            .get_or_insert_with(|| Bucket::new(under(LISTINGS_PER_WORLD), t))
            .has(t)
    }

    /// The next listing for the link at `t`, if one may go: what takes a character out first,
    /// then the profiles due, each within its character's pace. It is taken from the world's pace
    /// for listings.
    pub(crate) fn next_listing(&mut self, t: f64) -> Option<String> {
        let frame = self.next_removal(t).or_else(|| self.next_profile(t))?;
        if let Some(p) = self.directory.pace.as_mut() {
            p.take(t);
        }
        Some(frame)
    }

    fn next_removal(&mut self, t: f64) -> Option<String> {
        let d = &self.directory;
        let (key, who) = d
            .removals
            .iter()
            .find(|(k, _)| {
                !d.removing.contains(*k) && d.removals_wait.get(*k).is_none_or(|&at| at <= t)
            })
            .map(|(k, a)| (k.clone(), a.clone()))?;
        let id = self.fresh_id('l');
        let d = &mut self.directory;
        d.removing.insert(key.clone());
        d.removals_wait.remove(&key);
        d.sent.insert(
            id.clone(),
            Sent {
                name: None,
                key,
                announce: false,
                at: t,
            },
        );
        Some(
            DirectoryMessage::Listing {
                id,
                who,
                on: false,
                profile: None,
            }
            .to_json(),
        )
    }

    fn next_profile(&mut self, t: f64) -> Option<String> {
        let rate = under(LISTINGS_PER_CHARACTER);
        let ready: Vec<String> = self
            .directory
            .entries
            .iter()
            .filter(|(_, e)| e.due.is_some_and(|at| at <= t))
            .map(|(k, _)| k.clone())
            .collect();
        for key in ready {
            let paced = self
                .directory
                .paces
                .entry(key.clone())
                .or_insert_with(|| Bucket::new(rate, t))
                .take(t);
            if !paced {
                continue;
            }
            let id = self.fresh_id('l');
            let d = &mut self.directory;
            let Some(e) = d.entries.get_mut(&key) else {
                continue;
            };
            e.due = None;
            e.owed = true;
            e.sent = Some(e.profile.clone());
            let announce = std::mem::take(&mut e.announce);
            let (who, profile) = (e.who.clone(), e.profile.clone());
            d.last.insert(key.clone(), t);
            d.latest.insert(key.clone(), id.clone());
            d.sent.insert(
                id.clone(),
                Sent {
                    name: Some(key),
                    key: who.key(),
                    announce,
                    at: t,
                },
            );
            return Some(
                DirectoryMessage::Listing {
                    id,
                    who,
                    on: true,
                    profile: Some(Box::new(profile.into())),
                }
                .to_json(),
            );
        }
        None
    }

    /// One of the directory's messages from the hub.
    pub(crate) fn on_directory(&mut self, friends: &impl Friends, t: f64, m: DirectoryMessage) {
        match m {
            DirectoryMessage::ListingResult {
                id,
                outcome,
                reason,
                url,
            } => self.on_listing_result(t, &id, outcome, reason, url),
            DirectoryMessage::ListingOff { who, by } => self.on_listing_off(friends, &who, by),
            DirectoryMessage::Listing { .. } => {}
        }
    }

    fn on_listing_result(
        &mut self,
        t: f64,
        id: &str,
        outcome: ListingOutcome,
        reason: Option<Refusal>,
        url: Option<String>,
    ) {
        let d = &mut self.directory;
        let Some(sent) = d.sent.remove(id) else {
            return;
        };
        let Some(key) = sent.name else {
            d.removing.remove(&sent.key);
            if outcome == ListingOutcome::RateLimited {
                d.removals_wait.insert(sent.key, t + AGAIN_SECS);
            } else if d.removals.remove(&sent.key).is_some() {
                d.dirty = true;
            }
            return;
        };
        if d.latest.get(&key).map(String::as_str) != Some(id) {
            // An earlier listing of the character: the latest one's answer is the one that counts.
            return;
        }
        d.latest.remove(&key);
        let Some(e) = d.entries.get_mut(&key) else {
            return;
        };
        let name = e.who.name().to_owned();
        match outcome {
            ListingOutcome::Ok => {
                e.owed = false;
                if url.is_some() {
                    e.page.clone_from(&url);
                }
                if let (true, Some(u)) = (sent.announce, url) {
                    d.tell(&name, &page_line(&u));
                }
            }
            ListingOutcome::Unavailable => {
                e.owed = false;
                if sent.announce {
                    d.tell(&name, NO_DIRECTORY);
                }
            }
            ListingOutcome::RateLimited => {
                e.announce |= sent.announce;
                d.due(&key, t + AGAIN_SECS, false);
            }
            ListingOutcome::Refused => {
                d.entries.remove(&key);
                d.held_out.insert(key);
                if sent.announce {
                    let line = if reason == Some(Refusal::Full) {
                        FULL
                    } else {
                        TAKEN_DOWN
                    };
                    d.tell(&name, line);
                }
            }
        }
    }

    fn on_listing_off(&mut self, friends: &impl Friends, who: &Address, by: OffBy) {
        if who.world() != self.settings.slug {
            return;
        }
        let key = who.name().to_lowercase();
        // The world keeps what comes of this, so it takes it only of a character it knows.
        if !self.on.contains_key(&key)
            && !self.directory.entries.contains_key(&key)
            && !self.directory.removals.contains_key(&who.key())
        {
            return;
        }
        let name = self
            .on_name(&key)
            .map_or_else(|| who.name().to_owned(), str::to_owned);
        let d = &mut self.directory;
        d.entries.remove(&key);
        d.wanted.remove(&key);
        d.latest.remove(&key);
        d.owe_removal(who.clone());
        match by {
            OffBy::Operator => {
                d.held_out.insert(key);
                d.tell(&name, TAKEN_DOWN);
            }
            OffBy::Person => {
                d.tell(&name, FORGOTTEN);
                if let Some(id) = self.on.get(&key).map(|o| o.id) {
                    self.directory.offs.push(id);
                    self.went_off(friends, &key);
                }
            }
        }
    }
}
