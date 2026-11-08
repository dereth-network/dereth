//! The allegiance: patrons and vassals, their profiles, swearing and breaking, ranks, and the
//! experience vassals earn for their patrons.
//!
//! **The profile** a player is sent is centred on one member: the allegiance's name, the member,
//! its patron and the monarch (both none for the monarch itself; no monarch apart when the patron
//! is the monarch), its vassals in the order they
//! swore with whether each is online, the message of the day, the faction, how many members the
//! allegiance has. A member is its experience passed up, species, level, sex, experience waiting
//! for it, id, rank, faction and name. A character in no allegiance is sent the profile with
//! nothing in it.
//!
//! **As a player enters**: a member hears its allegiance's chat room twice, then its own profile
//! (one in no allegiance, the empty profile alone); then what its vassals earned for it while it
//! was away, and a rank that fell while it was away.
//!
//! **Asked for a member's profile** (the allegiance page walking the tree), the profile centred on
//! that member is sent, online or not; asked for nothing (id 0: the page no longer follows a
//! member), nothing is sent.
//!
//! **Officers**: the monarch is an officer of its allegiance, and so is each member it appoints
//! ([`crate::allegiance_management`]); a member's data says whether it is one, and a profile carries
//! the officers. A member that leaves the allegiance stops being one of its officers and loses the
//! title it was given there.
//!
//! **What the allegiance keeps besides its members**: its message of the day (in every profile and
//! the tree's export), its appointed officers, its kingdom restriction, and each member's title.
//! An allegiance whose monarch swears to another leaves these behind.
//!
//! **A kingdom restriction** refuses an oath from one of another kingdom than the monarch's (a
//! neutral one is let in when the restriction allows neutrals): the would-be vassal is told which
//! kingdom it would have to join.
//!
//! **Swearing**: the would-be vassal is walked to the would-be patron; the patron is asked; when it
//! accepts, the patron is told who swore (and of its rank if it rose), the vassal hears the chat
//! room, is told to whom it swore and its rank, both see the vassal's monarch on the vassal, and
//! each gets its profile; every other member in the world is sent its profile again, with the
//! allegiance's new size. A patron declining tells the vassal so.
//!
//! **Breaking** with a patron: told whom it broke with, the profile as it now stands, the
//! monarch gone from the player's object.
//!
//! **Ranks**: a member with no vassals is of rank 1; one with vassals is of its highest vassal's
//! rank, one more when two or more vassals hold it, ten at most. A rank's title is by species,
//! sex and rank.
//!
//! **Experience passed up**: a quarter of what a vassal earns from kills goes into its patron's
//! pool; what a quest pays does not. A pool that reaches a fifth of the experience the patron's
//! next level takes (fifty at least, two million at most), or whatever it holds when a vassal
//! reaches a new level, is paid to the patron, up to a third and a half of that level's experience
//! for the level, and a tenth of the payment goes on into its own patron's pool. A patron in the
//! world is told which vassal earned it; one away is paid as it enters.
//!
//! **The tree's export** (the chat command that writes the allegiance to a file): the asker is
//! sent the whole allegiance, every member's node with its vassals and its patron, the tables of
//! members by name and by id, and the file's name; the client writes the file.

use std::collections::{BTreeMap, BTreeSet};

use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject, TextInfo};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::Value;
use crate::graph::{Builder, Slot};
use crate::social::{self, name_text, name_var, show, world_of, NOTICE, REFUSED};
use crate::{progress, quals, session, Clock, Server};

/// The allegiance's calls.
pub mod function {
    /// The client's: a member's profile asked for, an oath sworn to a patron, an allegiance
    /// broken, an oath accepted or declined by the patron.
    pub const QUERY: u32 = 0x8203_0005;
    pub const SWEAR: u32 = 0x8203_000A;
    pub const BREAK: u32 = 0x8203_0009;
    pub const ACCEPT_VASSAL: u32 = 0x8203_001C;
    pub const DECLINE_VASSAL: u32 = 0x8203_0029;
    /// The server's: a profile, an oath asked of a patron, the oath's end on each side, the
    /// allegiance's chat room.
    pub const PROFILE: u32 = 0x8203_002E;
    pub const REQUEST_VASSAL: u32 = 0x8203_002F;
    pub const PATRON_SWORE: u32 = 0x8203_002D;
    pub const VASSAL_SWORE: u32 = 0x8203_0033;
    pub const CHAT_ROOM: u32 = 0x8203_0030;
    /// The client's: the tree exported to a file (its name); the server's answer, the whole
    /// allegiance and the file's name.
    pub const TREE_EXPORT: u32 = 0x8203_004A;
    pub const EXPORT: u32 = 0x8203_006A;
}

/// Script packages: a profile (13 slots), a member's data (14), the whole allegiance (16), a
/// member's place in it (5).
pub const PROFILE: (u16, usize) = (0x1B5, 13);
pub const DATA: (u16, usize) = (0x1B1, 14);
pub const HIERARCHY: (u16, usize) = (0x1B2, 16);
pub const NODE: (u16, usize) = (0x1B3, 5);
/// Native types: a string reference, a list of references, a list of words, a set of 64-bit ids,
/// a table of names to references, a table of ids to references, a string.
const TEXT: u16 = 0x6A;
const REF_LIST: u16 = 0x65;
const WORD_LIST: u16 = 0xA;
const ID_SET: u16 = 0x1AC8;
const NAME_TABLE: u16 = 0x4D;
const ID_TABLE: u16 = 0x40;
const STRING: u16 = 0x76;
/// The bucket counts' places: the officers' set (47 buckets), the export's tables (1,531).
const OFFICERS_SIZE: u8 = 2;
const EXPORT_TABLES_SIZE: u8 = 7;

/// This server's world number, in the allegiance's chat room name.
pub const WORLD_NUMBER: u32 = 1;

/// The visual quality naming a member's monarch on its object.
pub const MONARCH: u32 = 452;

/// The faction of a member with none, and the profile's faction.
pub const NO_FACTION: u32 = 0x8000_0000;
pub const PROFILE_FACTION: u32 = 1;

/// The highest rank.
pub const TOP_RANK: u32 = 10;

/// Answers to an oath: done, declined by the patron, the patron has too many vassals, the vassal
/// has a patron already, an oath that would close a circle.
pub mod error {
    pub const DONE: u32 = 0;
    /// Of another kingdom than the allegiance's restriction lets in (neutrals not let in, let in).
    pub const KINGDOM: u32 = 5514;
    pub const KINGDOM_OR_NEUTRAL: u32 = 5521;
    pub const DECLINED: u32 = 5509;
    pub const TOO_MANY_VASSALS: u32 = 5506;
    pub const HAS_PATRON: u32 = 5507;
    pub const CIRCLE: u32 = 5508;
}

/// Texts: (string id, table).
pub mod text {
    pub const TABLE: u32 = 0x2500_04AB;
    pub const RANK_UP: (u32, u32) = (0xE271_5184, TABLE);
    pub const RANK_DOWN: (u32, u32) = (0x2271_5026, TABLE);
    pub const AWAY_EARNED: (u32, u32) = (0x59E9_6AF2, TABLE);
    pub const VASSAL_EARNED: (u32, u32) = (0x2198_159E, TABLE);
    pub const BROKEN: (u32, u32) = (0xFBEE_DF8F, TABLE);
    /// A member as "title name".
    pub const TITLED: (u32, u32) = (0x6780_74F5, TABLE);
    /// The rank titles' table.
    pub const TITLES: u32 = 0x2500_0631;
    /// "You cannot swear to (patron) unless you join the (kingdom)", and "... or the (neutral)".
    pub const JOIN_KINGDOM: (u32, u32) = (0xE9BE_3AD1, TABLE);
    pub const JOIN_KINGDOM_OR_NEUTRAL: (u32, u32) = (0x557B_DB59, TABLE);
    /// Variables: a kingdom, the neutral kingdom.
    pub const KINGDOM: u32 = 0xBEE1_1D49;
    pub const NEUTRAL: u32 = 0x2C6D_BBF4;
    /// Variables: a title, a name, an amount, a vassal, a patron.
    pub const TITLE: u32 = 0x0926_A4C8;
    pub const NAME: u32 = 0x9D06_6C50;
    pub const AMOUNT: u32 = 0xB140_0000;
    pub const VASSAL: u32 = 0x9E06_9D38;
    pub const PATRON: u32 = 0x5286_A529;
}

/// The rank titles: species, sex, rank, title (in the titles' table), as the client's own table holds
/// them.
pub const TITLES: [(u32, u32, u32, u32); 100] = [
    (1, 0x2000, 1, 0x63F72220),
    (1, 0x2000, 2, 0x03F72220),
    (1, 0x2000, 3, 0x23F72220),
    (1, 0x2000, 4, 0xC3F72220),
    (1, 0x2000, 5, 0xE3F72220),
    (1, 0x2000, 6, 0x83F72220),
    (1, 0x2000, 7, 0xA3F72220),
    (1, 0x2000, 8, 0x43F72221),
    (1, 0x2000, 9, 0x63F72221),
    (1, 0x2000, 10, 0xA44B961F),
    (1, 0x1000, 1, 0x6373FAD5),
    (1, 0x1000, 2, 0x0373FAD5),
    (1, 0x1000, 3, 0x2373FAD5),
    (1, 0x1000, 4, 0xC373FAD5),
    (1, 0x1000, 5, 0xE373FAD5),
    (1, 0x1000, 6, 0x8373FAD5),
    (1, 0x1000, 7, 0xA373FAD5),
    (1, 0x1000, 8, 0x4373FAD4),
    (1, 0x1000, 9, 0x6373FAD4),
    (1, 0x1000, 10, 0x4E4A9FAE),
    (2, 0x2000, 1, 0xAF822B3B),
    (2, 0x2000, 2, 0xCF822B3B),
    (2, 0x2000, 3, 0xEF822B3B),
    (2, 0x2000, 4, 0x0F822B3B),
    (2, 0x2000, 5, 0x2F822B3B),
    (2, 0x2000, 6, 0x4F822B3B),
    (2, 0x2000, 7, 0x6F822B3B),
    (2, 0x2000, 8, 0x8F822B3A),
    (2, 0x2000, 9, 0xAF822B3A),
    (2, 0x2000, 10, 0x93D37C0D),
    (2, 0x1000, 1, 0x4583228A),
    (2, 0x1000, 2, 0x2583228A),
    (2, 0x1000, 3, 0x0583228A),
    (2, 0x1000, 4, 0xE583228A),
    (2, 0x1000, 5, 0xC583228A),
    (2, 0x1000, 6, 0xA583228A),
    (2, 0x1000, 7, 0x8583228A),
    (2, 0x1000, 8, 0x6583228B),
    (2, 0x1000, 9, 0x4583228B),
    (2, 0x1000, 10, 0xF0077E1E),
    (4, 0x2000, 1, 0x80CE259D),
    (4, 0x2000, 2, 0xE0CE259D),
    (4, 0x2000, 3, 0xC0CE259D),
    (4, 0x2000, 4, 0x20CE259D),
    (4, 0x2000, 5, 0x00CE259D),
    (4, 0x2000, 6, 0x60CE259D),
    (4, 0x2000, 7, 0x40CE259D),
    (4, 0x2000, 8, 0xA0CE259C),
    (4, 0x2000, 9, 0x80CE259C),
    (4, 0x2000, 10, 0xDF8DE410),
    (4, 0x1000, 1, 0xE31A278E),
    (4, 0x1000, 2, 0x831A278E),
    (4, 0x1000, 3, 0xA31A278E),
    (4, 0x1000, 4, 0x431A278E),
    (4, 0x1000, 5, 0x631A278E),
    (4, 0x1000, 6, 0x031A278E),
    (4, 0x1000, 7, 0x231A278E),
    (4, 0x1000, 8, 0xC31A278F),
    (4, 0x1000, 9, 0xE31A278F),
    (4, 0x1000, 10, 0xF94A4C14),
    (16, 0x2000, 1, 0xD4F9ACDB),
    (16, 0x2000, 2, 0xB4F9ACDB),
    (16, 0x2000, 3, 0x94F9ACDB),
    (16, 0x2000, 4, 0x74F9ACDB),
    (16, 0x2000, 5, 0x54F9ACDB),
    (16, 0x2000, 6, 0x34F9ACDB),
    (16, 0x2000, 7, 0x14F9ACDB),
    (16, 0x2000, 8, 0xF4F9ACDA),
    (16, 0x2000, 9, 0xD4F9ACDA),
    (16, 0x2000, 10, 0x53258B02),
    (16, 0x1000, 1, 0xF23E04DF),
    (16, 0x1000, 2, 0x923E04DF),
    (16, 0x1000, 3, 0xB23E04DF),
    (16, 0x1000, 4, 0x523E04DF),
    (16, 0x1000, 5, 0x723E04DF),
    (16, 0x1000, 6, 0x123E04DF),
    (16, 0x1000, 7, 0x323E04DF),
    (16, 0x1000, 8, 0xD23E04DE),
    (16, 0x1000, 9, 0xF23E04DE),
    (16, 0x1000, 10, 0x5B680452),
    (8, 0x2000, 1, 0x9BB69C1F),
    (8, 0x2000, 2, 0xFBB69C1F),
    (8, 0x2000, 3, 0xDBB69C1F),
    (8, 0x2000, 4, 0x3BB69C1F),
    (8, 0x2000, 5, 0x1BB69C1F),
    (8, 0x2000, 6, 0x7BB69C1F),
    (8, 0x2000, 7, 0x5BB69C1F),
    (8, 0x2000, 8, 0xBBB69C1E),
    (8, 0x2000, 9, 0x9BB69C1E),
    (8, 0x2000, 10, 0xDBBB1563),
    (8, 0x1000, 1, 0x71B795AE),
    (8, 0x1000, 2, 0x11B795AE),
    (8, 0x1000, 3, 0x31B795AE),
    (8, 0x1000, 4, 0xD1B795AE),
    (8, 0x1000, 5, 0xF1B795AE),
    (8, 0x1000, 6, 0x91B795AE),
    (8, 0x1000, 7, 0xB1B795AE),
    (8, 0x1000, 8, 0x51B795AF),
    (8, 0x1000, 9, 0x71B795AF),
    (8, 0x1000, 10, 0xB86F1770),
];

/// The world's record of the allegiances.
pub const RECORD: &str = "allegiances";

/// The first chat room the allegiances' rooms are numbered from.
pub const FIRST_ROOM: u32 = 0x40;

/// A member of an allegiance (a monarch, or a vassal of a patron).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Node {
    /// Its patron (0: it is a monarch).
    pub patron: u64,
    /// When it swore (game time), which orders its patron's vassals.
    pub joined: f64,
    /// The experience it has passed up to its patron.
    pub passed: u64,
    /// The experience waiting in its pool to be paid to it.
    pub pooled: u64,
    /// What it has been paid toward its level's cap, and the level that was for.
    pub inherited: u64,
    pub inherited_level: i32,
    /// What it was paid while it was away, to be told as it enters.
    pub owed: u64,
    /// The rank it was last told of.
    pub rank_seen: u32,
}

/// Every allegiance of the world, kept as the world's record [`RECORD`].
#[derive(Debug, Clone, Default)]
pub struct Allegiances {
    /// Read from the store.
    pub loaded: bool,
    /// Every member, by character.
    pub nodes: BTreeMap<u64, Node>,
    /// The allegiances' names, by monarch.
    pub names: BTreeMap<u64, String>,
    /// Oaths offered and not yet answered: patron, vassal.
    pub offers: Vec<(u64, u64)>,
    /// Chat rooms given out, by monarch.
    pub rooms: BTreeMap<u64, u32>,
    /// What each allegiance keeps besides its members, by monarch.
    pub charters: BTreeMap<u64, Charter>,
    /// Members' titles, set by an officer of their allegiance.
    pub titles: BTreeMap<u64, String>,
}

/// What an allegiance keeps besides its members.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Charter {
    /// The message of the day, as the client sent it (its meta marks escaped).
    pub motd: Option<Vec<u16>>,
    /// The members the monarch appointed officers.
    pub officers: BTreeSet<u64>,
    /// Who may swear into it, when restricted.
    pub restriction: Option<Restriction>,
}

impl Charter {
    fn is_empty(&self) -> bool {
        self.motd.is_none() && self.officers.is_empty() && self.restriction.is_none()
    }
}

/// An allegiance's kingdom restriction: new members must be of the monarch's kingdom (or neutral,
/// when neutrals are let in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restriction {
    pub neutrals: bool,
}

/// The kingdom an oath would need, when a restriction refuses it: the allegiance's kingdom, and
/// whether the neutral one would do too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KingdomNeeded {
    pub kingdom: u32,
    pub or_neutral: bool,
}

impl Allegiances {
    /// The member's monarch (itself for a monarch); none for one in no allegiance.
    #[must_use]
    pub fn monarch(&self, id: u64) -> Option<u64> {
        let mut at = id;
        let mut steps = 0;
        while let Some(n) = self.nodes.get(&at) {
            if n.patron == 0 {
                return Some(at);
            }
            at = n.patron;
            steps += 1;
            if steps > 10_000 {
                return None;
            }
        }
        None
    }

    /// A member's vassals in the order they swore.
    #[must_use]
    pub fn vassals(&self, id: u64) -> Vec<u64> {
        let mut v: Vec<(f64, u64)> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.patron == id)
            .map(|(k, n)| (n.joined, *k))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        v.into_iter().map(|(_, k)| k).collect()
    }

    /// Is the character in an allegiance (sworn to a patron, or with vassals)?
    #[must_use]
    pub fn member(&self, id: u64) -> bool {
        self.nodes.contains_key(&id)
    }

    /// A member's rank (0 for one in no allegiance).
    #[must_use]
    pub fn rank(&self, id: u64) -> u32 {
        if !self.member(id) {
            return 0;
        }
        Self::rank_below(
            id,
            &self.children(),
            &mut BTreeSet::new(),
            &mut BTreeMap::new(),
        )
    }

    /// Every member's rank.
    #[must_use]
    pub fn ranks(&self) -> BTreeMap<u64, u32> {
        let children = self.children();
        let mut known = BTreeMap::new();
        for id in self.nodes.keys() {
            Self::rank_below(*id, &children, &mut BTreeSet::new(), &mut known);
        }
        known
    }

    /// Every member's vassals, in the order they swore, read once.
    fn children(&self) -> BTreeMap<u64, Vec<u64>> {
        let mut sworn: BTreeMap<u64, Vec<(f64, u64)>> = BTreeMap::new();
        for (k, n) in &self.nodes {
            sworn.entry(n.patron).or_default().push((n.joined, *k));
        }
        sworn
            .into_iter()
            .map(|(patron, mut v)| {
                v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                (patron, v.into_iter().map(|(_, k)| k).collect())
            })
            .collect()
    }

    /// A member's rank by its vassals' ranks, each counted once however a shape of oaths reaches
    /// them. Ranks already reckoned are taken as they are.
    fn rank_below(
        id: u64,
        children: &BTreeMap<u64, Vec<u64>>,
        seen: &mut BTreeSet<u64>,
        known: &mut BTreeMap<u64, u32>,
    ) -> u32 {
        if let Some(rank) = known.get(&id) {
            return *rank;
        }
        seen.insert(id);
        let mut ranks = Vec::new();
        for v in children.get(&id).into_iter().flatten() {
            if seen.insert(*v) {
                ranks.push(Self::rank_below(*v, children, seen, known));
            }
        }
        let rank = match ranks.iter().max() {
            None => 1,
            Some(&top) => {
                let held = ranks.iter().filter(|r| **r == top).count();
                (if held >= 2 { top + 1 } else { top }).clamp(1, TOP_RANK)
            }
        };
        known.insert(id, rank);
        rank
    }

    /// Every member under a member, itself first, each once however a stored allegiance links
    /// them.
    #[must_use]
    pub fn tree(&self, id: u64) -> Vec<u64> {
        let children = self.children();
        let mut out = vec![id];
        let mut seen = BTreeSet::from([id]);
        let mut i = 0;
        while i < out.len() {
            for v in children.get(&out[i]).into_iter().flatten() {
                if seen.insert(*v) {
                    out.push(*v);
                }
            }
            i += 1;
        }
        out
    }

    /// The officers of a monarch's allegiance: those appointed, with the monarch itself first
    /// when `with_monarch` (a profile names it; the allegiance's own record does not).
    #[must_use]
    pub fn officers(&self, monarch: u64, with_monarch: bool) -> Vec<u64> {
        let mut out = Vec::new();
        if with_monarch && self.nodes.contains_key(&monarch) {
            out.push(monarch);
        }
        if let Some(c) = self.charters.get(&monarch) {
            out.extend(
                c.officers
                    .iter()
                    .filter(|o| **o != monarch && self.monarch(**o) == Some(monarch)),
            );
        }
        out
    }

    /// Is the member an officer of its allegiance (its monarch, or one the monarch appointed)?
    #[must_use]
    pub fn is_officer(&self, id: u64) -> bool {
        match self.monarch(id) {
            Some(m) if m == id => true,
            Some(m) => self
                .charters
                .get(&m)
                .is_some_and(|c| c.officers.contains(&id)),
            None => false,
        }
    }

    /// The message of the day of a monarch's allegiance.
    #[must_use]
    pub fn motd(&self, monarch: u64) -> Option<&[u16]> {
        self.charters.get(&monarch).and_then(|c| c.motd.as_deref())
    }

    /// The charter of a monarch's allegiance, made when first needed.
    pub fn charter_mut(&mut self, monarch: u64) -> &mut Charter {
        self.charters.entry(monarch).or_default()
    }

    /// Drop a monarch's charter when it keeps nothing.
    pub fn tidy_charter(&mut self, monarch: u64) {
        if self.charters.get(&monarch).is_some_and(Charter::is_empty) {
            self.charters.remove(&monarch);
        }
    }

    /// Would a monarch's restriction refuse an oath from one of this kingdom (none: the neutral
    /// one), the monarch being of `monarch_kingdom`? What it would need, if so.
    #[must_use]
    pub fn kingdom_needed(
        &self,
        monarch: u64,
        monarch_kingdom: Option<u32>,
        kingdom: Option<u32>,
    ) -> Option<KingdomNeeded> {
        let r = self.charters.get(&monarch)?.restriction?;
        let neutral = crate::kingdoms::kingdom::NEUTRAL;
        let (theirs, ours) = (
            kingdom.unwrap_or(neutral),
            monarch_kingdom.unwrap_or(neutral),
        );
        if (r.neutrals && theirs == neutral) || theirs == ours {
            return None;
        }
        Some(KingdomNeeded {
            kingdom: ours,
            or_neutral: r.neutrals,
        })
    }

    /// The members of a monarch's allegiance.
    #[must_use]
    pub fn size(&self, monarch: u64) -> u32 {
        u32::try_from(self.tree(monarch).len()).unwrap_or(u32::MAX)
    }

    /// The record in the server's encoding.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![2u8];
        let n = u32::try_from(self.nodes.len()).unwrap_or(0);
        out.extend(n.to_le_bytes());
        for (id, x) in &self.nodes {
            out.extend(id.to_le_bytes());
            out.extend(x.patron.to_le_bytes());
            out.extend(x.joined.to_bits().to_le_bytes());
            out.extend(x.passed.to_le_bytes());
            out.extend(x.pooled.to_le_bytes());
            out.extend(x.inherited.to_le_bytes());
            out.extend(x.inherited_level.to_le_bytes());
            out.extend(x.owed.to_le_bytes());
            out.extend(x.rank_seen.to_le_bytes());
        }
        let n = u32::try_from(self.names.len()).unwrap_or(0);
        out.extend(n.to_le_bytes());
        for (id, name) in &self.names {
            out.extend(id.to_le_bytes());
            let b = name.as_bytes();
            out.extend(u32::try_from(b.len()).unwrap_or(0).to_le_bytes());
            out.extend(b);
        }
        let n = u32::try_from(self.charters.len()).unwrap_or(0);
        out.extend(n.to_le_bytes());
        for (id, c) in &self.charters {
            out.extend(id.to_le_bytes());
            match &c.motd {
                Some(m) => {
                    out.push(1);
                    out.extend(u32::try_from(m.len()).unwrap_or(0).to_le_bytes());
                    for u in m {
                        out.extend(u.to_le_bytes());
                    }
                }
                None => out.push(0),
            }
            out.extend(u32::try_from(c.officers.len()).unwrap_or(0).to_le_bytes());
            for o in &c.officers {
                out.extend(o.to_le_bytes());
            }
            out.push(match c.restriction {
                None => 0,
                Some(Restriction { neutrals: false }) => 1,
                Some(Restriction { neutrals: true }) => 2,
            });
        }
        let n = u32::try_from(self.titles.len()).unwrap_or(0);
        out.extend(n.to_le_bytes());
        for (id, t) in &self.titles {
            out.extend(id.to_le_bytes());
            let b = t.as_bytes();
            out.extend(u32::try_from(b.len()).unwrap_or(0).to_le_bytes());
            out.extend(b);
        }
        out
    }

    /// Read the record back; none when it does not read.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        struct In<'a>(&'a [u8]);
        impl In<'_> {
            fn take(&mut self, n: usize) -> Option<&[u8]> {
                let (a, rest) = self.0.split_at_checked(n)?;
                self.0 = rest;
                Some(a)
            }
            fn u32(&mut self) -> Option<u32> {
                Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
            }
            fn u64(&mut self) -> Option<u64> {
                Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
            }
            fn byte(&mut self) -> Option<u8> {
                Some(self.take(1)?[0])
            }
            fn text(&mut self) -> Option<String> {
                let n = self.u32()? as usize;
                String::from_utf8(self.take(n)?.to_vec()).ok()
            }
        }
        // Version 1 kept the members and the names alone.
        let (&version, rest) = b.split_first()?;
        if version != 1 && version != 2 {
            return None;
        }
        let mut r = In(rest);
        let mut a = Self::default();
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let node = Node {
                patron: r.u64()?,
                joined: f64::from_bits(r.u64()?),
                passed: r.u64()?,
                pooled: r.u64()?,
                inherited: r.u64()?,
                inherited_level: r.u32()?.cast_signed(),
                owed: r.u64()?,
                rank_seen: r.u32()?,
            };
            a.nodes.insert(id, node);
        }
        for _ in 0..r.u32()? {
            let id = r.u64()?;
            let name = r.text()?;
            a.names.insert(id, name);
        }
        if version >= 2 {
            for _ in 0..r.u32()? {
                let id = r.u64()?;
                let mut c = Charter::default();
                if r.byte()? == 1 {
                    let n = r.u32()? as usize;
                    let units = r.take(n.checked_mul(2)?)?;
                    c.motd = Some(
                        units
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|u| u16::from_le_bytes(*u))
                            .collect(),
                    );
                }
                for _ in 0..r.u32()? {
                    c.officers.insert(r.u64()?);
                }
                c.restriction = match r.byte()? {
                    0 => None,
                    1 => Some(Restriction { neutrals: false }),
                    2 => Some(Restriction { neutrals: true }),
                    _ => return None,
                };
                a.charters.insert(id, c);
            }
            for _ in 0..r.u32()? {
                let id = r.u64()?;
                let t = r.text()?;
                a.titles.insert(id, t);
            }
        }
        r.0.is_empty().then_some(a)
    }
}

/// Read the allegiances from the store, once. Whether they are read: while the store cannot answer,
/// nothing about them may be done or kept.
pub fn ensure<S: Store>(s: &mut Server<S>) -> bool {
    if s.allegiances.loaded {
        return true;
    }
    let Some(read) = crate::saves::read_record(s, RECORD, Allegiances::from_bytes) else {
        return false;
    };
    let offers = std::mem::take(&mut s.allegiances.offers);
    s.allegiances = read.unwrap_or_default();
    s.allegiances.offers = offers;
    s.allegiances.loaded = true;
    true
}

/// Keep the allegiances.
pub fn keep<S: Store>(s: &mut Server<S>) {
    if !s.allegiances.loaded {
        return;
    }
    let b = s.allegiances.to_bytes();
    s.saves.hold_record(RECORD, b);
}

/// What a profile says of a member: its name (as the world writes it), species, sex, level, and
/// whether it is in the world.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Facts {
    pub name: String,
    /// The name alone, without its grammar mark.
    pub plain: String,
    pub species: u32,
    pub sex: u32,
    pub level: u32,
    pub online: bool,
    /// Its kingdom (none for one that never joined one).
    pub kingdom: Option<u32>,
}

/// A character's facts: from the world when it is in it, else from the store.
pub fn facts<S: Store>(s: &mut Server<S>, id: u64) -> Facts {
    if let Some(p) = s.players.values().find(|p| p.character == id) {
        return Facts {
            name: crate::player::world_name(&p.name, p.sex),
            plain: p.name.clone(),
            species: p.race,
            sex: p.sex,
            level: crate::examine::level_of(&p.qualities)
                .max(1)
                .cast_unsigned(),
            // A cloaked admin is told to nobody as in the world.
            online: !p.admin.cloaked,
            kingdom: crate::kingdoms::of(&p.qualities),
        };
    }
    let Some(c) = s.store.character(id).ok().flatten() else {
        return Facts::default();
    };
    let state = crate::saves::state_of(s, id).ok().flatten();
    let level = state
        .as_ref()
        .and_then(|w| quals::int(&w.qualities, progress::q::LEVEL))
        .unwrap_or(1)
        .max(1)
        .cast_unsigned();
    let kingdom = state
        .as_ref()
        .and_then(|w| crate::kingdoms::of(&w.qualities));
    Facts {
        name: crate::player::world_name(&c.name, c.sex),
        plain: c.name.clone(),
        species: c.race,
        sex: c.sex,
        level,
        online: false,
        kingdom,
    }
}

/// What a member's examination says of its allegiance (none for one in no allegiance).
pub fn view<S: Store>(s: &mut Server<S>, id: u64) -> Option<crate::profile::AllegianceView> {
    let _ = ensure(s);
    let monarch = s.allegiances.monarch(id)?;
    let patron = s.allegiances.nodes.get(&id).map_or(0, |n| n.patron);
    let me = facts(s, id);
    let monarch_name = facts(s, monarch).name;
    let patron_name = (patron != 0 && patron != monarch).then(|| facts(s, patron).name);
    let rank = s.allegiances.rank(id);
    Some(crate::profile::AllegianceView {
        name: s
            .allegiances
            .names
            .get(&monarch)
            .filter(|n| !n.is_empty())
            .cloned(),
        monarch: monarch_name,
        is_monarch: monarch == id,
        patron: patron_name,
        rank,
        rank_title: title(me.species, me.sex, rank),
        title: s.allegiances.titles.get(&id).cloned(),
    })
}

/// A rank's title for a species and sex.
#[must_use]
pub fn title(species: u32, sex: u32, rank: u32) -> Option<TextInfo> {
    TITLES
        .iter()
        .find(|(sp, sx, r, _)| *sp == species && *sx == sex && *r == rank)
        .map(|t| progress::text((t.3, text::TITLES)))
}

/// A set of members' ids, as a profile's officers.
fn id_set(ids: &[u64]) -> NativeObject {
    let items: Vec<Item> = ids.iter().map(|i| Item::Long(*i)).collect();
    crate::graph::set(Elem::Long, OFFICERS_SIZE, &items)
}

/// The empty profile: a character in no allegiance.
#[must_use]
pub fn no_allegiance() -> ServerMessage {
    crate::chat::no_allegiance()
}

fn data(b: &mut Builder, s_: &Allegiances, id: u64, f: &Facts, rank: u32) -> u32 {
    let node = s_.nodes.get(&id).copied().unwrap_or_default();
    let officer = s_.is_officer(id);
    let me = b.reserve(DATA.0);
    let name = b.native(TEXT, NativeObject::TextInfo(name_text(&f.name)));
    b.fill(
        me,
        &[
            Slot::Long(node.passed),
            Slot::Int(f.species),
            Slot::Int(f.level),
            Slot::Ref(None),
            Slot::Int(f.sex),
            Slot::Long(node.pooled),
            Slot::Int(u32::from(officer)),
            Slot::Long(id),
            Slot::Int(rank),
            Slot::Int(NO_FACTION),
            Slot::Ref(Some(name)),
        ],
    );
    me
}

/// The profile centred on a member (the empty profile for one in no allegiance).
pub fn profile<S: Store>(s: &mut Server<S>, member: u64) -> ServerMessage {
    let _ = ensure(s);
    let Some(monarch) = s.allegiances.monarch(member) else {
        return no_allegiance();
    };
    let patron = s.allegiances.nodes.get(&member).map_or(0, |n| n.patron);
    let vassals = s.allegiances.vassals(member);
    let me = facts(s, member);
    let pf = (patron != 0).then(|| facts(s, patron));
    // The monarch is named apart from the member and its patron: not for itself, not when it is
    // the patron.
    let mf = (monarch != member && monarch != patron).then(|| facts(s, monarch));
    let vf: Vec<(u64, Facts)> = vassals.iter().map(|v| (*v, facts(s, *v))).collect();
    // An allegiance not named yet goes by its monarch's, as its members' examinations name it.
    let name_info = match s.allegiances.names.get(&monarch).filter(|n| !n.is_empty()) {
        Some(n) => name_text(n),
        None => crate::profile::unnamed_allegiance(&facts(s, monarch).name),
    };
    let all = &s.allegiances;
    let mut b = Builder::new();
    let root = b.reserve(PROFILE.0);
    let name = Some(b.native(TEXT, NativeObject::TextInfo(name_info)));
    let patron_ref = pf
        .as_ref()
        .map(|f| data(&mut b, all, patron, f, all.rank(patron)));
    let member_ref = data(&mut b, all, member, &me, all.rank(member));
    let monarch_ref = mf
        .as_ref()
        .map(|f| data(&mut b, all, monarch, f, all.rank(monarch)));
    let (list, online) = if vf.is_empty() {
        (None, None)
    } else {
        let refs: Vec<u32> = vf
            .iter()
            .map(|(v, f)| data(&mut b, all, *v, f, all.rank(*v)))
            .collect();
        let list = b.native(
            REF_LIST,
            NativeObject::Sequence {
                item: Elem::Ref,
                items: refs.into_iter().map(Item::Ref).collect(),
            },
        );
        let online = b.native(
            WORD_LIST,
            NativeObject::Sequence {
                item: Elem::Word,
                items: vf
                    .iter()
                    .map(|(_, f)| Item::Word(u32::from(f.online)))
                    .collect(),
            },
        );
        (Some(list), Some(online))
    };
    let total = all.size(monarch);
    let motd = motd_text(&mut b, all, monarch);
    let officers = b.native(ID_SET, id_set(&all.officers(monarch, true)));
    b.fill(
        root,
        &[
            Slot::Ref(name),
            Slot::Ref(patron_ref),
            Slot::Ref(Some(member_ref)),
            Slot::Ref(monarch_ref),
            Slot::Tagged(4, u32::from(me.online)),
            Slot::Tagged(4, u32::from(pf.as_ref().is_some_and(|f| f.online))),
            Slot::Ref(list),
            Slot::Ref(online),
            Slot::Ref(motd),
            Slot::Tagged(4, u32::from(mf.as_ref().is_some_and(|f| f.online))),
            Slot::Tagged(2, PROFILE_FACTION),
            Slot::Tagged(4, total),
            Slot::Ref(Some(officers)),
        ],
    );
    session::call(function::PROFILE, &[Arg::Object(b.graph())]).expect("a profile packs")
}

/// The message of the day of a monarch's allegiance, as a literal string reference, if it has one.
fn motd_text(b: &mut Builder, all: &Allegiances, monarch: u64) -> Option<u32> {
    let m = all.motd(monarch)?;
    Some(b.native(
        TEXT,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(m.to_vec()),
            ..TextInfo::default()
        }),
    ))
}

/// The chat room of a monarch's allegiance, given out the first time it is asked for.
pub fn room<S: Store>(s: &mut Server<S>, monarch: u64) -> u32 {
    let next = FIRST_ROOM + u32::try_from(s.allegiances.rooms.len()).unwrap_or(0);
    *s.allegiances.rooms.entry(monarch).or_insert(next)
}

/// The members in the world of the allegiance whose chat room this is.
pub fn room_members<S: Store>(s: &Server<S>, room: u32) -> Option<Vec<u64>> {
    let monarch = s
        .allegiances
        .rooms
        .iter()
        .find(|(_, r)| **r == room)
        .map(|(m, _)| *m)?;
    Some(s.allegiances.tree(monarch))
}

fn chat_room_message(room: u32) -> ServerMessage {
    session::call(function::CHAT_ROOM, &[Arg::Int(room)]).expect("a word packs")
}

/// What a player entering the world is told of its allegiance: its chat room twice and its
/// profile (the empty profile alone for one in none), then what its vassals earned for it while it
/// was away and a rank that fell meanwhile.
pub fn enter<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    if !ensure(s) {
        return;
    }
    let Some(id) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if let Some(monarch) = s.allegiances.monarch(id) {
        let r = room(s, monarch);
        for _ in 0..2 {
            progress::private(net, world, &chat_room_message(r));
        }
    }
    let m = profile(s, id);
    progress::private(net, world, &m);
}

/// After the entry's channels: the experience earned for the player while it was away, and a rank
/// that fell.
pub fn after_entry<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(id) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let rank = s.allegiances.rank(id);
    let Some(node) = s.allegiances.nodes.get(&id).copied() else {
        return;
    };
    if rank < node.rank_seen {
        tell_rank(s, net, world, rank, text::RANK_DOWN);
    }
    if let Some(n) = s.allegiances.nodes.get_mut(&id) {
        n.rank_seen = rank;
        n.owed = 0;
    }
    if node.owed > 0 || rank != node.rank_seen {
        keep(s);
    }
    if node.owed > 0 {
        show(
            net,
            world,
            NOTICE,
            progress::text_with(
                text::AWAY_EARNED,
                vec![progress::long(text::AMOUNT, 12, node.owed)],
            ),
        );
        progress::award(s, net, world, node.owed, progress::Award::Allegiance);
    }
}

fn tell_rank<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    rank: u32,
    which: (u32, u32),
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(t) = title(p.race, p.sex, rank) else {
        return;
    };
    show(
        net,
        world,
        REFUSED,
        progress::text_with(which, vec![progress::nested(text::TITLE, t)]),
    );
}

/// The member's monarch on its object, as everyone who knows it sees it.
fn monarch_on<S: Store>(s: &mut Server<S>, net: &mut ServerNet, member: u64, monarch: u64) {
    let Some(w) = world_of(s, member) else {
        return;
    };
    let Some(p) = s.players.get_mut(&w) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let m = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        quality: MONARCH,
        value: monarch,
    });
    crate::places::to_all_knowing(s, net, member, &m);
}

fn id_of(v: &[Value]) -> u64 {
    v.iter()
        .find_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .unwrap_or(0)
}

/// A member's profile asked for: centred on it, whether or not it is in the world; nothing for
/// id 0.
pub fn query<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let target = id_of(v);
    if target == 0 {
        return;
    }
    let m = profile(s, target);
    progress::private(net, world, &m);
}

/// The hash the client's tables file a name under: the classic ELF hash of its characters.
#[must_use]
pub fn name_hash(name: &str) -> u32 {
    let mut h: u32 = 0;
    for c in name.encode_utf16() {
        h = (h << 4).wrapping_add(u32::from(c));
        let g = h & 0xF000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}

fn string_of(v: &[Value]) -> Option<String> {
    v.iter().find_map(|x| match x.root() {
        Some(crate::calls::Node::Text(t)) => Some(t.clone()),
        _ => None,
    })
}

/// The whole allegiance of a monarch, as the tree's export sends it: every member's node (its
/// vassals in the order they swore, its patron, its data), the tables of the nodes by name and by
/// id, the chat room, its name, and the officers appointed.
fn hierarchy<S: Store>(s: &mut Server<S>, monarch: u64) -> Builder {
    let members = s.allegiances.tree(monarch);
    let facts: Vec<(u64, Facts)> = members.iter().map(|m| (*m, facts(s, *m))).collect();
    let room = room(s, monarch);
    let all = &s.allegiances;
    let mut b = Builder::new();
    let root = b.reserve(HIERARCHY.0);
    let nodes: BTreeMap<u64, u32> = members.iter().map(|m| (*m, b.reserve(NODE.0))).collect();
    let buckets = crate::graph::bucket_count(EXPORT_TABLES_SIZE);
    let (mut by_name, mut by_id) = (Vec::new(), Vec::new());
    let (ranks, children) = (all.ranks(), all.children());
    for (m, f) in &facts {
        let node = nodes[m];
        let d = data(&mut b, all, *m, f, ranks.get(m).copied().unwrap_or(0));
        let vassals: Vec<Item> = children
            .get(m)
            .into_iter()
            .flatten()
            .map(|v| Item::Ref(nodes[v]))
            .collect();
        let list = b.native(
            REF_LIST,
            NativeObject::Sequence {
                item: Elem::Ref,
                items: vassals,
            },
        );
        let patron = all
            .nodes
            .get(m)
            .map(|n| n.patron)
            .and_then(|p| nodes.get(&p).copied());
        b.fill(
            node,
            &[
                Slot::Ref(Some(list)),
                Slot::Ref(patron),
                Slot::Ref(Some(d)),
                Slot::Ref(None),
                Slot::Ref(None),
            ],
        );
        let key = b.native(STRING, NativeObject::Text(f.plain.encode_utf16().collect()));
        by_name.push((
            Item::HashedRef {
                id: key,
                hash: name_hash(&f.plain) % buckets,
                use_hash: 0,
            },
            Item::Ref(node),
        ));
        by_id.push((Item::Long(*m), Item::Ref(node)));
    }
    let name = b.native(
        TEXT,
        NativeObject::TextInfo(name_text(
            all.names.get(&monarch).map_or("", String::as_str),
        )),
    );
    let recall = b.native(STRING, NativeObject::Text(Vec::new()));
    let monarch_name = facts.first().map_or("", |(_, f)| f.plain.as_str());
    let room_name = format!("Allegiance of {monarch_name} from world {WORLD_NUMBER}");
    let room_name = b.native(
        STRING,
        NativeObject::Text(room_name.encode_utf16().collect()),
    );
    let names = b.native(
        NAME_TABLE,
        crate::graph::hash(Elem::HashedRef, Elem::Ref, EXPORT_TABLES_SIZE, &by_name),
    );
    let ids = b.native(
        ID_TABLE,
        crate::graph::hash(Elem::Long, Elem::Ref, EXPORT_TABLES_SIZE, &by_id),
    );
    let officers = b.native(ID_SET, id_set(&all.officers(monarch, false)));
    let motd = motd_text(&mut b, all, monarch);
    let restriction = all.charters.get(&monarch).and_then(|c| c.restriction);
    b.fill(
        root,
        &[
            Slot::Tagged(4, room),
            Slot::Ref(Some(name)),
            Slot::Ref(Some(recall)),
            Slot::Ref(nodes.get(&monarch).copied()),
            Slot::Int(0),
            Slot::Ref(Some(room_name)),
            Slot::Tagged(4, 1),
            Slot::Int(u32::from(restriction.is_some())),
            Slot::Ref(Some(names)),
            Slot::Ref(Some(ids)),
            Slot::Ref(motd),
            Slot::Int(1),
            Slot::Int(u32::from(restriction.is_some_and(|r| r.neutrals))),
            Slot::Int(PROFILE_FACTION),
            Slot::Int(0),
            Slot::Ref(Some(officers)),
        ],
    );
    b
}

/// The tree exported to a file: the asker is sent its whole allegiance and the file's name (one
/// in no allegiance is refused by its own client before it asks). Its rows come out of the
/// connection's allowance for pages, as it reads a store's row for each (2 asked for in all the
/// recordings).
pub fn export<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    now: yalain_net::LocalTime,
) {
    if !ensure(s) {
        return;
    }
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(file) = string_of(v) else {
        s.unhandled.note("a tree export without a file name");
        return;
    };
    let Some(monarch) = s.allegiances.monarch(me) else {
        s.unhandled.note("a tree export by one in no allegiance");
        return;
    };
    let members = s.allegiances.tree(monarch).len();
    if !crate::market::rows_allowed(s, world, members + 1, now) {
        return;
    }
    let hier = hierarchy(s, monarch);
    let mut f = Builder::new();
    f.native(STRING, NativeObject::Text(file.encode_utf16().collect()));
    let m = session::call(
        function::EXPORT,
        &[Arg::Object(f.graph()), Arg::Object(hier.graph())],
    )
    .expect("an export packs");
    progress::private(net, world, &m);
}

/// The most vassals a patron of a level may have.
#[must_use]
pub fn most_vassals(level: i32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (f64::from(level) * 0.5 + 0.5).floor().max(0.0) as usize;
    n.min(12)
}

fn swore_to(patron: u64, name: &str, err: u32) -> ServerMessage {
    session::call(
        function::VASSAL_SWORE,
        &[
            Arg::Int(err),
            crate::chat::text_arg(name_text(name)),
            Arg::LongInt(patron),
        ],
    )
    .expect("packs")
}

fn swore_by(vassal: u64, name: &str, err: u32) -> ServerMessage {
    session::call(
        function::PATRON_SWORE,
        &[
            Arg::Int(err),
            crate::chat::text_arg(name_text(name)),
            Arg::LongInt(vassal),
        ],
    )
    .expect("packs")
}

/// Why an oath to a patron cannot be: the vassal has a patron, the patron is under the vassal,
/// the patron has as many vassals as it may.
fn refusal<S: Store>(s: &mut Server<S>, vassal: u64, patron: u64) -> Option<u32> {
    let _ = ensure(s);
    if s.allegiances
        .nodes
        .get(&vassal)
        .is_some_and(|n| n.patron != 0)
    {
        return Some(error::HAS_PATRON);
    }
    if patron == vassal || s.allegiances.tree(vassal).contains(&patron) {
        return Some(error::CIRCLE);
    }
    let level = world_of(s, patron)
        .and_then(|w| s.players.get(&w))
        .map_or(1, |p| crate::examine::level_of(&p.qualities));
    if s.allegiances.vassals(patron).len() >= most_vassals(level) {
        return Some(error::TOO_MANY_VASSALS);
    }
    kingdom_needed(s, vassal, patron).map(|k| {
        if k.or_neutral {
            error::KINGDOM_OR_NEUTRAL
        } else {
            error::KINGDOM
        }
    })
}

/// The kingdom the vassal would have to join to swear to the patron, when the patron's
/// allegiance is restricted to its monarch's kingdom.
fn kingdom_needed<S: Store>(s: &mut Server<S>, vassal: u64, patron: u64) -> Option<KingdomNeeded> {
    let monarch = s.allegiances.monarch(patron)?;
    s.allegiances.charters.get(&monarch)?.restriction?;
    let ours = facts(s, monarch).kingdom;
    let theirs = facts(s, vassal).kingdom;
    s.allegiances.kingdom_needed(monarch, ours, theirs)
}

/// What a would-be vassal is told when a kingdom restriction refuses its oath.
fn tell_kingdom_needed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    vassal: u64,
    patron: u64,
) {
    let Some(k) = kingdom_needed(s, vassal, patron) else {
        return;
    };
    let pname = facts(s, patron).name;
    let kingdom = |f: u32| {
        progress::nested(
            text::KINGDOM,
            crate::profile::string(crate::profile::line::KINGDOMS, crate::profile::kingdom(f)),
        )
    };
    let neutral = crate::kingdoms::kingdom::NEUTRAL;
    let t = if k.or_neutral {
        progress::text_with(
            text::JOIN_KINGDOM_OR_NEUTRAL,
            vec![
                name_var(text::PATRON, &pname),
                kingdom(k.kingdom),
                progress::nested(
                    text::NEUTRAL,
                    crate::profile::string(
                        crate::profile::line::KINGDOMS,
                        crate::profile::kingdom(neutral),
                    ),
                ),
            ],
        )
    } else {
        progress::text_with(
            text::JOIN_KINGDOM,
            vec![name_var(text::PATRON, &pname), kingdom(k.kingdom)],
        )
    };
    show(net, world, REFUSED, t);
}

/// A player swears to another: walked to it, then the other is asked.
pub fn swear<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let target = id_of(v);
    let Some(other) = crate::presence::player_for(s, world, target) else {
        s.unhandled.note("an oath to someone not in the world");
        return;
    };
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if let Some(err) = refusal(s, me, target) {
        let name = social::name_of(s, other);
        progress::private(net, world, &swore_to(target, &name, err));
        if matches!(err, error::KINGDOM | error::KINGDOM_OR_NEUTRAL) {
            tell_kingdom_needed(s, net, world, me, target);
        }
        return;
    }
    social::approach(s, net, world, other, social::Approach::Swear, clock);
}

/// The would-be vassal has reached its patron: the patron is asked.
pub fn ask_patron<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, patron: ConnId) {
    let (Some(vassal), Some(p)) = (
        s.players.get(&world).map(|p| p.character),
        s.players.get(&patron).map(|p| p.character),
    ) else {
        return;
    };
    let name = social::name_of(s, world);
    s.allegiances.offers.retain(|(_, v)| *v != vassal);
    s.allegiances.offers.push((p, vassal));
    let m = session::call(
        function::REQUEST_VASSAL,
        &[
            crate::chat::text_arg(name_text(&name)),
            Arg::LongInt(vassal),
        ],
    )
    .expect("packs");
    progress::private(net, patron, &m);
}

/// The patron takes the oath offered.
pub fn accept<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    if !ensure(s) {
        return;
    }
    let vassal = id_of(v);
    let Some(patron) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(at) = s
        .allegiances
        .offers
        .iter()
        .position(|o| *o == (patron, vassal))
    else {
        s.unhandled.note("an oath accepted that was not offered");
        return;
    };
    s.allegiances.offers.remove(at);
    let Some(vw) = world_of(s, vassal) else {
        return;
    };
    let (pname, vname) = (social::name_of(s, world), social::name_of(s, vw));
    if let Some(err) = refusal(s, vassal, patron) {
        progress::private(net, world, &swore_by(vassal, &vname, err));
        progress::private(net, vw, &swore_to(patron, &pname, err));
        if matches!(err, error::KINGDOM | error::KINGDOM_OR_NEUTRAL) {
            tell_kingdom_needed(s, net, vw, vassal, patron);
        }
        return;
    }
    let before = s.allegiances.ranks();
    let new_monarch = !s.allegiances.member(patron);
    // A monarch that swears to another leaves its allegiance's charter behind.
    s.allegiances.charters.remove(&vassal);
    let now = net.game_time(clock.local);
    s.allegiances.nodes.entry(patron).or_default();
    s.allegiances.nodes.insert(
        vassal,
        Node {
            patron,
            joined: now,
            ..Node::default()
        },
    );
    let Some(monarch) = s.allegiances.monarch(vassal) else {
        return;
    };
    let room = room(s, monarch);
    // The patron's side, then the vassal's.
    progress::private(net, world, &swore_by(vassal, &vname, error::DONE));
    let prank = s.allegiances.rank(patron);
    if prank > before.get(&patron).copied().unwrap_or(0) {
        tell_rank(s, net, world, prank, text::RANK_UP);
    }
    if new_monarch {
        for _ in 0..2 {
            progress::private(net, world, &chat_room_message(room));
        }
    }
    progress::private(net, vw, &chat_room_message(room));
    progress::private(net, vw, &swore_to(patron, &pname, error::DONE));
    tell_rank(s, net, vw, s.allegiances.rank(vassal), text::RANK_UP);
    if new_monarch {
        monarch_on(s, net, patron, monarch);
    }
    monarch_on(s, net, vassal, monarch);
    let m = profile(s, patron);
    progress::private(net, world, &m);
    let m = profile(s, vassal);
    progress::private(net, vw, &m);
    // Every other member in the world is sent its profile again (the allegiance's new size).
    for id in s.allegiances.tree(monarch) {
        if id == patron || id == vassal {
            continue;
        }
        if let Some(w) = world_of(s, id) {
            let m = profile(s, id);
            progress::private(net, w, &m);
        }
    }
    // Others up the chain whose rank rose are told it.
    for (id, rank) in s.allegiances.ranks() {
        if id != patron && id != vassal && rank > before.get(&id).copied().unwrap_or(0) {
            if let Some(w) = world_of(s, id) {
                tell_rank(s, net, w, rank, text::RANK_UP);
            }
        }
        let seen = if id == vassal || id == patron || world_of(s, id).is_some() {
            rank
        } else {
            before.get(&id).copied().unwrap_or(rank)
        };
        if let Some(n) = s.allegiances.nodes.get_mut(&id) {
            n.rank_seen = seen;
        }
    }
    keep(s);
}

/// The patron declines the oath offered: the would-be vassal is told so.
pub fn decline<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let vassal = id_of(v);
    let Some(patron) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let before = s.allegiances.offers.len();
    s.allegiances.offers.retain(|o| *o != (patron, vassal));
    if s.allegiances.offers.len() == before {
        return;
    }
    if let Some(vw) = world_of(s, vassal) {
        let name = social::name_of(s, world);
        progress::private(net, vw, &swore_to(patron, &name, error::DECLINED));
    }
}

/// A player breaks with its patron (or with one of its vassals): the vassal's side is told whom
/// it broke with and gets its profile as it now stands, and its object no longer names the
/// monarch; the other side, in the world, gets its profile.
pub fn break_with<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    if !ensure(s) {
        return;
    }
    let target = id_of(v);
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let my_patron = s.allegiances.nodes.get(&me).map_or(0, |n| n.patron);
    let (vassal, patron) = if target == my_patron && target != 0 {
        (me, target)
    } else if s
        .allegiances
        .nodes
        .get(&target)
        .is_some_and(|n| n.patron == me)
    {
        (target, me)
    } else {
        s.unhandled
            .note("an allegiance broken with someone not patron nor vassal");
        return;
    };
    let pname = facts(s, patron).name;
    unlink(s, vassal);
    keep(s);
    if let Some(vw) = world_of(s, vassal) {
        if vassal == me {
            show(
                net,
                vw,
                NOTICE,
                progress::text_with(text::BROKEN, vec![name_var(text::PATRON, &pname)]),
            );
        }
        let m = profile(s, vassal);
        progress::private(net, vw, &m);
        let monarch = s.allegiances.monarch(vassal).unwrap_or(0);
        monarch_on(s, net, vassal, monarch);
    }
    if let Some(pw) = world_of(s, patron) {
        let m = profile(s, patron);
        progress::private(net, pw, &m);
        if !s.allegiances.member(patron) {
            monarch_on(s, net, patron, 0);
        }
    }
}

/// Take a vassal from its patron: it heads its own allegiance if it has vassals, else it leaves
/// the allegiances; a patron left with nothing leaves too.
fn unlink<S: Store>(s: &mut Server<S>, vassal: u64) {
    let patron = s.allegiances.nodes.get(&vassal).map_or(0, |n| n.patron);
    // Those leaving are no longer the old allegiance's officers, nor keep the titles it gave.
    if let Some(old) = s.allegiances.monarch(vassal) {
        let leaving = s.allegiances.tree(vassal);
        if let Some(c) = s.allegiances.charters.get_mut(&old) {
            for id in &leaving {
                c.officers.remove(id);
            }
        }
        for id in &leaving {
            s.allegiances.titles.remove(id);
        }
        s.allegiances.tidy_charter(old);
    }
    if s.allegiances.vassals(vassal).is_empty() {
        s.allegiances.nodes.remove(&vassal);
    } else if let Some(n) = s.allegiances.nodes.get_mut(&vassal) {
        n.patron = 0;
        n.passed = 0;
    }
    if patron != 0
        && s.allegiances.vassals(patron).is_empty()
        && s.allegiances
            .nodes
            .get(&patron)
            .is_some_and(|n| n.patron == 0)
    {
        s.allegiances.nodes.remove(&patron);
        s.allegiances.names.remove(&patron);
        s.allegiances.charters.remove(&patron);
        s.allegiances.titles.remove(&patron);
    }
}

/// The experience a level takes to the next (none at the top or without a level table).
fn next_level_xp<S: Store>(s: &Server<S>, level: i32) -> Option<u64> {
    let here = s.levels.level(level)?.xp;
    let next = s.levels.level(level + 1)?.xp;
    next.checked_sub(here)
}

/// A level and whether in the world, of a member.
fn level_of<S: Store>(s: &mut Server<S>, id: u64) -> i32 {
    facts(s, id).level.cast_signed()
}

/// A vassal has earned experience: its patron's pool takes a quarter, and a pool that reaches its
/// threshold is paid to the patron, a tenth of which goes on up.
pub fn earned<S: Store>(s: &mut Server<S>, net: &mut ServerNet, vassal: u64, amount: u64) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let share = (amount as f64 * 0.25) as u64;
    pass_up(s, net, vassal, share, false);
}

/// A vassal has reached a new level: what its patron's pool holds is paid at once, however little.
pub fn level_reached<S: Store>(s: &mut Server<S>, net: &mut ServerNet, vassal: u64) {
    pass_up(s, net, vassal, 0, true);
}

/// A share of a vassal's goes into its patron's pool, up the chain as each pool is paid; the first
/// pool is paid `at_once`, or like the rest when it reaches its threshold.
fn pass_up<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    vassal: u64,
    mut share: u64,
    at_once: bool,
) {
    if !ensure(s) {
        return;
    }
    let mut from = vassal;
    let mut steps = 0;
    while (share > 0 || (at_once && steps == 0)) && steps < 64 {
        steps += 1;
        let Some(patron) = s.allegiances.nodes.get(&from).map(|n| n.patron) else {
            break;
        };
        if patron == 0 {
            break;
        }
        if let Some(n) = s.allegiances.nodes.get_mut(&from) {
            n.passed += share;
        }
        let level = level_of(s, patron);
        let Some(next) = next_level_xp(s, level) else {
            break;
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let (threshold, cap) = (
            ((next as f64 * 0.2) as u64).clamp(50, 2_000_000),
            ((next as f64 * 0.35) as u64).clamp(20, 1_000_000_000_000),
        );
        let Some(p) = s.allegiances.nodes.get_mut(&patron) else {
            break;
        };
        p.pooled += share;
        if p.pooled < threshold && !(at_once && steps == 1) {
            break;
        }
        if p.inherited_level != level {
            p.inherited_level = level;
            p.inherited = 0;
        }
        let paid = p.pooled.min(cap.saturating_sub(p.inherited));
        p.pooled = 0;
        p.inherited += paid;
        if paid == 0 {
            break;
        }
        pay(s, net, patron, from, paid);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let on = (paid as f64 * 0.1) as u64;
        from = patron;
        share = on;
    }
    keep(s);
}

/// Pay a patron what a vassal earned for it: told so in the world, else kept for its entry.
fn pay<S: Store>(s: &mut Server<S>, net: &mut ServerNet, patron: u64, vassal: u64, paid: u64) {
    let Some(w) = world_of(s, patron) else {
        if let Some(n) = s.allegiances.nodes.get_mut(&patron) {
            n.owed += paid;
        }
        // Kept with the character, as an award it takes as it enters.
        return;
    };
    let f = facts(s, vassal);
    let rank = s.allegiances.rank(vassal);
    let titled = progress::text_with(
        text::TITLED,
        title(f.species, f.sex, rank)
            .map(|t| {
                vec![
                    progress::nested(text::TITLE, t),
                    name_var(text::NAME, &f.name),
                ]
            })
            .unwrap_or_else(|| vec![name_var(text::NAME, &f.name)]),
    );
    show(
        net,
        w,
        NOTICE,
        progress::text_with(
            text::VASSAL_EARNED,
            vec![
                progress::long(text::AMOUNT, 12, paid),
                progress::nested(text::VASSAL, titled),
            ],
        ),
    );
    progress::award(s, net, w, paid, progress::Award::Allegiance);
}
