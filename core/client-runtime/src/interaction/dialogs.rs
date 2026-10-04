//! Confirmation queues and dialog responses.

use super::*;

impl Interaction {
    pub fn take_usage_confirmations(
        &mut self,
    ) -> Vec<(
        ObjectId,
        dereth_client_model::inventory::use_object::UsageConfirmation,
    )> {
        std::mem::take(&mut self.pending_usage_confirmations)
    }

    pub fn take_targeted_confirmations(
        &mut self,
    ) -> Vec<(
        ObjectId,
        ObjectId,
        dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation,
    )> {
        std::mem::take(&mut self.pending_targeted_confirmations)
    }

    /// The queue `crate::target_confirmation::TargetedDialogs` turns into
    /// current-UI dialogs with the close-vendor callback.
    pub fn take_vendor_close_confirmations(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_vendor_close_confirmations)
    }

    /// Buy-house and rent-by-proxy confirmation dialogs.
    pub fn take_house_payment_confirmations(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.pending_house_payment_confirmations)
    }

    /// The close-vendor dialog's **Yes** arm, and the whole of it.
    ///
    /// It returns early when property `0x92` is absent, clears the dialog context, and on a Yes
    /// raises the close-vendor notice with `false`.
    ///
    /// The close-vendor notice with `false`
    /// closes the vendor without completing the trade — the same
    /// function the empty-basket arm calls. **A No is
    /// nothing at all**: the baskets keep their contents and the window stays up.
    pub fn confirm_vendor_close(&mut self, game: &mut dereth_client_model::World) {
        let mut out = Notices::default();
        let req = RecordingRequests::default();
        if game.close_vendor(&mut out) {
            self.stats.vendor_closes += 1;
        }
        self.absorb(game, out, req);
    }

    /// Drain the five gameplay confirmation types carried by this
    /// frame's `0x0274` messages for `crate::target_confirmation::TargetedDialogs`.
    pub fn take_server_confirmations(&mut self) -> Vec<(i32, u32, String)> {
        std::mem::take(&mut self.pending_server_confirmations)
    }

    /// This frame's `0x0276`s — the server taking a question back.
    pub fn take_confirmation_aborts(&mut self) -> Vec<(i32, u32)> {
        std::mem::take(&mut self.pending_confirmation_aborts)
    }

    /// This frame's type-1 `0x0274`s — somebody swearing to the player.
    pub fn take_swear_requests(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_swear_requests)
    }

    /// This frame's `0x0004 Communication_PopUpString` texts, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen as message dialogs.
    pub fn take_pop_up_strings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_pop_up_strings)
    }

    /// How many `0x0004`s are waiting for a UI to show them.
    ///
    /// The queue is drained only once the shell is up and in `GAME_PLAY`, so a non-zero value here
    /// is "received but not yet shown" and not "lost" — which is the distinction a test needs and
    /// a counter on its own cannot make.
    #[must_use]
    pub fn pop_up_strings_pending(&self) -> usize {
        self.pending_pop_up_strings.len()
    }

    /// `@die`'s question, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen.
    pub fn take_die_confirmations(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_die_confirmations)
    }

    /// The die dialog's Yes arm — suicide event `0x0279`.
    ///
    /// **A No is nothing at all**: the native refusal branch skips the call, exactly as the three
    /// allegiance dialogs do, and unlike the gameplay confirmation close, which
    /// answers the server either way.
    pub fn confirm_die(&mut self, game: &mut dereth_client_model::World) {
        let mut req = RecordingRequests::default();
        game.confirm_die(&mut req);
        self.stats.chat_command_requests += 1;
        let out = Notices::default();
        self.absorb(game, out, req);
    }

    /// `@house abandon`'s two prompts, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen.
    pub fn take_house_abandon_first(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_house_abandon_first)
    }

    pub fn take_house_abandon_second(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_house_abandon_second)
    }

    /// The first house-abandon callback's Yes arm.
    ///
    /// Its whole body, once the `0x92` guard passes, is a second
    /// callback dialog carrying the same two properties — `0x8E` = enum 1 and
    /// `0xC5` = the prompt — and the second house-abandon callback as the callback. **It sends
    /// nothing**: the first Yes only asks again.
    pub fn confirm_house_abandon_first(&mut self) {
        self.pending_house_abandon_second
            .push(dereth_client_model::chat_cmd::HOUSE_ABANDON_SECOND);
        self.stats.house_abandon_second_raised += 1;
    }

    /// The second house-abandon confirmation's Yes arm sends the abandon-house event,
    /// `0x021F`, the only send on this path and the only sender of that event in retail.
    pub fn confirm_house_abandon(&mut self, game: &mut dereth_client_model::World) {
        let mut req = RecordingRequests::default();
        game.confirm_house_abandon(&mut req);
        self.stats.chat_command_requests += 1;
        let out = Notices::default();
        self.absorb(game, out, req);
    }

    pub fn take_allegiance_confirmations(&mut self) -> Vec<(AllegianceAction, ObjectId, String)> {
        std::mem::take(&mut self.pending_allegiance_confirmations)
    }

    /// Swear, break and kick confirmation callbacks — the **Yes** arms, which are the only arms
    /// that send.
    ///
    /// Break re-reads the patron from the player's profile at close time and sends a break request
    /// for that id. Kick sends a break request for the retained vassal id. Swear sends a swear
    /// request for the retained proposed-patron id.
    ///
    /// **A No is nothing at all** for all three: there is no confirmation response here,
    /// because these dialogs are the *client's* own question and not one of the server's seven
    /// `0x0274` types. (The one allegiance dialog that does answer the server is
    /// the accept-swear confirmation close, confirmation type 1, which is a different
    /// path and not this one.)
    ///
    /// Break re-reads the patron here rather than using the id the question was asked about,
    /// which is retail's own asymmetry with Kick: the kick confirmation latches the would-be
    /// kicked vassal's id when it opens, while the break confirmation looks the patron up when it
    /// closes.
    pub fn confirm_allegiance(
        &mut self,
        game: &mut dereth_client_model::World,
        action: AllegianceAction,
        target: ObjectId,
    ) {
        let out = Notices::default();
        let mut req = RecordingRequests::default();
        match action {
            AllegianceAction::Swear => {
                game.swear_allegiance(&mut req, target);
                self.stats.allegiance_swears += 1;
            }
            AllegianceAction::Break => {
                if game.break_allegiance_from_patron(&mut req).is_none() {
                    // The patron went away while the question was on screen. Retail sends
                    // a break-allegiance request for id 0 here, because the patron lookup leaves
                    // its out parameter at zero and the call is unconditional; this build declines
                    // to put an object id of 0 on the wire and counts the miss instead.
                    self.stats.allegiance_breaks_without_patron += 1;
                    return;
                }
                self.stats.allegiance_breaks += 1;
            }
            AllegianceAction::Kick => {
                game.break_allegiance(&mut req, target);
                self.stats.allegiance_kicks += 1;
            }
        }
        self.absorb(game, out, req);
    }
    /// This frame's `0x0274` type-4 invitations.
    pub fn take_fellowship_requests(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_fellowship_requests)
    }

    /// Close a server-confirmation dialog. The native callback reads the answer byte from
    /// property `0x92`, sends it with the retained server context and confirmation type, then
    /// clears the local dialog context, confirmation type and server context.
    ///
    /// There is **no branch on the answer**: a No is a `0x0275` with `accepted = 0`, which is what
    /// lets ACE's `ConfirmationManager.HandleResponse(…, false)` act on a refusal at once instead
    /// of sitting out the thirty seconds. Retail passes property `0x92` as the answer.
    pub fn confirm_server_confirmation(
        &mut self,
        confirmation_type: i32,
        context_id: u32,
        accepted: bool,
    ) {
        self.outbox.push(Request::ConfirmationResponse(
            dereth_protocol::comms::CharacterConfirmationResponse {
                confirmation_type,
                context_id,
                // The native callback widens the answer byte: the C `bool` reaches the wire as 1 or 0.
                accepted: i32::from(accepted),
            },
        ));
        self.stats.confirmations_answered += 1;
    }

    /// Accepted targeted-use callback. Identity comes from the dialog collection, never selection.
    pub fn confirm_targeted_usage(
        &mut self,
        game: &mut dereth_client_model::World,
        source: ObjectId,
        target: ObjectId,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        game.confirm_targeted_usage(&mut req, &mut out, source, target, game.split, now);
        self.absorb(game, out, req);
    }

    /// Accept a usage-confirmation dialog. The object is the instance id
    /// retained on the dialog, not the current selection.
    pub fn confirm_usage(
        &mut self,
        game: &mut dereth_client_model::World,
        object: ObjectId,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        game.confirm_usage(&mut req, &mut out, object, game.split, now);
        self.absorb(game, out, req);
    }
    /// Register local chat-system commands when the `0xF658` startup flag is true.
    pub fn startup_turbine_chat_commands(&mut self) {
        self.chat.add_turbine_chat_commands();
    }
    /// Drain this frame's external-container notices, preserving producer order.
    pub fn take_external_container_notices(&mut self) -> Vec<ExternalContainerNotice> {
        std::mem::take(&mut self.pending_external_container)
    }

    /// Drain this frame's salvage notices in producer order.
    pub fn take_salvage_notices(&mut self) -> Vec<SalvageNotice> {
        std::mem::take(&mut self.pending_salvage)
    }

    /// Drain this frame's housing range exits at the nine-unit threshold.
    pub fn take_slumlord_range_exits(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_slumlord_range_exits)
    }

    /// Drain this frame's unowned-book use-radius exits.
    pub fn take_book_range_exits(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_book_range_exits)
    }

    /// The ids accepted this
    /// frame, for delivery into the trade panel; see [`Self::pending_trade_for_dummies`].
    pub fn take_trade_for_dummies(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_trade_for_dummies)
    }
}
