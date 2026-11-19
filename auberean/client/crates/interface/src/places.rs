//! The panels' places kept with the player's options, and the layout files.
//!
//! - **Kept**: a panel moved, the play screen's panels whose place is kept are read in the order
//!   it made them, and the client is asked to keep them with the player's options and send them,
//!   with the windows' opacities, to the server to keep ([`locations`]).
//! - **Put back**: when the player's options come again (each entry into the world brings them),
//!   the table is read from them; when the display is set up again, it is asked of the player's
//!   interface (before the next frame's view: [`Places::wants_saved`]). Either way the panels are
//!   put where it keeps them, at the next frame.
//! - **A layout file**: the player's commands write the panels' places to one of the player's files
//!   and read them back, through the shell's files: the file's name is made at the next frame (it
//!   names the character and the world), and a file read is laid out the frame its answer comes.

use auberean_client_sdk::contract::view::View;
use auberean_client_sdk::contract::UiLocation;
use auberean_shell::interface::Request;
use auberean_ui::framework::{FrameId, Locations};
use auberean_ui::location::Location;
use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::layout_file;

/// What the play screen asked of the kept places, waiting for the client.
#[derive(Debug, Default)]
pub struct Places {
    /// Put the panels back: from the options that came again (`true`), or as the interface
    /// answers (`false`).
    put_back: Option<bool>,
    /// A layout file to write, by the name the player gave.
    save_layout: Option<String>,
    /// A layout file to read back, by the name the player gave.
    load_layout: Option<String>,
    /// The layout files asked to be read, by their files' names, until their answers come.
    reading: Vec<String>,
}

/// The play screen's kept places, in the order its panels were made, as the client keeps them.
#[must_use]
pub fn locations(kept: &[((u32, Token), Location)]) -> Vec<UiLocation> {
    kept.iter()
        .map(|((layout, element), l)| UiLocation {
            layout: *layout,
            element: element.raw(),
            corners: [l.x0, l.y0, l.x1, l.y1],
            visible: l.visible,
        })
        .collect()
}

/// A table of saved places as the framework puts places back.
#[must_use]
pub fn kept(t: &[UiLocation]) -> Locations {
    t.iter()
        .map(|p| {
            let [x0, y0, x1, y1] = p.corners;
            let l = Location {
                visible: p.visible,
                x0,
                y0,
                x1,
                y1,
            };
            ((p.layout, Token(p.element)), l)
        })
        .collect()
}

impl Places {
    /// The play screen's panels moved: their places read now, for the client to keep.
    #[must_use]
    pub fn save(ui: &Ui, frame: FrameId) -> Request {
        Request::Act(auberean_client_sdk::Action::KeepUiLocations {
            places: locations(&ui.frame_kept_places(frame)),
        })
    }

    /// The panels to be put back at the next frame.
    pub fn put_back(&mut self, reloaded: bool) {
        self.put_back = Some(reloaded);
    }

    /// The panels are to be put back where the player's interface keeps them: the next frame's
    /// view is to answer for those places.
    #[must_use]
    pub const fn wants_saved(&self) -> bool {
        matches!(self.put_back, Some(false))
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
    fn layout_name(view: &View, ui: &Ui, name: &str) -> Option<String> {
        let player = view.player.as_ref();
        let character = player.and_then(|p| p.name.clone());
        if name.is_empty() && character.is_none() {
            return None;
        }
        let world = player.map(|p| p.world_name.clone()).unwrap_or_default();
        let (width, height) = ui.display();
        Some(layout_file::file_name(
            name,
            &character.unwrap_or_default(),
            &world,
            width,
            height,
        ))
    }

    /// Before a frame, from the game's view: the panels of the play screen (when it is up) put
    /// back where the options keep them; the layout files asked for named, to be written or read
    /// (the shell's requests, into `requests`).
    pub fn take(
        &mut self,
        view: &View,
        ui: &mut Ui,
        frame: Option<FrameId>,
        requests: &mut Vec<Request>,
    ) {
        if let Some(file) = self
            .save_layout
            .take()
            .and_then(|name| Self::layout_name(view, ui, &name))
        {
            requests.push(Request::WritePlayerFile {
                name: file,
                bytes: layout_file::write(ui).into_bytes(),
            });
        }
        if let Some(file) = self
            .load_layout
            .take()
            .and_then(|name| Self::layout_name(view, ui, &name))
        {
            self.reading.push(file.clone());
            requests.push(Request::ReadPlayerFile { name: file });
        }
        let Some(reloaded) = self.put_back.take() else {
            return;
        };
        let Some(f) = frame else { return };
        let held = if reloaded {
            view.options.as_ref().and_then(|o| o.ui_locations.as_ref())
        } else {
            view.saved_places.as_ref()
        };
        if let Some(t) = held {
            ui.frame_apply_locations(f, &kept(t));
        }
    }

    /// The player's files read since the last frame: each layout file asked for laid out (one that
    /// isn't there changes nothing). Whether one moved the panels.
    pub fn read(&mut self, ui: &mut Ui, files: &[(String, Option<Vec<u8>>)]) -> bool {
        let mut laid_out = false;
        for (name, bytes) in files {
            let Some(at) = self.reading.iter().position(|r| r == name) else {
                continue;
            };
            self.reading.remove(at);
            if let Some(bytes) = bytes {
                layout_file::read(ui, &String::from_utf8_lossy(bytes));
                laid_out = true;
            }
        }
        laid_out
    }
}
