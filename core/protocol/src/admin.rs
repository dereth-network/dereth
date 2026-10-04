//! Family: admin, character progression, personal queries — `docs/networking/messages/12-admin-and-misc.md`
//! plus the **DDD** dat-patching set.
//!
//! **The DDD correction.** The community catalogue calls `0xF7EB DDD_EndDDDMessage` the
//! client-to-server end-of-patching message. It is not: the end message is **`0xF7EA` in both
//! directions** (the sender builds one with type `0xF7EA`, and the receive path dispatches a
//! received `0xF7EA` to the same handler), and `0xF7EB` is a
//! separate **received-only** "patching pending, wait" message the client turns into
//! `DDD_PatchtimePending`. ACE agrees.
//!
//! **The plugin-audit correction.** The catalogue gives the three `Admin_QueryPlugin*` messages
//! empty payloads. All three carry payloads, and `0x02B3` unpacks **five** strings — the fifth has
//! no name in the client. The handler is a folded stub so the strings are
//! dropped, but a rebuild must still consume them or the cursor is left in the wrong place.
//!
//! DDD messages are framed by an `Archive`, not by `PackObj`: the cache wraps
//! each blob in a transient archive in unpacking mode, which is **not word-aligned**, so nothing in
//! this half of the file aligns.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::Message;
use dereth_primitives::ObjectId;

macro_rules! empty_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> { Ok(Self) }
            fn write(&self, _: &mut Writer) -> Result<(), MessageError> { Ok(()) }
        }
    };
}

macro_rules! string_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name { pub $field: String }
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

// ---------------------------------------------------------------------------------------------
// 1. Character progression
// ---------------------------------------------------------------------------------------------

/// The four `Train_*` game actions: two dwords each, 12 bytes with the sub-type.
macro_rules! train_action {
    ($(#[$m:meta])* $name:ident, $op:ident, $a:ident, $b:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub $a: u32,
            pub $b: u32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $a: r.u32()?, $b: r.u32()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.$a);
                w.u32(self.$b);
                Ok(())
            }
        }
    };
}

train_action!(
    /// `0x0044 Train_TrainAttribute2nd`.
    TrainAttribute2nd,
    TRAIN_TRAIN_ATTRIBUTE2ND,
    vital_id,
    xp_spent
);
train_action!(
    /// `0x0045 Train_TrainAttribute`.
    TrainAttribute,
    TRAIN_TRAIN_ATTRIBUTE,
    attribute_id,
    xp_spent
);
train_action!(
    /// `0x0046 Train_TrainSkill`.
    TrainSkill,
    TRAIN_TRAIN_SKILL,
    skill_id,
    xp_spent
);
train_action!(
    /// `0x0047 Train_TrainSkillAdvancementClass`.
    TrainSkillAdvancementClass,
    TRAIN_TRAIN_SKILL_ADVANCEMENT_CLASS,
    skill_id,
    credits_spent
);

// ---------------------------------------------------------------------------------------------
// 2. Personal queries
// ---------------------------------------------------------------------------------------------

/// `0x01C2 Character_QueryAge` and `0x01C4 Character_QueryBirth` (C2S). Target 0 means self.
macro_rules! query_target {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name { pub target: ObjectId }
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { target: ObjectId(r.u32()?) })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.target.0);
                Ok(())
            }
        }
    };
}

query_target!(
    /// `0x01C2 Character_QueryAge` (`/age`).
    CharacterQueryAge,
    CHARACTER_QUERY_AGE
);
query_target!(
    /// `0x01C4 Character_QueryBirth` (`/birth`). There is **no dedicated response**: the server
    /// answers with an ordinary `Communication_TextboxString`.
    CharacterQueryBirth,
    CHARACTER_QUERY_BIRTH
);

/// `0x01C3 Character_QueryAgeResponse` (S2C). An empty target name means "self".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterQueryAgeResponse {
    pub target_name: String,
    /// Formatted server-side in the `1mo 1d 1h 1m 1s` form.
    pub age: String,
}

impl Message for CharacterQueryAgeResponse {
    const OPCODE: Opcode = Opcode::CHARACTER_QUERY_AGE_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target_name: r.pstring()?,
            age: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.target_name)?;
        w.pstring(&self.age)
    }
}

empty_message!(
    /// `0x01E9 Character_RequestPing` (`/ping`) — measures the round trip in the *game-event* layer,
    /// not the transport's own echo.
    CharacterRequestPing,
    CHARACTER_REQUEST_PING
);
empty_message!(
    /// `0x01EA Character_ReturnPing`.
    CharacterReturnPing,
    CHARACTER_RETURN_PING
);

// ---------------------------------------------------------------------------------------------
// 3. Consent and permission lists
// ---------------------------------------------------------------------------------------------

empty_message!(
    /// `0x0216 Character_ClearPlayerConsentList` (`/consent clear`).
    CharacterClearPlayerConsentList,
    CHARACTER_CLEAR_PLAYER_CONSENT_LIST
);
empty_message!(
    /// `0x0217 Character_DisplayPlayerConsentList` (`/consent list`).
    CharacterDisplayPlayerConsentList,
    CHARACTER_DISPLAY_PLAYER_CONSENT_LIST
);
string_message!(
    /// `0x0218 Character_RemoveFromPlayerConsentList` (`/consent remove <name>`).
    CharacterRemoveFromPlayerConsentList,
    CHARACTER_REMOVE_FROM_PLAYER_CONSENT_LIST,
    name
);
string_message!(
    /// `0x0219 Character_AddPlayerPermission` (`/permit add <name>`).
    CharacterAddPlayerPermission,
    CHARACTER_ADD_PLAYER_PERMISSION,
    name
);
string_message!(
    /// `0x021A Character_RemovePlayerPermission` (`/permit remove <name>`).
    ///
    /// **`0x021A`, not `0x0220`.** The client writes `0x021A` and ACE's
    /// `GameActionType.RemovePlayerPermission` agrees; the catalogue's `0x0220` is a documentation
    /// error. See `docs/CORRECTIONS.md`.
    CharacterRemovePlayerPermission,
    CHARACTER_REMOVE_PLAYER_PERMISSION,
    name
);

/// `0x0140 Character_AbuseLogRequest` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterAbuseLogRequest {
    /// The reported character.
    pub target: String,
    /// Always 1.
    pub status: u32,
    pub complaint: String,
}

impl Message for CharacterAbuseLogRequest {
    const OPCODE: Opcode = Opcode::CHARACTER_ABUSE_LOG_REQUEST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target: r.pstring()?,
            status: r.u32()?,
            complaint: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.target)?;
        w.u32(self.status);
        w.pstring(&self.complaint)
    }
}

// ---------------------------------------------------------------------------------------------
// 5. The Decal plugin audit — all three carry payloads
// ---------------------------------------------------------------------------------------------

/// `0x02AE Admin_QueryPluginList` (S2C) — **carries a context dword**, which the catalogue omits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdminQueryPluginList {
    pub context: u32,
}

impl Message for AdminQueryPluginList {
    const OPCODE: Opcode = Opcode::ADMIN_QUERY_PLUGIN_LIST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { context: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.context);
        Ok(())
    }
}

/// `0x02AF Admin_QueryPluginListResponse` (C2S) — the context echoed back with the plugin list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminQueryPluginListResponse {
    pub context: u32,
    pub plugin_list: String,
}

impl Message for AdminQueryPluginListResponse {
    const OPCODE: Opcode = Opcode::ADMIN_QUERY_PLUGIN_LIST_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            context: r.u32()?,
            plugin_list: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.context);
        w.pstring(&self.plugin_list)
    }
}

/// `0x02B1 Admin_QueryPlugin` (S2C) — **a context dword and a name**, which the catalogue omits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminQueryPlugin {
    pub context: u32,
    pub plugin_name: String,
}

impl Message for AdminQueryPlugin {
    const OPCODE: Opcode = Opcode::ADMIN_QUERY_PLUGIN;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            context: r.u32()?,
            plugin_name: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.context);
        w.pstring(&self.plugin_name)
    }
}

/// `0x02B2 Admin_QueryPluginResponse` (C2S) — the client's own answer about one plugin.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminQueryPluginResponseSend {
    pub context: u32,
    pub success: i32,
    pub name: String,
    pub author: String,
    pub email: String,
    pub webpage: String,
}

impl Message for AdminQueryPluginResponseSend {
    const OPCODE: Opcode = Opcode::ADMIN_QUERY_PLUGIN_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            context: r.u32()?,
            success: r.i32()?,
            name: r.pstring()?,
            author: r.pstring()?,
            email: r.pstring()?,
            webpage: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.context);
        w.i32(self.success);
        w.pstring(&self.name)?;
        w.pstring(&self.author)?;
        w.pstring(&self.email)?;
        w.pstring(&self.webpage)
    }
}

/// `0x02B3 Admin_QueryPluginResponse` (S2C) — **five strings**.
///
/// The handler unpacks five
/// consecutive strings; the catalogue says the payload is empty. The handler is an empty
/// stub so the strings are dropped, but the cursor must still be advanced past all five.
/// The fifth has no name in the client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminQueryPluginResponseRecv {
    pub name: String,
    pub author: String,
    pub email: String,
    pub webpage: String,
    // the fifth string's meaning is unknown. It is
    // decoded and named `unknown5`; a capture from an admin session would settle it.
    pub unknown5: String,
}

impl Message for AdminQueryPluginResponseRecv {
    const OPCODE: Opcode = Opcode::ADMIN_QUERY_PLUGIN_RESPONSE_RECV;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            author: r.pstring()?,
            email: r.pstring()?,
            webpage: r.pstring()?,
            unknown5: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.pstring(&self.author)?;
        w.pstring(&self.email)?;
        w.pstring(&self.webpage)?;
        w.pstring(&self.unknown5)
    }
}

// ---------------------------------------------------------------------------------------------
// 6-7. Admin data dumps and Environs
// ---------------------------------------------------------------------------------------------

/// The admin account-data and player-data rows — the same shape.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminRow {
    pub name: String,
    pub bookie_id: u32,
}

/// `0xF7CA Admin_ReceiveAccountData` and `0xF7CB Admin_ReceivePlayerData` (S2C).
///
/// The client parses these and does nothing: the target method is an empty `return 0`
/// stub, and the GM panel that consumed them was removed before this build.
macro_rules! admin_data_dump {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub context: u32,
            pub rows: Vec<AdminRow>,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self {
                    context: r.u32()?,
                    rows: r.packed_list(|r| {
                        Ok(AdminRow { name: r.pstring()?, bookie_id: r.u32()? })
                    })?,
                })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.context);
                w.packed_list(&self.rows, |w, row| {
                    w.pstring(&row.name)?;
                    w.u32(row.bookie_id);
                    Ok(())
                })
            }
        }
    };
}

admin_data_dump!(
    /// `0xF7CA Admin_ReceiveAccountData`.
    AdminReceiveAccountData,
    ADMIN_RECEIVE_ACCOUNT_DATA
);
admin_data_dump!(
    /// `0xF7CB Admin_ReceivePlayerData`.
    AdminReceivePlayerData,
    ADMIN_RECEIVE_PLAYER_DATA
);

empty_message!(
    /// `0xF7CC Admin_SendAdminGetServerVersion` (C2S, **Control queue**) — four bytes, no
    /// `OrderedActionHeader`. The response arrives as text.
    AdminSendAdminGetServerVersion,
    ADMIN_SEND_ADMIN_GET_SERVER_VERSION
);

/// `0xF7D9 Admin_SendAdminRestoreCharacter` (C2S, **Control queue**).
///
/// The client always passes two empty strings, so in practice the
/// body is `[guid][""][""]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminSendAdminRestoreCharacter {
    pub iid: ObjectId,
    pub restored_char_name: String,
    pub account_to_restore_to: String,
}

impl Message for AdminSendAdminRestoreCharacter {
    const OPCODE: Opcode = Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            iid: ObjectId(r.u32()?),
            restored_char_name: r.pstring()?,
            account_to_restore_to: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.iid.0);
        w.pstring(&self.restored_char_name)?;
        w.pstring(&self.account_to_restore_to)
    }
}

/// `0xEA60 Admin_Environs` (S2C) — the GM "set the mood" message, one dword.
///
/// Values 0..=6 and 9999 set the landscape override; **101–124 (`0x65`–`0x7C`) play a one-shot UI
/// sound instead** and leave the landscape alone; anything else is ignored. The table is in
/// `docs/networking/messages/12-admin-and-misc.md` §4 and belongs to the renderer, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdminEnvirons {
    pub environ_option: i32,
}

impl AdminEnvirons {
    /// Whether this value plays a sound rather than changing the landscape.
    #[must_use]
    pub fn is_sound_cue(option: i32) -> bool {
        (0x65..=0x7C).contains(&option)
    }
}

impl Message for AdminEnvirons {
    const OPCODE: Opcode = Opcode::ADMIN_ENVIRONS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            environ_option: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.environ_option);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// DDD — the dat-patching set, on queue 5
// ---------------------------------------------------------------------------------------------

/// A mostly-consecutive integer set — `int32 iterations` then a run-length list of `int32`s read until
/// the accumulated count reaches it.
///
/// A positive value counts one iteration; a negative value `x` counts `|x| - 1`. Cross-checked
/// against ACE's `ReadCMostlyConsecutiveIntSet`, which is the only reader of this structure that
/// exists outside the client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MostlyConsecutiveIntSet {
    pub iterations: i32,
    pub ints: Vec<i32>,
}

impl MostlyConsecutiveIntSet {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let iterations = r.i32()?;
        let mut ints = Vec::new();
        let mut counted = 0i32;
        while counted != iterations {
            let x = r.i32()?;
            counted = counted.wrapping_add(if x < 0 {
                x.wrapping_abs().wrapping_sub(1)
            } else {
                1
            });
            ints.push(x);
            if ints.len() * 4 > r.remaining() + ints.len() * 4 {
                break;
            }
            if r.remaining() == 0 && counted != iterations {
                return Err(MessageError::UnexpectedEof {
                    at: r.blob_offset(),
                    needed: 4,
                    available: 0,
                });
            }
        }
        Ok(Self { iterations, ints })
    }

    fn write(&self, w: &mut Writer) {
        w.i32(self.iterations);
        for v in &self.ints {
            w.i32(*v);
        }
    }
}

/// `PTaggedIterationList` — the **type dword first**, then the id dword, then the set.
///
/// Types: 0 portal, 1 cell and language, `HiFi` highres. Ids: 1 portal and highres, 2 cell,
/// 3 language. ACE's own switch on the *second* dword being 1/2/3 settles the order of the two
/// halves of the `u64`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaggedIterationList {
    pub dat_file_type: u32,
    pub dat_file_id: u32,
    pub iterations: MostlyConsecutiveIntSet,
}

impl TaggedIterationList {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            dat_file_type: r.u32()?,
            dat_file_id: r.u32()?,
            iterations: MostlyConsecutiveIntSet::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.dat_file_type);
        w.u32(self.dat_file_id);
        self.iterations.write(w);
    }
}

/// `0xF7E5 DDD_InterrogationMessage` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DddInterrogation {
    pub servers_region: u32,
    pub name_rule_language: u32,
    /// Bit `0x4` asks the client to open `client_highres.dat`.
    pub product_id: u32,
    pub supported_languages: Vec<u32>,
}

impl DddInterrogation {
    /// Product-id bit `4` makes the client load the high-resolution dat on interrogation.
    pub const PRODUCT_HIGHRES: u32 = 0x4;
}

impl Message for DddInterrogation {
    const OPCODE: Opcode = Opcode::DDD_INTERROGATION_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            servers_region: r.u32()?,
            name_rule_language: r.u32()?,
            product_id: r.u32()?,
            supported_languages: r.packed_list(Reader::u32)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.servers_region);
        w.u32(self.name_rule_language);
        w.u32(self.product_id);
        w.packed_list(&self.supported_languages, |w, v| {
            w.u32(*v);
            Ok(())
        })
    }
}

/// `0xF7E6 DDD_InterrogationResponseMessage` (C2S).
///
/// `iters_without_keys` is a second iteration list that is empty in every observed session;
/// ACE reads only the first and then skips the rest.
///
/// **The overlay extension** (this client's and Empyrean's alone). The retail client sends
/// `flags` as 0, and a server reads nothing after the first list. A client that keeps a world's
/// records in an overlay over its locked files sets [`Self::FLAG_OVERLAY`] and follows the flags
/// with the base files it holds ([`OverlayBase`]); a server that knows the extension answers it
/// with a [`DddOverlayManifest`] and the world's cell records in its patch, and any other server
/// reads the message as a retail one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DddInterrogationResponse {
    pub client_language: u32,
    pub iters_with_keys: Vec<TaggedIterationList>,
    pub iters_without_keys: Vec<TaggedIterationList>,
    pub flags: u32,
    /// The base files the client holds, sent only with [`Self::FLAG_OVERLAY`].
    pub overlay_bases: Vec<OverlayBase>,
}

impl DddInterrogationResponse {
    /// The client keeps the world's records in an overlay and takes the overlay manifest.
    pub const FLAG_OVERLAY: u32 = 0x0000_0001;
}

/// One base file a client holds, by the file's wire pair and its fingerprint (the SHA-256 of its
/// header and directory).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayBase {
    pub dat_file_type: u32,
    pub dat_file_id: u32,
    pub fingerprint: [u8; 32],
}

fn read_hash(r: &mut Reader<'_>) -> Result<[u8; 32], MessageError> {
    let mut h = [0u8; 32];
    h.copy_from_slice(r.bytes(32)?);
    Ok(h)
}

impl Message for DddInterrogationResponse {
    const OPCODE: Opcode = Opcode::DDD_INTERROGATION_RESPONSE_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let client_language = r.u32()?;
        let iters_with_keys = r.packed_list(TaggedIterationList::read)?;
        let iters_without_keys = r.packed_list(TaggedIterationList::read)?;
        let flags = r.u32()?;
        let overlay_bases = if flags & Self::FLAG_OVERLAY != 0 {
            r.packed_list(|r| {
                Ok(OverlayBase {
                    dat_file_type: r.u32()?,
                    dat_file_id: r.u32()?,
                    fingerprint: read_hash(r)?,
                })
            })?
        } else {
            Vec::new()
        };
        Ok(Self {
            client_language,
            iters_with_keys,
            iters_without_keys,
            flags,
            overlay_bases,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.client_language);
        w.packed_list(&self.iters_with_keys, |w, l| {
            l.write(w);
            Ok(())
        })?;
        w.packed_list(&self.iters_without_keys, |w, l| {
            l.write(w);
            Ok(())
        })?;
        w.u32(self.flags);
        if self.flags & Self::FLAG_OVERLAY != 0 {
            w.packed_list(&self.overlay_bases, |w, b| {
                w.u32(b.dat_file_type);
                w.u32(b.dat_file_id);
                w.bytes(&b.fingerprint);
                Ok(())
            })?;
        }
        Ok(())
    }
}

/// One revision block of a [`DddBeginDdd`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatchRevision {
    pub dat_file_type: u32,
    pub dat_file_id: u32,
    pub iteration: u32,
    pub ids_to_download: Vec<u32>,
    pub ids_to_purge: Vec<u32>,
}

/// `0xF7E7 DDD_BeginDDDMessage` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DddBeginDdd {
    /// Total bytes of patch to expect. The client subtracts the records it already received during
    /// interrogation before reporting the figure to the UI.
    pub data_expected: u32,
    pub revisions: Vec<PatchRevision>,
}

impl Message for DddBeginDdd {
    const OPCODE: Opcode = Opcode::DDD_BEGIN_DDDMESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let data_expected = r.u32()?;
        let revisions = r.packed_list(|r| {
            Ok(PatchRevision {
                dat_file_type: r.u32()?,
                dat_file_id: r.u32()?,
                iteration: r.u32()?,
                ids_to_download: r.packed_list(Reader::u32)?,
                ids_to_purge: r.packed_list(Reader::u32)?,
            })
        })?;
        Ok(Self {
            data_expected,
            revisions,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.data_expected);
        w.packed_list(&self.revisions, |w, m| {
            w.u32(m.dat_file_type);
            w.u32(m.dat_file_id);
            w.u32(m.iteration);
            w.packed_list(&m.ids_to_download, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
            w.packed_list(&m.ids_to_purge, |w, v| {
                w.u32(*v);
                Ok(())
            })
        })
    }
}

/// `0xF7E2 DDD_DataMessage` (S2C).
///
/// The client reads: the `u64` dat-file id (type dword then id dword), the `QualifiedDataID`
/// (`Type` then `ID`, 8-aligned), the iteration, the compressed flag, then a version dword, a size dword and the payload.
///
// the archive's object serialiser is the helper that
// serialises the compressed flag and its width was not identified; ACE writes a **one-byte** bool there
// and the archive is not word-aligned, so a byte is consistent with both. Everything downstream of
// this field shifts if it is really a dword. ACE's DDD patching is off by default, so no local test
// exercises it — a capture from a server with `DDD.EnableDATPatching` on would settle it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DddData {
    pub dat_file_type: u32,
    pub dat_file_id: u32,
    /// The qualified data id's type.
    pub resource_type: u32,
    /// The qualified data id's id.
    pub resource_id: u32,
    pub iteration: u32,
    /// One byte; see the note above.
    pub compressed: u8,
    pub version: u32,
    /// The declared size, which ACE writes as `payload.len() + 4`.
    pub data_size: u32,
    pub data: Vec<u8>,
}

impl Message for DddData {
    const OPCODE: Opcode = Opcode::DDD_DATA_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            dat_file_type: r.u32()?,
            dat_file_id: r.u32()?,
            resource_type: r.u32()?,
            resource_id: r.u32()?,
            iteration: r.u32()?,
            compressed: r.u8()?,
            version: r.u32()?,
            data_size: r.u32()?,
            data: r.rest().to_vec(),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.dat_file_type);
        w.u32(self.dat_file_id);
        w.u32(self.resource_type);
        w.u32(self.resource_id);
        w.u32(self.iteration);
        w.u8(self.compressed);
        w.u32(self.version);
        w.u32(self.data_size);
        w.bytes(&self.data);
        Ok(())
    }
}

/// `0xF7E3 DDD_RequestDataMessage` (C2S) — one `QualifiedDataID`, 8 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DddRequestData {
    pub resource_type: u32,
    pub resource_id: u32,
}

impl Message for DddRequestData {
    const OPCODE: Opcode = Opcode::DDD_REQUEST_DATA_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            resource_type: r.u32()?,
            resource_id: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.resource_type);
        w.u32(self.resource_id);
        Ok(())
    }
}

/// `0xF7E4 DDD_ErrorMessage` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DddError {
    pub resource_type: u32,
    pub resource_id: u32,
    pub error: u32,
}

impl Message for DddError {
    const OPCODE: Opcode = Opcode::DDD_ERROR_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            resource_type: r.u32()?,
            resource_id: r.u32()?,
            error: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.resource_type);
        w.u32(self.resource_id);
        w.u32(self.error);
        Ok(())
    }
}

/// One record of a world's overlay, as a [`DddOverlayManifest`] lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayRecord {
    pub id: u32,
    /// The revision that brought it.
    pub iteration: u32,
    /// Its size once inflated.
    pub size: u32,
    /// The SHA-256 of its inflated bytes.
    pub sha256: [u8; 32],
}

/// One deletion of a world's overlay: `id` alone when `mask` is 0, else every id whose bits under
/// `mask` are `id`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlayTombstone {
    pub id: u32,
    pub mask: u32,
    /// The revision that deleted it.
    pub iteration: u32,
}

/// One file's part of a [`DddOverlayManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayFileManifest {
    /// The file's wire pair, as [`TaggedIterationList`] names it.
    pub dat_file_type: u32,
    pub dat_file_id: u32,
    /// The base file the overlay was made against: its name, fingerprint and iteration count.
    pub base_name: String,
    pub base_fingerprint: [u8; 32],
    pub base_iterations: u32,
    /// The overlay's revisions.
    pub revisions: Vec<u32>,
    pub records: Vec<OverlayRecord>,
    pub tombstones: Vec<OverlayTombstone>,
}

/// `0xF7EC DDD_OverlayManifestMessage` (S2C), on queue 5: the overlay extension's own message,
/// outside the retail client's `0xF7E2..=0xF7EB` dispatch, sent only to a client that set
/// [`DddInterrogationResponse::FLAG_OVERLAY`], before its `0xF7E7`. It names the world, the base
/// each file's overlay was made against, and every record and deletion the overlay holds, so the
/// client can refuse an overlay made against another base and check every record it is sent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DddOverlayManifest {
    /// The world the overlay belongs to, as the world names itself.
    pub world_key: String,
    /// The bytes the patch will send.
    pub total_bytes: u32,
    pub files: Vec<OverlayFileManifest>,
}

impl Message for DddOverlayManifest {
    const OPCODE: Opcode = Opcode::DDD_OVERLAY_MANIFEST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let world_key = r.pstring()?;
        let total_bytes = r.u32()?;
        let files = r.packed_list(|r| {
            Ok(OverlayFileManifest {
                dat_file_type: r.u32()?,
                dat_file_id: r.u32()?,
                base_name: r.pstring()?,
                base_fingerprint: read_hash(r)?,
                base_iterations: r.u32()?,
                revisions: r.packed_list(Reader::u32)?,
                records: r.packed_list(|r| {
                    Ok(OverlayRecord {
                        id: r.u32()?,
                        iteration: r.u32()?,
                        size: r.u32()?,
                        sha256: read_hash(r)?,
                    })
                })?,
                tombstones: r.packed_list(|r| {
                    Ok(OverlayTombstone {
                        id: r.u32()?,
                        mask: r.u32()?,
                        iteration: r.u32()?,
                    })
                })?,
            })
        })?;
        Ok(Self {
            world_key,
            total_bytes,
            files,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.world_key)?;
        w.u32(self.total_bytes);
        w.packed_list(&self.files, |w, f| {
            w.u32(f.dat_file_type);
            w.u32(f.dat_file_id);
            w.pstring(&f.base_name)?;
            w.bytes(&f.base_fingerprint);
            w.u32(f.base_iterations);
            w.packed_list(&f.revisions, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
            w.packed_list(&f.records, |w, x| {
                w.u32(x.id);
                w.u32(x.iteration);
                w.u32(x.size);
                w.bytes(&x.sha256);
                Ok(())
            })?;
            w.packed_list(&f.tombstones, |w, t| {
                w.u32(t.id);
                w.u32(t.mask);
                w.u32(t.iteration);
                Ok(())
            })
        })
    }
}

empty_message!(
    /// `0xF7EA DDD_OnEndDDD` — **the end-of-patching message, in both directions**.
    ///
    /// The client both receives it through the end-DDD handler and sends it back with
    /// type `0xF7EA`.
    DddEndDdd,
    DDD_ON_END_DDD
);

empty_message!(
    /// `0xF7EB DDD_EndDDDMessage` — **received only**, and it does *not* mean "end".
    ///
    /// The client turns it into a `DDD_PatchtimePending` DDD event: "patching pending, wait".
    /// It has no sender. The community catalogue's name for this opcode is the source of the whole
    /// confusion; see the module docs.
    DddPatchtimePending,
    DDD_END_DDDMESSAGE
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{round_trip, write_body};

    /// A row in
    /// `docs/CORRECTIONS.md`: all three `Admin_QueryPlugin*` messages
    /// carry payloads, and `0x02B3` unpacks five strings.
    #[test]
    fn the_query_plugin_messages_carry_payloads() {
        assert!(!write_body(&AdminQueryPluginList { context: 7 })
            .unwrap()
            .is_empty());
        assert!(!write_body(&AdminQueryPlugin {
            context: 7,
            plugin_name: "Decal".into()
        })
        .unwrap()
        .is_empty());

        let five = AdminQueryPluginResponseRecv {
            name: "Decal".into(),
            author: "A".into(),
            email: "a@example.com".into(),
            webpage: "http://example.com".into(),
            unknown5: "?".into(),
        };
        let bytes = write_body(&five).unwrap();
        let back: AdminQueryPluginResponseRecv = round_trip(&bytes);
        assert_eq!(back.unknown5, "?");

        // Reading only four strings would leave the fifth on the wire, which `expect_exhausted`
        // rejects — that is why the cursor must be advanced past all five.
        let mut r = Reader::body(&bytes);
        for _ in 0..4 {
            r.pstring().unwrap();
        }
        assert!(r.expect_exhausted().is_err());
    }

    /// The ddd end message is f7ea both ways.
    #[test]
    fn the_ddd_end_message_is_f7ea_both_ways() {
        assert_eq!(DddEndDdd::OPCODE.0, 0xF7EA);
        assert_eq!(DddPatchtimePending::OPCODE.0, 0xF7EB);
        assert!(write_body(&DddEndDdd).unwrap().is_empty());
        assert!(write_body(&DddPatchtimePending).unwrap().is_empty());
        // Both live on the database queue.
        for op in [DddEndDdd::OPCODE, DddPatchtimePending::OPCODE] {
            assert_eq!(
                op.info().unwrap().recv_queue,
                Some(dereth_primitives::NetQueue::ClientCache)
            );
        }
    }

    /// Oracle: ACE's `ReadCMostlyConsecutiveIntSet`, the only reader of this structure outside the
    /// client, cross-checked against the server-interrogation handler's iteration-list load.
    #[test]
    fn the_iteration_set_is_run_length_encoded() {
        // Three single iterations.
        let s = MostlyConsecutiveIntSet {
            iterations: 3,
            ints: vec![1, 2, 3],
        };
        let mut w = Writer::body();
        s.write(&mut w);
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(MostlyConsecutiveIntSet::read(&mut r).unwrap(), s);
        r.expect_exhausted().unwrap();

        // A negative value is a run: -5 accounts for four iterations, then one more single.
        let s = MostlyConsecutiveIntSet {
            iterations: 5,
            ints: vec![-5, 9],
        };
        let mut w = Writer::body();
        s.write(&mut w);
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(MostlyConsecutiveIntSet::read(&mut r).unwrap(), s);
        r.expect_exhausted().unwrap();
    }

    /// The type dword comes **first**. Oracle: ACE's `ReadPTaggedIterationList` and
    /// `GameMessageDDDDataMessage`, whose own switch on the second dword being 1/2/3 settles which
    /// half is which. An earlier reading had it the other way round.
    #[test]
    fn the_tagged_iteration_list_puts_the_type_first() {
        let l = TaggedIterationList {
            dat_file_type: 0,
            dat_file_id: 1, // portal
            iterations: MostlyConsecutiveIntSet {
                iterations: 0,
                ints: vec![],
            },
        };
        let mut w = Writer::body();
        l.write(&mut w);
        let bytes = w.into_inner();
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0,
            "type"
        );
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1, "id");
        let mut r = Reader::body(&bytes);
        assert_eq!(TaggedIterationList::read(&mut r).unwrap(), l);
        r.expect_exhausted().unwrap();
    }

    /// The interrogation exchange, which is the whole of DDD against ACE.
    #[test]
    fn the_interrogation_exchange_round_trips() {
        let ask = DddInterrogation {
            servers_region: 1,
            name_rule_language: 1,
            product_id: 1,
            supported_languages: vec![0, 1],
        };
        let bytes = write_body(&ask).unwrap();
        assert_eq!(bytes.len(), 24, "ACE budgets 28 bytes including the opcode");
        let _: DddInterrogation = round_trip(&bytes);

        let answer = DddInterrogationResponse {
            client_language: 1,
            iters_with_keys: vec![TaggedIterationList {
                dat_file_type: 0,
                dat_file_id: 1,
                iterations: MostlyConsecutiveIntSet {
                    iterations: 0,
                    ints: vec![],
                },
            }],
            iters_without_keys: vec![],
            flags: 0,
            overlay_bases: vec![],
        };
        let retail = write_body(&answer).unwrap();
        let _: DddInterrogationResponse = round_trip(&retail);

        // The overlay extension: the flag, then the bases the client holds after it. A retail
        // reader (ACE's, which reads the first list and nothing after it) sees the same leading
        // bytes.
        let extended = DddInterrogationResponse {
            flags: DddInterrogationResponse::FLAG_OVERLAY,
            overlay_bases: vec![OverlayBase {
                dat_file_type: 0,
                dat_file_id: 1,
                fingerprint: [9; 32],
            }],
            ..answer
        };
        let bytes = write_body(&extended).unwrap();
        assert_eq!(bytes[..retail.len() - 4], retail[..retail.len() - 4]);
        let back: DddInterrogationResponse = round_trip(&bytes);
        assert_eq!(back, extended);
    }

    /// Behaviour: none (the overlay manifest's own round trip)
    #[test]
    fn the_overlay_manifest_round_trips() {
        let m = DddOverlayManifest {
            world_key: "a world".into(),
            total_bytes: 1234,
            files: vec![OverlayFileManifest {
                dat_file_type: 1,
                dat_file_id: 2,
                base_name: "client_cell_1.dat".into(),
                base_fingerprint: [3; 32],
                base_iterations: 982,
                revisions: vec![983],
                records: vec![OverlayRecord {
                    id: 0xA9B4_0100,
                    iteration: 983,
                    size: 40,
                    sha256: [4; 32],
                }],
                tombstones: vec![OverlayTombstone {
                    id: 0xA9B4_0101,
                    mask: 0,
                    iteration: 983,
                }],
            }],
        };
        let back: DddOverlayManifest = round_trip(&write_body(&m).unwrap());
        assert_eq!(back, m);
        assert_eq!(DddOverlayManifest::OPCODE.0, 0xF7EC);
    }

    #[test]
    fn the_rest_of_the_ddd_set_round_trips() {
        let _: DddBeginDdd = round_trip(
            &write_body(&DddBeginDdd {
                data_expected: 1024,
                revisions: vec![PatchRevision {
                    dat_file_type: 0,
                    dat_file_id: 1,
                    iteration: 5,
                    ids_to_download: vec![0x0100_0001],
                    ids_to_purge: vec![],
                }],
            })
            .unwrap(),
        );
        let _: DddData = round_trip(
            &write_body(&DddData {
                dat_file_type: 0,
                dat_file_id: 1,
                resource_type: 1,
                resource_id: 0x0100_0001,
                iteration: 5,
                compressed: 1,
                version: 3,
                data_size: 8,
                data: vec![1, 2, 3, 4],
            })
            .unwrap(),
        );
        let _: DddRequestData = round_trip(
            &write_body(&DddRequestData {
                resource_type: 1,
                resource_id: 2,
            })
            .unwrap(),
        );
        let _: DddError = round_trip(
            &write_body(&DddError {
                resource_type: 1,
                resource_id: 2,
                error: 3,
            })
            .unwrap(),
        );
    }

    #[test]
    fn the_admin_and_progression_messages_round_trip() {
        let _: TrainSkill = round_trip(
            &write_body(&TrainSkill {
                skill_id: 22,
                xp_spent: 1000,
            })
            .unwrap(),
        );
        let _: TrainAttribute = round_trip(
            &write_body(&TrainAttribute {
                attribute_id: 1,
                xp_spent: 10,
            })
            .unwrap(),
        );
        let _: TrainAttribute2nd = round_trip(
            &write_body(&TrainAttribute2nd {
                vital_id: 1,
                xp_spent: 10,
            })
            .unwrap(),
        );
        let _: TrainSkillAdvancementClass = round_trip(
            &write_body(&TrainSkillAdvancementClass {
                skill_id: 22,
                credits_spent: 4,
            })
            .unwrap(),
        );
        let _: CharacterQueryAge = round_trip(
            &write_body(&CharacterQueryAge {
                target: ObjectId(0),
            })
            .unwrap(),
        );
        let _: CharacterQueryAgeResponse = round_trip(
            &write_body(&CharacterQueryAgeResponse {
                target_name: String::new(),
                age: "1mo 2d".into(),
            })
            .unwrap(),
        );
        assert!(write_body(&CharacterRequestPing).unwrap().is_empty());
        let _: CharacterRemovePlayerPermission = round_trip(
            &write_body(&CharacterRemovePlayerPermission { name: "Bob".into() }).unwrap(),
        );
        let _: CharacterAbuseLogRequest = round_trip(
            &write_body(&CharacterAbuseLogRequest {
                target: "Bob".into(),
                status: 1,
                complaint: "spam".into(),
            })
            .unwrap(),
        );
        let _: AdminQueryPluginListResponse = round_trip(
            &write_body(&AdminQueryPluginListResponse {
                context: 1,
                plugin_list: "Decal".into(),
            })
            .unwrap(),
        );
        let _: AdminQueryPluginResponseSend = round_trip(
            &write_body(&AdminQueryPluginResponseSend {
                context: 1,
                success: 1,
                name: "Decal".into(),
                author: "A".into(),
                email: "a@b".into(),
                webpage: "c".into(),
            })
            .unwrap(),
        );
        let _: AdminReceiveAccountData = round_trip(
            &write_body(&AdminReceiveAccountData {
                context: 0,
                rows: vec![AdminRow {
                    name: "bob".into(),
                    bookie_id: 1,
                }],
            })
            .unwrap(),
        );
        let _: AdminSendAdminRestoreCharacter = round_trip(
            &write_body(&AdminSendAdminRestoreCharacter {
                iid: ObjectId(1),
                restored_char_name: String::new(),
                account_to_restore_to: String::new(),
            })
            .unwrap(),
        );
        let _: AdminEnvirons =
            round_trip(&write_body(&AdminEnvirons { environ_option: 1 }).unwrap());
        assert!(AdminEnvirons::is_sound_cue(0x65));
        assert!(AdminEnvirons::is_sound_cue(0x7C));
        assert!(!AdminEnvirons::is_sound_cue(6));
        assert!(!AdminEnvirons::is_sound_cue(9999));
    }
}
