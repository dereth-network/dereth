//! Character selection: the screen's frame and its buttons, the list of characters, the enter
//! button over the portal, and the scene of the characters standing on the arms of a carousel.
//!
//! The list comes from what lives across screens (the account's characters as the server last
//! listed them), rebuilt whenever they come again: a row each (a deleted one red), the carousel an
//! arm per slot, Create lit while slots remain, and the row the list comes back on selected. The
//! selected row's arm turns to the front, an arm's step at a time over a second and a half (at
//! once when the list comes, or for a double press). Enter (or a double press on a row or a
//! figure) enters the world, Create goes to character creation, Options to the key map, Credits
//! to the credits, each with please wait up and the buttons ghosted; Delete asks first; a press on
//! a figure in the scene selects its row.

use std::any::Any;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg};
use auberean_ui::types::button_id;
use auberean_ui::types::text::text_msg;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::flow::mode;
use crate::pregame::{self, message, Character, Data, Notice, Request, Screen, INVALID_ID};

/// The layout the screen is built from, by enumeration value (record `0x22000002`).
pub const LAYOUT_ENUM: u32 = 0x4100_0006;

mod name {
    use auberean_ui::Token;
    pub const BACKGROUND_FIELD: Token = Token::of("BackgroundField");
    pub const ENTER_BUTTON_FIELD: Token = Token::of("EnterButtonField");
    pub const PORTAL_VIEWPORT: Token = Token::of("PortalViewport");
    pub const VIEWPORT: Token = Token::of("Viewport");
    pub const CHARACTER_LIST_BOX: Token = Token::of("CharacterListBox");
    pub const LIST_BOX_ITEM: Token = Token::of("list_box_item");
    pub const DELETED_LIST_BOX_ITEM: Token = Token::of("deleted_list_box_item");
    pub const ENTER_BUTTON: Token = Token::of("EnterButton");
    pub const CREATE_BUTTON: Token = Token::of("CreateButton");
    pub const DELETE_BUTTON: Token = Token::of("DeleteButton");
    pub const RESTORE_BUTTON: Token = Token::of("RestoreButton");
    pub const OPTIONS_BUTTON: Token = Token::of("OptionsButton");
    pub const CREDITS_BUTTON: Token = Token::of("CreditsButton");
    pub const QUIT_BUTTON: Token = Token::of("QuitButton");
    pub const DELETE_RESTORE_FIELD: Token = Token::of("DeleteRestoreField");
    pub const ERROR_MSG: Token = Token::of("ErrorMsg");
    pub const CLICK_SELECT: Token = Token::of("click_select");
    pub const DELETE: Token = Token::of("delete");
    pub const RESTORE: Token = Token::of("restore");
    /// The screen's music, on its frame's field.
    pub const MUSIC: Token = Token::of("music");
}

/// The message that no character stands in the row: "No character found in this slot."
const NO_CHARACTER: u32 = 0x1CCC_C266;

/// How many slots the carousel has when the server says nothing.
const DEFAULT_SLOTS: usize = 20;

/// One arm of the carousel: its heading (degrees clockwise from the front), and the turn under
/// way (from, the time it started, how long it takes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Arm {
    from: f32,
    to: f32,
    start: f64,
    duration: f32,
}

impl Arm {
    /// Its heading at `now`.
    #[must_use]
    pub fn heading(&self, now: f64) -> f32 {
        if self.duration <= 0.0 || now >= self.start + f64::from(self.duration) {
            return self.to;
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = (((now - self.start) / f64::from(self.duration)) as f32).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t
    }

    /// Whether it is turning at `now`.
    #[must_use]
    pub fn moving(&self, now: f64) -> bool {
        self.duration > 0.0 && now < self.start + f64::from(self.duration)
    }

    /// Set at `heading` at once.
    fn set(&mut self, heading: f32) {
        self.from = heading;
        self.to = heading;
        self.duration = 0.0;
    }

    /// Turn to `heading` from where it is at `now`, over `duration` seconds.
    fn turn_to(&mut self, heading: f32, now: f64, duration: f32) {
        self.from = self.heading(now);
        self.to = heading;
        self.start = now;
        self.duration = duration;
    }
}

/// The character selection screen's panel.
#[derive(Debug)]
pub struct CharacterSelect {
    frame: FrameId,
    background: Option<Handle>,
    enter_field: Option<Handle>,
    list: Option<Handle>,
    viewport: Option<Handle>,
    portal: Option<Handle>,
    /// The carousel: its arms, the slots, the row at the front and the one to turn to, how long a
    /// step takes, the angles of the steps beside the front and elsewhere, and whether the next
    /// turn is at once.
    arms: Vec<Arm>,
    slots: usize,
    current: usize,
    target: usize,
    move_duration: f32,
    small_angle: f32,
    large_angle: f32,
    move_immediate: bool,
    /// A selection in the scene asked, and whether its press was a double press.
    do_select: bool,
    double_click: bool,
    /// The delete question up and the row it asks about; please wait up.
    delete_context: u32,
    delete_index: usize,
    wait_context: u32,
}

impl CharacterSelect {
    /// Build the screen: the frame, the enter button, the portal and the scene, every button
    /// ghosted, the buttons, the list, the rows and the scene heard, the clock heard; then the list
    /// shown when the characters are here, and the screen's music.
    // D2-01820, D2-01821, D2-01822
    pub fn new(ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let background = ui.frame_create_root(frame, LAYOUT_ENUM, name::BACKGROUND_FIELD);
        let enter_field = ui.frame_create_root(frame, LAYOUT_ENUM, name::ENTER_BUTTON_FIELD);
        let portal = ui.frame_create_root(frame, LAYOUT_ENUM, name::PORTAL_VIEWPORT);
        let viewport = ui.frame_create_root(frame, LAYOUT_ENUM, name::VIEWPORT);
        let mut me = Self {
            frame,
            background,
            enter_field,
            list: None,
            viewport,
            portal,
            arms: Vec::new(),
            slots: 0,
            current: 0,
            target: 0,
            move_duration: 1.5,
            small_angle: 45.0,
            large_angle: 0.0,
            move_immediate: false,
            do_select: false,
            double_click: false,
            delete_context: 0,
            delete_index: 0,
            wait_context: 0,
        };
        me.ghost_buttons(ui, true);
        for (element, what) in [
            (name::VIEWPORT, msg::MOUSE_PRESS_TOP),
            (name::LIST_BOX_ITEM, msg::MOUSE_PRESS_TOP),
            (name::ENTER_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::CREATE_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::DELETE_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::RESTORE_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::OPTIONS_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::CREDITS_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::QUIT_BUTTON, msg::BUTTON_RELEASE_ON),
            (name::CHARACTER_LIST_BOX, msg::NEW_SELECTION),
            (name::LIST_BOX_ITEM, text_msg::DOWNLOAD_DONE),
            (name::DELETED_LIST_BOX_ITEM, text_msg::DOWNLOAD_DONE),
        ] {
            pregame::listen(ui, frame, element, what);
        }
        pregame::listen_global(ui, frame, global::TIME);
        me.list = background.and_then(|b| ui.child_recursive(b, name::CHARACTER_LIST_BOX));
        me.update(ui, data, out);
        crate::intro::play_music(ui, background, name::MUSIC, out);
        me
    }

    /// The scene's viewport (the characters on their arms) and the portal's.
    #[must_use]
    pub const fn viewports(&self) -> (Option<Handle>, Option<Handle>) {
        (self.viewport, self.portal)
    }

    /// The carousel's arms' headings at `now`, an arm per slot (the row on it is its place).
    #[must_use]
    pub fn arm_headings(&self, now: f64) -> Vec<f32> {
        self.arms.iter().map(|a| a.heading(now)).collect()
    }

    /// The row at the front of the carousel, and the one it turns to.
    #[must_use]
    pub const fn carousel(&self) -> (usize, usize) {
        (self.current, self.target)
    }

    /// The dialogs the screen has up (the delete question, please wait).
    #[must_use]
    pub const fn dialogs(&self) -> [u32; 2] {
        [self.delete_context, self.wait_context]
    }

    /// Please wait put up (the one up taken down first).
    // D2-01797
    fn make_please_wait_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        pregame::dialog_down(self.wait_context, out);
        self.wait_context = 0;
        self.wait_context = pregame::please_wait(ui, self.frame, out);
    }

    /// The question whether to delete a row's character (by its name), unless one is up: the
    /// buttons ghosted while it waits.
    // D2-01808
    fn make_delete_dialog(
        &mut self,
        ui: &mut Ui,
        index: usize,
        character_name: &str,
        out: &mut Vec<Request>,
    ) {
        if self.delete_context != 0 {
            return;
        }
        self.ghost_buttons(ui, true);
        let text = message(
            ui,
            "ID_CharSelect_DeleteConfirm",
            &[(
                "name",
                Var::Text(Box::new(StringInfo::literal(character_name))),
            )],
        );
        self.delete_index = index;
        self.delete_context = pregame::question(text, self.frame, out);
    }

    /// A question closed: the delete question's answer carried out.
    // D2-01802
    fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        context: u32,
        yes: bool,
        out: &mut Vec<Request>,
    ) {
        if context == self.delete_context {
            self.do_delete(ui, data, yes, out);
        }
    }

    /// The delete question answered: yes deletes the character it asked about (the buttons stay
    /// ghosted until the list comes again); no lights the buttons that never wait, lets the list
    /// be pressed again, and shows the list again.
    // D2-01796
    fn do_delete(&mut self, ui: &mut Ui, data: &mut Data, yes: bool, out: &mut Vec<Request>) {
        let index = self.delete_index;
        self.delete_index = 0;
        self.delete_context = 0;
        if !yes {
            self.ghost(ui, false, name::OPTIONS_BUTTON, false);
            self.ghost(ui, false, name::CREDITS_BUTTON, false);
            if let Some(l) = self.list {
                ui.set_attr(l, name::CLICK_SELECT, Value::Bool(true));
            }
            self.update(ui, data, out);
            return;
        }
        if let Some(c) = data.characters.as_ref().and_then(|s| s.identity(index)) {
            out.push(Request::Delete { character: c.id });
        }
    }

    fn button(&self, ui: &Ui, in_enter_field: bool, n: Token) -> Option<Handle> {
        let root = if in_enter_field {
            self.enter_field
        } else {
            self.background
        };
        root.and_then(|r| ui.child_recursive(r, n))
    }

    fn ghost(&self, ui: &mut Ui, in_enter_field: bool, n: Token, on: bool) {
        if let Some(b) = self.button(ui, in_enter_field, n) {
            ui.set_attr(b, attr::GHOSTED, Value::Bool(on));
        }
    }

    /// Ghost the buttons while the screen waits (or light those that never wait), and let the
    /// list be pressed or not.
    // D2-01794
    fn ghost_buttons(&self, ui: &mut Ui, on: bool) {
        if on {
            self.ghost(ui, false, name::CREATE_BUTTON, true);
            self.ghost(ui, true, name::ENTER_BUTTON, true);
            self.ghost(ui, false, name::DELETE_BUTTON, true);
            self.ghost(ui, false, name::RESTORE_BUTTON, true);
        }
        self.ghost(ui, false, name::OPTIONS_BUTTON, on);
        self.ghost(ui, false, name::CREDITS_BUTTON, on);
        if let Some(l) = self.list {
            ui.set_attr(l, name::CLICK_SELECT, Value::Bool(!on));
        }
    }

    /// Ask the scene for the figure under the pointer.
    // D2-01795
    fn start_3d_selection(&mut self, data: &mut Data, out: &mut Vec<Request>) {
        self.do_select = true;
        if self.viewport.is_some() {
            data.selection = None;
            out.push(Request::FindUnderPointer);
        }
    }

    /// The scene answered: the figure's row selected (turned to at once and entered for a double
    /// press).
    // D2-01816
    fn finish_3d_selection(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        found: Option<usize>,
        out: &mut Vec<Request>,
    ) {
        let double = self.double_click;
        self.do_select = false;
        self.double_click = false;
        let n = data.characters.as_ref().map_or(0, |s| s.characters.len());
        let (Some(row), Some(list)) = (found.filter(|r| *r < n), self.list) else {
            return;
        };
        ui.list_select_index(list, row, true);
        self.handle_mail(ui, data, out);
        self.move_immediate = double;
        if double {
            self.listen_to_message(ui, data, name::ENTER_BUTTON, Token(0), 0, None, out);
        }
    }

    /// The carousel steps a slot (`up`: the next row comes to the front), at once or over the
    /// step's time: the arm at the front and the one coming to it turn by the small angle, the
    /// others by the large. Returns false while the first arm is still turning.
    // D2-01806
    fn move_arms(&mut self, up: bool, immediate: bool, now: f64) -> bool {
        let n = self.slots;
        if self.arms.is_empty() || n == 0 || self.arms[0].moving(now) {
            return false;
        }
        let beside = if up {
            (self.current + n + 1) % n
        } else {
            (self.current + n - 1) % n
        };
        for i in 0..self.arms.len() {
            let big = i == self.current || i == beside;
            self.move_arm(i, big, up, immediate, now);
        }
        self.current = if up {
            (self.current + 1) % n
        } else {
            (self.current + n - 1) % n
        };
        true
    }

    /// One arm's step.
    // D2-01800
    fn move_arm(&mut self, i: usize, big: bool, up: bool, immediate: bool, now: f64) {
        let step = match (big, up) {
            (true, true) => self.small_angle,
            (true, false) => -self.small_angle,
            (false, true) => self.large_angle,
            (false, false) => -self.large_angle,
        };
        let d = self.move_duration;
        if let Some(a) = self.arms.get_mut(i) {
            let to = a.to + step;
            if immediate {
                a.set(to);
            } else {
                a.turn_to(to, now, d);
            }
        }
    }

    /// The carousel's arms, a slot each: the first at the front, the second the small angle to
    /// its left, the rest the large angle apart round to the last, the small angle to the
    /// first's right.
    // D2-01811
    fn init_arms(&mut self) {
        let n = self.slots;
        #[allow(clippy::cast_precision_loss)]
        let between = n.saturating_sub(2).max(1) as f32;
        self.large_angle = (360.0 - (self.small_angle + self.small_angle)) / between;
        for k in 1..=n {
            let mut a = Arm::default();
            if k != 1 {
                #[allow(clippy::cast_precision_loss)]
                let h = -self.small_angle - (k - 2) as f32 * self.large_angle;
                a.set(h);
            }
            self.arms.push(a);
        }
    }

    /// The arms leave the scene.
    // D2-01801
    fn unparent_and_delete_arms(&mut self) {
        self.arms.clear();
    }

    /// Show an error line at the foot of the screen.
    // D2-01805
    fn set_error(&self, ui: &mut Ui, text: &str) {
        if let Some(e) = self
            .background
            .and_then(|b| ui.child_recursive(b, name::ERROR_MSG))
        {
            ui.set_text(e, text);
        }
    }

    /// The selected row, if any.
    #[must_use]
    pub fn selected_index(&self, ui: &Ui) -> Option<usize> {
        self.list.and_then(|l| ui.list_selected_index(l))
    }

    /// What the screen's listener heard besides the clock, carried out.
    fn handle_mail(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            match m {
                Mail::Element {
                    from,
                    element,
                    what,
                    param,
                } => self.listen_to_message(ui, data, element, what, param, Some(from), out),
                Mail::Global {
                    what: global::TIME, ..
                } => self.loop_tick(ui, data, out),
                _ => {}
            }
        }
    }

    /// What an element said: Quit; a press on the scene (a selection asked); a row's name too
    /// long for it ("..."); a double press on a row, or Enter (the selected character into the
    /// world, unless there is none, it is not valid, has no name or is deleted: then why, at the
    /// foot); Create, Options, Credits; Delete (asks); Restore; a new row selected (Enter for a
    /// living character, Restore for a deleted one).
    // D2-01810
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn listen_to_message(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        element: Token,
        what: Token,
        param: u32,
        from: Option<Handle>,
        out: &mut Vec<Request>,
    ) {
        if element == name::QUIT_BUTTON {
            out.push(Request::Quit);
            return;
        }
        if element == name::VIEWPORT && what == msg::MOUSE_PRESS_TOP {
            if param != button_id::LEFT && param != button_id::DOUBLE_LEFT {
                return;
            }
            if !self.do_select {
                self.start_3d_selection(data, out);
            }
            if !self.double_click && self.do_select {
                self.double_click = param == button_id::DOUBLE_LEFT;
            }
            return;
        }
        if (element == name::DELETED_LIST_BOX_ITEM || element == name::LIST_BOX_ITEM)
            && what == text_msg::DOWNLOAD_DONE
        {
            if let Some(h) = from {
                let ellipsis = message(ui, "ID_Ellipsis", &[]);
                ui.truncate_text(h, &ellipsis);
            }
            return;
        }
        if data.characters.is_none() {
            return;
        }
        let enter = if element == name::LIST_BOX_ITEM && param == button_id::DOUBLE_LEFT {
            let (Some(row), Some(list)) = (from, self.list) else {
                return;
            };
            self.move_immediate = true;
            ui.list_select_item(list, Some(row), true);
            true
        } else {
            element == name::ENTER_BUTTON
        };
        if enter {
            let index = self.selected_index(ui);
            let set = data.characters.as_ref();
            let who = index.and_then(|i| set.and_then(|s| s.identity(i)).cloned());
            let greyed = index.is_some_and(|i| set.is_some_and(|s| s.greyed_out_for(i) != 0));
            let why = match who {
                None => {
                    let text = pregame::message_by_token(ui, NO_CHARACTER, &[]);
                    self.set_error(ui, &text);
                    return;
                }
                Some(c) if c.id == INVALID_ID => "ID_Character_Err_InvalidID",
                Some(c) if c.name.is_empty() => "ID_Character_Err_NoName",
                Some(_) if greyed => "ID_Character_Err_EnterWDel",
                Some(c) => {
                    self.ghost_buttons(ui, true);
                    out.push(Request::Play { character: c.id });
                    self.make_please_wait_dialog(ui, out);
                    pregame::queue(mode::GAMEPLAY, out);
                    return;
                }
            };
            let text = message(ui, why, &[]);
            self.set_error(ui, &text);
            return;
        }
        let queue_to = [
            (name::CREATE_BUTTON, mode::AVATAR_SELECTION),
            (name::OPTIONS_BUTTON, mode::KEYBOARD),
            (name::CREDITS_BUTTON, mode::CREDITS),
        ];
        if let Some(&(_, m)) = queue_to.iter().find(|(e, _)| *e == element) {
            self.ghost_buttons(ui, true);
            self.make_please_wait_dialog(ui, out);
            pregame::queue(m, out);
            return;
        }
        if element == name::DELETE_BUTTON || element == name::RESTORE_BUTTON {
            let index = self.selected_index(ui);
            let who = index.and_then(|i| {
                data.characters
                    .as_ref()
                    .and_then(|s| s.identity(i))
                    .map(|c| (i, c.clone()))
            });
            match who {
                Some((i, c)) if element == name::DELETE_BUTTON => {
                    self.make_delete_dialog(ui, i, &c.name, out);
                }
                Some((_, c)) => {
                    self.ghost_buttons(ui, true);
                    out.push(Request::Restore {
                        character: c.id,
                        name: c.name,
                    });
                }
                None => {
                    let text = pregame::message_by_token(ui, NO_CHARACTER, &[]);
                    self.set_error(ui, &text);
                }
            }
            return;
        }
        if element == name::CHARACTER_LIST_BOX && what == msg::NEW_SELECTION {
            let index = self.selected_index(ui);
            if let Some(i) = index {
                self.target = i;
            }
            let greyed = index.is_some_and(|i| {
                data.characters
                    .as_ref()
                    .is_some_and(|s| s.greyed_out_for(i) != 0)
            });
            if self.button(ui, true, name::ENTER_BUTTON).is_some() {
                self.ghost(ui, true, name::ENTER_BUTTON, greyed);
                if let Some(f) = self.button(ui, false, name::DELETE_RESTORE_FIELD) {
                    ui.set_state(f, if greyed { name::RESTORE } else { name::DELETE });
                    self.ghost(ui, false, name::DELETE_BUTTON, greyed);
                    self.ghost(ui, false, name::RESTORE_BUTTON, !greyed);
                }
            }
        }
    }

    /// The clock's tick: a selection the scene answered finished; the carousel turned a step
    /// toward the selected row (the shorter way round), or all the way at once.
    // D2-01818
    fn loop_tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        if self.do_select {
            if let Some(found) = data.selection.take() {
                self.finish_3d_selection(ui, data, found, out);
            }
        }
        let now = ui.now();
        let n = self.slots;
        if self.arms.is_empty() || n == 0 {
            return;
        }
        if self.target < n && self.current != self.target && !self.arms[0].moving(now) {
            if self.move_immediate {
                while self.current != self.target {
                    if !self.move_arms(false, true, now) {
                        self.move_immediate = false;
                        return;
                    }
                }
            } else {
                #[allow(clippy::cast_precision_loss)]
                let (cur, tgt, half) = (self.current as f32, self.target as f32, n as f32 * 0.5);
                let shorter = (cur - tgt).abs() < half;
                let up = if cur - tgt >= 0.0 { !shorter } else { shorter };
                self.move_arms(up, false, now);
            }
        }
        self.move_immediate = false;
    }

    /// The server answered a restore (or a character made): restored, its row living again;
    /// else why, at the foot; then the buttons that never wait lit, the list pressable, and the
    /// list shown again.
    // D2-01807
    fn char_gen_verification(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        response: u32,
        character: Option<&Character>,
        out: &mut Vec<Request>,
    ) {
        if data.characters.is_none() {
            return;
        }
        let why = match response {
            1 => {
                let index = self.selected_index(ui);
                if let (Some(c), Some(i), Some(set)) = (character, index, data.characters.as_mut())
                {
                    if set.which_slot(c.id).is_some() {
                        if let Some(at) = set.characters.get_mut(i) {
                            if at.id == c.id && at.greyed != 0 {
                                at.greyed = c.greyed;
                            }
                        }
                    }
                }
                "ID_Character_Err_OK"
            }
            2 => "ID_Character_Err_Pending",
            3 => "ID_Character_Err_NameInUse",
            5 => "ID_Character_Err_Corrupt",
            6 => "ID_Character_Err_DatabaseDown",
            7 => "ID_Character_Err_AdminPriv",
            _ => "ID_Character_Err_Unknown",
        };
        let text = message(
            ui,
            why,
            &[("response", Var::Int(i32::try_from(response).unwrap_or(0)))],
        );
        self.set_error(ui, &text);
        self.ghost(ui, false, name::OPTIONS_BUTTON, false);
        self.ghost(ui, false, name::CREDITS_BUTTON, false);
        if let Some(l) = self.list {
            ui.set_attr(l, name::CLICK_SELECT, Value::Bool(true));
        }
        self.update(ui, data, out);
    }
}

impl Screen for CharacterSelect {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// What the screen heard: the elements first, then the clock.
    fn tick(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        let mail = pregame::take(ui, self.frame);
        let mut clock = false;
        for m in mail {
            match m {
                Mail::Global {
                    what: global::TIME, ..
                } => clock = true,
                Mail::Element {
                    from,
                    element,
                    what,
                    param,
                } => self.listen_to_message(ui, data, element, what, param, Some(from), out),
                _ => {}
            }
        }
        // What the elements said in turn while the screen acted on them.
        self.handle_mail(ui, data, out);
        if clock {
            self.loop_tick(ui, data, out);
        }
    }

    /// The account's characters, shown: the error line cleared, the buttons lit, the carousel an
    /// arm per slot (the server's, 20 when it says nothing, never fewer than the characters), a
    /// row each (a deleted one red; one with no id left out; one with no name "GARBAGE!"), Create
    /// lit while slots remain, the row the list comes back on selected and turned to at once.
    // D2-01819
    fn update(&mut self, ui: &mut Ui, data: &mut Data, _out: &mut Vec<Request>) {
        let Some(set) = data.characters.clone() else {
            return;
        };
        self.set_error(ui, "");
        self.ghost_buttons(ui, false);
        self.unparent_and_delete_arms();
        self.slots = usize::try_from(set.slots).unwrap_or(DEFAULT_SLOTS);
        if self.slots <= set.characters.len() {
            self.slots = set.characters.len();
        }
        self.init_arms();
        let Some(list) = self.list else { return };
        ui.list_flush(list);
        for (i, c) in set.characters.iter().enumerate() {
            let template = if set.greyed_out_for(i) != 0 {
                name::DELETED_LIST_BOX_ITEM
            } else if c.id == INVALID_ID {
                continue;
            } else {
                name::LIST_BOX_ITEM
            };
            if let Some(row) = ui.frame_create_element(LAYOUT_ENUM, template) {
                let shown = if c.name.is_empty() {
                    "GARBAGE!"
                } else {
                    c.name.as_str()
                };
                ui.set_text(row, shown);
                ui.list_add(list, row);
            }
        }
        if set.characters.len() < self.slots {
            self.ghost(ui, false, name::CREATE_BUTTON, false);
        }
        self.current = 0;
        let selected = std::mem::take(&mut data.selected);
        ui.list_select_index(list, selected, true);
        self.move_immediate = true;
    }

    // D2-01807, D2-01802
    fn notice(&mut self, ui: &mut Ui, data: &mut Data, n: &Notice, out: &mut Vec<Request>) {
        match n {
            Notice::ConfirmationClosed { id, yes } => {
                self.close_confirmation_dialog(ui, data, *id, *yes, out);
            }
            Notice::CharGenVerification {
                response,
                character,
                ..
            } => self.char_gen_verification(ui, data, *response, character.as_ref(), out),
            _ => {}
        }
    }

    // D2-01805
    fn set_error_msg(&mut self, ui: &mut Ui, text: &str) {
        self.set_error(ui, text);
    }

    /// The screen goes: its buttons ghosted, the carousel's arms and the list's rows gone, its
    /// elements and listening, its dialogs down.
    // D2-01813, D2-01814
    fn close(mut self: Box<Self>, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        self.ghost_buttons(ui, true);
        self.unparent_and_delete_arms();
        if let Some(l) = self.list.take() {
            ui.list_flush(l);
        }
        ui.drop_frame(self.frame);
        pregame::dialog_down(self.delete_context, out);
        self.delete_context = 0;
        pregame::dialog_down(self.wait_context, out);
        self.wait_context = 0;
        self.delete_index = 0;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
