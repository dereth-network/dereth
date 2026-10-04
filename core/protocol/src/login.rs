//! Family: login and character management —
//! `docs/networking/messages/01-login-and-character.md`.
//!
//! Everything from "the transport is connected" to "the player is standing in the world". The client
//! sends its requests on the **Logon queue (4)** through the login transport; answers arrive on the **UI
//! queue (9)**, which is why the login queue's own consumer discards everything but `0xF7DE`.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::property::PackObjPropertyCollection;
use crate::types::qualities::{AcQualities, CreationProfile};
use crate::types::{ContentProfile, InventoryPlacement};
use crate::Message;
use dereth_primitives::ObjectId;

// ---------------------------------------------------------------------------------------------
// 0xF658 Login_LoginCharacterSet
// ---------------------------------------------------------------------------------------------

/// One entry of the character list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterIdentity {
    pub gid: ObjectId,
    pub name: String,
    /// 0 = not being deleted; otherwise a countdown in seconds.
    pub seconds_greyed_out: u32,
}

impl CharacterIdentity {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = Self {
            gid: ObjectId(r.u32()?),
            name: r.pstring()?,
            seconds_greyed_out: r.u32()?,
        };
        r.align4()?;
        Ok(id)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.gid.0);
        w.pstring(&self.name)?;
        w.u32(self.seconds_greyed_out);
        w.align4();
        Ok(())
    }
}

/// `0xF658 Login_LoginCharacterSet`.
///
/// The first thing the world server sends after the session authenticates.
/// The login handler rewrites `status == 0xFFFFFFFF` to
/// `0x14`; that is a handler behaviour, not a wire one, so it is not done here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginCharacterSet {
    pub status: u32,
    pub characters: Vec<CharacterIdentity>,
    /// Greyed-out slots pending deletion.
    pub deleted: Vec<CharacterIdentity>,
    /// Slots this account may use; the client's own default is 5.
    pub num_allowed_characters: i32,
    pub account: String,
    pub use_turbine_chat: u32,
    pub has_throne_of_destiny: u32,
}

impl LoginCharacterSet {
    /// Adding an identity to the character set.
    ///
    /// If a row with the same gid is already present nothing is added and it returns false;
    /// otherwise the identity is appended and it returns true.
    ///
    /// The dedupe is by **gid only** — not by name — and it is the reason a repeated
    /// `0xF643` cannot double-add. The set is appended to, so the new character lands at the
    /// **end** of the list rather than in a free slot.
    ///
    /// Its one caller is the char-gen verification response's
    /// `CG_VERIFICATION_RESPONSE_OK` arm, whose return value decides whether
    /// the character-set notice is raised — so the bool is load-bearing and is
    /// returned here for the same reason.
    pub fn add_identity(&mut self, id: &CharacterIdentity) -> bool {
        if self.characters.iter().any(|c| c.gid == id.gid) {
            return false;
        }
        self.characters.push(id.clone());
        true
    }

    /// Fetching an identity from the set and unpacking it over the
    /// row it returns — **the restore arm** of the char-gen verification response.
    ///
    /// It looks up the identity at the char-gen state's slot and, if there is one, unpacks the
    /// rest of the payload over it.
    ///
    /// A **replace in place**, not an append: ACE's `GameMessageCharacterRestore` re-states the
    /// same `gid` with `secondsDisabled = 0`, so overwriting the row is what takes the character
    /// out of the pending-delete state — [`add_identity`](Self::add_identity) would refuse it on
    /// the `gid` dedupe and change nothing.
    ///
    /// The bool is load-bearing for the same reason `add_identity`'s is: it decides whether
    /// the character-set notice is raised, and that notice is the **only** thing
    /// that closes the please-wait modal a restore puts up.
    ///
    /// The retail lookup finds nothing on three conditions and all three are here: the slot is
    /// past the row count, the slot is negative (retail compares it unsigned, so `-1` becomes
    /// `0xFFFFFFFF` and fails the same test), or the row it names is **empty** — its gid is 0.
    /// Nothing is written and the notice is not raised.
    #[must_use]
    pub fn replace_identity(&mut self, slot: i32, id: &CharacterIdentity) -> bool {
        let Ok(slot) = usize::try_from(slot) else {
            return false;
        };
        let Some(row) = self.characters.get_mut(slot) else {
            return false;
        };
        if row.gid.0 == 0 {
            return false;
        }
        row.clone_from(id);
        true
    }
}

impl Message for LoginCharacterSet {
    const OPCODE: Opcode = Opcode::LOGIN_LOGIN_CHARACTER_SET;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            status: r.u32()?,
            characters: r.packed_list(CharacterIdentity::read)?,
            deleted: r.packed_list(CharacterIdentity::read)?,
            num_allowed_characters: r.i32()?,
            account: r.pstring()?,
            use_turbine_chat: r.u32()?,
            has_throne_of_destiny: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.status);
        w.packed_list(&self.characters, |w, c| c.write(w))?;
        w.packed_list(&self.deleted, |w, c| c.write(w))?;
        w.i32(self.num_allowed_characters);
        w.pstring(&self.account)?;
        w.u32(self.use_turbine_chat);
        w.u32(self.has_throne_of_destiny);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The small ones
// ---------------------------------------------------------------------------------------------

/// `0xF7E1 Login_WorldInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginWorldInfo {
    pub connections: i32,
    pub max_connections: i32,
    pub world_name: String,
}

impl Message for LoginWorldInfo {
    const OPCODE: Opcode = Opcode::LOGIN_WORLD_INFO;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            connections: r.i32()?,
            max_connections: r.i32()?,
            world_name: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.connections);
        w.i32(self.max_connections);
        w.pstring(&self.world_name)
    }
}

/// A message whose body is empty — the opcode dword is the whole blob.
macro_rules! empty_message {
    ($(#[$m:meta])* $name:ident, $op:expr) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl Message for $name {
            const OPCODE: Opcode = $op;

            fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self)
            }

            fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
                Ok(())
            }
        }
    };
}

empty_message!(
    /// `0xF7C8 Login_SendEnterWorldRequest` (C2S).
    /// A bare 4-byte body on the Logon queue; the first half of the two-step enter-world exchange.
    LoginSendEnterWorldRequest,
    Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST
);

empty_message!(
    /// `0xF7DF Login_EnterGame_ServerReady` (S2C) — the server's answer to `0xF7C8`. Losing it
    /// leaves the retail client stuck at "connecting" with no timeout of its own; the 110 s
    /// `ServerDied` timer keys off `0x0013`, not this.
    LoginEnterGameServerReady,
    Opcode::LOGIN_ENTER_GAME_SERVER_READY
);

empty_message!(
    /// `0x00A1 Character_LoginCompleteNotification` (C2S) — an ordered game action with no body.
    ///
    /// The client only sends it once `player_desc_received` is set, the player object exists and
    /// every id in the two `ContentProfile` lists resolves to a game object
    /// (the login-complete gate). The reference server consumes this notification as the signal
    /// that it may complete the player's login.
    CharacterLoginCompleteNotification,
    Opcode::CHARACTER_LOGIN_COMPLETE_NOTIFICATION
);

empty_message!(
    /// `0xF653 Login_ExecuteLogOff`, **server to client**: the body is only the opcode.
    /// See [`LoginExecuteLogOffRequest`] for the client's side, which carries the character id.
    LoginExecuteLogOff,
    Opcode::LOGIN_EXECUTE_LOG_OFF
);

empty_message!(
    /// `0xF655 Character_CharacterDelete`, **server to client**: the body is only the opcode.
    CharacterDeleteAck,
    Opcode::CHARACTER_CHARACTER_DELETE
);

/// `0xF653 Login_ExecuteLogOff`, **client to server** —,
/// Logon queue. Same opcode as [`LoginExecuteLogOff`], different body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoginExecuteLogOffRequest {
    pub character: ObjectId,
}

impl Message for LoginExecuteLogOffRequest {
    const OPCODE: Opcode = Opcode::LOGIN_EXECUTE_LOG_OFF;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            character: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.character.0);
        Ok(())
    }
}

/// `0xF657 Login_SendEnterWorld` (C2S) —, Logon queue.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginSendEnterWorld {
    pub character: ObjectId,
    pub account: String,
}

impl Message for LoginSendEnterWorld {
    const OPCODE: Opcode = Opcode::LOGIN_SEND_ENTER_WORLD;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            character: ObjectId(r.u32()?),
            account: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.character.0);
        w.pstring(&self.account)
    }
}

/// `0xF655 Character_CharacterDelete` (C2S).
///
/// Note that it identifies the character by its **slot index**, not by its id;
/// the client looks the slot up by character id
/// and refuses when that is -1.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterDeleteRequest {
    pub account: String,
    pub slot_index: i32,
}

impl Message for CharacterDeleteRequest {
    const OPCODE: Opcode = Opcode::CHARACTER_CHARACTER_DELETE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            account: r.pstring()?,
            slot_index: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.account)?;
        w.i32(self.slot_index);
        Ok(())
    }
}

/// `0xF659 Character_CharacterError` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterError {
    pub char_error: u32,
}

impl Message for CharacterError {
    const OPCODE: Opcode = Opcode::CHARACTER_CHARACTER_ERROR;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            char_error: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.char_error);
        Ok(())
    }
}

/// `0xF65A Login_CharacterScreenMessage` (S2C): the character screen's message, two strings the
/// box shows one after the other.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginCharacterScreenMessage {
    pub text: String,
    pub more: String,
}

impl LoginCharacterScreenMessage {
    /// The whole message as the box shows it: the first string, then the second on a line of its
    /// own when there is one.
    #[must_use]
    pub fn shown(&self) -> String {
        if self.more.is_empty() {
            self.text.clone()
        } else {
            format!("{}\n{}", self.text, self.more)
        }
    }
}

impl Message for LoginCharacterScreenMessage {
    const OPCODE: Opcode = Opcode::LOGIN_CHARACTER_SCREEN_MESSAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            text: r.pstring()?,
            more: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.text)?;
        w.pstring(&self.more)
    }
}

/// The retail character-error enum, with the `ID_CHAR_ERROR_*` string token each value resolves
/// to in the UI flow's character-error notice.
///
/// The English text lives in string table `0x10000002` of `client_local_English.dat` and is **not**
/// hard-coded here: it is resolved through the string table by id hash.
/// Three values (`2 LOGGED_ON`, `7 NO_PREMADE`, `22 CHARACTER_IS_BOOTED`) have no case in the
/// client, and `0 UNDEF` falls through to `default:` and displays nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharErrorCode(pub u32);

impl CharErrorCode {
    /// The string token, or `None` where the client has no case for the value.
    #[must_use]
    pub fn token(self) -> Option<&'static str> {
        Some(match self.0 {
            1 => "ID_CHAR_ERROR_LOGON",
            3 => "ID_CHAR_ERROR_ACCOUNT_LOGON",
            // 8 (ACCOUNT_IN_USE) deliberately shares 4's token in the client.
            4 | 8 => "ID_CHAR_ERROR_SERVER_CRASH",
            5 => "ID_CHAR_ERROR_LOGOFF",
            6 => "ID_CHAR_ERROR_DELETE",
            9 => "ID_CHAR_ERROR_ACCOUNT_INVALID",
            10 => "ID_CHAR_ERROR_ACCOUNT_DOESNT_EXIST",
            11 => "ID_CHAR_ERROR_ENTER_GAME_GENERIC",
            12 => "ID_CHAR_ERROR_ENTER_GAME_STRESS_ACCOUNT",
            13 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD",
            14 => "ID_CHAR_ERROR_ENTER_GAME_PLAYER_ACCOUNT_MISSING",
            15 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_NOT_OWNED",
            16 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD_SERVER",
            17 => "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER",
            18 => "ID_CHAR_ERROR_ENTER_GAME_CORRUPT_CHARACTER",
            19 => "ID_CHAR_ERROR_ENTER_GAME_START_SERVER_DOWN",
            20 => "ID_CHAR_ERROR_ENTER_GAME_COULDNT_PLACE_CHARACTER",
            21 => "ID_CHAR_ERROR_LOGON_SERVER_FULL",
            23 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_LOCKED",
            24 => "ID_CHAR_ERROR_SUBSCRIPTION_EXPIRED",
            _ => return None,
        })
    }
}

/// `0xF651 Login_AwaitingSubscriptionExpiration` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoginAwaitingSubscriptionExpiration {
    pub minutes: u32,
}

impl Message for LoginAwaitingSubscriptionExpiration {
    const OPCODE: Opcode = Opcode::LOGIN_AWAITING_SUBSCRIPTION_EXPIRATION;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { minutes: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.minutes);
        Ok(())
    }
}

/// `0xF7C1 Login_AccountBanned` (S2C).
///
/// `expiry` is absolute seconds; `<= 0` means permanent. The handler rounds a positive expiry **up**
/// to the next whole minute (`t += 60 - t % 60`) before formatting it, which is a handler behaviour
/// and not done here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginAccountBanned {
    pub expiry: i32,
    pub reason: String,
}

impl Message for LoginAccountBanned {
    const OPCODE: Opcode = Opcode::LOGIN_ACCOUNT_BANNED;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            expiry: r.i32()?,
            reason: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.expiry);
        w.pstring(&self.reason)
    }
}

/// `0xF7DC Login_AccountBooted` (S2C).
///
/// The reason string may be **absent entirely**, not merely empty; the client substitutes the
/// literal `" for Code of Conduct Violations"` in that case. Modelled as `Option` so the two are
/// distinguishable and so a bodyless instance re-encodes as one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginAccountBooted {
    pub reason: Option<String>,
}

impl Message for LoginAccountBooted {
    const OPCODE: Opcode = Opcode::LOGIN_ACCOUNT_BOOTED;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        if r.remaining() == 0 {
            return Ok(Self { reason: None });
        }
        Ok(Self {
            reason: Some(r.pstring()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if let Some(s) = &self.reason {
            w.pstring(s)?;
        }
        Ok(())
    }
}

/// `0xF630 Character_SetPlayerVisualDesc` (S2C).
///
/// The client unpacks a single string and hands it to a notice handler with an
/// empty body, so the message is **parsed and ignored**. ACE never sends it. Decoded here anyway,
/// because the round-trip gate is about bytes, not about whether anyone cares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerAppearanceMessage {
    pub text: String,
}

impl Message for PlayerAppearanceMessage {
    const OPCODE: Opcode = Opcode::CHARACTER_SET_PLAYER_VISUAL_DESC;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { text: r.pstring()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.text)
    }
}

// ---------------------------------------------------------------------------------------------
// Character generation
// ---------------------------------------------------------------------------------------------

/// The character-creation payload.
///
/// Note the interleaving: style and colour alternate for headgear, shirt, trousers and footwear, and
/// the six shades come only afterwards. Field 39 is a **checksum** the server uses to reject
/// tampered packets; [`CharGenResult::checksum`] computes it exactly as the client does — a plain
/// sum of the listed fields, not of the whole packet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharGenResult {
    /// Hard-coded 1 by the client.
    pub version: u32,
    pub heritage_group: u32,
    pub gender: u32,
    pub eyes_strip: i32,
    pub nose_strip: i32,
    pub mouth_strip: i32,
    pub hair_color: i32,
    pub eye_color: i32,
    pub hair_style: i32,
    pub headgear_style: i32,
    pub headgear_color: u32,
    pub shirt_style: i32,
    pub shirt_color: u32,
    pub trousers_style: i32,
    pub trousers_color: u32,
    pub footwear_style: i32,
    pub footwear_color: u32,
    pub skin_shade: f64,
    pub hair_shade: f64,
    pub headgear_shade: f64,
    pub shirt_shade: f64,
    pub trousers_shade: f64,
    pub footwear_shade: f64,
    pub template_num: i32,
    pub strength: i32,
    pub endurance: i32,
    pub coordination: i32,
    pub quickness: i32,
    pub focus: i32,
    pub self_: i32,
    pub slot: i32,
    pub class_id: u32,
    pub skill_advancement_classes: Vec<i32>,
    pub name: String,
    pub start_area: u32,
    pub is_admin: i32,
    pub is_envoy: i32,
    /// Field 39; see [`CharGenResult::checksum`].
    pub checksum_value: i32,
}

impl CharGenResult {
    /// The client's checksum: the arithmetic sum of fields 2–10, 12, 14, 16 and 24–31.
    ///
    /// That is heritage, gender, the eight face/hair/style ints, the shirt, trousers and footwear
    /// *styles* (not their colours), the template number, the six attributes and the slot. The
    /// client accumulates each as it writes it, which is why the colours and shades are excluded.
    #[must_use]
    pub fn checksum(&self) -> i32 {
        // Wrapping, because the client accumulates into a 32-bit int with no overflow check.
        [
            self.heritage_group as i32,
            self.gender as i32,
            self.eyes_strip,
            self.nose_strip,
            self.mouth_strip,
            self.hair_color,
            self.eye_color,
            self.hair_style,
            self.headgear_style,
            self.shirt_style,
            self.trousers_style,
            self.footwear_style,
            self.template_num,
            self.strength,
            self.endurance,
            self.coordination,
            self.quickness,
            self.focus,
            self.self_,
            self.slot,
        ]
        .into_iter()
        .fold(0i32, i32::wrapping_add)
    }

    #[allow(clippy::too_many_lines)] // 39 wire fields, one line each
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            version: r.u32()?,
            heritage_group: r.u32()?,
            gender: r.u32()?,
            eyes_strip: r.i32()?,
            nose_strip: r.i32()?,
            mouth_strip: r.i32()?,
            hair_color: r.i32()?,
            eye_color: r.i32()?,
            hair_style: r.i32()?,
            headgear_style: r.i32()?,
            headgear_color: r.u32()?,
            shirt_style: r.i32()?,
            shirt_color: r.u32()?,
            trousers_style: r.i32()?,
            trousers_color: r.u32()?,
            footwear_style: r.i32()?,
            footwear_color: r.u32()?,
            skin_shade: r.f64()?,
            hair_shade: r.f64()?,
            headgear_shade: r.f64()?,
            shirt_shade: r.f64()?,
            trousers_shade: r.f64()?,
            footwear_shade: r.f64()?,
            template_num: r.i32()?,
            strength: r.i32()?,
            endurance: r.i32()?,
            coordination: r.i32()?,
            quickness: r.i32()?,
            focus: r.i32()?,
            self_: r.i32()?,
            slot: r.i32()?,
            class_id: r.u32()?,
            skill_advancement_classes: {
                let n = r.i32()?;
                let n = usize::try_from(n).map_err(|_| MessageError::InvalidValue {
                    field: "CharGenResult skill count",
                    value: 0,
                })?;
                if n * 4 > r.remaining() {
                    return Err(MessageError::LengthOverrun {
                        field: "CharGenResult skill count",
                        len: n,
                        available: r.remaining(),
                    });
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.i32()?);
                }
                v
            },
            name: r.pstring()?,
            start_area: r.u32()?,
            is_admin: r.i32()?,
            is_envoy: r.i32()?,
            checksum_value: r.i32()?,
        })
    }

    #[allow(clippy::too_many_lines)] // 39 wire fields, one line each
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.version);
        w.u32(self.heritage_group);
        w.u32(self.gender);
        w.i32(self.eyes_strip);
        w.i32(self.nose_strip);
        w.i32(self.mouth_strip);
        w.i32(self.hair_color);
        w.i32(self.eye_color);
        w.i32(self.hair_style);
        w.i32(self.headgear_style);
        w.u32(self.headgear_color);
        w.i32(self.shirt_style);
        w.u32(self.shirt_color);
        w.i32(self.trousers_style);
        w.u32(self.trousers_color);
        w.i32(self.footwear_style);
        w.u32(self.footwear_color);
        w.f64(self.skin_shade);
        w.f64(self.hair_shade);
        w.f64(self.headgear_shade);
        w.f64(self.shirt_shade);
        w.f64(self.trousers_shade);
        w.f64(self.footwear_shade);
        w.i32(self.template_num);
        w.i32(self.strength);
        w.i32(self.endurance);
        w.i32(self.coordination);
        w.i32(self.quickness);
        w.i32(self.focus);
        w.i32(self.self_);
        w.i32(self.slot);
        w.u32(self.class_id);
        let n = i32::try_from(self.skill_advancement_classes.len()).map_err(|_| {
            MessageError::Unencodable {
                field: "CharGenResult skill count",
                reason: "more than 2 G skills",
            }
        })?;
        w.i32(n);
        for s in &self.skill_advancement_classes {
            w.i32(*s);
        }
        w.pstring(&self.name)?;
        w.u32(self.start_area);
        w.i32(self.is_admin);
        w.i32(self.is_envoy);
        w.i32(self.checksum_value);
        Ok(())
    }
}

/// `0xF656 Character_SendCharGenResult` (C2S).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharacterSendCharGenResult {
    pub account: String,
    pub result: CharGenResult,
}

impl Message for CharacterSendCharGenResult {
    const OPCODE: Opcode = Opcode::CHARACTER_SEND_CHAR_GEN_RESULT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            account: r.pstring()?,
            result: CharGenResult::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.account)?;
        self.result.write(w)
    }
}

/// `0xF643 Character_CharGenVerificationResponse` (S2C).
///
/// The reference server reuses the same opcode for a character-restore response.
///
/// **The `CharacterIdentity` is present only when the response is `OK` (1).**
///
/// The char-gen verification response reads the code, advances the cursor four bytes and selects
/// one of seven cases; anything outside 1..=7 falls to the default. Character identity
/// unpacking occurs only inside case 1. Each refusal arm clears the selected slot and verification
/// state, posts its notice and leaves the rest of the blob untouched. The reference server follows
/// the same shape: its character-creation response emits the guid, name and trailing zero
/// **only** for the successful response.
///
/// Reading one unconditionally would make every real refusal — especially `NameInUse` from
/// the character-restore name-availability check — come off
/// the wire as `SessionEvent::Dropped { Malformed }`, so the screen would never be told and the
/// please-wait modal would stay up for ever.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharGenVerificationResponse {
    /// The verification response code; 1 is success.
    pub response_type: u32,
    /// Meaningful only when `response_type == 1`; default on every other arm.
    pub identity: CharacterIdentity,
}

/// `CG_VERIFICATION_RESPONSE_OK` — the one arm of the handler's switch that unpacks a body.
pub const CG_VERIFICATION_RESPONSE_OK: u32 = 1;

impl Message for CharGenVerificationResponse {
    const OPCODE: Opcode = Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let response_type = r.u32()?;
        let identity = if response_type == CG_VERIFICATION_RESPONSE_OK {
            CharacterIdentity::read(r)?
        } else {
            CharacterIdentity::default()
        };
        Ok(Self {
            response_type,
            identity,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.response_type);
        if self.response_type == CG_VERIFICATION_RESPONSE_OK {
            self.identity.write(w)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0013 Login_PlayerDescription
// ---------------------------------------------------------------------------------------------

/// `ShortCutData` — 12 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShortCutData {
    pub index: i32,
    pub object_id: ObjectId,
    pub spell_id: u32,
}

impl ShortCutData {
    /// The per-record reader — three dwords in declaration order.
    ///
    /// The same three the `0x0013` player module's packed list reads; this is that reader lifted
    /// out so that [`CharacterAddShortCut`] and the module cannot disagree about the layout.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            index: r.i32()?,
            object_id: ObjectId(r.u32()?),
            spell_id: r.u32()?,
        })
    }

    /// The per-record writer.
    pub fn write(&self, w: &mut Writer) {
        w.i32(self.index);
        w.u32(self.object_id.0);
        w.u32(self.spell_id);
    }
}

// ---------------------------------------------------------------------------------------------
// 0x019C Character_AddShortCut / 0x019D Character_RemoveShortCut
// ---------------------------------------------------------------------------------------------

/// `0x019C` carries the header plus one packed
/// [`ShortCutData`].
///
/// Sent by the toolbar at the *moment of the drop*, not at the
/// options flush:
///
/// when the drop is accepted it builds the shortcut from the slot and object id, dispatches the
/// add-shortcut event (`0x019C`, here) and then appends the shortcut to the player module.
///
/// The two calls are consecutive under one condition, so a client that does only the second keeps a
/// bar the server has never heard of — it survives the session and is gone at the next login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterAddShortCut {
    pub shortcut: ShortCutData,
}

impl Message for CharacterAddShortCut {
    const OPCODE: Opcode = Opcode::CHARACTER_ADD_SHORT_CUT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            shortcut: ShortCutData::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.shortcut.write(w);
        Ok(())
    }
}

/// `0x019D`, the header plus the slot index.
///
/// The toolbar's remove sends it with the slot the item was found in, beside the player
/// module's own remove — again two consecutive calls under one
/// condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterRemoveShortCut {
    pub index: u32,
}

impl Message for CharacterRemoveShortCut {
    const OPCODE: Opcode = Opcode::CHARACTER_REMOVE_SHORT_CUT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { index: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.index);
        Ok(())
    }
}

/// The generic-qualities section is the `0x0100` block of [`PlayerModule`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenericQualitiesData {
    pub flags: u32,
    pub option_ints: Option<crate::archive::PackedHash<u32, u32>>,
    pub option_bools: Option<crate::archive::PackedHash<u32, u32>>,
    pub option_floats: Option<crate::archive::PackedHash<u32, f32>>,
    pub option_strings: Option<crate::archive::PackedHash<u32, String>>,
}

impl GenericQualitiesData {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let mut d = Self {
            flags,
            ..Self::default()
        };
        if flags & 0x1 != 0 {
            d.option_ints = Some(r.packed_hash(|r| Ok((r.u32()?, r.u32()?)))?);
        }
        if flags & 0x2 != 0 {
            d.option_bools = Some(r.packed_hash(|r| Ok((r.u32()?, r.u32()?)))?);
        }
        if flags & 0x4 != 0 {
            d.option_floats = Some(r.packed_hash(|r| Ok((r.u32()?, r.f32()?)))?);
        }
        if flags & 0x8 != 0 {
            d.option_strings = Some(r.packed_hash(|r| Ok((r.u32()?, r.pstring()?)))?);
        }
        Ok(d)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags);
        if let Some(h) = &self.option_ints {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.u32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &self.option_bools {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.u32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &self.option_floats {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.f32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &self.option_strings {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.pstring(v)
            })?;
        }
        Ok(())
    }
}

/// The `optionFlags` gates of [`PlayerModule`].
///
/// `0x0002 SquelchList` has **no branch** in this client's module unpacker: the squelch list
/// arrives separately as game event `0x01F4 Communication_SetSquelchDB`.
pub mod player_module_flags {
    pub const SHORTCUT: u32 = 0x0001;
    pub const MULTI_SPELL_LIST: u32 = 0x0004;
    pub const DESIRED_COMPS: u32 = 0x0008;
    pub const EXTENDED_MULTI_SPELL_LISTS: u32 = 0x0010;
    pub const SPELLBOOK_FILTERS: u32 = 0x0020;
    pub const CHARACTER_OPTIONS_2: u32 = 0x0040;
    pub const TIMESTAMP_FORMAT: u32 = 0x0080;
    pub const GENERIC_QUALITIES_DATA: u32 = 0x0100;
    pub const GAMEPLAY_OPTIONS: u32 = 0x0200;
    pub const SPELL_LISTS_8: u32 = 0x0400;
}

/// The player module's unpack, read directly.
///
/// The three spell-bar gates are **mutually exclusive and tested in this order**: `0x0004` gives
/// four more bars (5 total), else `0x0010` gives six more (7 total), else `0x0400` gives seven more
/// (8 total). Bar 0 is always present.
///
/// Two defaults the client applies when a gate is clear, kept here because a rebuild that omits them
/// shows the wrong spellbook: `spell_filters` defaults to `0x3FFF` and `options2` to `0x00948700`
/// (which is `HearGeneralChat | HearTradeChat | HearLFGChat | LeadMissileTargets | ShowHelm |
/// ShowCloak`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerModule {
    pub option_flags: u32,
    /// `CharacterOptions1`.
    pub options: u32,
    pub shortcuts: Option<Vec<ShortCutData>>,
    /// Spell bar 0 is always present; the rest depend on the three mutually exclusive gates.
    pub spell_bars: Vec<Vec<u32>>,
    pub desired_comps: Option<crate::archive::PackedHash<u32, i32>>,
    pub spell_filters: u32,
    /// `CharacterOptions2`.
    pub options2: u32,
    pub timestamp_format: Option<String>,
    pub generic_qualities: Option<GenericQualitiesData>,
    /// The gameplay-options collection — the window placement bag the client itself writes back through
    /// `0x0005 Character_CharacterOptions`. See [`crate::property`].
    pub gameplay_options: Option<PackObjPropertyCollection>,
}

/// A `PlayerModule` that **encodes**, which a derived `Default` would not.
///
/// `read` always consumes spell bar 0 unconditionally, so `write` always emits it — and a derived
/// `Default` gives an empty `spell_bars`, producing a value whose own writer rejects it. That is a
/// foot-gun for anyone building a description in a test fixture, the server side included. The
/// defaults for `spell_filters` and `options2` are the ones the client's unpack applies when
/// their gates are clear, so this is exactly what reading a minimal message produces.
impl Default for PlayerModule {
    fn default() -> Self {
        Self {
            option_flags: 0,
            options: 0,
            shortcuts: None,
            spell_bars: vec![Vec::new()],
            desired_comps: None,
            spell_filters: Self::DEFAULT_SPELL_FILTERS,
            options2: Self::DEFAULT_OPTIONS2,
            timestamp_format: None,
            generic_qualities: None,
            gameplay_options: None,
        }
    }
}

impl PlayerModule {
    /// One optional section's flag bit and its field must agree, or the message is unreadable.
    ///
    /// `read` decides what is present purely from `option_flags`, so `write` must too. It did not:
    /// four sections were emitted on `Option::is_some` instead, and `option_flags` is written to the
    /// wire verbatim. A `Some` field with its bit clear therefore put bytes on the wire that no
    /// reader consumes — desynchronising **everything after them**, including the two
    /// `PackableList`s that follow the module inside `0x0013`. A `None` field with its bit set is
    /// the same fault in the other direction.
    ///
    /// Neither is representable on the wire, so this refuses rather than guessing which half the
    /// caller meant.
    ///
    /// # Errors
    /// [`MessageError::Unencodable`] when the flag and the field disagree.
    fn check_section(flag: bool, present: bool, name: &'static str) -> Result<(), MessageError> {
        if flag == present {
            return Ok(());
        }
        Err(MessageError::Unencodable {
            field: name,
            reason: if flag {
                "option_flags marks this PlayerModule section present but the field is None"
            } else {
                "this PlayerModule field is Some but option_flags does not mark it present"
            },
        })
    }

    /// `spell_filters_` when `0x0020` is clear.
    pub const DEFAULT_SPELL_FILTERS: u32 = 0x3FFF;
    /// `options2_` when `0x0040` is clear: `0x0094_8700` plus bit 25, "hear player-killer
    /// deaths", which is on unless the character saved it off.
    pub const DEFAULT_OPTIONS2: u32 = 0x0294_8700;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        use player_module_flags as f;
        let option_flags = r.u32()?;
        let options = r.u32()?;
        let has = |b: u32| option_flags & b != 0;
        let mut m = Self {
            option_flags,
            options,
            spell_filters: Self::DEFAULT_SPELL_FILTERS,
            options2: Self::DEFAULT_OPTIONS2,
            // Explicitly empty: the bars are **pushed** below, and `Default` seeds bar 0 so that a
            // default-constructed module is encodable. Inheriting that seed here would read one bar
            // too many.
            spell_bars: Vec::new(),
            ..Self::default()
        };
        if has(f::SHORTCUT) {
            m.shortcuts = Some(r.packed_list(ShortCutData::read)?);
        }
        // Bar 0, then the extra bars: 0x0004 -> 4 more, else 0x0010 -> 6 more, else 0x0400 -> 7.
        let extra = if has(f::MULTI_SPELL_LIST) {
            4
        } else if has(f::EXTENDED_MULTI_SPELL_LISTS) {
            6
        } else if has(f::SPELL_LISTS_8) {
            7
        } else {
            0
        };
        for _ in 0..=extra {
            m.spell_bars.push(r.packed_list(Reader::u32)?);
        }
        if has(f::DESIRED_COMPS) {
            m.desired_comps = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
        }
        if has(f::SPELLBOOK_FILTERS) {
            m.spell_filters = r.u32()?;
        }
        if has(f::CHARACTER_OPTIONS_2) {
            m.options2 = r.u32()?;
        }
        if has(f::TIMESTAMP_FORMAT) {
            m.timestamp_format = Some(r.pstring()?);
        }
        if has(f::GENERIC_QUALITIES_DATA) {
            m.generic_qualities = Some(GenericQualitiesData::read(r)?);
        }
        if has(f::GAMEPLAY_OPTIONS) {
            // Packed serialized objects stream without a length: the decoder wraps the caller's
            // cursor in a versioned archive, lets the collection consume what it needs
            // and advances the caller by the archive's final position. There is no length, so the
            // schema in [`crate::property`] is what says where it ends.
            m.gameplay_options = Some(PackObjPropertyCollection::read(r)?);
        }
        r.align4()?;
        Ok(m)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        use player_module_flags as f;
        let has = |b: u32| self.option_flags & b != 0;
        w.u32(self.option_flags);
        w.u32(self.options);
        Self::check_section(has(f::SHORTCUT), self.shortcuts.is_some(), "shortcuts")?;
        if let Some(s) = self.shortcuts.as_ref().filter(|_| has(f::SHORTCUT)) {
            w.packed_list(s, |w, c| {
                c.write(w);
                Ok(())
            })?;
        }
        // The bar count is fixed by the gates exactly as it is on read: bar 0 always, then 4, 6 or
        // 7 more. A `spell_bars` of a different length is the same desync as a mismatched section.
        let want_bars = 1 + if has(f::MULTI_SPELL_LIST) {
            4
        } else if has(f::EXTENDED_MULTI_SPELL_LISTS) {
            6
        } else if has(f::SPELL_LISTS_8) {
            7
        } else {
            0
        };
        if self.spell_bars.len() != want_bars {
            return Err(MessageError::Unencodable {
                field: "spell_bars",
                reason: "the number of spell bars must match what option_flags' gates imply",
            });
        }
        for bar in &self.spell_bars {
            w.packed_list(bar, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
        }
        Self::check_section(
            has(f::DESIRED_COMPS),
            self.desired_comps.is_some(),
            "desired_comps",
        )?;
        if let Some(h) = self
            .desired_comps
            .as_ref()
            .filter(|_| has(f::DESIRED_COMPS))
        {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.i32(*v);
                Ok(())
            })?;
        }
        if has(f::SPELLBOOK_FILTERS) {
            w.u32(self.spell_filters);
        }
        if has(f::CHARACTER_OPTIONS_2) {
            w.u32(self.options2);
        }
        Self::check_section(
            has(f::TIMESTAMP_FORMAT),
            self.timestamp_format.is_some(),
            "timestamp_format",
        )?;
        if let Some(s) = self
            .timestamp_format
            .as_ref()
            .filter(|_| has(f::TIMESTAMP_FORMAT))
        {
            w.pstring(s)?;
        }
        Self::check_section(
            has(f::GENERIC_QUALITIES_DATA),
            self.generic_qualities.is_some(),
            "generic_qualities",
        )?;
        if let Some(g) = self
            .generic_qualities
            .as_ref()
            .filter(|_| has(f::GENERIC_QUALITIES_DATA))
        {
            g.write(w)?;
        }
        Self::check_section(
            has(f::GAMEPLAY_OPTIONS),
            self.gameplay_options.is_some(),
            "gameplay_options",
        )?;
        if let Some(o) = self
            .gameplay_options
            .as_ref()
            .filter(|_| has(f::GAMEPLAY_OPTIONS))
        {
            o.write(w)?;
        }
        w.align4();
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0005 Character_PlayerOptionChangedEvent
// ---------------------------------------------------------------------------------------------

/// `0x0005 Character_PlayerOptionChangedEvent` (C2S).
/// One `PlayerOption` ordinal and its new value, nothing else.
///
/// This is the option setter's immediate half: without it a client can only set the bit and mark
/// the module, and the server never hears the change. It matters well beyond the
/// options screen, because the auto-save option test's twenty-one ordinals are exactly the
/// options with a **server-visible** effect. For the six channel-listen options
/// (`ListenTo{Allegiance,General,LFG,Roleplay,Society,Trade}Chat`), the reference server responds by
/// joining or leaving the corresponding global chat room — which is the only way a client can
/// ask to (re)join a Turbine chat room without logging out.
///
/// # The body on the wire
///
/// The body is the action id `5`, the `PlayerOption` ordinal, the order header's align-to-4 pad
/// loop (already aligned), then the value.
///
/// Recorded retail fellowship sessions contain eleven outgoing `0x0005` messages, every one a
/// 20-byte queue-3 blob. One observed message turns on `AppearOffline` (ordinal 39).
/// This provides both the message size and the option/value boundary without relying on names.
/// The ordinals observed there are 1, 2, 17, 18 and 39, all five of them in
/// `dereth_client_model::player::options::AUTO_SAVE_ORDINALS`, and all five are server-side option ordinals,
/// which is what fixes the ordinal space: it is the client's `PlayerOption` index, not the bit.
///
/// `value` is a full dword, not a byte: retail writes the complete value word, and every
/// recorded body carries either zero or one as a four-byte value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterPlayerOptionChangedEvent {
    /// The `PlayerOption` ordinal — `dereth_client_model::player::options::PLAYER_OPTIONS`' index.
    pub option: u32,
    /// The new value. Retail sends `1` or `0`.
    pub value: u32,
}

impl Message for CharacterPlayerOptionChangedEvent {
    const OPCODE: Opcode = Opcode::CHARACTER_PLAYER_OPTION_CHANGED_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            option: r.u32()?,
            value: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.option);
        w.u32(self.value);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 0x01A1 Character_CharacterOptionsEvent
// ---------------------------------------------------------------------------------------------

/// `0x01A1 Character_CharacterOptionsEvent` (C2S), whose entire body is a **[`PlayerModule`]**
/// packed in the player module's wire format.
///
/// It is the bulk counterpart of `0x0005 Character_PlayerOptionChangedEvent`, which carries one
/// `PlayerOption` index and its new value: `0x01A1` goes out when several options change at once,
/// and whenever a shortcut, a spell bar or a desired-component level changes. Its two callers are
/// the player module's save-to-server path and its per-frame tick — the second
/// is the 480-second dirty timer, which is why the recorded sessions carry so few of them.
///
/// The `PlayerOption`-index-to-bit map for `options` / `options2` is **not** repeated here; the
/// option getter is tabulated in
/// `docs/networking/messages/03-qualities-and-updates.md` §6. This codec carries both words verbatim, as the client
/// does.
///
/// # The same struct, but not the same set of sections
///
/// The player-module decoder reads ten gates and `0x0013` exercises them from the server's
/// side. Its header construction decides what the **client** may put on the wire,
/// and it is narrower — read it before assuming a captured `0x01A1` can be any shape a `0x0013`
/// can:
///
/// * `0x0400` and `0x0060` are set **unconditionally**, so a client-packed module always carries
///   **eight** spell bars, the spell filters and the second options word, whatever their values;
/// * `0x0001`, `0x0008`, `0x0100` and `0x0200` follow the optional shortcuts,
///   desired-component levels, generic qualities and non-empty gameplay-options collection;
/// * `0x0004` and `0x0010` — the 5-bar and 7-bar shapes — are **never** set, which agrees with
///   the size calculation packing favorite spells in a loop of exactly 8 over a 16-byte
///   fixed part (the flag word, two option words and spell filters);
/// * `0x0080 TimestampFormat` is never set, **and the writer never emits the string**, so a
///   `timestamp_format` can only ever arrive from the server.
///
/// None of that is enforced here, deliberately: the two directions share one codec, and a decoder
/// narrowed to the client's shape would mis-read `0x0013`. The player module's own
/// flag/field agreement check is the only constraint, and it is the one the wire actually has.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharacterCharacterOptionsEvent {
    pub module: PlayerModule,
}

impl Message for CharacterCharacterOptionsEvent {
    const OPCODE: Opcode = Opcode::CHARACTER_CHARACTER_OPTIONS_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            module: PlayerModule::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.module.write(w)
    }
}

/// `0x0013 Login_PlayerDescription` (S2C).
///
/// The largest message in the protocol, delivered as an ordered game event whose object id is the
/// player's guid. **Its arrival is what makes the client "in world"**: the `0x0013` arm is the only
/// caller of the in-world transition, which releases every ephemeral UI blob held
/// since login. See [`crate::Message`]'s note on the double opcode check.
///
/// Four sections, in this order: `ACQualities`, `PlayerModule`, the main-pack inventory list and the
/// equipped-item list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoginPlayerDescription {
    pub qualities: AcQualities,
    pub player_module: PlayerModule,
    pub content_profiles: Vec<ContentProfile>,
    pub inventory_placements: Vec<InventoryPlacement>,
}

impl Message for LoginPlayerDescription {
    const OPCODE: Opcode = Opcode::LOGIN_PLAYER_DESCRIPTION;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            qualities: AcQualities::read(r)?,
            player_module: PlayerModule::read(r)?,
            content_profiles: r.packed_list(ContentProfile::read)?,
            inventory_placements: r.packed_list(InventoryPlacement::read)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.qualities.write(w)?;
        self.player_module.write(w)?;
        w.packed_list(&self.content_profiles, |w, c| {
            c.write(w);
            Ok(())
        })?;
        w.packed_list(&self.inventory_placements, |w, p| {
            p.write(w);
            Ok(())
        })
    }
}

/// Re-exported so callers of this family do not have to reach into `types::qualities`.
pub use crate::types::qualities::AcQualities as PlayerQualities;
/// Ditto.
pub type PlayerCreationProfile = CreationProfile;

#[cfg(test)]
mod tests {
    /// The default player module must **encode** and round-trip.
    ///
    /// The derived `Default` did not: it left `spell_bars` empty while `read` consumes bar 0
    /// unconditionally, so `write` rejected its own default value. Only test fixtures built through
    /// `Default` were bitten — production paths set the gates and build eight bars — but a default
    /// that cannot be written is a trap for the next person, and it was reported as one from the
    /// server side.
    #[test]
    fn the_default_player_module_encodes_and_round_trips() {
        let m = PlayerModule::default();
        assert_eq!(
            m.spell_bars.len(),
            1,
            "bar 0 is unconditional on both sides"
        );
        let mut w = Writer::new();
        m.write(&mut w)
            .expect("the default value must be encodable");
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(PlayerModule::read(&mut r).expect("round-trips"), m);
    }

    /// The player-module writer must emit exactly the sections `option_flags` claims and refuse
    /// when the flag word and the fields disagree, rather than putting unreadable bytes on the wire.
    ///
    /// Oracle: the player module's unpack decides what is present **purely from the flag
    /// word**, and `write` sends that same word verbatim. Four sections used to be emitted on
    /// `Option::is_some` instead, so a `Some` field with its bit clear appended bytes no reader
    /// consumes — desynchronising everything after the module, including the two `PackableList`s
    /// that follow it inside `0x0013`. Neither direction of the mismatch is representable, so this
    /// refuses instead of guessing.
    #[test]
    fn the_player_module_writer_emits_exactly_the_sections_its_flags_claim() {
        use player_module_flags as flags;
        // Flag clear, field present: would have written a section nothing reads.
        let mut m = PlayerModule {
            option_flags: 0,
            // Bar 0 is unconditional on both sides.
            spell_bars: vec![vec![]],
            spell_filters: PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: PlayerModule::DEFAULT_OPTIONS2,
            gameplay_options: Some(PackObjPropertyCollection::default()),
            ..PlayerModule::default()
        };
        let mut w = Writer::new();
        assert!(
            matches!(m.write(&mut w), Err(MessageError::Unencodable { field, .. }) if field == "gameplay_options"),
            "a Some field with its bit clear must be refused, not silently written"
        );

        // Flag set, field absent: the reader would run off the end of the body.
        m.option_flags = flags::GAMEPLAY_OPTIONS;
        m.gameplay_options = None;
        let mut w = Writer::new();
        assert!(
            matches!(m.write(&mut w), Err(MessageError::Unencodable { field, .. }) if field == "gameplay_options"),
            "a None field with its bit set must be refused"
        );

        // Agreeing: writes, and reads back identically.
        m.gameplay_options = Some(PackObjPropertyCollection::default());
        let mut w = Writer::new();
        m.write(&mut w).expect("flags and fields agree");
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        let back = PlayerModule::read(&mut r).expect("round-trips");
        assert_eq!(back, m);
    }

    use super::*;
    use crate::{read_body, round_trip, write_body};

    /// Oracle: the `CharacterSet` format and
    /// character-identity unpack. `CharacterSet` must unpack the account id.
    #[test]
    fn character_set_round_trips_and_carries_the_account() {
        let m = LoginCharacterSet {
            status: 0,
            characters: vec![
                CharacterIdentity {
                    gid: ObjectId(0x5000_0001),
                    name: "Lark".into(),
                    seconds_greyed_out: 0,
                },
                CharacterIdentity {
                    gid: ObjectId(0x5000_0002),
                    name: "Doomed".into(),
                    seconds_greyed_out: 3600,
                },
            ],
            deleted: vec![],
            num_allowed_characters: 5,
            account: "ac01".into(),
            use_turbine_chat: 1,
            has_throne_of_destiny: 1,
        };
        let bytes = write_body(&m).unwrap();
        let back: LoginCharacterSet = round_trip(&bytes);
        assert_eq!(back.account, "ac01");
        assert_eq!(back.characters.len(), 2);
    }

    /// Oracle: `docs/networking/messages/01-login-and-character.md` §4 — `0xF7E1` carries
    /// `(connections, max, name)`.
    #[test]
    fn world_info_yields_connections_max_and_name() {
        let m = LoginWorldInfo {
            connections: 12,
            max_connections: 800,
            world_name: "Frostfell".into(),
        };
        let bytes = write_body(&m).unwrap();
        let back: LoginWorldInfo = round_trip(&bytes);
        assert_eq!(
            (
                back.connections,
                back.max_connections,
                back.world_name.as_str()
            ),
            (12, 800, "Frostfell")
        );
    }

    /// `0xF65A` is two length-prefixed strings, each padded to four bytes; the box shows the
    /// first, then the second on a line of its own when there is one.
    #[test]
    fn the_character_screen_message_is_two_strings_shown_one_after_the_other() {
        let m = LoginCharacterScreenMessage {
            text: "Hi".into(),
            more: String::new(),
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes, [2, 0, b'H', b'i', 0, 0, 0, 0]);
        let back: LoginCharacterScreenMessage = round_trip(&bytes);
        assert_eq!(back.shown(), "Hi");
        let two = LoginCharacterScreenMessage {
            text: "Hi".into(),
            more: "there".into(),
        };
        assert_eq!(two.shown(), "Hi\nthere");
    }

    /// Oracle: the character-error code table (`docs/networking/messages/01-login-and-character.md`
    /// §4). All 20 tokens must map; the four values the client has no case for must map to nothing
    /// rather than to a wrong token.
    #[test]
    fn all_twenty_char_error_tokens_map() {
        let mapped: Vec<_> = (0..26u32)
            .filter(|v| CharErrorCode(*v).token().is_some())
            .collect();
        // 21 values map, because 4 and 8 deliberately share one token: 20 distinct tokens.
        let distinct: std::collections::BTreeSet<_> = mapped
            .iter()
            .map(|v| CharErrorCode(*v).token().unwrap())
            .collect();
        assert_eq!(distinct.len(), 20, "twenty ID_CHAR_ERROR_* tokens");
        assert_eq!(
            CharErrorCode(8).token(),
            CharErrorCode(4).token(),
            "ACCOUNT_IN_USE shares SERVER_CRASH's token"
        );
        for no_case in [0u32, 2, 7, 22, 25] {
            assert_eq!(CharErrorCode(no_case).token(), None, "value {no_case}");
        }
    }

    /// Oracle: the character-generation result's field 39
    /// (`docs/networking/messages/01-login-and-character.md` §3). The checksum is a sum of
    /// named fields, **not** of the whole packet, and excludes every colour and shade.
    #[test]
    fn the_chargen_checksum_sums_only_the_named_fields() {
        let mut r = CharGenResult {
            version: 1,
            heritage_group: 1,
            gender: 1,
            strength: 10,
            slot: 2,
            ..CharGenResult::default()
        };
        assert_eq!(r.checksum(), 1 + 1 + 10 + 2);
        // A colour is not in the sum.
        r.shirt_color = 0x1234;
        assert_eq!(r.checksum(), 1 + 1 + 10 + 2);
        // A style is.
        r.shirt_style = 3;
        assert_eq!(r.checksum(), 1 + 1 + 10 + 2 + 3);
    }

    #[test]
    fn chargen_result_round_trips() {
        let m = CharacterSendCharGenResult {
            account: "ac01".into(),
            result: CharGenResult {
                version: 1,
                heritage_group: 1,
                gender: 2,
                skill_advancement_classes: vec![2, 2, 3, 1],
                name: "Testy".into(),
                start_area: 0,
                checksum_value: 42,
                ..CharGenResult::default()
            },
        };
        let bytes = write_body(&m).unwrap();
        let _: CharacterSendCharGenResult = round_trip(&bytes);
    }

    /// The three spell-bar gates are mutually exclusive and give 5, 7 or 8 bars.
    /// Oracle: retail behaviour.
    #[test]
    fn the_spell_bar_gates_are_mutually_exclusive() {
        use player_module_flags as f;
        for (flag, bars) in [
            (0, 1usize),
            (f::MULTI_SPELL_LIST, 5),
            (f::EXTENDED_MULTI_SPELL_LISTS, 7),
            (f::SPELL_LISTS_8, 8),
            // 0x0004 wins over both of the others when several are set.
            (f::MULTI_SPELL_LIST | f::SPELL_LISTS_8, 5),
            (f::EXTENDED_MULTI_SPELL_LISTS | f::SPELL_LISTS_8, 7),
        ] {
            let m = PlayerModule {
                option_flags: flag,
                spell_bars: vec![Vec::new(); bars],
                ..PlayerModule::default()
            };
            let mut w = Writer::body();
            m.write(&mut w).unwrap();
            let bytes = w.into_inner();
            let mut r = Reader::body(&bytes);
            let back = PlayerModule::read(&mut r).unwrap();
            assert_eq!(back.spell_bars.len(), bars, "flags 0x{flag:04X}");
            r.expect_exhausted().unwrap();
        }
    }

    /// The two defaults the client applies when the gates are clear. A rebuild that leaves them zero
    /// shows an empty spellbook and the wrong chat channels.
    #[test]
    fn the_player_module_defaults_are_applied_when_the_gates_are_clear() {
        let m = PlayerModule {
            spell_bars: vec![Vec::new()],
            ..PlayerModule::default()
        };
        let mut w = Writer::body();
        m.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        let back = PlayerModule::read(&mut r).unwrap();
        assert_eq!(back.spell_filters, 0x3FFF);
        assert_eq!(back.options2, 0x0294_8700);
    }

    /// The player description is four sections in a fixed order; the round trip is what proves the
    /// order is stated once per direction.
    #[test]
    fn player_description_round_trips() {
        let m = LoginPlayerDescription {
            qualities: AcQualities::default(),
            player_module: PlayerModule {
                spell_bars: vec![Vec::new()],
                spell_filters: PlayerModule::DEFAULT_SPELL_FILTERS,
                options2: PlayerModule::DEFAULT_OPTIONS2,
                ..PlayerModule::default()
            },
            content_profiles: vec![ContentProfile {
                iid: ObjectId(0x5000_0010),
                container_properties: 0,
            }],
            inventory_placements: vec![InventoryPlacement {
                iid: ObjectId(0x5000_0011),
                location: 0x0010_0000,
                priority: 1,
            }],
        };
        let bytes = write_body(&m).unwrap();
        let _: LoginPlayerDescription = round_trip(&bytes);
    }

    /// `0xF7DC`'s reason string can be absent entirely, and an absent one must not re-encode as an
    /// empty one.
    #[test]
    fn account_booted_distinguishes_an_absent_reason_from_an_empty_one() {
        let absent: LoginAccountBooted = read_body(&[]).unwrap();
        assert_eq!(absent.reason, None);
        assert_eq!(write_body(&absent).unwrap(), Vec::<u8>::new());

        let empty = LoginAccountBooted {
            reason: Some(String::new()),
        };
        let bytes = write_body(&empty).unwrap();
        assert_eq!(bytes.len(), 4);
        let _: LoginAccountBooted = round_trip(&bytes);
    }

    #[test]
    fn the_empty_bodied_messages_have_empty_bodies() {
        assert_eq!(write_body(&LoginSendEnterWorldRequest).unwrap().len(), 0);
        assert_eq!(write_body(&LoginEnterGameServerReady).unwrap().len(), 0);
        assert_eq!(
            write_body(&CharacterLoginCompleteNotification)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(write_body(&LoginExecuteLogOff).unwrap().len(), 0);
        assert_eq!(write_body(&CharacterDeleteAck).unwrap().len(), 0);
    }

    /// The player option change is the two dwords retail sent.
    #[test]
    fn the_player_option_change_is_the_two_dwords_retail_sent() {
        // fellowship-one-vassal.jsonl t_rel = 350.029 -- AppearOffline (ordinal 39 = 0x27) turning
        // on.
        let on: CharacterPlayerOptionChangedEvent =
            read_body(&[0x27, 0, 0, 0, 0x01, 0, 0, 0]).expect("eight bytes");
        assert_eq!((on.option, on.value), (0x27, 1));
        // ...and t_rel = 388.854, the same option turning off again.
        let off: CharacterPlayerOptionChangedEvent =
            read_body(&[0x27, 0, 0, 0, 0x00, 0, 0, 0]).expect("eight bytes");
        assert_eq!((off.option, off.value), (0x27, 0));

        for (option, value, bytes) in [
            // fellowship-two-monarch t_rel = 22.900 / 31.569 -- IgnoreAllegianceRequests.
            (1u32, 1u32, [0x01, 0, 0, 0, 0x01, 0, 0, 0]),
            (1, 0, [0x01, 0, 0, 0, 0x00, 0, 0, 0]),
            // fellowship t_rel = 465.113 -- IgnoreFellowshipRequests off.
            (2, 0, [0x02, 0, 0, 0, 0x00, 0, 0, 0]),
            // fellowship-two-monarch t_rel = 367.775 -- FellowshipShareLoot on.
            (0x11, 1, [0x11, 0, 0, 0, 0x01, 0, 0, 0]),
            // fellowship t_rel = 472.220 -- FellowshipAutoAcceptRequests on.
            (0x12, 1, [0x12, 0, 0, 0, 0x01, 0, 0, 0]),
        ] {
            let m = CharacterPlayerOptionChangedEvent { option, value };
            assert_eq!(
                write_body(&m).unwrap(),
                bytes,
                "option {option} value {value}"
            );
            let _: CharacterPlayerOptionChangedEvent = round_trip(&bytes);
        }
        assert_eq!(CharacterPlayerOptionChangedEvent::OPCODE.0, 0x0005);
    }

    /// The same opcode carries a different body each way; the two types must not be interchangeable.
    #[test]
    fn log_off_has_a_different_body_in_each_direction() {
        assert_eq!(
            LoginExecuteLogOff::OPCODE,
            LoginExecuteLogOffRequest::OPCODE,
            "same opcode"
        );
        let req = LoginExecuteLogOffRequest {
            character: ObjectId(0x5000_0001),
        };
        let bytes = write_body(&req).unwrap();
        assert_eq!(bytes.len(), 4);
        let _: LoginExecuteLogOffRequest = round_trip(&bytes);
        // The S2C form of the same opcode rejects that body, because it must be empty.
        assert!(read_body::<LoginExecuteLogOff>(&bytes).is_err());
    }
}
