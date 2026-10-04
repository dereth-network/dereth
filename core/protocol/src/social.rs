//! Family: social — `docs/networking/messages/09-social.md`. Friends, titles, allegiance,
//! fellowship and contracts.
//!
//! **Two community-catalogue errors this family exists to correct:**
//!
//! * `0x0021 Social_FriendsUpdate` carries a **list** of friends, not a single `FriendData`.
//! * `0x02C0 Fellowship_UpdateFellow` **begins with an object id** the catalogue omits entirely.
//!
//! And one the catalogue gets wrong more quietly: a contract tracker's two times are IEEE
//! **doubles**, not `long`s.

use crate::archive::{string_key_hash, PHash, PackedHash, Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::types::PositionWire;
use crate::Message;
use dereth_primitives::ObjectId;

// ---------------------------------------------------------------------------------------------
// Small shapes shared across the family
// ---------------------------------------------------------------------------------------------

macro_rules! string_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: String,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.pstring()? })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.pstring(&self.$field)
            }
        }
    };
}

macro_rules! scalar_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident, $ty:ty, $rd:ident, $wr:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: $ty,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.$rd()? })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.$wr(self.$field);
                Ok(())
            }
        }
    };
}

macro_rules! id_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: ObjectId,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: ObjectId(r.u32()?) })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.$field.0);
                Ok(())
            }
        }
    };
}

macro_rules! empty_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self)
            }
            fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
                Ok(())
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Friends
// ---------------------------------------------------------------------------------------------

string_message!(
    /// `0x0018 Social_AddFriend` (C2S) — you *add* by name.
    SocialAddFriend,
    SOCIAL_ADD_FRIEND,
    name
);
id_message!(
    /// `0x0017 Social_RemoveFriend` (C2S) — but you *remove* by object id.
    SocialRemoveFriend,
    SOCIAL_REMOVE_FRIEND,
    friend_id
);
empty_message!(
    /// `0x0025 Social_ClearFriends` (C2S).
    SocialClearFriends,
    SOCIAL_CLEAR_FRIENDS
);

/// One friend-list entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FriendData {
    pub id: ObjectId,
    pub online: i32,
    pub appear_offline: i32,
    pub name: String,
    pub friends_list: Vec<u32>,
    pub friend_of_list: Vec<u32>,
}

impl FriendData {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            online: r.i32()?,
            appear_offline: r.i32()?,
            name: r.pstring()?,
            friends_list: r.packed_list(Reader::u32)?,
            friend_of_list: r.packed_list(Reader::u32)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.i32(self.online);
        w.i32(self.appear_offline);
        w.pstring(&self.name)?;
        w.packed_list(&self.friends_list, |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        w.packed_list(&self.friend_of_list, |w, v| {
            w.u32(*v);
            Ok(())
        })
    }
}

/// `0x0021 Social_FriendsUpdate` (S2C).
///
/// **The community catalogue is wrong**: it lists a single `FriendData`. The client unpacks a
/// count-prefixed list of `FriendData` and *then* the update type
/// (the friends-update dispatch). See `docs/CORRECTIONS.md`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SocialFriendsUpdate {
    pub friends: Vec<FriendData>,
    /// The friends-update type: 0 full list, 1 added, 2 removed, 4 login change.
    pub update_type: u32,
}

impl Message for SocialFriendsUpdate {
    const OPCODE: Opcode = Opcode::SOCIAL_FRIENDS_UPDATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            friends: r.packed_list(FriendData::read)?,
            update_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.friends, |w, f| f.write(w))?;
        w.u32(self.update_type);
        Ok(())
    }
}

/// `0xF7CD Social_SendFriendsCommand` (C2S, **Control queue (2)**).
///
/// Not a game action: no `OrderedActionHeader`, no stamp. The client only ever sends `cmd = 0`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SocialSendFriendsCommand {
    pub cmd: u32,
    pub player: String,
}

impl Message for SocialSendFriendsCommand {
    const OPCODE: Opcode = Opcode::SOCIAL_SEND_FRIENDS_COMMAND;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            cmd: r.u32()?,
            player: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.cmd);
        w.pstring(&self.player)
    }
}

// ---------------------------------------------------------------------------------------------
// Titles
// ---------------------------------------------------------------------------------------------

/// `0x0029 Social_CharacterTitleTable` (S2C).
///
/// The title table reads a leading table-version dword
/// **before** the displayed title. Without it the list count is read out of the displayed title
/// and the read overruns the payload. The recorded corpus in `fixtures/packet-captures` confirms
/// it: the version is `1` in all three sessions while the display title is not. See
/// `docs/networking/messages/09-social.md` and
/// `docs/CORRECTIONS.md`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterTitlesMessage {
    /// The table version, the initial version 1 in every observed message.
    pub version: u32,
    pub display_title: u32,
    pub titles: Vec<u32>,
}

impl Message for CharacterTitlesMessage {
    const OPCODE: Opcode = Opcode::SOCIAL_CHARACTER_TITLE_TABLE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            version: r.u32()?,
            display_title: r.u32()?,
            titles: r.packed_list(Reader::u32)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.version);
        w.u32(self.display_title);
        w.packed_list(&self.titles, |w, v| {
            w.u32(*v);
            Ok(())
        })
    }
}

/// `0x002B Social_AddOrSetCharacterTitle` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SocialAddOrSetCharacterTitle {
    pub new_title: u32,
    pub set_as_display_title: i32,
}

impl Message for SocialAddOrSetCharacterTitle {
    const OPCODE: Opcode = Opcode::SOCIAL_ADD_OR_SET_CHARACTER_TITLE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            new_title: r.u32()?,
            set_as_display_title: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.new_title);
        w.i32(self.set_as_display_title);
        Ok(())
    }
}

scalar_message!(
    /// `0x002C Social_SetDisplayCharacterTitle` (C2S).
    SocialSetDisplayCharacterTitle,
    SOCIAL_SET_DISPLAY_CHARACTER_TITLE,
    title_id,
    u32,
    u32,
    u32
);

// ---------------------------------------------------------------------------------------------
// Allegiance
// ---------------------------------------------------------------------------------------------

/// One member of the allegiance hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceData {
    pub id: ObjectId,
    pub name: String,
    pub gender: u32,
    /// Heritage group.
    pub heritage_group: u32,
    pub rank: u32,
    pub level: u32,
    /// Online / officer / gagged / may-pass-up-XP flags.
    ///
    // only the may-pass-up-experience bit has a named setter in the client; the
    // other bits are decoded as raw flags and exposed as the whole word.
    pub bitfield: u32,
    pub cp_tithed: u32,
    pub cp_cached: u32,
    pub loyalty: u32,
    pub leadership: u32,
    pub time_online: i32,
    pub allegiance_age: i32,
}

impl AllegianceData {
    /// The may-pass-up-experience bit. Hierarchy unpacking forces it to 0 on the
    /// monarch after every unpack: the monarch can never tithe.
    ///
    /// **It is not `0x0001`, which is `LoggedIn`.**
    /// The pass-up-experience setter is `_bitfield |= 0x10` / `&= 0xFFFFFFEF`, while
    /// the logged-in test is `_bitfield & 1`. With `0x0001`
    /// [`Self::may_passup_experience`] answered "is this member online" — not a crash, a different
    /// game: every online vassal tithed and every offline one did not.
    pub const MAY_PASSUP_EXPERIENCE: u32 = 0x0000_0010;

    /// The logged-in bit, rendered as a trailing `" *"`.
    pub const LOGGED_IN: u32 = 0x0000_0001;

    /// The has-allegiance-age bit. **It decides the record's length**: with it,
    /// `_time_online` and `_allegiance_age` follow as two `i32`; without it a single `f64` does and
    /// `_allegiance_age` is zero.
    pub const HAS_ALLEGIANCE_AGE: u32 = 0x0000_0004;

    /// The has-packed-level bit. Also a length gate: with it `_level` follows as a `u32`,
    /// and without it the client sets [`Self::MAY_PASSUP_EXPERIENCE`] on the record itself.
    pub const HAS_PACKED_LEVEL: u32 = 0x0000_0008;

    /// The pass-up-experience bit.
    #[must_use]
    pub fn may_passup_experience(&self) -> bool {
        self.bitfield & Self::MAY_PASSUP_EXPERIENCE != 0
    }

    /// The logged-in bit.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        self.bitfield & Self::LOGGED_IN != 0
    }

    /// One allegiance entry — **the wire order, which is not the struct order.**
    ///
    /// **Reading it in struct order breaks the Allegiance panel.** The client's in-memory layout
    /// (`_id`, `_name`, `_gender`, …) is not the wire order below: read in struct order, every
    /// field lands at the wrong offset and five at the wrong width, so any
    /// `0x0020 Allegiance_AllegianceUpdate` carrying **one or more members** fails to decode and is
    /// dropped. The locked corpus cannot show this — all twelve of its `0x0020` carry
    /// `recordCount = 0`, so the record loop never runs — and a round trip of this file's writer
    /// against its reader cannot either; the check is 244 bytes recorded from a live shard.
    ///
    /// ```text
    /// id                 u32
    /// cp_cached          u32
    /// cp_tithed          u32
    /// bitfield           u32
    /// gender, heritage   u8, u8;  rank u16
    /// level              u32 if bitfield & 8, else absent and bitfield |= 0x10
    /// loyalty, leadership u16, u16
    /// time_online        i32 and allegiance_age i32 if bitfield & 4,
    ///                    else one f64 truncated to time_online and allegiance_age = 0
    /// name               narrow string
    /// ```
    ///
    /// Both gates change the record's **length**, not merely its meaning, so a reader that ignores
    /// them desynchronises every record after the first.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = ObjectId(r.u32()?);
        let cp_cached = r.u32()?;
        let cp_tithed = r.u32()?;
        let mut bitfield = r.u32()?;
        let gender = u32::from(r.u8()?);
        let heritage_group = u32::from(r.u8()?);
        let rank = u32::from(r.u16()?);
        // The legacy default is the one place the client *writes* this bitfield rather than
        // reading it: a record old enough to carry no packed level predates the tithe change.
        let level = if bitfield & Self::HAS_PACKED_LEVEL == 0 {
            bitfield |= Self::MAY_PASSUP_EXPERIENCE;
            0
        } else {
            r.u32()?
        };
        let loyalty = u32::from(r.u16()?);
        let leadership = u32::from(r.u16()?);
        let (time_online, allegiance_age) = if bitfield & Self::HAS_ALLEGIANCE_AGE == 0 {
            // Retail truncates the `double` the pre-`HasAllegianceAge` form carried, toward zero,
            // with the integer-indefinite value for one out of range.
            (dereth_primitives::num::to_i32_f64(r.f64()?), 0)
        } else {
            (r.i32()?, r.i32()?)
        };
        // **Last**, not second: narrow-string unpacking is the final call in the record unpack.
        let name = r.pstring()?;
        Ok(Self {
            id,
            name,
            gender,
            heritage_group,
            rank,
            level,
            bitfield,
            cp_tithed,
            cp_cached,
            loyalty,
            leadership,
            time_online,
            allegiance_age,
        })
    }

    /// The exact inverse of [`Self::read`] — and byte-for-byte what ACE's
    /// `AllegianceData.cs` emits, which is what `serv-game::allegiance::wire_member` needs in order
    /// to be believed by a retail client.
    ///
    /// The two gates are honoured on the way out too: a record whose `bitfield` lacks
    /// `HAS_PACKED_LEVEL` carries no level dword, and one lacking `HAS_ALLEGIANCE_AGE` carries the
    /// legacy `f64` in place of the two `i32`. Both are therefore **lossy** round trips, exactly as
    /// they are in retail, which is why `wire_member` sets both bits.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        fn narrow<T: TryFrom<u32>>(v: u32, field: &'static str) -> Result<T, MessageError> {
            T::try_from(v).map_err(|_| MessageError::Unencodable {
                field,
                reason: "the wire field is narrower than the client's stored word",
            })
        }
        w.u32(self.id.0);
        w.u32(self.cp_cached);
        w.u32(self.cp_tithed);
        w.u32(self.bitfield);
        w.u8(narrow::<u8>(self.gender, "allegiance gender")?);
        w.u8(narrow::<u8>(
            self.heritage_group,
            "allegiance heritage group",
        )?);
        w.u16(narrow::<u16>(self.rank, "allegiance rank")?);
        if self.bitfield & Self::HAS_PACKED_LEVEL != 0 {
            w.u32(self.level);
        }
        w.u16(narrow::<u16>(self.loyalty, "allegiance loyalty")?);
        w.u16(narrow::<u16>(self.leadership, "allegiance leadership")?);
        if self.bitfield & Self::HAS_ALLEGIANCE_AGE == 0 {
            w.f64(f64::from(self.time_online));
        } else {
            w.i32(self.time_online);
            w.i32(self.allegiance_age);
        }
        w.pstring(&self.name)?;
        Ok(())
    }
}

/// The allegiance version — the forward-compatibility discriminator in the top half of the
/// hierarchy's first dword.
pub mod allegiance_version {
    pub const SPOKESPERSON_ADDED: u32 = 1;
    pub const POOLS_ADDED: u32 = 2;
    pub const MOTD_ADDED: u32 = 3;
    pub const CHAT_ROOM_ID_ADDED: u32 = 4;
    /// Has **no field** in the client's unpack: the ban list never travels with the profile.
    pub const BANNED_CHARACTERS_ADDED: u32 = 5;
    pub const MULTIPLE_ALLEGIANCE_OFFICERS_ADDED: u32 = 6;
    pub const BINDSTONES: u32 = 7;
    pub const ALLEGIANCE_NAME: u32 = 8;
    pub const OFFICERS_TITLES_ADDED: u32 = 9;
    pub const LOCKED_STATE: u32 = 10;
    pub const APPROVED_VASSAL: u32 = 11;
    pub const NEWEST: u32 = APPROVED_VASSAL;
}

/// The allegiance hierarchy.
///
/// The first dword is **two 16-bit fields**: the low half is the record count, the high half the
/// allegiance version. Everything after that is gated on the version — this is the client's
/// forward-compatibility scheme for a structure that grew over thirteen years.
///
/// The tree is **not** serialised as a tree: record 0 is the monarch with no prefix, and every later
/// record is preceded by the object id of its patron.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllegianceHierarchy {
    pub version: u32,
    /// `version >= 6`.
    pub officers: Option<PHash<u32, u32>>,
    /// `1 <= version < 6`: the single old officer id, which the client **skips**.
    pub old_officer: Option<u32>,
    /// `version >= 9`.
    pub officer_titles: Option<Vec<String>>,
    /// `version >= 2`.
    pub pools: Option<AllegiancePools>,
    /// `version >= 3`.
    pub motd: Option<(String, String)>,
    /// `version >= 4`.
    pub chat_room_id: Option<u32>,
    /// `version >= 7` — 32 bytes on the wire.
    pub bind_point: Option<PositionWire>,
    /// `version >= 8`.
    pub allegiance_name: Option<(String, i32)>,
    /// `version >= 10`.
    pub is_locked: Option<i32>,
    /// `version >= 11`.
    pub approved_vassal: Option<u32>,
    /// Record 0 is the monarch and has no patron prefix; every later record carries one.
    pub members: Vec<(Option<ObjectId>, AllegianceData)>,
}

/// The `version >= 2` broadcast pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllegiancePools {
    pub monarch_broadcast_time: i32,
    pub monarch_broadcasts_today: u32,
    pub spokes_broadcast_time: i32,
    pub spokes_broadcasts_today: u32,
}

impl AllegianceHierarchy {
    #[allow(clippy::too_many_lines)] // one branch per version gate; splitting hides the order
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        use allegiance_version as v;
        let dw = r.u32()?;
        let count = dw & 0xFFFF;
        let version = dw >> 16;
        let mut h = Self {
            version,
            ..Self::default()
        };
        if version >= v::MULTIPLE_ALLEGIANCE_OFFICERS_ADDED {
            h.officers = Some(r.phash(|r| Ok((r.u32()?, r.u32()?)))?);
        } else if version >= v::SPOKESPERSON_ADDED {
            h.old_officer = Some(r.u32()?);
        }
        if version >= v::OFFICERS_TITLES_ADDED {
            h.officer_titles = Some(r.packed_list(Reader::pstring)?);
        }
        if version >= v::POOLS_ADDED {
            h.pools = Some(AllegiancePools {
                monarch_broadcast_time: r.i32()?,
                monarch_broadcasts_today: r.u32()?,
                spokes_broadcast_time: r.i32()?,
                spokes_broadcasts_today: r.u32()?,
            });
        }
        if version >= v::MOTD_ADDED {
            h.motd = Some((r.pstring()?, r.pstring()?));
        }
        if version >= v::CHAT_ROOM_ID_ADDED {
            h.chat_room_id = Some(r.u32()?);
        }
        if version >= v::BINDSTONES {
            h.bind_point = Some(PositionWire::read(r)?);
        }
        if version >= v::ALLEGIANCE_NAME {
            h.allegiance_name = Some((r.pstring()?, r.i32()?));
        }
        if version >= v::LOCKED_STATE {
            h.is_locked = Some(r.i32()?);
        }
        if version >= v::APPROVED_VASSAL {
            h.approved_vassal = Some(r.u32()?);
        }
        for i in 0..count {
            let patron = if i == 0 {
                None
            } else {
                Some(ObjectId(r.u32()?))
            };
            h.members.push((patron, AllegianceData::read(r)?));
        }
        Ok(h)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let count = u32::try_from(self.members.len()).map_err(|_| MessageError::Unencodable {
            field: "allegiance-hierarchy count",
            reason: "more than 4 G members",
        })?;
        if count > 0xFFFF || self.version > 0xFFFF {
            return Err(MessageError::Unencodable {
                field: "AllegianceHierarchy",
                reason: "the count and version share one dword, 16 bits each",
            });
        }
        w.u32((self.version << 16) | count);
        if let Some(o) = &self.officers {
            w.phash(o, |w, k, v| {
                w.u32(*k);
                w.u32(*v);
                Ok(())
            })?;
        } else if let Some(o) = self.old_officer {
            w.u32(o);
        }
        if let Some(t) = &self.officer_titles {
            w.packed_list(t, |w, s| w.pstring(s))?;
        }
        if let Some(p) = self.pools {
            w.i32(p.monarch_broadcast_time);
            w.u32(p.monarch_broadcasts_today);
            w.i32(p.spokes_broadcast_time);
            w.u32(p.spokes_broadcasts_today);
        }
        if let Some((motd, by)) = &self.motd {
            w.pstring(motd)?;
            w.pstring(by)?;
        }
        if let Some(c) = self.chat_room_id {
            w.u32(c);
        }
        if let Some(b) = &self.bind_point {
            b.write(w);
        }
        if let Some((name, t)) = &self.allegiance_name {
            w.pstring(name)?;
            w.i32(*t);
        }
        if let Some(l) = self.is_locked {
            w.i32(l);
        }
        if let Some(a) = self.approved_vassal {
            w.u32(a);
        }
        for (i, (patron, data)) in self.members.iter().enumerate() {
            match (i, patron) {
                (0, None) => {}
                (0, Some(_)) => {
                    return Err(MessageError::Unencodable {
                        field: "allegiance-hierarchy first member",
                        reason: "the monarch has no patron prefix",
                    })
                }
                (_, Some(p)) => w.u32(p.0),
                (_, None) => {
                    return Err(MessageError::Unencodable {
                        field: "allegiance-hierarchy members",
                        reason: "every record after the first carries its patron id",
                    })
                }
            }
            data.write(w)?;
        }
        Ok(())
    }
}

/// The allegiance profile.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllegianceProfile {
    pub total_members: u32,
    pub total_vassals: u32,
    pub hierarchy: AllegianceHierarchy,
}

impl AllegianceProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            total_members: r.u32()?,
            total_vassals: r.u32()?,
            hierarchy: AllegianceHierarchy::read(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.total_members);
        w.u32(self.total_vassals);
        self.hierarchy.write(w)
    }
}

/// `0x0020 Allegiance_AllegianceUpdate` (S2C).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllegianceUpdate {
    pub rank: u32,
    pub profile: AllegianceProfile,
}

impl Message for AllegianceUpdate {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            rank: r.u32()?,
            profile: AllegianceProfile::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.rank);
        self.profile.write(w)
    }
}

scalar_message!(
    /// `0x0003 Allegiance_AllegianceUpdateAborted` (S2C).
    AllegianceUpdateAborted,
    ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED,
    failure_type,
    u32,
    u32,
    u32
);

scalar_message!(
    /// `0x01C8 Allegiance_AllegianceUpdateDone` (S2C). The retail client drops it. The retail server
    /// sent it to allegiance members on a steady beat of about five seconds, not after each
    /// allegiance update (the retail captures).
    AllegianceUpdateDone,
    ALLEGIANCE_ALLEGIANCE_UPDATE_DONE,
    failure_type,
    u32,
    u32,
    u32
);

/// `0x027A Allegiance_AllegianceLoginNotificationEvent` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AllegianceLoginNotification {
    pub member: ObjectId,
    pub now_logged_in: i32,
}

impl Message for AllegianceLoginNotification {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_ALLEGIANCE_LOGIN_NOTIFICATION_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            member: ObjectId(r.u32()?),
            now_logged_in: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.member.0);
        w.i32(self.now_logged_in);
        Ok(())
    }
}

/// `0x027C Allegiance_AllegianceInfoResponseEvent` (S2C). Unlike `0x0020` this profile is **not**
/// cached; the client formats one member's report and prints it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AllegianceInfoResponse {
    pub target: ObjectId,
    pub profile: AllegianceProfile,
}

impl Message for AllegianceInfoResponse {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target: ObjectId(r.u32()?),
            profile: AllegianceProfile::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.target.0);
        self.profile.write(w)
    }
}

// The allegiance game actions.
id_message!(
    /// `0x001D Allegiance_SwearAllegiance`.
    AllegianceSwearAllegiance,
    ALLEGIANCE_SWEAR_ALLEGIANCE,
    target
);
id_message!(
    /// `0x001E Allegiance_BreakAllegiance`.
    AllegianceBreakAllegiance,
    ALLEGIANCE_BREAK_ALLEGIANCE,
    target
);
scalar_message!(
    /// `0x001F Allegiance_UpdateRequest` — subscribe/unsubscribe.
    AllegianceUpdateRequest,
    ALLEGIANCE_UPDATE_REQUEST,
    on_off,
    i32,
    i32,
    i32
);
empty_message!(
    /// `0x0030 Allegiance_QueryAllegianceName`.
    AllegianceQueryAllegianceName,
    ALLEGIANCE_QUERY_ALLEGIANCE_NAME
);
empty_message!(
    /// `0x0031 Allegiance_ClearAllegianceName`.
    AllegianceClearAllegianceName,
    ALLEGIANCE_CLEAR_ALLEGIANCE_NAME
);
string_message!(
    /// `0x0033 Allegiance_SetAllegianceName`.
    AllegianceSetAllegianceName,
    ALLEGIANCE_SET_ALLEGIANCE_NAME,
    name
);
empty_message!(
    /// `0x003D Allegiance_ListAllegianceOfficerTitles`.
    AllegianceListAllegianceOfficerTitles,
    ALLEGIANCE_LIST_ALLEGIANCE_OFFICER_TITLES
);
empty_message!(
    /// `0x003E Allegiance_ClearAllegianceOfficerTitles`.
    AllegianceClearAllegianceOfficerTitles,
    ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICER_TITLES
);
scalar_message!(
    /// `0x003F Allegiance_DoAllegianceLockAction`.
    AllegianceDoAllegianceLockAction,
    ALLEGIANCE_DO_ALLEGIANCE_LOCK_ACTION,
    action,
    u32,
    u32,
    u32
);
string_message!(
    /// `0x0040 Allegiance_SetAllegianceApprovedVassal`.
    AllegianceSetAllegianceApprovedVassal,
    ALLEGIANCE_SET_ALLEGIANCE_APPROVED_VASSAL,
    name
);
scalar_message!(
    /// `0x0042 Allegiance_DoAllegianceHouseAction`.
    AllegianceDoAllegianceHouseAction,
    ALLEGIANCE_DO_ALLEGIANCE_HOUSE_ACTION,
    action,
    u32,
    u32,
    u32
);
string_message!(
    /// `0x0254 Allegiance_SetMotd`.
    AllegianceSetMotd,
    ALLEGIANCE_SET_MOTD,
    motd
);
empty_message!(
    /// `0x0255 Allegiance_QueryMotd`.
    AllegianceQueryMotd,
    ALLEGIANCE_QUERY_MOTD
);
empty_message!(
    /// `0x0256 Allegiance_ClearMotd`.
    AllegianceClearMotd,
    ALLEGIANCE_CLEAR_MOTD
);
string_message!(
    /// `0x027B Allegiance_AllegianceInfoRequest`.
    AllegianceInfoRequest,
    ALLEGIANCE_ALLEGIANCE_INFO_REQUEST,
    name
);
string_message!(
    /// `0x02A1 Allegiance_AddAllegianceBan`.
    AllegianceAddAllegianceBan,
    ALLEGIANCE_ADD_ALLEGIANCE_BAN,
    name
);
string_message!(
    /// `0x02A2 Allegiance_RemoveAllegianceBan`.
    AllegianceRemoveAllegianceBan,
    ALLEGIANCE_REMOVE_ALLEGIANCE_BAN,
    name
);
empty_message!(
    /// `0x02A3 Allegiance_ListAllegianceBans`.
    AllegianceListAllegianceBans,
    ALLEGIANCE_LIST_ALLEGIANCE_BANS
);
string_message!(
    /// `0x02A5 Allegiance_RemoveAllegianceOfficer`.
    AllegianceRemoveAllegianceOfficer,
    ALLEGIANCE_REMOVE_ALLEGIANCE_OFFICER,
    name
);
empty_message!(
    /// `0x02A6 Allegiance_ListAllegianceOfficers`.
    AllegianceListAllegianceOfficers,
    ALLEGIANCE_LIST_ALLEGIANCE_OFFICERS
);
empty_message!(
    /// `0x02A7 Allegiance_ClearAllegianceOfficers`.
    AllegianceClearAllegianceOfficers,
    ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICERS
);
empty_message!(
    /// `0x02AB Allegiance_RecallAllegianceHometown`.
    AllegianceRecallAllegianceHometown,
    ALLEGIANCE_RECALL_ALLEGIANCE_HOMETOWN
);

/// `0x003B Allegiance_SetAllegianceOfficer`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceSetAllegianceOfficer {
    pub name: String,
    pub level: u32,
}

impl Message for AllegianceSetAllegianceOfficer {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_SET_ALLEGIANCE_OFFICER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            level: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.u32(self.level);
        Ok(())
    }
}

/// `0x003C Allegiance_SetAllegianceOfficerTitle` — level **first**, then the title.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceSetAllegianceOfficerTitle {
    pub level: u32,
    pub title: String,
}

impl Message for AllegianceSetAllegianceOfficerTitle {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_SET_ALLEGIANCE_OFFICER_TITLE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            level: r.u32()?,
            title: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.level);
        w.pstring(&self.title)
    }
}

/// `0x0041 Allegiance_AllegianceChatGag`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceChatGag {
    pub name: String,
    pub gagged: i32,
}

impl Message for AllegianceChatGag {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_ALLEGIANCE_CHAT_GAG;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            gagged: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.i32(self.gagged);
        Ok(())
    }
}

/// `0x0277 Allegiance_BreakAllegianceBoot`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceBreakAllegianceBoot {
    pub name: String,
    pub account_boot: i32,
}

impl Message for AllegianceBreakAllegianceBoot {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_BREAK_ALLEGIANCE_BOOT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            account_boot: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.i32(self.account_boot);
        Ok(())
    }
}

/// `0x02A0 Allegiance_AllegianceChatBoot`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceChatBoot {
    pub name: String,
    pub reason: String,
}

impl Message for AllegianceChatBoot {
    const OPCODE: Opcode = Opcode::ALLEGIANCE_ALLEGIANCE_CHAT_BOOT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            reason: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.pstring(&self.reason)
    }
}

// ---------------------------------------------------------------------------------------------
// Fellowship
// ---------------------------------------------------------------------------------------------

/// `Fellow` — note there is **no id inside**: the id is the hash key in a full update and the
/// leading dword in [`FellowshipUpdateFellow`].
///
/// **Wire order is not struct order.** The fellow's unpack reads
/// `_cp_cache, _lum_cache, _level, _max_health, _max_stamina, _max_mana, _current_health,
/// _current_stamina, _current_mana, _share_loot` and **then** `_name`; ACE's `WriteFellow`
/// (`GameEventFellowshipFullUpdate.cs`) writes the same. Following the client's in-memory field
/// order (`_name` first, then `_level`, ...) round-trips happily but **no real `0x02BE` decodes
/// through it**: the first one the corpus carries, `fixtures/message-corpus/fellowship-one-vassal`
/// blob 622, fails with *"needed 4 bytes at offset 152, 0 available"* in struct order and decodes
/// to `One`/`Two` in `Of The ring` in wire order. The same defect class as [`AllegianceData`]. The
/// struct keeps its field order; only `read`/`write` are the wire.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fellow {
    pub name: String,
    pub level: u32,
    pub cp_cache: u32,
    pub lum_cache: u32,
    pub share_loot: i32,
    pub max_health: u32,
    pub max_stamina: u32,
    pub max_mana: u32,
    pub current_health: u32,
    pub current_stamina: u32,
    pub current_mana: u32,
}

impl Fellow {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        // In wire order -- see the struct's doc comment.
        let cp_cache = r.u32()?;
        let lum_cache = r.u32()?;
        let level = r.u32()?;
        let max_health = r.u32()?;
        let max_stamina = r.u32()?;
        let max_mana = r.u32()?;
        let current_health = r.u32()?;
        let current_stamina = r.u32()?;
        let current_mana = r.u32()?;
        let share_loot = r.i32()?;
        let name = r.pstring()?;
        Ok(Self {
            name,
            level,
            cp_cache,
            lum_cache,
            share_loot,
            max_health,
            max_stamina,
            max_mana,
            current_health,
            current_stamina,
            current_mana,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        // The pack mirrors the unpack: ten dwords, then the name.
        for v in [
            self.cp_cache,
            self.lum_cache,
            self.level,
            self.max_health,
            self.max_stamina,
            self.max_mana,
            self.current_health,
            self.current_stamina,
            self.current_mana,
        ] {
            w.u32(v);
        }
        w.i32(self.share_loot);
        w.pstring(&self.name)?;
        Ok(())
    }
}

/// One fellowship.
///
/// **The server sends a lock table the client never reads.** After the
/// departed-fellows table comes [`FellowshipLocks`]; the Sept 2013 and 2015 clients stop at the
/// departed-fellows table and ignore whatever follows. Every `0x02BE` in the retail captures
/// (recorded against Turbine's servers) carries that table: empty, `00 00 20 00` (count 0, table
/// size 32, the same bytes ACE writes), in an unlocked fellowship, and holding the fellowship's
/// lock entries in a locked one. So the codec writes it always and reads it when it is there.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fellowship {
    pub members: PackedHash<u32, Fellow>,
    pub name: String,
    pub leader: ObjectId,
    pub share_xp: i32,
    pub even_xp_split: i32,
    pub open_fellow: i32,
    pub locked: i32,
    pub fellows_departed: PackedHash<u32, i32>,
    pub locks: FellowshipLocks,
}

/// One entry of the fellowship lock table: twenty bytes, a dword, a double and two dwords.
///
/// The retail captures confirm the layout: every locked fellowship's full update decodes to its
/// last byte as name keys and these fields (the first always 0 there, the double minus the
/// seconds since the lock was made, the timestamp a Unix time in whole seconds).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FellowshipLock {
    /// Always 0 in the captures.
    pub unknown_1: u32,
    /// Minus the seconds since the lock was made, as the sending server saw it (for example
    /// -4288.56 for a lock made 4288 s earlier). The bytes ACE writes as two zero dwords.
    pub age: f64,
    /// Whole seconds.
    pub timestamp: u32,
    /// 1 when the lock is made, one more each time its timestamp is renewed.
    pub sequence: u32,
}

impl FellowshipLock {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            unknown_1: r.u32()?,
            age: r.f64()?,
            timestamp: r.u32()?,
            sequence: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.unknown_1);
        w.f64(self.age);
        w.u32(self.timestamp);
        w.u32(self.sequence);
    }
}

/// The fellowship lock table that ends [`FellowshipFullUpdate`]: a packed hash table of name to
/// [`FellowshipLock`], 32 buckets. Written in bucket order, the name's [`string_key_hash`] mod the
/// table size, whatever order the entries are held in: the retail server wrote them that way in
/// every captured table of more than one lock (ACE writes insertion order). Within a bucket the
/// order held is kept; retail's varied.
///
/// Its default is the empty table, `00 00 20 00`, as the retail server sent it for an unlocked
/// fellowship; a locked one's entries are keyed by the lock's name (a padded `u16` string).
#[derive(Debug, Clone, PartialEq)]
pub struct FellowshipLocks(pub PackedHash<String, FellowshipLock>);

impl FellowshipLocks {
    /// The table size the server writes.
    pub const TABLE_SIZE: u32 = 32;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        r.packed_hash(|r| Ok((r.pstring()?, FellowshipLock::read(r)?)))
            .map(Self)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_hash_in_bucket_order(
            &self.0,
            |k| string_key_hash(k),
            |w, k, v| {
                w.pstring(k)?;
                v.write(w);
                Ok(())
            },
        )
    }
}

impl Default for FellowshipLocks {
    /// The empty table as the retail server sent it: count 0, table size 32.
    fn default() -> Self {
        Self(PackedHash {
            table_size: Self::TABLE_SIZE,
            entries: Vec::new(),
        })
    }
}

/// `0x02BE Fellowship_FullUpdate` (S2C).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FellowshipFullUpdate(pub Fellowship);

impl Message for FellowshipFullUpdate {
    const OPCODE: Opcode = Opcode::FELLOWSHIP_FULL_UPDATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(Fellowship {
            members: r.packed_hash(|r| Ok((r.u32()?, Fellow::read(r)?)))?,
            name: r.pstring()?,
            leader: ObjectId(r.u32()?),
            share_xp: r.i32()?,
            even_xp_split: r.i32()?,
            open_fellow: r.i32()?,
            locked: r.i32()?,
            fellows_departed: r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?,
            // The client stops before the lock table, so a body without it still decodes.
            locks: if r.remaining() >= 4 {
                FellowshipLocks::read(r)?
            } else {
                FellowshipLocks::default()
            },
        }))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let f = &self.0;
        w.packed_hash(&f.members, |w, k, v| {
            w.u32(*k);
            v.write(w)
        })?;
        w.pstring(&f.name)?;
        w.u32(f.leader.0);
        w.i32(f.share_xp);
        w.i32(f.even_xp_split);
        w.i32(f.open_fellow);
        w.i32(f.locked);
        w.packed_hash(&f.fellows_departed, |w, k, v| {
            w.u32(*k);
            w.i32(*v);
            Ok(())
        })?;
        f.locks.write(w)
    }
}

/// `0x02C0 Fellowship_UpdateFellow` (S2C).
///
/// **The community catalogue omits the leading `ObjectID`.** The update-fellow dispatch
/// reads `*(uint32*)(msg+4)` as the id, unpacks the `Fellow` from +8, then reads the
/// update type. See `docs/CORRECTIONS.md`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FellowshipUpdateFellow {
    pub fellow_id: ObjectId,
    pub fellow: Fellow,
    /// The fellow-update type: 1 full update, 2 stats update, 3 vitals update.
    pub update_type: u32,
}

impl Message for FellowshipUpdateFellow {
    const OPCODE: Opcode = Opcode::FELLOWSHIP_UPDATE_FELLOW;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            fellow_id: ObjectId(r.u32()?),
            fellow: Fellow::read(r)?,
            update_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.fellow_id.0);
        self.fellow.write(w)?;
        w.u32(self.update_type);
        Ok(())
    }
}

empty_message!(
    /// `0x02BF Fellowship_Disband` (S2C).
    FellowshipDisband,
    FELLOWSHIP_DISBAND
);
empty_message!(
    /// `0x01C9 Fellowship_FellowUpdateDone` — the client does **nothing** with it; it exists so the
    /// server can bracket a burst of `0x02C0` updates.
    FellowshipFellowUpdateDone,
    FELLOWSHIP_FELLOW_UPDATE_DONE
);
empty_message!(
    /// `0x01CA Fellowship_FellowStatsDone` — likewise inert.
    FellowshipFellowStatsDone,
    FELLOWSHIP_FELLOW_STATS_DONE
);

/// `0x00A3 Fellowship_Quit`, **client to server**: whether to disband.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FellowshipQuitRequest {
    pub disband: i32,
}

impl Message for FellowshipQuitRequest {
    const OPCODE: Opcode = Opcode::FELLOWSHIP_QUIT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { disband: r.i32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.disband);
        Ok(())
    }
}

id_message!(
    /// `0x00A3 Fellowship_Quit`, **server to client**: the member who left.
    FellowshipQuitNotice,
    FELLOWSHIP_QUIT,
    member
);
id_message!(
    /// `0x00A4 Fellowship_Dismiss`, both directions: the target / the dismissed member. The two
    /// bodies happen to be identical here, unlike `0x00A3`.
    FellowshipDismiss,
    FELLOWSHIP_DISMISS,
    target
);
id_message!(
    /// `0x00A5 Fellowship_Recruit` (C2S).
    FellowshipRecruit,
    FELLOWSHIP_RECRUIT,
    target
);
id_message!(
    /// `0x0290 Fellowship_AssignNewLeader` (C2S).
    FellowshipAssignNewLeader,
    FELLOWSHIP_ASSIGN_NEW_LEADER,
    target
);
scalar_message!(
    /// `0x00A6 Fellowship_UpdateRequest` (C2S) — the subscribe toggle for the live vitals feed.
    FellowshipUpdateRequest,
    FELLOWSHIP_UPDATE_REQUEST,
    on,
    i32,
    i32,
    i32
);
scalar_message!(
    /// C2S opcode `0x0291`, which changes fellowship openness.
    FellowshipChangeFellowOpenness,
    FELLOWSHIP_CHANGE_FELLOW_OPENNESS,
    open,
    i32,
    i32,
    i32
);

/// `0x00A2 Fellowship_Create` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FellowshipCreate {
    pub name: String,
    pub share_xp: i32,
}

impl Message for FellowshipCreate {
    const OPCODE: Opcode = Opcode::FELLOWSHIP_CREATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            share_xp: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.i32(self.share_xp);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Contracts
// ---------------------------------------------------------------------------------------------

/// One contract tracker — exactly 28 (0x1C) bytes.
///
/// The two times are IEEE **doubles**; the community catalogue types them as `long`.
/// `_time_of_server_update` is client-only and never on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContractTracker {
    pub version: u32,
    pub contract_id: u32,
    pub contract_stage: u32,
    pub time_when_done: f64,
    pub time_when_repeats: f64,
}

impl ContractTracker {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            version: r.u32()?,
            contract_id: r.u32()?,
            contract_stage: r.u32()?,
            time_when_done: r.f64()?,
            time_when_repeats: r.f64()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.version);
        w.u32(self.contract_id);
        w.u32(self.contract_stage);
        w.f64(self.time_when_done);
        w.f64(self.time_when_repeats);
    }
}

/// `0x0314 Social_SendClientContractTrackerTable` (S2C) — the whole table is replaced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SocialSendClientContractTrackerTable(pub PackedHash<u32, ContractTracker>);

impl Message for SocialSendClientContractTrackerTable {
    const OPCODE: Opcode = Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER_TABLE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(r.packed_hash(|r| {
            Ok((r.u32()?, ContractTracker::read(r)?))
        })?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_hash(&self.0, |w, k, v| {
            w.u32(*k);
            v.write(w);
            Ok(())
        })
    }
}

/// `0x0315 Social_SendClientContractTracker` (S2C).
///
/// The client reads 36 bytes (the tracker and two flags) and never checks for more. The retail
/// server sent **48**: twelve more bytes after the flags that vary from message to message and look
/// like uninitialised memory. The codec accepts that opaque
/// tail without interpreting or keeping it, and writes the 48-byte form with the tail as zeros
/// (V294). A 36-byte body (ACE's) still reads.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SocialSendClientContractTracker {
    pub tracker: ContractTracker,
    pub delete_contract: i32,
    pub set_as_display_contract: i32,
}

/// The length of the retail server's unread tail on `0x0315`.
pub const CONTRACT_TRACKER_TAIL: usize = 12;

impl Message for SocialSendClientContractTracker {
    const OPCODE: Opcode = Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let m = Self {
            tracker: ContractTracker::read(r)?,
            delete_contract: r.i32()?,
            set_as_display_contract: r.i32()?,
        };
        if r.remaining() >= CONTRACT_TRACKER_TAIL {
            r.bytes(CONTRACT_TRACKER_TAIL)?;
        }
        Ok(m)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.tracker.write(w);
        w.i32(self.delete_contract);
        w.i32(self.set_as_display_contract);
        w.bytes(&[0; CONTRACT_TRACKER_TAIL]);
        Ok(())
    }
}

scalar_message!(
    /// `0x0316 Social_AbandonContract` (C2S).
    SocialAbandonContract,
    SOCIAL_ABANDON_CONTRACT,
    contract_id,
    u32,
    u32,
    u32
);

#[cfg(test)]
mod tests {
    /// `LoggedIn` and `MayPassupExperience` are **different bits**, and confusing them is silent.
    ///
    /// Oracle: the logged-in test is `_bitfield & 1`;
    /// the pass-up-experience setter is `|= 0x10` and `&= 0xFFFFFFEF`. The knowledge base's
    /// own index table agrees: logged-in is 1, may-pass-up-experience is 16 (the recovered
    /// allegiance behavior).
    ///
    /// `MAY_PASSUP_EXPERIENCE` was once `0x0001`, so `may_passup_experience()` was
    /// answering "is this member online". Nothing failed; the allegiance simply behaved differently.
    /// The test asserts the two predicates **disagree** on each single-bit input, which is the
    /// property a shared constant cannot satisfy.
    #[test]
    fn logged_in_and_may_passup_experience_are_not_the_same_bit() {
        let online = AllegianceData {
            bitfield: AllegianceData::LOGGED_IN,
            ..Default::default()
        };
        assert!(online.is_logged_in());
        assert!(
            !online.may_passup_experience(),
            "bit 0 is LoggedIn, not MayPassupExperience"
        );

        let tithes = AllegianceData {
            bitfield: AllegianceData::MAY_PASSUP_EXPERIENCE,
            ..Default::default()
        };
        assert!(tithes.may_passup_experience());
        assert!(
            !tithes.is_logged_in(),
            "bit 4 is MayPassupExperience, not LoggedIn"
        );

        assert_eq!(AllegianceData::MAY_PASSUP_EXPERIENCE, 0x10);
        assert_eq!(AllegianceData::LOGGED_IN, 0x01);
    }

    use super::*;
    use crate::{read_body, round_trip, write_body};

    /// The friends-update dispatch reads a count-prefixed list of `FriendData` and *then*
    /// the update type. This order is documented in
    /// `docs/CORRECTIONS.md`.
    #[test]
    fn friends_update_carries_a_list_not_one_friend() {
        let m = SocialFriendsUpdate {
            friends: vec![
                FriendData {
                    id: ObjectId(1),
                    name: "Alice".into(),
                    ..FriendData::default()
                },
                FriendData {
                    id: ObjectId(2),
                    name: "Bob".into(),
                    ..FriendData::default()
                },
            ],
            update_type: 0,
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            2,
            "the body opens with a count, not with a FriendData's object id"
        );
        let back: SocialFriendsUpdate = round_trip(&bytes);
        assert_eq!(back.friends.len(), 2);

        // A single-friend reading takes the count as the id and then desynchronises: here it reads
        // a friends-list length out of the middle of a name and fails outright.
        let mut r = Reader::body(&bytes);
        assert!(
            FriendData::read(&mut r).is_err(),
            "the catalogue's single-FriendData reading does not survive a two-friend update"
        );
    }

    /// Oracle: `docs/networking/messages/09-social.md` §1 and §4 — the catalogue omits the
    /// leading `ObjectID` entirely.
    #[test]
    fn update_fellow_begins_with_an_object_id() {
        let m = FellowshipUpdateFellow {
            fellow_id: ObjectId(0x5000_0001),
            fellow: Fellow {
                name: "Alice".into(),
                level: 50,
                ..Fellow::default()
            },
            update_type: 2,
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x5000_0001,
            "the id precedes the Fellow record"
        );
        let _: FellowshipUpdateFellow = round_trip(&bytes);
    }

    fn member(id: u32, name: &str) -> AllegianceData {
        AllegianceData {
            id: ObjectId(id),
            name: name.into(),
            bitfield: AllegianceData::HAS_PACKED_LEVEL | AllegianceData::HAS_ALLEGIANCE_AGE,
            ..AllegianceData::default()
        }
    }

    /// An allegiance record is laid out as unpack reads it.
    #[test]
    fn an_allegiance_record_is_laid_out_as_unpack_reads_it() {
        let mut w = Writer::body();
        let d = AllegianceData {
            id: ObjectId(0x5000_001F),
            name: "Bex".into(),
            gender: 1,
            heritage_group: 1,
            rank: 1,
            level: 1,
            bitfield: AllegianceData::LOGGED_IN
                | AllegianceData::HAS_ALLEGIANCE_AGE
                | AllegianceData::HAS_PACKED_LEVEL,
            cp_tithed: 0,
            cp_cached: 0,
            loyalty: 10,
            leadership: 5,
            time_online: 0,
            allegiance_age: 0,
        };
        d.write(&mut w).unwrap();
        let b = w.into_inner();
        // The monarch record out of `fixtures/packet-captures/fellowship-one-vassal.jsonl`'s 244-byte `0x0020` at
        // `t_rel = 293.487227`: 44 bytes with only the three-byte name pseudonymized. All other
        // operands stay exact, including the only member the wrong reader could even reach.
        let recorded: [u8; 44] = [
            0x1F, 0x00, 0x00, 0x50, // _id        = 0x5000001F
            0x00, 0x00, 0x00, 0x00, // _cp_cached = 0
            0x00, 0x00, 0x00, 0x00, // _cp_tithed = 0
            0x0D, 0x00, 0x00, 0x00, // _bitfield  = LoggedIn|HasAllegianceAge|HasPackedLevel
            0x01, // _gender, ONE byte
            0x01, // _hg, ONE byte
            0x01, 0x00, // _rank, TWO bytes
            0x01, 0x00, 0x00, 0x00, // _level, four, and only because HasPackedLevel is set
            0x0A, 0x00, // _loyalty, TWO bytes
            0x05, 0x00, // _leadership, TWO bytes
            0x00, 0x00, 0x00, 0x00, // _time_online, gated on HasAllegianceAge
            0x00, 0x00, 0x00, 0x00, // _allegiance_age, same gate
            0x03, 0x00, b'B', b'e', b'x', 0x00, 0x00, 0x00, // _name, LAST and padded to four
        ];
        assert_eq!(
            b.as_slice(),
            &recorded[..],
            "the record with its same-width name pseudonym, byte for byte"
        );
        // Spelled out again field by field, because a single slice comparison says "wrong"
        // without saying where.
        assert_eq!(
            &b[0..16],
            &recorded[0..16],
            "id, cp_cached, cp_tithed, bitfield"
        );
        assert_eq!(b[16], 1, "_gender is one byte");
        assert_eq!(b[17], 1, "_hg is one byte");
        assert_eq!(u16::from_le_bytes([b[18], b[19]]), 1, "_rank is two bytes");
        assert_eq!(
            u32::from_le_bytes([b[20], b[21], b[22], b[23]]),
            1,
            "_level, gated"
        );
        assert_eq!(
            u16::from_le_bytes([b[24], b[25]]),
            10,
            "_loyalty is two bytes"
        );
        assert_eq!(
            u16::from_le_bytes([b[26], b[27]]),
            5,
            "_leadership is two bytes"
        );
        assert_eq!(
            &b[28..36],
            &[0u8; 8],
            "_time_online then _allegiance_age, gated"
        );
        assert_eq!(
            &b[36..],
            b"\x03\x00Bex\0\0\0",
            "the name is LAST, not second"
        );
        let mut r = Reader::body(&b);
        assert_eq!(AllegianceData::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// The two gates decide the record's **length**. A reader that ignores them desynchronises
    /// every record after the first, which is how a three-member roster became `UnexpectedEof`.
    #[test]
    fn the_two_length_gates_shorten_the_record() {
        let full = member(1, "");
        let mut w = Writer::body();
        full.write(&mut w).unwrap();
        let full_len = w.into_inner().len();

        let no_level = AllegianceData {
            bitfield: AllegianceData::HAS_ALLEGIANCE_AGE,
            ..full
        };
        let mut w = Writer::body();
        no_level.write(&mut w).unwrap();
        assert_eq!(
            w.into_inner().len(),
            full_len - 4,
            "no HasPackedLevel, no level dword"
        );

        let no_age = AllegianceData {
            bitfield: AllegianceData::HAS_PACKED_LEVEL,
            ..member(1, "")
        };
        let mut w = Writer::body();
        no_age.write(&mut w).unwrap();
        let bytes = w.into_inner();
        assert_eq!(
            bytes.len(),
            full_len,
            "the legacy f64 is the same eight bytes as two i32"
        );
        let mut r = Reader::body(&bytes);
        let back = AllegianceData::read(&mut r).unwrap();
        assert_eq!(
            back.bitfield,
            AllegianceData::HAS_PACKED_LEVEL,
            "HasAllegianceAge does not make the client set anything"
        );

        // The one bit the reader writes for itself.
        let legacy = AllegianceData {
            bitfield: 0,
            ..member(1, "")
        };
        let mut w = Writer::body();
        legacy.write(&mut w).unwrap();
        let legacy_bytes = w.into_inner();
        let mut r = Reader::body(&legacy_bytes);
        let back = AllegianceData::read(&mut r).unwrap();
        assert_eq!(
            back.bitfield,
            AllegianceData::MAY_PASSUP_EXPERIENCE,
            "no packed level is the legacy form, and the legacy form always tithed"
        );
        r.expect_exhausted().unwrap();
    }

    /// Oracle: the allegiance hierarchy's first dword is count in the low half
    /// and version in the high half, and every field after it is gated on the version.
    #[test]
    fn the_allegiance_hierarchy_is_versioned_in_the_first_dword() {
        let h = AllegianceHierarchy {
            version: allegiance_version::NEWEST,
            officers: Some(PHash::new(vec![(1u32, 2u32)])),
            officer_titles: Some(vec!["Speaker".into()]),
            pools: Some(AllegiancePools::default()),
            motd: Some(("hello".into(), "Alice".into())),
            chat_room_id: Some(7),
            bind_point: Some(PositionWire::default()),
            allegiance_name: Some(("The Guild".into(), 0)),
            is_locked: Some(0),
            approved_vassal: Some(0),
            old_officer: None,
            members: vec![
                (None, member(1, "Monarch")),
                (Some(ObjectId(1)), member(2, "Vassal")),
            ],
        };
        let mut w = Writer::body();
        h.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let dw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(dw & 0xFFFF, 2, "two members");
        assert_eq!(dw >> 16, allegiance_version::NEWEST);
        let mut r = Reader::body(&bytes);
        assert_eq!(AllegianceHierarchy::read(&mut r).unwrap(), h);
        r.expect_exhausted().unwrap();
    }

    /// A version-1 hierarchy skips the officer table and reads the single old officer id instead;
    /// version 5 adds nothing at all.
    #[test]
    fn the_older_allegiance_versions_read_different_fields() {
        for (version, extra) in [(1u32, 4usize), (5, 4)] {
            let h = AllegianceHierarchy {
                version,
                old_officer: Some(0),
                pools: if version >= 2 {
                    Some(AllegiancePools::default())
                } else {
                    None
                },
                motd: if version >= 3 {
                    Some((String::new(), String::new()))
                } else {
                    None
                },
                chat_room_id: if version >= 4 { Some(0) } else { None },
                members: vec![],
                ..AllegianceHierarchy::default()
            };
            let mut w = Writer::body();
            h.write(&mut w).unwrap();
            let bytes = w.into_inner();
            let mut r = Reader::body(&bytes);
            assert_eq!(
                AllegianceHierarchy::read(&mut r).unwrap(),
                h,
                "version {version}"
            );
            r.expect_exhausted().unwrap();
            assert!(bytes.len() >= 4 + extra);
        }
    }

    /// Oracle: a contract tracker is exactly 28 bytes, with the two times as
    /// doubles. The catalogue types them as `long`.
    #[test]
    fn a_contract_tracker_is_twenty_eight_bytes_with_two_doubles() {
        let t = ContractTracker {
            version: 1,
            contract_id: 100,
            contract_stage: 2,
            time_when_done: 1.5,
            time_when_repeats: -1.0,
        };
        let mut w = Writer::body();
        t.write(&mut w);
        assert_eq!(w.len(), 0x1C);
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(ContractTracker::read(&mut r).unwrap(), t);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: retail server `0x0315` bodies from the January 2017 captures (after the event
    /// type). Each is 48 bytes: the client's 36 and a 12-byte tail that differs every time. The
    /// codec reads them exhausted, keeps the client's fields, and re-encodes the 36 with the tail
    /// zeroed. ACE's 36-byte body still reads, and re-encodes as 48.
    #[test]
    fn a_contract_tracker_update_accepts_retails_twelve_byte_tail_and_writes_zeros() {
        fn hex(s: &str) -> Vec<u8> {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        }
        for (body, id) in [
            ("000000002f0100000400000000000000000000000000000000000000000000000000000078daac30d07fe53a507fe53a", 0x12F),
            ("000000003001000004000000000000000000000000000000000000000000000000000000f4ced52fb4ced52f34ced52f", 0x130),
            ("000000003101000004000000000000000000000000000000000000000000000000000000007f912ec07e912e407e912e", 0x131),
        ] {
            let body = hex(body);
            assert_eq!(body.len(), 48);
            let m: SocialSendClientContractTracker = read_body(&body).unwrap();
            assert_eq!((m.tracker.contract_id, m.tracker.contract_stage), (id, 4));
            let again = write_body(&m).unwrap();
            assert_eq!(again[..36], body[..36], "the client's 36 bytes are reproduced");
            assert_eq!(again[36..], [0u8; 12], "the tail is written as zeros");
            let _: SocialSendClientContractTracker = round_trip(&again);

            let short: SocialSendClientContractTracker = read_body(&body[..36]).unwrap();
            assert_eq!(short, m, "the 36-byte form reads the same fields");
        }
    }

    /// `0x00A3` carries a different body in each direction — an `int32` out, an `ObjectID` back.
    #[test]
    fn fellowship_quit_differs_by_direction() {
        assert_eq!(FellowshipQuitRequest::OPCODE, FellowshipQuitNotice::OPCODE);
        let out = write_body(&FellowshipQuitRequest { disband: 1 }).unwrap();
        let back = write_body(&FellowshipQuitNotice {
            member: ObjectId(1),
        })
        .unwrap();
        assert_eq!(
            out.len(),
            back.len(),
            "both are four bytes; the meaning differs"
        );
        let _: FellowshipQuitRequest = round_trip(&out);
        let _: FellowshipQuitNotice = round_trip(&back);
    }

    #[test]
    fn the_rest_of_the_family_round_trips() {
        let _: SocialAddFriend =
            round_trip(&write_body(&SocialAddFriend { name: "Bob".into() }).unwrap());
        let _: SocialRemoveFriend = round_trip(
            &write_body(&SocialRemoveFriend {
                friend_id: ObjectId(1),
            })
            .unwrap(),
        );
        assert_eq!(write_body(&SocialClearFriends).unwrap().len(), 0);
        let _: SocialSendFriendsCommand = round_trip(
            &write_body(&SocialSendFriendsCommand {
                cmd: 0,
                player: "Bob".into(),
            })
            .unwrap(),
        );
        let _: CharacterTitlesMessage = round_trip(
            &write_body(&CharacterTitlesMessage {
                version: 1,
                display_title: 1,
                titles: vec![1, 2, 3],
            })
            .unwrap(),
        );
        let _: SocialAddOrSetCharacterTitle = round_trip(
            &write_body(&SocialAddOrSetCharacterTitle {
                new_title: 2,
                set_as_display_title: 1,
            })
            .unwrap(),
        );
        let _: SocialSetDisplayCharacterTitle =
            round_trip(&write_body(&SocialSetDisplayCharacterTitle { title_id: 2 }).unwrap());

        let profile = AllegianceProfile {
            total_members: 2,
            total_vassals: 1,
            hierarchy: AllegianceHierarchy {
                version: allegiance_version::NEWEST,
                officers: Some(PHash::new(vec![])),
                officer_titles: Some(vec![]),
                pools: Some(AllegiancePools::default()),
                motd: Some((String::new(), String::new())),
                chat_room_id: Some(0),
                bind_point: Some(PositionWire::default()),
                allegiance_name: Some((String::new(), 0)),
                is_locked: Some(0),
                approved_vassal: Some(0),
                members: vec![(
                    None,
                    AllegianceData {
                        bitfield: AllegianceData::HAS_PACKED_LEVEL
                            | AllegianceData::HAS_ALLEGIANCE_AGE,
                        ..AllegianceData::default()
                    },
                )],
                old_officer: None,
            },
        };
        let _: AllegianceUpdate = round_trip(
            &write_body(&AllegianceUpdate {
                rank: 1,
                profile: profile.clone(),
            })
            .unwrap(),
        );
        let _: AllegianceInfoResponse = round_trip(
            &write_body(&AllegianceInfoResponse {
                target: ObjectId(1),
                profile,
            })
            .unwrap(),
        );
        let _: AllegianceUpdateAborted =
            round_trip(&write_body(&AllegianceUpdateAborted { failure_type: 5 }).unwrap());
        let _: AllegianceLoginNotification = round_trip(
            &write_body(&AllegianceLoginNotification {
                member: ObjectId(1),
                now_logged_in: 1,
            })
            .unwrap(),
        );
        let _: AllegianceSetAllegianceOfficer = round_trip(
            &write_body(&AllegianceSetAllegianceOfficer {
                name: "Bob".into(),
                level: 2,
            })
            .unwrap(),
        );
        let _: AllegianceSetAllegianceOfficerTitle = round_trip(
            &write_body(&AllegianceSetAllegianceOfficerTitle {
                level: 2,
                title: "Speaker".into(),
            })
            .unwrap(),
        );
        let _: AllegianceChatGag = round_trip(
            &write_body(&AllegianceChatGag {
                name: "Bob".into(),
                gagged: 1,
            })
            .unwrap(),
        );
        let _: AllegianceBreakAllegianceBoot = round_trip(
            &write_body(&AllegianceBreakAllegianceBoot {
                name: "Bob".into(),
                account_boot: 0,
            })
            .unwrap(),
        );
        let _: AllegianceChatBoot = round_trip(
            &write_body(&AllegianceChatBoot {
                name: "Bob".into(),
                reason: "spam".into(),
            })
            .unwrap(),
        );

        let _: FellowshipFullUpdate = round_trip(
            &write_body(&FellowshipFullUpdate(Fellowship {
                members: PackedHash {
                    table_size: 8,
                    entries: vec![(
                        1u32,
                        Fellow {
                            name: "Alice".into(),
                            ..Fellow::default()
                        },
                    )],
                },
                name: "Team".into(),
                leader: ObjectId(1),
                share_xp: 1,
                even_xp_split: 1,
                open_fellow: 0,
                locked: 0,
                fellows_departed: PackedHash {
                    table_size: 8,
                    entries: vec![],
                },
                locks: FellowshipLocks::default(),
            }))
            .unwrap(),
        );
        assert_eq!(write_body(&FellowshipDisband).unwrap().len(), 0);
        assert_eq!(write_body(&FellowshipFellowUpdateDone).unwrap().len(), 0);
        assert_eq!(write_body(&FellowshipFellowStatsDone).unwrap().len(), 0);
        let _: FellowshipCreate = round_trip(
            &write_body(&FellowshipCreate {
                name: "Team".into(),
                share_xp: 1,
            })
            .unwrap(),
        );
        let _: FellowshipRecruit = round_trip(
            &write_body(&FellowshipRecruit {
                target: ObjectId(2),
            })
            .unwrap(),
        );
        let _: FellowshipDismiss = round_trip(
            &write_body(&FellowshipDismiss {
                target: ObjectId(2),
            })
            .unwrap(),
        );
        let _: FellowshipAssignNewLeader = round_trip(
            &write_body(&FellowshipAssignNewLeader {
                target: ObjectId(2),
            })
            .unwrap(),
        );
        let _: FellowshipUpdateRequest =
            round_trip(&write_body(&FellowshipUpdateRequest { on: 1 }).unwrap());
        let _: FellowshipChangeFellowOpenness =
            round_trip(&write_body(&FellowshipChangeFellowOpenness { open: 1 }).unwrap());

        let _: SocialSendClientContractTrackerTable = round_trip(
            &write_body(&SocialSendClientContractTrackerTable(PackedHash {
                table_size: 8,
                entries: vec![(100u32, ContractTracker::default())],
            }))
            .unwrap(),
        );
        let _: SocialSendClientContractTracker = round_trip(
            &write_body(&SocialSendClientContractTracker {
                tracker: ContractTracker::default(),
                delete_contract: 0,
                set_as_display_contract: 1,
            })
            .unwrap(),
        );
        let _: SocialAbandonContract =
            round_trip(&write_body(&SocialAbandonContract { contract_id: 100 }).unwrap());
    }

    /// A full update ends with the empty lock table retail sent.
    #[test]
    fn a_full_update_ends_with_the_empty_lock_table_retail_sent() {
        let body = write_body(&FellowshipFullUpdate::default()).unwrap();
        assert_eq!(body[body.len() - 4..], [0x00, 0x00, 0x20, 0x00]);
        let m: FellowshipFullUpdate = round_trip(&body);
        assert_eq!(m.0.locks, FellowshipLocks::default());

        let without: FellowshipFullUpdate = read_body(&body[..body.len() - 4]).unwrap();
        assert_eq!(
            without, m,
            "a body without the lock table reads as the empty table"
        );

        // A non-empty entry: a name, then a dword, a double (0.0 here) and two dwords.
        let lock = FellowshipLock {
            timestamp: 5678,
            sequence: 2,
            ..FellowshipLock::default()
        };
        let locks = FellowshipLocks(PackedHash {
            table_size: 32,
            entries: vec![("a".into(), lock)],
        });
        let m = FellowshipFullUpdate(Fellowship {
            locks,
            ..Fellowship::default()
        });
        let body = write_body(&m).unwrap();
        assert_eq!(
            body[body.len() - 28..],
            [
                0x01, 0x00, 0x20, 0x00, 0x01, 0x00, b'a', 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0x2E, 0x16, 0, 0, 0x02, 0, 0, 0,
            ]
        );
        let _: FellowshipFullUpdate = round_trip(&body);
    }

    /// A locked fellowship's full update as the retail server sent it (the retail captures,
    /// `pkt_2017-1-25_1485380032_log.pcap`, t = 3872.5 s; the body after the game-event header):
    /// nine members, `locked` 1, an empty departed table, then a lock table of three entries, each
    /// a name (padded to four bytes) and twenty bytes of lock. The codec reads every byte of it and writes
    /// it back identically.
    #[test]
    fn a_locked_full_update_from_the_retail_captures_round_trips() {
        let body = hex(concat!(
            "0900100063440350000000000000000013010000AA010000DC0100000E020000A1010000DC010000F901000001000000",
            "07004D61672D6F6E6500000064440350000000000000000013010000AC010000400200000E020000AC01000040020000",
            "0E0200000100000007004D61672D74776F00000065440350000000000000000013010000A20100004002000004020000",
            "A201000040020000040200000100000009004D61672D74687265650066440350000000000000000013010000A2010000",
            "4002000004020000A201000040020000040200000100000008004D61672D666F75720000674403500000000000000000",
            "13010000A20100004002000004020000A201000040020000040200000100000008004D61672D66697665000068440350",
            "000000000000000013010000A20100004002000004020000A201000040020000040200000100000007004D61672D7369",
            "780000006A440350000000000000000013010000BB010000040200009A020000BB010000040200009A02000001000000",
            "09004D61672D6569676874006B440350000000000000000013010000BF010000040200009A020000BF01000004020000",
            "FC0100000100000008004D61672D6E696E6500006C440350000000000000000013010000BA010000040200009A020000",
            "BA01000004020000EA0100000100000009004D61672D736576656E000400417364660000634403500100000001000000",
            "000000000100000000002000030020000F00416363657373436F6C6F737365756D0000000000000000001CE999A1ACC0",
            "9A1A8958010000001000416363657373436F6C6F737365756D4100000000000000001CE999A1ACC09A1A895801000000",
            "0E0046656C6C6F7773686970546573740000000000000000F834A8BFEA28895802000000",
        ));
        let m: FellowshipFullUpdate = read_body(&body).expect("the whole body decodes");
        let f = &m.0;
        assert_eq!(
            (f.name.as_str(), f.leader, f.locked, f.open_fellow),
            ("Asdf", ObjectId(0x5003_4463), 1, 0)
        );
        assert_eq!(f.members.entries.len(), 9);
        assert!(f.fellows_departed.entries.is_empty());
        assert_eq!(f.locks.0.table_size, FellowshipLocks::TABLE_SIZE);
        let locks: Vec<(&str, FellowshipLock)> = f
            .locks
            .0
            .entries
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        let colosseum = FellowshipLock {
            unknown_1: 0,
            age: f64::from_bits(0xC0AC_A199_E91C_0000),
            timestamp: 0x5889_1A9A,
            sequence: 1,
        };
        assert_eq!(
            locks,
            [
                ("AccessColosseum", colosseum),
                ("AccessColosseumA", colosseum),
                (
                    "FellowshipTest",
                    FellowshipLock {
                        unknown_1: 0,
                        age: f64::from_bits(0xBFA8_34F8_0000_0000),
                        timestamp: 1_485_383_914,
                        sequence: 2
                    }
                ),
            ]
        );
        // The double is minus the seconds since each lock was made: about an hour for the two
        // Colosseum locks, a twentieth of a second for FellowshipTest.
        let ages: Vec<f64> = locks.iter().map(|(_, l)| l.age).collect();
        assert!((ages[0] + 3664.8).abs() < 0.1, "{ages:?}");
        assert!(ages.iter().all(|&a| a < 0.0 && a > -86_400.0), "{ages:?}");
        assert!(ages[2] > -1.0, "{ages:?}");
        assert_eq!(write_body(&m).unwrap(), body, "written back byte for byte");
        // The captured order is bucket order under the string hash: 13, 17, 20.
        let buckets: Vec<u32> = f
            .locks
            .0
            .entries
            .iter()
            .map(|(k, _)| string_key_hash(k) % 32)
            .collect();
        assert_eq!(buckets, [13, 17, 20]);
    }

    /// Locks held out of bucket order are written in bucket order; two in one bucket keep the order
    /// held. `"b"` hashes to 0x62 (bucket 2), `"a"` to 0x61 (bucket 1), `"A"` to 0x41 (bucket 1).
    #[test]
    fn locks_are_written_in_bucket_order_and_held_order_within_a_bucket() {
        let lock = |n| FellowshipLock {
            timestamp: n,
            sequence: 1,
            ..FellowshipLock::default()
        };
        let held = |names: &[&str]| {
            FellowshipLocks(PackedHash {
                table_size: 32,
                entries: names
                    .iter()
                    .zip(1..)
                    .map(|(k, n)| ((*k).to_owned(), lock(n)))
                    .collect(),
            })
        };
        let written = |t: &FellowshipLocks| {
            let mut w = Writer::new();
            t.write(&mut w).unwrap();
            let got = FellowshipLocks::read(&mut Reader::new(&w.into_inner())).unwrap();
            got.0
                .entries
                .into_iter()
                .map(|(k, v)| (k, v.timestamp))
                .collect::<Vec<_>>()
        };
        let s = |v: &[(&str, u32)]| {
            v.iter()
                .map(|(k, n)| ((*k).to_owned(), *n))
                .collect::<Vec<_>>()
        };
        assert_eq!(written(&held(&["b", "a"])), s(&[("a", 2), ("b", 1)]));
        assert_eq!(
            written(&held(&["b", "a", "A"])),
            s(&[("a", 2), ("A", 3), ("b", 1)])
        );
        assert_eq!(
            written(&held(&["b", "A", "a"])),
            s(&[("A", 2), ("a", 3), ("b", 1)])
        );
        assert_eq!(
            written(&held(&[
                "FellowshipTest",
                "AccessColosseumA",
                "AccessColosseum"
            ])),
            s(&[
                ("AccessColosseum", 3),
                ("AccessColosseumA", 2),
                ("FellowshipTest", 1)
            ])
        );
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }
}
