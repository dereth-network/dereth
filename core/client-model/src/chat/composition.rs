//! The chat **display** templates: the six built-in format strings the client composes an
//! incoming speech line with, and the two transforms scroll-text submission applies on the way to
//! the scrollback.
//!
//! The separator between speaker and message is not a string-table row. It is six `printf`
//! templates built into the client, reproduced here byte for byte, so the speaker name is part of
//! the line's body rather than a separate prefix.
//!
//! # Where each one is used
//!
//! | template | composed by |
//! |---|---|
//! | `YOU_SAY` | ordinary speech where `senderID == player` |
//! | `SAYS` | ordinary and ranged speech from another sender |
//! | `SAYS_CLICKABLE` | both of those, when the sender is in `CLICKABLE_PLAYER_IDS` |
//! | `YOU_THINK` | direct speech where `senderID == targetID` |
//! | `TELLS_YOU` | the same, for a sender outside the clickable range |
//! | `TELLS_YOU_CLICKABLE` | the same, inside it |
//! | `GARBLED` | both, when the listener cannot understand the speaker's language |
//!
//! Every one of them is **narrow**: the format and arguments are byte strings, and composition
//! uses the byte-string formatter.
//! The tell-prefill format `L"@tell %hs,"` is a **wide** format with a
//! narrow argument and is the odd one out; nothing on the display side mixes widths.
//! The one wide
//! chat template in the retail client is the chat-room formatter's
//! `L"[%ws] <Tell:IIDString:0:%ws>%ws<\\Tell> says, \"%ws\""`, which belongs to the
//! chat-room path and not to any of these.
//!
//! # The two transforms between `sprintf` and the scrollback
//!
//! Scroll-text submission decodes the composed byte string to wide text and hands it to the
//! scrollback insertion path,
//! whose **first act** is
//! a trim of leading and trailing newline markers —
//! the `L"\n"` marker. That is what removes the `\n` every template ends with;
//! see `add_text_to_scroll_trim`.
//!
//! The scroll receiver then builds the notice as
//! a final display-string notice `(type, body = the trimmed line, prefix = the timestamp,
//! window id)`. **The `prefix` argument is the timestamp, not the speaker.** It is
//! `wcsftime(L"%#H:%M:%S ")` when timestamp display is enabled, and it is left empty for
//! chat type `0x1A`, which the function special-cases. That is why the prefix is drawn in colour
//! index 12 whatever the message's own type is: a timestamp is not part of anybody's speech.
//! So a speech line has **no prefix at all** — the whole of `Bob says, "hi"` is the *body*, drawn
//! in the message's own type colour.

use std::ops::Range;

/// `"%s says, \"%s\"\n"` — 14 bytes plus the NUL.
///
/// Format arguments: the **sender name**, then the **message**.
pub const SAYS: &str = "%s says, \"%s\"\n";

/// `"<Tell:IIDString:%d:%s>%s<\\Tell> says, \"%s\"\n"` — 43 bytes.
///
/// Arguments: the **sender id** (`%d`, signed — the branch is only reached inside
/// `CLICKABLE_PLAYER_IDS`, so it is always positive), the name **twice**, then the message.
pub const SAYS_CLICKABLE: &str = "<Tell:IIDString:%d:%s>%s<\\Tell> says, \"%s\"\n";

/// `"You say, \"%s\"\n"` — 14 bytes. One argument, the message.
pub const YOU_SAY: &str = "You say, \"%s\"\n";

/// `"%s tells you, \"%s\"\n"` — 19 bytes. Name, then message.
pub const TELLS_YOU: &str = "%s tells you, \"%s\"\n";

/// `"<Tell:IIDString:%d:%s>%s<\\Tell> tells you, \"%s\"\n"` — 48 bytes.
/// Id, name, name, message.
pub const TELLS_YOU_CLICKABLE: &str = "<Tell:IIDString:%d:%s>%s<\\Tell> tells you, \"%s\"\n";

/// `"You think, \"%s\"\n"` — 16 bytes. One argument, the message.
pub const YOU_THINK: &str = "You think, \"%s\"\n";

/// `"%s %s\n"` — 6 bytes. The **untranslated** line: name, then
/// a random human or Olthoi phrase ([`HUMAN_TEXT`] / [`OLTHOI_TEXT`]) in place of the message.
/// Both speech handlers share it; it is also the emote handler's format.
pub const GARBLED: &str = "%s %s\n";

/// The trailing byte every one of the templates above carries, and the marker
/// scroll-text submission trims: `L"\n"`.
pub const TEMPLATE_TERMINATOR: char = '\n';

/// The clickable-name id range uses `0x50000000 < id && id < 0x70000000` at all three call sites —
/// exclusive at both ends, so it is written that way here. Identical
/// to `Hud::CLICKABLE_PLAYER_IDS`, which is the same three comparisons read for a different
/// purpose (arming `@r`).
pub const CLICKABLE_PLAYER_IDS: Range<u32> = 0x5000_0001..0x7000_0000;

/// Trim leading and trailing `L"\n"` characters, the scroll-submission path's
/// first operation on the composed line.
///
/// **Leading and trailing**, both — which is why a shard body that itself ends in a newline (ACE's
/// enter-world broadcast does) reaches the log without one.
pub use dereth_client_contract::chat::interface::add_text_to_scroll_trim;

/// `<Tell:IIDString:{id}:{name}>{name}<\Tell>` — the clickable-name run the two `_CLICKABLE`
/// templates open with. `dereth_text::tag::parse` strips the markup from the drawn glyphs and
/// hangs it on them as a tag, which is what makes a clicked name compose a tell.
fn tell_run(id: u32, name: &str) -> String {
    format!("<Tell:IIDString:{id}:{name}>{name}<\\Tell>")
}

/// The subject half of a `says` / `tells you` line: the bare name, or the clickable run.
fn subject(sender_id: u32, name: &str) -> String {
    if CLICKABLE_PLAYER_IDS.contains(&sender_id) {
        tell_run(sender_id, name)
    } else {
        name.to_owned()
    }
}

/// The `Communication_HearSpeech` handler — `0x02BB`, composed and trimmed.
///
/// The branch order is the client's: the self-echo first, then the clickable test.
/// The hearing-range test and the language garble are *upstream* gates and are not this
/// function's business — the caller owns them, as the two handlers do.
///
/// **`0x02BC` is not this function; use [`hear_ranged_speech_line`].** They differ in one branch,
/// and it is easy to miss.
#[must_use]
pub fn hear_speech_line(
    sender_id: u32,
    player_id: Option<u32>,
    name: &str,
    message: &str,
) -> String {
    let line = if player_id == Some(sender_id) {
        format!("You say, \"{message}\"\n")
    } else {
        format!("{} says, \"{message}\"\n", subject(sender_id, name))
    };
    add_text_to_scroll_trim(&line).to_owned()
}

/// `0x02BC`, composed and trimmed.
///
/// **It has no self-echo branch.** The function references exactly two templates, `SAYS` and
/// `SAYS_CLICKABLE`, while `"You say, \"%s\"\n"` has **one** caller in the whole client,
/// inside ordinary speech handling. So a ranged line
/// the shard echoes back to its own speaker would render as `Lark says, "…"`, not `You say, "…"`;
/// the pair of gates it does have (squelch and in-range checks) makes that unreachable in
/// practice, and this function
/// reproduces the branch structure rather than the practice. A separate function rather than
/// `hear_speech_line(id, None, …)` because "pass `None` here" is exactly the kind of instruction a
/// caller drops.
#[must_use]
pub fn hear_ranged_speech_line(sender_id: u32, name: &str, message: &str) -> String {
    add_text_to_scroll_trim(&format!(
        "{} says, \"{message}\"\n",
        subject(sender_id, name)
    ))
    .to_owned()
}

/// The `Communication_HearDirectSpeech` handler, composed and trimmed.
///
/// Returns `None` for the one case the client draws nothing at all: a tell the shard copied to us
/// but addressed to somebody else. It tests `senderID == targetID` **before**
/// `targetID == player_id`, so a self-tell prints `You think` without consulting the player id.
///
/// **The garble branch is outside this function and outside that gate.** The untranslated
/// arm composes `GARBLED` and adds it directly to the scroll, bypassing the recipient check.
/// Therefore a tell in a language you cannot understand is shown **whoever it was addressed
/// to**, which is an asymmetry in the client rather than a simplification here; see
/// [`garbled_line`]. No message in the capture corpus takes it.
#[must_use]
pub fn hear_direct_speech_line(
    sender_id: u32,
    target_id: u32,
    player_id: Option<u32>,
    name: &str,
    message: &str,
) -> Option<String> {
    let line = if sender_id == target_id {
        format!("You think, \"{message}\"\n")
    } else if player_id == Some(target_id) {
        format!("{} tells you, \"{message}\"\n", subject(sender_id, name))
    } else {
        return None;
    };
    Some(add_text_to_scroll_trim(&line).to_owned())
}

/// The untranslated form **all three** handlers fall back to: `GARBLED` with the name and a
/// phrase from [`random_text`].
///
/// The three call sites are the runtime receiver's `0x02BB`, `0x02BD` and `0x01E0`/`0x01E2` arms —
/// matching retail's three references to the shared format string, so the count of the format
/// string's users and the count of this function's callers agree at three.
///
/// No clickable-name run: the garble branch of every one of the three composes `GARBLED`
/// directly and never reaches the `0x50000000 < id < 0x70000000` test, so a garbled line is never
/// a `<Tell:…>`. That is the client's answer and not a simplification here.
#[must_use]
pub fn garbled_line(name: &str, random_text: &str) -> String {
    add_text_to_scroll_trim(&format!("{name} {random_text}\n")).to_owned()
}

// =================================================================================================
// The twenty phrases, transcribed from the retail client.
//
// The Olthoi and human phrase picks each roll once over the inclusive range
// 1 through 10 and use a ten-arm `switch`; their
// twenty string literals
// are the whole of the data.
//
// Each function maps roll *n* to its *n*-th phrase, and every phrase is used by exactly one
// function.
// =================================================================================================

/// The bounds 1 and 10, both inclusive — the only draw either function
/// makes. [`dereth_primitives::num::rng::Ran2::roll_i32`] is the transcription.
pub const GARBLE_ROLL: (i32, i32) = (1, 10);

/// The Olthoi phrase pick's ten phrases, in dice-roll order (index = roll − 1).
///
/// | roll | phrase |
/// |---:|---|
/// | 1 | `glares menacingly.` |
/// | 2 | `clicks its pincers together in anticipation of destruction.` |
/// | 3 | `screeches in a horrible fashion.` |
/// | 4 | `lets out a maddening series of clicks and hisses.` |
/// | 5 | `surveys the area as acid drips from its mandibles.` |
/// | 6 | `hisses some kind of threat.` |
/// | 7 | `calls out searching for other Olthoi.` |
/// | 8 | `cries out to indicate to its kin that prey is near.` |
/// | 9 | `casts about looking for victims.` |
/// | 10 | `prepares to hunt enemies of the queen.` |
///
///
pub const OLTHOI_TEXT: [&str; 10] = [
    "glares menacingly.",
    "clicks its pincers together in anticipation of destruction.",
    "screeches in a horrible fashion.",
    "lets out a maddening series of clicks and hisses.",
    "surveys the area as acid drips from its mandibles.",
    "hisses some kind of threat.",
    "calls out searching for other Olthoi.",
    "cries out to indicate to its kin that prey is near.",
    "casts about looking for victims.",
    "prepares to hunt enemies of the queen.",
];

/// The human phrase pick's ten phrases, in dice-roll order (index = roll − 1).
///
/// | roll | phrase |
/// |---:|---|
/// | 1 | `cowers in fear.` |
/// | 2 | `calls out for help and prepares to fight you.` |
/// | 3 | `seems startled to see Olthoi.` |
/// | 4 | `cries out to warn others that Olthoi are present.` |
/// | 5 | `surveys the area with weapons at the ready.` |
/// | 6 | `regards you warily.` |
/// | 7 | `lets out a battle cry as a challenge.` |
/// | 8 | `goes into a defensive posture at the sight of you.` |
/// | 9 | `seems to be looking for an escape route.` |
/// | 10 | `gestures a challenge to you.` |
///
/// **Both tables are written from the *speaker's* point of view and neither is a translation.**
/// The human phrases talk *about* Olthoi ("seems startled to see Olthoi") because they are what
/// an **Olthoi listener** is shown when a human speaks; the Olthoi phrases are what everybody
/// else is shown when an Olthoi speaks. Getting the two the wrong way round is invisible in a
/// screenshot and is the reason [`random_text`] keys on the *speaker* flag rather than on the
/// listener.
pub const HUMAN_TEXT: [&str; 10] = [
    "cowers in fear.",
    "calls out for help and prepares to fight you.",
    "seems startled to see Olthoi.",
    "cries out to warn others that Olthoi are present.",
    "surveys the area with weapons at the ready.",
    "regards you warily.",
    "lets out a battle cry as a challenge.",
    "goes into a defensive posture at the sight of you.",
    "seems to be looking for an escape route.",
    "gestures a challenge to you.",
];

/// The one substitution the three untranslated arms make, before [`garbled_line`] composes it.
///
/// `speaker_is_olthoi` is the **ampersand flag** — the marker [`language_marker`] found on the
/// sender's name — and nothing else. All three sites spell the choice the same way: the flag
/// set selects the Olthoi phrases, clear selects the human ones.
///
/// The same pair appears in direct-speech handling (selected by its own flag test) and in
/// emote handling. Those six calls are **every** use of either
/// function in the retail client, so "three arms" is a measurement rather than a claim.
///
/// `roll` is [`dereth_primitives::num::rng::Ran2::roll_i32`]`(1, 10)`. A value outside `1..=10` cannot come out
/// of a 1-to-10 dice roll, and the `switch`'s own default arm uses an unsigned comparison after
/// decrementing the roll and returns the **empty** string,
/// not a phrase — so that arm is reproduced rather than clamped.
#[must_use]
pub fn random_text(roll: i32, speaker_is_olthoi: bool) -> &'static str {
    let table = if speaker_is_olthoi {
        &OLTHOI_TEXT
    } else {
        &HUMAN_TEXT
    };
    // `checked_sub` and not `roll - 1`: the `switch`'s own bounds test is an **unsigned**
    // compare of the decremented value against 9, so every roll outside
    // `1..=10` — including `i32::MIN`, where a bare `- 1` overflows in a debug build — reaches the
    // default arm rather than panicking.
    roll.checked_sub(1)
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| table.get(i).copied())
        .unwrap_or("")
}

/// The listener half of the language test.
///
/// It fetches the player description, reads integer quality `0xbc` from it, and answers true
/// for `0xc` or `0xd` and false for anything else.
///
/// `0xBC` is `PropertyInt HeritageGroup`, the same property used by the paper doll's
/// race update — and **12 and 13 are exactly the two heritages that race update gives
/// their own `UIASSET` animation enum to** (`0x10000011` and `0x10000013`), because they are the
/// two with no humanoid idle. Two functions, two different jobs, the same pair of ids: that is
/// the second reading.
///
/// A description that carries no `HeritageGroup` reads `0` through
/// [`crate::qualities::Qualities::inq_int`], which is the int-quality lookup's own
/// answer for an absent property and is not 12 or 13 — so "no player description yet" is *not*
/// Olthoi, which is the client's behaviour.
pub const OLTHOI_HERITAGE_GROUPS: [i32; 2] = [12, 13];

/// The complete player-is-Olthoi decision. See [`OLTHOI_HERITAGE_GROUPS`].
#[must_use]
pub fn is_olthoi(heritage_group: i32) -> bool {
    OLTHOI_HERITAGE_GROUPS.contains(&heritage_group)
}

// =================================================================================================
// The emote path.
//
// `0x01E0 Communication_HearEmote` and `0x01E2 Communication_HearSoulEmote` need their own arms:
// left to `Hud::ui_event`'s catch-all, **every emote any player performed would be silently
// discarded** and the hearing check's second caller would have nothing to gate.
// =================================================================================================

/// The two language markers a **sender name** can carry, in the order
/// the emote handler checks them.
///
/// The emote handler first runs the hearing check on `(senderID, 0xC)` and returns without
/// composing anything when it is refused. It then checks whether the player is Olthoi and looks for `"^"` anywhere in
/// the sender name and, finding it, trims it from the end and sets the caret flag; otherwise it
/// looks for `"&"` and sets the ampersand flag the same way.
///
/// **The marker is looked for anywhere in the name and trimmed only from the end** — `strstr`
/// against `trim(leading = 0, trailing = 1, marker)`. That is the same pair
/// direct-speech handling runs before remembering the last teller, which is
/// why the runtime receiver's `trim_language_marker` delegates here rather than keeping a second
/// copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageMarker {
    /// No marker; the name is composed as it arrived.
    None,
    /// `^`. Looked for **first**, and it is also the marker
    /// the `Communication_HearSoulEmote` handler appends itself — see
    /// [`soul_emote_sender_name`].
    Caret,
    /// `&` is only looked for when `^` is absent.
    Ampersand,
}

/// Split a sender name into its marker and the name the line is composed with.
///
/// `strstr` first, `trim` second, `^` before `&`. A name
/// carrying **both** therefore reports `Caret` and keeps its `&`, which is the client's answer and
/// not a simplification.
#[must_use]
pub fn language_marker(name: &str) -> (LanguageMarker, &str) {
    if name.contains('^') {
        (LanguageMarker::Caret, name.trim_end_matches('^'))
    } else if name.contains('&') {
        (LanguageMarker::Ampersand, name.trim_end_matches('&'))
    } else {
        (LanguageMarker::None, name)
    }
}

/// Soul-emote handling performs one step before ordinary emote handling:
/// it appends `"^"` to the **sender name** and passes that.
///
/// It builds a caret string, appends it to the sender name, and invokes
/// ordinary emote handling with `(senderID, marked name, text)`.
///
/// So a soul emote is an acted emote whose name is caret-marked, and emote handling trims the caret
/// straight back off. The marker is not cosmetic: it is what makes
/// [`is_untranslated`] answer `false`, so a soul emote is **never** garbled.
#[must_use]
pub fn soul_emote_sender_name(name: &str) -> String {
    format!("{name}^")
}

/// The `Communication_HearEmote` handler's composition — the arm that draws the emote the
/// speaker actually performed.
///
/// The line starts as the already-trimmed sender name; when the text begins with `'` it is
/// appended with no space, otherwise a space is inserted first; then the text, then `"\n"`,
/// and the whole line goes to scroll-text submission as `(line, 0xC, true, 0)`.
///
/// **The apostrophe is the whole reason this is not `GARBLED` with the name and the text.** An
/// emote beginning `'` is joined with no space, so `/e 's cloak is torn` renders as
/// `Alba's cloak is torn` — and the *other* arm of the same function, the untranslated one,
/// composes `"%s %s\n"` through `sprintf` and always inserts the space. Two
/// compositions in one handler that agree on every input but that one.
///
/// An empty text is treated as a lone NUL rather than as its first character, so an **empty**
/// emote text is not an apostrophe and does get the space: `"Alba "`, trailing space and all.
/// Scroll-text submission trims only `L"\n"`, so the space survives to the scrollback.
#[must_use]
pub fn hear_emote_line(name: &str, text: &str) -> String {
    let joiner = if text.as_bytes().first() == Some(&b'\'') {
        ""
    } else {
        " "
    };
    add_text_to_scroll_trim(&format!("{name}{joiner}{text}\n")).to_owned()
}

/// The language decision **all three** hearing handlers make: does this listener hear the
/// speaker's own words, or a random phrase from `GARBLED`?
///
/// It is not the emote arm's alone. The identical pair of tests, over the identical pair of
/// flags, in the same order, appears in speech, direct-speech and emote handling.
///
/// The three arms differ only in what they compose afterwards. One name for one decision, because
/// three arms sharing a producer is exactly where two get wired and one is forgotten.
///
/// The decision is: if the ampersand flag equals the player's Olthoi state, the line is
/// understood; if the caret flag is set it is understood; otherwise it is garbled — and the
/// player's own `NoOlthoiTalk` boolean property overrides both.
///
/// So: **the ampersand marks the speaker as Olthoi and [`is_olthoi`] answers for the listener; a
/// mismatch garbles, a match does not.** A caret-marked name — which is every soul emote, see
/// [`soul_emote_sender_name`] — is always understood, and the player's own `NoOlthoiTalk`
/// (`BooleanPropertyID 0x81 = 129`) overrides the lot.
///
/// `no_olthoi_talk` folds retail's three-way test (no player description, a failed quality
/// lookup, or Boolean `0x81` not set) into the one answer this build can give: the HUD
/// delegates to the World-owned shared quality store, the same store the interface exposes, and its Boolean inquiry returns `false`
/// for an absent table, which is the no-description arm.
#[must_use]
pub fn is_untranslated(
    marker: LanguageMarker,
    listener_is_olthoi: bool,
    no_olthoi_talk: bool,
) -> bool {
    if no_olthoi_talk {
        return false;
    }
    if marker == LanguageMarker::Caret {
        return false;
    }
    (marker == LanguageMarker::Ampersand) != listener_is_olthoi
}

// -------------------------------------------------------------------------------------------------
// The `Communication_ChannelBroadcast` handler — the `0x0147` line.
// -------------------------------------------------------------------------------------------------

/// `"<unknown>"` — substituted when
/// channel-name lookup fails.
pub const CHANNEL_UNKNOWN: &str = "<unknown>";
/// `"You say to your "` — pushed for an own line on `0x1000`,
/// `0x2000` or `0x4000`; followed by the channel name and `", \""`.
pub const YOU_SAY_TO_YOUR: &str = "You say to your ";
/// `"You say on the "` — pushed for an own line; followed by the channel name
/// and `" channel, \""`.
pub const YOU_SAY_ON_THE: &str = "You say on the ";
/// `"<\\Tell> says on the "` — pushed after
/// `"<Tell:IIDString:0:%s>"` and the sender name.
pub const SAYS_ON_THE: &str = "<\\Tell> says on the ";
/// `"Your vassal <Tell:IIDString:0:%s>%s<\\Tell> says to you, \""` — the
/// **Patron channel (`0x2000`)** with a named sender: a vassal of ours speaking up the tree.
pub const YOUR_VASSAL_SAYS: &str = "Your vassal <Tell:IIDString:0:%s>%s<\\Tell> says to you, \"";
/// `"Your patron <Tell:IIDString:0:%s>%s<\\Tell> says to you, \""` — the
/// **Vassals channel (`0x1000`)** with a named sender: our patron speaking down the tree.
pub const YOUR_PATRON_SAYS: &str = "Your patron <Tell:IIDString:0:%s>%s<\\Tell> says to you, \"";
/// `"Your follower <Tell:IIDString:0:%s>%s<\\Tell> says to you, \""` — the
/// **Monarch channel (`0x4000`)** with a named sender: somebody below the monarch speaking to the
/// top.
pub const YOUR_FOLLOWER_SAYS: &str =
    "Your follower <Tell:IIDString:0:%s>%s<\\Tell> says to you, \"";

/// The line and text type channel-broadcast handling submits to the scroll,
/// composed from a `0x0147`'s `channel`, `senderName`
/// and `msg`. The `"\n"` every template ends with is still on it; the caller trims it with
/// `add_text_to_scroll_trim` as the client does.
///
/// The format strings and text types below were checked against retail.
///
/// **Own line** — `senderName` is empty (the string has length one including its NUL,
/// a string holding only its NUL terminator — the shard echoing our own broadcast):
///
/// | channel | prefix | text type |
/// |---|---|---:|
/// | `0x1000` `0x2000` `0x4000` | `You say to your <name>, "` | `0xb` |
/// | `0x800` | `[Fellowship] You say, "` | `0x13` |
/// | `0x1000000` | `[Co-Vassals] You say, "` | `0xa` |
/// | `0x2000000` | `[Allegiance Broadcast] You say, "` | `0xa` |
/// | `0x4000000` | *(empty prefix)* | `0x13` |
/// | other | `You say on the <name> channel, "` | see below |
///
/// **Named sender**:
///
/// | channel | prefix | text type |
/// |---|---|---:|
/// | `0x4000` | [`YOUR_FOLLOWER_SAYS`], then the sender is remembered as the last monarch-channel name | `0xa` |
/// | `0x2000` | [`YOUR_VASSAL_SAYS`], then the sender is remembered as the last patron-channel name | `0xa` |
/// | `0x1000` | [`YOUR_PATRON_SAYS`] | `0xa` |
/// | `0x800` | `[Fellowship] <Tell:IIDString:0:%s>%s<\Tell> says, "` | `0x13` |
/// | `0x1000000` | `[Co-Vassals] <Tell…> says, "` | `0xa` |
/// | `0x2000000` | `[Allegiance Broadcast] <Tell…> says, "` | `0xa` |
/// | other | `<Tell:IIDString:0:%s>%s<\Tell> says on the <name> channel, "` | see below |
///
/// The **generic** type is computed, not tabled: `0x400` (Help) is `0xf`;
/// channel `1` (Abuse) is `0xe` either way; every other channel is `9` (`Channel_Send`) outgoing and `8`
/// (`Channel`) incoming.
///
/// The terminator is decided by channel alone after the branches rejoin: channel `0x4000000`
/// picks `"\n"` for FellowBroadcast and `"\"\n"` for
/// everything else — so a *named* FellowBroadcast line, which takes the generic format, has no
/// closing quote. That is the client's own asymmetry and is kept.
///
/// `<name>` is the answer from [`super::get_channel_name`],
/// or [`CHANNEL_UNKNOWN`]. Both `%s` of the clickable run are the raw `senderName`
/// (the same buffer data passed twice), untrimmed.
#[must_use]
pub fn channel_broadcast_line(channel: u32, sender_name: &str, message: &str) -> (u32, String) {
    use super::text_type as t;

    let name = super::get_channel_name(channel).unwrap_or(CHANNEL_UNKNOWN);
    let generic_type = |send: bool| match channel {
        0x400 => t::HELP,
        1 => t::ABUSE,
        _ if send => t::CHANNEL_SEND,
        _ => t::CHANNEL,
    };
    let (ty, prefix) = if sender_name.is_empty() {
        match channel {
            0x1000 | 0x2000 | 0x4000 => (t::SOCIAL_SEND, format!("{YOU_SAY_TO_YOUR}{name}, \"")),
            0x800 => (t::FELLOWSHIP, "[Fellowship] You say, \"".to_owned()),
            0x0100_0000 => (t::SOCIAL, "[Co-Vassals] You say, \"".to_owned()),
            0x0200_0000 => (t::SOCIAL, "[Allegiance Broadcast] You say, \"".to_owned()),
            0x0400_0000 => (t::FELLOWSHIP, String::new()),
            _ => (
                generic_type(true),
                format!("{YOU_SAY_ON_THE}{name} channel, \""),
            ),
        }
    } else {
        let run = tell_run(0, sender_name);
        match channel {
            0x4000 => (t::SOCIAL, format!("Your follower {run} says to you, \"")),
            0x2000 => (t::SOCIAL, format!("Your vassal {run} says to you, \"")),
            0x1000 => (t::SOCIAL, format!("Your patron {run} says to you, \"")),
            0x800 => (t::FELLOWSHIP, format!("[Fellowship] {run} says, \"")),
            0x0100_0000 => (t::SOCIAL, format!("[Co-Vassals] {run} says, \"")),
            0x0200_0000 => (t::SOCIAL, format!("[Allegiance Broadcast] {run} says, \"")),
            _ => (
                generic_type(false),
                format!(
                    "<Tell:IIDString:0:{sender_name}>{sender_name}{SAYS_ON_THE}{name} channel, \""
                ),
            ),
        }
    };
    let terminator = if channel == 0x0400_0000 { "\n" } else { "\"\n" };
    (ty, format!("{prefix}{message}{terminator}"))
}

// ---------------------------------------------------------------------------------------------
// The fellowship panel's own lines.
// ---------------------------------------------------------------------------------------------
//
// "You have created the Fellowship of …" and "You are no longer a member of …" are not
// `0x028B WeenieErrorWithString` codes. **The server never sends them.** ACE's
// `Player.FellowshipCreate` sends a `0x02BE` and a `0x01C9`, and `Fellowship.QuitFellowship` a
// `0x00A3`; the sentences are composed by the panel on receipt of the notice each raises. Every
// one uses a wide formatted string with a `%hs` narrow operand and every
// one goes to scroll-text submission as `(text, 0, true, 0)` -- chat type **0**, the colour
// table's green.
// Each literal below is the retail client's own format string, operands included.
//
// | function | literal | condition |
// |---|---|---|
// | fellowship update | `You have created the Fellowship of %hs.\n` | table was empty **and** `_leader == player_id` |
// | same | `You have been recruited into the %hs fellowship, %hs fellowship led by %hs.\n` | table was empty, somebody else leads; `%hs` = name, `"an open"` / `"a closed"` according to `_open_fellow`, leader's name |
// | disband notice | `You have disbanded your Fellowship.\n` | `_leader == player_id` |
// | same | `%hs has disbanded your Fellowship.\n` | leader's name |
// | dismissal notice | `%hs has dismissed you from the Fellowship.\n` | the dismissed is the player; leader's name |
// | same | `You dismiss %hs from your Fellowship.\n` | a fellow **and** `_leader == player_id` |
// | same | `%hs has been dismissed from the Fellowship.\n` | a fellow, somebody else leads |
// | quit notice | `You are no longer a member of the %hs Fellowship.\n` | the quitter is the player; `%hs` = fellowship name |
// | same | `%hs has left your Fellowship.\n` | a fellow; the fellow's name |
// | member-added notice | `%hs is now a member of your Fellowship.\n` | always (raised only when not yet a fellow before the `0x02C0`) |
//
// "Table was empty" means the player has no fellowship or its fellowship table has no
// members. The panel keeps its own copy and deletes it on
// disband, on the player's quit and on the player's dismissal, so the next `0x02BE` after any of
// those is "created" or "recruited" again -- which is what `fellowship-two-monarch` blob 583 shows.
//
// The composers return the literal **with** its trailing newline, the way the templates above do;
// the `hud.rs` arm applies [`add_text_to_scroll_trim`] as scroll-text submission would.

/// Template for the creation notice.
pub const YOU_HAVE_CREATED_THE_FELLOWSHIP_OF: &str = "You have created the Fellowship of %hs.\n";
/// Template for the recruitment notice.
pub const YOU_HAVE_BEEN_RECRUITED_INTO: &str =
    "You have been recruited into the %hs fellowship, %hs fellowship led by %hs.\n";
/// Open/closed fellowship descriptions -- the second `%hs` of [`YOU_HAVE_BEEN_RECRUITED_INTO`].
pub const AN_OPEN: &str = "an open";
pub const A_CLOSED: &str = "a closed";
/// Template for the local disband notice.
pub const YOU_HAVE_DISBANDED: &str = "You have disbanded your Fellowship.\n";
/// Template for a leader's disband notice.
pub const HAS_DISBANDED: &str = "%hs has disbanded your Fellowship.\n";
/// Template for the player's dismissal notice.
pub const HAS_DISMISSED_YOU: &str = "%hs has dismissed you from the Fellowship.\n";
/// Template for dismissing another fellow.
pub const YOU_DISMISS: &str = "You dismiss %hs from your Fellowship.\n";
/// Template for another fellow's dismissal.
pub const HAS_BEEN_DISMISSED: &str = "%hs has been dismissed from the Fellowship.\n";
/// Template for the player's quit notice.
pub const YOU_ARE_NO_LONGER_A_MEMBER: &str = "You are no longer a member of the %hs Fellowship.\n";
/// Template for another fellow's quit notice.
pub const HAS_LEFT: &str = "%hs has left your Fellowship.\n";
/// Template for a new-fellow notice.
pub const IS_NOW_A_MEMBER: &str = "%hs is now a member of your Fellowship.\n";

/// Every fellowship UI line is scrolled with chat type zero.
pub const FELLOWSHIP_UI_CHAT_TYPE: u32 = 0;

/// The fellowship-update handler's line for a `0x02BE` that lands on an empty table.
/// `None` when the table was not empty: that branch skips straight to the update.
#[must_use]
pub fn fellowship_update_line(
    table_was_empty: bool,
    player_leads: bool,
    name: &str,
    open: bool,
    leader_name: &str,
) -> Option<String> {
    if !table_was_empty {
        return None;
    }
    Some(if player_leads {
        YOU_HAVE_CREATED_THE_FELLOWSHIP_OF.replacen("%hs", name, 1)
    } else {
        let openness = if open { AN_OPEN } else { A_CLOSED };
        YOU_HAVE_BEEN_RECRUITED_INTO
            .replacen("%hs", name, 1)
            .replacen("%hs", openness, 1)
            .replacen("%hs", leader_name, 1)
    })
}

/// The chat line for a fellowship being disbanded.
#[must_use]
pub fn fellowship_disbanded_line(player_leads: bool, leader_name: &str) -> String {
    if player_leads {
        YOU_HAVE_DISBANDED.to_owned()
    } else {
        HAS_DISBANDED.replacen("%hs", leader_name, 1)
    }
}

/// The chat line for a fellow being dismissed. `None` when the dismissed is neither the player
/// nor a member -- a failed fellow-membership test returns without a line.
#[must_use]
pub fn fellow_dismissed_line(
    is_the_player: bool,
    is_fellow: bool,
    player_leads: bool,
    leader_name: &str,
    fellow_name: &str,
) -> Option<String> {
    if is_the_player {
        Some(HAS_DISMISSED_YOU.replacen("%hs", leader_name, 1))
    } else if !is_fellow {
        None
    } else if player_leads {
        Some(YOU_DISMISS.replacen("%hs", fellow_name, 1))
    } else {
        Some(HAS_BEEN_DISMISSED.replacen("%hs", fellow_name, 1))
    }
}

/// The chat line for a fellow quitting. `None` when the quitter is neither the player nor a member.
#[must_use]
pub fn fellow_quit_line(
    is_the_player: bool,
    is_fellow: bool,
    fellowship_name: &str,
    fellow_name: &str,
) -> Option<String> {
    if is_the_player {
        Some(YOU_ARE_NO_LONGER_A_MEMBER.replacen("%hs", fellowship_name, 1))
    } else if is_fellow {
        Some(HAS_LEFT.replacen("%hs", fellow_name, 1))
    } else {
        None
    }
}

/// The chat line for a fellow being added.
#[must_use]
pub fn fellow_added_line(fellow_name: &str) -> String {
    IS_NOW_A_MEMBER.replacen("%hs", fellow_name, 1)
}
