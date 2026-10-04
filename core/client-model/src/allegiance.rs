//! The allegiance hierarchy and the rank-title tables.
//!
//! **The update protocol is not incremental on the client.** The allegiance-update handler throws
//! the old tree away and copies the new one; the update bit ([`allegiance_index::UPDATE`]) and
//! the per-node patron ids exist so the *server* can send a partial tree, but the client applies
//! whatever it gets as the complete picture. Anything not in the message disappears from the panel.

pub use dereth_rules::allegiance::swear_xp_cost_after_breaks;

use dereth_primitives::ObjectId;
use std::collections::BTreeMap;

/// Allegiance data bitfield.
pub mod allegiance_index {
    pub const UNDEF: u32 = 0;
    /// Rendered as a trailing `" *"`.
    pub const LOGGED_IN: u32 = 1;
    pub const UPDATE: u32 = 2;
    /// `time_online` + `allegiance_age` follow as two `i32`.
    pub const HAS_ALLEGIANCE_AGE: u32 = 4;
    /// `level` follows as a `u32`.
    pub const HAS_PACKED_LEVEL: u32 = 8;
    pub const MAY_PASSUP_EXPERIENCE: u32 = 16;
}

/// The allegiance data versions — what each version added.
pub mod version {
    pub const UNDEF: u32 = 0;
    pub const SPOKESPERSON_ADDED: u32 = 1;
    pub const POOLS_ADDED: u32 = 2;
    pub const MOTD_ADDED: u32 = 3;
    pub const CHAT_ROOM_ID_ADDED: u32 = 4;
    pub const BANNED_CHARACTERS_ADDED: u32 = 5;
    pub const MULTIPLE_ALLEGIANCE_OFFICERS_ADDED: u32 = 6;
    pub const BINDSTONES: u32 = 7;
    pub const ALLEGIANCE_NAME: u32 = 8;
    pub const OFFICERS_TITLES_ADDED: u32 = 9;
    pub const LOCKED_STATE: u32 = 10;
    pub const APPROVED_VASSAL: u32 = 11;
    pub const NEWEST: u32 = APPROVED_VASSAL;
}

/// One allegiance member as represented in the decoded hierarchy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceData {
    pub id: ObjectId,
    pub name: String,
    /// 1 = male, 2 = female.
    pub gender: u8,
    /// Heritage group, 1..=11.
    pub hg: u8,
    pub rank: u16,
    pub level: u32,
    pub bitfield: u32,
    pub cp_tithed: u32,
    pub cp_cached: u32,
    pub loyalty: u16,
    pub leadership: u16,
    pub time_online: i32,
    pub allegiance_age: i32,
}

impl AllegianceData {
    /// Whether this member is flagged as logged in.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        self.bitfield & allegiance_index::LOGGED_IN != 0
    }

    /// Behavior: `title + " " + name`, or just `name` when
    /// the title lookup fails.
    #[must_use]
    pub fn full_name(&self) -> String {
        match get_title(self.rank, self.hg, self.gender) {
            Some(t) => format!("{t} {}", self.name),
            None => self.name.clone(),
        }
    }

    /// One member as the hierarchy's unpack stores it. This is separate from the
    /// allegiance-update handler so that the allegiance-info response can build
    /// the **message's own**
    /// hierarchy without replacing the client's: `0x027C`'s handler walks the profile it was
    /// handed and never touches the client's own allegiance tree.
    ///
    /// The wire carries each narrow field as a dword and the client stores the narrow field;
    /// `AllegianceData`'s own layout is the authority for the width.
    #[must_use]
    pub fn from_wire(d: &dereth_protocol::social::AllegianceData) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        Self {
            id: d.id,
            name: d.name.clone(),
            gender: d.gender as u8,
            hg: d.heritage_group as u8,
            rank: d.rank as u16,
            level: d.level,
            bitfield: d.bitfield,
            cp_tithed: d.cp_tithed,
            cp_cached: d.cp_cached,
            loyalty: d.loyalty as u16,
            leadership: d.leadership as u16,
            time_online: d.time_online,
            allegiance_age: d.allegiance_age,
        }
    }

    /// Set or clear the may-pass-up-experience bit.
    pub fn set_may_passup_experience(&mut self, on: bool) {
        if on {
            self.bitfield |= allegiance_index::MAY_PASSUP_EXPERIENCE;
        } else {
            self.bitfield &= !allegiance_index::MAY_PASSUP_EXPERIENCE;
        }
    }
}

/// The heritage ids the title tables key on.
pub mod heritage {
    pub const ALUVIAN: u8 = 1;
    pub const GHARUNDIM: u8 = 2;
    pub const SHO: u8 = 3;
    pub const VIAMONTIAN: u8 = 4;
    /// Penumbraen.
    pub const SHADOWBOUND: u8 = 5;
    pub const GEARKNIGHT: u8 = 6;
    pub const TUMEROK: u8 = 7;
    pub const LUGIAN: u8 = 8;
    pub const EMPYREAN: u8 = 9;
    /// An alias for [`SHADOWBOUND`].
    pub const SHADOWBOUND_ALIAS: u8 = 10;
    pub const UNDEAD: u8 = 11;
}

const ALUVIAN_M: [&str; 10] = [
    "Yeoman",
    "Baronet",
    "Baron",
    "Reeve",
    "Thane",
    "Ealdor",
    "Duke",
    "Aetheling",
    "King",
    "High King",
];
const ALUVIAN_F: [&str; 10] = [
    "Yeoman",
    "Baronet",
    "Baroness",
    "Reeve",
    "Thane",
    "Ealdor",
    "Duchess",
    "Aetheling",
    "Queen",
    "High Queen",
];
const GHARUNDIM_M: [&str; 10] = [
    "Sayyid", "Shayk", "Maulan", "Mu'allim", "Naquib", "Qadi", "Mushir", "Amir", "Malik", "Sultan",
];
const GHARUNDIM_F: [&str; 10] = [
    "Sayyida",
    "Shayka",
    "Maulana",
    "Mu'allima",
    "Naquiba",
    "Qadiya",
    "Mushira",
    "Amira",
    "Malika",
    "Sultana",
];
const SHO_M: [&str; 10] = [
    "Jinin",
    "Jo-chueh",
    "Nan-chueh",
    "Shi-chueh",
    "Ta-chueh",
    "Kun-chueh",
    "Kou",
    "Taikou",
    "Ou",
    "Koutei",
];
const SHO_F: [&str; 10] = [
    "Jinin",
    "Jo-chueh",
    "Nan-chueh",
    "Shi-chueh",
    "Ta-chueh",
    "Kun-chueh",
    "Kou",
    "Taikou",
    "Jo-ou",
    "Koutei",
];
const VIAMONTIAN_M: [&str; 10] = [
    "Squire",
    "Banner",
    "Baron",
    "Viscount",
    "Count",
    "Marquis",
    "Duke",
    "Grand Duke",
    "King",
    "High King",
];
const VIAMONTIAN_F: [&str; 10] = [
    "Dame",
    "Banner",
    "Baroness",
    "Viscountess",
    "Countess",
    "Marquise",
    "Duchess",
    "Grand Duchess",
    "Queen",
    "High Queen",
];
const SHADOWBOUND_M: [&str; 10] = [
    "Tenebrous",
    "Shade",
    "Squire",
    "Knight",
    "Void Knight",
    "Void Lord",
    "Duke",
    "Archduke",
    "Highborn",
    "King",
];
const SHADOWBOUND_F: [&str; 10] = [
    "Tenebrous",
    "Shade",
    "Squire",
    "Knight",
    "Void Knight",
    "Void Lady",
    "Duchess",
    "Archduchess",
    "Highborn",
    "Queen",
];
const GEARKNIGHT_M: [&str; 10] = [
    "Tribunus",
    "Praefectus",
    "Optio",
    "Centurion",
    "Principes",
    "Legatus",
    "Consul",
    "Dux",
    "Secondus",
    "Primus",
];
const TUMEROK_M: [&str; 10] = [
    "Xutua", "Tuona", "Ona", "Nuona", "Turea", "Rea", "Nurea", "Kauh", "Sutah", "Tah",
];
const LUGIAN_M: [&str; 10] = [
    "Laigus", "Raigus", "Amploth", "Arintoth", "Obeloth", "Lithos", "Kantos", "Gigas", "Extas",
    "Tiatus",
];
const EMPYREAN_M: [&str; 10] = [
    "Ensign",
    "Corporal",
    "Lieutenant",
    "Commander",
    "Captain",
    "Commodore",
    "Admiral",
    "Warlord",
    "Ipharsin",
    "Aulin",
];
const EMPYREAN_F: [&str; 10] = [
    "Ensign",
    "Corporal",
    "Lieutenant",
    "Commander",
    "Captain",
    "Commodore",
    "Admiral",
    "Warlord",
    "Ipharsia",
    "Aulia",
];
const UNDEAD_M: [&str; 10] = [
    "Neophyte",
    "Acolyte",
    "Adept",
    "Esquire",
    "Squire",
    "Knight",
    "Count",
    "Viscount",
    "Highness",
    "Annointed",
];
const UNDEAD_F: [&str; 10] = [
    "Neophyte",
    "Acolyte",
    "Adept",
    "Esquire",
    "Squire",
    "Knight",
    "Countess",
    "Viscountess",
    "Highness",
    "Annointed",
];

/// The allegiance title lookup `(rank, heritage, gender)`.
///
/// Dispatches on gender (1 male, 2 female) then heritage. **Heritage 5 and 10 both map to
/// Shadowbound**, and heritages 6, 7 and 8 use the *male* table for both genders because no female
/// variants exist. Ranks are 1..=10; anything else fails and the caller falls back to the bare name.
#[must_use]
pub fn get_title(rank: u16, hg: u8, gender: u8) -> Option<&'static str> {
    if !matches!(gender, 1 | 2) {
        return None;
    }
    if !(1..=10).contains(&rank) {
        return None;
    }
    let female = gender == 2;
    let table: &[&str; 10] = match hg {
        heritage::ALUVIAN => {
            if female {
                &ALUVIAN_F
            } else {
                &ALUVIAN_M
            }
        }
        heritage::GHARUNDIM => {
            if female {
                &GHARUNDIM_F
            } else {
                &GHARUNDIM_M
            }
        }
        heritage::SHO => {
            if female {
                &SHO_F
            } else {
                &SHO_M
            }
        }
        heritage::VIAMONTIAN => {
            if female {
                &VIAMONTIAN_F
            } else {
                &VIAMONTIAN_M
            }
        }
        heritage::SHADOWBOUND | heritage::SHADOWBOUND_ALIAS => {
            if female {
                &SHADOWBOUND_F
            } else {
                &SHADOWBOUND_M
            }
        }
        // No female variants exist for these three.
        heritage::GEARKNIGHT => &GEARKNIGHT_M,
        heritage::TUMEROK => &TUMEROK_M,
        heritage::LUGIAN => &LUGIAN_M,
        heritage::EMPYREAN => {
            if female {
                &EMPYREAN_F
            } else {
                &EMPYREAN_M
            }
        }
        heritage::UNDEAD => {
            if female {
                &UNDEAD_F
            } else {
                &UNDEAD_M
            }
        }
        _ => return None,
    };
    Some(table[(rank - 1) as usize])
}

/// The original client's 268-byte allegiance-hierarchy layout is a first-child / next-sibling tree.
/// [`AllegianceHierarchy`] flattens it to each node's patron edge, exactly as the wire form provides.
#[derive(Debug, Clone, Default)]
pub struct AllegianceHierarchy {
    pub old_version: u32,
    pub monarch: Option<ObjectId>,
    /// the number of records the server sent, i.e. the size of
    /// the tree the client holds. **Not** what either "Followers" box shows: the shard sends the
    /// player's neighbourhood (monarch, patron, self, direct vassals), so this is a lower bound on
    /// the allegiance and the allegiance panel does not read it.
    pub total: u32,
    /// the dword before the hierarchy on the wire
    /// (the profile's unpack stores it). ACE writes
    /// `Monarch.TotalFollowers + 1`. The panel's monarch row formats **this minus one**
    /// into the monarch row's Followers box.
    ///
    /// Retail keeps the pair on the profile that wraps the hierarchy; this build has one tree per
    /// world and no profile object, so they live beside `total`, and a `0x0020` replaces all three.
    pub total_members: u32,
    /// the dword after it. ACE writes
    /// the player's own follower total -- the whole subtree, not the direct vassals.
    /// The panel's player row formats it unchanged
    /// into the player row's Followers box.
    pub total_vassals: u32,
    /// `id → its patron`. The monarch has none.
    pub patrons: BTreeMap<ObjectId, ObjectId>,
    /// `patron → its vassals`, in the order `Add` builds them: **head insertion**, so the newest
    /// vassal is first.
    pub vassals: BTreeMap<ObjectId, Vec<ObjectId>>,
    pub data: BTreeMap<ObjectId, AllegianceData>,
    pub monarch_broadcast_time: i32,
    pub monarch_broadcasts_today: u32,
    pub spokes_broadcast_time: i32,
    pub spokes_broadcasts_today: u32,
    pub motd: String,
    pub motd_set_by: String,
    pub chat_room_id: u32,
    pub allegiance_name: String,
    pub name_last_set_time: i32,
    /// `member → officer level`. The client never names which numeric level means speaker,
    /// seneschal, or castellan, so carry the number and let the server-supplied title strings label
    /// it.
    pub officers: BTreeMap<ObjectId, u32>,
    pub officer_titles: Vec<String>,
    pub is_locked: u32,
    pub approved_vassal: ObjectId,
}

/// The node count above which [`AllegianceHierarchy::add`] refuses another vassal: an add is
/// allowed while the tree holds 40,000 nodes or fewer.
pub const MAX_NODES_BEFORE_ADD: u32 = 40_000;

impl AllegianceHierarchy {
    /// Drop the whole tree.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Add allegiance data beneath `patron`.
    ///
    /// Inserts at the **head** of the patron's vassal list, creates the monarch when the tree is
    /// empty, rejects duplicate ids and increments `total`.
    ///
    /// A vassal is refused once the tree already holds more than [`MAX_NODES_BEFORE_ADD`] nodes,
    /// so the tree never grows past 40,001; the monarch is not subject to the cap.
    pub fn add(&mut self, patron: Option<ObjectId>, d: AllegianceData) -> bool {
        if self.data.contains_key(&d.id) {
            return false;
        }
        let id = d.id;
        match patron {
            None => {
                if self.monarch.is_some() {
                    return false;
                }
                self.monarch = Some(id);
            }
            Some(_) if self.total > MAX_NODES_BEFORE_ADD => return false,
            Some(p) => {
                self.patrons.insert(id, p);
                self.vassals.entry(p).or_default().insert(0, id);
            }
        }
        self.data.insert(id, d);
        self.total += 1;
        true
    }

    /// The lookup by id.
    #[must_use]
    pub fn look_up(&self, id: ObjectId) -> Option<&AllegianceData> {
        self.data.get(&id)
    }

    /// The patron of `id`.
    #[must_use]
    pub fn get_patron(&self, id: ObjectId) -> Option<&AllegianceData> {
        self.data.get(self.patrons.get(&id)?)
    }

    /// The first vassal of `id`.
    #[must_use]
    pub fn get_first_vassal(&self, id: ObjectId) -> Option<&AllegianceData> {
        self.data.get(self.vassals.get(&id)?.first()?)
    }

    /// The next vassal after `id` — the **peer** of `id`, not of its vassal.
    #[must_use]
    pub fn get_next_vassal(&self, id: ObjectId) -> Option<&AllegianceData> {
        let patron = self.patrons.get(&id)?;
        let sibs = self.vassals.get(patron)?;
        let i = sibs.iter().position(|x| *x == id)?;
        self.data.get(sibs.get(i + 1)?)
    }

    /// The monarch's id.
    #[must_use]
    pub fn get_monarch_id(&self) -> Option<ObjectId> {
        self.monarch
    }

    /// The version-gated blocks supported by this decoder, exposed for callers and tests.
    #[must_use]
    pub fn version_gates(v: u32) -> VersionGates {
        VersionGates {
            officers_hash: v >= version::MULTIPLE_ALLEGIANCE_OFFICERS_ADDED,
            // The pre-6 form carries a single spokesperson dword the client skips, but only when
            // the version is not zero at all.
            skip_spokesperson: v < version::MULTIPLE_ALLEGIANCE_OFFICERS_ADDED
                && v != version::UNDEF,
            officer_titles: v >= version::OFFICERS_TITLES_ADDED,
            pools: v >= version::POOLS_ADDED,
            motd: v >= version::MOTD_ADDED,
            chat_room_id: v >= version::CHAT_ROOM_ID_ADDED,
            banned_characters: v >= version::BANNED_CHARACTERS_ADDED,
            bind_point: v >= version::BINDSTONES,
            allegiance_name: v >= version::ALLEGIANCE_NAME,
            locked_state: v >= version::LOCKED_STATE,
            approved_vassal: v >= version::APPROVED_VASSAL,
        }
    }
}

/// Which optional blocks an `AllegianceHierarchy` payload carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionGates {
    pub officers_hash: bool,
    pub skip_spokesperson: bool,
    pub officer_titles: bool,
    pub pools: bool,
    pub motd: bool,
    pub chat_room_id: bool,
    pub banned_characters: bool,
    pub bind_point: bool,
    pub allegiance_name: bool,
    pub locked_state: bool,
    pub approved_vassal: bool,
}

/// The header's split: **the low 16 bits are the node count and the high 16 are the version**.
#[must_use]
pub fn split_hierarchy_header(header: u32) -> (u32, u32) {
    (header & 0xFFFF, header >> 16)
}

/// The client's chat block, at text type 0.
#[must_use]
pub fn allegiance_info_block(
    subject: &AllegianceData,
    patron: Option<&AllegianceData>,
    vassals: &[&AllegianceData],
) -> String {
    let star = |d: &AllegianceData| if d.is_logged_in() { " *" } else { "" };
    let mut s =
        String::from("Note: An asterisk (*) indicates that the character is currently online.\n");
    s.push_str(&format!(
        "Allegiance information for {}{}:\n",
        subject.full_name(),
        star(subject)
    ));
    if let Some(p) = patron {
        s.push_str(&format!("   Patron: {}{}\n", p.full_name(), star(p)));
    }
    s.push_str("   Vassals:\n");
    for v in vassals {
        s.push_str(&format!("      {}{}\n", v.full_name(), star(v)));
    }
    s
}

impl crate::world::World {
    /// The allegiance subscribe toggle — `0x001F`.
    ///
    /// This is the **only** producer of an allegiance roster. The shard answers the request with
    /// `0x0020 Allegiance_AllegianceUpdate` and then pushes further `0x0020` as the allegiance
    /// changes; a client that never asks receives only the empty pushes a login produces, which is
    /// all [`Self::handle_allegiance_update`] would ever see.
    ///
    /// The wire field is an `i32`, not a byte: a recorded fellowship session carries
    /// `1f000000 01000000` and `1f000000 00000000` and nothing else in 31 sends.
    pub fn allegiance_update_request(&mut self, req: &mut dyn crate::RequestSink, on: bool) {
        req.send(crate::Request::AllegianceUpdateRequest(
            dereth_protocol::social::AllegianceUpdateRequest {
                on_off: i32::from(on),
            },
        ));
    }

    /// The swear-allegiance send — `0x001D`.
    ///
    /// The body is one dword and nothing else; the recorded captures carry seven of them, each
    /// `1d000000 <objectId>` on queue 3.
    ///
    /// It is reached only from the allegiance panel's swear-confirmation dialog's confirmed arm,
    /// never from the button: the dialog latches the prospective patron when it opens and the
    /// send happens when it closes.
    pub fn swear_allegiance(&mut self, req: &mut dyn crate::RequestSink, target: ObjectId) {
        req.send(crate::Request::SwearAllegiance(
            dereth_protocol::social::AllegianceSwearAllegiance { target },
        ));
    }

    /// The break-allegiance send — `0x001E`.
    ///
    /// The **Kick** button's send as well as the Break button's: the panel's
    /// kick-confirmation close clears the kick context, sends a break for the latched vassal if
    /// the answer was yes, and then clears the latched vassal —
    /// so kicking a vassal and breaking from a patron are the same event with a different
    /// argument. `0x0277 Allegiance_BreakAllegianceBoot` is a different message (a name string
    /// and an account-boot flag) that no button on this panel raises.
    pub fn break_allegiance(&mut self, req: &mut dyn crate::RequestSink, target: ObjectId) {
        req.send(crate::Request::BreakAllegiance(
            dereth_protocol::social::AllegianceBreakAllegiance { target },
        ));
    }

    /// The allegiance panel's break-confirmation close, confirmed arm, whole: it looks up the
    /// player's patron in the profile and sends a break for that id.
    ///
    /// The patron is re-read **when the dialog closes**, not when it opens, so a roster that
    /// moved while the question was on screen breaks from whoever the patron is *now*. That
    /// asymmetry with the Kick dialog (which latches its target at open time) is transcribed
    /// rather than tidied. Returns the id it sent for, if any.
    pub fn break_allegiance_from_patron(
        &mut self,
        req: &mut dyn crate::RequestSink,
    ) -> Option<ObjectId> {
        let me = self.player?;
        let patron = self.allegiance.get_patron(me)?.id;
        self.break_allegiance(req, patron);
        Some(patron)
    }

    /// The client's allegiance-update handler — `0x0020`.
    ///
    /// It copies every field the client copies: the members, and also the pools, the MOTD, the
    /// chat room, the allegiance name, the officer titles, the lock and the approved vassal.
    ///
    /// The update is a **full replace**, not a patch: the module note at the top of this file, and
    /// the tree being cleared before the first `Add`.
    ///
    /// Returns the number of members the rebuilt tree holds.
    pub fn handle_allegiance_update(
        &mut self,
        p: &dereth_protocol::social::AllegianceProfile,
    ) -> u32 {
        self.allegiance_updates += 1;
        let h = &p.hierarchy;
        self.allegiance.clear();
        self.allegiance.old_version = h.version;
        // The two header dwords are what the two "Followers" boxes read; `total` is neither.
        self.allegiance.total_members = p.total_members;
        self.allegiance.total_vassals = p.total_vassals;
        for (patron, d) in &h.members {
            self.allegiance.add(*patron, AllegianceData::from_wire(d));
        }
        if let Some(o) = &h.officers {
            self.allegiance.officers = o.entries.iter().map(|(k, v)| (ObjectId(*k), *v)).collect();
        }
        if let Some(t) = &h.officer_titles {
            self.allegiance.officer_titles.clone_from(t);
        }
        if let Some(pools) = &h.pools {
            self.allegiance.monarch_broadcast_time = pools.monarch_broadcast_time;
            self.allegiance.monarch_broadcasts_today = pools.monarch_broadcasts_today;
            self.allegiance.spokes_broadcast_time = pools.spokes_broadcast_time;
            self.allegiance.spokes_broadcasts_today = pools.spokes_broadcasts_today;
        }
        if let Some((motd, by)) = &h.motd {
            self.allegiance.motd.clone_from(motd);
            self.allegiance.motd_set_by.clone_from(by);
        }
        if let Some(c) = h.chat_room_id {
            self.allegiance.chat_room_id = c;
        }
        if let Some((name, set_at)) = &h.allegiance_name {
            self.allegiance.allegiance_name.clone_from(name);
            self.allegiance.name_last_set_time = *set_at;
        }
        if let Some(l) = h.is_locked {
            #[allow(clippy::cast_sign_loss)] // the client stores the dword it was sent
            {
                self.allegiance.is_locked = l as u32;
            }
        }
        if let Some(v) = h.approved_vassal {
            self.allegiance.approved_vassal = ObjectId(v);
        }
        // The hierarchy unpack's last step before it succeeds: if there is a
        // monarch, clear the monarch's may-pass-up-experience flag.
        //
        // The monarch has nobody to pass experience up to, so the client clears the bit on every
        // unpack whatever the server sent. It lives in the hierarchy unpack rather than in the
        // allegiance-update handler, so a transcription of the handler alone misses it, and it is
        // the only place the bit is cleared anywhere in the client.
        if let Some(m) = self.allegiance.monarch {
            if let Some(d) = self.allegiance.data.get_mut(&m) {
                d.set_may_passup_experience(false);
            }
        }
        let ids = self.allegiance_roster_ids();
        let online = |id| {
            self.allegiance
                .look_up(id)
                .is_some_and(AllegianceData::is_logged_in)
        };
        let monarch = ids
            .monarch
            .filter(|id| Some(*id) != ids.subject)
            .is_some_and(online);
        let patron = ids.patron.is_some_and(online);
        let vassals = ids.vassals.into_iter().any(online);
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Monarch, monarch);
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Patron, patron);
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Vassals, vassals);
        self.allegiance.total
    }

    /// The client's allegiance-info response handler —
    /// **`0x027C`**, the `/allegiance info <name>` report.
    ///
    /// **This is a chat report, not a panel update**: `0x027C` touches no line of the allegiance
    /// panel at all. Every
    /// one of the handler's five writes goes to the chat scroll on chat type 0.
    ///
    /// ```text
    ///   query allegiance data for the target
    ///   not found -> return 0                 ; the target is not in this profile
    ///   L"Note: An asterisk (*) indicates that the character is currently online.\n"
    ///   call the scroll's add-text entry point (.., 0, true, 0)
    ///   read the member's full name and logged-in state
    ///   star = logged_in ? L" *" : L""
    ///   L"Allegiance information for %hs%s:\n"   (the wide formatted-string helper)
    ///   query the target's patron
    ///   L"   Patron: %hs%s\n"                    (sprintf)
    ///   query the target's first vassal
    ///   "   Vassals: \n"                         (the NARROW add-text overload)
    ///   L"      %hs%s\n"  per vassal, get_next_vassal(prev, &data)
    /// ```
    ///
    /// Three readings a plainer transcription loses:
    ///
    /// 1. **The walk is over the profile the message carried, not over the client's tree.**
    ///    The profile parameter is used throughout; the client's own
    ///    hierarchy is never read and never written. So a `/allegiance info` on somebody else's
    ///    allegiance cannot disturb the panel, and this method builds a scratch hierarchy.
    /// 2. **The data lookup failing is the whole guard** and it prints *nothing at all* — not even
    ///    the asterisk note, which is why that note comes after the lookup and not before it.
    /// 3. **`%hs` is a narrow-string conversion inside a wide format.** The names use
    ///    narrow strings inside the wide format; the vassal rows are
    ///    indented six spaces and the patron row three.
    ///
    /// Returns whether a report was printed.
    pub fn allegiance_info_response(
        &mut self,
        target: ObjectId,
        p: &dereth_protocol::social::AllegianceProfile,
    ) -> bool {
        // The profile as the message carried it. `add` is what the hierarchy unpack does per
        // member, so the patron/vassal links are built the same way `0x0020`'s
        // are; nothing else in the profile is read by this handler.
        let mut h = AllegianceHierarchy::default();
        for (patron, d) in &p.hierarchy.members {
            h.add(*patron, AllegianceData::from_wire(d));
        }
        let Some(target_data) = h.look_up(target) else {
            return false;
        };

        let star = |d: &AllegianceData| if d.is_logged_in() { " *" } else { "" };
        let mut lines = vec![
            "Note: An asterisk (*) indicates that the character is currently online.\n".to_owned(),
            format!(
                "Allegiance information for {}{}:\n",
                target_data.full_name(),
                star(target_data)
            ),
        ];
        if let Some(patron) = h.get_patron(target) {
            lines.push(format!(
                "   Patron: {}{}\n",
                patron.full_name(),
                star(patron)
            ));
        }
        if let Some(first) = h.get_first_vassal(target) {
            lines.push("   Vassals: \n".to_owned());
            let mut next = Some(first);
            while let Some(v) = next {
                lines.push(format!("      {}{}\n", v.full_name(), star(v)));
                next = h.get_next_vassal(v.id);
            }
        }
        for l in &lines {
            // A scroll print at window 0 on chat type **0**, `text_type::DEFAULT`, for
            // every one of the five call sites. Not `ALLEGIANCE` (18): the report is a command
            // answer, and the client puts it on the default channel.
            self.scroll
                .add_text_to_scroll(l, crate::chat::text_type::DEFAULT, true, 0);
        }
        true
    }

    /// The client's allegiance-update-aborted handler —
    /// **`0x0003`**.
    ///
    /// The handler passes its argument to the allegiance update-aborted notice and returns. The
    /// notice has exactly **one** receiver in retail, the allegiance panel, and that receiver
    /// checks whether the panel is visible and, if so, runs the panel's update.
    ///
    /// So it **ignores the `u32` it is handed** and runs the panel's update when the panel is
    /// up. The count is the panel's edge; the word is kept as a **measurement**, as `0x0226`'s
    /// is: a shard that ever sends a different one is then visible
    /// instead of silently taking the same arm.
    pub fn allegiance_update_aborted(&mut self, reason: u32) {
        self.allegiance_aborts += 1;
        self.allegiance_abort_last_reason = reason;
    }

    /// Return whether `id` is in the **player's**
    /// allegiance?
    ///
    /// Retail returns false when there is no other object or the subject has no monarch;
    /// otherwise it returns whether the two monarch ids are equal.
    ///
    /// This is **not** derived from the hierarchy: the client never consults the tree
    /// here. It compares two ids — which is the only thing it *could*
    /// do, since the radar plots strangers whose allegiance tree the client has never been sent.
    /// The monarch field is decoded with the object and mirrored by
    /// `weenie::mirror_stat_update`'s key 26.
    ///
    /// The guard is on **the subject's** monarch, not the player's: an object with no monarch is
    /// never a member, and two objects that both have none are not members of each other.
    #[must_use]
    pub fn is_allegiance_member(&self, id: ObjectId) -> bool {
        let Some(player) = self.player else {
            return false;
        };
        let Some(subject) = self.weenie(id) else {
            return false;
        };
        let Some(mine) = self.weenie(player) else {
            return false;
        };
        match subject.pwd.monarch {
            Some(m) if m.0 != 0 => Some(m) == mine.pwd.monarch,
            _ => false,
        }
    }

    /// The three walks the allegiance panel performs over the hierarchy, reduced to ids.
    ///
    /// This is the four walkers' production caller. The client's panel does exactly
    /// this and in this order:
    ///
    /// | client | here |
    /// |---|---|
    /// | the panel's monarch row → `get_monarch_id()` | [`Self::monarch`](AllegianceRosterIds::monarch) |
    /// | the panel's patron row → `get_patron(player)` | `patron` |
    /// | the panel's vassal rows → `get_first_vassal(player)` then `get_next_vassal` on the **previous vassal**, until it returns 0 | `vassals` |
    ///
    /// The vassal loop's `get_next_vassal` argument is the id just emitted, **not** the player: the
    /// function returns the *peer* of what it is given. Walking it with the player's id instead
    /// yields the player's own next sibling and is the mistake the asymmetry invites.
    ///
    /// The loop terminates because `get_next_vassal` advances strictly along one `Vec` by index; no
    /// bound is needed and the client has none either.
    #[must_use]
    pub fn allegiance_roster_ids(&self) -> AllegianceRosterIds {
        let subject = self.player;
        let mut r = AllegianceRosterIds {
            subject,
            monarch: self.allegiance.get_monarch_id(),
            ..AllegianceRosterIds::default()
        };
        let Some(me) = subject else { return r };
        r.patron = self.allegiance.get_patron(me).map(|d| d.id);
        let mut next = self.allegiance.get_first_vassal(me).map(|d| d.id);
        while let Some(id) = next {
            r.vassals.push(id);
            next = self.allegiance.get_next_vassal(id).map(|d| d.id);
        }
        r
    }
}

/// What returns: the monarch, the player's patron and the
/// player's direct vassals in list order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllegianceRosterIds {
    /// current player id, the id every walk is keyed on.
    pub subject: Option<ObjectId>,
    pub monarch: Option<ObjectId>,
    pub patron: Option<ObjectId>,
    /// Newest first — inserts at the head.
    pub vassals: Vec<ObjectId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The node cap: a vassal is still added while the tree holds 40,000 nodes and refused once
    /// it holds 40,001, so the tree stops at 40,001.
    #[test]
    fn the_hierarchy_refuses_a_vassal_once_it_holds_40001_nodes() {
        let node = |id: u32| AllegianceData {
            id: ObjectId(id),
            ..AllegianceData::default()
        };
        // A chain, each member the vassal of the one before, so no vassal list grows long.
        let mut h = AllegianceHierarchy::default();
        assert!(h.add(None, node(1)));
        for id in 2..=40_000u32 {
            assert!(h.add(Some(ObjectId(id - 1)), node(id)));
        }
        assert_eq!(h.total, 40_000);
        assert!(
            h.add(Some(ObjectId(40_000)), node(40_001)),
            "40,000 held: allowed"
        );
        assert_eq!(h.total, 40_001);
        assert!(
            !h.add(Some(ObjectId(1)), node(40_002)),
            "40,001 held: refused"
        );
        assert_eq!(h.total, 40_001);
        assert!(h.look_up(ObjectId(40_002)).is_none());
    }

    /// Oracle: the five recovered title tables, spot-checked at every heritage and at both ends of
    /// the rank range.
    #[test]
    fn the_rank_titles_match_the_documented_tables() {
        assert_eq!(get_title(1, heritage::ALUVIAN, 1), Some("Yeoman"));
        assert_eq!(get_title(10, heritage::ALUVIAN, 1), Some("High King"));
        assert_eq!(get_title(10, heritage::ALUVIAN, 2), Some("High Queen"));
        assert_eq!(get_title(3, heritage::ALUVIAN, 2), Some("Baroness"));
        assert_eq!(get_title(9, heritage::GHARUNDIM, 2), Some("Malika"));
        assert_eq!(get_title(9, heritage::SHO, 1), Some("Ou"));
        assert_eq!(
            get_title(9, heritage::SHO, 2),
            Some("Jo-ou"),
            "Sho rank 9 is the only Sho rank with a female variant"
        );
        assert_eq!(get_title(1, heritage::VIAMONTIAN, 2), Some("Dame"));
        assert_eq!(get_title(6, heritage::SHADOWBOUND, 2), Some("Void Lady"));
        assert_eq!(get_title(10, heritage::EMPYREAN, 2), Some("Aulia"));
        assert_eq!(get_title(7, heritage::UNDEAD, 2), Some("Countess"));
    }

    /// Oracle: §2's two exceptions — heritages 5 and 10 are the same table, and 6, 7 and 8 use the
    /// male table for both genders.
    #[test]
    fn the_two_heritage_exceptions_hold() {
        for rank in 1..=10u16 {
            assert_eq!(
                get_title(rank, heritage::SHADOWBOUND, 1),
                get_title(rank, heritage::SHADOWBOUND_ALIAS, 1),
                "rank {rank}"
            );
        }
        for hg in [heritage::GEARKNIGHT, heritage::TUMEROK, heritage::LUGIAN] {
            for rank in 1..=10u16 {
                assert_eq!(
                    get_title(rank, hg, 1),
                    get_title(rank, hg, 2),
                    "heritage {hg} rank {rank} has no female variant"
                );
            }
        }
    }

    /// An out of range rank or heritage falls back to the bare name.
    #[test]
    fn an_out_of_range_rank_or_heritage_falls_back_to_the_bare_name() {
        const MALE: u8 = 1;

        // Arm 3 -- the rank, checked inside the per-heritage helper as an unsigned `rank - 1 > 9`
        // on a dword, so rank 0 wraps to 0xFFFFFFFF and fails.
        for rank in [0u16, 11, 255, u16::MAX] {
            assert_eq!(
                get_title(rank, heritage::ALUVIAN, MALE),
                None,
                "rank {rank}"
            );
        }
        // ...and every rank inside the bound resolves, so the four above are a boundary and not
        // a blanket refusal that would pass with the table missing.
        for rank in 1..=10u16 {
            assert!(
                get_title(rank, heritage::ALUVIAN, MALE).is_some(),
                "rank {rank}"
            );
        }

        // Arm 2 -- the heritage, an unsigned `heritage - 1 > 10`: 1..=11 and no more.
        for hg in [0u8, 12, 255] {
            assert_eq!(get_title(1, hg, MALE), None, "heritage {hg}");
        }
        for hg in 1..=11u8 {
            assert!(get_title(1, hg, MALE).is_some(), "heritage {hg}");
        }

        let mut d = AllegianceData {
            name: "Lark".into(),
            gender: MALE,
            hg: heritage::ALUVIAN,
            ..Default::default()
        };
        assert_eq!(
            d.rank, 0,
            "the default rank is the out-of-range one this arm wants"
        );
        assert_eq!(
            d.full_name(),
            "Lark",
            "rank 0 has no title -- the fallback arm copies the name"
        );
        d.rank = 1;
        assert_eq!(
            d.full_name(),
            "Yeoman Lark",
            "a valid rank takes the title arm "
        );
        d.hg = 12;
        assert_eq!(
            d.full_name(),
            "Lark",
            "an out-of-range heritage falls back the same way"
        );
    }

    /// The title lookup accepts only gender 1 or 2. `full_name` copies the member name unchanged
    /// when that lookup fails.
    #[test]
    fn an_invalid_gender_falls_back_to_the_bare_name() {
        for gender in [0, 3, 255] {
            assert_eq!(
                get_title(9, heritage::ALUVIAN, gender),
                None,
                "gender {gender}"
            );
            let d = AllegianceData {
                name: "Bob".into(),
                gender,
                hg: heritage::ALUVIAN,
                rank: 9,
                ..Default::default()
            };
            assert_eq!(d.full_name(), "Bob", "gender {gender}");
        }
        assert_eq!(get_title(9, heritage::ALUVIAN, 1), Some("King"));
        assert_eq!(get_title(9, heritage::ALUVIAN, 2), Some("Queen"));
    }

    /// Oracle: §1 — the header packs the node count in the low half and the version in the high.
    #[test]
    fn the_hierarchy_header_splits_count_and_version() {
        assert_eq!(split_hierarchy_header(0x000B_0042), (0x42, 11));
        assert_eq!(split_hierarchy_header(0), (0, 0));
    }

    /// Oracle: the recovered version-gate table. Versions 5, 10, and 11 remain inferred; the gates
    /// are asserted so a corpus round trip will expose a wrong one.
    #[test]
    fn the_version_gates_open_in_the_documented_order() {
        let v0 = AllegianceHierarchy::version_gates(0);
        assert!(!v0.officers_hash);
        assert!(!v0.skip_spokesperson, "version 0 skips nothing at all");

        let v1 = AllegianceHierarchy::version_gates(1);
        assert!(
            v1.skip_spokesperson,
            "1..5 skip the old single spokesperson dword"
        );
        assert!(!v1.pools);

        let v6 = AllegianceHierarchy::version_gates(6);
        assert!(v6.officers_hash);
        assert!(!v6.skip_spokesperson);
        assert!(v6.pools && v6.motd && v6.chat_room_id && v6.banned_characters);
        assert!(!v6.bind_point && !v6.officer_titles);

        let v11 = AllegianceHierarchy::version_gates(11);
        assert!(v11.officers_hash);
        assert!(v11.officer_titles);
        assert!(v11.bind_point && v11.allegiance_name);
        assert!(v11.locked_state && v11.approved_vassal);
    }

    /// Oracle: §1's tree operations — `add` inserts at the **head** and `get_next_vassal` returns
    /// the peer of `id`, not of its vassal.
    #[test]
    fn the_tree_inserts_at_the_head_and_walks_peers() {
        let d = |id: u32| AllegianceData {
            id: ObjectId(id),
            ..Default::default()
        };
        let mut h = AllegianceHierarchy::default();
        assert!(h.add(None, d(1)));
        assert!(!h.add(None, d(2)), "a second monarch is rejected");
        assert!(h.add(Some(ObjectId(1)), d(10)));
        assert!(h.add(Some(ObjectId(1)), d(11)));
        assert!(
            !h.add(Some(ObjectId(1)), d(11)),
            "duplicate ids are rejected"
        );
        assert_eq!(h.total, 3);

        assert_eq!(h.get_monarch_id(), Some(ObjectId(1)));
        assert_eq!(
            h.get_first_vassal(ObjectId(1)).map(|d| d.id),
            Some(ObjectId(11)),
            "head insertion: the newest vassal is first"
        );
        assert_eq!(
            h.get_next_vassal(ObjectId(11)).map(|d| d.id),
            Some(ObjectId(10))
        );
        assert_eq!(h.get_next_vassal(ObjectId(10)), None);
        assert_eq!(h.get_patron(ObjectId(10)).map(|d| d.id), Some(ObjectId(1)));
        assert_eq!(h.get_patron(ObjectId(1)), None, "the monarch has no patron");

        h.clear();
        assert_eq!(h.total, 0);
        assert!(h.get_monarch_id().is_none());
    }

    /// Oracle: §3's `/allegiance info` block, printed verbatim in the document.
    #[test]
    fn the_allegiance_info_block_matches_the_documented_layout() {
        let mut me = AllegianceData {
            id: ObjectId(1),
            name: "Lark".into(),
            rank: 1,
            hg: heritage::ALUVIAN,
            gender: 1,
            bitfield: allegiance_index::LOGGED_IN,
            ..Default::default()
        };
        let patron = AllegianceData {
            name: "Bob".into(),
            ..Default::default()
        };
        let v = AllegianceData {
            name: "Cid".into(),
            ..Default::default()
        };
        let s = allegiance_info_block(&me, Some(&patron), &[&v]);
        assert_eq!(
            s,
            "Note: An asterisk (*) indicates that the character is currently online.\n\
             Allegiance information for Yeoman Lark *:\n\
             \x20  Patron: Bob\n\
             \x20  Vassals:\n\
             \x20     Cid\n"
        );
        me.bitfield = 0;
        assert!(!allegiance_info_block(&me, None, &[]).contains("Lark *"));
    }

    use crate::World;
    use dereth_protocol::social as wire;

    /// One synthesised member record. `bitfield` deliberately has **both** `LOGGED_IN` and
    /// `MAY_PASSUP_EXPERIENCE` set on every node, so the monarch's forced clear is visible.
    fn member(id: u32, name: &str, rank: u32) -> wire::AllegianceData {
        wire::AllegianceData {
            id: ObjectId(id),
            name: name.into(),
            gender: 1,
            heritage_group: u32::from(heritage::ALUVIAN),
            rank,
            level: 10 * rank,
            bitfield: wire::AllegianceData::LOGGED_IN
                | wire::AllegianceData::MAY_PASSUP_EXPERIENCE
                | wire::AllegianceData::HAS_PACKED_LEVEL
                | wire::AllegianceData::HAS_ALLEGIANCE_AGE,
            cp_tithed: 100 * id,
            cp_cached: 7 * id,
            loyalty: 200,
            leadership: 300,
            time_online: 60,
            allegiance_age: 90,
        }
    }

    fn version_eleven_blocks() -> wire::AllegianceHierarchy {
        wire::AllegianceHierarchy {
            version: wire::allegiance_version::APPROVED_VASSAL,
            officers: Some(dereth_protocol::archive::PHash::new(Vec::new())),
            officer_titles: Some(Vec::new()),
            pools: Some(wire::AllegiancePools::default()),
            motd: Some((String::new(), String::new())),
            chat_room_id: Some(0),
            bind_point: Some(dereth_protocol::types::PositionWire::default()),
            allegiance_name: Some((String::new(), 0)),
            is_locked: Some(0),
            approved_vassal: Some(0),
            old_officer: None,
            members: Vec::new(),
        }
    }

    /// The synthesised tree, as it travels: record 0 is the monarch with no patron prefix, every
    /// later record carries its patron's id.
    ///
    /// ```text
    /// 9  Bob   (monarch)
    /// └ 5  Cid          (the player's patron)
    ///   └ 1  Lark       (the player)
    ///     ├ 11 Eve      \ added second, so head insertion puts it first
    ///     └ 10 Dee      / added first
    ///        └ 20 Fay   (a vassal's vassal — the node get_next_vassal must NOT return)
    /// ```
    fn synthesised_profile() -> wire::AllegianceProfile {
        wire::AllegianceProfile {
            total_members: 6,
            total_vassals: 2,
            hierarchy: wire::AllegianceHierarchy {
                allegiance_name: Some(("The Hand of Dereth".into(), 1234)),
                members: vec![
                    (None, member(9, "Bob", 9)),
                    (Some(ObjectId(9)), member(5, "Cid", 5)),
                    (Some(ObjectId(5)), member(1, "Lark", 3)),
                    (Some(ObjectId(1)), member(10, "Dee", 1)),
                    (Some(ObjectId(1)), member(11, "Eve", 1)),
                    (Some(ObjectId(10)), member(20, "Fay", 1)),
                ],
                ..version_eleven_blocks()
            },
        }
    }

    /// Encode the profile, decode it again, and apply it — so what the tree is built from is a
    /// decoded message and not the struct the test typed.
    fn through_the_wire(p: &wire::AllegianceProfile) -> wire::AllegianceProfile {
        use dereth_protocol::Message as _;
        let m = wire::AllegianceUpdate {
            rank: 3,
            profile: p.clone(),
        };
        let mut w = dereth_protocol::archive::Writer::body();
        m.write(&mut w)
            .expect("the synthesised profile must encode");
        let bytes = w.into_inner();
        let mut r = dereth_protocol::archive::Reader::new(&bytes);
        let back = wire::AllegianceUpdate::read(&mut r).expect("and decode again");
        r.expect_exhausted().expect("with the cursor exhausted");
        assert_eq!(
            &back.profile, p,
            "the round trip must be lossless before anything is asserted"
        );
        back.profile
    }

    fn world_with(profile: &wire::AllegianceProfile) -> World {
        let mut w = World::new();
        w.set_player(ObjectId(1));
        let n = w.handle_allegiance_update(profile);
        assert_eq!(n, w.allegiance.total);
        w
    }

    /// Behaviour: allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel
    #[test]
    fn received_rosters_update_all_three_chat_focuses_without_a_panel() {
        use crate::chat::TalkFocus;
        let mut world = World::new();
        world.set_player(ObjectId(1));
        for bits in 0..8 {
            let mut profile = synthesised_profile();
            for (_, member) in &mut profile.hierarchy.members {
                let online = match member.id.0 {
                    9 => bits & 1 != 0,
                    5 => bits & 2 != 0,
                    10 => bits & 4 != 0,
                    11 => false,
                    _ => true,
                };
                if online {
                    member.bitfield |= wire::AllegianceData::LOGGED_IN;
                } else {
                    member.bitfield &= !wire::AllegianceData::LOGGED_IN;
                }
            }
            world.handle_allegiance_update(&through_the_wire(&profile));
            let notices = world.chat.take_talk_focus_notices();
            assert_eq!(
                notices
                    .iter()
                    .map(|n| (n.focus as u32, n.enabled))
                    .collect::<Vec<_>>(),
                [(5, bits & 1 != 0), (4, bits & 2 != 0), (6, bits & 4 != 0)]
            );
        }
        let mut world = World::new();
        assert!(world.set_player(ObjectId(9)));
        world.handle_allegiance_update(&through_the_wire(&synthesised_profile()));
        assert!(!world.chat.is_talk_focus_enabled(TalkFocus::Monarch));
        assert!(!world.chat.is_talk_focus_enabled(TalkFocus::Patron));
        assert!(world.chat.is_talk_focus_enabled(TalkFocus::Vassals));
        let mut empty = synthesised_profile();
        empty.hierarchy.members.clear();
        world.handle_allegiance_update(&through_the_wire(&empty));
        for focus in [TalkFocus::Monarch, TalkFocus::Patron, TalkFocus::Vassals] {
            assert!(!world.chat.is_talk_focus_enabled(focus));
        }
    }

    /// The roster walk reproduces the panels three walks.
    #[test]
    fn the_roster_walk_reproduces_the_panels_three_walks() {
        let p = through_the_wire(&synthesised_profile());
        let w = world_with(&p);
        let r = w.allegiance_roster_ids();

        assert_eq!(r.subject, Some(ObjectId(1)));
        assert_eq!(r.monarch, Some(ObjectId(9)), "the monarch lookup");
        assert_eq!(r.patron, Some(ObjectId(5)), "the player's patron");
        assert_eq!(
            r.vassals,
            vec![ObjectId(11), ObjectId(10)],
            "first vassal, then each next peer; head insertion puts the newest vassal first"
        );
        assert!(
            !r.vassals.contains(&ObjectId(20)),
            "20 is 10's vassal, not the player's: the next-vassal step returns the PEER of the id \
             it is given, and a walk that descended would find it"
        );

        // The asymmetry, stated directly: the peer of 10 is nothing, and 20 is reached only by
        // starting the walk again at 10.
        assert_eq!(w.allegiance.get_next_vassal(ObjectId(10)), None);
        assert_eq!(
            w.allegiance.get_first_vassal(ObjectId(10)).map(|d| d.id),
            Some(ObjectId(20))
        );
    }

    /// Every node the message carried is in the tree with the message's own values, and every
    /// patron edge is the one the record was prefixed with.
    ///
    /// This is the assertion that makes the walk above meaningful: a roster in the right order
    /// over the wrong data would pass the order test alone. SYNTHESISED oracle.
    #[test]
    fn the_tree_matches_the_messages_own_records() {
        let p = through_the_wire(&synthesised_profile());
        let w = world_with(&p);
        assert_eq!(w.allegiance.total, 6, "one node per record, of 6 records");
        assert_eq!(w.allegiance.allegiance_name, "The Hand of Dereth");
        assert_eq!(w.allegiance.old_version, 11);

        for (patron, d) in &p.hierarchy.members {
            let got = w
                .allegiance
                .look_up(d.id)
                .unwrap_or_else(|| panic!("{:?} is missing", d.id));
            assert_eq!(got.name, d.name);
            assert_eq!(u32::from(got.rank), d.rank);
            assert_eq!(got.level, d.level);
            assert_eq!(got.cp_cached, d.cp_cached);
            assert_eq!(got.cp_tithed, d.cp_tithed);
            assert_eq!(
                w.allegiance.get_patron(d.id).map(|x| x.id),
                *patron,
                "the patron edge of {:?} is the record's own prefix",
                d.id
            );
        }
        // The title lookup uses fields from the message-supplied record: rank 9, Aluvian, male.
        assert_eq!(
            w.allegiance.look_up(ObjectId(9)).unwrap().full_name(),
            "King Bob"
        );
        assert_eq!(
            w.allegiance.look_up(ObjectId(1)).unwrap().full_name(),
            "Baron Lark"
        );
    }

    /// Unpack clears may passup experience on the monarch only.
    #[test]
    fn unpack_clears_may_passup_experience_on_the_monarch_only() {
        let p = through_the_wire(&synthesised_profile());
        for (_, d) in &p.hierarchy.members {
            assert!(
                d.may_passup_experience(),
                "the wire sets the bit on {:?}",
                d.id
            );
        }
        let w = world_with(&p);
        let bit = allegiance_index::MAY_PASSUP_EXPERIENCE;
        assert_eq!(
            bit, 16,
            "the may-pass-up-experience bit is 0x10, as a literal"
        );
        assert_eq!(
            w.allegiance.look_up(ObjectId(9)).unwrap().bitfield & bit,
            0,
            "the monarch cannot pass experience up"
        );
        for id in [1u32, 5, 10, 11, 20] {
            assert_ne!(
                w.allegiance.look_up(ObjectId(id)).unwrap().bitfield & bit,
                0,
                "{id} is not the monarch and keeps the bit"
            );
        }
        // And the neighbouring bit is untouched: the clear is `&= ~0x10`, not a
        // wholesale clear, so `is_logged_in` still answers.
        assert!(w.allegiance.look_up(ObjectId(9)).unwrap().is_logged_in());
    }

    /// **The case the corpus can witness.** All twelve captured `0x0020` carry
    /// `total_members = 0` and no member records; this is what the walk must do with one.
    ///
    /// A build that shows nothing has to be distinguishable from a build that shows nothing
    /// *correctly*, so the assertions are on all four outputs and not on "it did not panic".
    #[test]
    fn an_empty_allegiance_walks_to_an_empty_roster() {
        let p = through_the_wire(&wire::AllegianceProfile {
            total_members: 0,
            total_vassals: 0,
            hierarchy: version_eleven_blocks(),
        });
        assert!(p.hierarchy.members.is_empty(), "this is the corpus's shape");
        let w = world_with(&p);
        assert_eq!(w.allegiance.total, 0);
        assert_eq!(w.allegiance.get_monarch_id(), None);
        let r = w.allegiance_roster_ids();
        assert_eq!(
            r.subject,
            Some(ObjectId(1)),
            "the walk still ran: the player is known"
        );
        assert_eq!(r.monarch, None);
        assert_eq!(r.patron, None);
        assert_eq!(r.vassals, Vec::<ObjectId>::new());
    }

    /// A second update **replaces** the tree rather than merging into it, so leaving an allegiance
    /// empties the roster instead of leaving the old one on screen.
    #[test]
    fn a_later_empty_update_replaces_the_populated_tree() {
        let mut w = world_with(&through_the_wire(&synthesised_profile()));
        assert_eq!(w.allegiance_roster_ids().vassals.len(), 2);
        let empty = wire::AllegianceProfile {
            hierarchy: version_eleven_blocks(),
            ..wire::AllegianceProfile::default()
        };
        assert_eq!(w.handle_allegiance_update(&empty), 0);
        let r = w.allegiance_roster_ids();
        assert_eq!((r.monarch, r.patron, r.vassals.len()), (None, None, 0));
        assert_eq!(
            w.allegiance.allegiance_name, "",
            "and the header goes with it"
        );
    }

    /// Allegiance membership compares the two monarch ids.
    #[test]
    fn allegiance_membership_compares_the_two_monarch_ids() {
        use dereth_protocol::types::PublicWeenieDesc;
        let mut w = World::new();
        w.set_player(ObjectId(1));
        let mut add = |id: u32, monarch: Option<u32>| {
            let mut o = crate::weenie::Weenie::new(ObjectId(id));
            o.pwd = PublicWeenieDesc {
                monarch: monarch.map(ObjectId),
                ..PublicWeenieDesc::default()
            };
            w.tables.weenies.insert(ObjectId(id), o);
        };
        add(1, Some(9)); // the player, in allegiance 9
        add(2, Some(9)); // an ally
        add(3, Some(8)); // someone else's allegiance
        add(4, None); // no allegiance at all
        add(5, Some(0)); // the explicit zero the guard tests

        assert!(w.is_allegiance_member(ObjectId(2)));
        assert!(!w.is_allegiance_member(ObjectId(3)));
        assert!(!w.is_allegiance_member(ObjectId(4)));
        assert!(
            !w.is_allegiance_member(ObjectId(5)),
            "_monarch == 0 is never a member"
        );
        assert!(
            w.is_allegiance_member(ObjectId(1)),
            "the player is in his own allegiance"
        );
        assert!(
            !w.is_allegiance_member(ObjectId(99)),
            "an unknown object is not a member"
        );

        // **The guard's own discriminator, added after a mutation survived.** Dropping
        // `m.0 != 0` changed nothing above, because every case there also differed in the *other*
        // id. The guard is load-bearing only when **both** sides are zero: two unaffiliated
        // objects must not be allies, and without the guard `Some(0) == Some(0)` says they are.
        w.weenie_mut(ObjectId(1)).unwrap().pwd.monarch = Some(ObjectId(0));
        assert!(
            !w.is_allegiance_member(ObjectId(5)),
            "two objects with _monarch == 0 are not in an allegiance together"
        );

        // With the *player* unaffiliated nobody matches, which is the corpus's own state.
        w.weenie_mut(ObjectId(1)).unwrap().pwd.monarch = None;
        assert!(!w.is_allegiance_member(ObjectId(2)));
    }
}
