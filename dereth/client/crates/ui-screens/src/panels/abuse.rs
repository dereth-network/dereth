//! `AbusePanel`'s report pages and reply text.
//!
//! The three abuse failures are deliberately silent in the failure-event handler. At the common arm
//! retail sends the abuse-report response notice; the global
//! notice reaches this panel, whose only effect is
//! to set page three's text on the result-text child. It neither opens nor closes this window.

use dereth_ui::{ElemHandle, ElementId, ElementType, StateId, UiSystem};

use crate::view::{GameView, UiRequest};

/// `AbusePanel`'s registered element type. Its instance has no native id constant, so post-init
/// finds it by class just as the friends, fellowship and allegiance panels do.
pub const PANEL_TYPE: ElementType = ElementType(0x1000_0018);
/// The panel's result-text child, bound at post-init.
pub const RESULT_TEXT: ElementId = ElementId(0x1000_010B);
/// Page-one Reset/Back button and page-three Done button. Both call the panel's reset.
pub const RESET_BUTTON: ElementId = ElementId(0x1000_0101);
pub const DONE_BUTTON: ElementId = ElementId(0x1000_010C);
/// Page one's Continue button. It enters page two, then copies a selected player's name.
pub const SELECTED_NAME_BUTTON: ElementId = ElementId(0x1000_0102);
pub const NAME_ENTRY: ElementId = ElementId(0x1000_0105);
pub const COMPLAINT_ENTRY: ElementId = ElementId(0x1000_0107);
pub const CONTINUE_BUTTON: ElementId = ElementId(0x1000_0109);
pub const PAGE_ONE: StateId = StateId(0x1000_0008);
pub const PAGE_TWO: StateId = StateId(0x1000_0009);
pub const PAGE_THREE: StateId = StateId(0x1000_000A);
/// The string-table enum the page-three text is written with.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0001;
const WAIT_TOKEN: &str = "ID_Abuse_PageThree_WaitText";
const EMPTY_REPORT: &str = "Please specify a character and complaint.";
const NOTICE_CHANNEL: u32 = 0x1A;

/// The three error codes handled by the abuse-report-response notice.
pub mod response {
    pub const NO_SUCH_CHARACTER: u32 = 0x04B8;
    pub const SELF_REPORT: u32 = 0x04B9;
    pub const SUCCESS: u32 = 0x04BA;
}

/// The hashed string-id names read by the abuse-report-response notice's three arms.
#[must_use]
pub const fn token(code: u32) -> Option<&'static str> {
    match code {
        response::NO_SUCH_CHARACTER => Some("ID_Abuse_Response_NoSuchCharacter"),
        response::SELF_REPORT => Some("ID_Abuse_Response_Self"),
        response::SUCCESS => Some("ID_Abuse_Response_Success"),
        _ => None,
    }
}

/// The bound state for `AbusePanel`'s report producer and response consumer.
#[derive(Debug, Default)]
pub struct AbusePanel {
    pub panel: Option<ElemHandle>,
    name_entry: Option<ElemHandle>,
    complaint_entry: Option<ElemHandle>,
    result_text: Option<ElemHandle>,
    continue_button: Option<ElemHandle>,
    pending: Vec<u32>,
    /// Number of native response arms applied to the bound text element.
    pub responses_applied: u32,
    /// Number of `0x0140` requests raised by the authored Continue button.
    pub reports_requested: u32,
}

impl AbusePanel {
    /// The four child bindings the panel takes at post-init.
    ///
    /// Rebinding means a new native panel instance, so queued work belonging to the destroyed
    /// instance is discarded instead of leaking across character sessions.
    pub fn post_init(&mut self, ui: &UiSystem, root: ElemHandle) {
        self.pending.clear();
        self.responses_applied = 0;
        self.reports_requested = 0;
        self.panel = find_panel(ui, root);
        self.name_entry = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, NAME_ENTRY));
        self.complaint_entry = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, COMPLAINT_ENTRY));
        self.result_text = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, RESULT_TEXT));
        self.continue_button = self
            .panel
            .and_then(|p| ui.get_child_recursive(p, CONTINUE_BUTTON));
    }

    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some() && self.result_text.is_some()
    }

    /// The panel's element-message handler.
    ///
    /// The generic element has already inserted/deleted text or changed the button before this
    /// panel sees the broadcast. This method owns only the page's decisions and requests.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::CHARACTER || m.id == msg::TEXT_CHANGED {
            if Some(m.source) != self.name_entry && Some(m.source) != self.complaint_entry {
                return false;
            }
            self.update_continue(ui);
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            RESET_BUTTON | DONE_BUTTON => {
                self.reset(ui);
                true
            }
            SELECTED_NAME_BUTTON => {
                if let Some(panel) = self.panel {
                    ui.set_state(panel, PAGE_TWO);
                }
                self.handle_selection(ui, view);
                true
            }
            CONTINUE_BUTTON => {
                self.report_abuse(ui);
                true
            }
            _ => false,
        }
    }

    /// Selection handling gates on the selected object answering "is a player".
    /// Once that strict gate passes, `NAME_APPROPRIATE` and `NAME_SINGULAR` are identical for a
    /// player (there is no stack/plural branch), so the existing [`GameView::name`] carrier is the
    /// native `alias = false` answer without introducing another name seam.
    fn handle_selection(&self, ui: &mut UiSystem, view: &dyn GameView) {
        let Some(selected) = view.selected_object() else {
            return;
        };
        if Some(selected) == view.player()
            || !view
                .selection_query_facts(selected)
                .is_some_and(|f| f.is_player)
        {
            return;
        }
        let Some(name) = view.name(selected) else {
            return;
        };
        if let Some(text) = self.name_entry.and_then(|h| ui.text_element_mut(h)) {
            text.set_text(name);
        }
    }

    /// Reset: page one, both fields empty, Continue disabled.
    fn reset(&self, ui: &mut UiSystem) {
        if let Some(panel) = self.panel {
            ui.set_state(panel, PAGE_ONE);
        }
        for h in [self.name_entry, self.complaint_entry]
            .into_iter()
            .flatten()
        {
            if let Some(text) = ui.text_element_mut(h) {
                text.set_text("");
            }
        }
        set_enabled(ui, self.continue_button, false);
    }

    /// Text entry: both native PStrings count only the terminator when empty.
    fn update_continue(&self, ui: &mut UiSystem) {
        let on = self
            .name_entry
            .is_some_and(|h| !entry_text(ui, h).is_empty())
            && self
                .complaint_entry
                .is_some_and(|h| !entry_text(ui, h).is_empty());
        set_enabled(ui, self.continue_button, on);
    }

    /// Send the abuse report built from the name field and the complaint field.
    fn report_abuse(&mut self, ui: &mut UiSystem) {
        let name = self
            .name_entry
            .map_or_else(String::new, |h| entry_text(ui, h));
        let complaint = self
            .complaint_entry
            .map_or_else(String::new, |h| entry_text(ui, h));
        if name.is_empty() || complaint.is_empty() {
            ui.requests.emit(UiRequest::DisplayChatText {
                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                channel: NOTICE_CHANNEL,
                text: EMPTY_REPORT.to_owned(),
            });
            return;
        }
        ui.requests.emit(UiRequest::AbuseLog {
            target: name,
            complaint,
        });
        self.reports_requested += 1;
        if let Some(panel) = self.panel {
            ui.set_state(panel, PAGE_THREE);
        }
        self.set_page_three_text(ui, WAIT_TOKEN);
    }

    /// The abuse-report response notice: enqueue the native notice for this panel.
    /// Unknown error codes are the receiver's no-op default arm. An absent panel has no registered
    /// native notice handler and therefore does not retain the response for a later instance.
    pub fn recv_response(&mut self, code: u32) -> bool {
        if !self.bound() || token(code).is_none() {
            return false;
        }
        self.pending.push(code);
        true
    }

    /// Deliver queued notices in producer order. Visibility is deliberately not a guard:
    /// the abuse-report-response notice writes the result text even while its ancestors are hidden.
    pub fn update(&mut self, ui: &mut UiSystem) -> u32 {
        if self.pending.is_empty() {
            return 0;
        }
        let Some(result) = self.result_text else {
            return 0;
        };
        let Some(table) = ui
            .env()
            .cloned()
            .and_then(|e| e.did_by_enum(4, STRING_TABLE_ENUM))
        else {
            return 0;
        };
        let mut applied = 0;
        for code in self.pending.drain(..) {
            let Some(name) = token(code) else { continue };
            let text = ui
                .resolve_string(
                    table,
                    dereth_primitives::num::hash::str_hash(name.as_bytes()),
                )
                .unwrap_or_default();
            if let Some(element) = ui.text_element_mut(result) {
                element.set_text(&text);
                applied += 1;
            }
        }
        self.responses_applied += applied;
        applied
    }

    fn set_page_three_text(&self, ui: &mut UiSystem, token: &str) {
        let Some(result) = self.result_text else {
            return;
        };
        let Some(table) = ui
            .env()
            .cloned()
            .and_then(|e| e.did_by_enum(4, STRING_TABLE_ENUM))
        else {
            return;
        };
        let text = ui
            .resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(token.as_bytes()),
            )
            .unwrap_or_default();
        if let Some(element) = ui.text_element_mut(result) {
            element.set_text(&text);
        }
    }
}

fn entry_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn set_enabled(ui: &mut UiSystem, h: Option<ElemHandle>, on: bool) {
    if let Some(h) = h {
        ui.set_state(
            h,
            if on {
                dereth_ui::widgets::button::state::NORMAL
            } else {
                dereth_ui::widgets::button::state::DISABLED
            },
        );
    }
}

fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}
