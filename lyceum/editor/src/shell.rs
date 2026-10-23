//! The front end the runtime calls at its frame's steps: the window's events and the overlay.
//!
//! The editor has no game interface. Its front end hands the window's lifecycle events (close,
//! resize, focus) to the runtime as any front end does, keeps the device events (keys, the
//! pointer, the wheel) for the editor to read between frames, uploads the panels' two textures
//! before the frame's bracket, and draws the editor's overlay over the finished world.
//!
//! The world is drawn into the world view only, as a screen of its own (the editor sets that
//! part of the window before the runtime's frame); the overlay goes over the whole window, and a
//! resize reaches the whole window too.

use dereth_client_contract::overlay::{OverlayItem, OverlayTexture};
use dereth_client_runtime::present::PresentError;
use dereth_client_runtime::shell::{PlainHud, Shell};
use dereth_client_runtime::ui_context::UiContext;
use dereth_client_shell::platform::window::{lifecycle, WindowEvents};
use dereth_client_shell::present::ClientPresentation;
use dereth_input::host::HostEvent;
use dereth_primitives::TextureData;

/// The editor's front end.
#[derive(Debug, Default)]
pub struct EditorShell {
    /// The window's queue, which the window fills as it pumps.
    pub events: WindowEvents,
    /// The device events since the editor last read them.
    pub inbox: Vec<HostEvent>,
    /// This frame's overlay.
    pub items: Vec<OverlayItem>,
    /// Textures to upload before the next frame.
    pub uploads: Vec<(OverlayTexture, TextureData)>,
    /// What an upload or a draw answered when it failed.
    pub errors: Vec<String>,
    /// The display's extent, as the runtime last said.
    pub display: Option<(i32, i32)>,
}

impl Shell for EditorShell {
    type Hud = PlainHud;
    type Present = dyn ClientPresentation;

    fn window_input(&mut self, cx: &mut UiContext<'_, Self>, time_ms: u32) {
        let events: Vec<HostEvent> = std::mem::take(&mut *self.events.borrow_mut());
        for e in events {
            match lifecycle(&e) {
                Some(l) => {
                    // A new window size is the whole window's: the world view's part of it is
                    // laid out again at the next frame.
                    if matches!(
                        l,
                        dereth_client_runtime::platform::window::HostEvent::Resized { .. }
                    ) {
                        cx.present_mut().set_inset(None);
                    }
                    cx.window_event(self, &l, time_ms);
                }
                None => self.inbox.push(e),
            }
        }
    }

    fn compose_ui(&mut self, cx: &mut UiContext<'_, Self>) {
        for (texture, data) in std::mem::take(&mut self.uploads) {
            if let Err(e) = cx.present_mut().overlay_upload(texture, &data) {
                self.errors.push(format!("upload: {e}"));
            }
        }
    }

    fn draw_ui(&mut self, present: &mut Self::Present) -> Result<(), PresentError> {
        // The panels go over the whole window, whatever part of it the world was drawn in.
        let view = present.inset();
        if view.is_some() {
            present.set_inset(None);
        }
        let drawn = present.draw_overlay(&self.items);
        if view.is_some() {
            present.set_inset(view);
        }
        drawn
    }

    fn set_display(&mut self, display: (i32, i32)) {
        self.display = Some(display);
    }
}
