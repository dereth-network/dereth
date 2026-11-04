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
//! - **A layout file**: the player's commands write the panels' places to one of the player's files
//!   and read them back.

use auberean_client_sdk::script::ui_locations::UiLocations;
use auberean_client_sdk::Client;
use auberean_ui::framework::{FrameId, Locations};
use auberean_ui::location::Location;
use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::layout_file;

/// What the play screen asked of the kept places, waiting for the client.
#[derive(Debug, Default)]
pub struct Places {
    /// A table to keep and send.
    keep: Option<Vec<u8>>,
    /// Put the panels back: from the options that came again (`true`), or as the interface
    /// answers (`false`).
    put_back: Option<bool>,
    /// A layout file to write, by the name the player gave.
    save_layout: Option<String>,
    /// A layout file to read back, by the name the player gave.
    load_layout: Option<String>,
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

    /// The panels' places to be written to a layout file at the next frame.
    pub fn save_layout(&mut self, name: String) {
        self.save_layout = Some(name);
    }

    /// The panels' places to be read back from a layout file at the next frame.
    pub fn load_layout(&mut self, name: String) {
        self.load_layout = Some(name);
    }

    /// A layout file's name, for this character, world and screen when the player gave none; with
    /// no name and no player, none.
    fn layout_name(client: &mut Client, ui: &Ui, name: &str) -> Option<String> {
        let character = client.player_name();
        if name.is_empty() && character.is_none() {
            return None;
        }
        let world = client.world_name();
        let (width, height) = ui.display();
        Some(layout_file::file_name(
            name,
            &character.unwrap_or_default(),
            &world,
            width,
            height,
        ))
    }

    /// What was asked, through the client: the table kept with the player's options and sent;
    /// the panels of the play screen (when it is up) put back where the options keep them; a
    /// layout file written or read. Whether a layout file moved the panels.
    pub fn prepare(&mut self, client: &mut Client, ui: &mut Ui, frame: Option<FrameId>) -> bool {
        let mut laid_out = false;
        if let Some(file) = self
            .save_layout
            .take()
            .and_then(|name| Self::layout_name(client, ui, &name))
        {
            client.write_player_file(&file, layout_file::write(ui).as_bytes());
        }
        if let Some(file) = self
            .load_layout
            .take()
            .and_then(|name| Self::layout_name(client, ui, &name))
        {
            if let Some(bytes) = client.read_player_file(&file) {
                layout_file::read(ui, &String::from_utf8_lossy(&bytes));
                laid_out = true;
            }
        }
        if let Some(t) = self.keep.take() {
            client.set_ui_save_locations(t);
            client.upload_ui_settings();
        }
        let Some(reloaded) = self.put_back.take() else {
            return laid_out;
        };
        let Some(f) = frame else { return laid_out };
        let held = if reloaded {
            client.options().map(|o| o.ui_locations)
        } else {
            client.ui_save_locations()
        };
        if let Some(t) = held.as_deref().and_then(UiLocations::read) {
            ui.frame_apply_locations(f, &kept(&t));
        }
        laid_out
    }
}
