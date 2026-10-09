//! What gamepad mode adds to the item windows: X on an item (or a pack, or a thing in an open
//! chest or corpse) opens a small menu of what can be done with it, and a split is a box of its
//! own, driven by the pad.

use dereth_client_contract::panels::salvage::SalvageAction;
use dereth_client_contract::view::DropTarget;
use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{GameState, Item, Relation};
use crate::ui::kit::{self, Ctx};
use crate::ui::nav::PanelKind;
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The salvaging tool's weenie class.
pub const UST: u32 = 20646;

/// The salvaging skill's name, as the character window lists it.
const SALVAGING: &str = "Salvaging";

/// An entry of the item menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemAct {
    Use,
    Equip,
    Unequip,
    Give,
    Drop,
    /// Put into the chest or corpse open.
    Put,
    /// Take out of the chest or corpse open, into the main pack.
    Take,
    /// Pick up a thing on the ground, into the main pack.
    PickUp,
    Split,
    Sell,
    Salvage,
    Examine,
    SetToHotbar,
    /// Pick a pack up to put it down at another place among the packs.
    Move,
    /// Open a container in the world (use it).
    Open,
    /// Put what a pack holds on the shop's list.
    SellContents,
    /// Make it the selection (nothing else the pad does to an item selects it).
    SetAsTarget,
}

impl ItemAct {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Use => "Use",
            Self::Move => "Move",
            Self::Open => "Open",
            Self::SellContents => "Sell Contents",
            Self::SetAsTarget => "Set as Target",
            Self::Equip => "Equip",
            Self::Unequip => "Unequip",
            Self::Give => "Give",
            Self::Drop => "Drop",
            Self::Put => "Put",
            Self::Take => "Take",
            Self::PickUp => "Pick Up",
            Self::Split => "Split",
            Self::Sell => "Sell",
            Self::Salvage => "Salvage",
            Self::Examine => "Examine",
            Self::SetToHotbar => "Set to Hotbar",
        }
    }
}

/// The item menu, open on an item.
#[derive(Debug, Clone)]
pub struct ItemMenu {
    pub item: Item,
    /// The tile it was opened from, which it stands beside.
    pub at: Rect,
    /// Opened on the selection in the world rather than on an item in a window.
    pub world: bool,
}

/// What can be done with the selection in the world:
/// - a container (a corpse, a chest, a pack on the ground) is opened, picked up where it can be
///   (a pack), and examined;
/// - a thing on the ground is picked up and examined;
/// - anything else (a person, a creature, a door, a portal) is used and examined.
#[must_use]
pub fn world_acts(state: &GameState) -> Vec<(ItemAct, bool)> {
    let mut v = Vec::new();
    if state.target_container {
        v.push((ItemAct::Open, true));
        if !state.target_stuck {
            v.push((ItemAct::PickUp, true));
        }
    } else if state.target_pickable {
        v.push((ItemAct::PickUp, true));
    } else {
        v.push((ItemAct::Use, true));
    }
    v.push((ItemAct::Examine, true));
    v
}

/// Where the item menu is opened, which changes what it offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Situation {
    /// A shop is open: things can be sold.
    pub shop: bool,
    /// The salvage window is open.
    pub salvaging: bool,
}

/// The places on the body that hold one thing each: the head, hands and feet, and armour's
/// places (chest, abdomen, arms, legs). A thing worn there fills every one of its places at once.
/// Clothing worn under armour (shirts, trousers) layers by its own rules, which the game keeps,
/// and is left out.
pub const BODY: u32 = 0x1 | 0x20 | 0x100 | 0x7E00;

/// What is worn in any of the places `item` would fill on the body, which has to come off before
/// it goes on. Nothing for what is not worn on the body (a weapon, a ring, which have places of
/// their own to choose from).
#[must_use]
pub fn blockers(state: &GameState, item: &Item) -> Vec<ObjectId> {
    let places = item.equip_locations & BODY;
    if places == 0 {
        return Vec::new();
    }
    state
        .equipped
        .iter()
        .filter(|e| e.id != item.id && e.worn & places != 0)
        .map(|e| e.id)
        .collect()
}

/// A thing to put on, waiting for what is worn in its places to come off.
#[derive(Debug, Clone, PartialEq)]
pub struct EquipWaiting {
    pub item: ObjectId,
    pub places: u32,
    pub blockers: Vec<ObjectId>,
    /// When it was asked for: it is given up after a few seconds.
    pub since: f64,
}

/// How long a thing to put on waits for what is in its places to come off, in seconds.
const EQUIP_WAIT: f64 = 5.0;

/// Put `item` on: what is worn in any of its places comes off to the pack first, and the item
/// goes on once all of that is off (see [`equip_step`]).
#[must_use]
pub fn equip(state: &GameState, item: &Item, now: f64) -> (Vec<UiRequest>, Option<EquipWaiting>) {
    let wear = UiRequest::DragDrop {
        item: item.id,
        target: DropTarget::EquipLocation {
            mask: item.equip_locations,
            side: 0,
        },
    };
    let off = blockers(state, item);
    if off.is_empty() {
        return (vec![wear], None);
    }
    let requests = off
        .iter()
        .map(|b| UiRequest::DragDrop {
            item: *b,
            target: DropTarget::BackpackButton,
        })
        .collect();
    (
        requests,
        Some(EquipWaiting {
            item: item.id,
            places: item.equip_locations,
            blockers: off,
            since: now,
        }),
    )
}

/// The thing waiting to go on, a frame later: put on once nothing it waits for is worn; given up
/// after a few seconds. The request, and whether it still waits.
#[must_use]
pub fn equip_step(
    state: &GameState,
    waiting: &EquipWaiting,
    now: f64,
) -> (Option<UiRequest>, bool) {
    let still_worn = waiting
        .blockers
        .iter()
        .any(|b| state.equipped.iter().any(|e| e.id == *b && e.worn != 0));
    if !still_worn {
        return (
            Some(UiRequest::DragDrop {
                item: waiting.item,
                target: DropTarget::EquipLocation {
                    mask: waiting.places,
                    side: 0,
                },
            }),
            false,
        );
    }
    (None, now - waiting.since < EQUIP_WAIT)
}

/// The Ust the character carries, if it carries one.
#[must_use]
pub fn ust(state: &GameState) -> Option<ObjectId> {
    state
        .pack
        .iter()
        .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
        .find(|i| i.wcid == UST)
        .map(|i| i.id)
}

/// Whether the character can salvage now: the salvage window is open, or salvaging is trained
/// and an Ust is carried.
#[must_use]
pub fn can_salvage(state: &GameState, salvaging: bool) -> bool {
    salvaging
        || (ust(state).is_some()
            && state
                .skills
                .iter()
                .any(|s| s.name == SALVAGING && s.training >= 2))
}

/// What can be done with `item` now, and whether each can be chosen.
///
/// A thing in the chest or corpse open is taken or examined. Anything else is used (not a pack),
/// worn or wielded (where its description says it can be) or taken off, given (greyed until a
/// person is selected, and when it is attuned), dropped (put in the chest or corpse open, while
/// one is; greyed when attuned), split (a stack), sold (to the shop open), salvaged (when the
/// character can salvage; not a focus, which holds nothing), examined, or set to a cross hotbar.
/// What is worn is not given, dropped or salvaged.
#[must_use]
pub fn acts(state: &GameState, item: &Item, at: Situation) -> Vec<(ItemAct, bool)> {
    let in_loot = state
        .loot
        .as_ref()
        .is_some_and(|(_, _, items)| items.iter().any(|i| i.id == item.id));
    if in_loot {
        return vec![(ItemAct::Take, true), (ItemAct::Examine, true)];
    }
    let mut v = Vec::new();
    // The main pack is the character itself; what is worn or can be is put on and taken off
    // rather than used.
    let main_pack = state.player_id == Some(item.id)
        || state.main_pack.as_ref().is_some_and(|m| m.id == item.id);
    let equipment = item.worn != 0 || item.equip_locations != 0;
    if !item.container && !equipment {
        v.push((ItemAct::Use, true));
    }
    if item.worn != 0 {
        v.push((ItemAct::Unequip, true));
    } else if item.equip_locations != 0 {
        v.push((ItemAct::Equip, true));
    }
    // What is worn is taken off first: it is not given, dropped or salvaged from where it is.
    let worn = item.worn != 0;
    if !worn && !main_pack {
        // An attuned item keeps both entries, greyed: what is learned of it later (on examining
        // it) never takes entries away.
        let to_person = state.target.as_ref().is_some_and(|t| {
            t.id != item.id && matches!(t.relation, Relation::Npc | Relation::Player)
        });
        v.push((ItemAct::Give, to_person && !item.attuned));
        v.push((
            if state.loot.is_some() {
                ItemAct::Put
            } else {
                ItemAct::Drop
            },
            !item.attuned,
        ));
    }
    if item.stack.is_some_and(|n| n > 1) {
        v.push((ItemAct::Split, true));
    }
    // A pack or a focus beside the main pack can be moved to another place among them.
    if item.container && !main_pack && state.side_packs.iter().any(|(p, _)| p.id == item.id) {
        v.push((ItemAct::Move, true));
    }
    // To the shop open: a pack with things in it puts its things on the list, as the game does
    // with a pack dropped on the list (the pack itself is sold empty).
    if at.shop && !worn {
        let full = state
            .side_packs
            .iter()
            .any(|(p, items)| p.id == item.id && !items.is_empty());
        v.push((
            if full {
                ItemAct::SellContents
            } else {
                ItemAct::Sell
            },
            true,
        ));
    }
    let focus = item.container
        && item
            .decoration
            .as_ref()
            .is_some_and(|d| d.items_capacity == 0);
    if !worn && !focus && item.wcid != UST && can_salvage(state, at.salvaging) {
        v.push((ItemAct::Salvage, true));
    }
    v.push((ItemAct::Examine, true));
    v.push((ItemAct::SetAsTarget, true));
    v.push((ItemAct::SetToHotbar, true));
    v
}

/// The name the pad's split box is known by.
pub const SPLIT: &str = "Split";

/// Where a split puts what it takes off `item`: the first empty place after the things in
/// `item`'s own pack, as a drop of part of the stack there does.
#[must_use]
pub fn split_place(state: &GameState, item: &Item) -> DropTarget {
    let packs = std::iter::once((state.player_id.unwrap_or_default(), &state.pack))
        .chain(state.side_packs.iter().map(|(p, items)| (p.id, items)));
    for (container, items) in packs {
        if items.iter().any(|i| i.id == item.id) {
            let n = u32::try_from(items.len()).unwrap_or(0);
            return DropTarget::ItemListSlot {
                container,
                under: None,
                index: n,
                num_ui_items: n,
                dragged_is_container: false,
                container_list: false,
            };
        }
    }
    DropTarget::BackpackButton
}

/// The width of the item menu, and a row's height, in layout units.
const MENU_W: f32 = 190.0;
const ROW_H: f32 = 26.0;

impl Windows {
    /// Open the item menu on `item`, drawn at `at`.
    pub(super) fn open_item_menu(&mut self, item: &Item, at: Rect) {
        self.item_menu = Some(ItemMenu {
            item: item.clone(),
            at,
            world: false,
        });
    }

    /// Open the item menu on the selection in the world.
    /// Open the item menu on the selection in the world, hanging from `at` (where it stands on
    /// screen), or a little right of the middle of the screen where that is not known.
    pub fn open_world_menu(&mut self, target: &crate::ui::game::Target, at: Option<(f32, f32)>) {
        let mut item = target.look.clone().unwrap_or_default();
        item.id = target.id;
        item.name.clone_from(&target.name);
        self.item_menu = Some(ItemMenu {
            item,
            at: at.map_or_else(Rect::default, |(x, y)| {
                Rect::new(x - 20.0, y - 20.0, 40.0, 40.0)
            }),
            world: true,
        });
    }

    /// The item menu and the split box, over every window, while one is open; and a salvage
    /// waiting for the salvage window to open.
    pub(super) fn pad_item_boxes(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        self.pad_box = None;
        if let Some(waiting) = self.equip_waiting.take() {
            let (request, wait) = equip_step(state, &waiting, ctx.time);
            out.requests.extend(request);
            if wait {
                self.equip_waiting = Some(waiting);
            }
        }
        if let Some(item) = self
            .salvage_waiting
            .filter(|_| self.is_open(WindowId::Salvage))
        {
            self.salvage_waiting = None;
            out.requests
                .push(UiRequest::SalvageList(SalvageAction::Add(item)));
        }
        self.item_menu_box(p, ctx, state, out);
        self.split_box(p, ctx, state, out);
    }

    /// Use `item`: an Ust used while the salvage window is open closes it instead.
    pub(super) fn use_item(&mut self, state: &GameState, item: &Item, now: f64, out: &mut Outcome) {
        if item.wcid == UST && self.is_open(WindowId::Salvage) {
            out.requests
                .push(UiRequest::SalvageList(SalvageAction::Close));
            self.toggle(WindowId::Salvage, now);
            return;
        }
        out.requests.push(if state.targeting {
            UiRequest::ExecuteTargetItem(item.id)
        } else {
            UiRequest::Use(item.id)
        });
    }

    /// The item menu: a small list on the soft notice ground beside the item, the item's name
    /// over its rows, the row the focus is on lit as a hotbar slot's name is. The pad's cancel,
    /// or the focus leaving it, closes it.
    fn item_menu_box(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some(menu) = self.item_menu.clone() else {
            return;
        };
        let k = p.scale;
        let at = Situation {
            shop: self.is_open(WindowId::Vendor),
            salvaging: self.is_open(WindowId::Salvage),
        };
        let list = if menu.world {
            world_acts(state)
        } else {
            acts(state, &menu.item, at)
        };
        // Drawn as a dropdown's open list hanging from the item: the same plain dark ground,
        // rows and highlight, under the item (over it where there is no room below); for the
        // selection in the world, a little right of the middle of the screen.
        let row_h = ROW_H * k;
        #[allow(clippy::cast_precision_loss)]
        let h = row_h * list.len() as f32 + 6.0 * k;
        let w = (MENU_W * k).max(menu.at.w);
        let (x, y) = if menu.world && menu.at.w <= 0.0 {
            (p.screen.0 / 2.0 + 60.0 * k, p.screen.1 / 2.0 - h / 2.0)
        } else {
            let below = menu.at.bottom() + 2.0 * k;
            let y = if below + h <= p.screen.1 || menu.at.y - h - 2.0 * k < 0.0 {
                below
            } else {
                menu.at.y - h - 2.0 * k
            };
            (menu.at.x.min(p.screen.0 - w), y)
        };
        let rect = Rect::new(x, y, w, h);
        self.pad_box = Some(rect);
        if ctx
            .input
            .pad
            .close
            .is_some_and(|r| (r.x - rect.x).abs() < 1.0 && (r.y - rect.y).abs() < 1.0)
        {
            ctx.input.pad.close = None;
            self.item_menu = None;
            return;
        }
        ctx.input.nav.layer(rect, "Item Menu", PanelKind::Window);
        p.fill(rect, 0xFF12_161C);
        kit::plain_ground(p, rect);
        let row_style =
            TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let off = row_style.colour(ctx.colours.dim());
        let mut chosen = None;
        for (i, (act, enabled)) in list.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                x + 3.0 * k,
                y + 3.0 * k + i as f32 * row_h,
                w - 6.0 * k,
                row_h,
            );
            // An entry that cannot be chosen now shows greyed; the focus can rest on it, but it
            // does nothing.
            let over = if *enabled {
                ctx.over(&r)
            } else {
                ctx.stop(&r);
                false
            };
            // The focus starts on the first thing that can be chosen.
            if *enabled && list[..i].iter().all(|(_, e)| !e) {
                ctx.input.nav.home(r);
            }
            if over && *enabled {
                crate::ui::pregame::list_highlight(p, r, true);
            }
            p.text_in(
                if *enabled { &row_style } else { &off },
                r.offset(8.0 * k, 0.0),
                Align::Left,
                act.label(),
            );
            if *enabled && over && ctx.input.clicked(&r) {
                chosen = Some(*act);
            }
        }
        if ctx.input.hover(&rect) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        ctx.input.nav.end_layer();
        let Some(act) = chosen else {
            return;
        };
        self.item_menu = None;
        let it = &menu.item;
        let id = it.id;
        match act {
            ItemAct::Use if menu.world => out.requests.push(UiRequest::Use(id)),
            ItemAct::Use | ItemAct::Open => self.use_item(state, it, ctx.time, out),
            ItemAct::PickUp => out.actions.push(crate::ui::hud::ACTION_PICK_UP),
            // Into the slots the item's description names; what is worn there now goes to the
            // pack first, as a drop on the paper doll's slot does.
            ItemAct::Equip => {
                let (requests, waiting) = equip(state, it, ctx.time);
                out.requests.extend(requests);
                self.equip_waiting = waiting;
            }
            ItemAct::Unequip | ItemAct::Take => out.requests.push(UiRequest::DragDrop {
                item: id,
                target: DropTarget::BackpackButton,
            }),
            ItemAct::Give => {
                if let Some(t) = &state.target {
                    out.requests.push(UiRequest::GiveTo {
                        item: id,
                        target: t.id,
                        amount: it.stack.unwrap_or(1),
                    });
                }
            }
            ItemAct::Drop => out.requests.push(UiRequest::PutInWorld(id)),
            ItemAct::Put => {
                if let Some((chest, _, _)) = &state.loot {
                    out.requests.push(UiRequest::DragDrop {
                        item: id,
                        target: DropTarget::Container(*chest),
                    });
                }
            }
            ItemAct::Move => self.moving_pack = Some(id),
            ItemAct::SetAsTarget => out.requests.push(UiRequest::Select(id)),
            ItemAct::Split => {
                let n = it.stack.unwrap_or(1);
                self.split_box = Some((it.clone(), (n / 2).max(1)));
            }
            ItemAct::Sell | ItemAct::SellContents => {
                out.requests.push(UiRequest::VendorAddToSell { item: id });
            }
            // Into the salvage window; with it shut, the Ust opens it and the item goes in once
            // it is up.
            ItemAct::Salvage => {
                if self.is_open(WindowId::Salvage) {
                    out.requests
                        .push(UiRequest::SalvageList(SalvageAction::Add(id)));
                } else if let Some(ust) = ust(state) {
                    out.requests.push(UiRequest::Use(ust));
                    self.salvage_waiting = Some(id);
                }
            }
            ItemAct::Examine => {
                out.requests.push(UiRequest::Select(id));
                out.requests.push(UiRequest::Examine(id));
            }
            // The main pack stands for the character: bound, it is what an armed use goes on.
            ItemAct::SetToHotbar => {
                use crate::ui::hud::cross::CrossBind;
                let me = state.main_pack.as_ref().is_some_and(|m| m.id == id)
                    || state.player_id == Some(id);
                out.bind_to_hotbar = Some(if me {
                    CrossBind::Myself
                } else {
                    CrossBind::Item(id)
                });
            }
        }
    }

    /// The pad's split box over every window: a small frame with the stack's name, the amount
    /// split off of the whole, a slider for it, and Split and Cancel. The d-pad's left and right
    /// move the amount by one, the shoulders by ten; Split puts that many in a stack of their own
    /// beside the rest, in the same pack.
    fn split_box(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some((item, amount)) = self.split_box.clone() else {
            return;
        };
        let max = item.stack.unwrap_or(1).max(1);
        let k = p.scale;
        let (sw, sh) = p.screen;
        let (w, h) = (300.0 * k, 118.0 * k);
        let r = Rect::new(sw / 2.0 - w / 2.0, sh / 2.0 - h / 2.0, w, h);
        self.pad_box = Some(r);
        ctx.input.nav.layer(r, SPLIT, PanelKind::Box);
        p.fill(r, 0xFF12_161C);
        kit::plain_ground(p, r);
        let mut next = amount;
        let by = std::mem::take(&mut ctx.input.pad.nudge);
        if by != 0 {
            next = u32::try_from((i64::from(next) + i64::from(by)).clamp(1, i64::from(max)))
                .unwrap_or(1);
        }
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let num = TextStyle::new(Family::Numerals, 18.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let line = Rect::new(r.x + 10.0 * k, r.y + 6.0 * k, r.w - 20.0 * k, 22.0 * k);
        let name = p.fit(&text, &item.name, line.w - 90.0 * k);
        p.text_in(&text, line, Align::Left, &name);
        p.text_in(&num, line, Align::Right, &format!("{next} / {max}"));
        #[allow(clippy::cast_precision_loss)]
        let t = if max > 1 {
            (next - 1) as f32 / (max - 1) as f32
        } else {
            1.0
        };
        let track = Rect::new(r.x + 12.0 * k, r.y + 34.0 * k, r.w - 24.0 * k, 22.0 * k);
        if let Some(t) = kit::track_slider(p, ctx, track, t) {
            next = super::split::at(t, max);
        }
        let bw = 120.0 * k;
        let by = r.bottom() - 40.0 * k;
        let split = Rect::new(r.x + r.w / 2.0 - bw - 5.0 * k, by, bw, 30.0 * k);
        let cancel = Rect::new(r.x + r.w / 2.0 + 5.0 * k, by, bw, 30.0 * k);
        ctx.input.nav.home(split);
        if kit::button(p, ctx, split, "Split", true) {
            out.requests
                .push(UiRequest::StackSliderChanged { split: next, max });
            out.requests.push(UiRequest::DragDrop {
                item: item.id,
                target: split_place(state, &item),
            });
            self.split_box = None;
            ctx.input.nav.end_layer();
            return;
        }
        let cancelled = kit::button(p, ctx, cancel, "Cancel", true);
        ctx.input.nav.cancel(cancel);
        if ctx.input.hover(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        ctx.input.nav.end_layer();
        if cancelled {
            self.split_box = None;
            return;
        }
        self.split_box = Some((item, next));
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)
    use super::*;
    use crate::ui::game::Target;
    use dereth_primitives::ObjectId;

    fn item(stack: Option<u32>, worn: u32, container: bool) -> Item {
        Item {
            id: ObjectId(0x8000_0010),
            name: "Thing".into(),
            stack,
            worn,
            container,
            ..Item::default()
        }
    }

    fn names(v: &[(ItemAct, bool)]) -> Vec<ItemAct> {
        v.iter().map(|(a, _)| *a).collect()
    }

    const NONE: Situation = Situation {
        shop: false,
        salvaging: false,
    };
    const SHOP: Situation = Situation {
        shop: true,
        salvaging: false,
    };

    #[test]
    fn the_item_menu_offers_only_what_applies_and_greys_give_with_nobody_to_give_to() {
        let state = GameState::default();
        let plain = acts(&state, &item(None, 0, false), NONE);
        assert_eq!(
            names(&plain),
            [
                ItemAct::Use,
                ItemAct::Give,
                ItemAct::Drop,
                ItemAct::Examine,
                ItemAct::SetAsTarget,
                ItemAct::SetToHotbar
            ],
            "nothing says it can be worn: no Equip"
        );
        assert!(
            plain.contains(&(ItemAct::Give, false)),
            "Give shown, greyed"
        );
        let mut sword = item(None, 0, false);
        sword.equip_locations = 0x0010_0000;
        assert!(acts(&state, &sword, NONE).contains(&(ItemAct::Equip, true)));
        let worn = names(&acts(&state, &item(None, 4, false), NONE));
        assert!(worn.contains(&ItemAct::Unequip) && !worn.contains(&ItemAct::Equip));
        assert!(
            !worn.contains(&ItemAct::Give)
                && !worn.contains(&ItemAct::Drop)
                && !worn.contains(&ItemAct::Salvage),
            "what is worn is not given, dropped or salvaged"
        );
        let stack = names(&acts(&state, &item(Some(5), 0, false), SHOP));
        assert!(stack.contains(&ItemAct::Split) && stack.contains(&ItemAct::Sell));
        let pack = names(&acts(&state, &item(None, 0, true), NONE));
        assert!(!pack.contains(&ItemAct::Use) && !pack.contains(&ItemAct::Equip));
        let with_npc = GameState {
            target: Some(Target {
                id: ObjectId(0x5000_0001),
                relation: Relation::Npc,
                ..Target::default()
            }),
            ..GameState::default()
        };
        assert!(acts(&with_npc, &item(None, 0, false), NONE).contains(&(ItemAct::Give, true)));
    }

    #[test]
    fn an_attuned_item_shows_drop_and_give_greyed() {
        let mut bound = item(None, 0, false);
        bound.attuned = true;
        let v = acts(&GameState::default(), &bound, NONE);
        assert!(v.contains(&(ItemAct::Drop, false)) && v.contains(&(ItemAct::Give, false)));
        assert!(names(&v).contains(&ItemAct::Use) && names(&v).contains(&ItemAct::Examine));
    }

    #[test]
    fn with_a_chest_or_corpse_open_its_things_are_taken_and_drop_becomes_put() {
        let mut state = GameState::default();
        let mut inside = item(None, 0, false);
        inside.id = ObjectId(0x8000_0099);
        state.loot = Some((ObjectId(0x8000_0090), "Corpse".into(), vec![inside.clone()]));
        assert_eq!(
            names(&acts(&state, &inside, NONE)),
            [ItemAct::Take, ItemAct::Examine]
        );
        let carried = names(&acts(&state, &item(None, 0, false), NONE));
        assert!(carried.contains(&ItemAct::Put) && !carried.contains(&ItemAct::Drop));
    }

    #[test]
    fn salvage_is_offered_only_to_a_trained_salvager_carrying_an_ust() {
        let mut ust_item = item(None, 0, false);
        ust_item.id = ObjectId(0x8000_0077);
        ust_item.wcid = UST;
        let mut state = GameState {
            pack: vec![ust_item.clone()],
            ..GameState::default()
        };
        let thing = item(None, 0, false);
        assert!(
            !names(&acts(&state, &thing, NONE)).contains(&ItemAct::Salvage),
            "salvaging not trained"
        );
        state.skills.push(crate::ui::game::Skill {
            name: SALVAGING.into(),
            training: 2,
            ..crate::ui::game::Skill::default()
        });
        assert!(names(&acts(&state, &thing, NONE)).contains(&ItemAct::Salvage));
        assert!(
            !names(&acts(&state, &ust_item, NONE)).contains(&ItemAct::Salvage),
            "the Ust is not salvaged"
        );
        state.pack.clear();
        assert!(
            !names(&acts(&state, &thing, NONE)).contains(&ItemAct::Salvage),
            "no Ust"
        );
        let open = Situation {
            shop: false,
            salvaging: true,
        };
        assert!(names(&acts(&state, &thing, open)).contains(&ItemAct::Salvage));
    }

    #[test]
    fn a_selection_in_the_world_offers_what_its_kind_allows() {
        let mut state = GameState::default();
        assert_eq!(
            names(&world_acts(&state)),
            [ItemAct::Use, ItemAct::Examine],
            "a person, a door"
        );
        state.target_pickable = true;
        assert_eq!(
            names(&world_acts(&state)),
            [ItemAct::PickUp, ItemAct::Examine],
            "a thing on the ground"
        );
        state.target_pickable = false;
        state.target_container = true;
        assert_eq!(
            names(&world_acts(&state)),
            [ItemAct::Open, ItemAct::PickUp, ItemAct::Examine],
            "a pack on the ground"
        );
        state.target_stuck = true;
        assert_eq!(
            names(&world_acts(&state)),
            [ItemAct::Open, ItemAct::Examine],
            "a corpse, a chest"
        );
    }

    fn worn(id: u32, places: u32) -> Item {
        Item {
            id: ObjectId(id),
            worn: places,
            equip_locations: places,
            ..Item::default()
        }
    }

    /// Chest, abdomen armour: a breastplate. Chest and both arm armours: a coat.
    const PLATE: u32 = 0x200 | 0x400;
    const COAT: u32 = 0x200 | 0x800 | 0x1000;

    #[test]
    fn putting_on_armour_takes_off_everything_in_any_of_its_places_first_either_way_round() {
        let wear = |item: u32, places: u32| UiRequest::DragDrop {
            item: ObjectId(item),
            target: DropTarget::EquipLocation {
                mask: places,
                side: 0,
            },
        };
        let off = |item: u32| UiRequest::DragDrop {
            item: ObjectId(item),
            target: DropTarget::BackpackButton,
        };
        for (on, places, new_places) in [(0x10, PLATE, COAT), (0x10, COAT, PLATE)] {
            let mut state = GameState {
                equipped: vec![worn(on, places), worn(0x30, 0x8000)],
                ..GameState::default()
            };
            let mut new = worn(0x20, new_places);
            new.worn = 0;
            let (requests, waiting) = equip(&state, &new, 1.0);
            assert_eq!(
                requests,
                [off(on)],
                "only what is in its places: not the necklace"
            );
            let waiting = waiting.expect("it waits");
            assert_eq!(
                equip_step(&state, &waiting, 2.0),
                (None, true),
                "not while the old one is on"
            );
            state.equipped.retain(|e| e.id.0 != on);
            assert_eq!(
                equip_step(&state, &waiting, 2.5),
                (Some(wear(0x20, new_places)), false)
            );
        }
    }

    #[test]
    fn a_coat_over_a_breastplate_and_sleeves_takes_both_off_and_gives_up_in_the_end() {
        let state = GameState {
            equipped: vec![worn(0x10, PLATE), worn(0x11, 0x1000)],
            ..GameState::default()
        };
        let mut coat = worn(0x20, COAT);
        coat.worn = 0;
        let (requests, waiting) = equip(&state, &coat, 1.0);
        assert_eq!(requests.len(), 2);
        let waiting = waiting.unwrap();
        assert_eq!(waiting.blockers, [ObjectId(0x10), ObjectId(0x11)]);
        assert_eq!(equip_step(&state, &waiting, 10.0), (None, false));
        // A ring has two places to choose from: nothing comes off for it here.
        let mut ring = worn(0x40, 0x40000 | 0x80000);
        ring.worn = 0;
        assert_eq!(equip(&state, &ring, 1.0).0.len(), 1);
    }

    #[test]
    fn equipment_is_put_on_not_used_the_main_pack_is_kept_and_a_side_pack_can_be_moved() {
        let mut sword = item(None, 0, false);
        sword.equip_locations = 0x0010_0000;
        let state = GameState {
            player_id: Some(ObjectId(0x5000_0001)),
            side_packs: vec![(item(None, 0, true), Vec::new())],
            ..GameState::default()
        };
        let v = names(&acts(&state, &sword, NONE));
        assert!(!v.contains(&ItemAct::Use) && v.contains(&ItemAct::Equip));
        let worn = names(&acts(&state, &worn(0x11, 0x200), NONE));
        assert!(
            !worn.contains(&ItemAct::Use),
            "what is worn is taken off, not used"
        );
        let mut main = item(None, 0, true);
        main.id = ObjectId(0x5000_0001);
        let main = names(&acts(&state, &main, NONE));
        assert!(
            !main.contains(&ItemAct::Give)
                && !main.contains(&ItemAct::Drop)
                && !main.contains(&ItemAct::Move),
            "{main:?}"
        );
        let side = names(&acts(&state, &item(None, 0, true), NONE));
        assert!(side.contains(&ItemAct::Move));
    }

    #[test]
    fn a_pack_moved_is_put_down_on_the_row_of_packs_at_its_new_place() {
        let pack = |id: u32| {
            let mut p = item(None, 0, true);
            p.id = ObjectId(id);
            (p, Vec::new())
        };
        let state = GameState {
            player_id: Some(ObjectId(0x5000_0001)),
            side_packs: vec![pack(0x10), pack(0x11), pack(0x12)],
            ..GameState::default()
        };
        // The third pack to the first place beside the main pack (the column's second).
        assert_eq!(
            crate::ui::panels::pack_move(&state, ObjectId(0x12), 1),
            UiRequest::DragDrop {
                item: ObjectId(0x12),
                target: DropTarget::ItemListSlot {
                    container: ObjectId(0x5000_0001),
                    under: Some(ObjectId(0x10)),
                    index: 0,
                    num_ui_items: 3,
                    dragged_is_container: true,
                    container_list: true,
                },
            }
        );
    }

    #[test]
    fn a_pack_moved_down_to_the_last_place_takes_it() {
        let pack = |id: u32| {
            let mut p = item(None, 0, true);
            p.id = ObjectId(id);
            (p, Vec::new())
        };
        let state = GameState {
            player_id: Some(ObjectId(0x5000_0001)),
            side_packs: vec![pack(0x10), pack(0x11), pack(0x12)],
            ..GameState::default()
        };
        let UiRequest::DragDrop {
            target: DropTarget::ItemListSlot { index, under, .. },
            ..
        } = crate::ui::panels::pack_move(&state, ObjectId(0x10), 3)
        else {
            panic!("a drop on the row of packs");
        };
        assert_eq!((index, under), (3, Some(ObjectId(0x12))), "after the last");
    }

    #[test]
    fn trousers_do_not_take_off_a_shirt_that_shares_a_place_with_them() {
        // A tunic over the chest, arms and abdomen; trousers over the abdomen and legs.
        let state = GameState {
            equipped: vec![worn(0x10, 0x2 | 0x8 | 0x10 | 0x4)],
            ..GameState::default()
        };
        let mut trousers = worn(0x20, 0x4 | 0x40 | 0x80);
        trousers.worn = 0;
        assert_eq!(
            equip(&state, &trousers, 1.0).0.len(),
            1,
            "put on, nothing off"
        );
    }
}
