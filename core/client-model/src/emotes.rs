//! Chat poses, acted versus soul emotes, and `EmoteTable`.
//!
//! Two entirely different things are called "emote" in the codebase: **chat poses**, which the
//! *client* runs, and `EmoteTable`'s NPC behaviour scripts, which the *server* runs and the client
//! only stores. This module is almost entirely about the first.
//!
//! The pose animation is the one piece of animation the client **does** predict — it queues the
//! motion locally and immediately, without waiting for the server. That does not contradict "the
//! client predicts nothing": that rule is about combat and inventory, and the pose is a local echo
//! the server also broadcasts to everyone else.

use dereth_assets::tables::ChatPoseTable;

/// `ChatEmoteData` — what the actor sees and what everyone else sees.
///
/// **Checked against the shipped table.** Neither field has a `%s` in it (not `"%s waves."`):
/// the shipped `ChatPoseTable` has **no `%s` in either**. `Wave` is
/// `("wave.", "waves.")` and `Scratch Head` is `("scratch your head.", "scratches %p head.")` — a
/// bare verb phrase, because the *name* is prepended by the receiving side, not by a format.
/// The pose sender uses [`Self::other_emote`] as the whole `Request::SoulEmote` payload. The receiver
/// composes `name + " " + text`, so a `%s`
/// here would reach the scrollback verbatim. `%p` is real and is substituted before sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatEmoteData {
    /// What **you** see: `"wave."`, drawn as `"You wave."` by the local echo, which is
    /// the hear-soul-emote handler called with sender `"You"` and `my_emote`.
    pub my_emote: String,
    /// What everyone else sees: `"waves."`, with `%p` for the possessive pronoun. This is the
    /// `Request::SoulEmote` (`0x01E1`) payload.
    pub other_emote: String,
}

/// The `Gender` int property the pronoun substitution reads. 1 = male.
pub const GENDER_PROPERTY: u32 = 0x71;

/// The emote formatter's pronoun choice: `"his"` for gender 1, `"her"` otherwise.
#[must_use]
pub fn possessive_pronoun(gender: i32) -> &'static str {
    if gender == 1 {
        "his"
    } else {
        "her"
    }
}

/// Substitute `%p` with the possessive pronoun, as does before sending.
#[must_use]
pub fn substitute_pronoun(text: &str, gender: i32) -> String {
    text.replace("%p", possessive_pronoun(gender))
}

/// Remove text between a matching delimiter pair in one pass.
///
/// Finds text delimited by `open`…`close`, hands it to `pose`, and **removes it only when `pose`
/// answers `true`**. Returns the new cursor position.
///
/// **The gate is the whole point of the function.** Stripping every `*…*` unconditionally
/// would send `hello *xyzzy* there` as `hello  there` with the typo silently eaten. The client
/// does not:
///
/// ```text
///   find open                  ; not found -> return, nothing removed
///   curr = min(curr + 1, len)
///   find close                 ; not found -> return
///   s = the text between       ; failure -> return
///   if !pose(s)                ; FALSE -> nothing is removed, curr stays at `close`
///       return
///   mark = mark - 1            ; TRUE  -> i.e. the open delimiter
///   delete [open, close]
///   curr = curr < 1 ? 0 : curr - 1
/// ```
///
/// So an unrecognised `*word*` stays in the line and is spoken. And [`pose`] is `true` only when
/// the command lookup resolved the motion name — see there; a pose whose *emote strings* resolve but
/// whose *motion* does not is sent as a soul emote **and** left in the speech.
///
/// The public-chat entry point calls this twice per iteration, with `('*','*')` and then `('<','>')`, so
/// `<wave>` is a pose too and a `<Tell:…>` run a player types is looked up, missed, and left
/// alone.
fn remove_text_between(
    s: &mut Vec<char>,
    curr: usize,
    open: char,
    close: char,
    pose: &mut impl FnMut(&str) -> bool,
) -> usize {
    let Some(o) = s[curr.min(s.len())..]
        .iter()
        .position(|c| *c == open)
        .map(|i| i + curr)
    else {
        return s.len();
    };
    let start = (o + 1).min(s.len());
    let Some(c) = s[start..]
        .iter()
        .position(|ch| *ch == close)
        .map(|i| i + start)
    else {
        return s.len();
    };
    let inner: String = s[start..c].iter().collect();
    if !pose(&inner) {
        // `curr` is left at the close delimiter, which is where the next delimiter search starts.
        return c;
    }
    // The selection runs from `mark - 1` to `min(curr + 1, len)` and is replaced with nothing: the range deleted
    // is the delimiters *and* their contents.
    let end = (c + 1).min(s.len());
    s.drain(o..end);
    // `curr = curr == 0 ? 0 : curr - 1` — the splice point, backed up one.
    o.saturating_sub(1)
}

/// What one `*…*` did — [`pose`]'s answer, and the three things it can do with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoseOutcome {
    /// The motion-command **name** out of the first hash; the command lookup turns it
    /// into the `0x1xxxxxxx`/`0x4xxxxxxx` id that the animation crate owns.
    pub motion_name: String,
    /// The command lookup's answer, and **`pose`'s return value is `motion_command.is_some()`** —
    /// the flag byte is initialised 0 at the start and set to 1 in exactly one
    /// place, inside the `if (command_lookup(...) != 0)`, and nowhere else.
    pub motion_command: Option<u32>,
    /// The soul-emote event (`0x01E1`) payload: `other_emote` with `%p`
    /// substituted. `None` when `other_emote` is empty (its length including the terminator is
    /// 1, which skips the whole arm), in which case the
    /// message is not sent and the gender is never even read.
    pub soul_emote: Option<String>,
    /// The local echo: the hear-soul-emote handler with sender `"You"` and `my_emote`.
    /// `None` when `my_emote` is empty.
    pub my_emote: Option<String>,
}

/// The sender name the pose routine gives its local echo — a literal.
///
/// The hear-soul-emote handler appends `^` to it and tail-calls the hear-emote handler, so the
/// echo reads `"You wave."` and is **never garbled**, whatever the player's heritage.
pub const LOCAL_ECHO_NAME: &str = "You";

/// Choose and send a chat pose.
///
/// ```text
///   load the chat-pose table from database category 7, subtype 2, enum 0x11
///   if (!table) return false;
///   ok = inq_chat_pose_command(name) -> (motion, my_emote, other_emote)
///   if (!ok) return false;                ; nothing at all happens
///   cmd = command_lookup(motion)
///   if (cmd) { cmdinterp.run(cmd); ret = true; }
///   if (other_emote is not empty) {
///       g = int quality 0x71 (Gender)
///       other_emote.replace("%p", g == 1 ? "his" : "her")
///       send the soul-emote event with other_emote
///           }
///   if (my_emote is not empty)
///       hear_soul_emote(0, "You", my_emote)
///   return ret
/// ```
///
/// **The output order matters**: the second is the value the command lookup is later handed, the
/// third the one the local echo is handed and the fourth the one the soul-emote event is handed.
/// So it is
/// `(motion, my_emote, other_emote)` and a swap of the last two would send the actor's own wording
/// to everybody and show them the third-person form.
///
/// Returns `None` for `inq_chat_pose_command`'s miss, which is the *silent* failure: no animation, no
/// message, no echo, and the `*word*` is left in the line.
#[must_use]
pub fn pose(
    t: &ChatPoseTable,
    command: &str,
    gender: i32,
    string2command: impl Fn(&str) -> Option<u32>,
) -> Option<PoseOutcome> {
    let (motion_name, d) = inq_chat_pose_command(t, command)?;
    let motion_command = string2command(&motion_name);
    let some = |s: String| if s.is_empty() { None } else { Some(s) };
    Some(PoseOutcome {
        motion_command,
        soul_emote: some(substitute_pronoun(&d.other_emote, gender)),
        my_emote: some(d.my_emote),
        motion_name,
    })
}

/// Result of deciding how public chat should be delivered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PublicChatOutcome {
    /// One per `*…*` or `<…>` run that resolved, in the order the client ran them — which is the
    /// order they are sent in, and each is a `Request::SoulEmote` plus a local echo.
    pub poses: Vec<PoseOutcome>,
    /// The talk event (`0x0015`) payload: whatever is left after the strips,
    /// trimmed with [`crate::chat::WHITESPACE`]. `None` when nothing is left — the
    /// emptiness test after the trim, so `*wave*` on its own performs the pose and says
    /// **nothing**.
    pub speech: Option<String>,
}

/// Process one public-chat request.
///
/// ```text
///   while (the char at iter.curr != 0) {
///       remove_text_between(iter, '*', '*');
///       iter.curr = min(SAVED curr, len)                                 // restored!
///       remove_text_between(iter, '<', '>');                             // NOT restored
///   }
///   trim(leading = 1, trailing = 1, WHITESPACE)
///   if empty: say nothing at all
///   send the talk event with the line
/// ```
///
/// **The asymmetric restore after the `*` pass is the whole behaviour of this function.** The
/// iterator's `curr` and `mark` are saved at the top of every iteration; after the `*` pass the
/// pair is written
/// back clamped to the new length, so **whatever the `*` pass did to the cursor is discarded**.
/// After the `<` pass only `mark` is written back, so **that** pass's
/// cursor survives — and a delimiter search that does not find its delimiter leaves the cursor at
/// the string's length, where the next read finds the terminator and the loop ends.
///
/// Two consequences, and the second is a player-visible quirk:
///
/// 1. it terminates. A line with an unresolvable `*xyzzy*` and no `<` runs the body **once**: the
///    `*` pass changes nothing, the `<` pass runs the cursor to the end, and the next read returns
///    the NUL. Without the `<` pass driving it there is nothing in the loop that advances the
///    cursor at all;
/// 2. **at most one `*…*` fires per line**, unless the line also contains `<…>` to keep the loop
///    going. `*wave* *bow*` waves, and then *says* `*bow*`. `[inferred]` — from the cursor
///    bookkeeping above rather than from an observation; it is a one-line question for anyone with
///    the retail client in front of them, and it is asserted in
///    `only_the_first_asterisk_pose_fires_on_a_line_with_no_angle_brackets` so that an answer of
///    "no, both fire" lands on a failing test rather than on nothing.
///
/// `resolve` is [`pose`] bound to the table, the player's gender and the command lookup; the caller
/// owns those because `dereth-client-model` has neither the dat store nor `dereth_animation`. A `None` answer is
/// `inq_chat_pose_command`'s miss and leaves the run in the line; a `Some` whose
/// [`PoseOutcome::motion_command`] is `None` is sent and **also** left in the line, which is the
/// odd case `pose`'s return value produces and is reproduced here rather than tidied.
#[must_use]
pub fn public_chat(
    line: &str,
    mut resolve: impl FnMut(&str) -> Option<PoseOutcome>,
) -> PublicChatOutcome {
    let mut s: Vec<char> = line.chars().collect();
    let mut poses = Vec::new();
    let mut curr = 0usize;
    // The loop runs while the cursor is not at the terminator, which is at the string's length.
    while curr < s.len() {
        let mut run = |inner: &str| -> bool {
            match resolve(inner) {
                Some(p) => {
                    let removed = p.motion_command.is_some();
                    poses.push(p);
                    removed
                }
                None => false,
            }
        };
        // The `*` pass's cursor is discarded: `iter.curr = min(saved, len)`.
        let saved = curr;
        let _ = remove_text_between(&mut s, curr, '*', '*', &mut run);
        curr = saved.min(s.len());
        // The `<` pass's cursor is kept, and it is the only thing that ends the loop.
        curr = remove_text_between(&mut s, curr, '<', '>', &mut run).min(s.len());
    }
    let joined: String = s.into_iter().collect();
    let speech = joined.trim_matches(crate::chat::WHITESPACE);
    PublicChatOutcome {
        poses,
        speech: if speech.is_empty() {
            None
        } else {
            Some(speech.to_owned())
        },
    }
}

/// Look up the pose command associated with chat text.
///
/// Two lookups: the **case-insensitive** pose hash gives the motion-command name, and the
/// exact-keyed emote hash gives the two strings. Either miss is a silent failure, and `pose` then
/// does nothing at all.
#[must_use]
pub fn inq_chat_pose_command(t: &ChatPoseTable, name: &str) -> Option<(String, ChatEmoteData)> {
    let key = name.to_lowercase();
    let motion = t
        .poses
        .iter()
        .find(|(k, _)| k.to_lowercase() == key)
        .map(|(_, v)| v.clone())?;
    let d = t
        .emotes
        .iter()
        .find(|(k, _)| *k == motion)
        .map(|(_, v)| v)?;
    Some((
        motion,
        ChatEmoteData {
            my_emote: d.0.clone(),
            other_emote: d.1.clone(),
        },
    ))
}

// A pose is not one text "sent as a soul emote *and* echoed locally at once". It is two —
// `other_emote` goes on the wire and `my_emote` is echoed (two separate calls) — and the whole
// point of `ChatEmoteData` having two fields is that they differ ("waves." against "wave.").
// The command lookup is `pose`'s return value and therefore the thing that decides whether the
// `*…*` is removed from the line.

/// Which of the two emote messages a line produces.
///
/// Both are displayed at chat type `0x0C` (`Emote`, grey) — see [`crate::chat`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmoteKind {
    /// Send opcode `0x01E1`, received as soul-emote opcode `0x01E2`: the table's complete sentence
    /// plus the animation.
    Soul,
    /// Send opcode `0x01DF`, received as acted-emote opcode `0x01E0`:
    /// `"<name> <whatever was typed>"`, with no animation.
    Acted,
}

/// The client's list, verbatim and in order.
///
/// Printed to the scroll with type 0, announce-to-plugins false, window 1. Note that
/// the last three carry a space, which the earlier entries do not.
pub const STANDARD_EMOTES: [&str; 41] = [
    "ShakeFist",
    "Beckon",
    "BeSeeingYou",
    "BlowKiss",
    "BowDeep",
    "ClapHands",
    "Cry",
    "Laugh",
    "Nod",
    "Point",
    "Shrug",
    "Wave",
    "Akimbo",
    "HeartyLaugh",
    "Salute",
    "TapFoot",
    "WaveHigh",
    "WaveLow",
    "Yawn",
    "Stretch",
    "Cringe",
    "Kneel",
    "Plead",
    "Shiver",
    "Shoo",
    "Slouch",
    "Spit",
    "Surrender",
    "Woah",
    "Winded",
    "YMCA",
    "Eat",
    "Drink",
    "Teapot",
    "Pray",
    "Mock",
    "Cheer",
    "Helper",
    "Warm Hands",
    "Scratch Head",
    "Shake Head",
];

/// The two header lines `do_emote_list` prints before the list.
pub const EMOTE_LIST_HEADER: [&str; 2] = [
    "Standard Emotes:",
    "Note: These commands should be bound on either side by asterisks. (Example: *wave*)",
];

/// `@emotes`, in full.
///
/// **It is one string, not forty-three lines.** The whole function writes a single
/// literal: the two headers, then the forty-one names joined with `"; "`, then `"\n\n"`. This
/// builds it from [`EMOTE_LIST_HEADER`] and [`STANDARD_EMOTES`] so
/// there is one source of truth for the names; a test pins the
/// composed result against the client's literal, which is what stops the separator or the terminator
/// drifting.
#[must_use]
pub fn emote_list_text() -> String {
    format!(
        "{}\n{}\n{}\n\n",
        EMOTE_LIST_HEADER[0],
        EMOTE_LIST_HEADER[1],
        STANDARD_EMOTES.join("; ")
    )
}

/// `do_emote_list`'s scroll-print arguments: text, type, announce-to-plugins, window.
///
/// Retail passes text, type **0**, announce **false**, window **1** — a fixed window, not the
/// command's source window.
pub const EMOTE_LIST_TEXT_TYPE: u32 = 0;
/// See [`EMOTE_LIST_TEXT_TYPE`].
pub const EMOTE_LIST_ANNOUNCE_TO_PLUGINS: bool = false;
/// See [`EMOTE_LIST_TEXT_TYPE`]. Window **1** is the main chat window.
pub const EMOTE_LIST_WINDOW_ID: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §3 — `%p` is the possessive pronoun, chosen from the player's `Gender` int quality
    /// (1 = male).
    #[test]
    fn the_pronoun_substitution_reads_gender_one_as_male() {
        assert_eq!(possessive_pronoun(1), "his");
        assert_eq!(possessive_pronoun(2), "her");
        assert_eq!(
            possessive_pronoun(0),
            "her",
            "anything that is not 1 takes the female form"
        );
        assert_eq!(
            substitute_pronoun("%s scratches %p head.", 1),
            "%s scratches his head."
        );
        assert_eq!(substitute_pronoun("%s waves.", 1), "%s waves.");
        assert_eq!(GENDER_PROPERTY, 113);
    }

    /// A [`pose`]-shaped stub for [`public_chat`]: `known` resolves and everything else misses.
    ///
    /// `motion_command` is `Some` for every hit, which is `Pose`'s `true`; the
    /// `resolved-but-no-motion` case has its own test below because it is the only input on which
    /// "sent" and "removed" disagree.
    fn stub(
        known: &'static [&'static str],
        with_motion: bool,
    ) -> impl FnMut(&str) -> Option<PoseOutcome> {
        move |name: &str| {
            known
                .iter()
                .find(|k| k.eq_ignore_ascii_case(name))
                .map(|k| PoseOutcome {
                    motion_name: (*k).to_owned(),
                    motion_command: if with_motion { Some(0x1000_0001) } else { None },
                    soul_emote: Some(format!("{k}s.")),
                    my_emote: Some(format!("{k}.")),
                })
        }
    }

    /// Asterisks are stripped only when the pose resolves.
    #[test]
    fn asterisks_are_stripped_only_when_the_pose_resolves() {
        let out = public_chat("hello *wave* there", stub(&["wave"], true));
        assert_eq!(out.speech.as_deref(), Some("hello  there"));
        assert_eq!(out.poses.len(), 1);
        assert_eq!(out.poses[0].motion_name, "wave");

        // The same line with a pose the table does not have: nothing is removed and the whole
        // thing is spoken, typo and all.
        let out = public_chat("hello *xyzzy* there", stub(&["wave"], true));
        assert_eq!(out.speech.as_deref(), Some("hello *xyzzy* there"));
        assert!(out.poses.is_empty());

        // `*wave*` alone leaves nothing to say -- the emptiness test after the trim.
        let out = public_chat("*wave*", stub(&["wave"], true));
        assert_eq!(out.speech, None);
        assert_eq!(out.poses.len(), 1);

        // An unpaired asterisk is not a pose and stays in the speech.
        let out = public_chat("five * three", stub(&["wave"], true));
        assert_eq!(out.speech.as_deref(), Some("five * three"));
        assert!(out.poses.is_empty());

        let out = public_chat("no poses here", stub(&["wave"], true));
        assert_eq!(out.speech.as_deref(), Some("no poses here"));
        assert!(out.poses.is_empty());

        // Public chat runs the same function again with `<`/`>`,
        // so angle brackets are poses too.
        let out = public_chat("hi <wave> there", stub(&["wave"], true));
        assert_eq!(out.speech.as_deref(), Some("hi  there"));
        assert_eq!(out.poses.len(), 1);

        // …and a `<Tell:…>` run a player types is looked up, missed, and left alone.
        let out = public_chat("<Tell:IIDString:1:Bob>Bob<\\Tell>", stub(&["wave"], true));
        assert_eq!(
            out.speech.as_deref(),
            Some("<Tell:IIDString:1:Bob>Bob<\\Tell>")
        );
        assert!(out.poses.is_empty());
    }

    /// **Only the first `*…*` on a line fires, unless a `<…>` keeps the loop going.**
    ///
    /// See [`public_chat`]: the retail loop restores the cursor after the `*` pass and not
    /// after the `<` pass, so with no angle brackets the body runs exactly once. `[inferred]` from
    /// the cursor bookkeeping above, and **worth one question to anyone who played**: does
    /// `*wave* *bow*` perform both in retail, or wave and then say `*bow*`? If both fire, this
    /// test is the thing that reddens.
    #[test]
    fn only_the_first_asterisk_pose_fires_on_a_line_with_no_angle_brackets() {
        let out = public_chat("a *one* b *two* c", stub(&["one", "two"], true));
        assert_eq!(out.speech.as_deref(), Some("a  b *two* c"));
        assert_eq!(out.poses.len(), 1, "the `*` pass's cursor is discarded ");
        assert_eq!(out.poses[0].motion_name, "one");

        // …and a `<…>` on the same line restarts the loop, so the second one does fire. Both
        // arms of the same mechanism, which is what makes the first a measurement.
        let out = public_chat(
            "<one> a *two* b *three* c",
            stub(&["one", "two", "three"], true),
        );
        assert_eq!(
            out.poses.len(),
            3,
            "the `<` pass keeps its cursor and the loop goes round"
        );
        assert_eq!(out.speech.as_deref(), Some("a  b  c"));
    }

    /// **`pose` itself, over a table built here** — the wire string and the echo string are two
    /// different fields and must not be swapped.
    ///
    /// **Added because a mutation that swapped them SURVIVED this crate's whole `--lib` suite.**
    /// Every other test in this module drives [`public_chat`] through a `stub` closure and never
    /// calls [`pose`], so `dereth-client-model --lib` structurally could not observe the swap — the fourth
    /// reading of a surviving test. `dereth-client`'s `o540_emote_send` catches it
    /// against the *shipped* table; this catches it here, where the function lives, on a table
    /// small enough to read.
    #[test]
    fn pose_puts_other_emote_on_the_wire_and_my_emote_in_the_echo() {
        let t = ChatPoseTable {
            id: dereth_primitives::DataId(0x0E00_0007),
            pose_buckets: 1,
            poses: vec![
                ("wave".into(), "Wave".into()),
                ("scratch head".into(), "ScratchHead".into()),
            ],
            emote_buckets: 1,
            emotes: vec![
                ("Wave".into(), ("wave.".into(), "waves.".into())),
                (
                    "ScratchHead".into(),
                    ("scratch your head.".into(), "scratches %p head.".into()),
                ),
            ],
        };
        let s2c = |name: &str| {
            if name == "Wave" {
                Some(0x1000_0001)
            } else {
                None
            }
        };

        let p = pose(&t, "WAVE", 1, s2c).expect("the first hash is case-insensitive");
        assert_eq!(p.motion_name, "Wave");
        assert_eq!(p.motion_command, Some(0x1000_0001));
        assert_eq!(
            p.soul_emote.as_deref(),
            Some("waves."),
            "the third-person text goes on the wire"
        );
        assert_eq!(
            p.my_emote.as_deref(),
            Some("wave."),
            "the first-person text is echoed locally"
        );
        assert_ne!(
            p.soul_emote, p.my_emote,
            "the two are different strings, which is the point"
        );

        // `%p` is substituted in the wire string only, and by the *sender's* gender.
        let m = pose(&t, "scratch head", 1, s2c).expect("shipped-shaped row");
        let f = pose(&t, "scratch head", 2, s2c).expect("shipped-shaped row");
        assert_eq!(m.soul_emote.as_deref(), Some("scratches his head."));
        assert_eq!(f.soul_emote.as_deref(), Some("scratches her head."));
        assert_eq!(
            m.my_emote.as_deref(),
            Some("scratch your head."),
            "no %p in the first-person text"
        );
        assert_eq!(
            m.motion_command, None,
            "string2command missed, so Pose returns false"
        );

        // A pose name the first hash does not have: nothing at all happens.
        assert!(pose(&t, "xyzzy", 1, s2c).is_none());
        // …and a motion name the SECOND hash does not have is the same silent miss, which is a
        // different lookup and a different way to fail.
        let dangling = ChatPoseTable {
            emotes: Vec::new(),
            ..t.clone()
        };
        assert!(
            pose(&dangling, "wave", 1, s2c).is_none(),
            "the emote table missed"
        );
    }

    /// **The one input on which "was sent" and "was removed" disagree.**
    ///
    /// `Pose`'s return value is set in exactly one place, inside
    /// the test that the motion name resolves to a command. The `Request::SoulEmote` and the local echo
    /// are **below** that and are not conditional on it. So a pose whose emote
    /// strings resolve but whose motion name is not in `command_strings` is broadcast to
    /// everybody **and** left in the line the player says.
    #[test]
    fn a_pose_with_no_motion_command_is_sent_and_is_not_removed() {
        let out = public_chat("hello *wave* there", stub(&["wave"], false));
        assert_eq!(
            out.speech.as_deref(),
            Some("hello *wave* there"),
            "the pose lookup returned false, so the marker text was not replaced"
        );
        assert_eq!(
            out.poses.len(),
            1,
            "and yet the soul emote was still composed"
        );
        assert_eq!(out.poses[0].soul_emote.as_deref(), Some("waves."));
        assert_eq!(out.poses[0].my_emote.as_deref(), Some("wave."));
        assert_eq!(out.poses[0].motion_command, None);
    }

    /// Oracle: the emote-list command's single literal, byte for byte.
    ///
    /// The literal is pinned here and nowhere else; [`emote_list_text`] builds the same string
    /// from [`EMOTE_LIST_HEADER`] and [`STANDARD_EMOTES`], so a drifting separator, a lost
    /// terminator or a mistyped name reddens this and only this. A transcribed constant needs one
    /// independent literal pin, or the symbol remains self-consistent and unfalsifiable.
    #[test]
    fn the_emote_list_is_the_shipped_literal() {
        const SHIPPED_EMOTE_LIST: &str = "Standard Emotes:\nNote: These commands should be bound on either side by asterisks. (Example: *wave*)\nShakeFist; Beckon; BeSeeingYou; BlowKiss; BowDeep; ClapHands; Cry; Laugh; Nod; Point; Shrug; Wave; Akimbo; HeartyLaugh; Salute; TapFoot; WaveHigh; WaveLow; Yawn; Stretch; Cringe; Kneel; Plead; Shiver; Shoo; Slouch; Spit; Surrender; Woah; Winded; YMCA; Eat; Drink; Teapot; Pray; Mock; Cheer; Helper; Warm Hands; Scratch Head; Shake Head\n\n";
        assert_eq!(emote_list_text(), SHIPPED_EMOTE_LIST);
        assert_eq!(SHIPPED_EMOTE_LIST.len(), 438, "the literal, NUL excluded");
        let (ty, announce, window) = (
            EMOTE_LIST_TEXT_TYPE,
            EMOTE_LIST_ANNOUNCE_TO_PLUGINS,
            EMOTE_LIST_WINDOW_ID,
        );
        assert_eq!(ty, 0);
        assert_eq!(u32::from(announce), 0, "announceToPlugins is 0");
        assert_eq!(window, 1);
    }

    /// Oracle: §3's, printed verbatim in the document.
    #[test]
    fn the_standard_emote_list_is_the_documented_forty() {
        assert_eq!(STANDARD_EMOTES.len(), 41);
        assert_eq!(STANDARD_EMOTES[0], "ShakeFist");
        assert_eq!(STANDARD_EMOTES[11], "Wave");
        assert_eq!(STANDARD_EMOTES[30], "YMCA");
        assert_eq!(STANDARD_EMOTES[38], "Warm Hands");
        assert_eq!(STANDARD_EMOTES[39], "Scratch Head");
        assert_eq!(STANDARD_EMOTES[40], "Shake Head");
        // Every name is distinct.
        let mut v = STANDARD_EMOTES.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 41);
        assert!(EMOTE_LIST_HEADER[1].contains("*wave*"));
    }
}
