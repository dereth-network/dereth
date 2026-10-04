//! The epilogue screen — mode `0x10000009`, the client's only normal exit path.
//!
//! The epilogue screen's construction sequence:
//!
//! ```text
//! epilogue screen construction:
//!     listen for root element message 0x10000002
//!     create and store root element 0x10000399 from layout 0x10000037
//!     listen for global message 1                 // any key press
//!     if the player session exists and the world connection is live:
//!         request a clean character logout
//! end
//! ```
//!
//! The shipped `epilogue` layout (`0x21000036`, 110 bytes, consumed exactly) is **one element**:
//! the 800×600 root `0x10000399` whose element-level media list is a single message entry
//! carrying `0x10000002` at probability 1.0. The initial media reset therefore
//! broadcasts the quit message on the screen's very first tick and the client exits without ever
//! drawing anything — "it plays whatever the layout says", and what the layout says is *nothing*.

use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{ElemHandle, ElementId, ElementMessage, ListenerId, MessageId, UiError};

use crate::view::UiRequest;

/// The screen's root: layout enum `0x10000037`, element `0x10000399`.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0037);
const ROOT: ElementId = ElementId(0x1000_0399);
/// The framework's own listener identity.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// Message `0x10000002` on element `0x10000399` — registered **before** the root is created,
/// which is the retail order and the reason the layout's own message media entry is never
/// missed: the media machine fires it while the root element is being created.
pub const MSG_ANIMATION_DONE: MessageId = MessageId(0x1000_0002);

/// The epilogue screen — mode `0x10000009`.
#[derive(Debug, Default)]
pub struct EpilogueScreen {
    roots: Vec<ElemHandle>,
    /// True once the main loop's done has been requested.
    pub done: bool,
    /// True once a clean character logoff has been asked for.
    pub log_off_requested: bool,
}

impl EpilogueScreen {
    /// The factory registered for this screen's mode.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    fn device_done(&mut self, requests_out: &mut crate::requests::Outbox) {
        if !self.done {
            self.done = true;
            requests_out.emit(UiRequest::DeviceDone);
        }
    }
}

impl Screen for EpilogueScreen {
    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        // The registration comes first, then the root — which matters, because the root's own media
        // list broadcasts `0x10000002` while the root is being created.
        ui.register_for_element_message(ROOT, MSG_ANIMATION_DONE, ME);
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        // The framework's own registration. The client
        // has both, and delivers to both: the element broadcast's id-table pass does
        // not stamp the serial number, so the bubble pass delivers a second time. Requesting done
        // is idempotent, which is why that is harmless there and here.
        ui.register_for_element_messages(root, ME);
        ui.register_for_global_message(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, ME);
        // "If the player session exists and the net is still up, request a clean logout."
        // The host owns the session, so the screen records the call and the host makes it. This is
        // the *clean logout*: without it the account stays logged in on the server
        // until its own timeout, which is exactly what the live-run evidence warns about.
        self.log_off_requested = true;
        ui.requests
            .emit(UiRequest::EndCharacterSession { ask: false });
        Ok(())
    }

    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        let ui = &mut *cx.ui;
        ui.unregister_for_element_message(ROOT, MSG_ANIMATION_DONE, ME);
        ui.unregister_for_global_message(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, ME);
        for r in std::mem::take(&mut self.roots) {
            ui.unregister_from_element(r, ME);
        }
    }

    fn on_global_message(&mut self, cx: &mut ScreenCx<'_>, id: MessageId, _param: u32) {
        if id == dereth_ui::msg::global::KEY_DOWN_UNCONSUMED {
            self.device_done(&mut cx.ui.requests);
        }
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        if m.id == MSG_ANIMATION_DONE {
            self.device_done(&mut cx.ui.requests);
        }
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the screen catalogue, and the layout-enum map (enum `0x10000037` →
    /// `0x21000036` `epilogue`, whose one and only element is the root `0x10000399`).
    #[test]
    fn both_documented_paths_end_the_main_loop_and_neither_does_it_twice() {
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0037));
        assert_eq!(ROOT, ElementId(0x1000_0399));

        let mut s = EpilogueScreen::default();
        s.on_global_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            dereth_ui::msg::global::KEY_DOWN_UNCONSUMED,
            0,
        );
        assert!(s.done);
        assert_eq!(ui.requests.take(), vec![UiRequest::DeviceDone]);

        // A second trigger does not queue a second shutdown.
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &ElementMessage {
                source_id: ROOT,
                source: ElemHandle::for_test(1),
                id: MSG_ANIMATION_DONE,
                p1: 0,
                p2: 0,
                point: dereth_ui::msg::MessagePoint::default(),
                serial: 1,
            },
        );
        assert!(ui.requests.take().is_empty());

        // …and the element-message path works on its own.
        let mut s = EpilogueScreen::default();
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &ElementMessage {
                source_id: ROOT,
                source: ElemHandle::for_test(1),
                id: MSG_ANIMATION_DONE,
                p1: 0,
                p2: 0,
                point: dereth_ui::msg::MessagePoint::default(),
                serial: 2,
            },
        );
        assert_eq!(ui.requests.take(), vec![UiRequest::DeviceDone]);
    }
}
