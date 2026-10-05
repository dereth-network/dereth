//! Incoming chat, speaker state, and scroll delivery.

use super::*;

impl Hud {
    /// Scroll insertion's prefix step, for the lines that do not pass through
    /// [`dereth_client_model::scroll::Scroll`].
    ///
    /// Every reproduced chat handler ends in one shared scroll insertion, and that operation is
    /// what stamps. It does so exactly once per line, so this runs
    /// over a batch rather than at each `chat.push`, and it never overwrites a prefix that is
    /// already there (nothing produces one yet, and if something ever does, a silently clobbered
    /// prefix is the kind of thing no test would notice).
    ///
    /// Both counters move, always: a batch with the option off is a batch that ran and declined,
    /// and it must not read like a stamper that never ran.
    pub(super) fn stamp_timestamps(
        &mut self,
        world: &dereth_client_model::World,
        lines: &mut [ChatMessage],
    ) {
        for m in lines {
            if m.prefix.is_some() {
                continue;
            }
            if dereth_client_model::scroll::wants_timestamp(
                u32::from(m.ty),
                world.scroll.display_time_stamps,
            ) {
                m.prefix = Some(dereth_client_model::scroll::timestamp_prefix(
                    world.scroll.now_unix,
                    world.scroll.utc_offset_secs,
                ));
                self.stats.timestamps_stamped += 1;
            } else {
                self.stats.timestamps_suppressed += 1;
            }
        }
    }

    /// Host's actual gameplay subscriber, before processing this network batch.
    pub fn set_turbine_chat_generation(&mut self, generation: Option<u64>) {
        self.turbine_chat_generation = generation;
        self.pending_chat
            .retain(|(g, _)| g.is_none() || *g == generation);
    }

    /// **Display-string fan-out.**
    ///
    /// This is the last link of the chain `dereth_client_model::scroll` documents. The
    /// `Notice::DisplayString` emitters in `dereth_client_model` and `interaction.rs` are reached
    /// from every refusal call site, and three combat-state refusal paths add their line to the
    /// chat scroll directly; without this drain none of them would say anything.
    ///
    /// The notice has **two** registered receivers and they are not alternatives:
    ///
    /// | receiver | test | what it does |
    /// |---|---|---|
    /// | chat windows | the window id is the window's own id, else it is 0 and the type is active | appends the grey prefix and the body to that window's log, the body in the type's own colour |
    /// | spew strip | `type == 0x1A` | queues the bubble strip across the top of the viewport |
    ///
    /// **and the split between them is the whole answer to "where does a refusal land".** The main
    /// chat window's default text-type filter is `0xFBFFFFFF` — bit 26 clear — so type `0x1A`,
    /// the channel every client-generated message in this build uses, is the one type the
    /// scrollback drops, and no floaty window's default carries it either. A player who ticks the
    /// **Error** group (`0x04000000`) in the chat options gets it in both places; by default it is
    /// the spew box alone. Sending it to both from here is not a guess about which one wins — it is
    /// the client's fan-out, with each receiver's own test left where the client put it.
    pub(super) fn collect_scroll(
        &mut self,
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
    ) -> Vec<ChatMessage> {
        let mut lines = Vec::new();
        for line in world.scroll.drain() {
            let ty = u8::try_from(line.chat_type).unwrap_or(0);
            let took = panels.spew_offer(ty, &line.body, line.feedback);
            if took {
                self.stats.spew_lines += 1;
            }
            if crate::trace::notice() {
                tracing::debug!(
                    target: "dereth::trace::notice",
                    "notice-trace line from the client scroll: type={:#x} window={} \
                     text={:?} -> spew box took={took}; queued for the chat windows",
                    ty,
                    line.window,
                    line.body
                );
            }
            lines.push(ChatMessage {
                feedback: line.feedback,
                ty,
                body: line.body,
                prefix: line.prefix,
                window: line.window,
            });
            self.stats.scroll_lines += 1;
        }
        lines
    }

    pub(super) fn drain_scroll(
        &mut self,
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        chat: &mut Vec<ChatMessage>,
    ) {
        let lines = self.collect_scroll(world, panels);
        // The chat half is queued exactly as a network line is; `drive` routes it through the five
        // interfaces, where the filter that drops `0x1A` lives. Only this drain's lines are
        // appended: it also runs before incoming Turbine callbacks to preserve event order.
        self.pending_chat
            .extend(lines.iter().cloned().map(|m| (None, m)));
        chat.extend(lines);
    }

    /// The tail all four combat notification-event handlers share.
    ///
    /// Unless `is_squelched(0, "", 6)`, the text is added to the chat scroll as
    /// `(text, type, true, 0)`.
    /// The squelch call really does pass a **zero object id and an empty account name**: the gate
    /// is on the *type* alone (the combat text type, 6), not on who hit you — which is why a
    /// per-speaker squelch cannot mute your own combat log.
    pub(super) fn push_combat_line(
        &mut self,
        world: &dereth_client_model::World,
        chat: &mut Vec<ChatMessage>,
        text_type: u32,
        body: String,
    ) {
        if world.chat.is_squelched(
            dereth_primitives::ObjectId(0),
            "",
            dereth_client_model::chat::text_type::COMBAT,
        ) {
            self.stats.combat_lines_squelched += 1;
            return;
        }
        chat.push(ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: u8::try_from(text_type).unwrap_or(0),
            body,
            prefix: None,
            window: 0,
        });
        self.stats.combat_lines += 1;
    }

    // ---- the three names the reply key and the text replacements read -----------------------
    //
    // `dereth_client_contract::chat::window::ReplyTargets` is read at two sites in
    // `GamePlayScreen`, and `dereth_client_model::chat::ChatState::last_teller` /
    // `last_teller_name` are transcribed from the last-teller id and name setters. Everything
    // below is the producer.
    //
    /// The clickable-player id range. The `Communication_HearDirectSpeech` handler and the
    /// ranged-talk handler both spell it `0x50000000 < id && id < 0x70000000`, so
    /// it is **exclusive at both ends** and this constant pair is written that way rather than as
    /// a tidier inclusive range.
    ///
    /// **This is the whole misdirection guard.** Of the **126** `0x02BD` tells in the capture
    /// corpus, **126** are from ids outside it — `Sparring Golem` at `0x8000_0DE9`,
    /// `Academy Researcher` at `0x77F0_xxxx` and fourteen more NPCs — every one of them addressed
    /// to the player. Without the test, `@r hi ` after any tutorial NPC speaks composes a tell to
    /// the golem.
    const CLICKABLE_PLAYER_IDS: std::ops::Range<u32> = 0x5000_0001..0x7000_0000;

    /// Direct-speech handling records the last teller id and trimmed name after displaying a line,
    /// but only when the local player is the target and the sender id is in the clickable-player
    /// range.
    ///
    /// Three gates, and each one is a different way for a reply to go to the wrong person:
    ///
    /// * **`targetID == me`.** A tell the shard copied to us but addressed elsewhere — an
    ///   admin or a monitored line — must not become the reply target.
    /// * **`senderID != targetID`.** The client takes that pair down its own branch (`You
    ///   think, "…"`) well before this code and never reaches the last-teller setter, so a tell to
    ///   yourself does not arm `@r`. Written as its own test rather than left implicit, because
    ///   *self* is inside the player range and would otherwise pass.
    /// * **the id range.** The 126-of-126 case above.
    ///
    /// The sender name is passed through `trim_language_marker` before it becomes
    /// the reply target name.
    pub(super) fn note_last_teller(
        &mut self,
        world: &mut dereth_client_model::World,
        m: &dereth_protocol::comms::CommunicationHearDirectSpeech,
    ) {
        let mine = self.player == Some(m.target_id);
        let from_a_player = Self::CLICKABLE_PLAYER_IDS.contains(&m.sender_id.0);
        if !mine || m.sender_id == m.target_id || !from_a_player {
            self.stats.last_teller_declined += 1;
            return;
        }
        world.chat.last_teller = Some(m.sender_id);
        world.chat.last_teller_name = trim_language_marker(&m.sender_name).to_owned();
        self.stats.last_teller_writes += 1;
    }

    /// Compose and deliver one heard emote.
    ///
    /// One body for both opcodes because that is the shape of retail: soul-emote hearing does
    /// its self-echo test, appends `"^"` to the name, and **calls this function**.
    /// Writing the composition twice would have been two transcriptions of one retail arm.
    ///
    /// The order is the function's own and every step of it is observable in the counters:
    ///
    /// 1. `CanHear(senderID, 0xC)` — refused means *return*, with nothing composed.
    ///    `account` is `""` because `CanHear` builds its own empty string,
    ///    and the type is the literal `0xC` (the emote text type), which is not on the wire:
    ///    neither `0x01E0` nor `0x01E2` carries a text type.
    /// 2. the `^` then `&` search over the **sender name**, trailing-trimmed
    ///    ([`dereth_client_model::chat::composition::language_marker`]).
    /// 3. the language decision ([`dereth_client_model::chat::composition::is_untranslated`]).
    /// 4. the composition ([`dereth_client_model::chat::composition::hear_emote_line`]) and
    ///    adding the line to the chat scroll as `(line, 0xC, true, 0)`.
    ///
    /// **Step 3's `true` arm** replaces
    /// the text with a random Olthoi / human phrase — ten
    /// fixed phrases each, picked by `(1, 10)`, transcribed as
    /// [`dereth_client_model::chat::composition::OLTHOI_TEXT`] / [`dereth_client_model::chat::composition::HUMAN_TEXT`] — and composes
    /// [`dereth_client_model::chat::composition::GARBLED`] through [`dereth_client_model::chat::composition::garbled_line`] instead of the
    /// apostrophe-aware join. [`HudStats::emote_lines_untranslated`] counts lines that were
    /// actually garbled. See [`Self::garbled_or_plain`], which is the one producer all three arms
    /// share.
    pub(super) fn hear_emote(
        &mut self,
        world: &dereth_client_model::World,
        chat: &mut Vec<ChatMessage>,
        sender: ObjectId,
        sender_name: &str,
        text: &str,
    ) {
        const EMOTE: u32 = dereth_client_model::chat::text_type::EMOTE;

        // Step 1, and genuinely first: test whether the listener can hear this sender before
        // doing any composition work.
        let ps = self.speaker_player_space(sender);
        let radius = dereth_client_contract::radar::radar_range(self.player_outside());
        if !world.chat.can_hear(sender, "", EMOTE, ps, radius) {
            if world.chat.is_squelched(sender, "", EMOTE) {
                self.stats.emote_lines_squelched += 1;
            } else {
                self.stats.emote_lines_out_of_earshot += 1;
            }
            return;
        }

        // Steps 2 and 3. The creature-type predicate is `Self::is_olthoi`.
        let (marker, name) = dereth_client_model::chat::composition::language_marker(sender_name);
        let name = name.to_owned();
        // Step 4. The garble arm composes `GARBLED`; the plain arm composes by hand and omits the
        // space before an apostrophe.
        let line = match self.garbled_or_plain(marker, &name, world) {
            Some(g) => {
                self.stats.emote_lines_untranslated += 1;
                g
            }
            None => dereth_client_model::chat::composition::hear_emote_line(&name, text),
        };
        chat.push(speech(EMOTE, line));
        self.stats.emote_lines_composed += 1;
    }

    /// Whether the local player is Olthoi.
    ///
    /// The listener half of the language test, used at
    /// **two** seams in this file: here and the player-description handler's
    /// enabling of the chat talk focuses in the `0x0013` arm. Both are this one call, and
    /// they read the same local player description the client does: the canonical local-player
    /// object row, queried for `PropertyInt 0xBC HeritageGroup` against 12 and 13. See
    /// [`dereth_client_model::chat::composition::OLTHOI_HERITAGE_GROUPS`].
    ///
    /// No player description yet reads `0` — the int-quality query's own answer for an absent
    /// property — which is not Olthoi.
    #[must_use]
    pub fn is_olthoi(&self, world: &dereth_client_model::World) -> bool {
        let heritage = self.player_desc(world).map_or(0, |q| {
            q.inq_int(dereth_client_contract::panels::inventory::HERITAGE_GROUP_PROPERTY)
        });
        dereth_client_model::chat::composition::is_olthoi(heritage)
    }

    /// Seed the chat path's generator only.
    ///
    /// Retail seeds its one random generator with `(long)time(NULL)`, which
    /// is [`crate::audio::ran2_seed`] and is what [`Self::garble_roll`] uses if nothing has called
    /// this. Exposed so that a caller — today, a test — can make the draw reproducible; a
    /// generator whose sequence cannot be pinned is a generator no assertion can check, which is
    /// how `dereth_primitives::num::rng`'s own callers are all written.
    pub fn seed_random(&mut self, seed: i32) {
        self.garble_rng = Some(dereth_primitives::num::rng::Ran2::new(seed));
    }

    /// One inclusive draw from 1 through 10. See [`Self::garble_rng`].
    fn garble_roll(&mut self) -> i32 {
        let (lo, hi) = dereth_client_model::chat::composition::GARBLE_ROLL;
        self.garble_rng
            .get_or_insert_with(|| {
                dereth_primitives::num::rng::Ran2::new(crate::audio::ran2_seed_at(
                    crate::platform::clock::system_unix_time(),
                ))
            })
            .roll_i32(lo, hi)
    }

    /// **The one garble producer, shared by all three arms that have one.**
    ///
    /// Returns `Some(line)` when this listener cannot understand this speaker — in which case the
    /// line is already composed, because the substitution and the format are the same in all
    /// three handlers — and `None` when the caller should compose normally.
    ///
    /// The three callers are the `0x02BB`, `0x02BD` and `0x01E0`/`0x01E2` arms. Together they are
    /// every use of the random garbled-text producers, so "three arms" is a count and not an
    /// estimate. `0x02BC`
    /// (ranged talk) is **not** one of them: it never asks whether the player is Olthoi,
    /// never looks for a marker and cannot garble.
    ///
    /// `no_olthoi_talk` is queried as local-player property `0x81 NoOlthoiTalk`
    /// in the emote arm and in the direct-speech arm. A missing local player description is
    /// retail's null-interface arm and answers the same `false`,
    /// which is why this is `is_some_and` and not an `Option` three-way.
    pub(super) fn garbled_or_plain(
        &mut self,
        marker: dereth_client_model::chat::composition::LanguageMarker,
        trimmed_name: &str,
        world: &dereth_client_model::World,
    ) -> Option<String> {
        let no_olthoi_talk = self
            .player_desc(world)
            .is_some_and(|q| q.inq_bool(bool_property::NO_OLTHOI_TALK));
        if !dereth_client_model::chat::composition::is_untranslated(
            marker,
            self.is_olthoi(world),
            no_olthoi_talk,
        ) {
            return None;
        }
        // The **speaker's** flag chooses the table, not the listener's.
        let speaker_is_olthoi =
            marker == dereth_client_model::chat::composition::LanguageMarker::Ampersand;
        let roll = self.garble_roll();
        Some(dereth_client_model::chat::composition::garbled_line(
            trimmed_name,
            dereth_client_model::chat::composition::random_text(roll, speaker_is_olthoi),
        ))
    }

    /// The `Communication_ChannelBroadcast` handler's two user-name stores; see the
    /// `0x0147` arm for what of that handler is and is not reproduced.
    ///
    /// The channel constants are `0x4000` for Monarch and `0x2000` for Patron. Every other
    /// channel stores nothing, including Vassals and Allegiance, which is why there is no `@vr`.
    pub(super) fn note_at_channel_speaker(
        &mut self,
        m: &dereth_protocol::comms::CommunicationChannelBroadcastRecv,
        world: &mut dereth_client_model::World,
    ) {
        // A buffer length of 1 means the string holds only its terminator. An empty
        // sender name is the shard echoing our **own** broadcast, and the client takes the whole
        // `"You say to your …"` branch, which contains neither call.
        if m.sender_name.is_empty() {
            return;
        }
        let name = trim_language_marker(&m.sender_name).to_owned();
        match m.channel {
            channel::MONARCH => world.chat.last_monarch_sender = name,
            channel::PATRON => world.chat.last_patron_sender = name,
            _ => return,
        }
        self.stats.at_channel_name_writes += 1;
    }

    /// The reply target triple read by reply-key handling and text replacement, assembled from the
    /// two places this client keeps it.
    ///
    /// An empty name is `None` here because that is what the two consumers do with it: the reply
    /// key returns early on an empty name and the replacement tests for a length of 1. The
    /// distinction between "no name" and "the empty name" does not exist in the client: both are
    /// represented by the same stored name field.
    #[must_use]
    pub fn reply_targets(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::window::ReplyTargets {
        world.chat.reply_targets()
    }

    /// The chat lines waiting for the chat windows, oldest first, for the screen generation
    /// `screen`: a line raised for an earlier generation of the screen (a notice of a window that
    /// has since been rebuilt) is dropped, and a line tied to no screen is kept. The queue is
    /// emptied either way, so no line is delivered twice. The lines handed again
    /// ([`Self::replayed_chat`]) come first; the queue's lines are also recorded in
    /// [`Self::delivered_chat`].
    pub fn take_chat_lines(&mut self, screen: u64) -> Vec<ChatMessage> {
        let lines: Vec<_> = std::mem::take(&mut self.pending_chat)
            .into_iter()
            .filter(|(generation, _)| !generation.is_some_and(|g| g != screen))
            .map(|(_, m)| m)
            .collect();
        self.delivered_chat.extend(lines.iter().cloned());
        let mut out = std::mem::take(&mut self.replayed_chat);
        out.extend(lines);
        out
    }

    /// The communication state projected into either interface's target menu.
    #[must_use]
    pub fn chat_focus_view(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::mainchat::ChatFocusView {
        let selected = world.selected_chat_player();
        let mut enabled = world.chat.enabled_focuses();
        let mut selectable = world.chat.selectable_focuses();
        enabled[2] = selected.is_some();
        selectable[2] = selected.is_some();
        dereth_client_contract::chat::mainchat::ChatFocusView {
            focus: world.chat.talk_focus as u32,
            enabled,
            selectable,
            is_olthoi: self.is_olthoi(world),
            target: selected.and_then(|id| {
                world.weenie(id).map(
                    |w| dereth_client_contract::chat::mainchat::SpeakableTarget {
                        id: id.0,
                        name: w.object_name(dereth_client_model::weenie::NameType::Appropriate),
                        talkable: w.is_talkable(),
                        squelched: world.chat.is_squelched(id, "", 1),
                    },
                )
            }),
        }
    }
}

/// Remove a trailing language marker from the sender name through [`dereth_client_model::chat::composition::language_marker`].
/// An unmarked name is returned unchanged.
fn trim_language_marker(name: &str) -> &str {
    dereth_client_model::chat::composition::language_marker(name).1
}

/// The shift from UTC to the zone `localtime` would have used for `at`, in seconds.
///
/// Scroll insertion uses the local wall-clock time. The host supplies the offset through
/// [`crate::platform::clock::local_utc_offset_secs`]; the runtime performs no platform query.
///
/// It takes the instant because the answer depends on it — a daylight rule is not a constant.
pub(super) fn utc_offset_secs(at: i64) -> i32 {
    crate::platform::clock::local_utc_offset_secs(at)
}

/// `time(NULL)` — seconds since the Unix epoch, read at the moment a batch of lines lands, which
/// is where the client reads it.
///
/// A clock before the epoch is not representable and is not a case this build has to render, so it
/// falls back to 0 rather than panicking in a chat path.
pub(super) fn wall_clock_unix() -> i64 {
    crate::platform::clock::system_unix_time()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// One composed speech line on its way to the chat scroll.
///
/// `body` is the **whole** line — speaker, verb, comma and quotes — as one of the seven fixed
/// templates in [`dereth_client_model::chat::composition`] composed it. `prefix` is `None` here because the notice's prefix
/// slot is the **timestamp**, and scroll insertion is what fills it; see
/// [`crate::hud::Hud::stamp_timestamps`]. Putting the speaker's name in the prefix slot and the
/// raw message in the body would draw a grey `Lark` abutting a bare `W`.
///
/// `window` is 0: both speech handlers pass their fifth argument straight through, and it is the
/// broadcast id — the final-string display notice then offers the line to every
/// window whose 64-bit text-type filter accepts the type.
pub(super) fn speech(text_type: u32, body: String) -> ChatMessage {
    ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: u8::try_from(text_type).unwrap_or(0),
        body,
        prefix: None,
        window: 0,
    }
}

/// The tag a player-killer death broadcast carries in its `0xF7E0` text.
pub const PK_DEATH_TAG: &str = "[PKDe]";

/// What the system-line handler does with one `0xF7E0` text before its squelch gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PkDeathLine {
    /// No death tag: the line is drawn as sent.
    Untagged,
    /// Tagged, and the hear-PK-deaths option is off: the line is not drawn at all.
    Dropped,
    /// Tagged and heard: the line with every copy of the tag removed.
    Stripped(String),
}

/// Decide a `0xF7E0` line's fate by the player-killer death tag and the hear-PK-deaths option.
///
/// The tag is searched for anywhere in the text. A tagged line is dropped when the option is off;
/// when it is on, every copy of the tag is replaced by nothing and the rest of the text is left
/// exactly as sent (a space beside the tag stays).
#[must_use]
pub fn pk_death_filter(text: &str, hear_pk_deaths: bool) -> PkDeathLine {
    if !text.contains(PK_DEATH_TAG) {
        PkDeathLine::Untagged
    } else if !hear_pk_deaths {
        PkDeathLine::Dropped
    } else {
        PkDeathLine::Stripped(text.replace(PK_DEATH_TAG, ""))
    }
}

/// One Fellowship-panel line on its way to the chat scroll.
///
/// The composers in [`dereth_client_model::chat::composition`] return the fixed literal with its trailing newline;
/// scroll insertion's first act is `trim(text, true, true, L"\n")`, applied here. Chat type 0 —
/// every one of those five functions pushes `0` ( …).
pub(super) fn fellowship_ui_line(body: String) -> ChatMessage {
    speech(
        dereth_client_model::chat::composition::FELLOWSHIP_UI_CHAT_TYPE,
        dereth_client_model::chat::composition::add_text_to_scroll_trim(&body).to_owned(),
    )
}

/// A failure-event line on its way to the scroll: the same trim, the arm's own
/// type kept. The `0x028A` / `0x028B` arms push the trimmed literal.
pub(super) fn failure_line(m: ChatMessage) -> ChatMessage {
    ChatMessage {
        body: dereth_client_model::chat::composition::add_text_to_scroll_trim(&m.body).to_owned(),
        ..m
    }
}
