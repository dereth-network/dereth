//! The skill window (a toolbar window): the skill trees and crafting.
//!
//! [`SkillWindow`] is the window: its button and key show and hide it, its close button hides it,
//! its title bar moves it (the windows' places kept when let go), and it comes to the front of the
//! toolbar's windows whenever it shows or its title bar is pressed. Its pages (the skill trees and
//! the crafting page) are panels of their own under it.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::craft::Craft;
use crate::examination::{window_to_front, BRING_TO_FRONT, WINDOW_FIELDS};
use crate::gameplay::{self, Child, Notice, Request};
use crate::skill_tree::SkillTree;

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("SkillField");
    pub const BUTTON: Token = Token::of("SkillButton");
    pub const CLOSE_BUTTON: Token = Token::of("SkillCloseButton");
    pub const HELP_BUTTON: Token = Token::of("SkillHelpButton");
    pub const PANEL_TOP: Token = Token::of("SkillPanelTop");
}

/// The skill window's panel: the window, and the crafting page and the trees under it.
#[derive(Debug)]
pub struct SkillWindow {
    frame: FrameId,
    field: Option<Handle>,
    pub craft: Option<Craft>,
    pub tree: Option<SkillTree>,
}

impl SkillWindow {
    /// The window under the toolbar's framework: its field found, its buttons, its title bar and
    /// the windows' fields heard; hidden, its field at the back of the windows.
    // D2-02632
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::FIELD);
        let craft = Some(Craft::new(ui, frame));
        let tree = Some(SkillTree::new(ui, frame, field));
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::CLOSE_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::HELP_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_RELEASE);
            ui.listen_for_message(l, name::BUTTON, msg::BUTTON_RELEASE_ON);
            for f in WINDOW_FIELDS {
                ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
            }
        }
        let mut me = Self {
            frame,
            field,
            craft,
            tree,
        };
        me.show(ui, false, &mut Vec::new());
        me
    }
}

impl SkillWindow {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            crate::skill_tree::SkillTree::notices_of(),
            crate::craft::Craft::notices_of(),
        ])
    }
}

impl Child for SkillWindow {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: it comes to the front, its field shown as it is.
    // D2-02630
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(c) = &mut self.craft {
            gameplay::hear(c, ui, out);
        }
        if let Some(t) = &mut self.tree {
            gameplay::hear(t, ui, out);
        }
        let Some(f) = self.field else { return };
        ui.broadcast(f, BRING_TO_FRONT, 0);
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(f, shown);
    }

    /// A window come to the front; the title bar pressed (to the front) and let go (the places
    /// kept); its button, close and help buttons.
    // D2-02631
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element {
            element,
            what,
            from,
            ..
        } = *mail
        else {
            return;
        };
        if what == BRING_TO_FRONT {
            window_to_front(ui, self.field, Some(from));
        }
        if element == name::PANEL_TOP {
            if what == msg::MOUSE_PRESS {
                if let Some(f) = self.field {
                    ui.broadcast(f, BRING_TO_FRONT, 0);
                }
            }
            if what == msg::MOUSE_RELEASE {
                gameplay::GamePlay::save_locations_changed(out);
            }
        }
        if element == name::BUTTON && what == msg::BUTTON_RELEASE_ON {
            let shown = ui.frame_shown(self.frame);
            self.show(ui, !shown, out);
        } else if element == name::CLOSE_BUTTON && what == msg::BUTTON_RELEASE_ON {
            self.show(ui, false, out);
        } else if element == name::HELP_BUTTON && what == msg::BUTTON_RELEASE_ON {
            out.push(Request::Help { open: true });
        }
    }

    /// The scripts' notices reach the crafting page and the trees.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        if let Some(c) = &mut self.craft {
            gameplay::hand(c, ui, n, out);
        }
        if let Some(t) = &mut self.tree {
            gameplay::hand(t, ui, n, out);
        }
    }

    /// Its own panels' frame: the crafting page and the trees.
    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, out: &mut Vec<Request>) {
        if let Some(c) = &mut self.craft {
            gameplay::hear(c, ui, out);
            c.update(ui, view, out);
        }
        if let Some(t) = &mut self.tree {
            gameplay::hear(t, ui, out);
            t.update(ui, view, out);
        }
    }

    // D2-02633
    fn close(self: Box<Self>, ui: &mut Ui, out: &mut Vec<Request>) {
        if let Some(c) = self.craft {
            Box::new(c).close(ui, out);
        }
        if let Some(t) = self.tree {
            Box::new(t).close(ui, out);
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
