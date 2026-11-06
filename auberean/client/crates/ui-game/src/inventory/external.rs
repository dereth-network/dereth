//! An opened container's window (a corpse, a chest).
//!
//! [`ExternalContainer`] listens to every object: when the player's scripts say a container
//! opened for the player, the window shows its cells ([`ContainerCells`], the container's
//! first segment) under the container where it stands on the screen (the screen's middle when it
//! is not in view), kept on the screen; when they say it closed, or the container goes, the cells
//! are let go and the window hidden. Its close button closes the container for the player;
//! take-all takes everything out of it, melt-all melts everything in it into money.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Ui};

use super::container::ContainerCells;
use super::{notice, Facts, Request, PLAY_LAYOUT_ENUM};
use crate::gameplay::{self, Child, Notice, View};

mod name {
    use auberean_ui::Token;
    pub const EXTERNAL_CONTAINER_FIELD: Token = Token::of("ExternalContainerField");
    pub const EXTERNAL_CONTAINER: Token = Token::of("ExternalContainer");
    pub const EXTERNAL_CONTAINER_LABEL: Token = Token::of("ExternalContainerLabel");
    pub const CLOSE_BUTTON: Token = Token::of("ExternalContainerCloseButton");
    pub const LOOT_ALL_BUTTON: Token = Token::of("LootAllButton");
    pub const TRANSMUTE_ALL_BUTTON: Token = Token::of("TransmuteAllButton");
}

/// An opened container's window.
#[derive(Debug)]
pub struct ExternalContainer {
    frame: FrameId,
    root: Option<Handle>,
    element: Option<Handle>,
    label: Option<Handle>,
    display: Option<ContainerCells>,
    facts: Facts,
}

impl ExternalContainer {
    /// Its window made from the play layout, its cells' element and label found, its buttons
    /// heard, every object listened to; hidden.
    // D2-02186, D2-02185, D2-02180
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let root = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::EXTERNAL_CONTAINER_FIELD);
        let mut me = Self {
            frame,
            root,
            element: None,
            label: None,
            display: None,
            facts: Facts::default(),
        };
        let Some(r) = root else { return me };
        me.element = ui.child_recursive(r, name::EXTERNAL_CONTAINER);
        if me.element.is_none() {
            return me;
        }
        me.label = ui.child_recursive(r, name::EXTERNAL_CONTAINER_LABEL);
        if me.label.is_none() {
            return me;
        }
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::CLOSE_BUTTON,
                name::LOOT_ALL_BUTTON,
                name::TRANSMUTE_ALL_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
        }
        me.display = Some(ContainerCells::new(ui, frame));
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The container shown (none while hidden).
    #[must_use]
    pub fn container(&self) -> Option<u64> {
        self.display
            .as_ref()
            .map(ContainerCells::container)
            .filter(|c| *c != super::NO_OBJECT)
    }

    /// Its cells.
    #[must_use]
    pub const fn display(&self) -> Option<&ContainerCells> {
        self.display.as_ref()
    }

    /// Under the container where it stands on the screen (`at`; the screen's middle when it is
    /// not in view), kept on the screen.
    // D2-02182
    pub fn position_window(&self, ui: &mut Ui, at: Option<(i32, i32)>) {
        let Some(r) = self.root else { return };
        let Some(rect) = ui.node(r).map(|n| n.rect) else {
            return;
        };
        let hw = rect.width() as f32 * 0.5;
        let hh = rect.height() as f32 * 0.5;
        let (dw, dh) = ui.display();
        let to = dereth_primitives::num::to_i32;
        let (mut x, mut y) = match at {
            Some((px, py)) => (to(px as f32 - hw), to(py as f32 - hh)),
            None => (to(dw as f32 * 0.5 - hw), to(dh as f32 * 0.5 - hh)),
        };
        if x < 0 {
            x = 0;
        } else if x - to(hw * -2.0) > dw {
            x = to(dw as f32 - (hw + hw));
        }
        if y < 0 {
            y = 0;
        } else if y - to(hh * -2.0) > dh {
            y = to(dh as f32 - (hh + hh));
        }
        ui.move_to(r, x, y);
    }

    /// The scripts say a container opened for the player (shown with its state, placed under it,
    /// its first segment in the cells) or closed (the cells let go, hidden).
    // D2-02184
    pub fn container_state_update(
        &mut self,
        ui: &mut Ui,
        n: &Notice,
        screen: &std::collections::BTreeMap<u64, (i32, i32)>,
        out: &mut Vec<gameplay::Request>,
    ) {
        if self.display.is_none() {
            return;
        }
        let state = n.word_at(2);
        if state == 0 {
            if let Some(d) = &mut self.display {
                d.cleanup(ui, out);
            }
            self.show(ui, false, out);
            return;
        }
        self.show(ui, state != 0, out);
        let at = n.about.and_then(|o| screen.get(&o).copied());
        self.position_window(ui, at);
        if self.element.is_some() && self.label.is_some() {
            let element = self.element;
            let facts = self.facts.clone();
            if let Some(d) = &mut self.display {
                d.init(ui, n.long(0), element, 0, 0, &facts, out);
            }
        }
    }

    /// An object went: while the window shows the container it was, the cells let go and the
    /// window hidden.
    // D2-02181
    pub fn being_deleted(&mut self, ui: &mut Ui, object: u64, out: &mut Vec<gameplay::Request>) {
        if !self.shown(ui) {
            return;
        }
        let Some(d) = &mut self.display else { return };
        if d.container() == object {
            d.cleanup(ui, out);
            self.show(ui, false, out);
        }
    }
}

impl ExternalContainer {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[
        0x409, 0x1_040C, 0x406, 0x45A, 0x1_040B, 0x408, 0x42B, 0x454, 0x1_040E,
    ];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for ExternalContainer {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: its window with it.
    // D2-02178
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<gameplay::Request>) {
        ui.frame_show(self.frame, on);
        if let Some(r) = self.root {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(r, shown);
        }
    }

    /// The close button (the container closed for the player, the cells let go, hidden), take
    /// all, melt all.
    // D2-02179
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element { element, what, .. } = *mail else {
            return;
        };
        if what != msg::BUTTON_RELEASE_ON {
            return;
        }
        let container = self
            .display
            .as_ref()
            .map_or(super::NO_OBJECT, ContainerCells::container);
        if element == name::CLOSE_BUTTON {
            if !self.shown(ui) || !self.facts.here(container) || !self.facts.here(self.facts.player)
            {
                return;
            }
            out.push(gameplay::Request::Inventory(Request::Close { container }));
            if let Some(d) = &mut self.display {
                d.cleanup(ui, out);
            }
            self.show(ui, false, out);
        } else if element == name::LOOT_ALL_BUTTON {
            if self.facts.here(self.facts.player) {
                out.push(gameplay::Request::Inventory(Request::TakeAll { container }));
            }
        } else if element == name::TRANSMUTE_ALL_BUTTON && self.facts.here(self.facts.player) {
            out.push(gameplay::Request::Inventory(Request::MeltAll { container }));
        }
    }

    fn notice(&mut self, _ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        if let Some(d) = &mut self.display {
            d.notice(n, &self.facts, out);
        }
    }

    /// A frame: the scripts' answers kept; containers opening and closing; the container gone;
    /// the cells' own.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        let mut changed = false;
        if let Some(f) = view.inventory {
            if f.version != self.facts.version || f.player != self.facts.player {
                self.facts = f.clone();
                changed = true;
            }
        }
        for n in view.everyone {
            if n.native == notice::CONTAINER_STATE_UPDATE {
                let screen = view.inventory.map(|f| f.screen.clone()).unwrap_or_default();
                self.container_state_update(ui, n, &screen, out);
            }
        }
        if let Some(c) = self.container() {
            if view.inventory.is_some() && !self.facts.here(c) {
                self.being_deleted(ui, c, out);
            }
        }
        let facts = &self.facts;
        if let Some(d) = &mut self.display {
            d.update(ui, view.everyone, facts, changed, out);
        }
    }

    /// It goes: its cells, its window.
    // D2-02177, D2-02183
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        if let Some(d) = self.display.take() {
            d.close(ui, &mut Vec::new());
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
