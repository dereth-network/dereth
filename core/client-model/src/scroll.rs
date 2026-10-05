//! The chat scroll — **where a client-generated notice becomes text on
//! the screen**.
//!
//! Every `Notice::DisplayString` this crate raises — every inventory refusal, the
//! unblock retry's *"Moving %s to your backpack"*, `" - cannot unwield the %s"`, the fifteen
//! *"You're already wearing …"* strings, *"You can't put that item there"*, the four `@tell`
//! refusals — reaches the screen through here. This module is the client's own
//! chain between the notice and the screen:
//!
//! ```text
//! display-string notice (channel, text), produced in this crate
//!   -> communication-system string-info notice
//!        -> the scroll's add-text entry point (text, channel, true, 0)
//!             -> final string-info notice (type, body, prefix, window)
//!                  -> final string-info notice for five chat windows
//!                  -> final string-info notice for speech bubbles
//! ```
//!
//! # The channel is the chat type
//!
//! It is easy to read the middle step as `add_text_to_scroll(text, 0, (bool)channel, 1)` — type
//! **0**, the channel used as a boolean, window **1**. That is wrong: the add-text call receives
//! `(text, channel, 1, 0)`. The plug-in announce flag is the third argument, the value compared
//! against `0x1A` is the second, and the window id is the fourth.
//!
//! So **`add_text_to_scroll(text, chat_type = channel, announce_to_plugins = true, window = 0)`**, and
//! the channel a notice carries is the chat type — which selects the colour (the 34-entry table,
//! `dereth_ui_screens::chat::colors`) *and* the destination (the chat interface's per-window 64-bit
//! filter, and the spew box's `type == 0x1A` test).
//!
//! # Where `0x1A` actually lands, which is not the chat log
//!
//! The main window's default filter is `0xFBFFFFFF` — bit 26 clear —
//! so type `0x1A` is the one type the main window drops, and no floaty window's default carries it
//! either. The spew box accepts **only** type
//! `0x1A`. The client-generated feedback channel therefore draws in the over-head/on-screen bubble
//! list at the top of the viewport, never in the scrollback, unless the player ticks the
//! **Error** filter group (`0x04000000`) on. Both halves are fanned out from here.

/// One final string-info notice carrying type, body, prefix, and window id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FinalString {
    pub feedback: dereth_client_contract::feedback::Feedback,
    /// The chat type, which is a notice's channel unchanged.
    pub chat_type: u32,
    /// The body: the trimmed text.
    pub body: String,
    /// The prefix: the timestamp for a type that gets one, empty otherwise. Drawn in
    /// colour index 12 whatever the body's type is.
    pub prefix: Option<String>,
    /// The window id — `0` is a broadcast, subject to each window's filter.
    pub window: u32,
}

/// The client's text scroll: the queue between a notice and the frame that draws it.
///
/// The client has no queue — the final string-info notice is a synchronous fan-out to the
/// registered handlers. Here the producers (`dereth_client_model`, driven from
/// `dereth_client_runtime::interaction`) and the consumers (`dereth_client_runtime::hud`, driven
/// from the frame's step 7) are separate borrows of the same `World`, so the fan-out is deferred
/// to whichever of them runs next. That is at most one
/// frame of latency and it is stated rather than hidden: `Hud::apply_events` drains this at the top
/// of the frame, and `interaction_use_time` — which raises the pick/use notices — runs after step
/// 7, so a refusal produced by a double-click is drawn on the following frame.
#[derive(Debug, Default, Clone)]
pub struct Scroll {
    /// Waiting for a consumer.
    pending: Vec<FinalString>,
    /// How many `add_text_to_scroll` calls this scroll has taken, ever. A counter and not a length,
    /// because the queue is drained: a test that asserts "nothing was raised" must be able to tell
    /// that from "something was raised and consumed".
    pub added: u64,
    /// How many of those were eligible for the chat log file — every type except `0x1A`. This
    /// historical counter moves whether or not an output file is currently open.
    pub logged: u64,
    /// Behavior: bit 6 of the second option word, the option that puts
    /// `%#H:%M:%S ` in the prefix.
    ///
    /// `dereth_client_runtime::hud::Hud::apply_events` pushes it from
    /// `PlayerSystem::options` at the head of every batch, the way `set_lock_ui` and
    /// `reply_targets` are pushed; without that producer the prefix could never be anything but
    /// empty. `false` is the shipped default
    /// (the default second options word `0x00948700`, bit 6 clear).
    pub display_time_stamps: bool,
    /// Whether language filtering is enabled, followed by the table it gates. The host pushes both,
    /// just as it pushes [`Self::display_time_stamps`], because this crate has no dat store.
    pub filter_language: bool,
    pub taboo_table: Option<std::sync::Arc<dereth_assets::TabooTable>>,
    /// The host's text conversion, for the censor check's narrowing and for nothing else
    /// here. Pushed for the same reason [`Self::taboo_table`] is: this crate has no
    /// platform, so it cannot read an ANSI code page. The default is the
    /// workspace's 1252 table; `dereth_client_runtime::platform::text::install` puts the `kernel32`
    /// arm on the production `World`.
    pub encoding: crate::HostText,
    /// The clock `add_text_to_scroll` reads — seconds since the Unix epoch, host-pushed for the
    /// same reason [`Self::display_time_stamps`] is: this crate has no clock and the client reads one
    /// at the moment the line lands. See [`timestamp_prefix`] for why the offset is separate.
    pub now_unix: i64,
    /// The shift from UTC to the zone `localtime` would have used, for [`Self::now_unix`].
    /// `dereth_client_runtime::hud` fills it from
    /// `dereth_client_runtime::platform::clock::local_utc_offset_secs`; a literal `0` would stamp
    /// in UTC. See [`timestamp_prefix`].
    pub utc_offset_secs: i32,
    /// The client's one log-file handle, shared by snapshots of this scroll just as the native handle is
    /// process-owned. Cloning a `World` must neither duplicate an append nor close another clone's
    /// handle when it is dropped.
    output_file: std::sync::Arc<std::sync::Mutex<Option<ChatOutput>>>,
}

#[derive(Debug)]
struct ChatOutput {
    name: String,
    /// The host's handle, so this crate itself does no file-system i/o outside its tests.
    sink: Box<dyn dereth_primitives::TextSink>,
}

/// The chat type special-cases: no timestamp prefix, and nothing
/// written to the chat log file. It is also the message-log panel's only accepted type and the one
/// bit the main chat window's default filter clears.
pub const LOCAL_ERROR_TYPE: u32 = 0x1A;

/// `L"%#H:%M:%S "` — the `wcsftime` format the chat scroll pushes.
///
///
/// A **wide** literal, transcribed here as a Rust `&str` because the only thing this build does
/// with it is reproduce its output. Note the **trailing space**: it is part of the format, and it
/// is the whole of the gap between the stamp and the line in the drawn log — there is no separator
/// glyph and the prefix run is not padded by the window.
///
/// MSVC's `#` flag on `%H` means *remove the leading zero*, so nine in the morning is `9:` and not
/// `09:`; `%M` and `%S` carry no flag and stay two digits. `%H` is the 24-hour clock, so midnight
/// is `0:00:00 `. Reproduced by [`timestamp_prefix`].
///
/// **This constant is deliberately not read by [`timestamp_prefix`]** — there is no `strftime` in
/// this workspace to feed it to. It exists so the format has one place where it is stated
/// independently of the code that renders it. Without it, `timestamp_prefix`'s output is only ever
/// checked against strings written by the same author from the same reading, and a
/// mis-transcribed format is unfalsifiable. The independent literal is deliberate; test-only
/// readers are the point here, not a symptom.
pub const TIMESTAMP_FORMAT: &str = "%#H:%M:%S ";

/// The chat scroll's timestamp gate, both halves of it:
///
/// ```text
///   chat type == 0x1a                   -> no stamp, and no chat-log line either
///   display-timestamps option is off    -> no stamp   (bit 6 of the second option word)
/// ```
///
#[must_use]
pub fn wants_timestamp(chat_type: u32, display_time_stamps: bool) -> bool {
    chat_type != LOCAL_ERROR_TYPE && display_time_stamps
}

/// [`TIMESTAMP_FORMAT`] rendered — `wcsftime(dst, 0x400, L"%#H:%M:%S ", localtime(time(NULL)))`.
///
/// # Two things this signature says out loud
///
/// **`now_unix` is an input.** The client reads the clock inside the add-text entry point, at the
/// instant the line lands, so the prefix is not a function of anything on the wire. Passing it in is what
/// makes it assertable at all; it is the same shape as
/// `dereth_ui_screens::screens::disconnected::account_banned_message`, which took the same decision
/// for `asctime(localtime(&t))` for the same reason.
///
/// **`utc_offset_secs` is the zone.** The client formats with `localtime`, which MSVCR70 resolves
/// through `_tzset` → `GetTimeZoneInformation`; passing **0** renders the stamp in UTC. Every
/// Win32 entry point in the `windows` crate is an `unsafe fn` and this workspace forbids `unsafe`,
/// so the host reads the offset through WinRT, whose bindings are safe:
/// `dereth_client_runtime::platform::clock::local_utc_offset_secs`.
/// It stays a parameter rather than a clock read so a test can pin a zone.
#[must_use]
pub fn timestamp_prefix(now_unix: i64, utc_offset_secs: i32) -> String {
    let secs = (now_unix + i64::from(utc_offset_secs)).rem_euclid(86_400);
    format!("{}:{:02}:{:02} ", secs / 3600, (secs / 60) % 60, secs % 60)
}

impl Scroll {
    /// Preserve the process-owned log file across a character-session model reset. Retail closes
    /// the log in three places: shutdown, an explicit output command, and a replacement open.
    /// A session-model reset is not one of them.
    pub fn preserve_output_from(&mut self, old: &Self) {
        self.output_file = old.output_file.clone();
    }

    /// Start copying output to a file: close the current output first, then open
    /// the replacement in append/update mode. A failed replacement deliberately leaves logging
    /// stopped.
    ///
    /// The `fopen` belongs to the host, so `open` is a callback and is invoked at
    /// the point the original calls it — *after* [`Self::close_log_file`], which is what makes a failed
    /// replacement still close the old handle and still print its "Chat log … closed." line.
    /// `None` from `open` is that failed `fopen`. `name` is what the close prints: the path
    /// as the command spelled it. The return is simply whether logging is on, because this crate
    /// performs none of the i/o that could fail.
    pub fn start_copy_output_to_file(
        &mut self,
        name: &str,
        current_command_source: u32,
        open: impl FnOnce() -> Option<Box<dyn dereth_primitives::TextSink>>,
    ) -> bool {
        self.close_log_file(current_command_source);
        let Some(sink) = open() else { return false };
        let Ok(mut output) = self.output_file.lock() else {
            return false;
        };
        *output = Some(ChatOutput {
            name: name.to_owned(),
            sink,
        });
        true
    }

    /// Close the log file. Returns whether there was an active handle, which
    /// is the branch the argument-less output command uses for its two different messages.
    pub fn close_log_file(&mut self, current_command_source: u32) -> bool {
        let name = self
            .output_file
            .lock()
            .ok()
            .and_then(|output| output.as_ref().map(|output| output.name.clone()));
        let Some(name) = name else { return false };
        // Retail sends this literal through the add-text path
        // before `fclose`, so the line is visible and is also the last append to this file.
        self.add_text_to_scroll(
            &format!("Chat log {name} closed.\n"),
            0,
            true,
            current_command_source,
        );
        if let Ok(mut output) = self.output_file.lock() {
            output.take();
        }
        true
    }

    /// The `fprintf(log_file, "%ls%ls\n", prefix, body)` tail of the chat scroll's
    /// add-text entry point, for a line already filtered, trimmed and
    /// timestamped by its producer. The explicit CRLF is what MSVCR70 text mode produces on
    /// Windows; Rust append mode otherwise writes a bare LF.
    ///
    /// This is public for the incoming `Hud` handlers that still compose their final strings
    /// directly. They call it only for the newly received slice; scroll-drained lines have already
    /// passed this method, so a displayed line is appended once.
    pub fn copy_final_to_log(
        &self,
        chat_type: u32,
        prefix: Option<&str>,
        body: &str,
    ) -> std::io::Result<bool> {
        if chat_type == LOCAL_ERROR_TYPE {
            return Ok(false);
        }
        let mut output = self
            .output_file
            .lock()
            .map_err(|_| std::io::Error::other("chat output lock poisoned"))?;
        let Some(output) = output.as_mut() else {
            return Ok(false);
        };
        // One `fprintf`, prefix first. The legacy byte encoding on disk and the
        // CRLF that MSVCR70 text mode produces belong to the sink, because both are facts about
        // the host and not about the line. `dereth_client_runtime::platform::text::ChatLog` is the
        // sink the client installs and it writes windows-1252, per-character `?` where the table
        // has no byte, then the two-byte terminator.
        match prefix {
            Some(prefix) => output.sink.write_line(&format!("{prefix}{body}")),
            None => output.sink.write_line(body),
        }
        // The poisoned lock above is the only failure left here. A write that fails is the host's
        // to notice; ignores what `fprintf` returned.
        Ok(true)
    }

    /// The language-filter half of the add-text entry point, also used by the host for incoming
    /// handlers
    /// that still compose their final line directly before the shared queue is fully unified.
    #[must_use]
    pub fn filter_text(&self, text: &str) -> String {
        if self.filter_language {
            self.taboo_table.as_deref().map_or_else(
                || text.to_owned(),
                |table| {
                    dereth_rules::taboo::filter_chat_line(table, self.encoding.as_encoding(), text)
                },
            )
        } else {
            text.to_owned()
        }
    }

    /// The scroll's add-text entry point `(text, chat_type, announce_to_plugins, window)`.
    ///
    /// Three of the client's five steps are reproduced and two are deliberately not:
    ///
    /// | step | here |
    /// |---|---|
    /// | the plug-in announce, when `announce_to_plugins` | **not** — this build hosts no decal plug-in; the flag is carried so a call site's argument stays honest |
    /// | whitespace trim | yes, both ends |
    /// | the language filter's word replacement | yes — audience 1, mode 1, and the shipped table |
    /// | `if (type != 0x1A)` timestamp prefix and `fprintf(log_file, "%ls%ls\n", …)` | yes |
    /// | the final string-info notice `(type, body, prefix, window)` | yes — queued here |
    pub fn add_text_to_scroll(
        &mut self,
        text: &str,
        chat_type: u32,
        announce_to_plugins: bool,
        window: u32,
    ) {
        self.add_feedback_to_scroll(
            text,
            chat_type,
            announce_to_plugins,
            window,
            dereth_client_contract::feedback::Feedback::ORDINARY,
        );
    }

    /// Insert a line with the meaning supplied by its producing operation.
    pub fn add_feedback_to_scroll(
        &mut self,
        text: &str,
        chat_type: u32,
        announce_to_plugins: bool,
        window: u32,
        feedback: dereth_client_contract::feedback::Feedback,
    ) {
        let _ = announce_to_plugins;
        self.added += 1;
        let body = text.trim();
        let body = self.filter_text(body);
        if chat_type != LOCAL_ERROR_TYPE {
            self.logged += 1;
        }
        // `None` when no stamp is due, never `Some(String::new())`: an empty slot is exactly what
        // a missing producer looks like from the outside.
        let prefix = wants_timestamp(chat_type, self.display_time_stamps)
            .then(|| timestamp_prefix(self.now_unix, self.utc_offset_secs));
        let _ = self.copy_final_to_log(chat_type, prefix.as_deref(), &body);
        self.pending.push(FinalString {
            feedback,
            chat_type,
            body,
            prefix,
            window,
        });
    }

    /// The communication system's incoming display-string notice — the four arguments
    /// described in this module's header.
    pub fn on_display_string_info(&mut self, channel: u32, text: &str) {
        self.add_feedback_to_scroll(
            text,
            channel,
            true,
            0,
            dereth_client_contract::feedback::Feedback::LOCAL,
        );
    }

    /// Take what is queued. The consumer fans it out.
    pub fn drain(&mut self) -> Vec<FinalString> {
        std::mem::take(&mut self.pending)
    }

    /// What is queued but not yet drained, without taking it.
    #[must_use]
    pub fn pending(&self) -> &[FinalString] {
        &self.pending
    }

    /// Logging off takes the chat windows down with the screen, so a line
    /// still in flight at log-off is gone with them. The counters are not reset: they are this
    /// process's tally, not the session's.
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// The notice → scroll step, for every [`crate::Notice`] whose client-side handler ends in
/// the add-text entry point.
///
/// Exactly one variant does today. The other nineteen are consumed by panels and by per-frame state
/// tests. Returns whether the notice produced a line, so a
/// caller can count what it routed rather than assuming.
pub fn recv_notice(scroll: &mut Scroll, n: &crate::Notice) -> bool {
    match n {
        crate::Notice::DisplayString {
            channel,
            text,
            feedback,
        } => {
            scroll.add_feedback_to_scroll(text, *channel, true, 0, *feedback);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the argument order in this module's header — the channel becomes the chat type and
    /// the window id is 0, not the other way round.
    #[test]
    fn the_notice_channel_becomes_the_chat_type_and_the_window_is_zero() {
        let mut s = Scroll::default();
        s.on_display_string_info(0x1A, "You can't put that item there");
        let out = s.drain();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].chat_type, 0x1A);
        assert_eq!(out[0].window, 0);
        assert_eq!(out[0].body, "You can't put that item there");
    }

    /// Oracle: the client's `if (chat_type != 0x1a)` — the one type that gets no
    /// timestamp and no chat-log line.
    #[test]
    fn only_the_local_error_type_skips_the_log_and_the_timestamp() {
        let mut s = Scroll {
            display_time_stamps: true,
            now_unix: 9 * 3600 + 5 * 60 + 3,
            ..Scroll::default()
        };
        s.add_text_to_scroll("a refusal", LOCAL_ERROR_TYPE, true, 0);
        s.add_text_to_scroll("a broadcast", 0, true, 0);
        assert_eq!(s.added, 2);
        assert_eq!(s.logged, 1, "0x1A is not written to the chat log");
        let out = s.drain();
        assert!(out[0].prefix.is_none(), "0x1A gets no timestamp prefix");
        assert_eq!(out[1].prefix.as_deref(), Some("9:05:03 "));
    }

    #[derive(Debug)]
    struct FileSink(std::fs::File);

    impl dereth_primitives::TextSink for FileSink {
        fn write_line(&mut self, line: &str) {
            use std::io::Write as _;
            let bytes = dereth_primitives::text::cp1252::encode(line).unwrap_or_else(|| {
                line.chars()
                    .flat_map(|c| {
                        dereth_primitives::text::cp1252::encode(&c.to_string())
                            .unwrap_or_else(|| vec![b'?'])
                    })
                    .collect()
            });
            self.0.write_all(&bytes).expect("append the line");
            self.0.write_all(b"\r\n").expect("append the terminator");
            self.0.flush().expect("flush the line");
        }
    }

    fn append_sink(path: &std::path::Path) -> Option<Box<dyn dereth_primitives::TextSink>> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .ok()?;
        Some(Box::new(FileSink(file)))
    }

    /// The log file is one process-global handle, not one per copied scroll snapshot. A clone must
    /// share it without duplicating writes, and type `0x1A` must still miss the physical file.
    #[test]
    fn cloned_scrolls_share_one_append_handle_and_local_errors_stay_out() {
        let dir =
            std::env::temp_dir().join(format!("dereth-scroll-output-{}-clone", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
        }
        std::fs::create_dir_all(&dir).expect("create disposable output directory");
        let path = dir.join("chat.txt");
        let mut original = Scroll::default();
        assert!(
            original.start_copy_output_to_file(&path.to_string_lossy(), 7, || append_sink(&path)),
            "open append output"
        );
        let mut snapshot = original.clone();
        snapshot.add_text_to_scroll("shared once", 0, true, 0);
        original.add_text_to_scroll("not logged", LOCAL_ERROR_TYPE, true, 0);
        assert!(
            original.close_log_file(7),
            "the clone wrote through the shared active handle"
        );
        assert_eq!(
            original.drain().last().map(|line| line.window),
            Some(7),
            "closing the log routes its visible line to the current command source"
        );
        assert_eq!(
            std::fs::read(&path).expect("read append output"),
            format!("shared once\r\nChat log {} closed.\r\n", path.display()).as_bytes(),
            "a clone duplicated the write or let the excluded type into the file"
        );
        std::fs::remove_dir_all(&dir).expect("remove disposable output directory");
    }

    /// A failed replacement open still closed the previous handle.
    #[test]
    fn a_failed_replacement_open_still_closed_the_previous_handle() {
        let dir = std::env::temp_dir().join(format!(
            "dereth-scroll-output-{}-failed",
            std::process::id()
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
        }
        std::fs::create_dir_all(&dir).expect("create disposable output directory");
        let path = dir.join("chat.txt");
        let mut scroll = Scroll::default();
        assert!(scroll.start_copy_output_to_file(&path.to_string_lossy(), 0, || append_sink(&path)));
        assert!(
            !scroll.start_copy_output_to_file("nowhere", 0, || None),
            "the replacement failed"
        );
        assert!(
            !scroll.close_log_file(0),
            "the failed replacement left logging stopped"
        );
        scroll.add_text_to_scroll("after the failure", 0, true, 0);
        assert_eq!(
            std::fs::read(&path).expect("read append output"),
            format!("Chat log {} closed.\r\n", path.display()).as_bytes(),
            "the first handle took the close line and nothing after it"
        );
        std::fs::remove_dir_all(&dir).expect("remove disposable output directory");
    }

    /// Oracle: the retail format literal `L"%#H:%M:%S "` and MSVC's `#` flag.
    ///
    /// The expected strings are spelled out rather than built from [`timestamp_prefix`], because a
    /// test that renders through the same function it checks cannot see a wrong format.
    #[test]
    fn the_stamp_strips_the_hours_leading_zero_and_keeps_the_others_and_the_trailing_space() {
        assert_eq!(TIMESTAMP_FORMAT, "%#H:%M:%S ");
        // 1970-01-01 09:05:03 UTC. `%#H` -> `9`, not `09`; `%M`/`%S` stay two digits.
        assert_eq!(timestamp_prefix(9 * 3600 + 5 * 60 + 3, 0), "9:05:03 ");
        // Midnight is `0:00:00 ` on the 24-hour clock, not `12:00:00 `.
        assert_eq!(timestamp_prefix(0, 0), "0:00:00 ");
        // And an afternoon keeps both digits.
        assert_eq!(timestamp_prefix(23 * 3600 + 59 * 60 + 59, 0), "23:59:59 ");
        // The trailing space is part of the format and is the entire gap before the body.
        assert!(timestamp_prefix(0, 0).ends_with(' '));
        // The offset seam is live: it is the one part of `localtime` not reproduced, so a test
        // that could not tell 0 from 3600 would be asserting nothing about it.
        assert_eq!(timestamp_prefix(0, 3_600), "1:00:00 ");
        assert_eq!(timestamp_prefix(0, -3_600), "23:00:00 ");
    }

    /// Oracle: the scroll's `0x1a` type check and its display-timestamps option check.
    /// Both gates, both ways — a one-armed assertion here cannot tell a live gate from a dead one.
    #[test]
    fn both_halves_of_the_timestamp_gate() {
        assert!(wants_timestamp(3, true));
        assert!(!wants_timestamp(3, false), "the option is off by default");
        assert!(
            !wants_timestamp(LOCAL_ERROR_TYPE, true),
            "0x1A is special-cased ahead of the gate"
        );
        assert!(!wants_timestamp(LOCAL_ERROR_TYPE, false));
    }

    /// Oracle: trim the configured whitespace from both ends.
    #[test]
    fn the_text_is_trimmed_at_both_ends() {
        let mut s = Scroll::default();
        s.add_text_to_scroll("  padded  ", 0, true, 0);
        assert_eq!(s.drain()[0].body, "padded");
    }

    /// Oracle: `recv_notice`'s table. Nineteen of the twenty variants are somebody else's.
    #[test]
    fn only_display_string_becomes_a_scroll_line() {
        let mut s = Scroll::default();
        assert!(recv_notice(
            &mut s,
            &crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                channel: 0x1A,
                text: "x".into()
            }
        ));
        assert!(!recv_notice(&mut s, &crate::Notice::EndPendingInPlayer));
        assert!(!recv_notice(
            &mut s,
            &crate::Notice::ObjectDeleted(dereth_primitives::ObjectId(1))
        ));
        assert_eq!(s.added, 1);
    }
}
