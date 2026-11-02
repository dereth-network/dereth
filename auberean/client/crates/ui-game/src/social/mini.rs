//! The fellowship's small panel: a row per fellow beside the play screen, while the player is in
//! a fellowship.
//!
//! [`MiniFellowship`] makes the panel hidden; the fellowship described puts the panel in its
//! fellowship state with a row per fellow (place, name, health and vigor, without numbers), and
//! its button shows and hides it; the fellowship gone empties and hides it. Moved by the player,
//! the windows' places are kept.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Ui};

use super::fellows::{Facts, Fellows};
use super::{notice, Fellowship};
use crate::gameplay::{self, Child, Notice, Record, Request, View, PLAY_LAYOUT_ENUM};

mod name {
    use auberean_ui::Token;
    pub const PANEL: Token = Token::of("FellowMiniPanel");
    pub const LIST: Token = Token::of("FellowMiniPanelListBox");
    pub const BUTTON: Token = Token::of("FellowshipMiniButton");
    pub const FELLOWSHIP: Token = Token::of("fellowship");
}

/// The fellowship's small panel.
#[derive(Debug)]
pub struct MiniFellowship {
    frame: FrameId,
    pub root: Option<Handle>,
    /// The fellows' rows, while in a fellowship.
    pub fellows: Option<Fellows>,
    /// The notices heard, acted on with the frame's game.
    pending: Vec<Notice>,
    player: u64,
    options: Option<u32>,
    shares: Vec<(u64, f32)>,
    /// The player's object went: its notices no longer heard.
    deaf: bool,
}

impl MiniFellowship {
    /// The panel under the play screen: its root made from the play layout; its button heard
    /// and the panel's being moved; hidden.
    // D2-02147
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::PANEL);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::PANEL, msg::MOUSE_RELEASE);
        }
        let me = Self {
            frame,
            root,
            fellows: None,
            pending: Vec::new(),
            player: 0,
            options: None,
            shares: Vec::new(),
            deaf: false,
        };
        ui.frame_show(frame, false);
        if let Some(r) = root {
            let shown = ui.frame_shown(frame);
            ui.set_visible(r, shown);
        }
        me
    }

    fn facts(&self) -> Facts<'_> {
        Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        }
    }

    /// The panel's places put back: shown again as it was.
    // D2-02143
    pub fn location_applied(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let shown = ui.frame_shown(self.frame);
        self.show(ui, shown, out);
    }

    /// The fellowship described: the panel in its fellowship state, its rows made again.
    // D2-02145
    pub fn update_fellowship(&mut self, ui: &mut Ui, f: &Fellowship, out: &mut Vec<Request>) {
        let Some(r) = self.root else { return };
        if ui.node(r).map(|n| n.state) != Some(name::FELLOWSHIP) {
            ui.set_state(r, name::FELLOWSHIP);
        }
        if let Some(old) = self.fellows.take() {
            let facts = self.facts();
            old.close(ui, &facts, out);
        }
        let list = ui.child_recursive(r, name::LIST);
        if list.is_some() {
            let facts = self.facts();
            self.fellows = Some(Fellows::new(ui, list, f, true, &facts, out));
        }
    }

    /// The fellowship gone: the rows gone, the panel hidden.
    // D2-02146
    pub fn clear_fellowship(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if let Some(old) = self.fellows.take() {
            let facts = self.facts();
            old.close(ui, &facts, out);
        }
        self.show(ui, false, out);
    }

    /// The player's object went: its notices no longer heard.
    // D2-02597
    pub fn being_deleted(&mut self) {
        self.deaf = true;
        if let Some(f) = &mut self.fellows {
            f.being_deleted();
        }
    }

    fn act(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        if self.deaf {
            return;
        }
        match n.native {
            notice::UPDATE_FELLOWSHIP => {
                if let Some(Some(Record::Fellowship(f))) = n.records.first() {
                    let f = f.as_ref().clone();
                    self.update_fellowship(ui, &f, out);
                }
            }
            notice::CLEAR_FELLOWSHIP => self.clear_fellowship(ui, out),
            _ => {}
        }
        let facts = Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        };
        if let Some(f) = &mut self.fellows {
            f.notice(ui, n, &facts, out);
        }
    }
}

impl MiniFellowship {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x41E, 0x41F, 0x1_040C, 0x420, 0x48C, 0x421, 0x422, 0x429, 0x408, 0x46D, 0x425, 0x426,
        0x427, 0x428,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for MiniFellowship {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: the panel with it.
    // D2-02178
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
        }
    }

    /// Its button (shown or hidden); the panel moved by the player (the places kept); its
    /// places put back.
    // D2-02144
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element { element, what, .. } => {
                if element == name::BUTTON {
                    let shown = ui.frame_shown(self.frame);
                    self.show(ui, !shown, out);
                }
                if element == name::PANEL && what == msg::MOUSE_RELEASE {
                    gameplay::GamePlay::save_locations_changed(out);
                }
            }
            Mail::PlacesPutBack => self.location_applied(ui, out),
            _ => {}
        }
    }

    /// Heard now, acted on with the frame's game.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        self.pending.push(n.clone());
    }

    /// The frame's game kept; the notices heard acted on; the rows' presses and keys.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let mut again = false;
        if let Some(w) = view.world {
            self.player = w.player;
            again = w.fellow_xp != self.shares;
            self.shares.clone_from(&w.fellow_xp);
            if view.gone.contains(&w.player) {
                self.being_deleted();
            }
        }
        if let Some(o) = view.options {
            self.options = Some(o.bits);
        }
        for n in std::mem::take(&mut self.pending) {
            self.act(ui, &n, out);
        }
        let facts = Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        };
        if let Some(f) = &mut self.fellows {
            if again {
                f.update_xp(ui, &facts);
            }
            f.hear(ui, out);
        }
    }

    /// It goes: the rows gone, its root gone.
    // D2-02142
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        let mut out = Vec::new();
        if let Some(f) = self.fellows.take() {
            let facts = self.facts();
            f.close(ui, &facts, &mut out);
        }
        if let Some(r) = self.root.take() {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
