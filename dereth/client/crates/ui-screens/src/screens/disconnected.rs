//! The disconnected screen — mode `0x10000002`, "you have been disconnected".
//!
//! The shipped `disconnected` layout (`0x21000002`, 435 bytes, consumed exactly) is four elements:
//! the 800×600 root `0x10000416` with a full-screen background image `0x06005D3F`, a 290×205 panel
//! `0x10000373` at (430, 115) drawn from `0x06004CC0`, and inside it the 260×126 text element
//! `0x10000417` at (15, 15), plus the 100×32 OK button `0x10000418` at (95, 158).

use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{ElemHandle, ElementId, ElementMessage, ListenerId, MessageId, UiError, UiSystem};

use crate::bind::{bind_children, child, Bound, ChildBinding};

/// The screen's root: layout enum `0x10000003`, element `0x10000416`.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0003);
const ROOT: ElementId = ElementId(0x1000_0416);
/// The framework's own listener identity.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// Descendant `0x10000417`, required to be a text element, is the error-text target.
pub const CHILDREN: &[ChildBinding] = &[child("error_text", 0x1000_0417)];

/// Element `0x10000418`, message 1 → queue mode `0x10000009` (epilogue → quit). The destructor
/// unregisters message 1 on `0x10000418`.
pub const OK_BUTTON: ElementId = ElementId(0x1000_0418);

/// The disconnect text: the string-table ids and the two literal messages, the contract's
/// (`dereth_client_contract::disconnect`) because the runtime resolves them for any UI.
pub use dereth_client_contract::disconnect::*;

/// The disconnected screen — mode `0x10000002`.
#[derive(Debug, Default)]
pub struct DisconnectedScreen {
    roots: Vec<ElemHandle>,
    bound: Bound,
    /// The pending message that mode adoption writes into the error-text element — the reason
    /// the server-died or character-error handler stashed while
    /// queuing the UI mode with an error. This is the `StringInfo`'s **symbolic id**, not display text; the
    /// host resolves it against table enum [`STRING_TABLE_ENUM`], exactly as it resolves the
    /// character screen's delete-confirmation phrase.
    pub error_text: Option<String>,
    /// The display text last written into the error-text element.
    pub shown_text: Option<String>,
    /// Set when [`Self::show_error`] has already written the current [`Self::error_text`].
    applied: bool,
}

impl DisconnectedScreen {
    /// The factory registered for this screen's mode.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    #[must_use]
    pub fn bound(&self) -> &Bound {
        &self.bound
    }

    /// The error-text element, once `create` has bound it.
    #[must_use]
    pub fn error_element(&self) -> Option<ElemHandle> {
        self.bound.get("error_text")
    }

    /// Writing the resolved `StringInfo` to the error-text element.
    ///
    /// `text` is the **resolved** string; the host owns the string tables (the client reaches its
    /// dat cache through a singleton and this crate has no such service), so it
    /// resolves [`Self::error_text`] and hands the result back. Passing the symbolic id straight
    /// through is what a host with no string table does, and it is legible rather than blank.
    pub fn show_error(&mut self, ui: &mut UiSystem, text: &str) {
        self.applied = true;
        self.shown_text = Some(text.to_string());
        if let Some(h) = self.error_element() {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text);
            }
        }
    }

    /// Whether the pending error message still needs [`Self::show_error`].
    #[must_use]
    pub fn needs_error_text(&self) -> bool {
        self.error_text.is_some() && !self.applied
    }
}

impl Screen for DisconnectedScreen {
    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        // The client registers the framework as a
        // listener on the root; without it `on_element_message` is never called and the OK
        // button does nothing.
        ui.register_for_element_messages(root, ME);
        self.bound = bind_children(ui, root, CHILDREN);
        // The client's constructor also takes the by-id registration. Both routes are live in the
        // original: the element broadcast's step 3 walks the id table without touching the
        // serial number and step 4 then bubbles — so a listener registered twice is called twice.
        // Reproduced rather than deduplicated; queueing the mode is idempotent, which is why it is
        // harmless there and here.
        ui.register_for_element_message(OK_BUTTON, MessageId(1), ME);
        Ok(())
    }

    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        let ui = &mut *cx.ui;
        ui.unregister_for_element_message(OK_BUTTON, MessageId(1), ME);
        for r in std::mem::take(&mut self.roots) {
            ui.unregister_from_element(r, ME);
        }
    }

    /// Record the error message while adopting the new mode,
    /// before the screen is shown.
    fn set_error_msg(&mut self, s: String) {
        self.error_text = Some(s);
        self.applied = false;
    }

    /// The original error setter receives the string the queued mode stashed. The screen holds
    /// its **symbolic id**; the host hands over the string table (enum `0x10000002`) it resolves
    /// in, because client-side string resolution reaches the asset cache through a singleton.
    fn on_pregame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        p: &dereth_ui::framework::PregameCx<'_>,
    ) -> Option<dereth_ui::UiMode> {
        if self.needs_error_text() {
            let id = self.error_text.clone().unwrap_or_default();
            let resolved = p.ui_strings.and_then(|t| {
                cx.ui
                    .resolve_string(t, dereth_primitives::num::hash::str_hash(id.as_bytes()))
            });
            let text = resolved.unwrap_or(id);
            self.show_error(cx.ui, &text);
        }
        None
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let requests_out = &mut cx.ui.requests;
        if m.source_id == OK_BUTTON && m.id == MessageId(1) {
            requests_out.emit(crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::EPILOGUE,
            ));
        }
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered screen catalogue, and the shipped `disconnected` layout `0x21000002`,
    /// whose four elements are the root, the panel and these two.
    #[test]
    fn the_screen_names_the_documented_layout_root_child_and_button() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0003));
        assert_eq!(ROOT, ElementId(0x1000_0416));
        assert_eq!(CHILDREN[0].id, ElementId(0x1000_0417));
        assert_eq!(OK_BUTTON, ElementId(0x1000_0418));
    }

    /// Oracle: §5 — the OK button is the screen's one exit path, and it goes to the epilogue rather
    /// than exiting directly (§10: "the quit path goes GamePlay → Epilogue → Done, never
    /// straight to `exit`").
    #[test]
    fn the_ok_button_takes_the_screens_one_exit_path() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DisconnectedScreen::default();
        s.set_error_msg("connection lost".into());
        assert_eq!(s.error_text.as_deref(), Some("connection lost"));
        assert!(s.needs_error_text());
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &ElementMessage {
                source_id: OK_BUTTON,
                source: ElemHandle::for_test(1),
                id: MessageId(1),
                p1: 0,
                p2: 0,
                point: dereth_ui::msg::MessagePoint::default(),
                serial: 1,
            },
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::EPILOGUE
            )]
        );
    }

    /// Oracle: the recovered disconnect and shutdown behavior's character-error table — 25 codes,
    /// 21 with a token, four (0, 2, 7, 22) that "never queue a mode", and 4/8 sharing one token.
    #[test]
    fn the_character_error_table_is_the_documented_one() {
        assert_eq!(character_error_string_id(0), None);
        assert_eq!(character_error_string_id(2), None);
        assert_eq!(character_error_string_id(7), None);
        assert_eq!(character_error_string_id(22), None);
        assert_eq!(character_error_string_id(4), character_error_string_id(8));
        assert_eq!(character_error_string_id(1), Some("ID_CHAR_ERROR_LOGON"));
        assert_eq!(
            character_error_string_id(24),
            Some("ID_CHAR_ERROR_SUBSCRIPTION_EXPIRED")
        );
        let named = (0..=24)
            .filter(|c| character_error_string_id(*c).is_some())
            .count();
        assert_eq!(named, 21, "21 of the 25 codes carry a token");
        // Nothing above the enum is invented.
        assert_eq!(character_error_string_id(25), None);
    }

    /// Oracle: "writes the message into the error-text element". With no element
    /// bound (no dat) it must still record what it would have shown, so the host can report it.
    #[test]
    fn the_error_message_is_recorded_even_with_no_text_element() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DisconnectedScreen::default();
        s.set_error_msg(SERVER_DIED_STRING_ID.into());
        s.show_error(&mut ui, "Your connection to the server has been lost.");
        assert!(!s.needs_error_text());
        assert_eq!(
            s.shown_text.as_deref(),
            Some("Your connection to the server has been lost.")
        );
    }

    /// The four literals are the ones in rdata.
    #[test]
    fn the_four_literals_are_the_ones_in_rdata() {
        assert_eq!(BOOTED_FORMAT, "You have been booted from Asheron's Call%s.");
        assert_eq!(BOOTED_DEFAULT_REASON, " for Code of Conduct Violations");
        assert_eq!(BANNED_FORMAT, "You have been banned from Asheron's Call%s.");
        assert_eq!(
            BANNED_UNTIL_FORMAT,
            "You have been banned until %s%s. For ban appeals, please visit support.turbine.com"
        );
        // One `%s` in the booted and permanent-ban formats, two in the timed one — the argument
        // counts the handlers actually pass.
        assert_eq!(BOOTED_FORMAT.matches("%s").count(), 1);
        assert_eq!(BANNED_FORMAT.matches("%s").count(), 1);
        assert_eq!(BANNED_UNTIL_FORMAT.matches("%s").count(), 2);
    }

    /// Oracle: a failed reason unpack **or** an unpacked length of 1
    /// substitutes `" for Code of Conduct Violations"`.
    ///
    /// The reason the server sends is the one shown, and the substitution happens only when there
    /// is nothing to show: a boot that showed the Code-of-Conduct default over a server that said
    /// something else would be the client putting words in the server's mouth.
    #[test]
    fn a_boot_shows_the_servers_reason_and_the_default_only_when_there_is_none() {
        // The recorded one — the only `0xF7DC` in the corpus.
        assert_eq!(
            account_booted_message(Some(
                " because the password entered for this account was not correct"
            )),
            "You have been booted from Asheron's Call because the password entered for this \
             account was not correct."
        );
        // No body at all, and an empty string: the account-booted handler cannot tell them apart.
        let default = "You have been booted from Asheron's Call for Code of Conduct Violations.";
        assert_eq!(account_booted_message(None), default);
        assert_eq!(account_booted_message(Some("")), default);
    }

    /// Oracle: the client's `expiry < 1` arm, which uses
    /// [`BANNED_FORMAT`] and **never** substitutes a default reason.
    ///
    /// Both directions: a permanent ban is not a timed one, and the boot sentence is not the ban
    /// sentence. `0` and every negative expiry are permanent.
    #[test]
    fn a_permanent_ban_names_no_date_and_substitutes_no_default() {
        for expiry in [0_i32, -1, i32::MIN] {
            assert_eq!(
                account_banned_message(expiry, " - griefing", 1_756_000_000, 0),
                "You have been banned from Asheron's Call - griefing.",
                "expiry {expiry} is permanent"
            );
        }
        // No reason: the sentence simply ends. Unlike a boot, nothing is filled in.
        assert_eq!(
            account_banned_message(0, "", 1_756_000_000, 0),
            "You have been banned from Asheron's Call."
        );
        assert!(!account_banned_message(0, "", 1_756_000_000, 0).contains("Code of Conduct"));
        assert!(!account_banned_message(0, "", 1_756_000_000, 0).contains("banned until"));
        assert!(!account_banned_message(0, "", 1_756_000_000, 0).contains("booted"));
    }

    /// Oracle: the account-banned handler's timed arm — round `time(NULL)` **up** to the next whole
    /// minute, add the expiry, `asctime(localtime(...))`, strip the trailing newline.
    #[test]
    fn a_timed_ban_renders_the_expiry_the_way_the_handler_does() {
        // 1_756_000_000 = Sun 2025-08-24 01:46:40 UTC, which is 40 s past a minute. Rounding **up**
        // gives 01:47:00, and a one-hour ban therefore expires at 02:47:00 the same day.
        let msg = account_banned_message(3_600, " - griefing", 1_756_000_000, 0);
        assert_eq!(
            msg,
            "You have been banned until Sun Aug 24 02:47:00 2025 - griefing. \
             For ban appeals, please visit support.turbine.com"
        );
        // The rounding is up and to a **minute**: 40 s past the minute moves forward 20 s, and an
        // instant already on a boundary does not move at all.
        assert_eq!(ban_expiry_epoch(0, 1_756_000_000), 1_756_000_020);
        assert_eq!(ban_expiry_epoch(0, 1_756_000_020), 1_756_000_020);
        assert_eq!(ban_expiry_epoch(60, 1_756_000_000), 1_756_000_080);

        // The offset seam is live: it is the only part of the handler not reproduced, so a test
        // that could not see it would leave a dead parameter behind.
        let shifted = account_banned_message(3_600, " - griefing", 1_756_000_000, 3_600);
        assert!(shifted.contains("Sun Aug 24 03:47:00 2025"), "{shifted}");
        assert_ne!(shifted, msg);
    }

    /// Asctime matches the crts own format.
    #[test]
    fn asctime_matches_the_crts_own_format() {
        assert_eq!(asctime(0, 0), "Thu Jan 01 00:00:00 1970");
        assert_eq!(asctime(-1, 0), "Wed Dec 31 23:59:59 1969");
        assert_eq!(
            asctime(951_782_400, 0),
            "Tue Feb 29 00:00:00 2000",
            "2000 is a leap year"
        );
        assert_eq!(asctime(1_709_164_800, 0), "Thu Feb 29 00:00:00 2024");
        assert_eq!(asctime(1_756_000_000, 0), "Sun Aug 24 01:46:40 2025");
        assert!(
            !asctime(0, 0).ends_with('\n'),
            "the handler strips the newline"
        );
        // Two digits, zero padded — `Jan 01`, and never the glibc `Jan  1`.
        assert!(asctime(0, 0).contains("Jan 01"));
        assert!(asctime(1_756_000_000, 0).contains("Aug 24"));
    }
}
