//! Chat channels, shared entry state, focus availability and squelch.
//!
//! The model decides destinations and remembered speakers. Interface adapters own widgets
//! and scrollback; the contract supplies routing and presentation policy data.

/// Display-line composition for incoming speech, emotes, channels and fellowship messages.
pub mod composition;

use dereth_primitives::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

/// Chat text types. The values 0x1A–0x1E, 0x20 and 0x21 exist in the colour table but have **no
/// name**: the name lookup returns `"Unknown"` and there is no known producer. Carry the colours;
/// leave the names blank.
pub mod text_type {
    pub const DEFAULT: u32 = 0;
    /// A pseudo-type meaning "every channel".
    pub const ALL_CHANNELS: u32 = 1;
    pub const SPEECH: u32 = 2;
    pub const SPEECH_DIRECT: u32 = 3;
    pub const SPEECH_DIRECT_SEND: u32 = 4;
    pub const SYSTEM: u32 = 5;
    pub const COMBAT: u32 = 6;
    pub const MAGIC: u32 = 7;
    pub const CHANNEL: u32 = 8;
    pub const CHANNEL_SEND: u32 = 9;
    pub const SOCIAL: u32 = 10;
    pub const SOCIAL_SEND: u32 = 11;
    pub const EMOTE: u32 = 12;
    pub const ADVANCEMENT: u32 = 13;
    pub const ABUSE: u32 = 14;
    pub const HELP: u32 = 15;
    pub const APPRAISAL: u32 = 16;
    pub const SPELLCASTING: u32 = 17;
    pub const ALLEGIANCE: u32 = 18;
    pub const FELLOWSHIP: u32 = 19;
    pub const WORLD_BROADCAST: u32 = 20;
    pub const COMBAT_ENEMY: u32 = 21;
    pub const COMBAT_SELF: u32 = 22;
    pub const RECALL: u32 = 23;
    pub const CRAFT: u32 = 24;
    pub const SALVAGING: u32 = 25;
    /// The client's **own local-error type**: no timestamp is prefixed and nothing is written to
    /// the chat log file. Every "You must select a valid combat target before attacking"-class message
    /// uses it, which is why it is also `dereth_client_model`'s feedback channel.
    pub const LOCAL_ERROR: u32 = 0x1A;
    pub const ADMIN_TELL: u32 = 0x1F;
    /// The colour table's size: 0..=0x21.
    pub const TABLE_LEN: usize = 34;
}

/// Maps a chat text type to its display name.
#[must_use]
pub fn log_text_type_name(t: u32) -> &'static str {
    match t {
        0 => "Default",
        1 => "All",
        2 => "Speech",
        3 => "Tell",
        4 => "Speech_Direct_Send",
        5 => "System",
        6 => "Combat",
        7 => "Magic",
        8 => "Channel",
        9 => "Channel_Send",
        10 => "Social",
        11 => "Social_Send",
        12 => "Emote",
        13 => "Advancement",
        14 => "Abuse",
        15 => "Help",
        16 => "Appraisal",
        17 => "Spellcasting",
        18 => "Allegiance",
        19 => "Fellowship",
        20 => "World_Broadcast",
        21 => "Combat_Enemy",
        22 => "Combat_Self",
        23 => "Recall",
        24 => "Craft",
        25 => "Salvaging",
        0x1F => "Admin_Tell",
        _ => "Unknown",
    }
}

/// Behavior: the fourteen types that may be squelched
/// per character. Everything else is unsquelchable.
pub const LEGAL_CHANNELS: [u32; 14] = [
    2, 3, 6, 7, 0x0C, 0x10, 0x11, 0x12, 0x13, 0x15, 0x16, 0x17, 0x18, 0x19,
];

#[must_use]
pub fn is_legal_channel(t: u32) -> bool {
    LEGAL_CHANNELS.contains(&t)
}

/// Maps a **command word** to its channel bit.
///
/// Nineteen command names share one handler. It re-reads the current command word and asks this
/// function which channel was meant. The comparisons are a flat chain in the order below and are
/// **ASCII case-insensitive** (`@F` is `@f`). An empty string is rejected before the first compare
/// by testing that the buffer contains only its terminating NUL, and returns `0`.
///
/// [`get_channel_name`] is the inverse mapping.
///
/// **`Help` (`0x400`) is in this table and is not a broadcast channel.** The caller excludes that
/// value before broadcasting, which is how `@help` stays a help command. The exclusion therefore
/// belongs in [`channel_command_broadcasts_on`], not in this lookup.
///
/// `fellowship` and `fellows` belong to the `0x800` group: both occur in the client's comparison
/// ladder and command table, though some alias lists omit them.
#[must_use]
pub fn get_channel_id(word: &str) -> u32 {
    // `_stricmp` against ASCII literals: ASCII-case-insensitive, and nothing else.
    let w = word.to_ascii_lowercase();
    match w.as_str() {
        "" => 0,
        "allegiance" | "a" | "ab" => 0x0200_0000,
        "co-vassals" | "covassals" | "covassal" | "c" => 0x0100_0000,
        "monarch" | "m" => 0x4000,
        "patron" | "p" => 0x2000,
        "vassals" | "vassal" | "v" => 0x1000,
        "fellowship" | "fellows" | "fellow" | "f" | "group" | "g" | "party" => 0x800,
        "av" | "av1" => 0x8,
        "av2" => 0x10,
        "av3" => 0x20,
        "abuse" => 0x1,
        "ad" | "admin" => 0x2,
        "au" | "audit" => 0x4,
        "help" => 0x400,
        "sent" => 0x200,
        "celhan" => 0x0800_0000,
        "eldweb" => 0x1000_0000,
        "radblo" => 0x2000_0000,
        "olthoi" | "ol" => 0x4000_0000,
        _ => 0,
    }
}

/// Whether a channel id can carry a broadcast. Zero and the Help channel are rejected;
/// every other id is accepted.
#[must_use]
pub fn channel_command_broadcasts_on(id: u32) -> bool {
    id != 0 && id != 0x400
}

/// Maps a channel bit to its display name, the inverse of [`get_channel_id`].
///
/// The order comes from the comparison ladder rather than the string table: `0x0100_0000` maps to
/// `"Co-vassals"`, `0x0200_0000` to `"Allegiance"`, and `0x400` to `"Help"`. Channel-broadcast
/// display composes `"You say on the %s channel"` or `"... says on the %s channel"` around this
/// name and substitutes `"<unknown>"` when lookup fails.
#[must_use]
pub fn get_channel_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0x0000_0001 => "Abuse",
        0x0000_0002 => "Admin",
        0x0000_0004 => "Audit",
        0x0000_0008 => "Advocate 1",
        0x0000_0010 => "Advocate 2",
        0x0000_0020 => "Advocate 3",
        0x0000_0200 => "Sentinel",
        0x0000_0400 => "Help",
        0x0000_0800 => "Fellowship",
        0x0000_1000 => "Vassals",
        0x0000_2000 => "Patron",
        0x0000_4000 => "Monarch",
        0x0100_0000 => "Co-vassals",
        0x0200_0000 => "Allegiance",
        0x0400_0000 => "FellowBroadcast",
        0x0800_0000 => "Celestial Hand",
        0x1000_0000 => "Eldrytch Web",
        0x2000_0000 => "Radiant Blood",
        0x4000_0000 => "Olthoi",
        _ => return None,
    })
}

/// The 13 talk focuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum TalkFocus {
    #[default]
    All = 1,
    Selected = 2,
    Fellowship = 3,
    Patron = 4,
    Monarch = 5,
    Vassals = 6,
    Allegiance = 7,
    General = 8,
    Trade = 9,
    Lfg = 10,
    Roleplay = 11,
    Society = 12,
    Olthoi = 13,
}

impl TalkFocus {
    #[must_use]
    pub fn from_raw(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::All,
            2 => Self::Selected,
            3 => Self::Fellowship,
            4 => Self::Patron,
            5 => Self::Monarch,
            6 => Self::Vassals,
            7 => Self::Allegiance,
            8 => Self::General,
            9 => Self::Trade,
            10 => Self::Lfg,
            11 => Self::Roleplay,
            12 => Self::Society,
            13 => Self::Olthoi,
            _ => return None,
        })
    }
}

/// The client's drop-down order, which is **not** the id order.
pub const TALK_FOCUS_MENU_ORDER: [TalkFocus; 12] = [
    TalkFocus::Monarch,
    TalkFocus::Selected,
    TalkFocus::Patron,
    TalkFocus::All,
    TalkFocus::Vassals,
    TalkFocus::Fellowship,
    TalkFocus::Allegiance,
    TalkFocus::General,
    TalkFocus::Trade,
    TalkFocus::Lfg,
    TalkFocus::Roleplay,
    TalkFocus::Society,
];

/// One squelch record — a 128-bit set of chat types plus the two carried-but-unread fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SquelchEntry {
    pub types: BTreeSet<u32>,
    /// The zone-squelch field is exposed as the account-squelch flag during panel iteration.
    pub is_zone_squelch: i32,
    pub name: String,
}

impl SquelchEntry {
    /// Tests whether this entry squelches a chat type.
    ///
    /// For `type == 1` (`All`) it returns true only if **all 128 bits** are set; otherwise it
    /// returns bit `type`.
    #[must_use]
    pub fn is_squelched(&self, t: u32) -> bool {
        if t == text_type::ALL_CHANNELS {
            return (0..128).all(|i| self.types.contains(&i));
        }
        self.types.contains(&t)
    }

    /// Behavior: scans the same 128 bits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        (0..128).all(|i| !self.types.contains(&i))
    }

    /// Set every bit, which is what makes `All` answer true.
    pub fn squelch_everything(&mut self) {
        self.types = (0..128).collect();
    }
}

/// The squelch database (0x44).
#[derive(Debug, Clone, Default)]
pub struct SquelchDb {
    /// Squelched **account** names.
    pub accounts: BTreeMap<String, u32>,
    pub characters: BTreeMap<ObjectId, SquelchEntry>,
    /// The "turn off this channel entirely" entry.
    pub global: SquelchEntry,
}

impl SquelchDb {
    /// Tests the global, account, and character squelch entries in client order.
    ///
    /// Note the middle line: **`Spellcasting` (0x11) is never account-squelched**.
    #[must_use]
    pub fn is_squelched(&self, character: ObjectId, account: &str, t: u32) -> bool {
        if self.global.is_squelched(t) {
            return true;
        }
        if t == text_type::SPELLCASTING {
            return false;
        }
        if self.accounts.contains_key(account) {
            return true;
        }
        self.characters
            .get(&character)
            .is_some_and(|e| e.is_squelched(t))
    }
}

/// One synchronous talk-focus-enabled UI notice, retained in call order until the host can borrow
/// the current subscriber. `is_olthoi` captures the value read when the notice is emitted, not a
/// later player-description value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TalkFocusNotice {
    pub focus: TalkFocus,
    pub enabled: bool,
    pub is_olthoi: bool,
}

/// Talk focus, the squelch DB and the last-used speaker names.
#[derive(Debug, Clone, Default)]
pub struct ChatState {
    /// Per-window text and history survive interface switches.
    pub entries: BTreeMap<u32, crate::chat_entry::ChatEntry>,
    pub last_monarch_sender: String,
    pub last_patron_sender: String,
    pub talk_focus: TalkFocus,
    /// Which of the 13 focuses is currently selectable.
    enabled: [bool; 14],
    menu_enabled: [bool; 14],
    pub(crate) is_olthoi: bool,
    talk_focus_notices: Vec<TalkFocusNotice>,
    pub squelch: SquelchDb,
    pub using_turbine_chat: bool,
    /// Read-only NLS environment, installed by the host: a handle to
    /// `dereth_primitives::HostEncoding`, whose Windows implementation is `CP_ACP`, the current OS
    /// ACP. Never saved as a preference. The default here is this workspace's 1252 table,
    /// because a crate with no platform cannot read an ACP.
    pub text_conversion: crate::HostText,
    pub(crate) turbine_client: crate::turbine::TurbineClient,
    pub(crate) turbine_spam: SpamGate,
    /// The last target that can receive selected-target speech.
    pub last_speakable_target: Option<ObjectId>,
    pub last_teller: Option<ObjectId>,
    pub last_teller_name: String,
    pub last_tellee_name: String,
    /// `ChatRoomTracker` — chat types 1..10 -> room ID (not talk-focus ordinals).
    pub chat_rooms: BTreeMap<u32, u32>,
    /// Whether the player wants allegiance chat; initially false. The optional allegiance
    /// chat-window restore producer is not yet ported; room receipt still honors this state.
    pub wants_allegiance_chat: bool,
}

impl ChatState {
    #[must_use]
    pub fn reply_targets(&self) -> dereth_client_contract::chat::window::ReplyTargets {
        let present = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        dereth_client_contract::chat::window::ReplyTargets {
            last_teller: present(&self.last_teller_name),
            monarch: present(&self.last_monarch_sender),
            patron: present(&self.last_patron_sender),
        }
    }

    #[must_use]
    pub fn new() -> Self {
        let mut s = Self::default();
        // Focuses 1 and 2 are always enabled; the rest depend on membership and on Turbine chat.
        s.enabled[TalkFocus::All as usize] = true;
        s.enabled[TalkFocus::Selected as usize] = true;
        s.menu_enabled[TalkFocus::All as usize] = true;
        s
    }

    /// Selects the active talk focus.
    pub fn set_talk_focus(&mut self, f: TalkFocus) {
        self.talk_focus = f;
        self.menu_enabled =
            dereth_client_contract::chat::mainchat::reset_focus_rows(self.enabled, self.is_olthoi);
        if f == TalkFocus::Allegiance {
            self.wants_allegiance_chat = true;
        }
    }

    /// Enables or disables one talk focus and emits the corresponding UI notice.
    pub fn set_talk_focus_enabled(&mut self, f: TalkFocus, enabled: bool) {
        self.enabled[f as usize] = enabled;
        let (row, fallback) = dereth_client_contract::chat::mainchat::focus_enable_transition(
            self.menu_enabled[f as usize],
            self.talk_focus as u32,
            f as u32,
            enabled,
            self.is_olthoi,
        );
        self.menu_enabled[f as usize] = row;
        if fallback {
            self.set_talk_focus(TalkFocus::All);
        }
        // Retail emits even when the bit was already equal. A disable/re-enable pair can
        // move the current UI focus to Say although the final mask is unchanged.
        self.talk_focus_notices.push(TalkFocusNotice {
            focus: f,
            enabled,
            is_olthoi: self.is_olthoi,
        });
    }

    /// Notices have no replay history. The App drains this even without a gameplay subscriber.
    pub fn take_talk_focus_notices(&mut self) -> Vec<TalkFocusNotice> {
        std::mem::take(&mut self.talk_focus_notices)
    }

    /// Reports whether one talk focus is selectable.
    #[must_use]
    pub fn is_talk_focus_enabled(&self, f: TalkFocus) -> bool {
        self.enabled[f as usize]
    }

    /// Recomputes the Turbine-chat talk-focus rows.
    ///
    /// Recomputes 8..13 whenever the Turbine-chat connection or the player's channel options
    /// change, and **disables all six** when Turbine chat is not in use.
    pub fn enable_chat_talk_focuses(&mut self, opts: &crate::player::Options, is_olthoi: bool) {
        self.is_olthoi = is_olthoi;
        let turbine = self.using_turbine_chat;
        let rows = [
            (TalkFocus::General, opts.get(35)),
            (TalkFocus::Trade, opts.get(36)),
            (TalkFocus::Lfg, opts.get(37)),
            (TalkFocus::Roleplay, opts.get(38)),
            (TalkFocus::Society, opts.get(46)),
            (TalkFocus::Olthoi, is_olthoi),
        ];
        for (f, want) in rows {
            self.set_talk_focus_enabled(f, turbine && want);
        }
    }

    /// Which of the fourteen focuses are currently selectable, as one snapshot — what a chat window
    /// needs in order to grey the rows the client greys.
    #[must_use]
    pub fn enabled_focuses(&self) -> [bool; 14] {
        self.enabled
    }

    /// The effective menu availability, including the character's channel restrictions.
    #[must_use]
    pub fn selectable_focuses(&self) -> [bool; 14] {
        self.menu_enabled
    }

    /// Update the selected speech target and its focus availability together.
    pub fn set_speakable_target(&mut self, target: Option<ObjectId>, named: bool) {
        self.last_speakable_target = target;
        self.set_talk_focus_enabled(TalkFocus::Selected, named);
        if !named && self.talk_focus == TalkFocus::Selected {
            self.set_talk_focus(TalkFocus::All);
        }
    }

    /// Applies the communication layer's legality guard before consulting the squelch database.
    ///
    /// Two guards on top of the DB: it returns **squelched** when there is no communication system
    /// at all, and **not squelched** when `type != 1` and the type is not a legal channel.
    #[must_use]
    pub fn is_squelched(&self, character: ObjectId, account: &str, t: u32) -> bool {
        if t != text_type::ALL_CHANNELS && !is_legal_channel(t) {
            return false;
        }
        self.squelch.is_squelched(character, account, t)
    }

    /// Performs the squelch-panel's one supported iteration.
    ///
    /// Three facts, each of which a reasonable guess gets wrong:
    ///
    /// * **Only the character hash is walked.** [`SquelchDb::accounts`] is never iterated by the
    ///   panel, and ACE's `SquelchDB.Write` sends that hash **always empty** and folds account
    ///   squelches into the character hash with `Account = true`. So an account squelch reaches
    ///   the player as a character-hash entry whose flag is set, and `accounts` — which
    ///   [`SquelchDb::is_squelched`] still consults — is dead on the wire.
    /// * **The zone-squelch field is the account flag.** The iterator reads it and hands it to the
    ///   panel, which selects the row's account or character state. ACE calls the same field
    ///   `Account`.
    /// * **An empty entry ends the walk.** The iterator does not *skip* the entry: it returns its
    ///   "no more" result, so everything after an empty entry in
    ///   hash order is invisible. That is transcribed rather than corrected — but note that the
    ///   order is a `BTreeMap`'s here and a `PackableHashTable`'s in the client, so *which*
    ///   entries a stop hides is not reproducible. The shard does not send empty entries:
    ///   `SquelchInfo`'s only constructor in ACE fills four `SquelchMask` limbs.
    #[must_use]
    pub fn squelch_iteration(&self) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for e in self.squelch.characters.values() {
            if e.is_empty() {
                break;
            }
            out.push((e.name.clone(), e.is_zone_squelch != 0));
        }
        out
    }

    /// Receives `0x01F4 SetSquelchDB`.
    ///
    /// This is [`Self::squelch`]'s only production writer: without it the table the chat router
    /// consults on every incoming line is permanently empty and a squelch the shard already
    /// knows about is discarded on each login.
    ///
    /// The handler first assigns the unpacked database. If an object is selected it then queries
    /// whether that object is squelched for `All`, using an empty account name, but **the query's
    /// result is never read**; it is clobbered while releasing the temporary empty string. There
    /// is therefore nothing to transcribe from that second leg.
    ///
    /// Assignment is followed by an update-squelch-panel notice. This is **replacement, not a
    /// merge**: the server owns the
    /// list and re-sends all of it, so a name it has dropped must disappear here -- which is why
    /// this takes the whole DB rather than inserting into the one we hold. The notice belongs to
    /// the squelch *panel*, which this build does not have; nothing is lost by not raising it,
    /// because [`Self::can_hear`] reads the DB directly rather than through a subscription.
    pub fn recv_set_squelch_db(&mut self, m: dereth_protocol::comms::CommunicationSetSquelchDb) {
        self.squelch = SquelchDb::from_wire(&m.0);
    }

    /// Clears the communication system's complete squelch database.
    ///
    /// The clear operation makes three calls and no stores of its own:
    /// empty the account-name map,
    /// empty the character-id map, and
    /// the global entry's clear operation — i.e. all three members of
    /// [`SquelchDb`], which is why this is an assignment of the default rather than a field list.
    ///
    /// **It is called as character-logon phase 2's very first act** — before the null checks,
    /// before the object-system reset, and before the enter-world request. So the client clears
    /// the table on the *entry* edge, and
    /// unconditionally: there is no ending it can be missed on.
    ///
    /// The trailing panel-update notice is what makes the player *see* an empty
    /// list on the next character. This build has no squelch panel and no notice bus that reaches a
    /// UI element, the deferred delivery `recv_set_squelch_db` above describes; nothing is
    /// lost by not raising it, because every reader goes through [`Self::is_squelched`].
    pub fn clear_squelch_db(&mut self) {
        self.squelch = SquelchDb::default();
    }
}

impl SquelchEntry {
    /// One `SquelchInfo` off the wire, expanded into the bit set this model keeps.
    ///
    /// The squelch-message mask is a legacy variable-length integer: a limb count, then
    /// that many little-endian dwords, used as one bit per chat type. Every set bit is carried,
    /// not only the low 128 -- the 128 in [`SquelchEntry::is_squelched`] is the range
    /// the client scans when asked about `All`, not a bound on what the
    /// server may send.
    #[must_use]
    pub fn from_wire(info: &dereth_protocol::comms::SquelchInfo) -> Self {
        let mut types = BTreeSet::new();
        for (limb, word) in info.squelch_msgs.0.iter().enumerate() {
            let Ok(base) = u32::try_from(limb * 32) else {
                break;
            };
            for bit in 0..32 {
                if word & (1u32 << bit) != 0 {
                    types.insert(base + bit);
                }
            }
        }
        Self {
            types,
            is_zone_squelch: info.is_zone_squelch,
            name: info.name.clone(),
        }
    }
}

impl SquelchDb {
    /// Converts an unpacked squelch database into this model.
    ///
    /// Both halves are `PackableHashTable`s decoded with duplicate keys tolerated.
    /// Insertion returns zero without overwriting when a key is already present.
    /// That zero normally signals a decode failure, but duplicate tolerance prevents it
    /// from failing the whole message. So **the first of a duplicate key wins** and the later one
    /// is dropped -- which is `or_insert`, not `collect`, and is the one place a `BTreeMap`
    /// built the obvious way would disagree with retail.
    #[must_use]
    pub fn from_wire(db: &dereth_protocol::comms::SquelchDb) -> Self {
        let mut accounts: BTreeMap<String, u32> = BTreeMap::new();
        for (name, v) in &db.account_hash.entries {
            accounts.entry(name.clone()).or_insert(*v);
        }
        let mut characters: BTreeMap<ObjectId, SquelchEntry> = BTreeMap::new();
        for (id, info) in &db.character_hash.entries {
            characters
                .entry(ObjectId(*id))
                .or_insert_with(|| SquelchEntry::from_wire(info));
        }
        Self {
            accounts,
            characters,
            global: SquelchEntry::from_wire(&db.global_squelch_info),
        }
    }
}

/// The **distance half** of the chat audibility test.
///
/// The player-space `x` and `y` offsets are squared and added at extended precision, then rounded
/// once by storing the sum as `f32`. The radar radius is fetched twice and multiplied by itself.
/// The comparison rejects both `r² < d²` and `r² == d²`, so only `d² < r²` is audible.
///
/// Three details, each of which is easy to get backwards:
///
/// * **`z` is not in the sum.** It is `x² + y²` over the cell-aware *player-space* offset, which
///   means a speaker directly above or below you is at distance zero.
/// * **The range carries no `- 1`.** The radar radius is fetched twice and the two answers are
///   multiplied; nothing is subtracted. The radar's own cull *does*
///   subtract one, transcribed as
///   `(range - 1.0) * (range - 1.0)` in `dereth_ui_screens::mapradar::radar::draw_objects` — so the
///   two consumers of one range deliberately differ by a metre: at 74.5 m outdoors a speaker is
///   **heard** and is **not drawn on the radar**.
/// * **The boundary is exclusive.** Both "less" and "equal" reject, so `d² == r²` returns 0 —
///   exactly on the radius you are **not** heard. An implementation that treats the boundary
///   as inclusive (`<=`) is wrong.
///
/// `radar_radius` is the current indoor/outdoor radar distance: `75.0f` outside and `25.0f`
/// inside. In this rebuild that is
/// `dereth_ui_screens::mapradar::radar::radar_range(hud.player_outside())`, which
/// `dereth-client-model` sits below and cannot name, so the caller passes it.
#[must_use]
pub fn within_earshot(dx: f32, dy: f32, radar_radius: f32) -> bool {
    // Retail keeps both products and their sum at extended precision and rounds once, when the
    // sum is stored as `f32`; `f64` reproduces that and the cast *is* that store. Done in `f32` throughout,
    // each product would round as well, which moves the answer inside the last ulp of the radius —
    // a band about 1.6 µm wide at 75 m, and this boundary is the whole point of the function.
    #[allow(clippy::cast_possible_truncation)]
    let d2 = (f64::from(dx) * f64::from(dx) + f64::from(dy) * f64::from(dy)) as f32;
    f64::from(d2) < f64::from(radar_radius) * f64::from(radar_radius)
}

impl ChatState {
    /// The composite test the chat pipeline applies
    /// before showing a line from another player. It has **two halves**; without the distance
    /// half every emote and every say would be heard across the world.
    ///
    /// The order is the function's own: the id-0 escape, then the squelch, then
    /// [`within_earshot`]. `speaker_player_space` is `None` when the object lookup returns null;
    /// the client then **skips the distance half and returns true**, so a speaker with no
    /// physics object is audible at any range. That escape is real and is also the way this
    /// predicate degrades if a caller forgets to look the speaker up, which is why
    /// [`within_earshot`] is public and pinned on its own as well as through here.
    ///
    /// **`account` is always `""` in the client.** The audibility check builds an empty account
    /// string and passes *that* to the squelch lookup; the caller's account name never reaches it.
    /// The
    /// parameter is kept because [`Self::is_squelched`] is shared with the combat-scroll gate,
    /// which has its own zero id and empty name — but pass `""` here to be faithful.
    #[must_use]
    pub fn can_hear(
        &self,
        speaker: ObjectId,
        account: &str,
        t: u32,
        speaker_player_space: Option<(f32, f32)>,
        radar_radius: f32,
    ) -> bool {
        // A zero speaker id returns true — ahead of *both* halves.
        if speaker.0 == 0 {
            return true;
        }
        if self.is_squelched(speaker, account, t) {
            return false;
        }
        match speaker_player_space {
            Some((dx, dy)) => within_earshot(dx, dy, radar_radius),
            None => true,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The three tell handlers, and the two gates that are *not* on their path
//
// `crate::cmd::table::INITIALIZE_COMMANDS` carries `tell`/`t`/`send`/`whisper`/`w`
// (`do_tell`), `reply`/`r`/`rp` (`do_reply`) and `retell`/`rt` (`do_retell`), and
// `CommandInterp::on_chat_command` classifies all ten `CommandOutcome::Handled`.
//
// The three functions live here, in the model, rather than in the wiring file: they read and
// write `ChatState`'s own `last_teller` / `last_tellee_name`, which is what makes `@reply` and
// `@retell` addressable at all.
// ---------------------------------------------------------------------------------------------

/// The narrow-string whitespace set — **five** characters, `" \n\r\t\x0C"`.
///
/// Observed rather than guessed: every communication-system trim uses the same set, the bytes
/// `20 0A 0D 09 0C 00`.
///
/// The **form feed** is the part worth stating. `crate::cmd::interp::on_chat_command` trims
/// `[' ', '\t', '\r', '\n']`, four characters, so a line whose leading character is a `\x0C` is
/// trimmed by the retail client and not by that interpreter.
pub const WHITESPACE: [char; 5] = [' ', '\n', '\r', '\t', '\u{000C}'];

/// `L"Use comma after the name for targeted chat."` — `do_tell`'s only refusal.
pub const USE_COMMA_AFTER_THE_NAME: &str = "Use comma after the name for targeted chat.";
/// `L"You must specify the text you wish to say!"` — `do_reply` with no text.
pub const YOU_MUST_SPECIFY_THE_TEXT: &str = "You must specify the text you wish to say!";
/// `L"Someone must @tell you first!"` — `do_reply` with no last teller.
pub const SOMEONE_MUST_TELL_YOU_FIRST: &str = "Someone must @tell you first!";
/// `L"You must first provide a name using @tell"` — `do_retell` with no last tellee.
pub const YOU_MUST_FIRST_PROVIDE_A_NAME: &str = "You must first provide a name using @tell";

/// The chat type used to print all four refusals above. Type `0x1A` is special-cased: no timestamp
/// is added and nothing is written to
/// the chat log file — which [`text_type::LOCAL_ERROR`] already encodes.
pub const REFUSAL_CHANNEL: u32 = 0x1A;

/// What one of the three tell handlers decided.
///
/// Four outcomes rather than three, because `do_retell` with an empty line does **nothing at
/// all** — no message, no packet — while `do_reply` with an empty line prints
/// [`YOU_MUST_SPECIFY_THE_TEXT`]. That asymmetry is the client's and is visible in the two
/// handlers: retell jumps straight to the epilogue while reply prints to the scroll. Collapsing
/// the cases would invent a message the client never shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TellOutcome {
    /// Sends `0x005D TalkDirectByName`. **Message
    /// first, name second**, which is the argument order retail sends them in.
    ByName {
        message: String,
        target_name: String,
    },
    /// Sends `0x0032 TalkDirect`.
    ById { message: String, target: ObjectId },
    /// Print the text on chat type `0x1A` at the command's source window.
    Refused(&'static str),
    /// `do_retell`'s empty-line arm: the client prints nothing and sends nothing.
    Nothing,
}

/// Prepares the recipient portion of a tell after the caller has joined the arguments. Argument
/// joining remains in `crate::cmd::interp::join_args`, so this crate does not carry a second
/// copy.
///
/// ```text
/// s = join_args(argv);
/// s.trim(leading, trailing, WHITESPACE);
/// plus = "+"; s.trim(leading, /* trailing */ false, plus);
/// ```
///
/// **The leading `+` strip is a recipient-changing step, which is why it is a named function.**
/// On a live shard a character list may read `+Alba` and `+Aldis`; typing `@tell +Alba, hi`
/// puts **`Alba`** on the wire, not `+Alba`, because the client removes the marker before it
/// ever looks for the comma. A rebuild that skipped this line would send a name the server
/// cannot resolve — silence again — and one that applied it to the *message* would eat a
/// leading `+` a player typed.
#[must_use]
pub fn join_args_as_name(joined: &str) -> &str {
    joined.trim_matches(WHITESPACE).trim_start_matches('+')
}

impl ChatState {
    /// Handles `@tell`, `@t`, `@send`, `@whisper`, and `@w`.
    ///
    /// The joined arguments go through [`join_args_as_name`], then the first comma is found. No
    /// comma, or a comma at index 0, prints *"Use comma after ..."* on chat type 0x1A. Otherwise
    /// the name is everything before the comma and the message everything after it, the message
    /// (only) is trimmed of whitespace, the name is remembered as the last tellee, and the message
    /// is sent to that name by name. The handler returns true.
    ///
    /// Three details that are each a way to address the wrong person (the two `substring`
    /// out-parameters are easy to pair the wrong way round):
    ///
    /// * **the message is trimmed and the name is not.** The single trim targets the
    ///   `comma + 1` substring. So `@tell Bob, hi` sends `"hi"`, and `@tell Bob , hi` sends the name
    ///   `"Bob "` untrimmed — a name the server will not resolve, which is silence rather than
    ///   misdirection, and is retail's behaviour.
    /// * **`comma - s < 1` rejects a comma at index 0**, so `@tell ,hi` is refused rather than
    ///   sent to the empty name.
    /// * **The last tellee name is set from the name, never the message** — retail takes the
    ///   *name* immediately after the trim and passes it by value.
    ///   The retell handler reads it back, so a swap here would misdirect every later `@rt`.
    pub fn do_tell(&mut self, joined_args: &str) -> TellOutcome {
        let s = join_args_as_name(joined_args);
        // The first comma; refused when there is none or it is at index 0.
        let Some(comma) = s.find(',') else {
            return TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME);
        };
        if comma < 1 {
            return TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME);
        }
        let target_name = s[..comma].to_owned();
        let message = s[comma + 1..].trim_matches(WHITESPACE).to_owned();
        // Remember the last tellee for `@retell`.
        self.last_tellee_name.clone_from(&target_name);
        TellOutcome::ByName {
            message,
            target_name,
        }
    }

    /// Handles `@reply`, `@r`, and `@rp`.
    ///
    /// The joined arguments are trimmed of whitespace. Empty text prints *"You must specify the
    /// text you wish to say!"* on chat type 0x1A; otherwise, with no last teller, it prints
    /// *"Someone must @tell you first!"* on 0x1A; otherwise the text is sent to the last teller
    /// by id.
    ///
    /// **Argument joining, not recipient preparation,** is used here, so a leading `+` in
    /// the *text* survives here where `@tell` would have stripped it. And the target is
    /// the last teller's object id — **never the selection**: talk focus 2's
    /// last speakable target is a different field reached from a different function.
    #[must_use]
    pub fn do_reply(&self, joined_args: &str) -> TellOutcome {
        let text = joined_args.trim_matches(WHITESPACE);
        if text.is_empty() {
            return TellOutcome::Refused(YOU_MUST_SPECIFY_THE_TEXT);
        }
        // Retail tests the last teller against zero. `None` and `Some(ObjectId(0))` are the same
        // zero on the client's side, where the field is a bare `ulong`.
        match self.last_teller {
            Some(target) if target.0 != 0 => TellOutcome::ById {
                message: text.to_owned(),
                target,
            },
            _ => TellOutcome::Refused(SOMEONE_MUST_TELL_YOU_FIRST),
        }
    }

    /// Handles `@retell` and `@rt`.
    ///
    /// The joined arguments are trimmed of whitespace. Empty text does nothing at all; otherwise,
    /// with no last tellee name, it prints *"You must first provide a name using @tell"* on chat
    /// type 0x1A; otherwise the text is sent by name to the last tellee.
    ///
    /// It does **not** set the last tellee name, so `@rt` re-uses the name `@tell` stored and never replaces it.
    #[must_use]
    pub fn do_retell(&self, joined_args: &str) -> TellOutcome {
        let text = joined_args.trim_matches(WHITESPACE);
        if text.is_empty() {
            return TellOutcome::Nothing;
        }
        if self.last_tellee_name.is_empty() {
            return TellOutcome::Refused(YOU_MUST_FIRST_PROVIDE_A_NAME);
        }
        TellOutcome::ByName {
            message: text.to_owned(),
            target_name: self.last_tellee_name.clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// `@e` / `@em` / `@emote` / `@me`.
//
// `crate::cmd::table::INITIALIZE_COMMANDS` maps all four names to the emote handler, and
// `CommandInterp::on_chat_command` resolves them (and rewrites `:x` and `;x` into `@emote x`).
// ---------------------------------------------------------------------------------------------

/// What the emote command handler decided.
///
/// Two outcomes, not three: emote has **no refusal string**. Reply answers an empty line with
/// [`YOU_MUST_SPECIFY_THE_TEXT`], while emote on
/// an empty line returns `true` and does nothing at all — no packet, no message, no error. That
/// asymmetry between two adjacent handlers is the client's, and inventing a refusal here would
/// show a line retail never shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmoteOutcome {
    /// Sends `0x01DF Emote`, an ordinary game action on
    /// the global `OrderedActionHeader` counter. The request blob contains the literal opcode bytes
    /// `df 01 00 00`.
    Send(String),
    /// The one-character (empty) line: returns true with nothing sent.
    Nothing,
}

/// Implements the whole emote handler: an empty joined string returns success and sends nothing;
/// every nonempty string is sent unchanged.
///
/// **There is no `trim` in it**, which is the one place it differs from the say handler besides the
/// refusal: say trims both ends with the shared whitespace set before its empty test, and emote
/// does not. It does not matter today, because
/// `crate::cmd::interp::find_all_words` has already dropped every empty token and `join_args`
/// re-joined them with single spaces — but it is why this takes the *joined* string and does not
/// trim it, and a caller that ever hands it raw text would see the difference.
///
/// The `argc`/`argv` half is `crate::cmd::interp::join_args`'s and the caller applies it,
/// exactly as [`ChatState::do_tell`] documents for [`join_args_as_name`]. (Named in prose rather than
/// linked.)
#[must_use]
pub fn do_emote(joined_args: &str) -> EmoteOutcome {
    if joined_args.is_empty() {
        return EmoteOutcome::Nothing;
    }
    EmoteOutcome::Send(joined_args.to_owned())
}

/// Checks whether a message is safe for the Turbine-chat path.
///
/// ```text
/// if length(text) >= 256 return false;        // counted in characters, before anything else
/// lower = copy(text); _strlwr(lower);
/// bad  = strstr(lower, "<tell:") != NULL;
/// bad |= strstr(text,  "\n")     != NULL;      // the ORIGINAL, not the lower-cased copy
/// return !bad;
/// ```
///
/// **Two measured facts that are easy to get wrong.**
///
/// 1. It rejects **three** things, not one: a line of 256 characters or more (the stored length
///    counts the terminator, and more than 256 of those is unsafe), the `<tell:` hyperlink markup
///    *case-insensitively*, and any embedded newline.
/// 2. It is **not** on the tell path, or on any speech path. Its only two callers in the whole
///    binary are the direct Turbine-chat sender and the room-send callback. Ordinary tell, reply,
///    retell, and public-chat handlers call neither this gate nor the spam gate.
///
/// Wired only on the world's Turbine-chat send path; ordinary speech and tells do not pass this
/// gate.
#[must_use]
pub fn is_message_safe(text: &str) -> bool {
    // The line is a one-byte-per-character string, so its length is its character count.
    if text.chars().count() >= MAX_SAFE_MESSAGE_CHARS {
        return false;
    }
    // `_strlwr` is the C-locale lower-caser, which touches `A`..`Z` only; the needle is ASCII, so
    // an ASCII fold is exactly equivalent for the search and does not disturb cp1252 high bytes.
    !text.to_ascii_lowercase().contains("<tell:") && !text.contains('\n')
}

/// A Turbine-chat line this many characters long, or longer, is refused as unsafe.
pub const MAX_SAFE_MESSAGE_CHARS: usize = 256;

/// The last-check time and message-count fields used by the Turbine-chat leaky bucket.
///
/// Like [`is_message_safe`], wired only on the Turbine path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpamGate {
    /// The real-time clock in whole seconds, or `0` before the first check.
    pub last_spam_check: i32,
    /// The chat-message count — the bucket, clamped at zero from below and never from above.
    pub chat_message_count: i32,
}

impl SpamGate {
    /// ```text
    /// now = real_time_seconds();
    /// last = last_spam_check; last_spam_check = now;
    /// if (last == 0) return false;                  // the first message is always allowed
    /// chat_message_count += 1 - (now - last);
    /// if (chat_message_count < 0) chat_message_count = 0;
    /// return chat_message_count > 0;
    /// ```
    ///
    /// The store happens **before** the `last == 0` test, so the first call arms the clock even
    /// though it answers "not spam". One message per second sustained, with no burst allowance
    /// beyond that first one.
    ///
    /// **And it does not forgive.** "One message per second sustained" reads
    /// like a rate limit you can come back from at that rate; the arithmetic says otherwise. Once
    /// two messages land in the same second the bucket sits at 1, and one a second thereafter adds
    /// `1 - 1 == 0` — so the bucket stays positive and **every** following message is dropped until
    /// a **two**-second gap drains it. From a clean bucket, one a second never trips at all.
    pub fn is_message_spam(&mut self, now_seconds: i32) -> bool {
        let last = self.last_spam_check;
        self.last_spam_check = now_seconds;
        if last == 0 {
            return false;
        }
        // Native long arithmetic wraps before the signed negative/>0 comparisons.
        self.chat_message_count = self
            .chat_message_count
            .wrapping_add(1_i32.wrapping_sub(now_seconds.wrapping_sub(last)));
        if self.chat_message_count < 0 {
            self.chat_message_count = 0;
        }
        self.chat_message_count > 0
    }
}

impl crate::world::World {
    /// The named, other player currently selected by the chat target menu.
    #[must_use]
    pub fn selected_chat_player(&self) -> Option<ObjectId> {
        let id = self.selected.filter(|id| Some(*id) != self.player)?;
        let object = self.weenie(id)?;
        (object.is_player()
            && !object
                .object_name(crate::weenie::NameType::Appropriate)
                .is_empty())
        .then_some(id)
    }

    /// Recomputes chat talk focuses from the two notices that drive it in the client.
    ///
    /// Its two callers, and there are no others:
    ///
    /// * the player-description handler — its whole body is this call, so it runs once
    ///   per `0x0013 Login_PlayerDescription`;
    /// * the character-option handler — the same call, guarded on the option being
    ///   one of `HearGeneralChat`, `HearTradeChat`, `HearLFGChat`, `HearRoleplayChat`,
    ///   `HearSocietyChat`.
    ///
    /// **The important thing about this function is what it does *not* read.** It is a pure
    /// function of the Turbine-chat connection state, five of the player's own character options,
    /// and whether the player is Olthoi. Nothing in it consults membership, the shard, or any message
    /// about a channel having been joined — so *"the shard says the character entered General,
    /// Trade and LFG, and the menu rows are greyed"* is not a contradiction: entering a channel and
    /// being able to select it as a talk focus are decided in two different places, and this one
    /// is entirely local.
    ///
    /// `is_olthoi` is derived from the player's heritage group against the two Olthoi ids. This
    /// build cannot answer that yet; `false` is the value for every
    /// character in the corpus and is passed by the caller so the gap is visible at the seam rather
    /// than buried here.
    pub fn enable_chat_talk_focuses(&mut self, is_olthoi: bool) -> [bool; 14] {
        let opts = self.player_system.options;
        self.chat.enable_chat_talk_focuses(&opts, is_olthoi);
        self.chat.enabled_focuses()
    }

    /// Sends **`0x0058 ModifyCharacterSquelch`** for the current speakable target.
    ///
    /// The message is the whole of it: the client does **not** mirror the change into its own
    /// squelch database here. The server owns the list and re-sends it, which is why a squelch
    /// that the server refuses simply does not take effect in the client either.
    pub fn modify_character_squelch(
        &mut self,
        req: &mut dyn crate::RequestSink,
        character: ObjectId,
        add: bool,
        account: &str,
        msg_type: u32,
    ) {
        req.send(crate::Request::ModifyCharacterSquelch(
            dereth_protocol::comms::CommunicationModifyCharacterSquelch {
                add: i32::from(add),
                character_id: character,
                character_name: account.to_owned(),
                msg_type,
            },
        ));
    }

    /// Sends **`0x0059 ModifyAccountSquelch`** with `add` and `name`.
    /// The sibling of [`Self::modify_character_squelch`].
    ///
    /// The request packs the opcode, `add`, and one string, and nothing else: there is
    /// no character id and no message type. That asymmetry is the shard's, not this build's —
    /// ACE's `GameActionModifyAccountSquelch.Handle` reads exactly those two fields, and its
    /// comment in `SquelchDB.Write` records why account squelches are always `AllChannels`:
    /// *"the client forces account squelches to be AllMessages"*, which is this message having
    /// nowhere to put a channel.
    ///
    /// The message is the whole of it, for the reason the character version's doc gives: the
    /// server owns the list and re-sends the whole DB, so a squelch the server refuses simply
    /// does not take effect in the client either.
    pub fn modify_account_squelch(
        &mut self,
        req: &mut dyn crate::RequestSink,
        add: bool,
        name: &str,
    ) {
        req.send(crate::Request::ModifyAccountSquelch(
            dereth_protocol::comms::CommunicationModifyAccountSquelch {
                add: i32::from(add),
                character_name: name.to_owned(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §1's set of fourteen.
    #[test]
    fn exactly_fourteen_types_are_squelchable() {
        assert_eq!(LEGAL_CHANNELS.len(), 14);
        for t in 0..=0x21u32 {
            assert_eq!(
                is_legal_channel(t),
                LEGAL_CHANNELS.contains(&t),
                "type 0x{t:X} ({})",
                log_text_type_name(t)
            );
        }
        assert!(!is_legal_channel(text_type::SYSTEM));
        assert!(!is_legal_channel(text_type::LOCAL_ERROR));
        assert!(is_legal_channel(text_type::COMBAT));
    }

    /// Oracle: §1's name table, including the five ranges that render as `Unknown`.
    #[test]
    fn the_unnamed_types_have_no_name() {
        assert_eq!(log_text_type_name(25), "Salvaging");
        assert_eq!(log_text_type_name(0x1F), "Admin_Tell");
        for t in [0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x20, 0x21] {
            assert_eq!(log_text_type_name(t), "Unknown", "0x{t:X}");
        }
    }

    /// Oracle: the `All` pseudo-type needs **all 128 bits**.
    #[test]
    fn the_all_pseudo_type_needs_every_bit() {
        let mut e = SquelchEntry::default();
        assert!(e.is_empty());
        e.types.insert(text_type::COMBAT);
        assert!(!e.is_empty());
        assert!(e.is_squelched(text_type::COMBAT));
        assert!(!e.is_squelched(text_type::ALL_CHANNELS));
        e.squelch_everything();
        assert!(e.is_squelched(text_type::ALL_CHANNELS));
    }

    /// Oracle: the client lookup order — the global entry first, then the
    /// `Spellcasting` exemption, then the account hash, then the character hash.
    #[test]
    fn spellcasting_is_never_account_squelched() {
        let mut db = SquelchDb::default();
        db.accounts.insert("badguy".into(), 1);
        assert!(db.is_squelched(ObjectId(5), "badguy", text_type::SPEECH));
        assert!(
            !db.is_squelched(ObjectId(5), "badguy", text_type::SPELLCASTING),
            "Spellcasting is exempt from the account hash"
        );
        // The global entry still wins over the exemption, because it is tested first.
        db.global.types.insert(text_type::SPELLCASTING);
        assert!(db.is_squelched(ObjectId(5), "badguy", text_type::SPELLCASTING));
    }

    /// Oracle: the communication-layer guard — an illegal channel is
    /// never squelched, so a squelched speaker's *system* lines still get through.
    #[test]
    fn a_squelched_speakers_lines_are_dropped_only_on_legal_channels() {
        let mut s = ChatState::new();
        s.squelch.characters.insert(
            ObjectId(7),
            SquelchEntry {
                types: [2u32].into(),
                ..SquelchEntry::default()
            },
        );
        // At the listener's own feet and outdoors, so the distance half admits every one of the
        // three and this stays a test about squelch. Deliberately **not** `None` — that is
        // `CanHear`'s no-physics-object escape, and a squelch test resting on it would be green on
        // a build with no distance half at all, which is the build this file used to describe.
        let here = Some((0.0f32, 0.0f32));
        let r = 75.0f32;
        assert!(!s.can_hear(ObjectId(7), "acct", text_type::SPEECH, here, r));
        assert!(s.can_hear(ObjectId(8), "acct", text_type::SPEECH, here, r));
        assert!(
            s.can_hear(ObjectId(7), "acct", text_type::SYSTEM, here, r),
            "System is not a legal channel and cannot be squelched"
        );
    }

    /// Oracle: §4's talk-focus table and the drop-down order, which is not the id order.
    #[test]
    fn talk_focus_ids_and_menu_order_match_the_document() {
        assert_eq!(TalkFocus::All as u32, 1);
        assert_eq!(TalkFocus::Olthoi as u32, 13);
        assert_eq!(TalkFocus::from_raw(7), Some(TalkFocus::Allegiance));
        assert_eq!(TalkFocus::from_raw(14), None);
        assert_eq!(TALK_FOCUS_MENU_ORDER[0], TalkFocus::Monarch);
        assert_eq!(TALK_FOCUS_MENU_ORDER[3], TalkFocus::All);
        assert_eq!(TALK_FOCUS_MENU_ORDER.len(), 12);
    }

    /// Oracle: the shared whitespace bytes are `20 0A 0D 09 0C 00`.
    #[test]
    fn the_whitespace_set_is_five_characters_and_includes_the_form_feed() {
        assert_eq!(WHITESPACE, [' ', '\n', '\r', '\t', '\u{000C}']);
        assert_eq!("\u{000C}hi\u{000C}".trim_matches(WHITESPACE), "hi");
        // The four-character set the command interpreter trims with would leave both of those on.
        assert_eq!(
            "\u{000C}hi\u{000C}".trim_matches([' ', '\t', '\r', '\n']),
            "\u{000C}hi\u{000C}"
        );
    }

    /// Pinned behavior: `trim(both, whitespace)` then
    /// `trim(leading, /* trailing */ false, "+")`, both of which loop
    /// (each trim advances while the current character belongs to its set).
    ///
    /// **The `+` is the shard's own.** All six capture sessions that reach character select list
    /// this account's characters as `+Alba`, `+Aldis`, `+Aldwyne`, `+Magic`, `+Tarinell`,
    /// `+Nobody` — every name carries the marker — so `@tell +Alba, hi` is exactly what a player
    /// here types, and this line is what decides whether the name on the wire is `Alba` or
    /// `+Alba`. The client sends `Alba`.
    #[test]
    fn join_args_as_name_trims_whitespace_then_any_leading_plus() {
        assert_eq!(join_args_as_name("  +Alba, hi  "), "Alba, hi");
        assert_eq!(
            join_args_as_name("++Alba, hi"),
            "Alba, hi",
            "the trim loops"
        );
        assert_eq!(
            join_args_as_name("Alba, +hi"),
            "Alba, +hi",
            "only a *leading* plus"
        );
        assert_eq!(join_args_as_name("+"), "");
    }

    /// Oracle: the before-comma and after-comma substrings, the single trim applied to the
    /// second of them, the last tellee being set from the *name*, and
    /// `Request::TalkDirectByName(msg, name)`.
    #[test]
    fn do_tell_splits_on_the_first_comma_and_trims_only_the_message() {
        let mut s = ChatState::new();
        assert_eq!(
            s.do_tell("Alba, well met"),
            TellOutcome::ByName {
                message: "well met".into(),
                target_name: "Alba".into()
            }
        );
        assert_eq!(
            s.last_tellee_name, "Alba",
            "a successful tell remembers its recipient"
        );

        // The name is **not** trimmed. This is retail's behaviour and it fails safe: an unresolvable
        // name is silence, where trimming the wrong operand would be a line to someone else.
        assert_eq!(
            s.do_tell("Alba , well met"),
            TellOutcome::ByName {
                message: "well met".into(),
                target_name: "Alba ".into()
            }
        );

        // The *first* comma wins, so a comma in the message survives.
        assert_eq!(
            s.do_tell("Alba, well met, friend"),
            TellOutcome::ByName {
                message: "well met, friend".into(),
                target_name: "Alba".into()
            }
        );

        // A name with spaces in it — every one of the corpus's thirteen speakers has one.
        assert_eq!(
            s.do_tell("Pathwarden Koro Ijida, hello"),
            TellOutcome::ByName {
                message: "hello".into(),
                target_name: "Pathwarden Koro Ijida".into(),
            }
        );
    }

    /// Oracle: `(int)comma - (int)s < 1`, and `L"Use comma after the name for
    /// targeted chat."`.
    #[test]
    fn do_tell_refuses_a_missing_comma_and_a_comma_at_index_zero() {
        let mut s = ChatState::new();
        assert_eq!(
            s.do_tell("Alba hello"),
            TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME)
        );
        assert_eq!(
            s.do_tell(",hello"),
            TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME)
        );
        assert_eq!(
            s.do_tell(""),
            TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME)
        );
        // `+` is eaten first, so `@tell +, hi` is a comma at index 0 and not a tell to "+".
        assert_eq!(
            s.do_tell("+, hi"),
            TellOutcome::Refused(USE_COMMA_AFTER_THE_NAME)
        );
        assert!(
            s.last_tellee_name.is_empty(),
            "a refused tell stores no name"
        );
    }

    /// Pinned behavior: test the text for empty, require a nonzero last-teller id, then send the
    /// text directly to that id.
    #[test]
    fn do_reply_addresses_the_last_teller_and_nothing_else() {
        let mut s = ChatState::new();
        assert_eq!(
            s.do_reply("   "),
            TellOutcome::Refused(YOU_MUST_SPECIFY_THE_TEXT)
        );
        assert_eq!(
            s.do_reply("hello"),
            TellOutcome::Refused(SOMEONE_MUST_TELL_YOU_FIRST)
        );

        // A zero id is the client's own "nobody has told me", where this crate has an Option.
        s.last_teller = Some(ObjectId(0));
        assert_eq!(
            s.do_reply("hello"),
            TellOutcome::Refused(SOMEONE_MUST_TELL_YOU_FIRST)
        );

        s.last_teller = Some(ObjectId(0x5000_0AB1));
        assert_eq!(
            s.do_reply("  hello  "),
            TellOutcome::ById {
                message: "hello".into(),
                target: ObjectId(0x5000_0AB1)
            }
        );
        // The empty test is applied **before** the teller test, so a blank `@r` with a teller
        // still says "you must specify the text".
        assert_eq!(
            s.do_reply(""),
            TellOutcome::Refused(YOU_MUST_SPECIFY_THE_TEXT)
        );
        // Plain argument joining, not `join_args_as_name`: `@r` does not eat a leading plus from the text.
        assert_eq!(
            s.do_reply("+1"),
            TellOutcome::ById {
                message: "+1".into(),
                target: ObjectId(0x5000_0AB1)
            }
        );
        // The selection is irrelevant to `@reply`; only the last teller decides.
        s.last_speakable_target = Some(ObjectId(0x8000_0DE9));
        assert_eq!(
            s.do_reply("hello"),
            TellOutcome::ById {
                message: "hello".into(),
                target: ObjectId(0x5000_0AB1)
            }
        );
    }

    /// Pinned behavior: the empty arm returns silently, a missing last-tellee name produces the
    /// refusal, and the handler never replaces the remembered name.
    #[test]
    fn do_retell_is_silent_on_an_empty_line_and_never_replaces_the_name() {
        let mut s = ChatState::new();
        assert_eq!(
            s.do_retell("  "),
            TellOutcome::Nothing,
            "retail prints nothing here"
        );
        assert_eq!(
            s.do_retell("hi"),
            TellOutcome::Refused(YOU_MUST_FIRST_PROVIDE_A_NAME)
        );

        let _ = s.do_tell("Alba, first");
        assert_eq!(
            s.do_retell(" second "),
            TellOutcome::ByName {
                message: "second".into(),
                target_name: "Alba".into()
            }
        );
        assert_eq!(
            s.last_tellee_name, "Alba",
            "@rt does not set the last tellee name"
        );
        // And the empty-line arm still stores nothing and sends nothing once a name exists.
        assert_eq!(s.do_retell(""), TellOutcome::Nothing);
        assert_eq!(s.last_tellee_name, "Alba");
    }

    /// Pinned behavior: the `_strlwr`'d copy is searched for `"<tell:"` and the
    /// **original** searched for `"\n"`.
    ///
    /// The doc names only the first of the two; the newline is the correction.
    #[test]
    fn is_message_safe_rejects_the_tell_markup_case_insensitively_and_any_newline() {
        assert!(is_message_safe("hello from the capture proxy"));
        assert!(!is_message_safe("look: <tell:Alba>"));
        assert!(
            !is_message_safe("look: <TELL:Alba>"),
            "the search is on a lower-cased copy"
        );
        assert!(!is_message_safe("look: <TeLl:Alba>"));
        assert!(is_message_safe("<tell>"), "only the colon form is markup");
        assert!(!is_message_safe("two\nlines"));
        assert!(is_message_safe("a carriage\rreturn is not tested"));
    }

    /// A Turbine-chat line of 255 characters is safe and one of 256 is not, whatever it says;
    /// the count is of characters, so one accented letter counts once.
    #[test]
    fn is_message_safe_refuses_a_line_of_256_characters_or_more() {
        assert!(is_message_safe(&"a".repeat(255)));
        assert!(!is_message_safe(&"a".repeat(256)));
        assert!(!is_message_safe(&"a".repeat(1000)));
        assert!(is_message_safe(&format!("{}é", "a".repeat(254))));
        assert!(!is_message_safe(&format!("{}é", "a".repeat(255))));
        assert_eq!(MAX_SAFE_MESSAGE_CHARS, 256);
    }

    /// Oracle: the five-line bucket algorithm quoted on [`SpamGate::is_message_spam`].
    #[test]
    fn the_spam_gate_allows_one_message_per_second_and_never_the_first() {
        // One a second from a clean bucket never trips: `count += 1 - 1 == 0`.
        let mut g = SpamGate::default();
        assert!(
            !g.is_message_spam(100),
            "the first message is always allowed"
        );
        assert_eq!(g.last_spam_check, 100, "and it still arms the clock");
        for t in 101..110 {
            assert!(!g.is_message_spam(t), "one per second at t={t}");
        }
        assert_eq!(g.chat_message_count, 0);

        // A second message in the **same** second as the last one trips it: `count += 1 - 0`.
        assert!(g.is_message_spam(109));
        assert_eq!(g.chat_message_count, 1);

        // **And one a second is then not enough to recover.** The bucket holds at 1, which is
        // `> 0`, so every following message is dropped until a **two**-second gap drains it.
        // That is the client's arithmetic and it is worth an assertion of its own, because
        // "one message per second sustained" reads like a rate limit that forgives.
        assert!(g.is_message_spam(110));
        assert!(g.is_message_spam(111));
        assert_eq!(g.chat_message_count, 1);

        // Two seconds of quiet: `count += 1 - 2 = -1`, clamped to 0.
        assert!(!g.is_message_spam(113));
        assert_eq!(g.chat_message_count, 0);

        // Three in one second: the second and the third both trip, and the bucket rises.
        assert!(g.is_message_spam(113));
        assert!(g.is_message_spam(113));
        assert_eq!(g.chat_message_count, 2);
    }

    /// Oracle: §4 — focuses 1 and 2 are always enabled, and 8..13 are all disabled when Turbine chat
    /// is not in use, whatever the options say.
    #[test]
    fn the_turbine_focuses_are_all_off_without_turbine_chat() {
        let mut s = ChatState::new();
        let opts = crate::player::Options::default(); // Hear{General,Trade,LFG,Roleplay} are on
        assert!(s.is_talk_focus_enabled(TalkFocus::All));
        assert!(s.is_talk_focus_enabled(TalkFocus::Selected));

        s.using_turbine_chat = false;
        s.enable_chat_talk_focuses(&opts, true);
        for f in [
            TalkFocus::General,
            TalkFocus::Trade,
            TalkFocus::Lfg,
            TalkFocus::Roleplay,
            TalkFocus::Society,
            TalkFocus::Olthoi,
        ] {
            assert!(!s.is_talk_focus_enabled(f), "{f:?}");
        }

        s.using_turbine_chat = true;
        s.enable_chat_talk_focuses(&opts, false);
        assert!(s.is_talk_focus_enabled(TalkFocus::General));
        assert!(s.is_talk_focus_enabled(TalkFocus::Trade));
        assert!(
            !s.is_talk_focus_enabled(TalkFocus::Society),
            "HearSocietyChat is off by default"
        );
        assert!(!s.is_talk_focus_enabled(TalkFocus::Olthoi));
    }
    /// Behaviour: chat.focus-state
    #[test]
    fn allegiance_desire_survives_another_focus_and_olthoi_rows_keep_notice_order() {
        let mut chat = ChatState::new();
        chat.set_talk_focus(TalkFocus::Allegiance);
        chat.set_talk_focus(TalkFocus::All);
        assert!(chat.wants_allegiance_chat);
        chat.recv_chat_room_tracker(dereth_protocol::comms::ChatRoomMembership {
            allegiance_room: 42,
            ..Default::default()
        });
        assert_eq!(chat.talk_focus, TalkFocus::Allegiance);
        assert_eq!(chat.chat_rooms[&1], 42);
        chat.is_olthoi = true;
        chat.set_talk_focus(TalkFocus::All);
        assert_eq!(
            chat.selectable_focuses()
                .iter()
                .enumerate()
                .filter_map(|(i, on)| on.then_some(i))
                .collect::<Vec<_>>(),
            [1, 13]
        );
        chat.set_talk_focus_enabled(TalkFocus::Selected, true);
        assert!(chat.selectable_focuses()[2]);
        chat.set_talk_focus(TalkFocus::Selected);
        assert!(!chat.selectable_focuses()[2]);
        chat.set_talk_focus_enabled(TalkFocus::Selected, false);
        assert_eq!(
            chat.talk_focus,
            TalkFocus::Selected,
            "an already disabled row does not reset focus"
        );
        chat.set_talk_focus_enabled(TalkFocus::Selected, true);
        chat.set_talk_focus_enabled(TalkFocus::Selected, false);
        assert_eq!(chat.talk_focus, TalkFocus::All);
        chat.set_talk_focus(TalkFocus::Monarch);
        chat.set_talk_focus_enabled(TalkFocus::Monarch, false);
        assert_eq!(
            chat.talk_focus,
            TalkFocus::Monarch,
            "an Olthoi-forbidden row does not take the normal fallback arm"
        );
    }
}
