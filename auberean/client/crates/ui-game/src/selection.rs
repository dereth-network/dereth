//! Selecting: what a click in the world, the selection keys and the game's own notices select.
//!
//! The selection's panel makes the field that lies over the whole world behind every other panel
//! (`ItemSelectionField`), so a press on the world reaches the interface: a left, double left or
//! right press there finds the object under the pointer in the world as it was last drawn, and at
//! the next tick of the clock after the world answered, selects it (or nothing, when the press
//! found nothing): the player's scripts take it and the player looks at it. An object that may
//! not be selected is passed over, unless the player's own kind lets it select anything. A double
//! press then uses the selection (not an item the player holds); a right press asks for the
//! examination panel.
//!
//! The selection keys ask the game to select as each does (the next, the previous and the nearest
//! creature, item, player, foe, fellow or pet; the player itself, the last selection, the last
//! attacker), whose rules choose what; Escape selects nothing. The game itself lets the selection
//! go when its object is taken out of the world or may no longer be selected.
//!
//! The scripts' targeting notice shows the field's usage cursor over the world (an item waits
//! for its target) or puts the cursor back.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::{global, Cursor};
use auberean_ui::token::msg;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{Child, Notice, Request, View, PLAY_LAYOUT_ENUM};
use crate::world::{kind, World};

mod name {
    use auberean_ui::Token;
    /// The field over the whole world, behind every panel.
    pub const ITEM_SELECTION_FIELD: Token = Token::of("ItemSelectionField");
    /// Its usage cursor and the cursor's point.
    pub const USAGE_CURSOR: Token = Token::of("usage_cursor");
    pub const USAGE_CURSOR_X: Token = Token::of("usage_cursor_x");
    pub const USAGE_CURSOR_Y: Token = Token::of("usage_cursor_y");
}

/// The selection keys (interface actions).
pub mod key {
    /// Escape: select nothing.
    pub const ESCAPE: u32 = 0x4000_001E;
    /// The player itself (or, already selected, the last selection).
    pub const SELF: u32 = 0x4100_0018;
    pub const PREVIOUS_CREATURE: u32 = 0x4100_0019;
    pub const NEXT_CREATURE: u32 = 0x4100_001A;
    pub const NEAREST_CREATURE: u32 = 0x4100_001B;
    pub const PREVIOUS_ITEM: u32 = 0x4100_001C;
    pub const NEXT_ITEM: u32 = 0x4100_001D;
    pub const NEAREST_ITEM: u32 = 0x4100_001E;
    pub const PREVIOUS_PLAYER: u32 = 0x4100_001F;
    pub const NEXT_PLAYER: u32 = 0x4100_0020;
    pub const NEAREST_PLAYER: u32 = 0x4100_0021;
    pub const PREVIOUS_FOE: u32 = 0x4100_0022;
    pub const NEXT_FOE: u32 = 0x4100_0023;
    pub const NEAREST_FOE: u32 = 0x4100_0024;
    pub const PREVIOUS_FELLOW: u32 = 0x4100_0025;
    pub const NEXT_FELLOW: u32 = 0x4100_0026;
    pub const NEAREST_FELLOW: u32 = 0x4100_0027;
    /// The last selection.
    pub const LAST: u32 = 0x4100_0028;
    /// The last to attack the player.
    pub const LAST_ATTACKER: u32 = 0x4100_002A;
    pub const PREVIOUS_PET: u32 = 0x4100_005C;
    pub const NEXT_PET: u32 = 0x4100_005D;
    pub const NEAREST_PET: u32 = 0x4100_005E;
}

/// The player's scripts' notices the selection hears.
pub mod notice {
    /// An object may (1) or may no longer (0) be selected; about the object. The game follows it.
    pub const UPDATE_SELECTABILITY: u32 = 0x0085 << 16 | 0x0004;
    /// An item waits for its target (1), or no longer (0): the usage cursor.
    pub const SET_TARGETING_POINTER: u32 = 0x0092 << 16 | 0x0001;
    /// The client's own: select as a selection key would (the word: the key).
    pub const SELECT: u32 = 0x464;
}

/// The selection's panel.
#[derive(Debug)]
pub struct Selection {
    frame: FrameId,
    /// The field over the world.
    pub field: Option<Handle>,
    /// A press waits for the world to say what lies under the pointer.
    selecting: bool,
    /// The press was a double left press; a right press.
    double_click: bool,
    right_click: bool,
    /// What the listener heard this frame, acted on with the frame's view of the world.
    mail: Vec<Mail>,
}

impl Selection {
    /// The selection's framework under the play screen: the field over the world, its presses
    /// heard, the selection keys and the clock heard.
    // D2-01988, D2-01985
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::ITEM_SELECTION_FIELD);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::ITEM_SELECTION_FIELD, msg::MOUSE_PRESS_TOP);
            ui.listen_for_global_message(l, global::KEY_DOWN);
            ui.listen_for_global_message(l, global::TIME);
        }
        Self {
            frame,
            field,
            selecting: false,
            double_click: false,
            right_click: false,
            mail: Vec::new(),
        }
    }

    /// Whether a press waits for the world's answer.
    #[must_use]
    pub const fn selecting(&self) -> bool {
        self.selecting
    }

    /// A press on the world: find what lies under the pointer, unless a search is under way.
    // D2-01981
    fn start_3d_selection(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        let (x, y) = ui.pointer();
        self.selecting = true;
        out.push(Request::Pick { x, y });
    }

    /// The world said what lay under the pointer: it is selected (nothing, when nothing was
    /// there), unless it may not be and the player's kind does not let it select anything;
    /// then a double press uses it (not an item the player holds) and a right press on
    /// something asks for the examination panel.
    // D2-01984
    fn finish_3d_selection(&mut self, world: &World, out: &mut Vec<Request>) {
        if world.player == 0 || world.portal_space {
            return;
        }
        let Some(picked) = world.picked else {
            // The world has not answered yet.
            return;
        };
        self.selecting = false;
        let selected = picked.unwrap_or(0);
        let thing = world.thing(selected);
        if let Some(t) = thing {
            if !t.selectable && world.player_kind & kind::ALL_SELECTABLE != kind::ALL_SELECTABLE {
                return;
            }
        }
        out.push(Request::Select(selected));
        if self.double_click {
            let held = thing.is_some_and(|t| t.container == Some(world.player));
            if !held {
                out.push(Request::UseSelected);
            }
        }
        if self.right_click && selected != 0 {
            out.push(Request::OpenExaminationPanel);
        }
        self.right_click = false;
        self.double_click = false;
    }

    /// A selection key: Escape selects nothing (unless a text element has the keyboard); every
    /// other selection key asks the game to select as it does.
    // D2-01986, D2-01987
    pub fn select(&self, ui: &Ui, world: &World, key: u32, out: &mut Vec<Request>) {
        if world.player == 0 {
            return;
        }
        match key {
            key::ESCAPE => {
                // Escape is the text's while a text element has the keyboard.
                if ui.active().is_some() {
                    return;
                }
                out.push(Request::Select(0));
            }
            key::SELF..=key::LAST | key::LAST_ATTACKER | key::PREVIOUS_PET..=key::NEAREST_PET => {
                out.push(Request::SelectBy(key))
            }
            _ => {}
        }
    }

    /// The scripts' targeting notice: the field's usage cursor over the world while an item
    /// waits for its target, the cursor it had put back after.
    // D2-01977
    pub fn set_targeting_pointer(&self, ui: &mut Ui, on: bool) {
        let Some(f) = self.field else { return };
        if !on {
            ui.restore_saved_cursor(f);
            return;
        }
        let Some(n) = ui.node(f) else { return };
        let (Some(x), Some(y), Some(id)) = (
            n.int_attr(name::USAGE_CURSOR_X),
            n.int_attr(name::USAGE_CURSOR_Y),
            n.data_id_attr(name::USAGE_CURSOR),
        ) else {
            return;
        };
        if id == 0 {
            return;
        }
        ui.save_cursor(f);
        ui.set_element_cursor(
            f,
            Cursor {
                picture: id,
                hot_x: x,
                hot_y: y,
            },
        );
    }
}

impl Selection {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x453, 0x1_040C, 0x1_0412, 0x42B, 0x1_040B, 0x464];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for Selection {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        if let Some(f) = self.field {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(f, shown);
        }
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        let Some(world) = view.world else { return };
        for m in std::mem::take(&mut self.mail) {
            self.hear(ui, world, &m, out);
        }
        // Every object's notices reach the panel, as it listens to each object as it comes.
        // D2-01974, D2-01975
        for n in view.everyone {
            self.notice_about(ui, world, n);
        }
    }

    fn listen(&mut self, _ui: &mut Ui, mail: &Mail, _out: &mut Vec<Request>) {
        // Heard before the frame's view: acted on with it.
        self.mail.push(*mail);
    }

    /// Asked to select as a selection key would: it selects at once.
    fn notice_seen(&mut self, ui: &mut Ui, view: &View<'_>, n: &Notice, out: &mut Vec<Request>) {
        if n.native == notice::SELECT {
            if let Some(world) = view.world {
                self.select(ui, world, n.word(), out);
            }
        }
    }

    // D2-01982
    fn close(self: Box<Self>, ui: &mut Ui, _out: &mut Vec<Request>) {
        if let Some(f) = self.field {
            ui.frame_remove_root(self.frame, f);
            ui.remove(f);
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

impl Selection {
    /// What the selection's listener heard: a left, double left or right press on the world
    /// starts a search under the pointer and notes which press it was; a selection key selects;
    /// the clock finishes a search the world has answered.
    // D2-01983, D2-01989
    fn hear(&mut self, ui: &mut Ui, world: &World, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element {
                element,
                what,
                param,
                ..
            } if element == name::ITEM_SELECTION_FIELD && what == msg::MOUSE_PRESS_TOP => {
                if !matches!(
                    param,
                    button_id::LEFT | button_id::DOUBLE_LEFT | button_id::RIGHT
                ) {
                    return;
                }
                if !self.selecting && world.player != 0 && !world.portal_space {
                    self.start_3d_selection(ui, out);
                }
                if !self.double_click && self.selecting {
                    self.double_click = param == button_id::DOUBLE_LEFT;
                }
                if !self.right_click && self.selecting {
                    self.right_click = param == button_id::RIGHT;
                }
            }
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => {
                let selects = param == key::ESCAPE
                    || param == key::LAST_ATTACKER
                    || (key::SELF..=key::LAST).contains(&param)
                    || (key::PREVIOUS_PET..=key::NEAREST_PET).contains(&param);
                if selects {
                    self.select(ui, world, param, out);
                }
            }
            Mail::Global {
                what: global::TIME, ..
            } if self.selecting => self.finish_3d_selection(world, out),
            _ => {}
        }
    }

    /// A notice of the player's scripts about any object.
    fn notice_about(&mut self, ui: &mut Ui, world: &World, n: &Notice) {
        if n.native == notice::SET_TARGETING_POINTER && n.about == Some(world.player) {
            self.set_targeting_pointer(ui, n.word() != 0);
        }
    }
}

/// The token of the field over the world.
#[must_use]
pub const fn field_name() -> Token {
    name::ITEM_SELECTION_FIELD
}
