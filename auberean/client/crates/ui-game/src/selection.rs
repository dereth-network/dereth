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
//! The selection keys select by kind among the objects around the player within the radar's
//! range (indoors the shorter): the next, the previous and the nearest creature, item, player,
//! foe, fellow or pet, ordered by how far each stands (along the ground, plus a fifth more for
//! each meter up or down) and then by id; after the last comes the first again. The player
//! itself, the last selection and the last attacker have keys of their own, and Escape selects
//! nothing.
//!
//! An object being taken out of the world while selected is no longer selected (the nearest foe
//! is selected in its place when the player targets what attacks it); one that may no longer be
//! selected stops being attacked and stops being selected, unless it may still be seen.
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

    /// Whether a key selects among the objects by kind (and so asks which are fellows and foes).
    #[must_use]
    pub const fn wants_targets(key: u32) -> bool {
        (key >= PREVIOUS_CREATURE && key <= NEAREST_FELLOW)
            || (key >= PREVIOUS_PET && key <= NEAREST_PET)
    }
}

/// The player's scripts' notices the selection hears.
pub mod notice {
    /// An object may (1) or may no longer (0) be selected; about the object.
    pub const UPDATE_SELECTABILITY: u32 = 0x0085 << 16 | 0x0004;
    /// An item waits for its target (1), or no longer (0): the usage cursor.
    pub const SET_TARGETING_POINTER: u32 = 0x0092 << 16 | 0x0001;
    /// The client's own: select as a selection key would (the word: the key).
    pub const SELECT: u32 = 0x464;
}

/// How far an object stands for the selection keys: along the ground, and a fifth more for each
/// meter above or below.
#[must_use]
pub fn distance(from_player: [f32; 3]) -> f32 {
    let (x, y) = (from_player[0], from_player[1]);
    (x * x + y * y).sqrt() + from_player[2].abs() * 1.2
}

/// Whether `(d1, id1)` lies beyond `(d2, id2)`: farther, or as far with a greater id.
// D2-01979
#[must_use]
pub fn farther(d1: f32, id1: u64, d2: f32, id2: u64) -> bool {
    if d2 < d1 {
        return true;
    }
    d1 == d2 && id1 > id2
}

/// Whether an object of these kind bits answers a selection key: items the item keys; players
/// the player, foe and fellow keys; other creatures the creature, foe and pet keys.
// D2-01980
#[must_use]
pub fn kind_answers(kind_bits: u32, key: u32) -> bool {
    let item = kind::ITEM.iter().any(|k| kind_bits & k == *k);
    if item {
        return (key::PREVIOUS_ITEM..=key::NEAREST_ITEM).contains(&key);
    }
    if kind_bits & kind::PLAYER == kind::PLAYER {
        return (key::PREVIOUS_PLAYER..=key::NEAREST_FELLOW).contains(&key);
    }
    if kind_bits & kind::CREATURE == kind::CREATURE {
        return matches!(
            key,
            key::PREVIOUS_CREATURE
                | key::NEXT_CREATURE
                | key::NEAREST_CREATURE
                | key::PREVIOUS_FOE
                | key::NEXT_FOE
                | key::NEAREST_FOE
                | key::PREVIOUS_PET
                | key::NEXT_PET
                | key::NEAREST_PET
        );
    }
    false
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

    /// A selection key: see the module's description.
    // D2-01986, D2-01987
    pub fn select(&self, ui: &Ui, world: &World, key: u32, out: &mut Vec<Request>) {
        if world.player == 0 {
            return;
        }
        let me = world.player;
        let mut fellow = false;
        let mut foe = false;
        let mut pet = false;
        match key {
            key::SELF => {
                if world.selection != me {
                    out.push(Request::Select(me));
                } else {
                    out.push(Request::Select(world.last_selection));
                }
                return;
            }
            key::ESCAPE => {
                // Escape is the text's while a text element has the keyboard.
                if ui.active().is_some() {
                    return;
                }
                out.push(Request::Select(0));
                return;
            }
            key::LAST => {
                out.push(Request::Select(world.last_selection));
                return;
            }
            key::LAST_ATTACKER => {
                out.push(Request::Select(world.last_attacker));
                return;
            }
            key::PREVIOUS_FOE..=key::NEAREST_FOE => foe = true,
            key::PREVIOUS_FELLOW..=key::NEAREST_FELLOW => fellow = true,
            key::PREVIOUS_PET..=key::NEAREST_PET => pet = true,
            _ => {}
        }
        let direction = match key {
            key::PREVIOUS_CREATURE
            | key::PREVIOUS_ITEM
            | key::PREVIOUS_PLAYER
            | key::PREVIOUS_FOE
            | key::PREVIOUS_FELLOW
            | key::PREVIOUS_PET => -1,
            key::NEXT_CREATURE
            | key::NEXT_ITEM
            | key::NEXT_PLAYER
            | key::NEXT_FOE
            | key::NEXT_FELLOW
            | key::NEXT_PET => 1,
            key::NEAREST_CREATURE
            | key::NEAREST_ITEM
            | key::NEAREST_PLAYER
            | key::NEAREST_FOE
            | key::NEAREST_FELLOW
            | key::NEAREST_PET => 0,
            _ => return,
        };
        let range = world.range();
        // The square of the range bounds the search on the ground, and stands for the farthest
        // distance a search starts from (the original compares the two as they are).
        let range_sq = range * range;
        // Where the search starts: from the selection (when it is in range), or from the
        // nearest (or the farthest) for a fresh search.
        let mut current = 0u64;
        let mut current_d = 0.0f32;
        if direction != 0 {
            if let Some(t) = world.thing(world.selection) {
                let [x, y, _] = t.from_player;
                if x * x + y * y <= range_sq {
                    current = t.id;
                    current_d = distance(t.from_player);
                }
            }
        }
        let backwards = direction == -1;
        let mut best_d = range_sq;
        if current != 0 || backwards {
            if current == 0 {
                current_d = range_sq;
            }
            if backwards {
                best_d = 0.0;
            }
        }
        let mut best = 0u64;
        for pass in 0..2 {
            for t in &world.things {
                if t.id == current || t.id == me || !t.selectable {
                    continue;
                }
                if !kind_answers(t.kind, key) {
                    continue;
                }
                if fellow && !t.fellow {
                    continue;
                }
                if foe && !(t.attackable && !t.fellow) {
                    continue;
                }
                if pet && t.owner != Some(me) {
                    continue;
                }
                if foe {
                    if let Some(o) = t.owner {
                        if o == me || t.owner_fellow {
                            continue;
                        }
                    }
                }
                let [x, y, _] = t.from_player;
                if x * x + y * y > range_sq {
                    continue;
                }
                let d = distance(t.from_player);
                if farther(d, t.id, best_d, best) == backwards
                    && farther(d, t.id, current_d, current) != backwards
                {
                    best = t.id;
                    best_d = d;
                }
            }
            if best != 0 {
                out.push(Request::Select(best));
                return;
            }
            if current == 0 || pass == 1 {
                return;
            }
            // Round again from the other end.
            current_d = if backwards { range_sq } else { 0.0 };
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

    /// An object may or may no longer be selected: when it is the selection (not the player)
    /// and may no longer be, it is no longer selected unless it is dead, it stops being
    /// attacked, and the nearest foe is selected when the player targets what attacks it.
    // D2-01990
    pub fn update_selectability(
        &self,
        ui: &Ui,
        world: &World,
        object: u64,
        selectable: bool,
        dead: bool,
        out: &mut Vec<Request>,
    ) {
        if world.player == 0
            || object == 0
            || world.selection != object
            || object == world.player
            || selectable
        {
            return;
        }
        if !dead {
            out.push(Request::Select(0));
        }
        out.push(Request::CeaseAttack(object));
        if world.auto_target {
            self.select(ui, world, key::NEAREST_FOE, out);
        }
    }

    /// An object went from the world: when it was the selection, the player stops attacking it
    /// and selects the nearest foe when it targets what attacks it, and it is no longer
    /// selected if nothing else was.
    // D2-01991
    pub fn being_deleted(&self, ui: &Ui, world: &World, object: u64, out: &mut Vec<Request>) {
        if world.player == 0 || object == 0 || world.selection != object {
            return;
        }
        let mut asked = Vec::new();
        if world.auto_target {
            out.push(Request::CeaseAttack(object));
            self.select(ui, world, key::NEAREST_FOE, &mut asked);
        }
        let other = asked
            .iter()
            .any(|r| matches!(r, Request::Select(x) if *x != object));
        out.extend(asked);
        if !other {
            out.push(Request::Select(0));
        }
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
            self.notice_about(ui, world, n, out);
        }
        for gone in view.gone {
            self.being_deleted(ui, world, *gone, out);
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
    fn close(self: Box<Self>, ui: &mut Ui) {
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
    fn notice_about(&mut self, ui: &mut Ui, world: &World, n: &Notice, out: &mut Vec<Request>) {
        match n.native {
            notice::SET_TARGETING_POINTER if n.about == Some(world.player) => {
                self.set_targeting_pointer(ui, n.word() != 0);
            }
            notice::UPDATE_SELECTABILITY => {
                let Some(object) = n.about else { return };
                let dead = world.thing(object).is_some_and(|t| t.dead);
                self.update_selectability(ui, world, object, n.word() != 0, dead, out);
            }
            _ => {}
        }
    }
}

/// The token of the field over the world.
#[must_use]
pub const fn field_name() -> Token {
    name::ITEM_SELECTION_FIELD
}
