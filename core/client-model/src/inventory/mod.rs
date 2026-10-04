//! Containment, slots, burden, the request senders, wield/wear legality and salvage.

pub mod burden;
pub mod equip;
pub mod requests;
pub mod salvage;
/// `INVENTORY_LOC` and the slot tables. Lives in [`dereth_rules::slots`].
pub use dereth_rules::slots;
pub mod targeted_use;
/// What a use on an object does, and the pickup it dispatches to.
pub mod use_object;

use crate::weenie::{item_type, NameType, Weenie};
use crate::world::World;
use crate::{Notice, NoticeSink, Request, RequestSink};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::items::{
    InventoryDropItem, InventoryGetAndWieldItem, InventoryGiveObjectRequest,
    InventoryPutItemInContainer, InventoryStackableMerge, InventoryStackableSplitTo3d,
    InventoryStackableSplitToContainer, InventoryStackableSplitToWield, InventoryUseEvent,
    InventoryUseWithTargetEvent,
};
use dereth_protocol::objects::ItemAppraise;
use requests::{ready_for_inventory_request, InventoryRequest, FEEDBACK_CHANNEL};

/// The drag quantity: how many of the stack to move, and the most that can be moved.
///
/// **`split_size == max_split_size` means "move everything"**; there is no separate boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitState {
    pub split_size: u32,
    pub max_split_size: u32,
}

impl SplitState {
    #[must_use]
    pub fn whole_stack(n: u32) -> Self {
        Self {
            split_size: n,
            max_split_size: n,
        }
    }

    #[must_use]
    pub fn is_whole_stack(self) -> bool {
        self.split_size >= self.max_split_size
    }
}

impl World {
    /// Selection and live stack-size changes seed the shared quantity once. Selecting nothing or
    /// an unknown object leaves the previous pair alone; a vendor's stack begins at one item.
    pub fn refresh_stack_split(&mut self) {
        let edge = self.split_selection != self.selected;
        self.split_selection = self.selected;
        let Some(item) = self.selected.and_then(|id| self.weenie(id)) else {
            return;
        };
        let count = u32::from(item.pwd.stack_size.unwrap_or(1)).max(1);
        if !edge && count == self.split.max_split_size {
            return;
        }
        let vendor_stock = self
            .vendor_id()
            .filter(|id| id.0 != 0)
            .is_some_and(|id| item.pwd.container_id == Some(id))
            && item.pwd.obj_type & 0x0dc4_1cb0 != 0;
        self.split = SplitState {
            split_size: if vendor_stock { 1 } else { count },
            max_split_size: count,
        };
    }

    /// Check whether the player is ready to make an inventory request.
    ///
    /// # Errors
    /// The client's own refusal string.
    pub fn ready_for_inventory_request(&self, quiet: bool) -> Result<(), &'static str> {
        let r = ready_for_inventory_request(&self.request_lock, self.attack_in_progress);
        // `quiet` only suppresses the message; the refusal is the same either way. The caller
        // decides whether to display, which is why nothing is emitted here.
        let _ = quiet;
        r
    }

    fn refuse(&self, out: &mut dyn NoticeSink, quiet: bool, msg: &str) {
        if !quiet {
            out.emit(Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: FEEDBACK_CHANNEL,
                text: msg.to_string(),
            });
        }
    }

    /// Attempt to wield the object at `loc` from the UI.
    ///
    /// Note the wield/split asymmetry: when the item is a stack and only part of it is being moved,
    /// the client sends `Request::StackableSplitToWield` (0x019B) and records **no** previous
    /// request, so a split-wield does not block the next action. `\[verified\]`; it looks like an
    /// oversight and a faithful rebuild must reproduce it.
    ///
    /// # Errors
    /// The refusal string when the lock or an attack blocks the request.
    // The argument list is the client's own signature plus the two sinks; bundling it into a
    // parameter struct would hide which of them the original actually takes.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_wield(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        loc: u32,
        split: SplitState,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        let stack_size = self
            .weenie(item)
            .and_then(|w| w.pwd.stack_size)
            .unwrap_or(0);
        if stack_size > 1 && !split.is_whole_stack() {
            req.send(Request::StackableSplitToWield(
                InventoryStackableSplitToWield {
                    stack: item,
                    slot: loc,
                    amount: i32::try_from(split.split_size).unwrap_or(i32::MAX),
                },
            ));
            self.set_waiting(item, true);
            return Ok(());
        }
        req.send(Request::GetAndWieldItem(InventoryGetAndWieldItem {
            item,
            slot: loc,
        }));
        self.set_waiting(item, true);
        self.request_lock.record(item, InventoryRequest::Wield, now);
        Ok(())
    }

    /// Attempt to give `amount` to `target` from the UI.
    ///
    /// # Errors
    /// The refusal string.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_give(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        target: ObjectId,
        amount: u32,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        req.send(Request::GiveObjectRequest(InventoryGiveObjectRequest {
            target,
            item,
            amount,
        }));
        self.set_waiting(item, true);
        self.request_lock.record(item, InventoryRequest::Give, now);
        Ok(())
    }

    /// Attempt to put the object in `container` at `place`.
    ///
    /// Records a pick-up request when the item was in the 3D world, else a put-in-container one.
    ///
    /// # Errors
    /// The refusal string.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_put_in_container(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        container: ObjectId,
        place: u32,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        let in_3d = self.is_in_3d_world(item);
        req.send(Request::PutItemInContainer(InventoryPutItemInContainer {
            item,
            container,
            slot: place,
        }));
        self.set_waiting(item, true);
        self.request_lock.record(
            item,
            if in_3d {
                InventoryRequest::PickUp
            } else {
                InventoryRequest::PutInContainer
            },
            now,
        );
        Ok(())
    }

    /// Attempt to place the object in the 3D world.
    ///
    /// Records `IR_MOVE` when already in the 3D world, else `IR_DROP`.
    ///
    /// # Errors
    /// The refusal string.
    pub fn attempt_put_in_3d(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        let in_3d = self.is_in_3d_world(item);
        req.send(Request::DropItem(InventoryDropItem { item }));
        self.set_waiting(item, true);
        self.request_lock.record(
            item,
            if in_3d {
                InventoryRequest::Move
            } else {
                InventoryRequest::Drop
            },
            now,
        );
        Ok(())
    }

    /// Attempt to merge `amount` into `target`.
    ///
    /// # Errors
    /// The refusal string.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_merge(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        target: ObjectId,
        amount: u32,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        req.send(Request::StackableMerge(InventoryStackableMerge {
            merge_from: item,
            merge_to: target,
            amount: i32::try_from(amount).unwrap_or(i32::MAX),
        }));
        self.set_waiting(item, true);
        self.request_lock.record(item, InventoryRequest::Merge, now);
        Ok(())
    }

    /// Attempt to split `amount` into `container` at `place`.
    ///
    /// Also records the pending-split bookkeeping, whose window is **10 s**.
    ///
    /// # Errors
    /// The refusal string.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_split_to_container(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        container: ObjectId,
        place: u32,
        amount: u32,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        req.send(Request::StackableSplitToContainer(
            InventoryStackableSplitToContainer {
                stack: item,
                container,
                slot: place,
                amount: i32::try_from(amount).unwrap_or(i32::MAX),
            },
        ));
        self.set_waiting(item, true);
        self.request_lock.record(item, InventoryRequest::Split, now);
        self.record_pending_split(item, amount, now);
        Ok(())
    }

    /// Attempt to split `amount` into the 3D world.
    ///
    /// # Errors
    /// The refusal string.
    pub fn attempt_split_to_3d(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        amount: u32,
        now: ServerTime,
        quiet: bool,
    ) -> Result<(), &'static str> {
        if let Err(e) = self.ready_for_inventory_request(quiet) {
            self.refuse(out, quiet, e);
            return Err(e);
        }
        req.send(Request::StackableSplitTo3d(InventoryStackableSplitTo3d {
            stack: item,
            amount: i32::try_from(amount).unwrap_or(i32::MAX),
        }));
        self.set_waiting(item, true);
        self.request_lock.record(item, InventoryRequest::Split, now);
        self.record_pending_split(item, amount, now);
        Ok(())
    }

    /// The appraise send — 0x00C8. **Not** gated by the inventory lock: examination is not
    /// an inventory request.
    pub fn attempt_appraise(&mut self, req: &mut dyn RequestSink, target: ObjectId) {
        req.send(Request::Appraise(ItemAppraise { target }));
    }

    /// The set-inscription send — 0x00BF.
    ///
    /// Like [`Self::attempt_appraise`] it is **not** gated by the inventory lock: writing on a
    /// piece of parchment moves nothing, and calls the
    /// packer straight with no lock of its own.
    ///
    /// The two guards the client puts in front of this call are the *panel's*, not this
    /// function's, because they are questions about what the edit box is showing —
    /// `text == "" && scribe_name == ""` and `text == inscription`.
    /// See `dereth_ui_screens::panels::examination`.
    pub fn attempt_set_inscription(
        &mut self,
        req: &mut dyn RequestSink,
        object: ObjectId,
        text: &str,
    ) {
        req.send(Request::SetInscription(
            dereth_protocol::trade::WritingSetInscription {
                object_id: object,
                text: text.to_string(),
            },
        ));
    }

    /// Send inventory-use event `0x0036`.
    pub fn attempt_use(&mut self, req: &mut dyn RequestSink, object: ObjectId) {
        req.send(Request::UseEvent(InventoryUseEvent { object }));
    }

    /// The query-item-mana send — 0x0263.
    ///
    /// Sent when the mana bar of an equipped item needs refreshing. Like
    /// [`Self::attempt_appraise`], it is **not** gated by the inventory lock: it asks a question
    /// and moves nothing.
    pub fn query_item_mana(&mut self, req: &mut dyn RequestSink, object: ObjectId) {
        req.send(Request::QueryItemMana(
            dereth_protocol::items::ItemQueryItemMana { object },
        ));
    }

    /// `Request::UseWithTargetEvent` — 0x0035.
    pub fn attempt_use_with_target(
        &mut self,
        req: &mut dyn RequestSink,
        object: ObjectId,
        target: ObjectId,
    ) {
        req.send(Request::UseWithTargetEvent(InventoryUseWithTargetEvent {
            object,
            target,
        }));
    }

    fn set_waiting(&mut self, item: ObjectId, waiting: bool) {
        if let Some(w) = self.weenie_mut(item) {
            w.set_waiting_state(waiting);
        }
    }

    /// Behavior: for the callers outside this module that
    /// have to undo a ghost.
    ///
    /// There is exactly one such caller and it is the reason this is public:
    /// The drop handler ghosts the icon before asking this method and answers a refusal with
    /// `set_waiting_state(item, false)` — the drop
    /// is declined and the item stays visibly where it was. Nothing else in the drop path can
    /// clear that ghost, because the server was never told (the wait has no timeout).
    pub fn set_waiting_state(&mut self, item: ObjectId, waiting: bool) {
        self.set_waiting(item, waiting);
    }

    /// The pending-row insert, plus the `set_waiting_state(row, true)` that
    /// always precedes it — once on the drag path and once on
    /// the notice path.
    ///
    /// The object half comes first because that is the order the client
    /// writes them in (the weenie first, then the element) and because the single mirror is
    /// object-driven: a row whose
    /// element was ghosted without its object would have no edge to follow back.
    ///
    /// A list holds one pending row at a time (a second drop on a list that
    /// already has one), and this build holds one for the whole world: the player has one pointer
    /// and one drag.
    pub fn set_pending_row(&mut self, row: PendingRow) {
        self.set_waiting(row.item, true);
        self.pending_row = Some(row);
    }

    /// Delete a pending item by clearing its UI item and updating its state,
    /// (`0x1000001C`, the empty frame) and clearing the pending item. The row is dropped; the object's
    /// `waiting` is **not** touched, because every native caller of this either follows it with a
    /// refill that re-reads the object or has already cleared the
    /// object itself (the place-in-backpack path).
    ///
    /// Returns whether a row was actually removed.
    pub fn delete_pending_item(&mut self) -> bool {
        self.pending_row.take().is_some()
    }

    /// The client's head, run for the two notices that refill
    /// a list, and both end
    /// in the item list's set-parent-container with refill, and that refill is the flush.
    ///
    /// Both notices reach the flush by the same two-armed test, transcribed here:
    ///
    /// ```text
    /// if (old_container == parent_container || new_container == parent_container) refill;
    /// else if (the list holds moved)                                              refill;
    /// if (failed == parent_container)                                             refill;
    /// else if (any row's item == failed)                                          refill;
    /// ```
    ///
    /// The second arm of each is what makes the pending row self-healing: **the pending row is a
    /// row of that list**, so any answer naming the pending object finds it, whatever container
    /// the object actually ended up in. That covers `attempt_to_place_in_container`'s spill
    /// where the item lands in a side pack and neither container term
    /// matches.
    pub(crate) fn flush_pending_row_for(&mut self, item: ObjectId, containers: [ObjectId; 2]) {
        let Some(p) = self.pending_row else { return };
        if p.item == item || containers.contains(&p.container) {
            self.pending_row = None;
        }
    }

    /// Behavior: the *other* producer, and the
    /// consumer of [`crate::Notice::ShowPendingInPlayer`].
    ///
    /// ```text
    ///   no 3D-items UI                                        -> return
    ///   no item list in it                                    -> return
    ///   item == 0                                             -> return
    ///   no weenie for item                                    -> return
    ///   the object is a container                             -> return
    ///   row = insert item into the list at index 0;   no row  -> return
    ///   set the row's waiting state;
    ///   set the row as the pending list item;
    /// ```
    ///
    /// Three things are transcribed exactly. The index is **0**, the head of the grid, not the
    /// drop index — this path has no pointer. A **container is skipped**: the container test returns,
    /// and the side-pack strip never gets a provisional row this way. The list is the 3D-items UI
    /// list, the same one the inventory notice handler reads as *"the grid's parent container"*:
    /// the inventory grid whose `parent_container` is the open container.
    /// That is here, and it is deliberately **not**
    /// the place-in-backpack destination: with `MainPackPreferred` set while a side pack is open,
    /// retail shows the provisional row in the side pack's grid and sends the item to the main
    /// pack, and so does this.
    pub fn show_pending_in_player(&mut self, item: ObjectId) {
        if item.0 == 0 {
            return;
        }
        let Some(w) = self.weenie(item) else { return };
        if w.is_container() {
            return;
        }
        let Some(container) = self.open_container.or(self.player) else {
            return;
        };
        self.set_pending_row(PendingRow {
            item,
            container,
            containers_list: false,
            index: 0,
        });
    }

    /// The inventory panel's end-pending notice — the pending row is deleted,
    /// then the row's waiting state comes off. The client raises it
    /// when the place-in-container attempt refused, having already run
    /// the weenie's own waiting-state clear.
    pub fn end_pending_in_player(&mut self) -> bool {
        self.delete_pending_item()
    }

    /// The object's current state is in the 3D world: no container and no wield location.
    #[must_use]
    pub fn is_in_3d_world(&self, item: ObjectId) -> bool {
        self.weenie(item).is_some_and(|w| {
            w.pwd.container_id.unwrap_or_default().0 == 0 && w.pwd.location.unwrap_or(0) == 0
        })
    }

    fn record_pending_split(&mut self, item: ObjectId, amount: u32, now: ServerTime) {
        if let Some(w) = self.weenie(item) {
            self.pending_split = Some(PendingSplit {
                wcid: w.pwd.wcid,
                stack_size: amount,
                at: now,
            });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Capacity and stacking legality.
    // -----------------------------------------------------------------------------------------

    /// Return the number of empty item slots — **−1 when the item capacity is −1**
    /// (unlimited). The wire type is a signed `int8`.
    #[must_use]
    pub fn num_empty_item_slots(&self, container: ObjectId) -> i32 {
        let Some(w) = self.weenie(container) else {
            return 0;
        };
        let used = self
            .inventory(container)
            .map_or(0, |inv| i32::try_from(inv.items.len()).unwrap_or(i32::MAX));
        dereth_rules::capacity::free_slots(w.pwd.items_capacity.unwrap_or(0), used)
            .unwrap_or(dereth_rules::capacity::UNLIMITED)
    }

    /// Check whether the item will fit in the container.
    ///
    /// The "already in this container" escape is what lets you reorder a full pack.
    #[must_use]
    pub fn will_item_fit_in_container(
        &self,
        item: ObjectId,
        container: ObjectId,
        split: SplitState,
    ) -> bool {
        let Some(c) = self.weenie(container) else {
            return false;
        };
        if item == container {
            return false;
        }
        // Not openable and not the player: refuse.
        if c.pwd.bitfield & crate::weenie::bitfield::OPENABLE == 0 && Some(container) != self.player
        {
            return false;
        }
        if c.trade_state == 1 {
            return false;
        }
        // The fit check runs the hook-status check too, in the
        // same position the placement path does. Outside housing, the hook-status check answers
        // `true` on the first line, so this only ever bites a
        // hook; it also stands in for the `if (!weenie(item)) return 0` beside it, because
        // a missing item weenie is the helper's own `(false, false)`.
        if !self.check_hook_status(item, container).0 {
            return false;
        }
        let item_is_container = self
            .weenie(item)
            .is_some_and(crate::weenie::Weenie::is_container);
        let (cap, count, in_list) = if item_is_container {
            let cap = c.pwd.containers_capacity.unwrap_or(0);
            let inv = self.inventory(container);
            (
                cap,
                inv.map_or(0, |i| i32::try_from(i.containers.len()).unwrap_or(i32::MAX)),
                inv.is_some_and(|i| i.containers.contains(&item)),
            )
        } else {
            let cap = c.pwd.items_capacity.unwrap_or(0);
            let inv = self.inventory(container);
            (
                cap,
                inv.map_or(0, |i| i32::try_from(i.items.len()).unwrap_or(i32::MAX)),
                inv.is_some_and(|i| i.items.contains(&item)),
            )
        };
        if !dereth_rules::capacity::has_room(cap, count) {
            if !item_is_container && !split.is_whole_stack() {
                // A partial split always needs a new slot.
                return false;
            }
            return in_list;
        }
        true
    }

    /// Check whether dragging `dragged` into `container` is legal. The drag-accept path consults
    /// this gate, and it is the gate's **only** caller in retail.
    ///
    /// It is entirely quiet: not one of its ~14 exit arms touches a string or
    /// the local-feedback notice, and a refusal on the drag path lands on the
    /// `idx--` block, **not** past the request — so a drag the gate refuses still sends its
    /// `attempt_to_place_in_container`. The gate decides whether the optimistic row is
    /// drawn and nothing else.
    ///
    /// It is three pieces, each already transcribed here:
    ///
    /// ```text
    /// require inventory readiness with quiet = 1
    /// apply the six quiet item-legality arms
    /// apply the inlined container-fit check, identical to the standalone helper
    /// ```
    ///
    /// **How it relates to the client's own gates.** The placement request runs
    /// `place_in_container_item_legal(item)` (spoken), then
    /// `place_in_container_owner_legal(item, owner)` (spoken), then the spill, whose first two
    /// steps are
    /// `will_item_fit_in_container(item, requested)` and `will_item_fit_in_container(item, owner)`.
    /// The drop handler passes
    /// `owner = container` and `requested = 0`, so **this gate is exactly the spill's second
    /// step**, evaluated early: it fails precisely when the move is going to land somewhere other
    /// than the list the pointer was over — a side pack of the destination. The item-side arms are
    /// the same `place_in_container_item_legal` the request runs a moment later, loudly, so they
    /// can only ever agree.
    #[must_use]
    pub fn is_drag_into_container_attempt_legal(
        &self,
        item: ObjectId,
        container: ObjectId,
        split: SplitState,
    ) -> bool {
        // Quiet is the literal 1, and it is the same lock `attempt_to_place_in_container` takes
        // with `quiet = 0`.
        if self.ready_for_inventory_request(true).is_err() {
            return false;
        }
        // The six legality arms: no weenie / the player himself / `type == CREATURE` /
        // `STUCK` and loose in the world / not owned by the player and wielded / a container with
        // its own container capacity. Quiet here, spoken.
        if self.place_in_container_item_legal(item).is_err() {
            return false;
        }
        // The inlined fit check.
        self.will_item_fit_in_container(item, container, split)
    }

    /// Check whether merging `src` into `dst` is legal.
    ///
    /// # Errors
    /// The refusal string, in the order the client tests them.
    pub fn is_merge_attempt_legal(&self, src: ObjectId, dst: ObjectId) -> Result<(), &'static str> {
        self.ready_for_inventory_request(true)?;
        if src == dst {
            return Err("");
        }
        let (Some(s), Some(d)) = (self.weenie(src), self.weenie(dst)) else {
            return Err("");
        };
        if s.pwd.max_stack_size.unwrap_or(0) <= 1 || d.pwd.max_stack_size.unwrap_or(0) <= 1 {
            return Err("");
        }
        if s.trade_state == 1 || d.trade_state == 1 {
            return Err(requests::messages::MERGE_WHILE_TRADING);
        }
        if s.pwd.wcid != d.pwd.wcid {
            return Err(requests::messages::MERGE_DIFFERENT_TYPES);
        }
        if d.pwd.stack_size.unwrap_or(0) >= d.pwd.max_stack_size.unwrap_or(0) {
            return Err(requests::messages::DESTINATION_STACK_FULL);
        }
        Ok(())
    }

    /// The client's **spill order**: the requested
    /// container, then the owner's main pack, then each side pack in containers list order.
    ///
    /// Observable: without it, auto-loot puts things in different packs.
    #[must_use]
    pub fn spill_target(
        &self,
        item: ObjectId,
        owner: ObjectId,
        container: ObjectId,
        split: SplitState,
    ) -> Option<ObjectId> {
        if self.will_item_fit_in_container(item, container, split) {
            return Some(container);
        }
        if self.will_item_fit_in_container(item, owner, split) {
            return Some(owner);
        }
        let packs = self.inventory(owner).map(|i| i.containers.clone())?;
        packs
            .into_iter()
            .find(|sp| self.will_item_fit_in_container(item, *sp, split))
    }

    /// Behavior: scan the target and its side packs for a
    /// partially-filled stack of the same weenie class id and merge into the **first** one found.
    #[must_use]
    pub fn find_auto_merge_target(&self, item: ObjectId, container: ObjectId) -> Option<ObjectId> {
        let src = self.weenie(item)?;
        if src.pwd.max_stack_size.unwrap_or(0) <= 1 {
            return None;
        }
        let wcid = src.pwd.wcid;
        let mut search = vec![container];
        if let Some(inv) = self.inventory(container) {
            search.extend(inv.containers.iter().copied());
        }
        for c in search {
            let inv = self.inventory(c)?;
            for cand in &inv.items {
                if *cand == item {
                    continue;
                }
                let Some(w) = self.weenie(*cand) else {
                    continue;
                };
                let max = w.pwd.max_stack_size.unwrap_or(0);
                if w.pwd.wcid == wcid && max > 1 && w.pwd.stack_size.unwrap_or(0) < max {
                    return Some(*cand);
                }
            }
        }
        None
    }

    /// Behavior: this container plus every
    /// side pack, flattened.
    #[must_use]
    pub fn exhaustive_contained_items(&self, container: ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let Some(inv) = self.inventory(container) else {
            return out;
        };
        out.extend(inv.items.iter().copied());
        for sp in &inv.containers {
            if let Some(si) = self.inventory(*sp) {
                out.extend(si.items.iter().copied());
            }
        }
        out
    }

    /// Behavior: hoisted so the vendor and trade panels share it.
    #[must_use]
    pub fn is_coin(&self, id: ObjectId) -> bool {
        self.weenie(id)
            .is_some_and(|w| w.inq_type() & item_type::MONEY != 0)
    }

    /// Attempt to merge `item` into `target` — the drag-acceptance path's first act.
    ///
    /// The merge legality check (`is_merge_attempt_legal`) runs first, and a refusal answers 0.
    /// The amount is the selected split size when `item` is the selected object and otherwise its
    /// stack size (at least 1), capped at the target's room, `max_stack_size - max(stack_size, 1)`.
    /// The merge is requested for that amount, a full-merging-item notice is sent for item and
    /// target, the target becomes the selection, and the answer is 1.
    ///
    /// This is the arm that makes dropping five pyreals on a stack of five *merge*
    /// rather than reposition, and it runs **before** any index arithmetic. Returns the client's
    /// own `int`.
    pub fn item_holder_attempt_merge(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        target: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        if self.is_merge_attempt_legal(item, target).is_err() {
            return false;
        }
        let (Some(src), Some(dst)) = (self.weenie(item), self.weenie(target)) else {
            return false;
        };
        let n = if self.selected == Some(item) {
            split.split_size
        } else {
            u32::from(src.pwd.stack_size.unwrap_or(0).max(1))
        };
        let room = u32::from(dst.pwd.max_stack_size.unwrap_or(0))
            .saturating_sub(u32::from(dst.pwd.stack_size.unwrap_or(0).max(1)));
        let amount = n.min(room);
        // The merge attempt passes `quiet` on to its legality test and **not** to the send,
        // which has no such argument: the send's own refusals are always spoken.
        let ok = self
            .attempt_merge(req, out, item, target, amount, now, false)
            .is_ok();
        if ok {
            self.set_selected_object(Some(target), false, out);
        }
        ok
    }

    /// Behavior: **where a dragged item lands.**
    ///
    /// Everything from the merge attempt onwards; the function takes the dragged id and the
    /// flag as two adjacent arguments. The early refusals and the local optimistic insert are the caller's — see
    /// [`ItemListDrop`] for the split.
    ///
    /// 1. A merge onto the object under the pointer (`under`, quiet 1) that succeeds answers `true`.
    /// 2. `idx` is the target's list-box index, clamped to the number of items in the list.
    /// 3. The destination starts as the list's parent container, with auto-merge off. If `under`
    ///    has a weenie: a dragged container re-aims the destination at `under` when `under` has
    ///    container capacity; any other dragged item re-aims it at `under`, with auto-merge on,
    ///    when `under` has item capacity; failing that, if `under` is a container, the drop prints
    ///    *"The %s cannot accept items"* and answers `false`.
    /// 4. If the destination has a weenie, `old` is the dragged item's place in its containers list
    ///    (for a dragged container) or its items list. When `old` is not -1, `idx == old` answers
    ///    `false`, and so does `idx == old + 1` when the split is the whole stack
    ///    (`split_size == max_split_size`).
    /// 5. A dragged item whose trade state is 1 answers `false` unless the split is the whole stack.
    /// 6. When there is a destination, it passes `is_drag_into_container_attempt_legal`, and it is
    ///    the list's parent container, the dragged item is inserted into this list at `idx` (the
    ///    caller's half), marked waiting, and made the pending item.
    /// 7. When `old` is not -1, `old < idx` and the split is the whole stack, `idx` is decremented.
    /// 8. With a destination, the answer is whether
    ///    `attempt_to_place_in_container(dragged, destination, 0, auto_merge, place)` succeeds, where
    ///    `place` is `idx` when the destination is the list's parent container and 0 otherwise.
    ///    With no destination the answer is `false`.
    ///
    /// **The three steps that decide where it lands**, each easy to lose:
    ///
    /// 1. **`place` is the slot index**, not `0`, whenever the destination is the list's own
    ///    container: `(container == parent_container) ? idx : 0`. Passing `0` unconditionally puts
    ///    every dragged item at the **head** of the pack.
    /// 2. **`idx--` when the item is moving *down* its own list**. Removing
    ///    an item from position `old` shifts everything after it down one, so a drop aimed at
    ///    position `idx > old` must land at `idx - 1`. Without it every downward move overshoots by
    ///    one — and a drop at the **head** never exercises it, which is why a one-item fixture
    ///    cannot see this.
    /// 3. **A drop on a plain item is a *reposition*, not a put-in-container.** `container` is only
    ///    re-aimed at the object under the pointer when that object has capacity; a non-container
    ///    that is not a container leaves `container == parent_container` and the drop becomes a
    ///    move to that index. Sending `Request::PutItemInContainer(item, under, 0)` instead is
    ///    a request no server can answer, so the inventory lock (which has **no timeout**, and
    ///    correctly so) would never be released and the icon would stay ghosted.
    ///
    /// Returns the drop handler's own `bool`. `false` is a refusal and the caller answers it with
    /// `set_waiting_state(item, false)` — the drop release's own tail.
    #[allow(clippy::too_many_arguments)]
    pub fn item_list_accept_drag(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        item: ObjectId,
        drop: ItemListDrop,
        split: SplitState,
        now: ServerTime,
    ) -> bool {
        // **The drag-accept's own head, and it is the first thing it refuses.**
        //
        // ```text
        //   load the dragged object
        //   if the list's parent container is not the dragged id, continue with normal acceptance
        //   an unknown object also continues with normal acceptance
        //   run the weenie's is-player test
        //   a known non-player object also continues with normal acceptance
        //   display "You cannot place yourself in your inventory!" on channel 0x1A
        //   return false
        // ```
        //
        // The player-object test compares the object's id with the player system's player id.
        // The shipped gesture uses a list whose single row **is** the player, so dragging it into
        // his own pack grid — whose
        // `parent_container` is also the player — is exactly this arm.
        //
        // Without it the drop would run on to `attempt_to_place_in_container`, whose
        // `place_in_container_item_legal` arm 3 says *"You cannot place yourself within a
        // container!"* instead —
        // a different sentence, from a different function, three gates later.
        if drop.parent_container == item && self.weenie(item).is_some() && self.player == Some(item)
        {
            self.refuse(out, false, "You cannot place yourself in your inventory!");
            return false;
        }

        // **The list's kind against the drag's own.**
        //
        // ```text
        // if this is a container list:
        //     accept a container, or a plain item dropped onto a pack row
        //     otherwise refuse with "Cannot place item in container list"
        // else if the dragged object is a container:
        //     refuse with "Cannot place container in item list"
        // ```
        //
        // The escape is the load-bearing half: a plain item is refused by the side-pack strip
        // only over an **empty** slot;
        // over a pack it is the ordinary "put it in that pack" drop, which re-aims the destination
        // and draws no provisional row.
        //
        // The slot's is-container flag is cached from the UI item, and the client fills it
        // with its own three-term expression — `(bitfield & 0x800000) || items_capacity ||
        // containers_capacity` — **not** with the client's two.
        // [`crate::weenie::Weenie::is_container`] is that expression, term for term, which is why
        // the element's cached copy is read here off the object instead of being threaded through
        // [`ItemListDrop`].
        if drop.container_list {
            let under_is_container = drop
                .under
                .is_some_and(|u| self.weenie(u).is_some_and(Weenie::is_container));
            if !drop.dragged_is_container && !under_is_container {
                self.refuse(out, false, "Cannot place item in container list");
                return false;
            }
        } else if drop.dragged_is_container {
            self.refuse(out, false, "Cannot place container in item list");
            return false;
        }

        // **The pending-row gate.** With the three above it this is the fourth gate
        // and the last before the merge attempt.
        //
        // ```text
        // if there is no pending item, continue to the merge attempt
        // read the pending row's item id, not the newly dragged id
        // format its appropriate name into "Already attempting to place %s here"
        // display the message on channel 0x1A and refuse the drop
        // ```
        //
        // It sits **before** the merge attempt, before the split count is read, and before the destination is
        // resolved at all — therefore before either indirect inventory-lock check. The drag handler
        // never calls the lock itself; it reaches it inside the legality gate (`quiet = 1`, silent)
        // and inside the final placement request (`quiet = 0`, loud). So a second drop on a list
        // that is already showing a provisional
        // row hears **this** line and never the lock's
        // [`crate::inventory::requests::BUSY_MESSAGE`].
        //
        // Pending placement is per item-list element, and [`PendingRow`] identifies its list with the
        // same two facts the element does: its parent container and
        // container-list kind. The test is that pair, so a drop on any *other* list is untouched — it
        // goes on to meet the lock, quietly and loudly, exactly as the native request does.
        //
        // The `%s` names the object **already** being placed, read from the pending row rather than
        // the one just dropped. It uses the appropriate-name form, the same form every use-path
        // literal takes, without the backpack alias, so the `Backpack` alias is **not** in play
        // here.
        if let Some(row) = self.pending_row {
            if row.container == drop.parent_container && row.containers_list == drop.container_list
            {
                let name = self.notice_name(row.item);
                self.refuse(
                    out,
                    false,
                    &format!("Already attempting to place {name} here"),
                );
                return false;
            }
        }

        // The merge of `dragged` onto `under`, quiet — `under` is 0 for an empty slot and
        // the legality test refuses a null target, so the empty-slot case falls straight
        // through, exactly as the client's does.
        if let Some(under) = drop.under {
            if self.item_holder_attempt_merge(req, out, item, under, split, now) {
                return true;
            }
        }

        let mut idx = drop.index.min(drop.num_ui_items);
        let mut auto_merge = false;
        let mut container = drop.parent_container;
        if let Some(under) = drop.under {
            if let Some(tw) = self.weenie(under) {
                if drop.dragged_is_container {
                    if tw.pwd.containers_capacity.unwrap_or(0) != 0 {
                        container = under;
                    }
                } else if tw.pwd.items_capacity.unwrap_or(0) != 0 {
                    container = under;
                    auto_merge = true;
                } else if tw.is_container() {
                    // "The %s cannot accept items", retail's literal.
                    let name = tw.object_name(NameType::Singular);
                    self.refuse(out, false, &format!("The {name} cannot accept items"));
                    return false;
                }
            }
        }

        // The place-in-items-list / place-in-containers-list pair — **the one list
        // the dragged object belongs in**, chosen by the drag's own is-a-container flag
        // and not by the list that was dropped on. Asking the other list returns an index into it,
        // which is a silently wrong slot.
        let old = self
            .inventory(container)
            .and_then(|inv| inv.place_in_list(item, drop.dragged_is_container))
            .and_then(|p| u32::try_from(p).ok());
        if let Some(old) = old {
            // Dropped back where it already is.
            if idx == old {
                return false;
            }
            // Dropped on the slot immediately after itself: with the whole stack in hand that is a
            // no-op too, because the `idx--` below would take it straight back to `old`.
            if idx == old + 1 && split.is_whole_stack() {
                return false;
            }
        }
        if self.weenie(item).is_some_and(|w| w.trade_state == 1) && !split.is_whole_stack() {
            return false;
        }

        // **The optimistic row, and it goes in BEFORE the request.**
        //
        // ```text
        //   a missing container skips optimistic insertion
        //   an illegal container move skips optimistic insertion
        //   only this list's parent container receives the optimistic row
        //   insert the dragged object at idx in this list
        //   set the row's waiting state
        //   pending_item = the row
        //   idx--                         ; ...and only NOW is idx adjusted
        //   send the place-in-container request
        // ```
        //
        // Three things are load-bearing and all three are here. The insert uses the **un-adjusted**
        // `idx`: the decrement runs after the insert and feeds only the request's `place`
        // argument. `container == parent_container` is the whole per-target rule — a drop
        // aimed at a *container row* re-aims `container` at that pack, which is not the list under
        // the pointer, so retail draws **no** provisional row anywhere. And the row is inserted
        // even though the request may still be refused, which is why the refusal below deletes it.
        //
        // The drag-into-container legality test is consulted here, in retail's
        // own position and order: `container != 0`, then the call, then
        // `container == parent_container`.
        //
        // What it adds over the request's own gates is one rule: the row is drawn only if the
        // dragged object fits **in this list's container**, not in some side pack the move will
        // spill into. Without it a pack already at item capacity would draw a provisional row in
        // the grid the pointer was over while the item actually travelled to a side pack — a row
        // retail never shows. The request is *not* skipped: the refusal jumps to the
        // `idx--` block, and the send still runs.
        let pending = (container.0 != 0
            && self.is_drag_into_container_attempt_legal(item, container, split)
            && container == drop.parent_container)
            .then_some(PendingRow {
                item,
                container,
                containers_list: drop.container_list,
                index: idx,
            });

        if let Some(old) = old {
            if old < idx && split.is_whole_stack() {
                idx -= 1;
            }
        }

        if container.0 == 0 {
            return false;
        }
        if let Some(row) = pending {
            self.set_pending_row(row);
        }
        let place = if container == drop.parent_container {
            idx
        } else {
            0
        };
        let sent = self.attempt_to_place_in_container(
            req,
            out,
            item,
            container,
            ObjectId(0),
            auto_merge,
            place,
            split,
            now,
        );
        // **Declared deviation, and the deliberate one.** Retail leaves the provisional row in
        // place here: it answers the drop handler's `false` by clearing only the object's
        // waiting state, so a drop refused *after* the insert
        // (a full pack, a locked inventory request, the hook rules) leaves an un-ghosted ghost
        // row until that list is next refilled from the model. In this build no refill would ever
        // come — the panels fill from this world and the row is part of what they fill — so the
        // quirk would be a permanently stuck tile. The row is dropped instead.
        if !sent && pending.is_some() {
            self.delete_pending_item();
        }
        sent
    }
}

/// The four facts read off the **element tree**,
/// which is the half of it `dereth_client_model` cannot see.
///
/// The screen fills these in from the list the drop landed on
/// (the inventory panels in `dereth_ui_screens`); everything else
/// the drop handler needs is a weenie and is read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemListDrop {
    /// The object whose contents this list is showing.
    pub parent_container: ObjectId,
    /// The item in the pointed-to slot; `None` for an empty slot.
    pub under: Option<ObjectId>,
    /// Behavior: the pointer's slot index in the list's items.
    pub index: u32,
    /// Behavior: the clamp on [`Self::index`].
    pub num_ui_items: u32,
    /// The drag's is-a-container flag — the **dragged** object is a pack.
    /// It selects container capacity/containers list over item capacity/items list
    /// everywhere in the drop handler, and it comes off the drag proxy rather than off the list.
    pub dragged_is_container: bool,
    /// Whether this is a container list — the server-confirmed move path uses it to select a
    /// list's identity. Read only by [`PendingRow`], which
    /// has to name the list holding the pending item; the drop handler's own decisions
    /// all use [`Self::dragged_is_container`] instead.
    pub container_list: bool,
}

/// One optimistic pending row, together with the list it is in.
///
/// Retail's copy is a UI-item pointer hanging off the list element, so the list it belongs to
/// is implicit. This build rebuilds every list from the world each changed frame, so the row has
/// to name its list, and a list in this build is exactly two facts: its parent container
/// and list kind — the same pair used to distinguish item-list placement from
/// container-list placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingRow {
    /// The object the provisional row shows. The row is created with this id; setting its waiting
    /// state writes the object half first, so
    /// the row draws ghosted through the ordinary mirror.
    pub item: ObjectId,
    /// The container whose contents that list shows.
    pub container: ObjectId,
    /// The list kind: the side-pack strip and a chest's pack strip are container lists,
    /// the item grids are not.
    pub containers_list: bool,
    /// The index the list insert of the dragged item was given —
    /// clamped by, read **before**
    /// the `idx--`, which only adjusts the *request's* place argument.
    pub index: u32,
}

/// The pending stack split: the split's weenie class id, stack size and time. The window is
/// **10 s**.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingSplit {
    pub wcid: u32,
    pub stack_size: u32,
    pub at: ServerTime,
}

/// The pending-split window, in seconds.
pub const SPLIT_WINDOW: f64 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RecordingRequests, RecordingSink};
    use dereth_protocol::types::{ContentProfile, PublicWeenieDesc};

    fn world_with(items: &[(u32, PublicWeenieDesc)]) -> World {
        let mut w = World::new();
        for (id, pwd) in items {
            let mut wn = crate::weenie::Weenie::new(ObjectId(*id));
            wn.pwd = pwd.clone();
            wn.valid = true;
            w.tables.weenies.insert(ObjectId(*id), wn);
        }
        w
    }

    fn stack(wcid: u32, n: u16, max: u16) -> PublicWeenieDesc {
        PublicWeenieDesc {
            name: "Pyreal".into(),
            wcid,
            stack_size: Some(n),
            max_stack_size: Some(max),
            ..PublicWeenieDesc::default()
        }
    }

    /// Oracle: §7's sender table — each `UIAttempt*` emits the documented opcode and records the
    /// documented `InventoryRequest`.
    #[test]
    fn every_sender_emits_its_opcode_and_records_its_request_type() {
        use dereth_protocol::{Message, Opcode};
        let mut w = world_with(&[(7, PublicWeenieDesc::default())]);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        let t = ServerTime(1.0);

        w.attempt_wield(
            &mut req,
            &mut out,
            ObjectId(7),
            0x0010_0000,
            SplitState::default(),
            t,
            false,
        )
        .unwrap();
        assert_eq!(InventoryGetAndWieldItem::OPCODE, Opcode(0x001A));
        assert_eq!(w.request_lock.pending, InventoryRequest::Wield);
        assert!(
            w.weenie(ObjectId(7)).unwrap().waiting,
            "the icon is ghosted, nothing is applied"
        );
        w.request_lock.clear();

        w.attempt_give(&mut req, &mut out, ObjectId(7), ObjectId(9), 1, t, false)
            .unwrap();
        assert_eq!(InventoryGiveObjectRequest::OPCODE, Opcode(0x00CD));
        assert_eq!(w.request_lock.pending, InventoryRequest::Give);
        w.request_lock.clear();

        // In the 3D world -> a pick-up request; in a container -> a put-in-container request.
        w.attempt_put_in_container(&mut req, &mut out, ObjectId(7), ObjectId(2), 0, t, false)
            .unwrap();
        assert_eq!(InventoryPutItemInContainer::OPCODE, Opcode(0x0019));
        assert_eq!(w.request_lock.pending, InventoryRequest::PickUp);
        w.request_lock.clear();
        w.weenie_mut(ObjectId(7)).unwrap().pwd.container_id = Some(ObjectId(2));
        w.attempt_put_in_container(&mut req, &mut out, ObjectId(7), ObjectId(3), 0, t, false)
            .unwrap();
        assert_eq!(w.request_lock.pending, InventoryRequest::PutInContainer);
        w.request_lock.clear();

        // In a container -> IR_DROP; in the 3D world -> IR_MOVE.
        w.attempt_put_in_3d(&mut req, &mut out, ObjectId(7), t, false)
            .unwrap();
        assert_eq!(InventoryDropItem::OPCODE, Opcode(0x001B));
        assert_eq!(w.request_lock.pending, InventoryRequest::Drop);
        w.request_lock.clear();

        w.attempt_merge(&mut req, &mut out, ObjectId(7), ObjectId(8), 3, t, false)
            .unwrap();
        assert_eq!(InventoryStackableMerge::OPCODE, Opcode(0x0054));
        assert_eq!(w.request_lock.pending, InventoryRequest::Merge);
        w.request_lock.clear();

        w.attempt_split_to_container(&mut req, &mut out, ObjectId(7), ObjectId(2), 0, 3, t, false)
            .unwrap();
        assert_eq!(InventoryStackableSplitToContainer::OPCODE, Opcode(0x0055));
        assert_eq!(w.request_lock.pending, InventoryRequest::Split);
        assert_eq!(
            w.pending_split.unwrap().stack_size,
            3,
            "the split size is the request amount"
        );
        w.request_lock.clear();

        w.attempt_split_to_3d(&mut req, &mut out, ObjectId(7), 3, t, false)
            .unwrap();
        assert_eq!(InventoryStackableSplitTo3d::OPCODE, Opcode(0x0056));
        assert_eq!(w.request_lock.pending, InventoryRequest::Split);
        assert_eq!(
            w.pending_split.unwrap().stack_size,
            3,
            "the 3D writer uses the same amount"
        );
    }

    /// The declaration validator checks the class and normalized stack size before its strict
    /// expiry fallback. A matching create therefore wins even when this invocation is late; an
    /// expired non-match clears the pending split so a later matching create cannot steal focus.
    #[test]
    fn declare_valid_selects_the_exact_pending_split_and_expires_only_after_a_miss() {
        let candidates = [
            (1, stack(0x100, 5, 100)),
            (2, stack(0x100, 4, 100)),
            (3, stack(0x200, 5, 100)),
            (4, stack(0x100, 0, 100)),
        ];
        let mut out = RecordingSink::default();

        let mut matching = world_with(&candidates);
        matching.selected = Some(ObjectId(99));
        matching.pending_split = Some(PendingSplit {
            wcid: 0x100,
            stack_size: 5,
            at: ServerTime(0.0),
        });
        matching.declare_valid(ObjectId(1), ServerTime(20.0), &mut out);
        assert_eq!(
            matching.selected,
            Some(ObjectId(1)),
            "the match precedes the expiry branch"
        );
        assert!(matching.pending_split.is_none());

        let mut expired = world_with(&candidates);
        expired.selected = Some(ObjectId(99));
        expired.pending_split = Some(PendingSplit {
            wcid: 0x100,
            stack_size: 5,
            at: ServerTime(0.0),
        });
        expired.declare_valid(ObjectId(2), ServerTime(SPLIT_WINDOW), &mut out);
        assert!(
            expired.pending_split.is_some(),
            "exactly ten seconds is still inside the window"
        );
        expired.declare_valid(ObjectId(3), ServerTime(SPLIT_WINDOW + 0.001), &mut out);
        assert!(
            expired.pending_split.is_none(),
            "a later non-match reaches the strict expiry"
        );
        expired.declare_valid(ObjectId(1), ServerTime(SPLIT_WINDOW + 0.002), &mut out);
        assert_eq!(
            expired.selected,
            Some(ObjectId(99)),
            "an expired split cannot select later"
        );

        let mut normalized = world_with(&candidates);
        normalized.pending_split = Some(PendingSplit {
            wcid: 0x100,
            stack_size: 1,
            at: ServerTime(0.0),
        });
        normalized.declare_valid(ObjectId(4), ServerTime(0.0), &mut out);
        assert_eq!(
            normalized.selected,
            Some(ObjectId(4)),
            "retail normalizes a zero stack to one"
        );
    }

    /// Oracle: §7's wield/split asymmetry — `Request::StackableSplitToWield` records **no**
    /// previous request. Verified against retail and reproduced deliberately.
    #[test]
    fn a_partial_split_wield_records_no_request_and_so_does_not_block() {
        let mut w = world_with(&[(7, stack(0x100, 10, 100))]);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        w.attempt_wield(
            &mut req,
            &mut out,
            ObjectId(7),
            0x0010_0000,
            SplitState {
                split_size: 3,
                max_split_size: 10,
            },
            ServerTime(1.0),
            false,
        )
        .unwrap();
        assert!(matches!(req.0[0], Request::StackableSplitToWield(_)));
        assert!(
            w.request_lock.is_idle(),
            "the split-wield hole: no previous request, so the next action is not blocked"
        );
        assert_eq!(w.ready_for_inventory_request(true), Ok(()));
    }

    /// Oracle: §6 — one request in flight *globally*, and the exact refusal string.
    #[test]
    fn a_second_request_is_refused_with_the_exact_string() {
        let mut w = world_with(&[
            (7, PublicWeenieDesc::default()),
            (8, PublicWeenieDesc::default()),
        ]);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        w.attempt_wield(
            &mut req,
            &mut out,
            ObjectId(7),
            1,
            SplitState::default(),
            ServerTime(1.0),
            false,
        )
        .unwrap();
        let e = w
            .attempt_put_in_3d(&mut req, &mut out, ObjectId(8), ServerTime(2.0), false)
            .unwrap_err();
        assert_eq!(e, "You can only move or use one item at a time");
        assert_eq!(req.0.len(), 1, "the second request is not sent at all");
        assert!(matches!(
            out.0.last(),
            Some(Notice::DisplayString { channel: 0x1A, .. })
        ));
    }

    /// Oracle: the recovered capacity checks, including the "already in this container"
    /// escape and the `-1 = unlimited` capacity.
    #[test]
    fn capacity_checks_follow_the_clients_escapes() {
        let mut container = PublicWeenieDesc {
            bitfield: crate::weenie::bitfield::OPENABLE,
            items_capacity: Some(2),
            ..PublicWeenieDesc::default()
        };
        let mut w = world_with(&[
            (1, container.clone()),
            (10, PublicWeenieDesc::default()),
            (11, PublicWeenieDesc::default()),
            (12, PublicWeenieDesc::default()),
        ]);
        let mut out = RecordingSink::default();
        w.view_object_contents(
            ObjectId(1),
            &[
                ContentProfile {
                    iid: ObjectId(10),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(11),
                    container_properties: 0,
                },
            ],
            &mut out,
        );
        let whole = SplitState::whole_stack(1);
        assert_eq!(w.num_empty_item_slots(ObjectId(1)), 0);
        assert!(
            !w.will_item_fit_in_container(ObjectId(12), ObjectId(1), whole),
            "full"
        );
        assert!(
            w.will_item_fit_in_container(ObjectId(10), ObjectId(1), whole),
            "already in this container: it fits, which is how you reorder a full pack"
        );
        assert!(
            !w.will_item_fit_in_container(
                ObjectId(10),
                ObjectId(1),
                SplitState {
                    split_size: 1,
                    max_split_size: 5
                }
            ),
            "a partial split always needs a new slot"
        );

        // -1 means unlimited.
        #[allow(clippy::cast_sign_loss)]
        {
            container.items_capacity = Some(-1i8 as u8);
        }
        w.weenie_mut(ObjectId(1)).unwrap().pwd = container;
        assert_eq!(w.num_empty_item_slots(ObjectId(1)), -1);
        assert!(w.will_item_fit_in_container(ObjectId(12), ObjectId(1), whole));

        // A container that is neither openable nor the player refuses everything.
        w.weenie_mut(ObjectId(1)).unwrap().pwd.bitfield = 0;
        assert!(!w.will_item_fit_in_container(ObjectId(12), ObjectId(1), whole));
    }

    /// The drag gate is is item legal plus will item fit.
    #[test]
    fn the_drag_gate_is_is_item_legal_plus_will_item_fit() {
        let chest = PublicWeenieDesc {
            bitfield: crate::weenie::bitfield::OPENABLE,
            items_capacity: Some(2),
            containers_capacity: Some(0),
            ..PublicWeenieDesc::default()
        };
        let mut w = world_with(&[
            (1, chest),
            (10, PublicWeenieDesc::default()),
            (12, PublicWeenieDesc::default()),
            (
                20,
                PublicWeenieDesc {
                    items_capacity: Some(10),
                    ..PublicWeenieDesc::default()
                },
            ),
            (30, PublicWeenieDesc::default()),
        ]);
        w.player = Some(ObjectId(30));
        let mut out = RecordingSink::default();
        w.view_object_contents(
            ObjectId(1),
            &[ContentProfile {
                iid: ObjectId(10),
                container_properties: 0,
            }],
            &mut out,
        );
        let whole = SplitState::whole_stack(1);
        let legal = |w: &World, item: u32| {
            w.is_drag_into_container_attempt_legal(ObjectId(item), ObjectId(1), whole)
        };

        // The control: one free slot in an openable chest.
        assert!(
            legal(&w, 12),
            "the legal drop is the whole point of the gate"
        );

        // The inventory lock, quiet (literal 1).
        w.request_lock.pending = InventoryRequest::PutInContainer;
        assert!(
            !legal(&w, 12),
            "a pending inventory request blocks the drag quietly"
        );
        w.request_lock.clear();
        assert!(legal(&w, 12));

        // No weenie at all.
        assert!(!legal(&w, 99), "weenie(dragged) == 0");

        // The local player himself.
        assert!(!legal(&w, 30), "dragged object is the local player");

        // `type == 0x10`, tested for equality and not as a mask.
        w.weenie_mut(ObjectId(12)).unwrap().pwd.obj_type = item_type::CREATURE;
        assert!(
            !legal(&w, 12),
            "a creature cannot be dragged as an inventory item"
        );
        w.weenie_mut(ObjectId(12)).unwrap().pwd.obj_type = 0;

        // STUCK, and neither contained nor wielded.
        w.weenie_mut(ObjectId(12)).unwrap().pwd.bitfield = crate::weenie::bitfield::STUCK;
        assert!(
            !legal(&w, 12),
            "a stuck object loose in the 3D world is illegal"
        );
        w.weenie_mut(ObjectId(12)).unwrap().pwd.container_id = Some(ObjectId(30));
        assert!(legal(&w, 12), "a stuck but contained object is legal");
        w.weenie_mut(ObjectId(12)).unwrap().pwd.bitfield = 0;
        w.weenie_mut(ObjectId(12)).unwrap().pwd.container_id = None;

        // Someone else's, and wielded. The foreign-owner arm.
        w.weenie_mut(ObjectId(12)).unwrap().pwd.location = Some(0x0010_0000);
        assert!(
            !legal(&w, 12),
            "a wielded object owned by someone else is illegal"
        );
        w.weenie_mut(ObjectId(12)).unwrap().pwd.container_id = Some(ObjectId(30));
        assert!(
            legal(&w, 12),
            "the player's own wielded item passes the same arm"
        );
        w.weenie_mut(ObjectId(12)).unwrap().pwd.location = None;
        w.weenie_mut(ObjectId(12)).unwrap().pwd.container_id = None;

        // A container that can itself hold containers never goes in anything.
        w.weenie_mut(ObjectId(12)).unwrap().pwd.containers_capacity = Some(1);
        assert!(
            !legal(&w, 12),
            "a container that can hold containers cannot be nested"
        );
        w.weenie_mut(ObjectId(12)).unwrap().pwd.containers_capacity = None;

        // The destination, and self-containment.
        assert!(
            !w.is_drag_into_container_attempt_legal(ObjectId(12), ObjectId(98), whole),
            "a missing destination object is illegal"
        );
        assert!(
            !w.is_drag_into_container_attempt_legal(ObjectId(12), ObjectId(12), whole),
            "an object cannot contain itself"
        );

        // Not openable and not the player.
        w.weenie_mut(ObjectId(1)).unwrap().pwd.bitfield = 0;
        assert!(!legal(&w, 12), "a non-openable destination is illegal");
        w.weenie_mut(ObjectId(1)).unwrap().pwd.bitfield = crate::weenie::bitfield::OPENABLE;

        // The destination is in a trade.
        w.weenie_mut(ObjectId(1)).unwrap().trade_state = 1;
        assert!(
            !legal(&w, 12),
            "a destination participating in trade is illegal"
        );
        w.weenie_mut(ObjectId(1)).unwrap().trade_state = 0;

        // The item capacity, the reorder escape, and the split rule.
        w.view_object_contents(
            ObjectId(1),
            &[
                ContentProfile {
                    iid: ObjectId(10),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(12),
                    container_properties: 0,
                },
            ],
            &mut out,
        );
        assert!(!legal(&w, 20), "a new item cannot enter a full item list");
        assert!(
            legal(&w, 12),
            "an existing row may be reordered in a full item list"
        );
        assert!(
            !w.is_drag_into_container_attempt_legal(
                ObjectId(12),
                ObjectId(1),
                SplitState {
                    split_size: 1,
                    max_split_size: 5
                }
            ),
            "a partial split needs a new slot of its own"
        );

        // The *containers* fork, chosen by whether the dragged object is a container and
        // not by which list caught the drop. The chest's container capacity is 0, so a pack
        // may never enter it even while its item slots are free.
        w.view_object_contents(ObjectId(1), &[], &mut out);
        assert!(legal(&w, 12), "the chest is empty again");
        assert!(
            !legal(&w, 20),
            "a container cannot enter a destination that refuses containers"
        );
        w.weenie_mut(ObjectId(1)).unwrap().pwd.containers_capacity = Some(1);
        assert!(
            legal(&w, 20),
            "the container is legal when one container slot is free"
        );
    }

    /// Oracle: the recovered merge-legality refusals, in the order the client tests.
    #[test]
    fn merge_legality_returns_the_documented_strings_in_order() {
        let mut w = world_with(&[
            (1, stack(0x100, 5, 100)),
            (2, stack(0x100, 5, 100)),
            (3, stack(0x200, 5, 100)),
            (4, stack(0x100, 100, 100)),
            (
                5,
                PublicWeenieDesc {
                    max_stack_size: Some(1),
                    ..PublicWeenieDesc::default()
                },
            ),
        ]);
        assert_eq!(w.is_merge_attempt_legal(ObjectId(1), ObjectId(2)), Ok(()));
        assert_eq!(w.is_merge_attempt_legal(ObjectId(1), ObjectId(1)), Err(""));
        assert_eq!(w.is_merge_attempt_legal(ObjectId(1), ObjectId(5)), Err(""));
        assert_eq!(
            w.is_merge_attempt_legal(ObjectId(1), ObjectId(3)),
            Err("You cannot merge different types of items.")
        );
        assert_eq!(
            w.is_merge_attempt_legal(ObjectId(1), ObjectId(4)),
            Err("The destination stack is already full.")
        );
        w.weenie_mut(ObjectId(2)).unwrap().trade_state = 1;
        assert_eq!(
            w.is_merge_attempt_legal(ObjectId(1), ObjectId(2)),
            Err("You cannot merge items while they are being traded.")
        );
    }

    /// Oracle: §9's `attempt_to_place_in_container` — requested container, then the owner's main pack,
    /// then each side pack in side-pack-list order.
    #[test]
    fn the_spill_order_is_requested_then_main_pack_then_side_packs_in_order() {
        let full = |cap: u8| PublicWeenieDesc {
            bitfield: crate::weenie::bitfield::OPENABLE,
            items_capacity: Some(cap),
            ..PublicWeenieDesc::default()
        };
        let mut w = world_with(&[
            (1, full(0)), // requested: full
            (2, full(0)), // owner main pack: full
            (3, full(0)), // side pack A: full
            (4, full(4)), // side pack B: room
            (99, PublicWeenieDesc::default()),
        ]);
        let mut out = RecordingSink::default();
        w.view_object_contents(
            ObjectId(2),
            &[
                ContentProfile {
                    iid: ObjectId(3),
                    container_properties: 1,
                },
                ContentProfile {
                    iid: ObjectId(4),
                    container_properties: 1,
                },
            ],
            &mut out,
        );
        let whole = SplitState::whole_stack(1);
        assert_eq!(
            w.spill_target(ObjectId(99), ObjectId(2), ObjectId(1), whole),
            Some(ObjectId(4))
        );
        // Give the main pack room and it wins over the side packs.
        w.weenie_mut(ObjectId(2)).unwrap().pwd.items_capacity = Some(9);
        assert_eq!(
            w.spill_target(ObjectId(99), ObjectId(2), ObjectId(1), whole),
            Some(ObjectId(2))
        );
    }

    /// Oracle: automatic merge chooses the **first** partially-filled stack of the
    /// same wcid, searching the container then its side packs.
    #[test]
    fn auto_merge_finds_the_first_partial_stack_of_the_same_wcid() {
        let mut w = world_with(&[
            (
                1,
                PublicWeenieDesc {
                    bitfield: 1,
                    ..PublicWeenieDesc::default()
                },
            ),
            (10, stack(0x100, 100, 100)), // full: skipped
            (11, stack(0x200, 5, 100)),   // wrong wcid: skipped
            (12, stack(0x100, 5, 100)),   // match
            (99, stack(0x100, 3, 100)),   // the incoming item
        ]);
        let mut out = RecordingSink::default();
        w.view_object_contents(
            ObjectId(1),
            &[
                ContentProfile {
                    iid: ObjectId(10),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(11),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(12),
                    container_properties: 0,
                },
            ],
            &mut out,
        );
        assert_eq!(
            w.find_auto_merge_target(ObjectId(99), ObjectId(1)),
            Some(ObjectId(12))
        );
    }

    /// Query item mana is sent and is not gated by the inventory lock.
    #[test]
    fn query_item_mana_is_sent_and_is_not_gated_by_the_inventory_lock() {
        let mut w = world_with(&[(1, PublicWeenieDesc::default())]);
        let mut req = crate::RecordingRequests::default();
        // Wedge the lock the way an outstanding move does.
        w.request_lock.record(
            ObjectId(1),
            requests::InventoryRequest::Drop,
            ServerTime(0.0),
        );
        assert!(
            w.ready_for_inventory_request(true).is_err(),
            "the lock is held"
        );

        w.query_item_mana(&mut req, ObjectId(1));
        assert_eq!(
            req.0,
            vec![Request::QueryItemMana(
                dereth_protocol::items::ItemQueryItemMana {
                    object: ObjectId(1)
                }
            )],
            "it asks a question and moves nothing, so the lock does not gate it"
        );
    }

    /// Oracle: §15's rebuild note — a split size equal to the maximum is the "whole stack"
    /// sentinel and there is no separate boolean.
    #[test]
    fn the_split_sentinel_is_equality_not_a_flag() {
        assert!(SplitState::whole_stack(7).is_whole_stack());
        assert!(!SplitState {
            split_size: 6,
            max_split_size: 7
        }
        .is_whole_stack());
        assert!(
            SplitState::default().is_whole_stack(),
            "0 == 0 is the whole of an empty stack"
        );
    }

    /// The three refusals before the pending row are the list kind and yourself.
    #[test]
    fn the_three_refusals_before_the_pending_row_are_the_list_kind_and_yourself() {
        let pack = PublicWeenieDesc {
            items_capacity: Some(6),
            ..PublicWeenieDesc::default()
        };
        let mut w = world_with(&[
            (
                30,
                PublicWeenieDesc {
                    items_capacity: Some(10),
                    containers_capacity: Some(2),
                    ..PublicWeenieDesc::default()
                },
            ),
            (12, PublicWeenieDesc::default()),
            (40, pack),
        ]);
        w.player = Some(ObjectId(30));
        let whole = SplitState::whole_stack(1);
        let grid = ItemListDrop {
            parent_container: ObjectId(30),
            under: None,
            index: 0,
            num_ui_items: 4,
            dragged_is_container: false,
            container_list: false,
        };
        let say = |w: &mut World, item: u32, drop: ItemListDrop| -> Option<String> {
            let mut req = RecordingRequests::default();
            let mut out = RecordingSink::default();
            // The return value is not the subject here: an escape may legitimately go on to
            // send. What each case is pinned on is the sentence, or its absence.
            let _ = w.item_list_accept_drag(
                &mut req,
                &mut out,
                ObjectId(item),
                drop,
                whole,
                ServerTime(1.0),
            );
            out.0.iter().rev().find_map(|n| match n {
                Notice::DisplayString { channel, text, .. } if *channel == FEEDBACK_CHANNEL => {
                    Some(text.clone())
                }
                _ => None,
            })
        };

        // The self-drop arm plus the is-the-player test: the grid's parent container is the player
        // and so is the drag. It wins over the list-kind arm below, which the player (a container)
        // would also trip.
        assert_eq!(
            say(&mut w, 30, grid).as_deref(),
            Some("You cannot place yourself in your inventory!"),
            "the drag handler's self-drop refusal must win over the later item-legality refusal"
        );
        // Not the player, same shape: the arm falls through to the list-kind arm.
        let other = ItemListDrop {
            parent_container: ObjectId(40),
            ..grid
        };
        assert_ne!(
            say(&mut w, 40, other).as_deref(),
            Some("You cannot place yourself in your inventory!"),
            "the test asks whether the object is the player, not whether the list shows it"
        );

        // A container over an item list.
        assert_eq!(
            say(
                &mut w,
                40,
                ItemListDrop {
                    dragged_is_container: true,
                    ..grid
                }
            )
            .as_deref(),
            Some("Cannot place container in item list"),
            "a container cannot be dropped on an item list"
        );
        // A plain item over a container list, and its escape.
        let strip = ItemListDrop {
            container_list: true,
            ..grid
        };
        assert_eq!(
            say(&mut w, 12, strip).as_deref(),
            Some("Cannot place item in container list"),
            "an empty container-list slot has no pack row to accept a plain item"
        );
        assert_ne!(
            say(
                &mut w,
                12,
                ItemListDrop {
                    under: Some(ObjectId(40)),
                    ..strip
                }
            )
            .as_deref(),
            Some("Cannot place item in container list"),
            "over a pack row, the action becomes an ordinary put-in-that-pack drop"
        );
        assert_ne!(
            say(
                &mut w,
                40,
                ItemListDrop {
                    dragged_is_container: true,
                    ..strip
                }
            )
            .as_deref(),
            Some("Cannot place item in container list"),
            "a container belongs in a container list"
        );
    }

    /// A second drop on the same list is refused before the lock is ever consulted.
    #[test]
    fn a_second_drop_on_the_same_list_is_refused_before_the_lock_is_ever_consulted() {
        let named = |n: &str| PublicWeenieDesc {
            name: n.into(),
            ..PublicWeenieDesc::default()
        };
        let mut w = world_with(&[
            (
                30,
                PublicWeenieDesc {
                    items_capacity: Some(10),
                    ..PublicWeenieDesc::default()
                },
            ),
            (10, named("Training Wand")),
            (12, named("Shield")),
            (
                40,
                PublicWeenieDesc {
                    items_capacity: Some(6),
                    ..PublicWeenieDesc::default()
                },
            ),
        ]);
        w.player = Some(ObjectId(30));
        // The state one accepted drop leaves behind: the row, and the lock it took.
        w.set_pending_row(PendingRow {
            item: ObjectId(10),
            container: ObjectId(30),
            containers_list: false,
            index: 3,
        });
        w.request_lock.pending = InventoryRequest::PutInContainer;

        let grid = ItemListDrop {
            parent_container: ObjectId(30),
            under: None,
            index: 5,
            num_ui_items: 8,
            dragged_is_container: false,
            container_list: false,
        };
        let whole = SplitState::whole_stack(1);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        let accepted = w.item_list_accept_drag(
            &mut req,
            &mut out,
            ObjectId(12),
            grid,
            whole,
            ServerTime(1.0),
        );
        assert!(!accepted, "the pending-row refusal returns false");
        assert!(
            req.0.is_empty(),
            "the pending-row refusal sends no request; got {:?}",
            req.0
        );
        assert_eq!(
            out.0.last(),
            Some(&Notice::DisplayString {
feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: FEEDBACK_CHANNEL,
                text: "Already attempting to place Training Wand here".into(),
            }),
            "the feedback uses the pending row's name, not the dragged Shield or the lock's line; got {:?}",
            out.0
        );
        assert_eq!(
            w.pending_row.map(|r| r.item),
            Some(ObjectId(10)),
            "the row is left alone"
        );

        let strip = ItemListDrop {
            container_list: true,
            under: Some(ObjectId(40)),
            ..grid
        };
        let mut out2 = RecordingSink::default();
        w.item_list_accept_drag(
            &mut req,
            &mut out2,
            ObjectId(12),
            strip,
            whole,
            ServerTime(1.0),
        );
        assert_eq!(
            out2.0.last(),
            Some(&Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: FEEDBACK_CHANNEL,
                text: crate::inventory::requests::BUSY_MESSAGE.into(),
            }),
            "a different list meets the inventory-request readiness check instead; got {:?}",
            out2.0
        );
    }
}
