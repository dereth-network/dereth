//! Object notices and request collection.

use super::*;

impl Interaction {
    /// **The last mile.** Every notice this call raised, plus the requests it queued.
    ///
    /// Display-string notice handling adds `(text, channel, true, 0)` to the scroll;
    /// here the scroll is the shared chat scroll, which is the one object this file and
    /// `hud.rs` — the two halves of the seam — both hold a `&mut` to.
    /// Three combat refusals bypass notices: two from combat-mode
    /// toggling and one from starting an attack. All three write
    /// `(text, 0x1A, true, 0)` to the scroll, reaching the same channel as
    /// `DisplayString` one step earlier in the chain.
    pub(super) fn refuse(&mut self, game: &mut dereth_client_model::World, text: &str) {
        game.scroll.add_feedback_to_scroll(
            text,
            dereth_client_model::scroll::LOCAL_ERROR_TYPE,
            true,
            0,
            dereth_client_contract::feedback::Feedback::LOCAL,
        );
        self.stats.notice_strings_scrolled += 1;
        self.last_refusal = Some(text.to_owned());
    }

    /// Object-maintenance/create/delete broadcasts join the same subscribers as UI replies.
    /// ObjectStream has already retired the exact old instance before offering these UI notices.
    pub fn apply_object_notices(
        &mut self,
        game: &mut dereth_client_model::World,
        notices: Vec<Notice>,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        for notice in notices {
            let moved = match &notice {
                Notice::ItemMoved {
                    object, container, ..
                } => Some((*object, *container)),
                _ => None,
            };
            out.emit(notice);
            if let Some((object, container)) = moved {
                self.on_item_moved(game, object, container, &mut out, &mut req);
            }
        }
        self.absorb(game, out, req);
    }

    /// Complete only this lifetime notice's selection subscribers while the old Weenie is
    /// queryable. A defender event is a different producer and is not drained opportunistically.
    pub fn dispatch_object_notice(
        &mut self,
        game: &mut dereth_client_model::World,
        notice: Notice,
        geometry: &crate::selection_geometry::SceneSelectionPhysics,
        radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let preceding = std::mem::take(&mut self.pending_selection_changes);
        self.apply_object_notices(game, vec![notice]);
        if self.pending_selection_changes != 0 {
            let geometry = geometry.for_current_objects(game);
            while self.pending_selection_changes != 0 {
                self.run_selection_change_notices(game, &geometry, radius, now);
            }
        }
        self.pending_selection_changes = preceding;
    }

    /// The toolbar's direct health/mana queries, shared with ordinary `UiRequest`
    /// delivery. This bounded tail drains no unrelated input and replays no UI tick.
    pub fn dispatch_toolbar_query(
        &mut self,
        game: &mut dereth_client_model::World,
        request: &UiRequest,
    ) -> bool {
        let mut req = RecordingRequests::default();
        match request {
            UiRequest::QueryHealth(id) => game.query_health(&mut req, *id),
            UiRequest::QueryItemMana(id) => game.query_item_mana(&mut req, *id),
            _ => return false,
        }
        self.stats.vital_queries += 1;
        self.outbox.extend(req.0);
        true
    }

    /// Complete the three client subscribers to item-move notices: the
    /// toolbar's ownership sweep first, then secure trade's partner-row lifetime, then the
    /// player's blocked inventory retry.
    pub(super) fn on_item_moved(
        &mut self,
        game: &mut dereth_client_model::World,
        object: ObjectId,
        container: ObjectId,
        out: &mut Notices,
        req: &mut RecordingRequests,
    ) {
        // **The toolbar's item-move handler.**
        //
        // The handler's second half is a sweep over the shortcut slots for the slot holding the
        // moved object, gated on the object having left the player:
        //
        // For each slot whose item id matches, remove its shortcut with server notification
        // when object lookup fails or the object is no longer owned by the player.
        //
        // The world's ownership query already answers `false` for an id with no weenie (the walk
        // terminates on the missing lookup, exactly as the original's null object lookup
        // does), so the two disjuncts are one call here.
        //
        // The notice's producers are every inventory movement — `0x0022`, `0x0024`, `0x0025`,
        // `0x019A`, and behind `0xF747` — so this one arm
        // covers both halves of losing items on death: the shortcut row goes, and with it the
        // per-object numeral `Hud::slot_decoration` reads (removing a shortcut sets its number
        // to -1), which is what stops the corpse window's copy being decorated.
        // An item moving between the player's own containers is still `is_owned_by_player`, so this
        // cannot disturb an ordinary move.
        if game.player_system.shortcut_slot_of(object).is_some() && !game.is_owned_by_player(object)
        {
            self.remove_shortcut(object, game, req);
        }
        if !game.is_owned_by_player(object) {
            game.payments.remove_unowned(object);
        }
        if game.trade_item_moved_to_partner(object, container) {
            self.stats.trade_rows_changed += 1;
        }
        if game.unblock.unblock_attempt_num == 0 {
            return;
        }
        let retried = game.unblock_on_item_moved(
            req,
            out,
            object,
            container,
            game.split,
            ServerTime(self.last_use_time.0),
        );
        if retried && game.unblock.unblock_attempt_num == 0 {
            self.stats.unblock_retries += 1;
        } else if retried {
            self.stats.unblocks_started += 1;
        } else if game.unblock.unblock_attempt_num == 0 {
            self.stats.unblocks_abandoned += 1;
        }
    }

    pub(super) fn absorb(
        &mut self,
        game: &mut dereth_client_model::World,
        mut out: Notices,
        mut req: RecordingRequests,
    ) {
        self.stats.notices += out.count;
        self.pending_external_container
            .extend(out.external_container);
        self.stats.salvage_panel_notices += out.salvage.len() as u64;
        let multiple = crate::hud::character_option(
            game,
            dereth_client_contract::PlayerOption::SalvageMultiple,
        )
        .unwrap_or(false);
        for notice in &out.salvage {
            for effect in game.salvage_notice(*notice, multiple) {
                match effect {
                    dereth_client_model::inventory::salvage::SalvageEffect::Notice(
                        text,
                        feedback,
                    ) => game
                        .scroll
                        .add_feedback_to_scroll(&text, 0x1A, true, 0, feedback),
                    dereth_client_model::inventory::salvage::SalvageEffect::Submit {
                        tool,
                        items,
                    } => {
                        game.create_tinkering_tool(&mut req, tool, &items);
                    }
                }
            }
        }
        self.pending_salvage.extend(out.salvage);
        for id in &out.slumlord_range_exits {
            if game
                .slumlord
                .as_ref()
                .is_some_and(|(current, _)| current == id)
            {
                game.payment_action(
                    dereth_client_contract::panels::slumlord::PaymentAction::Close,
                    |_| None,
                );
            }
        }
        self.pending_slumlord_range_exits
            .extend(out.slumlord_range_exits);
        for book in &out.book_range_exits {
            game.book_action(dereth_client_contract::book::BookAction::Close { book: *book });
        }
        self.pending_book_range_exits.extend(out.book_range_exits);
        self.pending_usage_confirmations
            .extend(out.usage_confirmations);
        self.pending_targeted_confirmations
            .extend(out.targeted_confirmations);
        // Selection-change notices converge here for the combat
        // subscriber. [`Self::run_selection_change_notices`] handles them in the same
        // frame; a handler that changes selection again feeds back into this queue.
        self.pending_selection_changes = self
            .pending_selection_changes
            .saturating_add(out.selection_changes);
        for (channel, text, feedback) in out.strings {
            game.scroll
                .add_feedback_to_scroll(&text, channel, true, 0, feedback);
            self.stats.notice_strings_scrolled += 1;
        }
        // The two notices resolving a pending trade split. An authoritative
        // result joins the already-validated automatic-offer queue and is added at
        // position 0 without repeating the selected-partial-stack refusal.
        for notice in out.trade_split {
            match notice {
                TradeSplitNotice::ItemAttributesChanged(item, kind) => {
                    if game.trade_split_item_attributes_changed(item, kind) {
                        self.pending_trade_for_dummies.push(item);
                    }
                    // The vendor's sell list listens to the same notice for its own split.
                    let mut selection = Notices::default();
                    game.vendor_split_item_attributes_changed(item, kind, &mut selection);
                    self.absorb(game, selection, RecordingRequests::default());
                }
                TradeSplitNotice::AttemptFailed => game.clear_pending_trade_split(),
            }
        }
        // Fold the inventory pending-row pair to its final state.
        //
        // The original synchronous notice handler shows the pending row inside
        // backpack placement, before the attempt. Refusal then clears waiting state,
        // followed by the end-pending notice. Here both queued notices are applied
        // after the clear; blindly replaying Show would restore a ghost that the
        // refusal just removed and strand it.
        // A refused full-backpack placement exposes that case.
        //
        // Show alone arms a row. Show plus End around a refused attempt does nothing,
        // leaving the refusal's final object state intact. End alone still removes
        // whatever pending row is armed.
        if !out.pending_in_player.is_empty() {
            let armed = out.pending_in_player.iter().fold(None, |_, n| match n {
                PendingInPlayer::Show(item) => Some(*item),
                PendingInPlayer::End => None,
            });
            match armed {
                Some(item) => game.show_pending_in_player(item),
                None => {
                    game.end_pending_in_player();
                }
            }
        }
        // **Automatic trade-offer handling.**
        //
        // Both producers meet here, and the decision needs the current and maximum
        // split sizes stored on this interaction state. Its refusal message goes
        // straight to the scroll on channel `0x1A`, like the strings above.
        for item in out.trade_for_dummies {
            match game.trade_an_item_for_dummies(item, game.split) {
                dereth_client_model::trade::ForDummies::Offer => {
                    self.pending_trade_for_dummies.push(item);
                    self.stats.trade_for_dummies_offered += 1;
                }
                dereth_client_model::trade::ForDummies::MustSplit => {
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::trade::messages::MUST_SPLIT,
                        dereth_client_model::trade::TRADE_MESSAGE_CHANNEL,
                        true,
                        0,
                        dereth_client_contract::feedback::Feedback::WARNING,
                    );
                    self.stats.notice_strings_scrolled += 1;
                    self.stats.trade_for_dummies_refused += 1;
                }
                // The item list's membership test answered yes: refuse, and retail says nothing.
                dereth_client_model::trade::ForDummies::AlreadyOffered => {
                    self.stats.trade_for_dummies_refused += 1;
                }
            }
        }
        // **The begin-game notice's receiver.**
        //
        // The minigame handler needs a mutable world borrow, so it runs in `absorb`,
        // where the world and notice batch meet. The original begin-game delivery is
        // synchronous; this path also completes it within the initiating call.
        //
        // The receiver shows the window before trying to join. The resulting `0x0269`
        // therefore enters this frame's outbox and the scroll receives the first
        // window message.
        let boards = std::mem::take(&mut out.begin_games);
        for board in boards {
            let mut join = RecordingRequests::default();
            game.begin_game(board, &mut join);
            self.stats.minigame_boards_used += 1;
            self.stats.minigame_requests += join.0.len() as u64;
            req.0.extend(join.0);
            self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
        }
        if let Some(t) = out.last {
            self.last_refusal = Some(t);
        }
        self.outbox.extend(req.0);
    }
}
