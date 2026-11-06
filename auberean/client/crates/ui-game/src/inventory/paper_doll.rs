//! What the player wears: the paper doll's slots, each worn item's wear, and the player's figure.
//!
//! [`PaperDoll`] shows each worn item in the slot of its place (head, torso, legs, feet, hands,
//! back, necklace, the two rings, the two hands), as the player's scripts list them; an item that
//! wears shows its wear on the meter beside its slot (full when new) with "current / greatest" as
//! the meter's tooltip, and the meter hides when nothing worn there wears. The scripts' notices
//! keep it current: everything again, an item worn or taken off, a request about one under way or
//! failed, an item's wear changed.
//!
//! An item dropped on a slot is worn there (dropped on the figure: at its preferred place); one
//! from a container that is not the player's pack is moved into the pack first. The figure shows
//! the player as it looks (not while mounted), scaled to the race; pressing a part of it selects
//! what is worn there, and pulling on it drags that item.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::system::Value;
use auberean_ui::token::msg;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};

use super::item::ItemCell;
use super::{attr, message, notice, Facts, Request, ITEM_SHORTCUT, TOOLBAR_LAYOUT_ENUM};
use crate::gameplay::{self, Child, Notice, View};

mod name {
    use auberean_ui::Token;
    pub const PAPER_DOLL_VIEWPORT: Token = Token::of("PaperDollViewport");
    pub const ITEM_ICON: Token = Token::of("ItemIcon");
    pub const DURABILITY_METER: Token = Token::of("DurabilityMeter");
    /// The meter's tooltip.
    pub const DURABILITY: &str = "ID_Inventory_TT_Durability";
}

/// The places worn items go, each with its slot and its wear meter.
pub const SLOTS: [(u32, &str, &str); 11] = [
    (0x400, "Necklace_Slot", "Necklace_Durability"),
    (0x4, "Back_Slot", "Back_Durability"),
    (0x100, "PrimaryRing_Slot", "PrimaryRing_Durability"),
    (0x200, "SecondaryRing_Slot", "SecondaryRing_Durability"),
    (0x40, "PrimaryHand_Slot", "PrimaryHand_Durability"),
    (0x80, "SecondaryHand_Slot", "SecondaryHand_Durability"),
    (0x1, "Head_Slot", "Head_Durability"),
    (0x2, "Torso_Slot", "Torso_Durability"),
    (0x8, "Legs_Slot", "Legs_Durability"),
    (0x10, "Feet_Slot", "Feet_Durability"),
    (0x20, "Hands_Slot", "Hands_Durability"),
];

/// What the scripts' worn-items notice says changed.
pub mod change {
    pub use super::super::container::change::*;
}

/// The race quality, and the figure's height for the races drawn shorter.
pub const RACE: u32 = 0x122;

/// Whether a dropped element may go in a slot: an item's icon.
// D2-02903
#[must_use]
pub fn takes(ui: &Ui, item: Handle, _slot: Handle) -> bool {
    ui.node(item)
        .is_some_and(|n| n.id_attr(attr::SHORTCUT_TYPE) == Some(ITEM_SHORTCUT))
}

/// The place a part of the figure belongs to (0: none).
// D2-02901
#[must_use]
pub fn mesh_type_to_location(mesh: u32) -> u32 {
    match mesh {
        0x8100_0001 => 0x10,
        0x8100_0002 => 0x20,
        0x8100_0003 | 0x8100_0004 | 0x8100_001B => 1,
        0x8100_001C => 2,
        0x8100_001D => 8,
        _ => 0,
    }
}

/// The figure's height by race: 0.9 for the races 1, 2, 8 and 16, 0.75 for 4, else 1.
// D2-02900
#[must_use]
pub fn doll_scale(race: Option<i32>) -> f32 {
    match race {
        Some(1 | 2 | 8 | 0x10) => 0.9,
        Some(4) => 0.75,
        _ => 1.0,
    }
}

/// What the player wears.
#[derive(Debug)]
pub struct PaperDoll {
    frame: FrameId,
    /// The figure's viewport.
    viewport: Option<Handle>,
    /// The worn items by place, and each worn item's place.
    items: BTreeMap<u32, ItemCell>,
    locations: BTreeMap<u64, u32>,
    /// The figure was asked for.
    shown: bool,
    /// A press on the figure waits to be found; the place it found.
    clicked: bool,
    last_location: u32,
    facts: Facts,
    /// The last answer heard from the search of the figure.
    answer: Option<u64>,
}

impl PaperDoll {
    /// The figure's viewport made; presses, drops and pulls on it heard, and the clock; the
    /// figure shown; the slots ready; what the player wears shown.
    // D2-02935
    pub fn new(ui: &mut Ui, parent: FrameId, facts: Option<&Facts>) -> Self {
        let frame = ui.new_child_frame(parent);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::PAPER_DOLL_VIEWPORT, msg::MOUSE_PRESS_TOP);
            ui.listen_for_message(l, name::PAPER_DOLL_VIEWPORT, message::STOP_DRAG_ITEM);
            ui.listen_for_message(l, name::PAPER_DOLL_VIEWPORT, message::NOT_DRAGABLE);
            ui.listen_for_global_message(l, global::TIME);
        }
        let viewport = ui.frame_create_root(frame, TOOLBAR_LAYOUT_ENUM, name::PAPER_DOLL_VIEWPORT);
        let mut me = Self {
            frame,
            viewport,
            items: BTreeMap::new(),
            locations: BTreeMap::new(),
            shown: false,
            clicked: false,
            last_location: 0,
            facts: facts.cloned().unwrap_or_default(),
            answer: facts.and_then(|f| f.picked),
        };
        me.init_slots(ui);
        me
    }

    /// The figure's viewport.
    #[must_use]
    pub const fn viewport(&self) -> Option<Handle> {
        self.viewport
    }

    /// The item worn at a place.
    #[must_use]
    pub fn worn_at(&self, location: u32) -> Option<u64> {
        self.items.get(&location).map(ItemCell::item)
    }

    /// The slots: each takes an item's icon and its drops are heard; the wear meters hidden; the
    /// figure takes an item's icon.
    // D2-02928
    pub fn init_slots(&mut self, ui: &mut Ui) {
        for (_, slot, wear) in SLOTS {
            if let Some(s) = ui.element(Token::of(slot)) {
                ui.set_drag_checks(s, None, Some(takes));
                if let Some(l) = ui.frame_listener(self.frame) {
                    ui.listen_for_message(l, Token::of(slot), message::STOP_DRAG_ITEM);
                }
            }
            if let Some(w) = ui.element(Token::of(wear)) {
                ui.set_visible(w, false);
            }
        }
        if let Some(v) = ui.element(name::PAPER_DOLL_VIEWPORT) {
            ui.set_drag_checks(v, None, Some(takes));
        }
    }

    /// The player's figure: shown in the viewport, as the player looks, scaled to its race.
    // D2-02906, D2-02907
    pub fn init_paper_doll(&self, out: &mut Vec<gameplay::Request>) {
        if let Some(viewport) = self.viewport {
            out.push(gameplay::Request::Inventory(Request::ShowDoll {
                viewport,
                portrait: false,
            }));
        }
    }

    /// Everything the player wears, each in its slot.
    // D2-02934
    pub fn get_inventory(&mut self, ui: &mut Ui) {
        let worn = self.facts.worn.clone();
        for (location, item) in worn {
            self.add_item(ui, item, location);
        }
    }

    /// An item worn at a place: shown in the place's slot (as worn, from the player); its wear
    /// shown when it wears.
    // D2-02924
    pub fn add_item(&mut self, ui: &mut Ui, item: u64, location: u32) {
        let Some((_, slot, _)) = SLOTS.iter().find(|s| s.0 == location) else {
            return;
        };
        let Some(slot) = ui.element(Token::of(slot)) else {
            return;
        };
        if let Some(old) = self.items.remove(&location) {
            self.locations.remove(&old.item());
            old.close(ui);
        }
        let mut d = ItemCell::new(ui, Some(&self.facts));
        let player = self.facts.player;
        d.init(
            ui,
            slot,
            name::ITEM_ICON,
            item,
            player,
            true,
            Some(&self.facts),
        );
        self.items.insert(location, d);
        self.locations.insert(item, location);
        self.show_wear(ui, item, location);
    }

    /// An item's wear at a place: on the meter when it wears (the meter shown), else the meter
    /// emptied and hidden.
    // D2-02922, D2-02923
    fn show_wear(&self, ui: &mut Ui, item: u64, location: u32) {
        let Some((_, _, wear)) = SLOTS.iter().find(|s| s.0 == location) else {
            return;
        };
        let Some(w) = ui.element(Token::of(wear)) else {
            return;
        };
        match self.facts.item(item).and_then(|f| f.wear) {
            Some((level, max)) => {
                update_durability_meter(ui, w, level, max);
                ui.set_visible(w, true);
            }
            None => {
                set_durability_tooltip(ui, w, None);
                if let Some(m) = ui.child_recursive(w, name::DURABILITY_METER) {
                    ui.set_attr(m, auberean_ui::token::attr::POSITION, Value::Float(0.0));
                }
                ui.set_visible(w, false);
            }
        }
    }

    /// An item taken off a place: its meter emptied and hidden, its slot let go.
    // D2-02925
    pub fn remove_item(&mut self, ui: &mut Ui, item: u64, location: u32) {
        if self.items.get(&location).map(ItemCell::item) != Some(item) {
            return;
        }
        if let Some((_, _, wear)) = SLOTS.iter().find(|s| s.0 == location) {
            if let Some(w) = ui.element(Token::of(wear)) {
                set_durability_tooltip(ui, w, None);
                if let Some(m) = ui.child_recursive(w, name::DURABILITY_METER) {
                    ui.set_attr(m, auberean_ui::token::attr::POSITION, Value::Float(0.0));
                }
                ui.set_visible(w, false);
            }
        }
        self.locations.remove(&item);
        if let Some(d) = self.items.remove(&location) {
            d.close(ui);
        }
    }

    /// A request about the item at a place under way (pending) or failed (not).
    // D2-02926, D2-02927
    pub fn set_pending(&mut self, ui: &mut Ui, item: u64, location: u32, pending: bool) {
        if let Some(d) = self.items.get_mut(&location) {
            if d.item() == item {
                d.set_pending(ui, pending, Some(&self.facts));
            }
        }
    }

    /// The place an item is worn at.
    // D2-02914
    #[must_use]
    pub fn item_location(&self, item: u64) -> Option<u32> {
        self.locations.get(&item).copied()
    }

    /// The place a slot (by name) stands for (0: none).
    // D2-02911
    #[must_use]
    pub fn token_to_location(slot: Token) -> u32 {
        SLOTS
            .iter()
            .find(|s| Token::of(s.1) == slot)
            .map_or(0, |s| s.0)
    }

    /// The scripts' notice about what the player wears: everything again, one worn, one taken
    /// off, a request about one failed or under way.
    // D2-02936
    pub fn update_inventory(&mut self, ui: &mut Ui, n: &Notice) {
        let item = n.long(1);
        let location = n.word_at(3);
        match n.word_at(0) {
            change::ALL => self.get_inventory(ui),
            change::ADDED => self.add_item(ui, item, location),
            change::REMOVED => self.remove_item(ui, item, location),
            change::FAILED => self.set_pending(ui, item, location, false),
            change::UNDER_WAY => self.set_pending(ui, item, location, true),
            _ => {}
        }
    }

    /// Something dropped on a slot (worn there) or on the figure (worn at its preferred place);
    /// from another container than the player's, moved into the player's pack instead.
    // D2-02912
    pub fn paper_doll_drop(
        &self,
        ui: &Ui,
        dropped: Handle,
        on: Token,
        out: &mut Vec<gameplay::Request>,
    ) {
        if !self.facts.here(self.facts.player) {
            return;
        }
        let Some(n) = ui.node(dropped) else { return };
        let item = n
            .instance_id_attr(attr::SHORTCUT_DATA)
            .unwrap_or(super::NO_OBJECT);
        let from = n
            .instance_id_attr(attr::FROM_ID)
            .unwrap_or(super::NO_OBJECT);
        if from == super::NO_OBJECT {
            return;
        }
        let Some(it) = self.facts.item(item) else {
            return;
        };
        let location = if on == name::PAPER_DOLL_VIEWPORT {
            it.preferred_location
        } else {
            Self::token_to_location(on)
        };
        if location == 0 {
            return;
        }
        let player = self.facts.player;
        if from != player {
            out.push(gameplay::Request::Inventory(Request::Put {
                item,
                from,
                to: player,
                old_slot: 0,
                new_slot: 0,
                quantity: 1,
            }));
            return;
        }
        out.push(gameplay::Request::Inventory(Request::Wear {
            item,
            location,
        }));
    }

    /// A press on the figure: what is under the pointer there to be found.
    // D2-02902
    pub fn paper_doll_click(&mut self, out: &mut Vec<gameplay::Request>) {
        if self.viewport.is_none() {
            return;
        }
        self.clicked = true;
        self.last_location = 0;
        self.facts.picked = None;
        out.push(gameplay::Request::Inventory(Request::Pick));
    }

    /// The press found a part of the figure while the left button is still down: what is worn
    /// there is selected.
    // D2-02915
    pub fn finish_paper_doll_click(
        &mut self,
        ui: &Ui,
        part: Option<u32>,
        out: &mut Vec<gameplay::Request>,
    ) {
        let Some(part) = part else { return };
        self.clicked = false;
        if !ui.left_down() {
            return;
        }
        self.last_location = mesh_type_to_location(part);
        if self.last_location == 0 {
            return;
        }
        if let Some(item) = self.worn_at(self.last_location) {
            if self.facts.here(self.facts.player) {
                out.push(gameplay::Request::Inventory(Request::Select(item)));
            }
        }
    }

    /// The figure pulled on: the item worn at the part last pressed is dragged.
    // D2-02916
    pub fn paper_doll_drag(&self, ui: &mut Ui) {
        if let Some(icon) = self.items.get(&self.last_location).and_then(ItemCell::icon) {
            ui.start_drag(icon);
        }
    }

    /// It goes: its slots, its figure's viewport.
    // D2-02919
    pub fn close_with(mut self, ui: &mut Ui, _out: &mut Vec<gameplay::Request>) {
        for (_, d) in std::mem::take(&mut self.items) {
            d.close(ui);
        }
        if let Some(v) = self.viewport.take() {
            ui.frame_remove_root(self.frame, v);
            ui.remove(v);
        }
        ui.drop_frame(self.frame);
    }
}

/// A wear meter's tooltip: "current / greatest" (none: cleared).
// D2-02905
fn set_durability_tooltip(ui: &mut Ui, meter_field: Handle, wear: Option<(u32, u32)>) {
    let (current, max) = wear.unwrap_or((0, 0));
    let text = super::write(
        ui,
        name::DURABILITY,
        &[
            (
                "current",
                auberean_text::Value::Int(i32::try_from(current).unwrap_or(i32::MAX)),
            ),
            (
                "max",
                auberean_text::Value::Int(i32::try_from(max).unwrap_or(i32::MAX)),
            ),
        ],
    );
    ui.set_tooltip(meter_field, &text);
}

/// A wear meter: filled by how much of its greatest wear an item has left (none when it has no
/// greatest), its tooltip.
// D2-02908
fn update_durability_meter(ui: &mut Ui, field: Handle, level: u32, max: u32) {
    #[allow(clippy::cast_precision_loss)]
    let position = if max == 0 {
        0.0
    } else {
        1.0 - (level as f32 / max as f32)
    };
    set_durability_tooltip(ui, field, Some((level, max)));
    if let Some(m) = ui.child_recursive(field, name::DURABILITY_METER) {
        ui.set_attr(
            m,
            auberean_ui::token::attr::POSITION,
            Value::Float(position),
        );
    }
}

impl PaperDoll {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x1_040B, 0x1_040C, 0x405, 0x1_040E, 0x448, 0x46E, 0x454];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for PaperDoll {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden by its own flag; the figure's viewport shows while it does.
    // D2-02178
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<gameplay::Request>) {
        ui.frame_show(self.frame, on);
        if let Some(v) = self.viewport {
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(v, shown);
        }
    }

    /// A press on the figure (unless one waits), a pull on it (the item last pressed dragged), a
    /// drop on it or on a slot; the clock (a press found).
    // D2-02920, D2-02921
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => {
                if self.clicked {
                    let part = self.facts.picked.map(|p| u32::try_from(p).unwrap_or(0));
                    self.finish_paper_doll_click(ui, part, out);
                }
            }
            Mail::Element {
                element,
                what,
                param,
                ..
            } => {
                if what == msg::MOUSE_PRESS_TOP && !self.clicked {
                    if param == button_id::LEFT || param == button_id::DOUBLE_LEFT {
                        self.paper_doll_click(out);
                    }
                    return;
                }
                if what == message::NOT_DRAGABLE && self.last_location != 0 {
                    self.paper_doll_drag(ui);
                }
                if what == message::STOP_DRAG_ITEM && param != 0 {
                    if let Some(d) = ui.dropped() {
                        self.paper_doll_drop(ui, d, element, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// The crafting panel opening shows it.
    // D2-02899
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        match n.native {
            notice::OPEN_INGREDIENT_PANEL if self.viewport.is_some_and(|v| ui.shown(v)) => {
                self.show(ui, true, out);
            }
            _ => {}
        }
    }

    /// A frame: the figure shown from the first (as the player looks, whatever the scripts have
    /// said yet); the scripts' answers kept (the player known: its figure and what it wears
    /// shown; wear changed); the notices about what the player wears and how it looks.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        if !self.shown {
            self.shown = true;
            self.init_paper_doll(out);
        }
        if let Some(f) = view.inventory {
            let picked = self.facts.picked;
            if f.player != self.facts.player {
                self.facts = f.clone();
                self.facts.picked = picked;
                self.init_paper_doll(out);
                self.get_inventory(ui);
            } else if f.version != self.facts.version {
                self.facts = f.clone();
                self.facts.picked = picked;
                let worn: Vec<(u64, u32)> = self.locations.iter().map(|(i, l)| (*i, *l)).collect();
                for (item, location) in worn {
                    self.show_wear(ui, item, location);
                }
                for d in self.items.values_mut() {
                    d.refresh(ui, Some(&self.facts));
                }
            }
            // An answer counts when it comes: the last search's, still there as this one is
            // asked, does not answer it.
            if f.picked != self.answer {
                self.answer = f.picked;
                self.facts.picked = f.picked;
            }
        }
        for n in view.everyone {
            if n.about != Some(self.facts.player) || self.facts.player == super::NO_OBJECT {
                if n.native == notice::ITEM_UPDATE {
                    if let (Some(item), true) = (n.about, true) {
                        if let Some(l) = self.item_location(item) {
                            self.show_wear(ui, item, l);
                        }
                    }
                }
                continue;
            }
            if n.native == notice::UPDATE_INVENTORY {
                self.update_inventory(ui, n);
            }
            for d in self.items.values_mut() {
                d.notice(ui, n, self.facts.player);
            }
        }
        for d in self.items.values_mut() {
            d.take(ui, out);
        }
    }

    fn close(self: Box<Self>, ui: &mut Ui) {
        self.close_with(ui, &mut Vec::new());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
