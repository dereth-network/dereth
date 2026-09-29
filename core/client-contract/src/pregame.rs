//! The pre-game view: what the application knows before a character is in the world, and what the
//! pre-game screens hand back.
//!
//! The data-patch, character-management, creation, disconnected and (for its logout prompt and
//! barber tables) gameplay screens read a handful of facts that the client keeps in process
//! globals: whether the socket is connected, the character list, the world name, the last
//! verification code. A rebuild has to hand them over explicitly, and this is the value it hands
//! over. The host assembles one per frame from the session and delivers it to the current screen
//! at the start of the UI frame, before input and deliveries.
//!
//! The other direction is here too: the player-session operations the character screens ask for
//! (`CharacterAction`, `CharGenAction`) travel back as [`crate::UiRequest`]s.

use dereth_primitives::ObjectId;

use crate::persist::CharacterSet;

/// What the application knows and the pre-game screens read out of globals.
///
/// The *values* come from the client session; the *meanings* are the screens'.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PregameView {
    /// Whether the network layer reports a live connection, read by the data-patch screen each frame.
    pub connected: bool,
    /// Whether a packet controller exists at all. With `--connect` it does; without it there is no
    /// network in this build, and the screen's own documented condition
    /// (`!has_packet_controller || received_set`) is what carries the flow forward.
    pub has_packet_controller: bool,
    /// The UI persistent data's received-set flag — false until the first `0xF658` arrives.
    pub received_set: bool,
    /// The character list itself, applied through the persistent-data object's
    /// character-set notice once per notice.
    pub character_set: Option<CharacterSet>,
    /// How many `0xF658` **notices** the application has raised.
    ///
    /// The character set is a notice, and a notice arrives once — but [`PregameView`] is a
    /// per-frame snapshot, and comparing the *set* with last frame's would miss a repeat. ACE
    /// re-sends an identical list on log-off (`0xF653` and `0xF658` in the same instant, all five
    /// recorded sessions), and an identical value is not an edge: the second notice would be
    /// silently dropped. A count changes even when the payload does not.
    pub character_set_notices: u32,
    /// The world name from `0xF7E1 Login_WorldInfo`.
    ///
    /// The character-management screen reads it from the client interface and pushes it into
    /// element `0x1000039B`, so like the character set it
    /// is a process global that a rebuild has to hand over.
    pub world_name: Option<String>,
    /// Whether the session reached the CharSel → InGame edge.
    pub in_world: bool,
    /// The name of the character this session entered the world as.
    ///
    /// The automatic-layout path reads the player's singular object name, and it is
    /// half of the automatic layout file's name. This build has no world-object singleton to ask,
    /// so the application records the name it entered with — the same value, one step earlier.
    pub entered_character: Option<String>,
    /// The server-died or character-error notice supplies the
    /// error string carried into the queued error UI mode (string table `0x10000002`).
    pub error: Option<String>,
    /// `0xF643 Character_CharGenVerificationResponse`'s code, applied once on the edge.
    ///
    /// The verification handler forwards this code to character management and the creation
    /// wizard through their notice handlers.
    pub chargen_response: Option<u32>,
    /// How many `0xF643`s have arrived — **a notice is a count, not a value**.
    ///
    /// The same holds as for `0xF658`: two refusals in a row carry the same code, and a
    /// value comparison cannot see the second. The char-gen verification notice
    /// is raised on **every** arm of the verification-response handler, so a player
    /// who presses RESTORE, is told the name is taken, and presses it again must get their second
    /// please-wait modal taken down too.
    pub chargen_response_notices: u32,
    /// `0xF658`'s Throne of Destiny flag, which
    /// gates the Viamontian heritage and the Sanamar start area.
    pub account_has_tod: bool,
    /// `DDD_PatchtimeEnd`: there is nothing to patch.
    pub patch_finished: bool,
    /// Data-patch notifications, in arrival order, since the last frame.
    ///
    /// The client's route is a plugin registered by the data-patch screen; the cache walks the
    /// plugin array on every
    /// event. Here the events come off the session's queue-5 dispatcher, are queued for one frame,
    /// and are delivered to whichever screen is up — which is the same "only while it is up"
    /// lifetime the plugin registration gives, because a screen that is not the data-patch screen
    /// ignores them.
    pub ddd: Vec<DddEvent>,
    /// The independent client and C-runtime random seeds from which the opening character is
    /// drawn, decided once when the host brings the UI up.
    ///
    /// The client generator is seeded from the wall clock unconditionally. The C runtime's seed
    /// is set from the clock only if the sound device initialised, so a silent client keeps the
    /// runtime's default of 1. `None` until the shell has come up, in which case the wizard keeps
    /// its own `(1, 1)` — which is what a headless run ends up with anyway, and is why a headless
    /// roll is reproducible.
    pub chargen_seeds: Option<(i32, u32)>,
}

/// A DDD event → the string id the data-patch screen shows for it, from table enum `0x10000002`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DddEvent {
    /// `DDD_PatchtimeInterrogation` → `ID_DataPatch_Interrogation`.
    PatchtimeInterrogation,
    /// `DDD_PatchtimePending` → `ID_DataPatch_Waiting` + variable `token_total`.
    PatchtimePending { total: u64 },
    /// `DDD_PatchtimeBegin` → `ID_DataPatch_Patching`; stores the expected byte count.
    PatchtimeBegin { expected: u64 },
    /// `DDD_DataDownloaded` → accumulate, then `ID_DataPatch_PatchProgress`.
    DataDownloaded { bytes: u64 },
    /// `DDD_PatchtimeEnd` → unregister the plugin, force the patch level to 1.0,
    /// `ID_DataPatch_PatchingDone`.
    PatchtimeEnd,
}

/// A player-session operation the character-management screen asks for. The screen has no
/// session; the host does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterAction {
    /// Log a character on — the two-step `0xF7C8` / `0xF657` exchange.
    LogOn(ObjectId),
    /// Delete a character — `0xF655`, addressed by **slot**.
    Delete(ObjectId),
    /// Restore a character — `0xF7D9`, on net queue 2.
    Restore(ObjectId),
}

/// What the character-generation wizard asks the host for.
#[derive(Debug, Clone, PartialEq)]
pub enum CharGenAction {
    /// The proto panel's send char gen result — `0xF656`.
    SendCharGenResult(Box<CharGenResultData>),
    /// Log on the character, the update's awaiting-character-set-for-login branch: the new
    /// character goes straight into the world.
    LogOn(ObjectId),
}

/// The values the char-gen result writes, in that struct's own vocabulary.
///
/// The UI crates may not depend on `dereth-protocol` (the wire is the session's), so this is
/// the hand-off type: the host copies it field for field into
/// `dereth_protocol::login::CharGenResult`, exactly as `character_set_from_login` copies the other
/// direction. Every field name is the client's.
#[derive(Debug, Clone, PartialEq)]
pub struct CharGenResultData {
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
    /// `TOTAL_NUM_SKILLS` entries, indexed by skill id.
    pub skill_advancement_classes: Vec<i32>,
    pub name: String,
    pub start_area: u32,
    pub is_admin: i32,
    pub is_envoy: i32,
}
