//! Item use, shortcuts, and world placement.

use super::*;

impl Interaction {
    /// **The inbound half of the inventory loop.**
    ///
    /// Dispatch the smart-box event `(id, 0, 0)`. All three of this file's "use" gestures — the
    /// viewport double-click's `SearchReason::Use` arm, the toolbar's Use button
    /// and the `USE` action — are that one call in the
    /// client.
    ///
    /// Object use runs
    /// use-result classification **first**, and for an object lying loose in the
    /// 3-D world the answer is 2, which goes to the item-use dispatcher's
    /// place-in-backpack arm and **returns before the use request is sent**.
    /// The pickup is the `0x0019` put-item-in-container `(item, player id, 0)` — there is no
    /// pickup-item event. See
    /// [`dereth_client_model::inventory::use_object`] for the whole chain and for what it leaves.
    ///
    /// Nothing is predicted: placing in the backpack ghosts the icon with the show-pending notice
    /// and the item does not move until `0x0022 Item_ServerSaysContainID` arrives and
    /// [`apply_events`] applies it.
    pub(super) fn use_object(
        &mut self,
        id: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        use dereth_client_model::inventory::use_object::{UseOutcome, UseResult};
        // The UI use command arms target mode for id zero; the inventory use operation does not.
        if id.0 == 0 {
            self.set_target_mode(TargetMode::Use);
            return;
        }
        // The tail's effects are notices and world state, not a return value, so
        // they are counted from what `dereth_client_model` raised during *this* call and not from a summary
        // flag: `ground_object` before and after is the only thing that says whether a corpse
        // actually became the open ground container.
        let (n_ground, n_contained, n_panels, n_confirm) = (
            out.ground.len(),
            out.contained.len(),
            out.panels,
            out.usage_confirmations.len(),
        );
        let ground_before = game.ground_object;
        let outcome = game.use_object(req, out, id, game.split, now);
        let opened_ground: usize = out.ground[n_ground..].iter().filter(|g| g.0 != 0).count();
        self.stats.contained_containers_opened += (out.contained.len() - n_contained) as u64;
        self.stats.panels_requested += out.panels - n_panels;
        self.stats.usage_confirmations +=
            u64::try_from(out.usage_confirmations.len() - n_confirm).unwrap_or(0);
        // Setting the ground object is attempted only at the tail of item use, and the tail runs
        // only for a container that is useable and not the player's. The setter's own "only when
        // it differs from the current ground object" test is what makes this a change rather than a
        // call, which is why the *field* is the oracle and `opened_ground` (the closing notice for
        // whatever was open before) is only a cross-check.
        if game.ground_object != ground_before && game.ground_object.is_some() {
            self.stats.ground_objects_requested += 1;
        } else if self.tail_asked_for_a_ground_object(game, id) {
            self.stats.ground_objects_refused += 1;
        }
        debug_assert!(
            opened_ground == 0,
            "setting the ground object raises only the closing notice; the opening one is the \
             view-contents handler's"
        );
        match outcome {
            UseOutcome::Dispatched {
                result: UseResult::PlaceInBackpack,
                sent: true,
            } => {
                self.stats.pickups_requested += 1;
            }
            UseOutcome::Dispatched {
                result: UseResult::Trade,
                sent: true,
            } => {
                self.stats.trades_requested += 1;
            }
            UseOutcome::Dispatched {
                result,
                sent: false,
            } => {
                // Still reported, and still not silent: arm 7 (the minigame start) answers `false` in
                // the client itself, so a game board reaches here having raised its notice.
                self.stats.uses_undispatched += 1;
                tracing::debug!("item use arm {result:?} for {id:?} sent nothing");
            }
            UseOutcome::Confirming(kind) => {
                tracing::debug!("{id:?} asks first: {}", kind.prompt());
            }
            UseOutcome::TargetModeArmed => {
                // Record the targeting object and set `TargetMode::UseTarget`. The mode is this
                // struct's, so this arm is the one place `dereth_client_model` can ask for it.
                self.set_target_mode(TargetMode::UseTarget);
                self.stats.target_modes_armed += 1;
            }
            UseOutcome::Refused(_) => self.stats.uses_refused += 1,
            UseOutcome::Dispatched { .. }
            | UseOutcome::UseEventSent
            | UseOutcome::NoObject
            | UseOutcome::Throttled
            | UseOutcome::Busy
            | UseOutcome::Nothing => {}
        }
    }

    /// Whether the item-use dispatcher's tail would have attempted to set
    /// this object as the ground container.
    ///
    /// The same four-part test as the tail itself, asked again so that a refusal can be counted
    /// apart from a gesture that never got that far. It is a *duplicate* of a condition in
    /// `dereth_client_model`, which is normally the wrong thing to do — it is here because the alternative
    /// is a counter that cannot distinguish "no corpse was ever double-clicked" from "every
    /// corpse refused to open". That distinction is the whole point of the measurement.
    fn tail_asked_for_a_ground_object(
        &self,
        game: &dereth_client_model::World,
        id: ObjectId,
    ) -> bool {
        use dereth_client_model::inventory::use_object::ItemUses;
        let Some(w) = game.weenie(id) else {
            return false;
        };
        let uses = ItemUses(w.pwd.useability.unwrap_or(0));
        uses.is_useable()
            && !uses.is_useable_targeted()
            && w.is_container()
            && !game.is_owned_by_player(id)
    }

    /// The toolbar-drop shortcut arm, past the ancestor sweep
    /// (which `ShortcutBar::slot_under` has already run to produce `slot`).
    ///
    /// It removes whatever shortcut is in slot `n`, creates a shortcut to the item there, and —
    /// if a different object was displaced and there is an empty slot to the right of `n` — adds
    /// the displaced object in the first such slot, all with server notification.
    ///
    /// The displacement is the reason [`Self::remove_shortcut_in_slot_num`] *returns* the id it
    /// removed rather than a bool: dropping onto an occupied slot pushes its occupant rightwards
    /// instead of destroying the shortcut.
    ///
    /// The search runs to the last of all eighteen slots before it wraps, so when the visible row
    /// is full to the right of `n` the occupant lands in the hidden second row: it stays a
    /// shortcut, and its picture keeps the second row's plate, which has no figure on it.
    pub(super) fn shortcut_drop(
        &mut self,
        item: ObjectId,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        let displaced = self.remove_shortcut_in_slot_num(slot, game, req);
        // `remove_shortcut_in_slot_num` runs first and unconditionally, so a `create_shortcut_to_item`
        // that then refuses leaves the destination **empty** and the displaced shortcut gone.
        // That is the client's behaviour and it is not repaired here: the conditional
        // below is reached only after a successful create.
        if !self.create_shortcut_to_item(item, Some(slot), game, req, out, now) {
            return false;
        }
        if let Some(old) = displaced.filter(|old| *old != item) {
            if let Some(k) = game
                .player_system
                .first_empty_shortcut_to_the_right_of(slot)
            {
                self.add_shortcut(old, k, game, req, now);
            }
        }
        true
    }

    /// Create a shortcut to `item`, as a drag or the make-shortcut key asks: `(item, slot,
    /// from a drag = true, quiet = false)`. The any-slot arm is the make-shortcut key's.
    ///
    /// The gates in the client's own order; see the `DropTarget::ShortcutSlot` arm for the one
    /// piece of the shortcut-eligibility check this build declines to guess at.
    ///
    /// `slot` is `None` for "any slot", the make-shortcut key's call. That arm sweeps the bar
    /// first: an object that already has a shortcut is refused with *"There is already a
    /// shortcut to the …"*, a full bar with *"There are no free shortcut slots"*, and otherwise
    /// the shortcut goes into the first empty slot. Both callers are loud, so every refusal that
    /// has a message prints it on the feedback channel `0x1A`.
    pub(super) fn create_shortcut_to_item(
        &mut self,
        item: ObjectId,
        slot: Option<usize>,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        // A zero id, or an id with no known object, is a no-op.
        if item.0 == 0 || game.weenie(item).is_none() {
            return false;
        }
        let name = |game: &dereth_client_model::World| {
            game.weenie(item).map_or_else(String::new, |w| {
                w.object_name(dereth_client_model::weenie::NameType::Appropriate)
            })
        };
        let refuse = |out: &mut Notices, text: String| {
            out.emit(dereth_client_model::Notice::DisplayString {
                channel: 0x1A,
                text,
                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
            });
            false
        };
        // The shortcut-eligibility check's third test: a non-zero container id equal to the
        // vendor's id —
        // you cannot make a shortcut to something that is still the shopkeeper's.
        if let Some(v) = game.vendor_id() {
            if v.0 != 0 && game.weenie(item).and_then(|w| w.pwd.container_id) == Some(v) {
                return refuse(
                    out,
                    format!("You cannot make a shortcut to the {}", name(game)),
                );
            }
        }
        // If the item is not the player's and this call came from a drag, pick it up first.
        if !game.is_owned_by_player(item)
            && !game.place_in_backpack(req, out, item, false, game.split, now)
        {
            return false;
        }
        let slot = match slot {
            Some(n) => n,
            None => {
                if game.player_system.shortcut_slot_of(item).is_some() {
                    return refuse(
                        out,
                        format!("There is already a shortcut to the {}", name(game)),
                    );
                }
                if game.player_system.first_empty_shortcut().is_none() {
                    return refuse(out, "There are no free shortcut slots".to_owned());
                }
                // Past the last slot, which `add_shortcut` reads as "the first empty one".
                dereth_client_model::player::SHORTCUT_SLOTS
            }
        };
        // Remove the item's existing shortcut, then add it at the slot, both with server
        // notification — the removal is what stops one object occupying two slots when it is
        // dragged from one tile to another.
        self.remove_shortcut(item, game, req);
        self.add_shortcut(item, slot, game, req, now)
    }

    /// Move `(item, slot)` with server notification on.
    ///
    /// Shortcut insertion's notify-server flag guards the pair this client did not have:
    /// the immediate `0x019C` send and the retained-module update.
    /// The fill step also fills the tile.
    pub(super) fn add_shortcut(
        &mut self,
        item: ObjectId,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        now: ServerTime,
    ) -> bool {
        // A slot outside the shortcut slots takes the first empty one instead and
        // gives up when there is none. A zero id is a no-op.
        let slot = if slot < dereth_client_model::player::SHORTCUT_SLOTS {
            slot
        } else {
            match game.player_system.first_empty_shortcut() {
                Some(k) => k,
                None => return false,
            }
        };
        if item.0 == 0 {
            return false;
        }
        let sc = dereth_protocol::login::ShortCutData {
            index: i32::try_from(slot).unwrap_or(-1),
            object_id: item,
            spell_id: 0,
        };
        if !game.player_system.add_shortcut(sc) {
            return false;
        }
        game.player_system.mark_dirty(now);
        dereth_client_model::RequestSink::send(
            req,
            dereth_client_model::Request::AddShortCut(
                dereth_protocol::login::CharacterAddShortCut { shortcut: sc },
            ),
        );
        self.stats.shortcuts_added += 1;
        true
    }

    /// Move `(item)` with server notification on.
    ///
    /// The sweep for the slot holding `item`, then flushing that slot's item list, resetting its
    /// shortcut number to -1 and — under the notify-server flag — the `0x019D` send beside the
    /// retained-module removal.
    /// Answers the slot it emptied.
    pub(super) fn remove_shortcut(
        &mut self,
        item: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) -> Option<usize> {
        let slot = game.player_system.shortcut_slot_of(item)?;
        game.player_system.remove_shortcut(slot);
        dereth_client_model::RequestSink::send(
            req,
            dereth_client_model::Request::RemoveShortCut(
                dereth_protocol::login::CharacterRemoveShortCut {
                    index: u32::try_from(slot).unwrap_or(0),
                },
            ),
        );
        self.stats.shortcuts_removed += 1;
        Some(slot)
    }

    /// `(slot)` with server notification on — read the
    /// slot's object id, remove *that object's* shortcut, and hand the id back.
    pub(super) fn remove_shortcut_in_slot_num(
        &mut self,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) -> Option<ObjectId> {
        let id = game.player_system.shortcut_at(slot)?;
        self.remove_shortcut(id, game, req);
        Some(id)
    }

    /// The complete UI examination operation.
    ///
    /// A nonzero id is passed to the world's examination operation and returns immediately.
    /// For id zero, when the target mode is not already Examine, native behavior changes it to
    /// Examine, clears the pending leave flag, registers input map `0x1000000B` with the UI's
    /// input-action callback if the input manager exists, then updates the cursor.
    ///
    /// **The zero-id arm is the examine key's behaviour with no selection.** The world's
    /// examination request correctly returns for id zero, but cannot arm the UI target cursor.
    ///
    /// With no selection, the examine key must arm the magnifying-glass cursor until
    /// a click spends it or Escape cancels it. Those consumers are
    /// `crate::cursor::update_cursor_state` and target-mode clearing; the toolbar Examine
    /// button and the key are their producers.
    ///
    /// The input-map registration is not repeated here: `App` mirrors [`Self::target_mode`] into
    /// the device input's target-mode registration after `use_time`, which is the same `0x1000000B` at the
    /// same unfocused-UI input priority this line would push, and doing it twice is how a map ends
    /// up registered under two callbacks.
    pub(super) fn examine_object(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        id: ObjectId,
    ) {
        if id.0 != 0 {
            game.examine_object(req, id);
            return;
        }
        if self.target_mode != TargetMode::Examine {
            self.set_target_mode(TargetMode::Examine);
            self.stats.target_modes_armed += 1;
        }
    }

    /// Execute the use/examine cursor's second click.
    pub(super) fn execute_target_mode_for_item(
        &mut self,
        target: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        match self.target_mode {
            // `execute_target_mode_for_item`'s examine arm is
            // not a bare appraisal request; see the
            // `SearchReason::Examine` site.
            // It goes through the complete examination operation rather than its non-zero
            // half, because the native path calls the complete UI examination operation with the
            // found id: a second click that hits nothing re-arms the mode instead of ending it. (It
            // is already armed here — that is what `TargetMode::Examine` means — so the zero arm's
            // `if` is false and the observable is unchanged. It is routed anyway so there is one
            // examination path in this file and not two.)
            TargetMode::Examine => self.examine_object(game, req, target),
            TargetMode::Use => self.use_object(target, game, req, out, now),
            TargetMode::UseTarget => {
                // target_acquired consumes the retained source before compatibility.
                // It is not the mutable selection, and does not re-run object use's throttle.
                let _ = game.target_acquired(req, out, target, game.split, now);
            }
            TargetMode::None => {}
        }
        // execute_target_mode_for_item never clears the mode. The click's deferred
        // `use_time` tail owns that; a generic Use that rearmed UseTarget must survive it.
    }

    /// Attempt to place the dropped item in 3D on the found object.
    ///
    /// **The whole decision is the world's 3D-placement routine**, not a two-way fork where a
    /// container takes the item and anything else puts it on the ground, because the drop has
    /// **several routes to another object**:
    ///
    /// * the item's own gates — a drop onto yourself places it in the backpack, an unowned item
    ///   refuses *"You must first pick up the %s"*, an item on the trade window refuses;
    /// * `attempt_merge`, so a stack dropped on a matching stack in the world merges;
    /// * the vendor arm (`BF_VENDOR`) — `attempt_sell_to_vendor`;
    /// * the secure-trade arm, behind `DragItemOnPlayerOpensSecureTrade`;
    /// * **a `TYPE_CREATURE` target — the give request, `0x00CD`**, which is how an item is
    ///   given to an NPC by dropping it on them;
    /// * the container's own gates — *"The %s is locked"* and *"You must open the %s first"*, so
    ///   this build never sends a put-in-container at a sealed chest it has never opened.
    ///
    /// The ground leg keeps the split test, which is split size >= maximum split size —
    /// [`SplitState::is_whole_stack`], not a separate boolean — and gains the `on_ground` gate and
    /// the *"Move cancelled"* arm.
    pub(super) fn place_in_3d(
        &mut self,
        item: ObjectId,
        onto: Option<ObjectId>,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        // Player id -> physics-object lookup -> ground-contact query,
        // which the client reaches through singletons and this build reaches through
        // [`Interaction::note_player_physics`], written once a frame by [`use_time`] from
        // `WorldScene::character`. `None` -- no body -- is the client's null pointer and takes the
        // **same** refusal branch as an airborne one in retail.
        //
        // What this gate does **not** cover is the give: the place-in-3-D path's creature arm
        // returns 1 much earlier in the path, so the client lets you hand an item
        // to an NPC in mid air. The refusal belongs to the *ground* leg alone -- split-to-3D,
        // put-in-3D and the "Move cancelled" arm.
        let player_on_ground = self.player_on_ground == Some(true);
        if !game.attempt_place_in_3d(
            req,
            out,
            item,
            onto,
            true,
            player_on_ground,
            game.split,
            now,
        ) {
            self.stats.requests_refused += 1;
            // A refused 3D placement clears the dropped item's waiting state, the same un-ghost
            // the item-list and paper-doll arms do for a refused drop.
            game.set_waiting_state(item, false);
        }
    }
}
