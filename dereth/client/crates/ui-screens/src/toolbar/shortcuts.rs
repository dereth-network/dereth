//! The eighteen shortcut slots and the four input-action ranges that fire them.
//!
//! The eighteen shortcut slots are two banks of nine with **four** separate input-action ranges
//! (primary and secondary for each bank). **Do not collapse them.**
//!
//! The shortcut store has 18 slots while retail shows one row: draw one row; the extra slots are
//! a storage matter, not a drawing one. Nothing here decides how many rows are drawn.

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::items::widget::{drag_flags, ItemListWidget, SlotInfo, TileInfo};
use crate::view::{DropTarget, GameView, UiRequest};

// **The tile's three frozen-world inputs, and the plural-name mapping that decides one of them,
// live in `items/widget.rs` and nowhere else.**
//
// The reader that fills the guard's memory and the matcher that compares it are the two halves of
// one early-out; a divergence between two copies would either flap the gate open every frame or
// close it for good, both invisible on screen and neither reddening anything. The possibility is
// removed rather than tested: see [`crate::items::widget::TileInfo`].

/// Whether a slot's tile inputs are still exactly what [`ShortcutBar::last_tiles`] holds.
///
/// A thin wrapper over [`TileInfo::matches`] for this panel's own empty-slot shape: a shortcut
/// slot holds `Option<ObjectId>`, and an empty slot that is still empty has not moved.
fn tile_matches(view: &dyn GameView, held: Option<ObjectId>, last: Option<&TileInfo>) -> bool {
    let Some(id) = held else {
        return last.is_none();
    };
    TileInfo::matches(view, id, last)
}

/// The eighteen item-list children, collected in slot
/// order: bank 1 is `0x100001A7`…`0x100001AF` (slots 0–8) and bank 2 `0x100006B7`…`0x100006BF`
/// (slots 9–17).
#[must_use]
pub fn slot_element(slot: u32) -> Option<ElementId> {
    match slot {
        0..=8 => Some(ElementId(0x1000_01A7 + slot)),
        9..=17 => Some(ElementId(0x1000_06B7 + (slot - 9))),
        _ => None,
    }
}

/// How many shortcut slots there are.
pub const SLOT_COUNT: u32 = 18;

/// What an input action asked the toolbar to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    /// Use the shortcut in `slot`, primary or secondary.
    Use { slot: u32, primary: bool },
    /// Create a shortcut to the selected object in the first free slot (`−1`) — "make a shortcut
    /// to the selected item".
    CreateToSelected,
}

/// The actions dispatched for global message 1 are listed below.
///
/// | Action range | Effect |
/// |---|---|
/// | `0x10000042` … `0x1000004D` | use slot `action − 0x10000042`, primary — slots 0–11 |
/// | `0x1000004E` … `0x10000059` | use slot `action − 0x1000004E`, secondary — slots 0–11 |
/// | `0x10000132` … `0x10000137` | use slots 12…17, primary |
/// | `0x10000138` … `0x1000013D` | use slots 12…17, secondary |
/// | `0x1000010D` | create a shortcut to the selected object |
///
/// Note the overlap the two tables describe: the first range covers **twelve** slots (0–11) even
/// though bank 1 holds nine, so actions `0x1000004B`…`0x1000004D` reach slots 9–11, which are the
/// first three of bank 2. That is what the client does and it is reproduced, not tidied.
#[must_use]
pub fn shortcut_action(action: u32) -> Option<ShortcutAction> {
    match action {
        0x1000_0042..=0x1000_004D => Some(ShortcutAction::Use {
            slot: action - 0x1000_0042,
            primary: true,
        }),
        0x1000_004E..=0x1000_0059 => Some(ShortcutAction::Use {
            slot: action - 0x1000_004E,
            primary: false,
        }),
        0x1000_0132..=0x1000_0137 => Some(ShortcutAction::Use {
            slot: 12 + (action - 0x1000_0132),
            primary: true,
        }),
        0x1000_0138..=0x1000_013D => Some(ShortcutAction::Use {
            slot: 12 + (action - 0x1000_0138),
            primary: false,
        }),
        0x1000_010D => Some(ShortcutAction::CreateToSelected),
        _ => None,
    }
}

/// The action that resets the stack-size box: "**only while the stack-size entry box does not have
/// focus** (when it does, action `0x27` resets the box to the maximum and un-focuses it)".
pub const ACTION_CANCEL: u32 = 0x27;

/// The toolbar panel's global-message handler's focus gate.
///
/// Returns `None` when the stack box has focus, because in that state action `0x27` resets the box
/// and every other action is swallowed rather than firing a shortcut.
#[must_use]
pub fn dispatch(action: u32, stack_box_has_focus: bool) -> Option<ShortcutAction> {
    if stack_box_has_focus {
        return None;
    }
    shortcut_action(action)
}

/// The eighteen live shortcut item lists, plus the two flags
/// the client keeps beside them.
///
/// # Where the nine numbers actually come from
///
/// An empty quickbar is a missing widget, not a drawing gap, but the numerals are **not** painted
/// by
/// [`ItemSlot::set_shortcut_num`](crate::items::widget::ItemSlot::set_shortcut_num) and never were.
/// Each of the eighteen lists names its *own* `ItemSlot` root through `UI_ItemList_ItemSlotID` —
/// bank 1 `0x1000043B`…`0x10000443`, bank 2 `0x100006C1`…`0x100006C9` — and in each of those roots
/// icon child `0x1000033B` declares state `0x1000001C` with one media step of its own:
///
/// | slot | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9…17 |
/// |---|---|---|---|---|---|---|---|---|---|---|
/// | empty-state image | `0x060010FA` | `0x060010FB` | `0x060010FC` | `0x060010FD` | `0x060010FE` | `0x060010FF` | `0x06001100` | `0x06001101` | `0x06001102` | `0x060074CF` |
///
/// **Those nine distinct images are the nine numbered tiles**, and the only thing needed to draw
/// them is setting UI-item state `0x1000001C`. What this type is, is the array itself: the
/// eighteen lists, what goes in them, and the numeral overlay that goes *on top of an occupied*
/// slot. [verified against the live element tree built from `client_local_English.dat`]
///
/// Bank 2 shares one image because bank 2 is not on screen: the toolbar window `<TBAR>` is 100
/// pixels tall, the strip inside it starts at y = 5, bank 1 sits at y = 58…89 and bank 2 at
/// y = 90…121 — nine tiles below the bottom edge. That is why retail shows one row of nine while
/// the shortcut manager holds eighteen.
#[derive(Debug, Default)]
pub struct ShortcutBar {
    /// The eighteen shortcut lists, in slot order. Empty until [`Self::init_shortcut_array`] has
    /// run.
    pub slots: Vec<ItemListWidget>,
    /// Whether the toolbar is active — the combat-mode notice's first step sets it to
    /// "the new mode is not magic". **Its one reader is the ghost flag every occupied slot's
    /// numeral is given (ghosted when the toolbar is not active)**, which is what greys the
    /// numerals in magic mode.
    pub toolbar_active: bool,
    /// The last dragged shortcut number — the slot a shortcut drag **left**, and
    /// where the client puts whatever that drag displaces.
    ///
    /// It is set by [`Self::on_item_list_begin_drag`] and by nothing else in retail; set on the
    /// *drop* path it would name the destination instead of the source. Its reader is the
    /// client's source-slot availability check, reached once `0x21` on a shortcut tile starts a
    /// drag.
    pub last_dragged: i32,
    /// How many UI-item elements the eighteen lists created between them. Eighteen when
    /// the bar is whole; zero means an empty bar, so it is a number.
    pub created: u32,
    /// Item creations that produced no element.
    pub create_failures: u32,
    /// How many slots the last [`Self::update`]'s numeral pass reached — eighteen when every list
    /// has its shortcut-numeral icon element, and the count includes the `-1` writes that take an
    /// emptied slot's numeral down.
    pub numerals_written: u32,
    /// How many of the eighteen tiles the last [`Self::update`] processed through the client's
    /// tooltip, cooldown-wedge, and selection-ring tail. A number because zero with a non-zero
    /// occupied count is exactly the defect that pass exists to prevent.
    pub slots_decorated: usize,
    /// How many tiles the last [`Self::do_heartbeat`] re-ran their cooldown display on.
    pub heartbeats: usize,
    /// The `(contents, toolbar_active)` the bar was last filled for — there is no per-frame walk,
    /// so the bar refills only when this moves.
    last: Option<(Vec<Option<ObjectId>>, bool)>,
    /// What [`ItemListWidget::decorate`] read for each of the eighteen tiles on the last pass —
    /// `None` for a slot that is empty or whose object the client has not seen
    /// (the object lookup returns nothing, the condition that makes the client skip decoration).
    ///
    /// It is a second snapshot rather than part of [`Self::last`] because the two edges are
    /// different notices in the client: re-runs `add_shortcut`
    /// when the *shortcut list* changes, and re-runs
    /// the item element's update when the *object* changes. A shortcut naming an object that has not
    /// arrived yet moves only the second, so folding them would make the delayed numeral
    /// unreachable — and folding them would also re-run the shortcut flush and `add_shortcut` on every
    /// descriptor change, which is not an edge the client has.
    ///
    /// **It is the whole decoration, not a per-slot bit.** An *"is the object known"* bit catches
    /// the object **arriving** and is blind to every later change *within* an already-known
    /// decoration: the structure, stack size, icon overlay, icon underlay and effects would move
    /// nothing in the guard, so a tile would never be re-decorated and a stack you drew down would
    /// keep its old count on the quickbar. `is_some()` on this vector is that bit.
    last_tiles: Vec<Option<TileInfo>>,
    /// How many slots the last pass parked a numeral on with
    /// [`ItemSlot::set_delayed_shortcut_num`](crate::items::widget::ItemSlot::set_delayed_shortcut_num)
    /// because the object was not in the table yet. A number because zero with a shortcut naming
    /// an unknown object is exactly the case that arm exists for.
    pub delayed_numerals: usize,
}

impl ShortcutBar {
    /// Eighteen recursive child lookups in slot
    /// order, each cast to an item list and retained in the slot array, with
    /// the drag handler registered on each.
    ///
    /// The drag handler is the toolbar's own sub-object; in this rebuild the screen is the
    /// handler, so "registration" is [`Self::slot_under`] answering
    /// [`Self::handle_drop_release`] rather than a stored pointer.
    ///
    /// Each list is then initialized, which builds its one
    /// UI-item element from the `ItemSlot` layout. Every shipped list carries
    /// `UI_ItemList_FixedListSize = 1` and `UI_ItemList_IsShortcut = true`, so exactly one slot
    /// element comes out of each.
    pub fn init_shortcut_array(&mut self, ui: &mut UiSystem, toolbar: ElemHandle) {
        self.slots.clear();
        for slot in 0..SLOT_COUNT {
            let Some(id) = slot_element(slot) else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(toolbar, id) else {
                continue;
            };
            self.slots.push(ItemListWidget::init(ui, h));
        }
        self.created = self.slots.iter().map(|w| w.created).sum();
        self.create_failures = self.slots.iter().map(|w| w.create_failures).sum();
        self.toolbar_active = true;
        self.last_dragged = -1;
        self.last = None;
        // `last = None` alone already forces the next pass to rebuild, so this is
        // not load-bearing today; it is here because a stale tile memory outliving the widgets it
        // describes is the kind of thing that becomes load-bearing silently when the guard above
        // it changes.
        self.last_tiles.clear();
    }

    /// Adopt a player description by flushing shortcuts, then adding the object id
    /// for each of the eighteen slots retained by the shortcut manager; this is
    /// folded together with the numeral pass from
    /// the set-combat-mode notice.
    ///
    /// The two belong together because they are two halves of one visible fact: the toolbar's
    /// add-shortcut path assigns the number unghosted, which the item
    /// update transfers to the item with its current ghost state on the next pass;
    /// the set-combat-mode notice makes the same update with the ghost flag flipped. An empty slot
    /// gets shortcut number `-1`, which is what removal applies through the object
    /// when a shortcut goes away.
    ///
    /// **Nothing here writes.** The eighteen ids come from
    /// [`GameView::shortcut`], i.e. from player state, and a
    /// drag-in leaves as a [`UiRequest`] like every other: the client predicts nothing.
    ///
    /// Returns true on a pass that changed something.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if self.slots.is_empty() {
            return false;
        }
        let want: Vec<Option<ObjectId>> = (0..SLOT_COUNT).map(|i| view.shortcut(i)).collect();
        // The combat-mode notice handler's first line.
        let active = super::combat_mode::toolbar_active(view.combat_mode());
        let shortcuts_changed = !self
            .last
            .as_ref()
            .is_some_and(|(w, a)| *w == want && *a == active);
        // **The edge, and the value compared over it.**
        //
        // One bit per slot — whether the client knows the object, which
        // `GameView::slot_decoration` answers for exactly the ids the object table does not hold —
        // is the null test both relevant client paths use. But that bit is only a **projection** of
        // what the fill below reads, and an early-out is sound exactly when the compared value is
        // a **superset** of the rebuild's inputs. The fill reads a whole [`SlotInfo`] per tile, so
        // the bit alone would catch the object arriving and nothing that happened to it
        // afterwards.
        //
        // Asked without allocating: on a still frame this walks eighteen slots and builds no
        // `String` and no `Vec` at all.
        //
        // **The length term is unfalsifiable here and is kept anyway.** `zip` stops at the shorter
        // side, so an empty `last_tiles` would make `any` answer *false* and the gate would close
        // over a bar that had never been filled. That cannot happen today, because `last_tiles`
        // is empty exactly when [`Self::last`] is `None` and `shortcuts_changed` is then already
        // true — the two are written and cleared together. That is containment *transitively*
        // rather than by construction, which is too fragile to rely on, so the term stays even
        // though no test can tell it is there.
        let tiles_changed = self.last_tiles.len() != want.len()
            || want
                .iter()
                .zip(&self.last_tiles)
                .any(|(held, last)| !tile_matches(view, *held, last.as_ref()));
        if !shortcuts_changed && !tiles_changed {
            return false;
        }
        self.toolbar_active = active;
        // One read of the seam per tile, used by the numeral fork below, by the fill, and stored
        // as the next pass's memory — so the value the guard compares and the value the screen
        // draws cannot disagree within a frame (the same rule as the inventory capacity pass).
        let tiles: Vec<Option<TileInfo>> = want
            .iter()
            .map(|held| held.and_then(|id| TileInfo::read(view, id)))
            .collect();
        // `set_contents` is handed the bare icon id because that is all its fill loop takes; the
        // decorate pass below immediately replaces it with the six-blit composite built from the
        // whole decoration, so this read is transient and is not an input the guard has to hold.
        let icon = |id: ObjectId| view.icon(id);
        // Flush the shortcuts, then add one
        // per slot. It is driven by the shortcut list and the combat mode and by nothing else, so
        // an object *arriving* must not re-run it: that edge belongs to the item element's update, below.
        if shortcuts_changed {
            self.numerals_written = 0;
            self.delayed_numerals = 0;
            for (i, w) in self.slots.iter_mut().enumerate() {
                let held = want.get(i).copied().flatten();
                let ids: Vec<ObjectId> = held.into_iter().collect();
                // Flush the list, then add the item; the list is `FixedListSize = 1`, so the
                // capacity argument is the list's own and never a container's.
                w.set_contents(ui, None, None, &ids, &icon);
                let num = if held.is_some() {
                    i32::try_from(i).unwrap_or(-1)
                } else {
                    -1
                };
                // The client's fork: if the client has not seen the object yet, the tile (if any)
                // gets a *delayed* numeral; otherwise the numeral is set at once.
                //
                // The delayed arm is what a shortcut restored from `PlayerModule` before its object
                // has been created takes — the numeral must not go up immediately, painted on an
                // empty tile, which retail does not do.
                if held.is_some() && !tiles.get(i).is_some_and(Option::is_some) {
                    if let Some(s) = w.slots.first_mut() {
                        s.set_delayed_shortcut_num(num);
                        self.delayed_numerals += 1;
                    }
                } else if w.set_shortcut_num(ui, 0, num, !active) {
                    self.numerals_written += 1;
                }
            }
        }
        // The client runs this tail for *every* list, and a quickbar tile is a UI-item element
        // like any other — so the cooldown wedge, the selection ring and the tooltip belong here
        // too. Without it a quickbar item has no tooltip and a cooldown cannot show on the one
        // control that starts it.
        //
        // **The one input of the four that the guard above CANNOT hold, and why.**
        // The three fields of [`TileInfo`] are facts about a frozen world: they change when
        // a datagram lands and are otherwise constant, so *"the compared value did not move"* and
        // *"nothing happened"* are the same statement. `cooldown_remaining` is
        // `(duration + start time) - now`, a function of the **clock**: it takes a
        // new value every frame of its own accord. Folding it into `last_tiles` would make this
        // early-out fire **never** — eighteen lists flushed, refilled and re-decorated on every
        // frame for as long as any quickbar item was on cooldown — and it would *look correct on
        // screen*, which is what makes it worse than the defect it would be fixing. So it stays
        // out, and the countdown is driven by the exemption instead:
        // [`Self::do_heartbeat`], reached from `GamePlayScreen::do_item_heartbeat`, which
        // `Hud::drive` calls every frame deliberately outside this guard. The read below is not
        // that mechanism: it is the fresh value the item element's update's own tail would have used on a
        // frame that really did rebuild, so a tile refilled mid-cooldown draws the right wedge at
        // once rather than at the next beat.
        let now = ui.now.0;
        let info = |id: ObjectId| -> Option<SlotInfo> {
            // Out of the snapshot, not off the view a second time.
            let t = want
                .iter()
                .zip(&tiles)
                .find_map(|(held, t)| (*held == Some(id)).then_some(t.as_ref()))??;
            Some(t.to_slot_info(view, now))
        };
        self.slots_decorated = self.slots.iter_mut().map(|w| w.decorate(ui, &info)).sum();
        self.last_tiles = tiles;
        self.last = Some((want, active));
        true
    }

    /// The global-message-3 listener over all eighteen tiles — the
    /// once-a-second pass that moves a cooldown wedge.
    ///
    /// Returns how many tiles re-ran their display.
    pub fn do_heartbeat(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let now = ui.now.0;
        let remaining = |id: u32| view.cooldown_remaining(id, now);
        self.heartbeats = self
            .slots
            .iter_mut()
            .map(|w| w.do_heartbeat(ui, now, &remaining))
            .sum();
        self.heartbeats
    }

    /// The item list's set-selected-item notice over all eighteen tiles.
    /// Every item list in the client registers for that notice in its post-init, so a selection change rings the quickbar as well as the pack.
    pub fn set_selected_item(
        &mut self,
        ui: &mut UiSystem,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
    ) -> usize {
        self.slots
            .iter_mut()
            .map(|w| w.set_selected_item(ui, old, new))
            .sum()
    }

    /// The item list element's element-message handler's
    /// single-selection branch over the eighteen tiles —
    /// each is an item list and reaches the same handler.
    ///
    /// `None` when the handle is in none of them. `Some(0)` for all eighteen in the shipped data:
    /// no quickbar list carries `UI_ItemList_SingleSelection`, and each holds one slot anyway, so
    /// the walk has nothing to find. It is wired because the client wires it and because a
    /// `FixedListSize` that is not 1 would make it observable.
    pub fn handle_single_selection(&mut self, ui: &mut UiSystem, h: ElemHandle) -> Option<usize> {
        for w in &mut self.slots {
            if let Some(i) = w.slot_of(h) {
                return Some(if w.single_selection {
                    w.handle_single_selection(ui, i)
                } else {
                    0
                });
            }
        }
        None
    }

    /// The toolbar panel's use shortcut: fetch the object in slot `n` and return if there is none;
    /// if a target mode is armed, execute it on that object, disarm it and return; otherwise use
    /// the object (primary) or select it.
    ///
    /// An empty slot is `None`, which is the client's no-item early-out — pressing `4` with
    /// nothing in slot 4 does nothing at all, not even a deselect, and crucially **does not clear
    /// an armed target mode**: the client leaves before the target mode is ever read.
    ///
    /// **The target-mode arm.** With a healing kit in slot 8 and a pack in slot 9, pressing 8 arms
    /// the use-on-a-target mode (the object use's `is_useable_targeted` leg, when not using
    /// immediately, sends nothing) and pressing 9 must finish that use on the pack, not send a
    /// *second* plain use of it. Retail tests the mode **before** the use is reached
    /// at all: with a target mode armed it executes the mode on the slot's object (when the id is
    /// non-zero), resets the mode to none and returns; only otherwise does it use or select.
    ///
    /// The reset to no target mode is this path's own and is **not** what
    /// the mouse does — the pointer path never clears the mode, and the
    /// click's deferred use-time tail owns that for a pointer. It is emitted by
    /// `crate::screens::gameplay::GamePlayScreen::on_global_message`, after the execute, because
    /// the consumer reads the live mode to decide what the execute *means*.
    ///
    /// `target_mode` is "a target mode is armed", which the host pushes in
    /// every frame through `GamePlayScreen::set_interaction_context`.
    #[must_use]
    pub fn use_shortcut(&self, slot: u32, primary: bool, target_mode: bool) -> Option<UiRequest> {
        let item = self.item_at(slot)?;
        if target_mode {
            return Some(UiRequest::ExecuteTargetItem(item));
        }
        Some(if primary {
            UiRequest::Use(item)
        } else {
            UiRequest::Select(item)
        })
    }

    /// The object in slot `n`, if any.
    #[must_use]
    pub fn item_at(&self, slot: u32) -> Option<ObjectId> {
        self.slots.get(usize::try_from(slot).ok()?)?.item_at(0)
    }

    /// Determine which retained shortcut list is an ancestor of an element
    /// handle belongs to.
    ///
    /// The client walks the eighteen lists asking each whether it is an
    /// ancestor of the element the drop landed on, because what the handler is handed is whichever
    /// descendant of the list the pointer was over — the list itself, its one item element,
    /// or one of that item's icon layers.
    #[must_use]
    pub fn slot_under(&self, ui: &UiSystem, target: ElemHandle) -> Option<u32> {
        let i = self.slots.iter().position(|w| {
            w.handle == target || w.slot_of(target).is_some() || is_ancestor(ui, w.handle, target)
        })?;
        u32::try_from(i).ok()
    }

    /// The item-list drag handler registered with all eighteen lists reaches this path first:
    /// dragging an item over the shortcut bar shows the green cross.
    ///
    /// The whole handler: no item id means no hint; a proxy that is not an alias (`flags & 0x0E`
    /// clear) gets drag-accept state `0x10000040`; an alias that is not a shortcut (`flags & 4`
    /// clear) gets no hint; a shortcut alias gets `0x10000040`; and it returns true every time.
    ///
    /// So a real item **or a shortcut being moved** gets the green cross unconditionally — the
    /// bar has no "wrong item" answer, because it takes anything with
    /// an object id — and a spell, vendor or salvage drag gets nothing. The handler returns true
    /// on every path, which is why [`ItemListWidget::drag_over`]'s `shortcut_list` early-out was
    /// never wrong, only unreachable in retail: the default is never consulted for these lists.
    ///
    /// `over` is the element the `0x3E` names; `info` is the drag's payload, or `None` on either
    /// of the UI-item handler's clearing routes (a zero parameter
    /// or no dragged element), which act on the tile before the handler is ever asked.
    ///
    /// **What takes the cross down on a drop is not here.** The pointer never leaves
    /// a tile it is dropped on, so no `0x3E` leave arrives; the clear is
    /// the item-list handler's `0x15` arm,
    /// which clears drag-accept state on the catcher before handling the drop release,
    /// which the eighteen shortcut lists get like any other list — see the `0x15` sweep in
    /// `GamePlayScreen::on_element_message`, which covers [`Self::slots`] too. There is
    /// no `0x3F` "drag-leave" message in retail, and the *tile* owns the state
    /// (the drag-accept icon belongs to the UI-item element); the list only
    /// decides.
    ///
    /// Returns `None` when `over` is not a tile of this bar, else `Some(state written)`.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        over: ElemHandle,
        info: Option<crate::items::widget::DropIconInfo>,
    ) -> Option<Option<dereth_ui::StateId>> {
        use crate::items::widget::drag_accept_state;
        let (w, slot) = self
            .slots
            .iter_mut()
            .find_map(|w| w.slot_of(over).map(|s| (w, s)))?;
        let Some(info) = info else {
            w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
            return Some(Some(drag_accept_state::NONE));
        };
        if info.item.is_none() {
            return Some(None);
        }
        if !info.is_inventory_move() && info.flags & drag_flags::IS_SHORTCUT == 0 {
            return Some(None);
        }
        w.slots[slot].set_drag_accept_state(ui, drag_accept_state::ACCEPT);
        Some(Some(drag_accept_state::ACCEPT))
    }

    /// The client's shortcut arm, reduced to the request it
    /// produces.
    ///
    /// The client's version is four steps — remove the shortcut in slot `n`, then create a
    /// shortcut to the item in `n` for a real item or re-add the moved shortcut in `n`, then put
    /// the displaced shortcut into the first empty slot to the right of `n` or back into the last
    /// dragged slot — and each of those ends in `AddShortCut` plus the local shortcut add.
    /// Both belong to the object side, so what leaves this crate is the drop, and the slot fills when
    /// [`GameView::shortcut`] says it has.
    ///
    /// The last dragged slot is recorded at pick-up because it is the only state the client keeps
    /// between the two halves of a shortcut-to-shortcut move.
    ///
    /// # The fork on `flags`
    ///
    /// Unlike an ordinary item list, which *refuses* every proxy with `flags & 0x0E`, the
    /// shortcut bar **forks** on the same mask. With the alias mask clear (a real item) it
    /// removes slot `n`'s shortcut, creates one to the dropped item there, and moves a displaced
    /// occupant (non-zero and not the dropped item) to the first empty slot right of `n`, if there
    /// is one. Otherwise, with `IS_SHORTCUT` (4) set (a shortcut being moved), it removes slot
    /// `n`'s shortcut, adds the moved one there, and puts a displaced occupant back in the last
    /// dragged slot if that slot is available.
    ///
    /// The alias mask is **14**, so the second arm is reached by a shortcut, a vendor
    /// or a salvage proxy and taken only by the first. The two arms differ in exactly two places —
    /// re-adding an existing shortcut in place of creating one to the item, because a shortcut
    /// alias names an object the player demonstrably already has; and the displaced occupant
    /// going back to the slot the drag *left* rather than rightwards from the slot it landed on.
    pub fn handle_drop_release(
        &mut self,
        ui: &UiSystem,
        target: ElemHandle,
        item: ObjectId,
        flags: u32,
    ) -> Option<UiRequest> {
        let slot = self.slot_under(ui, target)?;
        // The client — a real item off some other list.
        if flags & drag_flags::NOT_AN_INVENTORY_MOVE == 0 {
            return Some(UiRequest::DragDrop {
                item,
                target: DropTarget::ShortcutSlot(slot),
            });
        }
        // The client — `else if` the is-shortcut flag is set. A vendor or salvage proxy reaches
        // here and is declined, exactly as the client's `else if` declines it.
        if flags & drag_flags::IS_SHORTCUT == 0 {
            return None;
        }
        Some(UiRequest::DragDrop {
            item,
            target: DropTarget::ShortcutAlias {
                slot,
                from: self.last_dragged,
            },
        })
    }

    /// **The removal half of a shortcut move, and it happens at pick-up rather than at drop.**
    ///
    /// Retail sweeps the eighteen lists for the one that *is* the dragged list (matched by
    /// identity, not by object id) and returns if none is; it then fetches that list's item zero,
    /// does nothing (leaving the last dragged slot unwritten) if there is none, and otherwise
    /// records the slot and removes the shortcut.
    ///
    /// Two things worth not re-deriving. The argument matched is the list, not the slot number
    /// (which is unused). And **the last dragged slot is written here and nowhere else**: it is the
    /// only state the client keeps between the two halves of a shortcut-to-shortcut move, and
    /// writing it on the *drop* path instead makes it
    /// name the destination rather than the source, so a displaced occupant would be put back on
    /// top of the shortcut that displaced it.
    ///
    /// Answers the object whose shortcut was removed, for the caller to put on the wire as
    /// `RemoveShortCut` (`0x019D`) beside the player module's shortcut
    /// removal — the pair the toolbar's shortcut removal makes when asked to notify.
    pub fn on_item_list_begin_drag(&mut self, list: ElementId) -> Option<ObjectId> {
        let n = self.slots.iter().position(|w| w.element == list)?;
        // Fetching item zero and reading its object id: a shortcut list holds exactly one
        // UI-item element, and an empty one has object id zero.
        let id = self.slots[n]
            .slots
            .first()
            .and_then(|s| s.item)
            .filter(|id| id.0 != 0)?;
        self.last_dragged = i32::try_from(n).unwrap_or(-1);
        Some(id)
    }

    /// The shortcut-numeral icon's current picture and visibility, for a test that wants to see
    /// the numeral rather than trust that it was asked for.
    #[must_use]
    pub fn numeral_of(&self, ui: &UiSystem, slot: u32) -> Option<(Option<DataId>, bool)> {
        let w = self.slots.get(usize::try_from(slot).ok()?)?;
        let h = w.slots.first()?.shortcut_num_elem?;
        let n = ui.node(h)?;
        Some((
            n.region.image.as_ref().map(|g| g.did),
            n.region.flags.visible,
        ))
    }
}

/// Walk `target`'s parents looking for `ancestor`.
fn is_ancestor(ui: &UiSystem, ancestor: ElemHandle, target: ElemHandle) -> bool {
    let mut h = target;
    loop {
        if h == ancestor {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eighteen slots form two contiguous banks of nine.
    #[test]
    fn the_eighteen_slots_are_two_banks_of_nine_contiguous_ids() {
        assert_eq!(slot_element(0), Some(ElementId(0x1000_01A7)));
        assert_eq!(slot_element(8), Some(ElementId(0x1000_01AF)));
        assert_eq!(slot_element(9), Some(ElementId(0x1000_06B7)));
        assert_eq!(slot_element(17), Some(ElementId(0x1000_06BF)));
        assert_eq!(slot_element(18), None);
        // Every slot has a distinct element.
        let mut ids: Vec<u32> = (0..SLOT_COUNT)
            .map(|s| slot_element(s).unwrap().0)
            .collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert_eq!(n, 18);
    }

    /// Oracle: §2.1's five-row action table, at every range boundary. The four ranges are separate
    /// and must not be collapsed (§7).
    #[test]
    fn the_four_action_ranges_map_to_the_documented_slots_and_never_overlap() {
        use ShortcutAction::{CreateToSelected, Use};
        assert_eq!(
            shortcut_action(0x1000_0042),
            Some(Use {
                slot: 0,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_004D),
            Some(Use {
                slot: 11,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_004E),
            Some(Use {
                slot: 0,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0059),
            Some(Use {
                slot: 11,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0132),
            Some(Use {
                slot: 12,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0137),
            Some(Use {
                slot: 17,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0138),
            Some(Use {
                slot: 12,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_013D),
            Some(Use {
                slot: 17,
                primary: false
            })
        );
        assert_eq!(shortcut_action(0x1000_010D), Some(CreateToSelected));
        // Just outside every range.
        for a in [
            0x1000_0041,
            0x1000_005A,
            0x1000_0131,
            0x1000_013E,
            0x1000_010C,
        ] {
            assert_eq!(shortcut_action(a), None, "{a:#X}");
        }
        // Primary and secondary are genuinely different ranges reaching the same slot.
        assert_ne!(shortcut_action(0x1000_0042), shortcut_action(0x1000_004E));
        // Every slot 0..=17 is reachable.
        let mut seen: Vec<u32> = Vec::new();
        for a in 0x1000_0000_u32..0x1000_0200 {
            if let Some(Use { slot, .. }) = shortcut_action(a) {
                seen.push(slot);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, (0..18).collect::<Vec<u32>>());
    }

    /// Oracle: §2.1 — "**only while the stack-size entry box does not have focus**".
    #[test]
    fn a_focused_stack_box_swallows_every_shortcut_action() {
        assert!(dispatch(0x1000_0042, false).is_some());
        assert_eq!(dispatch(0x1000_0042, true), None);
        assert_eq!(dispatch(ACTION_CANCEL, true), None);
        assert_eq!(ACTION_CANCEL, 0x27);
    }
}
