//! The item list: the slots of one container or shortcut bar.

use super::*;

/// Everything [`ItemListWidget::decorate`] reads for one slot **except the clock** — the frozen
/// world half of a [`SlotInfo`].
///
/// `decorate`'s input per slot is a [`SlotInfo`], and a `SlotInfo` is **four** things, not one:
/// the whole [`crate::view::SlotDecoration`], the object's name, its plural name, and
/// `cooldown_remaining`. The first three are facts about a frozen world and belong in an
/// early-out's compared value; the fourth is `(duration + start time) - current time`, a
/// function of the **clock**, which takes a new value every frame of its own accord. Folding it
/// into a guard would make that guard fire **never**, and it would look correct on screen — so it
/// is not a field here, and the countdown is driven by the heartbeat exemption instead
/// (`GamePlayScreen::do_item_heartbeat`, which `Hud::drive` calls outside every guarded pass).
///
/// **It is one type rather than two copies deliberately.** `InventoryPanels` and `ShortcutBar`
/// both need it, and separate structs, readers and matchers would be six places for three
/// fields with the plural-name mapping written out four times. A second copy that omits a field
/// **looks exactly as fixed**. With one copy the two panels cannot drift apart.
///
/// `Eq` is not on this derive because [`crate::view::SlotDecoration`] carries
/// `cooldown_duration`, an `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TileInfo {
    /// Every field of it — stack size, structure, icon overlay, icon underlay, effects and the
    /// rest — because `decorate` reads every field of it.
    pub decoration: crate::view::SlotDecoration,
    /// The object's name, which the tooltip update draws.
    pub name: String,
    /// The object's plural name, mapped by [`plural_name_of`] — the name a stack above one shows.
    pub plural_name: Option<String>,
}

impl TileInfo {
    /// Read one slot's frozen-world inputs off the seam.
    ///
    /// `None` is an id with no live client object yet — the pre-placement window that `0x0022
    /// Item_ServerSaysContainID` opens. The item-attributes-changed edge exists to catch the
    /// transition once that object becomes live.
    ///
    /// This is the single read both a guard's memory and its fill are built from, so the value
    /// that decided to rebuild and the value that reaches the screen cannot disagree within a
    /// frame (the same rule the capacity pass follows, applied to the decoration itself).
    #[must_use]
    pub fn read(view: &dyn crate::view::GameView, id: ObjectId) -> Option<Self> {
        Some(Self {
            decoration: view.slot_decoration(id)?,
            name: view.name(id).unwrap_or_default().to_string(),
            plural_name: plural_name_of(view, id).map(str::to_string),
        })
    }

    /// Whether a remembered tile is still exactly what the view answers for `id`.
    ///
    /// Asked field by field against the view rather than by building a fresh [`TileInfo`], so a
    /// frame on which nothing moved allocates no `String` at all: the clone happens only on a
    /// frame that rebuilds. That is a second reason for preferring a whole-value comparison
    /// to a wider tuple.
    ///
    /// `last` is `Option` because the absence of a tile is itself a compared state: a slot that
    /// held nothing and still holds nothing has not moved.
    #[must_use]
    pub fn matches(view: &dyn crate::view::GameView, id: ObjectId, last: Option<&Self>) -> bool {
        let Some(d) = view.slot_decoration(id) else {
            return last.is_none();
        };
        let Some(t) = last else { return false };
        t.decoration == d
            && t.name == view.name(id).unwrap_or_default()
            && t.plural_name.as_deref() == plural_name_of(view, id)
    }

    /// The [`SlotInfo`] `decorate` wants, with the one clock-driven field supplied fresh.
    ///
    /// The three frozen-world fields come **out of the compared value**, not off the view a
    /// second time; only `cooldown_remaining` is read again, because it has to be.
    /// The fresh read is not the countdown mechanism — that is the heartbeat — it is the value
    /// the slot update's own tail would have used on a frame that really did rebuild, so a slot
    /// refilled mid-cooldown draws the right wedge at once rather than at the next beat.
    #[must_use]
    pub fn to_slot_info(&self, view: &dyn crate::view::GameView, now: f64) -> SlotInfo {
        SlotInfo {
            cooldown_remaining: (self.decoration.cooldown_id > 0)
                .then(|| view.cooldown_remaining(self.decoration.cooldown_id, now))
                .flatten(),
            decoration: self.decoration,
            name: self.name.clone(),
            plural_name: self.plural_name.clone(),
        }
    }
}

/// One live `ItemListWidget` — type `0x10000031`.
#[derive(Debug, Clone)]
pub struct ItemListWidget {
    pub element: ElementId,
    pub handle: ElemHandle,
    /// [`attr::ITEM_SLOT_ID`].
    pub slot_id: ElementId,
    /// Whether this list shows side packs ([`attr::IS_CONTAINER`]).
    pub container_list: bool,
    /// Whether this is a shortcut list ([`attr::IS_SHORTCUT`]).
    pub shortcut_list: bool,
    /// Whether this is a vendor list — [`attr::IS_VENDOR`]. Read by the drag-icon preparation and by the
    /// three-way `!vendor && !salvage && !shortcut` test the ghost is gated on.
    pub vendor_list: bool,
    /// Whether this is a salvage list — [`attr::IS_SALVAGE`]. See [`Self::vendor_list`].
    pub salvage_list: bool,
    /// `UI_ItemList_AllowDragging` — [`attr::ALLOW_DRAGGING`]. The whole body of
    /// [`Self::begin_drag`] runs only when attribute `0x10000016` is true, so a list
    /// without it can be dropped **on** and never dragged **from**.
    pub allow_dragging: bool,
    /// Bit 0 of the list box's flags — [`attr::HORIZONTAL`].
    pub horizontal: bool,
    /// [`attr::AT_LEAST_ONE_EMPTY`].
    pub at_least_one_empty: bool,
    /// The single-selection flag — [`attr::SINGLE_SELECTION`]. True on one shipped list
    /// (the vendor's); see the attribute's own note for the measurement.
    pub single_selection: bool,
    /// Which of *this* list's items is the open container.
    ///
    /// **Every `ItemListWidget` carries its own**, and the client never shares one: a click on
    /// the side-pack strip writes the strip's, a click on the main-pack slot writes
    /// the top-container list's, and [`Self::update_open_container_indicator`] tests the one
    /// belonging to the list it is called on. It is not `InventoryPanels::open_container`, which
    /// is the *grid's* parent container and a different quantity.
    pub open_item_id: Option<ObjectId>,
    /// The column count's source. `-1` means "one row of everything".
    ///
    /// The `-1` default below is **not** the client layout pass's: the client initialises its local to 0
    /// and an absent `0x5F` therefore reads 0, which takes the `max(v, 1)` arm and means *one
    /// column*. It makes no difference here — every one of the forty-odd `ItemListWidget`s in
    /// the shipped gameplay tree carries `0x5F` explicitly, so the default is unreachable
    /// — but a plain `ListBox` often
    /// carries none, which is why [`crate::panels::listbox::ListBoxWidget`] defaults to 0.
    pub max_columns: i32,
    /// The live fixed list size; the container-list-size update rewrites it.
    pub fixed_list_size: i32,
    /// The cell width and height, measured off the first slot the list created.
    pub cell: (i32, i32),
    /// The list's items, in list order.
    pub slots: Vec<ItemSlot>,
    /// The slot cache: retiring a slot pushes at the tail and
    /// the client reuses from the head. Cached slots are not scroll rows.
    pub cached_slots: std::collections::VecDeque<ItemSlot>,
    /// The container whose contents this list shows.
    pub parent_container: Option<ObjectId>,
    /// How many slot creations this list has made. Counted because a list that
    /// silently creates nothing leaves an empty panel with no other symptom.
    pub created: u32,
    /// Slot creations that produced no element — a layout miss.
    pub create_failures: u32,
    /// Slots whose drag icon could not be created. Non-zero means nothing in this list can be
    /// picked up, so it is a number.
    pub drag_icon_failures: u32,
}

impl ItemListWidget {
    /// The item list's initialisation: read the attributes, then create the initial
    /// slots — one when `FixedListSize < 0` (just to measure the cell size), or exactly
    /// `FixedListSize` of them — and flush.
    pub fn init(ui: &mut UiSystem, handle: ElemHandle) -> Self {
        let element = ui
            .node(handle)
            .map_or(ElementId(0), dereth_ui::ElementNode::element_id);
        let mut w = Self {
            element,
            handle,
            slot_id: ElementId(attr_enum(ui, handle, attr::ITEM_SLOT_ID).unwrap_or(0)),
            container_list: attr_bool(ui, handle, attr::IS_CONTAINER).unwrap_or(false),
            shortcut_list: attr_bool(ui, handle, attr::IS_SHORTCUT).unwrap_or(false),
            vendor_list: attr_bool(ui, handle, attr::IS_VENDOR).unwrap_or(false),
            salvage_list: attr_bool(ui, handle, attr::IS_SALVAGE).unwrap_or(false),
            allow_dragging: attr_bool(ui, handle, attr::ALLOW_DRAGGING).unwrap_or(false),
            horizontal: attr_bool(ui, handle, attr::HORIZONTAL).unwrap_or(false),
            at_least_one_empty: attr_bool(ui, handle, attr::AT_LEAST_ONE_EMPTY).unwrap_or(false),
            // Attribute `0x10000052` — the fifth of the client's nine boolean reads.
            single_selection: attr_bool(ui, handle, attr::SINGLE_SELECTION).unwrap_or(false),
            open_item_id: None,
            max_columns: attr_int(ui, handle, attr::MAX_COLUMNS).unwrap_or(-1),
            fixed_list_size: attr_int(ui, handle, attr::FIXED_LIST_SIZE).unwrap_or(-1),
            cell: (0, 0),
            slots: Vec::new(),
            cached_slots: std::collections::VecDeque::new(),
            parent_container: None,
            created: 0,
            create_failures: 0,
            drag_icon_failures: 0,
        };
        let n = if w.fixed_list_size < 0 {
            1
        } else {
            w.fixed_list_size
        };
        for _ in 0..n {
            w.add_slot(ui);
        }
        w.flush(ui);
        w.update_layout(ui);
        w
    }

    /// Create one slot, plus the add-item that every caller pairs it with.
    ///
    /// The client keeps a free list ([`Self::cached_slots`]) and reuses a slot element before it
    /// creates one. Retired slots are hidden and retained in the same FIFO order, so repeated
    /// refills reuse their existing trees and do not allocate another cache on every filter.
    fn add_slot(&mut self, ui: &mut UiSystem) {
        if let Some(mut s) = self.cached_slots.pop_front() {
            // The client's cache hit: empty slot and no drag-accept overlay.
            s.clear(ui);
            s.set_drag_accept_state(ui, drag_accept_state::NONE);
            ui.set_visible(s.handle, true);
            self.slots.push(s);
            return;
        }
        let r = ui.require_env().and_then(|e| {
            e.create_child_element_by_enum(ui, self.handle, ITEM_SLOT_LAYOUT, self.slot_id)
        });
        match r {
            Ok(h) => {
                self.created += 1;
                let mut s = ItemSlot::bind(ui, h);
                // The drag icon is created from the
                // **same** `ItemSlot` layout under element id `0x10000345`. It carries
                // `0x3A = true` and is 32×32,
                // which is what makes starting a drag on it at offset (16, 16) find a draggable
                // element at once and centre it on the pointer.
                //
                // **The slot is its parent**, not a second root of that layout. The client passes element id
                // `0x10000345`, the layout, and the item slot as the parent to child creation. The
                // new drag icon therefore reports the slot as its parent, exactly as it does here,
                // and walking from the icon to ancestor type `0x10000031`
                // from the icon finds the owning list. The next two steps hide the icon and make
                // the slot mouse-visible, which is what [`ItemSlot::post_init`] does.
                let drag_icon = ui
                    .require_env()
                    .and_then(|e| {
                        e.create_child_element_by_enum(
                            ui,
                            h,
                            ITEM_SLOT_LAYOUT,
                            ElementId(child::DRAG_ICON),
                        )
                    })
                    .ok();
                if drag_icon.is_none() {
                    self.drag_icon_failures += 1;
                }
                s.post_init(ui, drag_icon);
                s.rest(ui);
                let b = ui.node(h).map(|n| n.region.box_);
                if let Some(b) = b {
                    if b.width() > 0 && b.height() > 0 {
                        self.cell = (b.width(), b.height());
                    }
                }
                s.clear(ui);
                self.slots.push(s);
            }
            Err(_) => self.create_failures += 1,
        }
    }

    /// Every slot back to empty.
    pub fn flush(&mut self, ui: &mut UiSystem) {
        for s in &mut self.slots {
            s.clear(ui);
        }
    }

    /// Grow to `FixedListSize`, or shrink to
    /// it. Runs only while `FixedListSize != -1`.
    pub fn update_fixed_slots(&mut self, ui: &mut UiSystem) {
        if self.fixed_list_size < 0 {
            return;
        }
        let want = usize::try_from(self.fixed_list_size).unwrap_or(0);
        while self.slots.len() < want {
            let before = self.slots.len();
            self.add_slot(ui);
            if self.slots.len() == before {
                break; // the layout refused; do not spin
            }
        }
        while self.slots.len() > want {
            if let Some(mut s) = self.slots.pop() {
                s.clear(ui);
                ui.set_visible(s.handle, false);
                self.cached_slots.push_back(s);
            }
        }
    }

    /// The `FixedListSize == -1` arm: fill the
    /// list's own width with cells, and honour `AtLeastOneEmptySlot`.
    ///
    /// The `max_columns == -1` arm pads in width and removes trailing empty cells beyond
    /// the viewport. Otherwise `max_rows == -1` removes every trailing empty slot.
    pub fn update_empty_slots(&mut self, ui: &mut UiSystem) {
        if self.fixed_list_size != -1 {
            return;
        }
        if !ui.node(self.handle).is_some_and(|n| n.region.flags.visible) {
            return;
        }
        let (w, cell_w) = {
            let b = ui
                .node(self.handle)
                .map(|n| n.region.box_)
                .unwrap_or_default();
            (b.width(), self.cell.0)
        };
        if cell_w <= 0 {
            return;
        }
        if self.max_columns == -1 {
            let used = i32::try_from(self.slots.len()).unwrap_or(0) * cell_w;
            if used < w {
                for _ in 0..((w - used) / cell_w) {
                    self.add_slot(ui);
                }
            } else if used > w {
                // The empty-slot update: ceil((used - width) / cell width), stopping
                // at the first occupied slot. Cached elements remain hidden, outside the list.
                for _ in 0..((used - w + cell_w - 1) / cell_w) {
                    if !self.remove_trailing_empty(ui) {
                        break;
                    }
                }
            }
            if self.at_least_one_empty && self.slots.last().is_some_and(|s| s.item.is_some()) {
                self.add_slot(ui);
            }
        } else if attr_int(ui, self.handle, 0x60) == Some(-1) {
            while self.remove_trailing_empty(ui) {}
        }
    }

    fn remove_trailing_empty(&mut self, ui: &mut UiSystem) -> bool {
        if !self
            .slots
            .last()
            .is_some_and(|s| s.item.is_none() && s.spell.is_none())
        {
            return false;
        }
        if let Some(s) = self.slots.pop() {
            ui.set_visible(s.handle, false);
            self.cached_slots.push_back(s);
        }
        true
    }

    /// The list box's layout update, for a list of equal cells.
    ///
    /// The column count is `clamp(attribute 0x5F, 1, items)`, or `items` when the
    /// attribute is negative — in which case the row count is `items != 0`, i.e. one row. The
    /// row count is otherwise `ceil(items / cols)`. The fill order then depends on bit 0 of
    /// the flags: **horizontal fills row-major across the columns, and the default fills
    /// column-major down the rows.**
    pub fn update_layout(&mut self, ui: &mut UiSystem) {
        // The item list element's internal create item / the item insert:
        // the active list and hidden cache are separate arrays. Publish only active slots to
        // the inherited behavior; cached children must not extend the scrollable paper.
        if let Some(l) = ui.node_mut(self.handle).and_then(|n| {
            n.behaviour
                .as_mut()?
                .as_any_mut()?
                .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
        }) {
            let selected = l.selected.and_then(|i| l.items.get(i).copied());
            l.items = self.slots.iter().map(|s| s.handle).collect();
            l.items_are_authoritative = true;
            l.selected = selected.and_then(|h| l.items.iter().position(|i| *i == h));
        }
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        let (cols, rows) = if self.max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = self.max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        };
        let (cw, ch) = self.cell;
        let (mut x, mut y) = (0, 0);
        let (mut col, mut row) = (0, 0);
        for s in &self.slots {
            ui.move_to(s.handle, x, y);
            if self.horizontal {
                if col == cols - 1 {
                    x = 0;
                    col = 0;
                    y += ch;
                    row += 1;
                } else {
                    x += cw;
                    col += 1;
                }
            } else if row == rows - 1 {
                y = 0;
                row = 0;
                x += cw;
                col += 1;
            } else {
                y += ch;
                row += 1;
            }
        }
        let _ = col;
        // The client's layout pass ends by resizing the scrollable area and places rows at
        // their grid origin minus the scroll offset, in the same pass.
        dereth_ui::widgets::listbox::refresh_scroll_of(ui, self.handle);
    }

    /// The inherited Scrollable offsets, shared with scrollbar and wheel delivery.
    #[must_use]
    pub fn scroll(&self, ui: &UiSystem) -> (i32, i32) {
        dereth_ui::widgets::listbox::scroll_offset_of(ui, self.handle).unwrap_or((0, 0))
    }

    /// Scroll-to-show aligns the row's origin, then clamps it. This is not
    /// [`Self::scroll_to_view`]'s minimal movement.
    pub fn scroll_to_show(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        if index >= self.slots.len() {
            return false;
        }
        self.update_layout(ui);
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        let (cols, rows) = self.grid(n);
        let i = i32::try_from(index).unwrap_or(0);
        let (col, row) = if self.horizontal {
            (i % cols, i / cols)
        } else {
            (i / rows, i % rows)
        };
        dereth_ui::widgets::listbox::set_scroll_offset(
            ui,
            self.handle,
            col * self.cell.0,
            row * self.cell.1,
        )
    }

    /// The list box's scroll-to-view, which retail calls from the spellbook and
    /// spell bar's selection handlers. Unlike [`Self::scroll_to_show`], a fully visible item does
    /// not move.
    pub fn scroll_to_view(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        let Some(item) = self.slots.get(index).map(|s| s.handle) else {
            return false;
        };
        self.update_layout(ui);
        dereth_ui::widgets::listbox::scroll_item_to_view(ui, self.handle, item)
    }

    /// The item list set parent container + the item list update container list size +
    /// the loop, folded into one pass over the ids the server says
    /// this container holds.
    ///
    /// `capacity` is the container's items capacity or containers capacity depending on
    /// [`Self::container_list`], which is exactly what the client writes into
    /// `UI_ItemList_FixedListSize` before calling [`Self::update_fixed_slots`]. A negative
    /// capacity means "unbounded", which is the [`Self::update_empty_slots`] arm.
    ///
    /// Returns how many slots ended up holding an object.
    pub fn set_contents(
        &mut self,
        ui: &mut UiSystem,
        container: Option<ObjectId>,
        capacity: Option<i32>,
        ids: &[ObjectId],
        icon: &dyn Fn(ObjectId) -> Option<DataId>,
    ) -> usize {
        self.parent_container = container;
        if let Some(c) = capacity {
            self.fixed_list_size = c;
        }
        if self.fixed_list_size < 0 {
            // Grow enough to hold everything before the width-driven padding runs.
            while self.slots.len() < ids.len() {
                let before = self.slots.len();
                self.add_slot(ui);
                if self.slots.len() == before {
                    break;
                }
            }
        } else {
            self.update_fixed_slots(ui);
        }
        self.flush(ui);
        let mut filled = 0;
        for (i, id) in ids.iter().enumerate() {
            // The client's single-selection arm: with single selection on, a slot is selectable
            // only when the list does not already hold its id, evaluated **before** the slot is
            // initialised and therefore against the list as it stood a moment ago.
            // A second slot for an id this list already holds arrives *unselectable*, which is
            // what stops a vendor's duplicate rows taking two rings. The fill here
            // is a batch, so "as it stood a moment ago" is the ids already placed in this loop.
            let selectable = !self.single_selection || !ids[..i].contains(id);
            let Some(s) = self.slots.get_mut(i) else {
                break;
            };
            s.item = Some(*id);
            s.spell = None;
            super::super::runtime::set_identity(ui, s.handle, *id, 0);
            s.set_selectable_state(selectable);
            // The item list's add-item order: initialise the slot, set state `0x1000001D`,
            // then the slot update sets the icon. **The state goes first**, because
            // `0x1000001C`'s media step would otherwise overwrite the icon with the empty frame.
            s.set_state(ui, item_state::OCCUPIED);
            s.set_icon(ui, icon(*id));
            ui.set_visible(s.handle, true);
            filled += 1;
        }
        self.update_empty_slots(ui);
        self.update_layout(ui);
        filled
    }

    /// The tail of the slot update — everything after the icon, run
    /// once over every slot that holds an object.
    ///
    /// The slot update's order, and it is the order here:
    ///
    /// ```text
    /// is-container = (bitfield & 0x800000) || items capacity || containers capacity
    /// capacity display
    /// structure display
    /// quantity display
    /// cooldown display
    /// … waiting, shortcut, sell and trade state …
    /// tooltip
    /// ```
    ///
    /// The order below is the slot update's own, step for
    /// step, and it is the order because two of the arms are edge-triggered: the ring and
    /// the two markers act only when their cached copy differs from the weenie's, so running them
    /// out of order relative to the state changes above them draws the wrong thing once and then
    /// never corrects it.
    ///
    /// Three of the slot update's writes are deliberately **not** wired to anything that draws,
    /// because they are not wired to anything in the client either: `effects` is copied
    /// and read by nothing here, and the is-openable and holds-containers flags are
    /// assigned in the constructor and in this function and **read by nothing else in the
    /// client**. They are computed anyway, because a later window may want them; the
    /// open-container *frame* is a different mechanism entirely
    /// ([`Self::update_open_container_indicator`]).
    ///
    /// A slot the callback answers `None` for is left exactly as it was, which is the client's
    /// no-object early-out in all four display updates. Returns how many slots
    /// were decorated.
    pub fn decorate(
        &mut self,
        ui: &mut UiSystem,
        info: &dyn Fn(ObjectId) -> Option<SlotInfo>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let Some(id) = s.item else { continue };
            let Some(i) = info(id) else { continue };
            let d = i.decoration;
            // All six
            // blits, as the one composite the object's icon is. In the client this is inside
            // the set-icon's single image write, so it runs at the head of the slot update
            // rather than in this tail; it is here because [`Self::set_contents`]'s fill loop is
            // handed an icon `DataId` and has none of the other four fields to resolve a recipe
            // from. The observable result is the same for the reason given for the rest of
            // this pass: the fill is a batch and this is a second pass over the very same slots,
            // before the frame is drawn.
            s.set_icon_composite(ui, Some(object_recipe(ui, &d)));
            // When the slot's copy differs from the object's (and it is a clear, or the slot is
            // selectable) — the edge, then [`ItemSlot::set_selected_state`]'s own body.
            if s.selected != d.selected {
                s.set_selected_state(ui, d.selected);
            }
            // openable = (is container && (bitfield & BF_OPENABLE)) || item is the player.
            // The player's own slot is openable regardless of its container flags.
            s.is_openable = (d.is_container && d.openable) || d.is_player;
            s.is_container = d.is_container;
            s.is_container_holder = d.containers_capacity != 0;
            // Mirror the pair the capacity display is about to consume, so
            // that [`Self::drag_over`]'s empty-slot count can be answered from a message
            // handler. Written *before* the call for no reason other than reading order; the
            // call does not change them.
            s.items_capacity = d.items_capacity;
            s.contained_items = d.contained_items;
            s.update_capacity_display(ui, d.items_capacity, d.contained_items);
            s.update_structure_display(ui, d.structure, d.max_structure);
            s.update_quantity_display(ui);
            s.cooldown_id = d.cooldown_id;
            s.cooldown_duration = d.cooldown_duration;
            s.update_cooldown_display(ui, i.cooldown_remaining);
            // **The busy / in-use overlay, and it is the slot update's own next
            // block, in its own place.** When the object's waiting state differs from the slot's,
            // the slot copies it and runs its waiting-state setter, which writes it onto the live
            // object and (unless the slot is unghostable) shows or hides the ghost.
            //
            // Edge-triggered, like the ring and the two markers around it, and for the same
            // reason: the client's waiting-state write sets the object as well as the element, and
            // this crate must not write the object — so the compare is what keeps the
            // call from firing on a frame where nothing moved.
            //
            // **This is the one mirror, and it must stay the only one.** A second mechanism that
            // walks only some lists (say the item list and the doll) leaves the container list and
            // the top-container list out, so a ghosted **backpack** has no route back: the object's
            // flag goes down (the server's attempt-failed or move-item notice, or the tile's own
            // `0x15` arm) and the element stays grey for the session. This runs for every slot of
            // every list this function is called on.
            if s.waiting != d.waiting {
                s.set_waiting(ui, d.waiting);
            }
            // The slot update's own next two blocks, in its own order: a delayed
            // number other than `-1` is written onto the object (not ghosted) and reset to `-1`;
            // then, when the slot's number or ghosted flag differs from the object's, the slot's
            // numeral is rewritten from the object.
            //
            // The two collapse into one call here, and they may: nothing writes the object's
            // shortcut number between them, and the first write sets the object's ghosted flag
            // to `false` from its literal argument — so the pair is exactly a slot numeral write
            // of `(n, false)`. **The write into the object table is the half
            // this crate never makes**, and it is not needed: this build keeps the numeral on
            // the widget (`ShortcutBar::update`) rather than round-tripping it through
            // live object, so the delayed number lands where the direct one does.
            //
            // Reaching this line *is* the mechanism: the slot update returns early when the
            // slot has no object, so a shortcut naming an object the client has not seen sets the
            // delayed number and paints nothing until the object arrives — which is the first
            // pass where `info(id)` answers `Some`, i.e. this one.
            if s.delayed_shortcut_num != -1 {
                let n = s.delayed_shortcut_num;
                s.delayed_shortcut_num = -1;
                s.set_shortcut_num(ui, n, false);
            }
            // **This block follows the delayed update and makes the quickbar numeral
            // appear on the paper doll and in the
            // packs as well as in the bar slot.** When the slot's stored number or ghosted flag
            // differs from the object's, the object's number and flag are written to the slot.
            //
            // **The assignment lives on the object, not on the bar.** The shortcut-bar insert
            // writes `(slot, not ghosted)` onto the object, its removal writes `-1`, and the
            // toolbar flips the ghosted flag through the bar's own
            // tiles into the object. Thus every tile anywhere in
            // the tree learns the assignment from the same place, and this compare is how.
            // Without it the numeral is painted only by
            // [`crate::toolbar::shortcuts::ShortcutBar::update`] on the bar's own tile, and equipped
            // or backpacked copies of the item show no number.
            //
            // The delayed block above writes the **object**, while this implementation
            // is therefore read back by this compare on the same pass; here it writes the widget
            // directly, and this compare answers the same number for it, because the host
            // resolves both from the player module's one shortcut array.
            //
            // [`crate::view::SlotDecoration::shortcut_num`] is `Option<u32>` where the client's
            // field is an `i32` whose "none" is `-1`; the mapping is here, at the one call that
            // needs the client's spelling.
            let want_num = d
                .shortcut_num
                .map_or(-1, |n| i32::try_from(n).unwrap_or(-1));
            if s.shortcut_num != want_num || s.shortcut_ghosted != d.shortcut_ghosted {
                s.set_shortcut_num(ui, want_num, d.shortcut_ghosted);
            }
            // The two markers are the tail.
            s.set_sell_state(ui, d.sell_state);
            s.set_trade_state(ui, d.trade_state);
            s.update_tooltip(ui, &i.name, i.plural_name.as_deref(), d.stack_size);
            n += 1;
        }
        n
    }

    /// **The frame on the open
    /// pack.**
    ///
    /// The client walks every positional entry, keeps only item slots, and shows the open-container
    /// frame exactly on the slot whose item id equals the nonzero open id.
    ///
    /// It is called from five places, all of which mean that the open container changed or was
    /// forgotten. Setting the parent list and flushing the list pass **0**. Opening a named
    /// container and opening the first container pass the new open item id; the click path passes
    /// the container selected by that click.
    ///
    /// **Only the two `UI_ItemList_IsContainer` lists can show anything**, because only their slot
    /// root `0x1000033F` has the child — see [`ItemSlot::open_container`]. Running it over the
    /// grid is harmless and is what the client does.
    ///
    /// **The drop hint**, and the whole of its state machine.
    ///
    /// Reached from the slot's element-message `0x3E` arm: the slot under the pointer walks up to
    /// its `ItemListWidget` (type `0x10000031`) and asks *it* what hint to show, so the decision
    /// belongs to the list and the drawing to the slot.
    ///
    /// In the client's own test order: read the drag proxy's item, spell and flags; a registered
    /// drag handler that answers true ends it; a vendor, salvage or shortcut list ends it; no item
    /// ends it; a vendor/shortcut/salvage drag (`flags & 0xE`) ends it. Then a dragged **pack**
    /// gets `0x10000040` on a container list and `0x10000041` elsewhere; a plain item gets
    /// `0x10000040` on a non-container list; and on a container list it gets `0x10000046` when
    /// the slot under it is filled and that object has a non-zero empty-slot count, else
    /// `0x10000041`.
    ///
    /// Three things worth stating because each is invisible on screen if it is wrong:
    ///
    /// * **The legal/illegal pair is decided here and nowhere else.** A pack dragged onto the item
    ///   grid and a plain item dragged onto an empty side-pack slot both get `REFUSE`, and both
    ///   are drops the drag-accept test really does refuse ("Cannot place container in
    ///   item list" / "Cannot place item in container list"). Showing `ACCEPT` over either would
    ///   be one small sprite's difference and would tell the player the drop will work.
    /// * **`INTO_CONTAINER` is a third answer, not a second `ACCEPT`.** It is reached only over a
    ///   *filled* slot of a container list whose object still has room, which is exactly the drop
    ///   the drop acceptance re-targets from the list's own container to that pack.
    /// * **The empty-slot count returns `-1` for an unbounded pack** (items capacity
    ///   `-1`), and `-1 != 0`, so unbounded reads as *has room*. `free_slots` keeps
    ///   the same signed convention.
    ///
    /// **The drag handler is the caller's, not this function's.** The first arm is answered before
    /// this is reached, by whichever owner registered a handler on the list: the vendor's sell
    /// list (the vendor sell page), the eighteen shortcut lists
    /// (the toolbar's item-list drag-over) and the twenty-four doll lists
    /// (the paper doll's own). Each of
    /// those handlers returns true on every path, so this default never runs for their lists —
    /// which is why the `shortcut_list` early-out below is unreachable in retail rather than
    /// wrong.
    ///
    /// `slot` is the index of the slot the pointer is over. Returns the state that was chosen, or
    /// `None` when one of the early-outs fired — which is *not* the same as choosing
    /// [`drag_accept_state::NONE`], and the caller must be able to tell them apart.
    pub fn drag_over(
        &mut self,
        ui: &mut UiSystem,
        slot: usize,
        info: DropIconInfo,
        free_slots: &dyn Fn(ObjectId) -> Option<i64>,
    ) -> Option<StateId> {
        if self.vendor_list || self.salvage_list || self.shortcut_list {
            return None;
        }
        info.item?;
        if !info.is_inventory_move() {
            return None;
        }
        let dragging_a_container = info.flags & drag_flags::IS_CONTAINER != 0;
        let s = if dragging_a_container {
            if self.container_list {
                drag_accept_state::ACCEPT
            } else {
                drag_accept_state::REFUSE
            }
        } else if !self.container_list {
            drag_accept_state::ACCEPT
        } else {
            // The container strip, with a plain item over it. A slot state other than `0x1000001C`
            // is "this slot holds something"; the mirror of that here is `item.is_some()`.
            let under = self.slots.get(slot).and_then(|s| s.item);
            match under.and_then(free_slots) {
                Some(n) if n != 0 => drag_accept_state::INTO_CONTAINER,
                _ => drag_accept_state::REFUSE,
            }
        };
        self.slots.get_mut(slot)?.set_drag_accept_state(ui, s);
        Some(s)
    }

    /// Returns how many slots' frames actually changed.
    pub fn update_open_container_indicator(
        &mut self,
        ui: &mut UiSystem,
        open: Option<ObjectId>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let on = open.is_some() && s.item == open;
            if s.set_open_container_state(ui, on) {
                n += 1;
            }
        }
        n
    }

    /// How many slots are **not** in state
    /// `0x1000001C`.
    ///
    /// It is not the length of the list's items, and the difference is load-bearing:
    /// [`Self::handle_single_selection`] uses this as a **loop bound over positional indices**, so
    /// a list whose filled slots do not sit at the front of the array is walked short. That is the
    /// client's own arithmetic and it is reproduced rather than tidied — the item-list insert
    /// fills from the first free slot, so in practice the filled ones are contiguous
    /// and at the front.
    ///
    /// **Its second reader is the one that decides where a drop lands.**
    /// The drop clamps the slot index under the pointer to this count, which is what makes a drop on the *fourth* empty slot
    /// of a five-item pack land at the end of the list rather than at position 8.
    #[must_use]
    pub fn num_ui_items(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.item.is_some() || s.spell.is_some())
            .count()
    }

    /// Does **this** list hold `id`?
    ///
    /// One list, not the screen: the whole single-selection rule is about the same object id
    /// occupying two slots of the *same* `ItemListWidget` (a vendor listing the same wcid
    /// twice), never about two different lists.
    #[must_use]
    pub fn is_in_list(&self, id: ObjectId) -> bool {
        self.slots.iter().any(|s| s.item == Some(id))
    }

    /// **The click's deselect walk.**
    ///
    /// The client walks positional entries in order. For every other item slot with the clicked
    /// item id, it clears selectable state and then selected state. It then enables selectable
    /// state on the clicked slot and marks that slot selected.
    ///
    /// Two details the names do not give away, both observed in the retail client:
    ///
    /// * **the order is selectable-then-selected in both halves, and it is load-bearing in the
    ///   *set* half only.** The client's gate lets a clear through always and a set only when the
    ///   slot is selectable, so it covers the set and not the clear. So the clear half works in **either** order — a
    ///   slot just made unselectable can still be deselected, and a slot deselected before being
    ///   made unselectable ends in the same state. The set half cannot: a slot an earlier walk
    ///   made unselectable takes its ring back only because making it selectable runs
    ///   *first*, and swapping those two lines is what leaves a clicked item unringed.
    ///
    ///   [a mutation that swaps the clear half's order survives, and that is the explanation
    ///   rather than a gap in the test; the clear-half swap is kept in the harness as a deliberate
    ///   no-op probe and is expected to survive.]
    /// * **the comparison is on the item id alone and is not guarded against zero here.** The guard
    ///   lives in the caller (the element-message handler's "slot has an item" test), so this
    ///   function is
    ///   faithful only if it also compares empty against empty — which it does, `None == None`.
    ///
    /// `clicked` is a positional index into [`Self::slots`]. Returns how many *other* slots were
    /// deselected, which is 0 on every shipped list except the vendor's — see
    /// [`attr::SINGLE_SELECTION`].
    pub fn handle_single_selection(&mut self, ui: &mut UiSystem, clicked: usize) -> usize {
        let Some(id) = self.slots.get(clicked).map(|s| s.item) else {
            return 0;
        };
        let n = self.num_ui_items();
        let mut cleared = 0;
        for i in 0..n {
            if i == clicked {
                continue;
            }
            let Some(s) = self.slots.get_mut(i) else {
                continue;
            };
            if s.item != id {
                continue;
            }
            s.set_selectable_state(false);
            s.set_selected_state(ui, false);
            cleared += 1;
        }
        if let Some(s) = self.slots.get_mut(clicked) {
            s.set_selectable_state(true);
            s.set_selected_state(ui, true);
        }
        cleared
    }

    /// Re-open the first container in this item list, and a caller of
    /// [`Self::update_open_container_indicator`].
    ///
    /// The client examines array entry 0, rather than searching for the first occupied slot. A
    /// non-item entry clears the child list's parent container to zero and returns. An item with id
    /// zero is a no-op. Otherwise it returns when the id is already open or is no longer in this
    /// list; for a new valid id it links the child list to this list, sets that child's parent
    /// container, scrolls a nonempty child list to entry 0, stores the open id, and updates the frame.
    ///
    /// **Its two callers are the inventory and external-container move-item notice
    /// handlers**, not the click path;
    /// "the parent container changed" is a consequence rather than the trigger. The inventory arm compares the moved object with the
    /// 3D item grid's parent-container id. When they match and the new container is not the
    /// backpack list's parent container, it asks the top-container list to reopen its first item.
    ///
    /// I.e. **the pack the grid is showing has itself been moved somewhere that is not the
    /// player**, so the grid is pointing at a container that has left the inventory, and the
    /// main-pack list (whose one slot is the player) re-opens the player's own items.
    /// The top-container and container lists both receive the grid item list as their child list
    /// when the player-description notice arrives.
    ///
    /// Everything this function does to `this` happens here — the open item id and the frame. The
    /// child-list half is returned for the caller, because in this build the child list is a
    /// sibling field of the panel and not a pointer on the widget.
    pub fn open_first_container(&mut self, ui: &mut UiSystem) -> OpenFirstContainer {
        let Some(first) = self.slots.first() else {
            // Entry 0 of the list is not a `UiItemWidget`.
            return OpenFirstContainer::ClearChild;
        };
        let Some(id) = first.item else {
            return OpenFirstContainer::Unchanged;
        };
        if self.open_item_id == Some(id) {
            return OpenFirstContainer::Unchanged;
        }
        if !self.is_in_list(id) {
            return OpenFirstContainer::Unchanged;
        }
        self.open_item_id = Some(id);
        self.update_open_container_indicator(ui, Some(id));
        OpenFirstContainer::Open(id)
    }

    /// Move the ring from `old` to
    /// `new` across this list.
    ///
    /// For each item slot that is not a spell slot (a spell slot has no ring): deselect it when it
    /// holds `old`, then select it when it holds a non-zero `new`.
    ///
    /// Every `ItemListWidget` in the client registers for the selected-item notice in
    /// its post-init and reaches this through the set-selected-item notice, so
    /// a selection change moves the ring in **every** list at once — the backpack grid, the two
    /// container strips, the paper doll and all eighteen quickbar tiles. That is why the caller
    /// side of this is a single edge-triggered pass over every live list and not a per-panel
    /// refresh.
    ///
    /// The spell-slot skip is the client's: a spellbook row has no item id, so an `old` of `None`
    /// would otherwise deselect every spell in the book.
    ///
    /// Returns how many slots were written.
    pub fn set_selected_item(
        &mut self,
        ui: &mut UiSystem,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            if s.spell.is_some() {
                continue;
            }
            if s.item.is_some() && s.item == old {
                s.set_selected_state(ui, false);
                n += 1;
            }
            if new.is_some() && s.item == new {
                s.set_selected_state(ui, true);
                n += 1;
            }
        }
        n
    }

    /// Global message 3 over every slot of this list —
    /// the once-a-second pass that keeps a cooldown wedge moving.
    ///
    /// `remaining` is the player's registry: it is asked once per slot that carries a cooldown id,
    /// and not at all for the rest, which is the cooldown display's own `id > 0` guard.
    ///
    /// Returns how many slots re-ran their display.
    pub fn do_heartbeat(
        &mut self,
        ui: &mut UiSystem,
        now: f64,
        remaining: &dyn Fn(u32) -> Option<f64>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let r = if s.cooldown_id > 0 {
                remaining(s.cooldown_id)
            } else {
                None
            };
            if s.listen_to_global_message(ui, HEARTBEAT_MESSAGE, now, r) {
                n += 1;
            }
        }
        n
    }

    /// The client's fill, folded the same way
    /// [`Self::set_contents`] folds the inventory's.
    ///
    /// The client's version is a flush then one spell-shortcut insert per spell at the sorted
    /// index; each of those adds an empty slot and then the spell shortcut, and adding the spell
    /// shortcut finds the first slot whose state is `0x1000001C`, creating one when
    /// `FixedListSize == -1` and none is free. Feeding an already
    /// ordered list makes the insert index the position, so the loop is a plain fill — the
    /// ordering is [`SpellbookPanel::sorted`](crate::panels::spellbook::SpellbookPanel::sorted)'s.
    ///
    /// **The argument is the whole [`SpellEntry`](crate::view::SpellEntry)**, not
    /// `(id, icon, name)`, because the client's composite needs the power level and the spell's
    /// bitfield as well as the icon.
    ///
    /// Returns how many slots ended up holding a spell.
    pub fn set_spells(&mut self, ui: &mut UiSystem, spells: &[crate::view::SpellEntry]) -> usize {
        // The spell-shortcut add's "create one when the list is unbounded and nothing is free".
        if self.fixed_list_size < 0 {
            while self.slots.len() < spells.len() {
                let before = self.slots.len();
                self.add_slot(ui);
                if self.slots.len() == before {
                    break;
                }
            }
        } else {
            self.update_fixed_slots(ui);
        }
        self.flush(ui);
        let mut filled = 0;
        for (i, e) in spells.iter().enumerate() {
            let Some(s) = self.slots.get_mut(i) else {
                break;
            };
            s.set_spell(
                ui,
                e.id,
                e.icon,
                &e.name,
                Some(spell_recipe(ui, e.icon_power, e.icon, e.bitfield)),
            );
            ui.set_visible(s.handle, true);
            filled += 1;
        }
        self.update_empty_slots(ui);
        self.update_layout(ui);
        filled
    }

    /// The spell in slot `i`, if any.
    #[must_use]
    pub fn spell_at(&self, i: usize) -> Option<u32> {
        self.slots.get(i).and_then(|s| s.spell)
    }

    /// The object in slot `i`, if any.
    #[must_use]
    pub fn item_at(&self, i: usize) -> Option<ObjectId> {
        self.slots.get(i).and_then(|s| s.item)
    }

    /// Which slot a given element handle is, for routing a click or a drop.
    #[must_use]
    pub fn slot_of(&self, h: ElemHandle) -> Option<usize> {
        self.slots.iter().position(|s| s.handle == h)
    }

    /// The ui item element's shortcut num write on slot `i` — see
    /// [`ItemSlot::set_shortcut_num`].
    pub fn set_shortcut_num(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        num: i32,
        ghosted: bool,
    ) -> bool {
        let Some(s) = self.slots.get_mut(i) else {
            return false;
        };
        s.set_shortcut_num(ui, num, ghosted)
    }

    /// Set the waiting state on the slot holding `item` — the ghost an outstanding move leaves.
    pub fn ghost(&mut self, ui: &mut UiSystem, item: ObjectId) -> bool {
        let Some(i) = self.slots.iter().position(|s| s.item == Some(item)) else {
            return false;
        };
        self.slots[i].set_waiting(ui, true);
        true
    }

    /// The UI item element's element-message handler: failed drag completion clears
    /// waiting presentation without refilling/reordering the item's containing list.
    pub fn clear_waiting(&mut self, ui: &mut UiSystem, item: ObjectId) -> bool {
        let Some(i) = self.slots.iter().position(|s| s.item == Some(item)) else {
            return false;
        };
        self.slots[i].set_waiting(ui, false);
        true
    }

    /// The list box element's item index at point read, in **list-local** coordinates.
    ///
    /// An empty list, or a point outside the list's width and height, answers nothing. The point
    /// is offset by the scroll position; the column is the first whose running sum of item
    /// widths reaches `x` (else zero), and the row likewise with heights. The index is
    /// `cols * row + col` for a horizontal list and `rows * col + row` otherwise, and is valid
    /// when below the item count.
    ///
    /// Every cell in an item list is the same cell size, so the two running sums collapse to a
    /// division — and the index arithmetic is [`Self::update_layout`]'s fill order read backwards,
    /// which is why the two are asserted against each other rather than against a written table.
    #[must_use]
    pub fn item_index_at_point(&self, ui: &UiSystem, x: i32, y: i32) -> Option<usize> {
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        if n == 0 {
            return None;
        }
        let b = ui.node(self.handle).map(|n| n.region.box_)?;
        if x < 0 || x >= b.width() || y < 0 || y >= b.height() {
            return None;
        }
        let (cw, ch) = self.cell;
        if cw <= 0 || ch <= 0 {
            return None;
        }
        let (cols, rows) = self.grid(n);
        // The client uses <= at each cumulative boundary: an exact
        // cell edge belongs to the preceding band. Exhausting the array keeps its initial 0.
        let (sx, sy) = self.scroll(ui);
        let band = |point: i32, size: i32, count: i32| {
            let index = (point - 1).max(0) / size;
            if index < count {
                index
            } else {
                0
            }
        };
        let col = band(x + sx, cw, cols);
        let row = band(y + sy, ch, rows);
        let index = if self.horizontal {
            cols * row + col
        } else {
            rows * col + row
        };
        (index >= 0 && index < n).then(|| usize::try_from(index).unwrap_or(0))
    }

    /// The column and row counts, as the layout pass computes them for `n` items.
    fn grid(&self, n: i32) -> (i32, i32) {
        if self.max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = self.max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        }
    }

    /// Start a drag from the slot under the point.
    ///
    /// A list without `AllowDragging` (`0x10000016`) does nothing. Otherwise it finds the slot
    /// under the point (made list-local), prepares that slot's drag icon (and stops if it
    /// cannot), selects the slot's object if it is not already selected, ghosts the slot unless
    /// the list is a vendor, salvage or shortcut list, starts a drag of the drag icon at grab
    /// offset (16, 16), and sends the begin-drag notice (item, spell, list kind) and the item-list
    /// begin-drag notice (list, slot number).
    ///
    /// `x`/`y` are **window** coordinates — the press point, which reaches here as
    /// the element message's window point on element message `0x21`.
    ///
    /// Two things are deliberately not here. The selection of the object is the
    /// object model's; and the two notices tell the rest of the client a drag began.
    /// The vendor, salvage, housing, spellcasting, and toolbar consumers are implemented; the
    /// returned [`DragStart`] carries the values delivered across that host boundary.
    ///
    /// **The ghost goes on at pick-up, not at drop.** That is the client's order and it is visible:
    /// an icon dragged out of the backpack greys immediately, and the refusal clears the
    /// waiting state again when the drop is refused.
    pub fn begin_drag(&mut self, ui: &mut UiSystem, x: i32, y: i32) -> Option<DragStart> {
        if !self.allow_dragging {
            return None;
        }
        let (ox, oy) = ui.screen_origin(self.handle);
        let i = self.item_index_at_point(ui, x - ox, y - oy)?;
        let flags = (
            self.container_list,
            self.vendor_list,
            self.shortcut_list,
            self.salvage_list,
        );
        let slot = self.slots.get(i)?.clone();
        if !slot.prepare_drag_icon(ui, self) {
            return None;
        }
        let ghostable = !flags.1 && !flags.3 && !flags.2;
        if ghostable {
            if let Some(s) = self.slots.get_mut(i) {
                s.set_waiting(ui, true);
            }
        }
        let proxy_source = slot.drag_icon?;
        // **The drag start's result is discarded, and that is retail.**
        // The retail client never looks at it; both notices then go out unconditionally.
        // Returning early on a refused start would suppress them.
        //
        // Nothing a player can do reaches the refusal. Every gesture-dependent arm rejects a
        // null element, render device, or input-map manager; action 7 in progress; a negative or
        // off-display drag origin; and the 16-pixel
        // threshold — was answered by the **enclosing** drag start that
        // raised `0x21` in this same synchronous call, on manager state this function does not
        // touch: the refusal leaves the drag origin alone, and the outer
        // call re-arms the drag-started flag before broadcasting. `0x21` has exactly one
        // handler that gets here, so there is no second entry. What is left are the
        // arms that depend only on the layout of the drag icon — it carries `0x3A`, and its slot
        // does not carry `0x39`, so the drag builds the proxy — and those are pinned by an
        // inventory scenario on the shipped salvage row.
        //
        // Retail's behaviour when it *is* forced is a genuine defect and is transcribed rather
        // than repaired, because no modernisation has been chosen: the row leaves a salvage or
        // vendor list with nothing on the cursor, and the release short-circuits
        // on the null drag element, so it runs no drag-complete half and puts
        // nothing back.
        let _refused = !ui.start_drag_and_drop_at(proxy_source, DRAG_GRAB.0, DRAG_GRAB.1);
        Some(DragStart {
            list: self.element,
            slot: i,
            item: slot.item,
            spell: slot.spell,
            drag_icon: proxy_source,
            ghosted: ghostable,
        })
    }
}

/// What [`ItemListWidget::open_first_container`] decided, and what the caller still owes the
/// child list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFirstContainer {
    /// The three early-outs: no first item, the open item id already names it, or the list does not
    /// hold it. Nothing was written.
    Unchanged,
    /// Entry 0 of the list was not a `UiItemWidget`, so the client's tail sets the child list's
    /// parent container to 0, which flushes the child list.
    ClearChild,
    /// The open item id and the frame have been written on **this** list; the caller owes the child
    /// list its parent list (this one), its parent container (`id`), and a scroll-to-show of
    /// entry 0 when the child holds anything.
    Open(ObjectId),
}
