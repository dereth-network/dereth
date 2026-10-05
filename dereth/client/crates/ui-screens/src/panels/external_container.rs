//! External-container presentation.
//! These three ItemLists own their own child container, never the player's pickup destination.

use crate::items::widget::{ItemListWidget, OpenFirstContainer, TileInfo};
use crate::view::{GameView, UiRequest};
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, ElementId, ElementMessage, UiSystem};

pub const PANEL: ElementId = ElementId(0x1000_005D);
pub const TOP: ElementId = ElementId(0x1000_0064);
pub const CONTAINERS: ElementId = ElementId(0x1000_0067);
pub const CLOSE: ElementId = ElementId(0x1000_0068);
pub const ITEMS: ElementId = ElementId(0x1000_006A);

/// The two subscribed item notices, retained in their original relative order.
///
/// Defined in [`dereth_client_contract::panels::external_container`], because it is the
/// value `dereth_client_runtime::hud` queues and this panel drains.
pub use dereth_client_contract::panels::external_container::ExternalContainerNotice;

#[derive(Debug, Clone, PartialEq)]
struct ListSnapshot {
    parent: Option<ObjectId>,
    capacity: Option<i32>,
    tiles: Vec<(ObjectId, Option<DataId>, Option<TileInfo>, bool)>,
}

#[derive(Debug, Default)]
pub struct ExternalContainerPanel {
    /// The UI system's live target-mode flag as the item lists see it, refreshed by the host
    /// before delivery.
    pub target_mode_active: bool,
    pub root: Option<ElemHandle>,
    pub top_container: Option<ItemListWidget>,
    pub container_list: Option<ItemListWidget>,
    pub item_list: Option<ItemListWidget>,
    pub ground_object: Option<ObjectId>,
    pub open_container: Option<ObjectId>,
    last: [Option<ListSnapshot>; 3],
}

impl ExternalContainerPanel {
    /// The panel's post-init: bind the shipped lists and close the initial page.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let Some(page) = ui.get_child_recursive(root, PANEL) else {
            return;
        };
        self.root = Some(page);
        self.top_container = ui
            .get_child_recursive(page, TOP)
            .map(|h| ItemListWidget::init(ui, h));
        self.container_list = ui
            .get_child_recursive(page, CONTAINERS)
            .map(|h| ItemListWidget::init(ui, h));
        self.item_list = ui
            .get_child_recursive(page, ITEMS)
            .map(|h| ItemListWidget::init(ui, h));
        ui.set_visible(page, false);
    }

    fn open(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        id: Option<ObjectId>,
        view: &dyn GameView,
    ) {
        let old = self.open_container;
        self.open_container =
            id.filter(|id| view.slot_decoration(*id).is_some_and(|d| d.is_container));
        for list in [&mut self.top_container, &mut self.container_list]
            .into_iter()
            .flatten()
        {
            list.open_item_id = id.filter(|id| list.is_in_list(*id));
        }
        // The set-parent-container notice is also heard by the UI system. The game-side
        // owned-pack guard deliberately refuses external ids as pickup destinations.
        if old != self.open_container {
            if let Some(id) = self.open_container {
                requests_out.emit(UiRequest::NewParentContainer(id));
            }
        }
    }

    pub fn recv_notice(
        &mut self,
        ui: &mut UiSystem,
        notice: ExternalContainerNotice,
        view: &dyn GameView,
    ) {
        match notice {
            ExternalContainerNotice::SetGroundObject(id) => {
                // The client writes the ground-object id BEFORE hiding. Consequently a
                // server/range-generated zero does not run the current-container close's USE again.
                self.ground_object = (id.0 != 0).then_some(id);
                self.open_container = self
                    .ground_object
                    .filter(|id| view.slot_decoration(*id).is_some_and(|d| d.is_container));
                self.last = Default::default();
                if let Some(top) = &mut self.top_container {
                    top.flush(ui);
                    top.open_item_id = self.ground_object;
                }
                if let Some(list) = &mut self.container_list {
                    list.open_item_id = None;
                }
                if let Some(page) = self.root {
                    ui.set_visible(page, id.0 != 0);
                }
                if self.open_container.is_some() {
                    ui.requests.emit(UiRequest::NewParentContainer(id));
                }
            }
            ExternalContainerNotice::ItemMoved { object, container }
                if self.open_container == Some(object)
                    && self
                        .container_list
                        .as_ref()
                        .and_then(|w| w.parent_container)
                        != Some(container) =>
            {
                // The server-says-move-item notice. This is not an unconditional reset
                // on every pickup: only the currently viewed *child container* leaving its parent.
                let next = self
                    .top_container
                    .as_mut()
                    .map(|w| w.open_first_container(ui));
                match next {
                    Some(OpenFirstContainer::Open(id)) => {
                        self.open(&mut ui.requests, Some(id), view)
                    }
                    Some(OpenFirstContainer::ClearChild) => self.open(&mut ui.requests, None, view),
                    _ => {}
                }
            }
            ExternalContainerNotice::ItemMoved { .. } => {}
        }
        // Retail fills synchronously. A following notice in the same batch must see the new
        // top/child lists, not whichever snapshot happened to be drawn in the previous frame.
        self.update(ui, view);
    }

    fn close(&mut self, ui: &mut UiSystem) {
        // The visibility-changed handler -> the close current container. The game half
        // executes USE then unregisters range; server ground-object state is not cleared here.
        if let Some(id) = self.ground_object.take() {
            ui.requests.emit(UiRequest::CloseExternalContainer(id));
            self.open_container = None;
            for list in [&mut self.top_container, &mut self.container_list]
                .into_iter()
                .flatten()
            {
                list.open_item_id = None;
            }
            self.last = Default::default();
        }
        if let Some(root) = self.root {
            ui.set_visible(root, false);
        }
    }

    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The host delivered page visibility before this pass. This is the field's visibility
        // callback, not an ancestor-visibility/range heuristic: covering this page closes it.
        if self
            .root
            .is_some_and(|h| ui.node(h).is_some_and(|n| !n.region.flags.visible))
        {
            self.close(ui);
        }
        let mut changed = false;
        let ground = self.ground_object;
        let ground_parent =
            ground.filter(|id| view.slot_decoration(*id).is_some_and(|d| d.is_container));
        let open = self.open_container;
        let openable = |id| {
            view.slot_decoration(id)
                .is_some_and(|d| d.openable || d.is_player)
        };
        for (i, list) in [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ]
        .into_iter()
        .enumerate()
        {
            let Some(list) = list else { continue };
            let (parent, capacity, mut ids) = match i {
                0 => (None, None, ground.into_iter().collect::<Vec<_>>()),
                1 => (
                    ground_parent,
                    ground_parent.and_then(|id| view.containers_capacity(id)),
                    ground_parent
                        .filter(|id| openable(*id))
                        .map(|id| view.contained_containers(id).to_vec())
                        .unwrap_or_default(),
                ),
                _ => (
                    open,
                    open.and_then(|id| view.items_capacity(id)),
                    open.filter(|id| openable(*id))
                        .map(|id| view.container_contents(id).to_vec())
                        .unwrap_or_default(),
                ),
            };
            // The client's provisional row, spliced into
            // the ids this list fills from for exactly the reason `InventoryPanels::update`
            // splices it: this panel is the list's refill, so the row has to arrive with the
            // contents. `i == 1` is the chest's pack strip (the container list), `i == 2` its item
            // grid; `i == 0` is the top-container strip, one fixed row for the ground object
            // itself, which retail never inserts into. The splice is before the `same` guard below
            // so that arming or clearing the row is itself a frame that rebuilds.
            if i != 0 {
                if let Some((item, at)) = view.pending_row(parent, i == 1) {
                    let at = (at as usize).min(ids.len());
                    ids.insert(at, item);
                }
            }
            let ids = ids;
            // **Compare the element against the OBJECT.**
            //
            // Retail skips the tile's waiting-state write only when the tile already shows the
            // object's value.
            //
            // The guard below memoises *the view's* value against *the previous view's* value,
            // which is not the same question: it cannot see a tile that is drawn ghosted with no
            // object behind it, because nothing in the model ever changes. That state is reachable
            // here and not in retail — `begin_drag` puts the element half on,
            // its object half is [`UiRequest::SetItemWaiting`], and a chest row whose id the
            // `0x0196` listed but whose create the session never carried has no object to write —
            // so without this the ghost stays on that row for the rest of the session.
            //
            // One extra term, and it is the client itself: what is *drawn* against what the object
            // says. When they disagree the list rebuilds, `set_contents`'s `flush` takes every
            // ghost off, and the pass below re-raises only the ones the object justifies. The rule
            // stays object-first: the value that wins is always the object's.
            let drawn_matches_the_objects = list
                .slots
                .iter()
                .all(|s| s.item.is_none_or(|id| s.waiting == view.item_waiting(id)));
            let last = &self.last[i];
            let same = drawn_matches_the_objects
                && last.as_ref().is_some_and(|s| {
                    s.parent == parent
                        && s.capacity == capacity
                        && s.tiles.len() == ids.len()
                        && s.tiles
                            .iter()
                            .zip(&ids)
                            .all(|((old, icon, tile, waiting), id)| {
                                old == id
                                    && *icon == view.icon(*id)
                                    && TileInfo::matches(view, *id, tile.as_ref())
                                    && *waiting == view.item_waiting(*id)
                            })
                });
            if !same {
                let snapshot = ListSnapshot {
                    parent,
                    capacity,
                    tiles: ids
                        .iter()
                        .map(|id| {
                            (
                                *id,
                                view.icon(*id),
                                TileInfo::read(view, *id),
                                view.item_waiting(*id),
                            )
                        })
                        .collect(),
                };
                let changed_parent = list.parent_container != parent;
                let now = ui.now.0;
                list.set_contents(ui, parent, capacity, &ids, &|id| view.icon(id));
                list.decorate(ui, &|id| {
                    snapshot
                        .tiles
                        .iter()
                        .find(|t| t.0 == id)
                        .and_then(|t| t.2.as_ref())
                        .map(|t| t.to_slot_info(view, now))
                });
                // `set_contents`'s own `flush` has just put every slot back to `ItemSlot::clear`'s
                // state, `waiting` included, so this pass is the whole of the item tile update's
                // waiting-state write from the object: the raise, and the clear by omission.
                // It is *reached* for the row that needs it, because the
                // guard above compares what is drawn against the object as well.
                for (id, _, _, waiting) in &snapshot.tiles {
                    if *waiting {
                        list.ghost(ui, *id);
                    }
                }
                if changed_parent && !list.slots.is_empty() {
                    list.scroll_to_show(ui, 0);
                }
                self.last[i] = Some(snapshot);
                changed = true;
            }
            if i != 2 {
                list.update_open_container_indicator(ui, open);
            }
            let now = ui.now.0;
            list.do_heartbeat(ui, now, &|id| view.cooldown_remaining(id, now));
        }
        changed
    }

    /// The item list's drop-release handler, run over this panel's own three lists — what lets
    /// a player drag and drop items into an open container.
    ///
    /// What the client does: without both a drag source and a drop target, or when the target is
    /// neither this list nor inside it, it returns. It reads the dropped icon's item, spell and
    /// flags; no item, or any of flags `0x0E` (vendor, shortcut, salvage), returns. A drop onto the
    /// list itself or onto an alias list goes straight to the un-ghost; otherwise an accepted drag
    /// object (flags masked with `0xFFFFFF01`) returns, and a refused one clears the item's waiting
    /// state.
    ///
    /// **The external-container drag-acceptability predicate is not on the drop path.**
    /// Its only caller is the panel's list-drag-over callback, reached only from the item
    /// list's drag-over handler. That handler is the sole reader of the list's registered drag
    /// handler; registration writes the field and unregistration clears it. The callback's `bool`
    /// decides whether the list's **default hover feedback** runs, nothing more; the drop's own
    /// acceptance is the list's accept-drag-object step → the item holder's "attempt to place in
    /// container", which keeps an `OPENABLE` destination or the player itself; otherwise it
    /// discards the destination and lets the spill re-aim. **Three functions on that path make the
    /// same test and not one of them asks whether the player owns the destination**: this one, the
    /// "is drag into container attempt legal" test, and the "is container legal" test.
    /// `attempt_to_place_in_container` already transcribes that gate, which is why this seam needed
    /// no game-side change.
    ///
    /// The hover *hint* — drag-accept state `0x10000040` / `0x10000041` — is
    /// [`Self::on_drag_cursor_over`], which needs the ground object's hook type and hook item
    /// types across the [`GameView`] seam:
    /// [`GameView::external_container_drag_item_acceptable`].
    ///
    /// Returns whether the drop was consumed.
    pub fn handle_drop_release(
        &mut self,
        ui: &mut UiSystem,
        target: ElemHandle,
        owner: ElemHandle,
    ) -> bool {
        let info = crate::items::widget::inq_drop_icon_info(ui, owner);
        // No item returns, and so does any of flags `0x0E` — a vendor, shortcut or salvage
        // proxy is another window's business and this one must not turn it into a container move.
        let Some(item) = info.item else { return false };
        if !info.is_inventory_move() {
            return false;
        }
        // The item under the mouse first, then the list's slot index. The pointer's own element
        // is asked first because that is literally what the client asks; the drop message carries
        // the *list* (only it carries attribute `0x36`), so the target alone cannot name a slot.
        let under_mouse = ui.mouse_over();
        for list in [&self.top_container, &self.container_list, &self.item_list]
            .into_iter()
            .flatten()
        {
            // The target must be the list or inside it. **This guard
            // is load-bearing and not decoration**: `mouse_over()` below is asked before the
            // target is, so without it a drop released on the spell bar while the pointer happened
            // to sit over a chest slot would be stolen by this panel — and `RemainingPanels::
            // on_element_message` offers this panel the message *first*.
            if list.handle != target && !ui.is_ancestor_of(list.handle, target) {
                continue;
            }
            let index = under_mouse
                .and_then(|h| Self::slot_under(ui, list, h))
                .or_else(|| Self::slot_under(ui, list, target))
                .or_else(|| (list.handle == target).then_some(0));
            let Some(index) = index else { continue };
            ui.requests.emit(UiRequest::DragDrop {
                item,
                target: crate::view::DropTarget::ItemListSlot {
                    // The list's parent-container id, and **zero is a legal value that must be
                    // passed on, not declined**: the top-container strip `0x10000064` never has
                    // one, and the drop still works there because the destination re-aim takes the
                    // object *under the pointer* — the ground container itself — as soon as its
                    // items capacity is non-zero; only when that too is zero does the container
                    // test answer "cannot accept items".
                    //
                    // `item_list_accept_drag` runs that same re-aim and
                    // has its own `if container.0 == 0 { return false }` after it, which is
                    // the place-in-container step's guard against object 0.
                    container: list.parent_container.unwrap_or(ObjectId(0)),
                    under: list.item_at(index),
                    index: u32::try_from(index).unwrap_or(0),
                    num_ui_items: u32::try_from(list.num_ui_items()).unwrap_or(u32::MAX),
                    dragged_is_container: info.is_container(),
                    container_list: list.container_list,
                },
            });
            return true;
        }
        false
    }

    /// The slot of `list` that owns `h`, walking up as the client's item-under-mouse lookup would.
    fn slot_under(ui: &UiSystem, list: &ItemListWidget, h: ElemHandle) -> Option<usize> {
        let mut cur = Some(h);
        while let Some(c) = cur {
            if let Some(i) = list.slot_of(c) {
                return Some(i);
            }
            cur = ui.parent(c);
        }
        None
    }

    /// The client's `0x21` arm, over this panel's three
    /// lists.
    ///
    /// An item slot is picked up *by being refused* the generic drag (see
    /// [`crate::items::widget::begin_drag_from_rejected`]) — the client asks the pressed slot's own
    /// parent whether it is an item list (type `0x10000031`) and never consults a registry.
    ///
    /// It depends on the drop side reading the item straight off the proxy:
    /// `ItemListWidget::begin_drag` ghosts the source slot, and a release that resolved the drag's
    /// *owner* through `InventoryPanels::locate` would answer `None` for **these** lists and
    /// discard the whole release before any request was built, leaving the icon greyed for the
    /// rest of the session. The item-list drop handler never asks which list the owner belongs to,
    /// and neither does the screen, so the arm is safe.
    fn begin_item_drag(&mut self, ui: &mut UiSystem, m: &ElementMessage) -> bool {
        let (x, y) = m.point.window;
        let mut lists: Vec<&mut ItemListWidget> = [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ]
        .into_iter()
        .flatten()
        .collect();
        let started =
            crate::items::widget::begin_drag_from_rejected(ui, &mut lists, m.source, x, y);
        // **The drag start writes the OBJECT as well as the element.**
        //
        // Set the item's waiting state unless the source is a vendor, salvage or shortcut list.
        //
        // The waiting-state write is both halves: the object's waiting flag and the tile's
        // ghosted icon. This crate never writes the object, so the object write crosses the seam as [`UiRequest::SetItemWaiting`] — the
        // same hop `GamePlayScreen::begin_item_drag` and `SlumlordPanel::begin_payment_drag`
        // already make.
        //
        // Without it the ghost `begin_drag` puts on the tile had **no route back**: every clear
        // in the client — the drag owner's own `0x15`, the server-says-attempt-failed and
        // server-says-move-item paths — writes the object, and this panel's
        // [`Self::update`] mirrors the object onto the tile only on a frame its snapshot guard
        // says something moved. A flag that was never raised never falls, so the guard never
        // fired and a refused or cancelled drag out of a chest left that row grey for the rest
        // of the session.
        if let Some(item) = started.as_ref().filter(|d| d.ghosted).and_then(|d| d.item) {
            ui.requests.emit(UiRequest::SetItemWaiting(item));
        }
        started.is_some()
    }

    /// The client's `0x15` arm, on **this window's**
    /// tiles — the same arm the pack and the quickbar have.
    ///
    /// On `0x15` a tile that has an object clears that object's waiting state and, unless the tile
    /// is unghostable, hides its ghosted icon.
    ///
    /// It runs on the **catcher's** copy of the message — the one carrying the drag's owner in
    /// `p2` — and clears *that tile's own* weenie, not the drag source's: the accepted source is
    /// told `0x16` by the drag-and-drop completion and deliberately stays waiting. The clear
    /// is unconditional and happens before the parent `ItemListWidget` runs
    /// its drop handling, so a chest row that was waiting on an older request
    /// un-ghosts the moment something is dropped on it.
    fn release_catcher_ghost(&mut self, ui: &mut UiSystem, source: ElemHandle) {
        let item = [&self.top_container, &self.container_list, &self.item_list]
            .into_iter()
            .flatten()
            .find_map(|w| w.slot_of(source).and_then(|slot| w.slots[slot].item));
        let Some(item) = item else { return };
        for w in [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ]
        .into_iter()
        .flatten()
        {
            w.clear_waiting(ui, item);
        }
        ui.requests.emit(UiRequest::ClearItemWaiting(item));
    }

    /// The external container panel's item-list drag-over and, for the two lists that have no
    /// handler, the item list's own default drag-over.
    /// Chests and corpses do show a drag hint in retail, and it is green.
    ///
    /// # Which of the three lists is a drag handler's
    ///
    /// The client binds all three — `0x10000064`, `0x10000067` and `0x1000006A`, each as an item
    /// list (type `0x10000031`) — and registers a drag handler on exactly one, the item grid
    /// `0x1000006A`, so the contents grid answers the client and the two container strips fall
    /// through to the shared default, which is [`ItemListWidget::drag_over`].
    ///
    /// # The item grid's drag-over handler
    ///
    /// A zero item id (spell drag) or flags masked by `0x0E` (vendor/shortcut/salvage)
    /// returns true without painting. Otherwise call the containing panel's drag-acceptability
    /// predicate with quiet=1, an argument it never reads. Set hover state `0x10000040`
    /// on acceptance or `0x10000041` on refusal. Every branch returns true.
    ///
    /// The three readings that decide what the player sees:
    ///
    /// * **the decision is the hook-status check and nothing else** — see
    ///   [`GameView::external_container_drag_item_acceptable`]. A container
    ///   whose hook type or hook item types is zero answers `1`, so **a chest and a corpse
    ///   are always green**, for every item, with no ownership, capacity or type test anywhere
    ///   on the path. The window never asks whether the object is a corpse;
    /// * **it asks about the ground object, not the open child container.**
    ///   The ground-object notice is the only writer of that field, and the current-container
    ///   close uses it as the object to use shut, so the question is always about
    ///   the chest itself even while a side pack inside it is the open one;
    /// * **the handler returns true unconditionally**, so the default three-way never runs on
    ///   `0x1000006A` — which is why a *pack* carried over a chest's contents grid gets the plain
    ///   green here and not the default's `container list ? ACCEPT : REFUSE`.
    ///
    /// Returns whether the message named one of this window's slots.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::{drag_accept_state, inq_drop_icon_info};
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        // 0/1 are the two container strips, which have no drag handler; 2 is the item grid,
        // which has one. The index is kept because the two arms below are different functions.
        let which = [&self.top_container, &self.container_list, &self.item_list]
            .into_iter()
            .enumerate()
            .find_map(|(i, l)| l.as_ref().and_then(|w| w.slot_of(m.source)).map(|s| (i, s)));
        let Some((list, slot)) = which else {
            return false;
        };
        let lists = [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ];
        let Some(w) = lists.into_iter().nth(list).and_then(Option::as_mut) else {
            return false;
        };
        // Both of the client's clearing routes:
        // `p1 == 0` (the cursor left this tile), and `p1 != 0` with no drag
        // element. Neither reaches the list, let alone the handler.
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            if let Some(s) = w.slots.get_mut(slot) {
                s.set_drag_accept_state(ui, drag_accept_state::NONE);
            }
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        if list != 2 {
            // No handler on `0x10000064` / `0x10000067`: the client's own
            // default, reading the free item-slot count of the object in the slot under the
            // pointer -- the only object the default asks about.
            let under = w.slots.get(slot).and_then(|s| s.item);
            let free = w
                .slots
                .get(slot)
                .map(crate::items::widget::ItemSlot::num_empty_item_slots);
            w.drag_over(ui, slot, info, &|id| {
                if Some(id) == under {
                    free
                } else {
                    None
                }
            });
            return true;
        }
        // The client returns true without setting any drag-accept state, so the
        // tile keeps whatever it had. A spell carried over a chest paints nothing.
        let Some(item) = info.item.filter(|_| info.is_inventory_move()) else {
            return true;
        };
        // The client: no ground object -> the drag-acceptability predicate returns true before it
        // looks at anything, which is the same green.
        let accepted = self
            .ground_object
            .is_none_or(|g| view.external_container_drag_item_acceptable(item, g));
        let state = if accepted {
            drag_accept_state::ACCEPT
        } else {
            drag_accept_state::REFUSE
        };
        if let Some(s) = w.slots.get_mut(slot) {
            s.set_drag_accept_state(ui, state);
        }
        true
    }

    /// The item list element's element-message handler's **`0x15` arm** — the clear
    /// [`Self::on_drag_cursor_over`]'s hint needs when the drop lands.
    ///
    /// `0x3E` has exactly one producer, the mouse-over switch, and the pointer does not move
    /// between the release and the drag ending — so without this the tile the drop landed on keeps
    /// its green for as long as the window is open. Same split as `salvage.on_drop_release` and
    /// `trade.on_drop_release`: keyed on the message rather than on an id, it runs whether or not
    /// the drop is taken, and it never consumes the message.
    pub fn clear_drop_hint(&mut self, ui: &mut UiSystem, source: ElemHandle) {
        use crate::items::widget::drag_accept_state;
        for w in [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(slot) = w.slot_of(source) {
                if let Some(s) = w.slots.get_mut(slot) {
                    s.set_drag_accept_state(ui, drag_accept_state::NONE);
                }
            }
        }
    }

    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        if m.id == dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return self.on_drag_cursor_over(ui, m, view);
        }
        if m.id == dereth_ui::msg::element::id::BUTTON_CLICKED && m.source_id == CLOSE {
            self.close(ui);
            return true;
        }
        // Tested before the `MOUSE_PRESS` early-out below, which would otherwise swallow it:
        // every message that is not `BUTTON_CLICKED` or `0x1C` returns `false` there, and nothing
        // else can take a `0x15` for these lists, because `GamePlayScreen::handle_drop_release` resolves a target only through
        // `InventoryPanels::drop_target` and these three lists hang off `RemainingPanels`.
        if m.id == dereth_ui::msg::element::id::DROP_FAILED {
            // The client returns without both a drag source and a drop target. The
            // drop is broadcast twice — once to the **target** with the owner in `p2`, once to the
            // owner with `p2 == 0` — and the client's null test is what tells them apart.
            if m.p2 == 0 {
                return false;
            }
            // The child item tile runs its own `0x15` arm before the
            // parent list's drop-release handling — see [`Self::release_catcher_ghost`].
            self.release_catcher_ghost(ui, m.source);
            // `ItemListWidget`'s own `0x15` arm, in the same order the
            // client runs it — see [`Self::clear_drop_hint`].
            self.clear_drop_hint(ui, m.source);
            return self.handle_drop_release(ui, m.source, ElemHandle::from_raw(m.p2));
        }
        // The `0x21` for this panel's three lists — see
        // [`Self::begin_item_drag`]. Both this handler and `GamePlayScreen::begin_item_drag` are
        // offered the message and exactly one list can own the pressed slot, so the duplicate
        // delivery is a no-op on whichever misses; the spell bar has the same shape.
        if m.id == dereth_ui::msg::element::id::DRAG_REJECTED {
            return self.begin_item_drag(ui, m);
        }
        if m.id != dereth_ui::msg::element::id::MOUSE_PRESS {
            return false;
        }
        let mut open = None;
        let mut consumed = false;
        for list in [
            &mut self.top_container,
            &mut self.container_list,
            &mut self.item_list,
        ]
        .into_iter()
        .flatten()
        {
            let Some(index) = list.slot_of(m.source) else {
                continue;
            };
            let Some(item) = list.item_at(index) else {
                return false;
            };
            match m.p1 {
                7 if self.target_mode_active => {
                    ui.requests.emit(UiRequest::ExecuteTargetItem(item));
                }
                7 | 8 => {
                    if list.single_selection {
                        list.handle_single_selection(ui, index);
                    }
                    ui.requests.emit(UiRequest::Select(item));
                    if m.p1 == 8 {
                        ui.requests.emit(UiRequest::Examine(item));
                    } else if list.container_list {
                        open = Some(item);
                    }
                }
                10 => ui.requests.emit(UiRequest::Use(item)),
                _ => return false,
            }
            consumed = true;
            break;
        }
        if let Some(item) = open {
            self.open(&mut ui.requests, Some(item), view);
        }
        consumed
    }
}
