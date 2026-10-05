//! Item drag and drop: the drag's start, its cursor feedback and the release on a target.

use super::*;

impl GamePlayScreen {
    /// Element message `0x15` reached the
    /// drop **target**, with the drag's owner in `p2`.
    ///
    /// # The drag names itself.
    ///
    /// The item is not resolved by asking [`crate::panels::inventory::InventoryPanels::locate`]
    /// which of the lists owned the element the drag started from; that fails for drags begun in a
    /// chest, the shortcut bar or a vendor/trade list. **Retail never asks.**
    ///
    /// The element manager's drag-and-drop stop builds a drag-drop record — the element (the
    /// proxy), the owner, the catcher and a success flag — and hands it to "catch dropped item" on
    /// the catcher and "drag and drop complete" on the owner, each of which is one element-message
    /// broadcast. The drop-release handler then returns unless the catcher is this element or
    /// inside it, reads the proxy's (item, spell, flags), and returns if the item id is 0 or
    /// `flags & 0x0E` is set (the alias mask is 14).
    ///
    /// The record's owner is **never read** by this function. The identity is on the proxy, which
    /// is why a drag begun in a chest, a shortcut tile or a vendor list needs no registry entry
    /// anywhere: whatever picked the icon up wrote `0x1000000F`…`0x10000014` onto it
    /// ([`crate::items::widget::ItemSlot::prepare_drag_icon`]) and every drop handler in the
    /// client reads them back with [`crate::items::widget::inq_drop_icon_info`].
    ///
    /// `owner` here is the element `dereth_ui`'s `stop_drag_and_drop` puts in `p2` — the source
    /// slot's drag icon, which is the element the properties were written onto and the one
    /// the client copies them to make the proxy, so it answers
    /// [`crate::items::widget::inq_drop_icon_info`] identically. The `locate` walk survives only as
    /// the fallback for a
    /// **direct call**, where a test hands in a bare slot handle that carries no drag properties
    /// at all.
    ///
    /// Clearing the item's waiting state (0) — the client's tail, and the tile's own `0x15` arm.
    ///
    /// Both halves, because the client's clear is both halves: the object's flag crosses the seam
    /// as [`UiRequest::ClearItemWaiting`] and reaches every panel through
    /// `GameView::slot_decoration`, and the sweep over this screen's own widgets is the same clear
    /// a frame early, so the icon comes back on the frame the player let go rather than on the next
    /// one. The eighteen shortcut lists are in the sweep for the same reason they are in the
    /// drag-hint one: `UiItemWidget` is the class, and a quickbar tile is one.
    pub(super) fn release_item_ghost(&mut self, ui: &mut UiSystem, item: ObjectId) {
        for w in self
            .inventory
            .lists_mut()
            .chain(self.shortcuts.slots.iter_mut())
        {
            w.clear_waiting(ui, item);
        }
        ui.requests.emit(UiRequest::ClearItemWaiting(item));
    }

    /// Returns the request it emitted, for a test to assert on.
    pub fn handle_drop_release(
        &mut self,
        ui: &mut UiSystem,
        target: ElemHandle,
        owner: ElemHandle,
    ) -> Option<UiRequest> {
        // Read the dragged element's (item, spell, flags), then return when there is no item.
        let info = crate::items::widget::inq_drop_icon_info(ui, owner);
        let item = match info.item {
            Some(id) => id,
            // The direct-call fallback described above. It is deliberately **not** reached by a
            // real gesture: a real drag always has a prepared icon, because
            // the drag start is never reached when
            // preparing the drag icon fails.
            None => self.locate_drag_owner(ui, owner).and_then(|(_, _, i)| i)?,
        };
        // The toolbar's drop handling is asked **first**, because the toolbar's
        // eighteen shortcut lists are `ItemListWidget`s too and `InventoryPanels::locate` would
        // not find them. The client's own order is the same: `Toolbar`'s handler runs the
        // sweep over its shortcut slots before anything else looks at the drop.
        //
        // It is handed the flags as well. The toolbar does not *gate* on `flags & 0x0E` the way the
        // other handlers do — it **forks** on it, and the second arm is the whole of the shortcut
        // bar's "relocate": see [`crate::toolbar::shortcuts::ShortcutBar::handle_drop_release`].
        if let Some(r) = self
            .shortcuts
            .handle_drop_release(ui, target, item, info.flags)
        {
            // **Correction: no ghost here.** The waiting-state set (1) is what a request that the
            // *server* must answer sets, and the toolbar's drop handling never calls it: a shortcut
            // is the player module's add-shortcut, applied locally and flushed with the options
            // blob 480 s later. Ghosting it would leave the dragged icon greyed for ever, because
            // the reply that would clear it does not exist.
            //
            // **And the ghost the *pick-up* put on has to come off here, which is the item list's
            // own drop handling's second clear route.** The shortcut list under the pointer runs
            // the class's own `0x15` arm like every other list, and its second test is the "is
            // alias list" test — the vendor-item, salvage or shortcut list flag, exactly the three
            // lists the item list's begin-drag refuses to ghost a source in — which jumps straight
            // to the client's waiting-state clear (0). Without it dragging a **backpack** out of
            // the side-pack strip onto the quickbar would leave it greyed for the session: the
            // shortcut is local, nothing is asked of the shard, and no reply is ever coming.
            self.release_item_ghost(ui, item);
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // Return when `flags & 0x0E` is set — the client, and **the alias mask is 14, not one
        // bit**. Every handler below this line repeats the same test in the client:
        // The smart-box wrapper element's drop handling,
        // the secure-trade panel's, the vendor panel's,
        // the paper doll panel's and
        // the item list's own — so one gate here is **five**
        // transcriptions, not a shortcut past them. Each of the five pairs it with the no-item
        // return, in the same order.
        //
        // **This is what stops a shortcut dragged off the bar and released over the pack from being
        // sent to the shard as a container move for an item that never left the pack.**
        // The removal has already happened at pick-up;
        // the release is supposed to do nothing at all.
        if !info.is_inventory_move() {
            return None;
        }
        // The client tests the catcher's element id for the inventory button **before** walking the
        // shortcut lists. The shortcut walk is already above because it needs the drag flags; this
        // exact-id arm is otherwise independent and must precede `InventoryPanels::drop_target`,
        // where toolbar chrome correctly resolves to no inventory list. A successful move keeps the
        // pick-up ghost until the shard answers; the host applies the client's refusal clear after
        // the object-table gates have run.
        if ui.node(target).map(dereth_ui::ElementNode::element_id)
            == Some(crate::toolbar::INVENTORY_BUTTON)
        {
            let r = self.accept_drag_object(item, crate::view::DropTarget::BackpackButton)?;
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // The element-message handler's `0x15` arm →
        // its drop handling, which is "drop it in the world": the drop's *target* is
        // whatever the 3D pick then finds, so `DropTarget::World` arms a pick rather than naming a
        // destination. `locate(target)` returns `None` for the viewport, so without this arm the
        // whole request would be dropped, `DropTarget::World` would be a variant **nothing emits**,
        // and `interaction::place_in_3d` — the client's full container/ground/split fork — would be
        // unreachable.
        //
        // It is tested **before** the inventory's own lists because `<SBOX>` is not one of them and
        // `locate` would answer `None` for it either way; the order that matters is the toolbar's,
        // above.
        if is_in_world_view(ui, target) {
            let r = self.accept_drag_object(item, crate::view::DropTarget::World)?;
            // The waiting-state set (1) — the same ghost as any other move. The item does not
            // leave the pack until `Item_ServerSaysMoveItem` says so.
            self.inventory.ghost_item(ui, item);
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // The secure trade panel's drop handling: when the drop target is inside
        // its own items list, read the dropped icon's info and accept the item if there is one and
        // it is not an alias (flags `0x0E`).
        //
        // Tested here, before the inventory's own lists, for the same reason the smart box is:
        // `0x10000088` is not one of them and `InventoryPanels::drop_target` would answer `None`,
        // so without this test a drop on the trade table would be discarded whole. The
        // **partner's** list (`0x10000081`) is deliberately not a target -- initialization
        // registers the drag handler on the self list alone.
        //
        // No ghost: the client's whole-stack arm calls
        // the waiting-state clear (**0**) before adding the item, which is the opposite of the pack
        // move's
        // waiting-state set (1) -- the row goes into the window at once and the server's `0x0200`
        // confirms it rather than authorising it.
        if is_under_element(ui, target, crate::panels::trade::SELF_LIST) {
            // The secure trade panel's drag-accept test calls the waiting-state clear (0) before
            // adding the item. The row goes into the window at once and the ghost the pick-up put
            // on comes off with it.
            self.release_item_ghost(ui, item);
            self.trade_drops
                .push((item, self.splitter.split_size, self.splitter.max_split_size));
            return None;
        }
        // The housing panel registers the same drag handler on
        // both payment lists, and its drag-accept reads this gesture's splitter words:
        // a whole stack enters the panel immediately, while a partial stack asks the inventory
        // holder to split beside its source. In neither arm does the released source stay ghosted.
        if is_under_element(ui, target, crate::panels::slumlord::BUY_LIST)
            || is_under_element(ui, target, crate::panels::slumlord::RENT_LIST)
        {
            self.release_item_ghost(ui, item);
            self.slumlord_drops.push((
                item,
                self.splitter.split_size,
                self.splitter.max_split_size,
            ));
            return None;
        }
        // The vendor's drop handling, which is the arm above with one element id
        // changed: the drop target is tested against the sell list instead.
        //
        // Tested here, before the pack's own lists, for the reason the trade table and the smart
        // box are: `0x100000CE` is not one of `InventoryPanels`' twenty-seven, so
        // `InventoryPanels::drop_target` answers `None` for it and without this test the whole drop
        // would be discarded. **That ownership split is the same one that stops a drop reaching an
        // external container** -- the vendor's three lists and `ExternalContainerPanel`'s three
        // both hang off `RemainingPanels` -- and it is answered here rather than by widening
        // `InventoryPanels`, so the container half stays a separate change.
        //
        // The stock list `0x100000BD` and the buy basket `0x100000C5` are deliberately **not**
        // targets: the set-up registers the drag handler on the sell list alone, exactly as secure
        // trade registers on the self list and not the partner's.
        //
        // No ghost, and no clear: the client's whole-stack arm calls the waiting-state clear
        // (**0**) before adding the item to the sell basket, the same polarity secure trade uses --
        // the row enters the basket at once and nothing has been asked of the shard yet.
        if is_under_element(ui, target, crate::panels::vendor::SELL_LIST) {
            // The vendor sell page's drag-accept test's waiting-state clear (0). The sell list is
            // also an alias list (the vendor-item list), so the client's alias-list route reaches
            // the same clear. The splitter this gesture used goes with the drop: the whole-stack
            // fork `accept_drag_object` makes is read from it.
            self.release_item_ghost(ui, item);
            self.vendor_sell_drops.push((
                item,
                self.splitter.split_size,
                self.splitter.max_split_size,
            ));
            return None;
        }
        // The same three lines again
        // with the salvage list (`0x10000074`) as the target test. Tested here, before the pack's
        // own lists, for the reason the two above are: `0x10000074` is not one of
        // `InventoryPanels`' twenty-seven, so `InventoryPanels::drop_target` answers `None` for it
        // and without this test the whole drop would be discarded: the window could not be filled
        // by a drag even when open.
        //
        // The client registers the drag handler on the salvage list and there is no second list, so
        // this panel has exactly one drop target.
        if is_under_element(ui, target, crate::panels::salvage::LIST) {
            // The salvage list is flagged as the salvage alias list, so the
            // alias-list test answers true and clears.
            self.release_item_ghost(ui, item);
            self.salvage_drops.push(item);
            return None;
        }
        // The Create Spell page's formula takes a carried component dragged onto it. Nothing
        // leaves the pack: the component is only named in the formula, so the pick-up's ghost
        // comes straight off.
        if is_under_element(ui, target, crate::panels::research::FORMULA) {
            self.release_item_ghost(ui, item);
            self.research_drops.push(item);
            return None;
        }
        // `InventoryPanels::drop_target` is the paper doll panel's drop handling's
        // location-from-element-id fork: a drop on one of the doll's twenty-four slots is a
        // `DropTarget::EquipLocation`, not a container move.
        // The client's is-container drop flag, read off the drag proxy exactly as the client reads
        // it — `owner` here is the source
        // slot's drag icon, which is the element
        // [`crate::items::widget::ItemSlot::prepare_drag_icon`] wrote `0x10000011` onto and which
        // the client copies the instance properties to make the proxy. It is
        // **not** a property of the list that was dropped on: it selects the container capacity and
        // container list over the item capacity and item list throughout the drag-accept test.
        let dragged_is_container = crate::items::widget::inq_drop_icon_info(ui, owner).is_container()
        // A direct call (a test, or an owner that is the slot rather than its drag icon)
        // carries no instance properties; ask the source slot the same question
        // `prepare_drag_icon` asked it.
        || self.inventory.slot_is_container(ui, owner);
        // **The client's first and third clear routes.** When the catcher is the
        // list itself rather than one of its tiles, or the list is an alias list, the ghost is
        // cleared; otherwise the drag is offered to the drag-accept test, and only a refusal
        // clears it (an accepted move keeps the ghost, because the shard owes an answer).
        //
        // Both of the `?`s below are "the drop resolved to nothing": an element this panel does
        // not map (the strip's own body, a window frame, a chrome element that happens to catch)
        // or a target `accept_drag_object` has no request for. Retail's answer to both is the
        // clear, and without it a **backpack** let go anywhere inside the inventory
        // window but not on a slot kept the ghost put on it —
        // for ever, because the wait has no timeout and no reply is coming.
        let Some(t) = self.inventory.drop_target(target) else {
            self.release_item_ghost(ui, item);
            return None;
        };
        let target = self
            .inventory
            .resolve_item_list_drop(t, dragged_is_container);
        let Some(r) = self.accept_drag_object(item, target) else {
            self.release_item_ghost(ui, item);
            return None;
        };
        // **The ghost is not applied here.**
        //
        // The item list element's drag-accept test reaches its one
        // waiting-state set (1) only *past* every decline, and
        // three of those declines are no-ops that never reach the shard at all: the item is found
        // in neither list; it is dropped back onto its own slot; or, with the whole stack in hand,
        // it is dropped on the slot just after its own.
        //
        // Each of those simply returns false — no request, no notice, and **no ghost**.
        // Those two tests already exist on the far side of this seam, in
        // `item_list_accept_drag` (`if idx == old` / `if idx == old + 1 &&
        // whole stack`), because they need the "place in items list" read and this crate may not
        // read the object table. Ghosting here would leave the refusal no way to undo the overlay:
        // nothing moves, so no `Item_ServerSaysMoveItem` ever arrives, and the wait has no timeout.
        // The item would stay ghosted for the rest of the session.
        //
        // **And the drop *clears* the ghost rather than setting one, which is the item list's
        // own drop-handling tail:** it offers the drag with `flags & 0xFFFFFF01`; accepted, it
        // leaves the ghost alone; refused, it takes the ghost off the item's object (if any).
        //
        // The ghost that is being taken off there is the one put
        // on when the icon was *picked up* — not one the drop invented. Ours is the
        // same: [`crate::items::widget::ItemListWidget::begin_drag`] ghosts the source slot.
        //
        // The seam turns retail's synchronous `if (!accepted)` into "clear it, and let the model
        // put it back": `dereth_client_model` writes `waiting = true` on each of the eight paths that
        // actually emit a `Request`, so an accepted move is re-ghosted by
        // [`crate::panels::inventory::InventoryPanels::update`]'s ghost pass on the next pass over
        // a snapshot that has genuinely changed, and a declined one is not — its snapshot is
        // identical to the one before the drag, which is precisely why nothing could ever clear a
        // hand-applied ghost. A drop the *shard* later refuses is unaffected:
        // `Item_ServerSaysAttemptFailed` clears the same flag and the same pass answers it.
        //
        // **No local `clear_waiting` here: the item list's drop handling does not clear anything on
        // this leg.**
        // An **accepted** drag leaves the ghost exactly where
        // the item list's begin-drag put it, and only the refusal branch reaches
        // the waiting-state clear (0). Both halves of that exist on
        // the far side of this seam: the pick-up writes the object's flag
        // ([`UiRequest::SetItemWaiting`], the client), and `Interaction`'s `ItemListSlot` and
        // equipment arms already answer a model refusal with `set_waiting_state(item, false)`.
        //
        // Clearing it here would be a **desync**, not a deviation that merely lost a frame: the
        // widget would say "not waiting" while the object said "waiting", and
        // [`crate::panels::inventory::InventoryPanels::update`] only re-applies ghosts on a pass
        // whose snapshot changed -- and the snapshot cannot change, because the object's flag was
        // already `true` before the drop and is still `true` after it.
        ui.requests.emit(r.clone());
        Some(r)
    }

    /// The drop-release ancestor check, applied to
    /// the drag's **owner** instead of to the target.
    ///
    /// A drag started by [`crate::items::widget::ItemListWidget::begin_drag`] owns its drag icon —
    /// a child of the slot, not the slot — because that is the element the drag start is handed.
    /// The client never asks the owner what it holds (it reads the item id back off the proxy; see
    /// [`crate::items::widget::inq_drop_icon_info`]); this screen's map is keyed by slot, so the
    /// owner is walked up to the first element the inventory recognises. One step, for a drag icon;
    /// zero, for the direct call a test makes.
    pub(super) fn locate_drag_owner(
        &self,
        ui: &UiSystem,
        owner: ElemHandle,
    ) -> Option<(ElementId, u32, Option<ObjectId>)> {
        let mut cur = Some(owner);
        while let Some(h) = cur {
            if let Some(r) = self.inventory.locate(h) {
                return Some(r);
            }
            cur = ui.parent(h);
        }
        None
    }

    /// The client's `0x21` arm, over this screen's item
    /// lists.
    ///
    /// See [`crate::items::widget::begin_drag_from_rejected`] for why a rejected drag is how an
    /// item slot is picked up at all.
    ///
    /// # The eighteen shortcut lists are offered too.
    ///
    /// The client's parent check does not consult a registry: it asks the pressed slot's own
    /// parent whether it is an `ItemListWidget` (type `0x10000031`), and a shortcut tile's
    /// parent **is** one — the toolbar's shortcut slot `n`, built during shortcut-array
    /// initialization. Offering only the inventory's four would make `0x21` on a tile start no
    /// drag at all, which in turn would make the client's is-shortcut drop arm unreachable.
    pub fn begin_item_drag(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        x: i32,
        y: i32,
    ) -> Option<crate::items::widget::DragStart> {
        let mut lists: Vec<&mut crate::items::widget::ItemListWidget> = self
            .inventory
            .item_list
            .iter_mut()
            .chain(self.inventory.container_list.iter_mut())
            .chain(self.inventory.top_container.iter_mut())
            .chain(self.inventory.doll.iter_mut().map(|(_, w)| w))
            .chain(self.shortcuts.slots.iter_mut())
            .collect();
        let started = crate::items::widget::begin_drag_from_rejected(ui, &mut lists, source, x, y);
        // Retail parity, observed in play: T, type a quantity, then click-drag the already-selected
        // stack commits that quantity without Enter. The exact native focus edge is unresolved: the
        // item element itself takes a plain mouse-down, while its bubbled `0x1C` reaches the item
        // list's listener. Preserve the observable at the narrow point where a real item drag has
        // actually started, reusing the focus-loss parser/clamp above rather than teaching every
        // item element to take focus. A press that never crosses the threshold does not reach this
        // path.
        if started.is_some() {
            if let Some(entry) = self.toolbar_children.get("stack_size_entry_box") {
                if ui.focus_element() == Some(entry) {
                    self.commit_stack_box(ui, entry);
                    ui.relinquish_focus(entry);
                }
            }
        }
        // **Beginning an item-list drag writes the ghost onto the OBJECT, and that is the only
        // reason a refill cannot lose it.** See [`UiRequest::SetItemWaiting`] for the three
        // functions that settle it. The widget's own `set_waiting` inside `begin_drag` is the
        // client's element half (the overlay); this is its object-model half, which the crate
        // boundary will not let this crate perform directly.
        if let Some(item) = started.as_ref().filter(|d| d.ghosted).and_then(|d| d.item) {
            ui.requests.emit(UiRequest::SetItemWaiting(item));
        }
        // Remember which element carries the drag's drop-icon-info payload; the
        // client reads it off the drag element, which `dereth_ui` keeps private.
        self.drag_payload = started.as_ref().map(|d| d.drag_icon);
        // The item list's begin-drag tail — the item-list-begin-drag notice with the list and the
        // item's slot number, sent for **every** list, not only the toolbar's.
        //
        // The toolbar's item-list-begin-drag notice is the only listener that does
        // anything with it, and what it does is *"dragging items FROM the shortcut bar to remove
        // them"*: the shortcut comes off the bar at **pick-up**, and the drop has no
        // removal half at all. See
        // [`crate::toolbar::shortcuts::ShortcutBar::on_item_list_begin_drag`].
        if let Some(d) = started.as_ref() {
            if let Some(item) = self.shortcuts.on_item_list_begin_drag(d.list) {
                ui.requests.emit(UiRequest::RemoveShortcut(item));
            }
        }
        started
    }

    /// The client's **`0x3E` arm** — the one thing the
    /// retail client changes on screen while an inventory item is being dragged. This is the
    /// consumer arm; it is driven by the message, not by a per-frame poll.
    ///
    /// # The producer
    ///
    /// The element manager's mouse-over switch, after the ordinary mouse-over bookkeeping: while
    /// a drag is under way, the element under the pointer is asked for its drag-and-drop catcher.
    /// When that catcher differs from the last one, the old one (if any) gets `0x3E` with
    /// `p1 = 0` (leaving), the new one (if any) gets `0x3E` with `p1 = 1` (entering), and the new
    /// one is remembered.
    ///
    /// It lives in `dereth_ui::focus`'s `mouse_move`, so the remembered catcher
    /// is `DragState::last_drag_cursor_over` and **this screen keeps no copy of it**: the message
    /// names the element, which is the whole point of not polling.
    ///
    /// # The arm
    ///
    /// On `0x3E`: leaving (`p1 == 0`), or entering with no drag element, puts the slot's
    /// drag-accept icon (if it has one and is not already there) in state `0x1000003f`.
    /// Otherwise, if the slot's parent is an item list (type `0x10000031`), the list's drag-over
    /// handling is asked with the drag element and this slot.
    ///
    /// Three things it is worth being exact about, because each is a place a poll would differ:
    ///
    /// * **The leave arm acts on the one slot the message names**, not on every slot of the list.
    ///   A poll has no message and therefore has to sweep.
    /// * **The enter arm with no drag element clears rather than falls through** — it is the
    ///   same drag-accept state `0x1000003F` as the leave arm.
    /// * The item list's drag-over test is asked on the slot's **own list**, so the decision belongs to the
    ///   list and the drawing to the slot. Here the list is whichever of this panel's lists holds
    ///   the slot; `slot_of` finds it by handle, and deliberately not
    ///   [`crate::panels::inventory::InventoryPanels::locate`], which answers a bare *list* handle
    ///   with slot 0 and so cannot tell a list from its own first slot.
    ///
    /// # Where the empty-slot count comes from
    ///
    /// The item list's drag-over test's container branch looks up the weenie object for the item under the
    /// pointer and then asks its empty-slot count. An element-message handler on this
    /// screen has no [`GameView`] — the seam is `&dyn GameView` and the `Screen` trait's
    /// `on_element_message` does not carry one — so the two values that answer it, the item
    /// capacity and the number of items held, are kept on the slot by
    /// [`crate::items::widget::ItemSlot::num_empty_item_slots`], written by the item element's
    /// update's own decoration pass. That is the same pair the capacity bar reads off the same
    /// weenie in the same function, so the cache goes stale exactly when the capacity bar
    /// does and never independently of it.
    ///
    /// Returns the drag-accept state now showing, if any. `None` is *"one of the drag-over test's
    /// early-outs fired"*, which is not the same as choosing `drag_accept_state::NONE`.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        over: ElemHandle,
        entering: bool,
    ) -> Option<dereth_ui::StateId> {
        use crate::items::widget::{drag_accept_state, inq_drop_icon_info};

        // "a drag has started and there is a drag element" — `is_dragging` is "there is a drag
        // element", which is the stricter of the two and is the one the client tests.
        //
        // **Read the drag element itself.** The client reads the element manager's drag element and
        // nothing else. Reading a copy of the *owner* that `Self::begin_item_drag` recorded goes
        // wrong the moment a drag begins somewhere this screen does not own — a chest slot, a
        // vendor list — because `begin_item_drag` then answers `None` and the hover hint silently
        // stops appearing over the pack. `drag_payload` is still written, as a fallback and for the
        // tests that pin the producer.
        let payload = ui
            .drag_state()
            .element
            .or(self.drag_payload)
            .filter(|_| ui.is_dragging());
        let info = payload.map(|p| inq_drop_icon_info(ui, p));
        // **The client's first statement, the list's own drag handler.** Two of this screen's list
        // families register one, and neither ever falls through to the default below:
        //
        // * the eighteen shortcut lists, answered
        //   here by [`crate::toolbar::shortcuts::ShortcutBar::on_drag_cursor_over`];
        // * the twenty-four doll lists, whose handler asks about the item's valid locations and so
        //   needs a `GameView` this arm does not have. The message is already in
        //   [`Self::panel_messages`], and the host hands it to [`Self::on_paper_doll_drag_over`] in
        //   the same frame. Stepping aside here is what stops the default's
        //   "not a container list -> ACCEPT" from painting green on a slot the doll is about to
        //   refuse.
        if let Some(s) = self
            .shortcuts
            .on_drag_cursor_over(ui, over, info.filter(|_| entering))
        {
            return s;
        }
        if self.inventory.doll_slot_of(over).is_some() {
            return None;
        }
        for w in self.inventory.lists_mut() {
            let Some(slot) = w.slot_of(over) else {
                continue;
            };
            let Some(info) = info.filter(|_| entering) else {
                // Both of the arm's clearing routes: `p1 == 0`, and `p1 != 0` with no drag
                // element.
                w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
                return Some(drag_accept_state::NONE);
            };
            // The empty-slot count of the item's weenie object, read off the slot the
            // pointer is over — the only object the drag-over test asks about.
            let under = w.slots.get(slot).and_then(|s| s.item);
            let free = w
                .slots
                .get(slot)
                .map(crate::items::widget::ItemSlot::num_empty_item_slots);
            return w.drag_over(ui, slot, info, &|id| {
                if Some(id) == under {
                    free
                } else {
                    None
                }
            });
        }
        None
    }

    /// The paper doll's item-list drag-over, delivered by the host together with the
    /// view the arm needs. See
    /// [`crate::panels::inventory::InventoryPanels::on_paper_doll_drag_over`]. This is the same
    /// one-frame hop [`Self::panel_messages`] gives `RemainingPanels`, taken by the one handler
    /// whose lists live on this screen rather than there. Returns true when the message was a
    /// doll tile's `0x3E`.
    pub fn on_paper_doll_drag_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        self.inventory.on_paper_doll_drag_over(ui, m, view)
    }

    /// A drag released over one of this
    /// screen's item lists.
    ///
    /// A `DropTarget::ItemList` carries a *list element id and a slot index*, and only the panel
    /// that filled the list knows which container object that list is showing or which object
    /// sits in the slot, so the interaction layer cannot resolve it. Resolving it here, where the
    /// map is, turns the drop into the `DropTarget::Container` the interaction layer already sends
    /// `PutItemInContainer` for.
    ///
    /// Returns the request to emit, or `None` when the drop lands on nothing (an empty slot of a
    /// list with no container, or a shortcut slot, which is `Toolbar`'s).
    #[must_use]
    pub fn accept_drag_object(
        &self,
        item: ObjectId,
        target: crate::view::DropTarget,
    ) -> Option<UiRequest> {
        use crate::view::DropTarget;
        match target {
            // The model applies equipment legality to the resolved destination.
            DropTarget::BackpackButton
            | DropTarget::EquipLocation { .. }
            | DropTarget::EquipCanvas
            | DropTarget::World
            | DropTarget::Container(_) => Some(UiRequest::DragDrop { item, target }),
            // [`crate::panels::inventory::InventoryPanels::resolve_item_list_drop`] runs first and
            // produces a `DropTarget::ItemListSlot` for this panel's lists; resolving to a bare
            // `DropTarget::Container` would throw the slot index away and, on an occupied slot, aim
            // the move at *the object in that slot* whether or not it is a container. Anything
            // still arriving here as `ItemList` is a list this panel does not own and keeps this
            // deliberately conservative answer.
            DropTarget::ItemList { .. } => {
                let c = self.inventory.resolve_drop(target)?;
                Some(UiRequest::DragDrop {
                    item,
                    target: DropTarget::Container(c),
                })
            }
            DropTarget::ItemListSlot { .. } => Some(UiRequest::DragDrop { item, target }),
            // The toolbar's drop handling → its "create shortcut to item" / "add shortcut". The
            // slot is already resolved — it is a slot **number**, not an element id — so unlike
            // `ItemList` there is nothing left to look up: the request goes out unchanged and the
            // bar fills when `PlayerModule` says it has. `ShortcutAlias` is the client's
            // is-shortcut drop arm. Same shape and for the same reason: both slot numbers are
            // already resolved, so there is nothing left for this map to look up.
            DropTarget::ShortcutSlot(_) | DropTarget::ShortcutAlias { .. } => {
                Some(UiRequest::DragDrop { item, target })
            }
        }
    }
}
