//! The panels' places kept with the player's options: what the play screen asks, carried out
//! through the player's scripts at the next frame.
//!
//! - **Kept**: a panel moved, the play screen's panels whose place is kept are read in the order
//!   it made them, each added to a new table of saved places under its layout and its name; the
//!   table goes to the player's options through the interface, and the windows' places and
//!   opacities are sent to the server to keep.
//! - **Put back**: when the player's options come again (each entry into the world brings them),
//!   the table is read from them; when the display is set up again, it is asked of the player's
//!   interface. Either way the panels are put where it keeps them.

use auberean_client_sdk::script::ui_locations::UiLocations;
use auberean_client_sdk::Client;
use auberean_ui::framework::{FrameId, Locations};
use auberean_ui::location::Location;
use auberean_ui::{Token, Ui};

/// What the play screen asked of the kept places, waiting for the client.
#[derive(Debug, Default)]
pub struct Places {
    /// A table to keep and send.
    keep: Option<Vec<u8>>,
    /// Put the panels back: from the options that came again (`true`), or as the interface
    /// answers (`false`).
    put_back: Option<bool>,
}

/// The play screen's kept places, in the order its panels were made, as a table of saved places.
#[must_use]
pub fn table(kept: &[((u32, Token), Location)]) -> UiLocations {
    let mut t = UiLocations::new();
    for ((layout, element), l) in kept {
        let place = [
            l.x0.to_bits(),
            l.y0.to_bits(),
            l.x1.to_bits(),
            l.y1.to_bits(),
            u32::from(l.visible),
        ];
        t.add_location(*layout, element.raw(), place);
    }
    t
}

/// A table of saved places as the framework puts places back.
#[must_use]
pub fn kept(t: &UiLocations) -> Locations {
    t.iter()
        .map(|(window, element, d)| {
            let l = Location {
                visible: d[4] != 0,
                x0: f32::from_bits(d[0]),
                y0: f32::from_bits(d[1]),
                x1: f32::from_bits(d[2]),
                y1: f32::from_bits(d[3]),
            };
            ((window, Token(element)), l)
        })
        .collect()
}

impl Places {
    /// The play screen's panels moved: their places read now, kept at the next frame.
    pub fn save(&mut self, ui: &Ui, frame: FrameId) {
        self.keep = Some(table(&ui.frame_kept_places(frame)).write());
    }

    /// The panels to be put back at the next frame.
    pub fn put_back(&mut self, reloaded: bool) {
        self.put_back = Some(reloaded);
    }

    /// What was asked, through the client: the table kept with the player's options and sent;
    /// the panels of the play screen (when it is up) put back where the options keep them.
    pub fn prepare(&mut self, client: &mut Client, ui: &mut Ui, frame: Option<FrameId>) {
        if let Some(t) = self.keep.take() {
            client.set_ui_save_locations(t);
            client.upload_ui_settings();
        }
        let Some(reloaded) = self.put_back.take() else {
            return;
        };
        let Some(f) = frame else { return };
        let held = if reloaded {
            client.options().map(|o| o.ui_locations)
        } else {
            client.ui_save_locations()
        };
        if let Some(t) = held.as_deref().and_then(UiLocations::read) {
            ui.frame_apply_locations(f, &kept(&t));
        }
    }
}
