//! Measurements accumulated by the HUD and interaction owners.

/// What the interactions have done, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionStats {
    /// Object searches requested by a world click, of any [`crate::interaction::SearchReason`].
    ///
    /// **This counts requests, not arms.** `handle_drop_release`'s arm increments it without
    /// calling the object-search routine, so a rising counter is no evidence that the pick was
    /// armed. Assert [`crate::pick::WorldPicker::looking_for_object`] instead; this stays a count
    /// of gestures that asked.
    pub picks_requested: u64,
    /// `SearchReason::Drop` picks that the object search's unsigned viewport
    /// compare rejected, so no notice will answer them. Unreachable on the live tree, because
    /// `<SBOX>`'s box is the viewport; a non-zero here means the layout moved under that claim.
    pub drops_outside_the_viewport: u64,
    /// The frame loop's `SearchReason::MouseOver` searches that
    /// the object search refused, so `looking_for_object` stayed clear and no
    /// object-found notice will answer them. Each one would otherwise leave
    /// `search_reason` latched at 1 for the rest of the session. Counted
    /// rather than silently undone, because a non-zero value here is the layout putting the
    /// pointer outside `<SBOX>` and that is worth seeing.
    pub hover_searches_not_armed: u64,
    /// Hover searches refused because the pointer was over a HUD window painted
    /// over the 3-D view — the part of the render device's viewport that
    /// the game-viewport calculation subtracts in retail and that `<SBOX>`'s raw box
    /// does not.
    pub hover_searches_under_the_hud: u64,
    /// Object-found notice deliveries with a non-zero id.
    pub objects_found: u64,
    /// Tooltip assignments the object-found notice made on `<SBOX>` —
    /// i.e. hovered objects whose name reached the viewport element. Counted apart from
    /// [`Self::object_tooltips_cleared`] because "the pointer never found a named object" and
    /// "the tooltip was never taken down again" are different defects.
    pub object_tooltips_set: u64,
    /// Tooltip clears the object-found notice made — the pointer
    /// moved off every object, or the tooltip option is off.
    pub object_tooltips_cleared: u64,
    /// Hovered objects rejected by the empty-name check — an
    /// **empty** object name, which retail answers by neither setting nor clearing, so the
    /// previous tooltip stands. Its own counter because it is the one arm that leaves the
    /// element's state untouched and would otherwise read as "the hover never happened".
    pub object_tooltips_unnamed: u64,
    /// Object-selection assignments made by the object-found notice.
    pub selections: u64,
    /// [`dereth_client_model::Request`]s handed to the session.
    pub requests_sent: u64,
    /// Requests that had nowhere to go: no session, or the session refused to encode one.
    pub requests_undeliverable: u64,
    /// Requests refused by `dereth_client_model` before any bytes existed — the inventory lock, an attack in
    /// progress, an illegal combat mode. Every one of these is a refusal the client also makes.
    pub requests_refused: u64,
    /// `UiRequest`s this module consumed.
    pub ui_requests_handled: u64,
    /// Toolbar stance-icon clicks routed to combat-mode toggling.
    pub combat_mode_toggles: u64,
    /// `interpreted_state.current_style` -> `CombatState::current_style` crossings. Without a
    /// production writer for that field every dual-wielder would charge the power bar at 1.000 s
    /// instead of 0.800 s.
    pub combat_style_bridges: u64,
    /// How many times [`crate::interaction::Interaction::note_player_physics`] changed its answer —
    /// so a test can tell "the body has been on the ground the whole time" from "the producer never
    /// ran", which are the same reading of the field alone.
    pub physics_answers_changed: u64,
    /// The same, for [`crate::interaction::Interaction::player_ready`] — how many times the ready
    /// answer moved. Separate from `physics_answers_changed` on purpose: the two predicates share
    /// a producer call but are different questions, and a counter shared between them could not
    /// tell "the motion answer never ran" from "the motion answer never changed".
    pub ready_answers_changed: u64,
    /// How many times [`crate::interaction::draw_use_time_with_chat_focus`]'s step 3 raised
    /// the object-found notice from the arm that has **no scene to sweep**. The
    /// notice block sits outside the player-presence guard,
    /// so an armed pick in a scene-less frame is answered with `click_object_id == 0` rather than
    /// dropped.
    pub scene_less_notices: u64,
    /// `set_requested_attack_height` calls a key press or a combat-window
    /// button press made. Counted rather than inferred from the field, because the client's own
    /// guard means a repeat of the *same* height while a request is in progress does nothing —
    /// so "the field says Low" and "the arm ran" are different questions.
    pub attack_height_changes: u64,
    /// `end_attack_request` calls that put at least one attack on the wire.
    /// A release with nothing charged, or below the slider cap, ends the request and sends
    /// nothing; this counts the swings, not the releases.
    pub attacks_released: u64,
    /// Writes to the UI-requested power — the power (melee) / accuracy (missile)
    /// gauge. Both producers land here: the four `CombatDecrease`/`Increase` keys through
    /// [`crate::interaction::Interaction::on_actions`] and the window's scrollbar through
    /// `UiRequest::CombatSetDesiredPower`.
    pub desired_power_changes: u64,
    /// Magic-combat actions `handle_magic_action` turned into a
    /// magic-action notice. The spellcasting panel applies the notice, so this is the producer's
    /// count and the panel's receipt count is the consumer's — two numbers, because a
    /// key that dispatches and a spell bar that answers are different claims.
    pub magic_actions: u64,
    /// `Notice`s `dereth_client_model` raised while this module was driving it — the refusal
    /// strings among them.
    pub notices: u64,
    /// Target-mode assignments the toolbar's Use / Examine buttons made.
    pub target_modes_armed: u64,
    /// Shortcut additions requested by a drop onto the quickbar.
    pub shortcuts_added: u64,
    /// Shortcut-removal events (`0x019D`) sent this session. Counted apart from
    /// [`Self::shortcuts_added`] because a drop from one tile to another
    /// sends both, and a build that sent only the add would silently leave one object in two slots.
    pub shortcuts_removed: u64,
    /// Right-button releases that ended a **mouse-look drag** and therefore did *not*
    /// arm `SearchReason::Examine` — the mouse-up handler's early return when it turns mouse look
    /// off. Without it every camera turn would appraise whatever was under the cursor.
    pub mouse_look_releases: u64,
    /// `Item_ServerSaysContainID`, wear-item and move-item applies.
    pub move_items_applied: u64,
    /// `0x0022 Item_ServerSaysContainID` replies whose item did **not** exist yet and
    /// which therefore took the second branch of the client's net-blob handler —
    /// recording the pending contained id on the container.
    ///
    /// Separate from `move_items_applied` on purpose: the two branches of one arm do opposite
    /// things (one moves an object, one records an id for an object that does not exist), and a
    /// single counter would let a build that never reached the hard branch report a healthy total.
    pub contain_ids_preplaced: u64,
    /// `Character_ServerSaysAttemptFailed` applies.
    pub attempts_failed: u64,
    /// A `0x00A0` whose object, after previous-request-id substitution,
    /// is not a known game record. The missing-object branch skips failure handling
    /// entirely.
    ///
    /// A third state prevents silence: without
    /// it, "the arm ran and the lock was already idle" and "the arm did nothing at all" are the
    /// same reading of `attempts_failed`, and only the second one leaves a held lock behind.
    pub attempts_failed_unknown_object: u64,
    /// `0x0197 Item_UpdateStackSize` applies — a **source** stack's new count after a
    /// split or a merge, which is also the only thing that releases the inventory lock a split
    /// took. Counted apart from `move_items_applied` because they answer different halves of one
    /// split: that one is the new object's placement, this one is the old object's count.
    pub stack_sizes_applied: u64,
    /// `0x0196 Item_OnViewContents` applies — a container's contents list replaced.
    pub contents_viewed: u64,
    /// `0x0052 Item_StopViewingObjectContents` applies.
    pub contents_closed: u64,
    /// `0x00C9 Item_SetAppraiseInfo` applies.
    pub appraisals_applied: u64,
    /// `0x00B4 Writing_BookOpen` applies -- how many books the panel was told about.
    pub books_opened: u64,
    /// `0x00B8 Writing_BookPageDataResponse` replies that filled a page of the open
    /// book. `.page_data_ignored` is the ones that did not.
    pub book_pages_filled: u64,
    /// `0x01B8 Combat_HandleCommenceAttackEvent` applies.
    pub attacks_commenced: u64,
    /// `0x01A7 Combat_HandleAttackDoneEvent` applies.
    pub attacks_done: u64,
    /// `0x01C0 Combat_QueryHealthResponse` blobs that reached the arm.
    pub health_responses: u64,
    /// `0x0264 Item_QueryItemManaResponse` blobs that reached the arm.
    pub mana_responses: u64,
    /// How many of those responses actually wrote a toolbar meter.
    ///
    /// This is a **separate** counter from the two above on purpose. Both handlers begin with
    /// a selected-is-`id` test and retail drops the rest, so "reached the arm" and "changed
    /// what is drawn" are different numbers and a single counter would let one masquerade as the
    /// other.
    pub selection_meters_written: u64,
    /// `0x01C7 Item_UseDone` applies — one busy-count decrement each.
    pub uses_done: u64,
    /// `0x0020 Allegiance_AllegianceUpdate` applies.
    pub allegiance_updates: u64,
    /// The member count of the tree the last `0x0020` rebuilt.
    pub allegiance_members: u32,
    /// `0x0062 Vendor_VendorInfo` handled — vendor windows opened.
    ///
    /// This counter at zero on a session that carries eight of them means `0x0062` is not
    /// being handled.
    pub vendor_opens: u64,
    /// Stock rows the last `0x0062` carried, after unpacking item profiles.
    pub vendor_stock: usize,
    /// `0x005F Vendor_Buy` put on the wire — `buy_single_item` and "Buy All".
    pub vendor_buys: u64,
    /// `0x0060 Vendor_Sell` put on the wire — `sell_single_item` and "Sell All".
    pub vendor_sells: u64,
    /// Rows added to the buy basket by "Add to List". Local; nothing is sent.
    pub vendor_basket_rows: u64,
    /// Vendor stack-size writes applied to a description
    /// the world actually holds. Zero with a non-zero
    /// `VendorPanel::stack_size_writes` means the panel decided and nothing received it.
    pub vendor_stack_sizes_set: u64,
    /// Vendor-close requests that actually closed a vendor.
    pub vendor_closes: u64,

    // ---- fellowship -------------------------------------------------------------------
    /// Fellowship requests this client has put on the wire — all seven opcodes
    /// together, because the question the counter answers is "does the Fellowship tab reach the
    /// shard at all".
    pub fellowship_requests: u64,

    // ---- friends ----------------------------------------------------------------------
    /// Add-friend, remove-friend, clear-friends and friends-command requests
    /// put on the wire -- all four together, because the question
    /// is "does the Friends tab, or `@friends`, reach the shard at all".
    pub friends_requests: u64,
    /// Add-friend refusals when the list has more than `0x31` entries -- weenie error `0x561`,
    /// whose sentence this path does not print, so it is counted instead.
    pub friends_list_full_refusals: u64,

    // ---- contracts, friends commands and fellowship dialogs ---------------------------
    /// Abandon-contract requests (`0x0316`) put on the wire. The Contracts
    /// tab's one send.
    pub contract_requests: u64,
    /// The remove-friend chat command walking the whole list and finding no
    /// such name -- weenie error `0x563`, same treatment. `@friends remove <name>` takes a name
    /// and `0x0017` takes an id, so a name nobody answers to sends nothing at all.
    pub friends_not_a_friend_refusals: u64,
    /// The display-friends chat command runs -- `@friends` and
    /// `@friends online`. The one arm of the family that prints rather than sends.
    pub friends_listings: u64,
    /// `0x0274` bodies of type 4 that reached the fellowship panel and raised a
    /// request dialog. Counted separately because gameplay and fellowship dialogs
    /// have independent context guards and may be open simultaneously.
    pub fellowship_requests_raised: u64,

    // ---- pop-up strings ---------------------------------------------------------------
    /// `0x0004 Communication_PopUpString` bodies decoded and queued for a message dialog.
    pub pop_up_strings: u64,
    /// Popup bodies whose narrow string could not be decoded, matching the original
    /// early return. This counter is separate because a missing popup must be
    /// distinguishable from a decode failure elsewhere.
    pub pop_up_strings_undecodable: u64,
    /// Dialogs made in the current UI this session — how many of [`Self::pop_up_strings`] actually
    /// reached the screen. Two counters and not one for the reason
    /// `HudStats::fellowship_members` gives: a shell that is not yet in `GAME_PLAY` drains none,
    /// and "received but not shown yet" must not read the same as "shown".
    pub pop_ups_shown: u64,
    /// Popups the player dismissed with the box's one button (child `0x26`) —
    /// whose whole body is `CloseDialog`.
    pub pop_ups_dismissed: u64,

    // ---- confirmation requests --------------------------------------------------------
    /// `0x0274` requests queued for a gameplay confirmation or an accept-swear dialog.
    /// Fellowship invitations have their own counter. Together with the retained
    /// absent-panel and unknown-type counters, these distinguish handled questions
    /// from unsupported destinations and types.
    pub confirmations_raised: u64,
    /// Retained counter for confirmations dropped because their panel was absent.
    /// Allegiance type 1 and fellowship type 4 go to their own panels, so no current
    /// handled type reaches this counter; it stays zero. Unknown types are counted
    /// separately.
    pub confirmations_for_absent_panels: u64,
    /// `is_handled` said no: the confirmation dispatcher has no default arm, so retail drops these too.
    pub confirmations_unknown_type: u64,
    /// `0x0276` bodies that matched the open dialog and closed it —
    /// the abort-confirmation handler past both of its mismatch guards.
    pub confirmations_aborted: u64,
    /// `0x0276` bodies whose `(type, context)` pair matched nothing open; retail's two
    /// mismatch guards drop them in silence.
    pub confirmations_aborts_unmatched: u64,
    /// Answers that reached the gameplay confirmation dialog's close handler, Yes and No alike.
    pub confirmations_answered: u64,

    // ---- world-object stat updates ----------------------------------------------------
    /// `Qualities_*Update*` events for a **world object** that reached
    /// the world's stat-update path and were written.
    ///
    /// The twenty-six opcodes `0x02CD`..=`0x02EA` arrive here for any subject; the HUD-owned
    /// player-description route rejects other subjects. Without this arm a world-object stat
    /// update would miss its public mirror, sequence gate and item-attributes-changed notice,
    /// so a chest unlocked by the server would stay locked locally. The quality store, sequence
    /// gate and player-scoped handlers live in `dereth_client_model::World`;
    /// `Hud::apply_quality_update` delegates to that shared store rather than owning a second
    /// player copy.
    pub object_quality_updates: u64,
    /// Public updates the object's own `PropertySequenceGate` refused as older than what it holds.
    pub object_quality_updates_stale: u64,

    // ---- world-object stat removals -----------------------------------------------------
    /// `Qualities_*Remove*Event` for a **world object** that reached
    /// the world's stat-removal path — the eight public forms, `0x01D2`, `0x01D4`,
    /// `0x01D6`, `0x01D8`, `0x01DA`, `0x01DC`, `0x01DE` and `0x02B9`.
    ///
    /// The removal counterpart of [`Self::object_quality_updates`]: this arm is
    /// `apply_stat_remove`'s production caller. Counted separately from the updates because
    /// removing a stat is a different retail path — no value, no stat-updated mirror, and the
    /// remove handler runs instead.
    pub object_quality_removes: u64,
    /// Public removes the object's own `PropertySequenceGate` refused, or that named no live row.
    pub object_quality_removes_stale: u64,
    /// `SelectionPickUp` presses that reached backpack placement with a live selection —
    /// player-action case 1.
    pub pick_ups: u64,

    // ---- trade ------------------------------------------------------------------------
    /// `0x01FD Trade_RegisterTrade` handled — negotiations the mirror opened.
    pub trade_registers: u64,
    /// `0x01FE Trade_OpenTrade` handled. **This alone does not raise the trade
    /// window.** Trade registration raises it instead;
    /// the open-trade event reaches a no-op handler. ACE never sends `0x01FE`, so
    /// `trade_registers` is the useful counter on that server path.
    pub trade_opens: u64,
    /// `0x01FF Trade_CloseTrade` handled.
    pub trade_closes: u64,
    /// `0x0200`/`0x0201` that actually moved a row in the mirror. A message whose side is neither
    /// 1 nor 2 moves nothing and still raises its notice, which is the client's behaviour, so this
    /// is deliberately not the message count.
    pub trade_rows_changed: u64,
    /// `0x0202`/`0x0203`/`0x0205`/`0x0207`/`0x0208` handled — the five that only change flags.
    pub trade_flag_messages: u64,
    /// `0x01F8 Trade_AddToTrade` put on the wire by a drop on the table.
    pub trade_adds_sent: u64,
    /// `0x0055 StackableSplitToContainer` put on the wire by a partial-stack trade drop.
    pub trade_splits_sent: u64,
    /// `0x01FA Trade_AcceptTrade` put on the wire carrying the **mirror** — the accepting branch
    /// of the trade-accept handler.
    pub trade_accepts_sent: u64,
    /// `0x01FA` sent with an empty trade payload to report desynchronization. Counted
    /// separately from acceptance because the opcode is identical and only the body
    /// differs; a silently unused path could otherwise look like missing desync
    /// detection.
    pub trade_out_of_sync_sent: u64,
    // ---- trade offers by drag ---------------------------------------------------------
    /// The automatic trade-offer notice reached its `AddItem` arm — the
    /// **drag an item onto another player** gesture completing.
    pub trade_for_dummies_offered: u64,
    /// The same notice refused: the id was already on the table (silent), or the splitter was
    /// holding part of the selected stack and the window said so.
    pub trade_for_dummies_refused: u64,
    /// `0x01FB`, `0x0204` and `0x01F7` — decline, clear-all and close-negotiations.
    pub trade_control_sent: u64,
    /// `0x02C2 Magic_UpdateEnchantment` applies the registry accepted.
    pub enchantments_updated: u64,

    // ---- enchantment purges, squelch and spell sends -----------------------------------------
    /// `0x02C6 Magic_PurgeEnchantments` that removed at least one enchantment. The purge has two
    /// outcomes and a third state: this counts only the ones that changed something, so a session
    /// where the server sent one and the registry was already empty reads as **0 changed**, not as
    /// "the arm never ran" — `enchantments_updated` above is the denominator for that.
    pub enchantments_purged: u64,
    /// `0x0312 Magic_PurgeBadEnchantments` that removed at least one.
    pub bad_enchantments_purged: u64,
    /// `0x0058 Communication_ModifyCharacterSquelch` or sibling `0x0059` put on the wire by the
    /// chat target menu's squelch row or a typed `@squelch`/`@unsquelch`. **Zero for every recorded
    /// session**: the corpus carries neither event in either direction, so this counter can only
    /// move in a socket-free command station, on a live shard, or on a loopback.
    pub squelch_requests: u64,
    /// `0x0224 Character_SetDesiredComponentLevel` sent by the spell-component panel's edit field
    /// or the typed fill-components clear sentinel. Also **zero in the corpus**.
    pub desired_comp_sets: u64,
    /// `0x0048 Magic_CastUntargetedSpell` / `0x004A Magic_CastTargetedSpell` actually put on the
    /// wire by the spellcasting panel.
    ///
    /// It counts **sends**, not requests: a spell the spell table does not know makes
    /// spell casting return without sending and without a message, so
    /// a counter keyed on "the arm ran" would report a cast that never happened. Zero in every
    /// capture — the corpus carries no `0x0048` and no `0x004A` in either direction.
    pub spells_cast: u64,
    /// `0x01E3`/`0x01E4` pairs put on the wire by the spellbook -> spell-bar
    /// transfer: the denominator that separates "the drop was refused" from "the drop never
    /// reached a request".
    pub spell_favorites_changed: u64,
    /// `0x01A8 Magic_RemoveSpell` sent by a *confirmed* DELETE.
    pub spells_deleted: u64,
    /// Rows added to the buy basket.
    pub fill_components_rows: u64,
    /// Components it could not fill — not stocked at all, plus stocked short. Both go into the
    /// same `add_missing_comp` line in the client, which is why they are summed here and counted
    /// apart in `dereth_client_model::vendor::FillComponents`.
    pub fill_components_missing: u64,
    /// `0x02C7 Magic_DispelEnchantment` applies that found their layer.
    pub enchantments_removed: u64,

    // ---- the count-prefixed trio and the expiry line ----------------------------------------
    /// `0x02C4 Magic_UpdateMultipleEnchantments` messages consumed.
    pub multi_enchantment_updates: u64,
    /// …and the entries inside them the registry accepted. Two counters because
    /// the registry update's leading parity check can reject every entry of
    /// a well-formed list, and a single counter cannot tell that from an empty list.
    pub multi_enchantments_applied: u64,
    /// `0x02C5 Magic_RemoveMultipleEnchantments` (announcing) messages consumed.
    pub multi_enchantment_removals: u64,
    /// `0x02C8 Magic_DispelMultipleEnchantments` (silent) messages consumed. Kept apart from the
    /// counter above because the **only** difference between the two opcodes is the `bool` the
    /// dispatcher pushes, so folding them would make the silent/announcing split unobservable.
    pub multi_enchantment_dispels: u64,
    /// Ids those two took out of the registry.
    pub multi_enchantments_removed: u64,
    /// *"&lt;spell&gt; has expired."* lines written by
    /// enchantment-expiration handling, from `0x02C3` and `0x02C5`.
    /// It is **not** the number of ids removed: a cooldown (`id & 0xFFFF >= 0x8000`) and a spell
    /// the spell table does not know both take an early return with no line.
    pub enchantment_expiry_lines: u64,

    // ---- the book's authoring replies ---------------------------------------------------------
    /// `0x00B6 Writing_BookAddPageResponse` consumed.
    pub book_add_page_responses: u64,
    /// …of which got past the add-page response handler's **two id refusals** (the object is not
    /// the book, or the book id is zero) and reached the panel.
    ///
    /// It is **not** the number of pages inserted: the two later arms (a failed add, or a page
    /// other than the current one) are counted past this point, in
    /// `dereth_ui_screens::panels::book::BookPanel::book_data_refetches`, because only the panel
    /// knows the current page. `BookPanel::pages_added` is the insert count.
    pub book_add_pages_relayed: u64,
    /// `0x00B7 Writing_BookDeletePageResponse` consumed. **A counter and nothing else**: notice
    /// handling is an empty stub in
    /// **every** receiver — nothing in retail handles it. See the arm.
    ///
    /// The refetch count is
    /// `dereth_ui_screens::panels::book::BookPanel::book_data_refetches`: the decision belongs to
    /// the panel, because only the panel knows the current page.
    pub book_delete_page_responses: u64,

    // ---- three empty handlers -----------------------------------------------------------------
    /// `0x01CB Item_AppraiseDone` consumed. Its handler calls an empty
    /// stub returning zero — the same one behind
    /// `0x01C9`. A counter, for the reason [`Self::appraise_done`]'s doc gives.
    pub appraise_done: u64,
    /// `0x00C3 Item_GetInscriptionResponse` consumed. The arm decodes and discards
    /// three narrow strings, with no notice or further call.
    pub inscription_responses: u64,
    // ---- allegiance ---------------------------------------------------------------------------
    /// `0x027C Allegiance_AllegianceInfoResponseEvent` consumed.
    pub allegiance_info_responses: u64,
    /// …and how many printed a report. They differ by exactly the responses whose profile does not
    /// contain the target, which is the handler's one guard (a failed profile lookup returns).
    pub allegiance_info_reports: u64,
    /// `0x0003 Allegiance_AllegianceUpdateAborted` consumed.
    /// The panel's receiver updates only when the panel is visible
    /// and **ignores the `u32` it is handed**; the queue length is the count
    /// the panel watches and this is the message count at the router.
    pub allegiance_updates_aborted: u64,
    // ---- the two channel reports and /age -------------------------------------------------------
    /// `0x0148 Communication_ChannelList` consumed — despite the name, the list of **characters**
    /// listening on a channel (the `Communication_ChannelList` handler).
    pub channel_lists: u64,
    /// `0x0149 Communication_ChannelIndex` consumed — the list of **channels**
    /// (the `Communication_ChannelIndex` handler).
    pub channel_indices: u64,
    /// Indented rows the two of them wrote, **not** counting the header each always writes. Two
    /// counters because the header goes out for an empty list too, and a single one could not tell
    /// "no reply" from "a reply naming nobody".
    pub channel_rows: u64,
    /// `0x01C3 Character_QueryAgeResponse` consumed — `/age`'s one chat line.
    pub age_responses: u64,
    // ---- portal storms ------------------------------------------------------------------------
    /// `0x02C9`/`0x02CA`/`0x02CB`/`0x02CC` consumed, in that order.
    pub portal_storms_brewing: u64,
    pub portal_storms_imminent: u64,
    pub portal_storms_struck: u64,
    pub portal_storms_subsided: u64,
    /// Portal-storm notice deliveries — every one of the four
    /// handlers makes exactly one, so this equals the sum of the four counters above. The **level**
    /// itself is the indicator state: it is not a count.
    pub portal_storm_levels: u64,
    /// `0xF630 Character_SetPlayerVisualDesc` consumed, the only one of this group that is not a
    /// game event. The arm decodes one narrow string and passes it to an empty
    /// handler.
    pub player_visual_descs: u64,

    /// `0x01A1 Character_CharacterOptionsEvent` bodies put on the wire — the
    /// whole `PlayerModule`, re-packed from the blob the server sent.
    pub player_modules_sent: u64,
    /// Polls of live object-range registrations and the exit edges
    /// they produce. `range_exits` counts panels or selections closed by distance;
    /// `range_exits_without_a_window` distinguishes an exit with no consumer. Book
    /// and housing delivery both have consumers.
    pub range_polls: u64,
    pub range_exits: u64,
    pub range_exits_without_a_window: u64,
    /// The selection range-exit handler's
    /// `is_selected_object_in_view` arm: the selection watch re-arming instead of clearing.
    ///
    /// It stays at 0 unless the latch's producer runs; a driven selection-persistence run reads
    /// 6 of 6 range exits here. See the world's selected-object visibility state.
    pub range_selection_rearms: u64,
    /// The other arm of the same test: selection range exits that ran
    /// selection assignment with arguments `(0, 0)` and **emptied the selection**.
    ///
    /// It is the complement of `range_selection_rearms` over
    /// the selection exits whose id still matched.
    ///
    /// The latch `selected_object_in_view` is set by part drawing — here, the draw path's own
    /// `WorldScene::draw` — for any selected object whose parts a frame has submitted, and cleared
    /// only by an object-search request. Without that producer every selection range exit would
    /// take the clearing arm. What is left in this counter is the case that clears in retail too:
    /// a selection no frame has drawn.
    pub range_selection_clears: u64,
    /// Option changes that the player-option change handler sends
    /// out **immediately** as `0x0005 Character_PlayerOptionChangedEvent` — twenty-two of the
    /// fifty-three — and that this client could not send.
    ///
    /// **The message exists, so this counter does not move and stays at zero.** It is kept as
    /// the gate that makes a regression visible, and a test asserts it. Nothing increments it
    /// today.
    pub option_changes_unsendable: u64,
    /// Option changes actually put on the wire as `0x0005`, the counterpart of
    /// [`Self::option_changes_unsendable`]. The five listening options among them are chat channel
    /// subscriptions, so this is also the count of channel join/leave requests a session made.
    pub option_changes_sent: u64,
    /// Option changes that only marked the module dirty, exactly as the client
    /// does: they go out at the next `save_to_server` or the 480-second flush.
    pub option_changes_deferred: u64,
    /// `PlayerOption_*` input actions that flipped an option.
    pub option_actions_toggled: u64,
    /// The player-option change handler's engine side effects
    /// (time of day, fog, weather, target tracking) that were decided and **not applied**, because
    /// the landscape, the weather and the target tracker are not wired to this module. Counted so
    /// that "the option did nothing visible" is a number rather than a silence.
    pub option_side_effects_unapplied: u64,
    /// `0x01BF Combat_QueryHealth` and `0x0263 Item_QueryItemMana` sent by
    /// toolbar selection-change handling — the clears with id 0 and the
    /// per-selection queries together.
    pub vital_queries: u64,
    /// `0x001F Allegiance_UpdateRequest` sent by
    /// allegiance-panel visibility changes — the subscribe on show and the
    /// unsubscribe on hide. The roster has no other source, so without these the Allegiance
    /// tab does nothing.
    pub allegiance_update_requests: u64,
    /// How many Swear / Break / Kick *questions* the panel's buttons raised, and how
    /// many of each the player then confirmed. Without the three button ids bound here the tab's
    /// only working gesture would be opening it.
    pub allegiance_confirmations_raised: u64,
    /// `0x001D Allegiance_SwearAllegiance` sent out of the swear dialog's Yes arm.
    pub allegiance_swears: u64,
    /// `0x001E Allegiance_BreakAllegiance` on the **patron**, out of the break dialog's Yes arm.
    pub allegiance_breaks: u64,
    /// `0x001E Allegiance_BreakAllegiance` on a **vassal**, out of the kick dialog's Yes arm.
    /// The same opcode as `allegiance_breaks` and a different argument; see
    /// the world's allegiance-break handling.
    pub allegiance_kicks: u64,
    /// A break confirmed after the patron had already gone. Retail would put a zero id on the
    /// wire; this build declines and counts.
    pub allegiance_breaks_without_patron: u64,
    /// Allegiance events raised by `@allegiance`, `@motd` and `@alh`: the
    /// twenty-four opcodes without allegiance-panel buttons. Without the four
    /// associated command handlers every one would answer "That is not a valid command." and
    /// this counter would stay zero.
    pub allegiance_command_requests: u32,
    /// Chat lines taken off the entry box and put through
    /// the chat-command handler.
    pub chat_lines_sent: u64,
    /// Paper-doll drops that took `accept_drag_object`'s
    /// auto-wield branch and put a **single** location bit on the wire.
    pub wields_requested: u64,
    /// Paper-doll drops that took the `auto_wear` branch and sent the
    /// **whole** valid-locations mask.
    ///
    /// The two counters are separate — rather than one total — because the split
    /// between them *is* the decision this path makes, and a build that always took one
    /// branch would still show a plausible total.
    pub wears_requested: u64,
    /// Double-clicks (and Use buttons, and the `USE` action) that took
    /// `determine_use_result`'s arm 2 and put a put-item-in-container or a stackable merge
    /// on the wire. **This is the pickup**, and it is counted apart from
    /// `requests_sent` because a build that sent the use request for every double-click would
    /// also show a healthy `requests_sent`.
    pub pickups_requested: u64,
    /// Arms this build reaches and does not dispatch —
    /// open-trade, the salvage panel and the minigame start, plus a wield that `plan_auto_wield` found
    /// blocked. Reported rather than silent: the gesture was understood and nothing was sent.
    pub uses_undispatched: u64,
    /// Paper-doll drops onto an **occupied** slot that started
    /// `auto_wield`'s unblock — the blocker's `0x0019` put-item-in-container went on
    /// the wire and the dropped item's `0x001A` is owed until the server confirms.
    ///
    /// Counted apart from `wields_requested` because both are "the drop was accepted and something
    /// was sent".
    pub unblocks_started: u64,
    /// Server-says-move-item retries that put the owed `0x001A`
    /// on the wire. `unblocks_started - unblock_retries` is how many are still in flight or were
    /// abandoned; the client has **no timeout** on either.
    pub unblock_retries: u64,
    /// Unblocks the server refused (the attempt-failed notice),
    /// or whose retry found nowhere to wield after all. The dropped item's icon un-ghosts.
    pub unblocks_abandoned: u64,

    // ---- item-use completion and the arms that open a panel ----
    /// Container-open notices from a double-click on one of the player's own
    /// packs. Counted apart from `requests_sent` because it sends **nothing**: it is a panel
    /// request, and a build that never raised it looks identical on the wire.
    pub contained_containers_opened: u64,
    /// Ground-object requests that actually changed the ground-container id:
    /// a corpse or a chest became the open ground container.
    pub ground_objects_requested: u64,
    /// `attempt_set_ground_object` calls that were **understood and refused**: not `OPENABLE`, no
    /// weenie at all, or the inventory lock. Its own third state, so that "nobody double-clicked a
    /// corpse" and "every corpse refused to open" cannot report the same number.
    pub ground_objects_refused: u64,
    /// View-contents responses: `0x0196` replies that were for
    /// the **requested** ground object and therefore raised the panel.
    ///
    /// `contents_viewed - ground_panels_opened` is every other container's contents, which is the
    /// common case; this counts only the ones that opened a window.
    pub ground_panels_opened: u64,
    /// `0x0052 Item_StopViewingObjectContents` replies that closed the ground container, as
    /// opposed to merely dropping a contents list.
    pub ground_panels_closed: u64,
    /// The three object-use confirmations — a PK altar, an NPK altar or a
    /// volatile rare. **Nothing was sent**; the dialog's `Yes` sends the usage confirmation.
    pub usage_confirmations: u64,
    /// The item-use result's arm 5: `0x01F6 Trade_OpenTradeNegotiations` went out.
    pub trades_requested: u64,
    /// The item-use result's arms 6 and 7 — the salvage panel and the minigame start. Neither
    /// sends anything in retail either, so this counter is the only evidence they ran.
    ///
    /// It also counts the salvage half. The notice is carried to `panels::salvage` as well, so
    /// a non-zero here with [`Self::salvage_panel_notices`] at zero means the minigame arm and
    /// nothing else.
    pub panels_requested: u64,
    /// Salvage-panel notices queued by [`crate::interaction::Interaction::absorb`] —
    /// the producer-side count, so a zero here and a non-zero `panels_requested` localises the
    /// break to the sink rather than to the use path.
    pub salvage_panel_notices: u64,
    /// `UiRequest::SalvageItems` that put an `0x027D` in the outbox.
    pub salvage_requests: u64,
    /// Notices delivered to the chess panel —
    /// chess boards double-clicked. A non-zero [`Self::panels_requested`] with a zero here is the
    /// salvage arm alone, and this counter is the whole of "the board window opened".
    pub minigame_boards_used: u64,
    /// Chess messages the window put in the outbox -- the `0x0269` a use sends,
    /// plus the `0x026A`/`0x026B`/`0x026D`/`0x026E` its three buttons and its board send.
    pub minigame_requests: u64,
    /// Chess gestures the panel raised that the model took -- a button click, a
    /// board press that resolved to a square, or a resign-dialog answer.
    ///
    /// Separate from [`Self::minigame_requests`]: selecting a piece or refusing an
    /// invalid move is a handled gesture that sends nothing. Diverging counts can
    /// therefore show working local rules rather than broken delivery.
    pub minigame_gestures: u64,
    /// `UiRequest::DisplayChatText` lines a panel put in the scroll.
    pub panel_notice_strings: u64,
    /// `UiRequest::HousePayment` that put an `0x021C` or an `0x0221` in the
    /// outbox through the housing panel's payment operation.
    pub house_payments_sent: u64,
    /// Partial-stack slumlord drops that reached the generic inventory split
    /// request. The authoritative create selects the result; this does not count the second,
    /// whole-stack drop that adds the payment row locally.
    pub house_splits_sent: u64,
    /// `UiRequest::HouseQueryLord` that put an `0x0258` in the outbox —
    /// the failed-house-transaction handler's retry, which is the client's **only**
    /// sender of the query-lord event.
    pub house_lord_queries: u64,
    /// Object use's general path refused with one of the client's own six reasons.
    pub uses_refused: u64,

    // ---- tell, reply and retell --------------------------------------------------------------
    /// Tells that reached the wire: one `0x005D Communication_TalkDirectByName` per `@tell` and
    /// `@retell`, one `0x0032 Communication_TalkDirect` per `@reply`.
    pub tells_sent: u64,
    /// Chat commands that were understood and **refused with one of the client's own four
    /// strings**, raised as `Notice::DisplayString { channel: 0x1A, .. }`. Its own counter, not
    /// folded into `requests_refused`, so that a missing handler shows as a zero here.
    pub chat_commands_refused: u64,
    /// The retell command's empty-line arm — the one place the client understands a verb and
    /// answers with
    /// **nothing at all**. Counted so that "retail is silent here" and "this build lost the line"
    /// are different readings; every other silent drop in this arm is now one of the three above.
    pub tells_dropped_silently: u64,
    // ---- the notice → screen chain ---------------------------------------------------------
    /// Strings handed to [`dereth_client_model::scroll::Scroll`]: one per `Notice::DisplayString`,
    /// plus three combat refusal paths that write directly to the scroll.
    ///
    /// **This counter is the seam.** `notices` above counts everything the sink saw; this counts
    /// what reached a surface that draws, so a zero here beside a non-zero `notices` means
    /// nothing reached the screen.
    pub notice_strings_scrolled: u64,
    /// Verbs the command table resolves to a handler this build does not dispatch.
    ///
    /// Every handler the command table names is dispatched (the handler census test holds the list
    /// of undispatched ones empty), so this stays at zero. It is a tripwire: a verb that lost its
    /// handler would answer weenie error `0x26`'s "That is not a valid command." where retail runs
    /// the handler, and would count here, so a non-zero value is a regression rather than a silent
    /// change of behaviour.
    pub chat_commands_unimplemented: u64,
    /// `@loc` lines the `@loc` handler printed to the chat scroll.
    /// The two refusals (any argument, cell `0`) count under
    /// [`Self::chat_commands_refused`].
    pub loc_lines_printed: u64,
    // ---- the character, comms, consent and local families -------------------------------------
    /// Requests the twenty-four `dereth_client_model::chat_cmd` handlers put on the wire. Kept apart from
    /// [`Self::allegiance_command_requests`] and [`Self::friends_requests`] so a family that
    /// regresses to the catch-all shows as a zero here rather than being masked by a neighbour.
    pub chat_command_requests: u32,
    /// Lines the same handlers printed — every refusal, acknowledgement and listing, at whatever
    /// chat type retail passes. The refusals also count under [`Self::chat_commands_refused`].
    pub chat_command_lines: u64,
    /// `@die` questions raised. The `0x0279` itself is counted by
    /// [`Self::chat_command_requests`] when the Yes arm runs, so a raised question with no send is
    /// visible as the difference.
    pub die_confirmations_raised: u32,
    // ---- `@house abandon`'s two stages -------------------------------------------------------
    /// Callback dialogs raised with the first house-abandon callback —
    /// questions raised — the **first** *"Do you really want to abandon your house?"*.
    pub house_abandon_first_raised: u32,
    /// The first house-abandon callback's Yes arm — the **second**
    /// *"Are you absolutely certain…"* question. Counted separately because the whole point of
    /// the two-stage dialog is that the first Yes sends nothing, so
    /// `house_abandon_first_raised > house_abandon_second_raised` is the shape of a refusal at
    /// stage one and `second_raised > 0` with no `0x021F` the shape of one at stage two.
    pub house_abandon_second_raised: u32,
    /// `*name*` and `<name>` runs resolved out of a
    /// spoken line. A run that misses `inq_chat_pose_command` is silent and does not count here.
    pub poses_resolved: u64,
    /// Soul-emote (`0x01E1`) messages a pose put on the wire — fewer
    /// than [`Self::poses_resolved`] when a pose's emote for others is empty.
    pub soul_emotes_sent: u64,
    /// Local `"You wave."` echoes a pose printed — the `Communication_HearSoulEmote` handler run
    /// with sender 0, `"You"` and the pose's own emote. Counted apart from the send because the
    /// two carry **different** strings.
    pub pose_echoes_printed: u64,
    // ---- the channel-command words ------------------------------------------------
    /// Channel broadcasts (`0x0147`) sent because one of the channel-command handler's nineteen
    /// command words was typed. Kept apart from the talk-focus dropdown's own `0x0147`s, because
    /// one counter for both would hide a typed half stuck at zero behind a working dropdown half.
    pub channel_commands_sent: u64,
    /// The channel command's `argc <= 0` arm: a bare `@f` with nothing to say. Retail prints
    /// [`crate::interaction::CHANNEL_COMMAND_NEEDS_TEXT`] and **returns `true`**, so this is not a refusal and is not
    /// counted as one.
    pub channel_commands_without_text: u64,
    // ---- parent-container changes -----------------------------------------------------
    /// New-parent-container deliveries that actually **changed**
    /// the active pack — i.e. those that passed both of
    /// the new-parent-container handler's guards and named a different pack.
    ///
    /// Its own counter rather than a fold into `ui_requests_handled`, because the two failure
    /// modes are opposite: "the panel never told anyone" and "the panel told us about a container
    /// the object tables have never seen, or one the player does not own" are different bugs, and
    /// only this counter can tell a wired seam from a refused one.
    pub open_containers_changed: u64,

    // ---- the tab-target cycle ----------------------------------------------------------------
    /// Selection-cycle calls made by one of the **sixteen**
    /// player-action selection arms — the *first* call of each arm, one per
    /// action delivered.
    ///
    /// Counted apart from [`Self::selection_cycle_wraps`] because the two answer different
    /// questions and a single total cannot: "the arm ran" and "the arm ran and had to wrap" are
    /// the first and second halves of every non-"Closest" action, and an arm whose wrap never
    /// fires looks exactly like an arm that always finds something.
    pub selection_cycles: u64,
    /// The **second** call the twelve non-"Closest" arms make when the selected id did not move.
    /// Retail compares the selected id before and after the first call. This is the
    /// wrap-around: "Next" past the farthest object comes back to the nearest.
    pub selection_cycle_wraps: u64,
    /// `0x1000003E` / `0x1000003F` deliveries whose winning selection was a corpse and therefore
    /// reached item use. The other fourteen arms never touch it, and
    /// `0x10000121` / `0x10000122` deliberately select and stop.
    pub selection_corpse_uses: u64,
    /// Selection actions that ran against an **empty** geometry snapshot — no local body, so
    /// [`crate::selection_geometry::SceneSelectionPhysics`] could answer for nothing and the cycle
    /// could not select anything. A denominator: without it "the cycle selected nothing because
    /// every candidate was filtered" and "the cycle selected nothing because it was asked on a
    /// loading screen" are the same silence.
    pub selection_cycles_without_geometry: u64,
    /// `SelectionLastAttacker` (`0x10000038`) deliveries — player-action `case 0xD`,
    /// which is the only caller of the radar-range check.
    ///
    /// Counted apart from [`Self::selection_cycles`] because it is not a `select_next` cycle at
    /// all: it selects one nominated id or nothing, and folding it into the cycle count would
    /// make a dead arm and a live one read alike.
    pub selection_last_attacker: u64,
    /// Next/previous fellowship selection reaching the object-selection assignment
    /// — **not** merely the arm running. Both functions have early returns that select
    /// nothing (no fellowship at all; an empty one, for `Next`), and a counter that could not tell
    /// those from a delivered selection would make the dead case and the live one read alike.
    pub selection_fellow_cycles: u64,
    /// Player-action `case 0x1000002E` reaching its
    /// selection assignment — i.e. the previous selected id was non-zero.
    /// The zero-history early return leaves this alone, which is the distinction between
    /// *"P did nothing because there is no history"* and *"P has no arm"*.
    pub selection_previous_restores: u64,
    /// Actions of input map `0x10` that reached the system-key handler's
    /// unconditional success return — consumed and deliberately doing nothing.
    ///
    /// A counter for a body that is empty on purpose looks odd until you ask how else a test can
    /// tell *"the arm ran and did nothing"* from *"the action fell through `_ => {}`"*. The
    /// unconsumed-event count answers the second question; this answers the first.
    pub system_keys_swallowed: u64,
    /// Auto-target calls from the tail of combat-mode changes.
    pub auto_targets: u64,
    /// `0x01B2` and `0x01B4` deliveries that reached
    /// defender-notification auto-targeting — i.e. how many times
    /// `CombatState::last_attacked_time` was stamped.
    ///
    /// A denominator, and the one that matters for this arm: the stamp is unconditional and the
    /// `auto_target` call below it is not, so "no notification arrived" and "a notification arrived and
    /// the gate refused" must be different readings.
    pub defender_notifications: u64,
    /// Of those, how many ran `auto_target`.
    ///
    /// Counted apart from [`Self::auto_targets`], which is the combat-mode change call site:
    /// the two gates differ (no selection here against `get_attack_target` +
    /// `object_is_attackable` there), so one counter for both would hide a dead arm behind a live
    /// one.
    pub defender_auto_targets: u64,
    /// The examination action (`0x1000002B`) taking its **close-first** leg —
    /// hiding `<EXAM>` `0x100005F7`, instead of
    /// examining the selected object.
    pub examine_panel_closes: u64,

    // ---- the combat selection-change handler ------------------------------------------------
    /// Combat selection-change handler calls — one per absorbed selection-change notice.
    pub selection_change_notices: u64,
    /// Of those, how many reached `auto_target` through the handler's tail call.
    ///
    /// A third counter beside [`Self::auto_targets`] and [`Self::defender_auto_targets`], for the
    /// same reason those two are separate: this is the **only** one of `auto_target`'s four call
    /// sites that can reach the `>= 15.0` fallback leg, because it is the only one that does not
    /// stamp `last_attacked_time` on the way past.
    pub selection_change_auto_targets: u64,
    /// Of those, how many times the notice was consumed by the clear-once `target_willingly_lost`
    /// instead of auto-targeting. Counted apart so that "the flag refused it" and
    /// "some other gate refused it" are different readings.
    pub selection_changes_willingly_lost: u64,

    // ---- one counter per arm, because six in aggregate would pass with five dead ---
    /// `SelectLeft`/`SelectRight` arming `leave_target_mode` — the press edge with a
    /// target mode up.
    pub leave_target_mode_armed: u64,
    /// The per-frame UI update acting on it: the target mode actually dropped.
    pub target_modes_left: u64,
    /// `EscapeKey` reaching its finish-jump leg (jump power above `0.0`).
    pub escape_finish_jumps: u64,
    /// `EscapeKey` setting the target mode to `TargetMode::None`.
    pub escape_target_mode_clears: u64,
    /// `EscapeKey` reaching the visibility toggle for `0x1000001B` — nothing
    /// selected, so Escape opens the gameplay options panel.
    pub escape_options_toggles: u64,
    /// `EscapeKey` setting `target_willingly_lost` and clearing the selection.
    pub escape_deselects: u64,
    /// `EscapeKey` reaching `stop_completely` — the "you were doing something" leg.
    pub escape_stops: u64,
    /// `EscapeKey` printing *"Action interrupted"*, which happens on that leg only
    /// when the standing-still check said **false**.
    pub escape_interrupts: u64,
    /// `EscapeKey` reaching `abort_automatic_attack`.
    pub escape_attack_aborts: u64,
    /// Screenshot actions asking the rendering device to save an image.
    pub screenshots_requested: u64,
    /// `ToggleHelp` reaching the help-page request `(0, 0x10000001)`.
    pub help_opens: u64,
    /// Plugin-manager toggle actions taking the platform's open operation.
    pub plugin_manager_opens: u64,
    /// The same arm taking the close operation — the other half of the toggle, counted
    /// separately so that a single press cannot be mistaken for a working toggle.
    pub plugin_manager_closes: u64,
    /// `ToggleRadarPanel` flipping the radar-visible flag and sending
    /// the radar-visibility update notice.
    pub radar_visibility_notices: u64,
}

/// Counters. Everything here is tolerant of a missing field by design, so every tolerance has a
/// number and the tests assert on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HudStats {
    /// `0x0013` applied: the player's qualities and module state are live.
    pub player_desc_applied: u64,
    /// Rows read from `Option_PlacementArray`.
    pub placements_decoded: u64,
    /// Windows whose visibility the placement blob actually set.
    pub placements_applied: u64,
    /// `Qualities_*Attribute2nd*` events applied to the player's vitals.
    pub vital_updates: u64,
    /// A `Qualities_*` event about the player that named a vital this build cannot store. Must stay
    /// zero for the four `Attribute2nd` opcodes.
    pub vital_updates_unstorable: u64,
    /// Chat lines that reached at least one window.
    pub chat_lines: u64,
    /// Chat lines every window filtered out.
    pub chat_lines_dropped: u64,
    /// Lines taken off [`dereth_client_model::scroll::Scroll`] — every client-generated
    /// message, as opposed to the ones that came off the wire.
    pub scroll_lines: u64,
    /// …and how many of those the message-log panel accepted, which is the ones with chat type `0x1A`.
    /// Two counters, not one, because "the client generated nothing" and "it generated something
    /// the spew box refused" are the two readings this seam has to be able to tell apart.
    pub spew_lines: u64,
    /// A UI-queue opcode this module recognises but could not decode.
    pub undecodable: u64,
    /// `0x02BB` / `0x02BC` / `0x02BD` lines composed through one of
    /// [`dereth_client_model::chat::composition`]'s templates and pushed. Zero here with a non-zero
    /// [`Self::speech_lines_not_addressed_to_us`] would mean every tell was somebody else's; zero
    /// in both means no speech arrived at all. Two counters, not one.
    pub speech_lines_composed: u64,
    /// `0x02BD` tells the client deliberately draws nothing for: the
    /// target-is-the-player gate failed, so the handler returns
    /// early. Not a decode failure and not a drop — a real branch of the client.
    pub speech_lines_not_addressed_to_us: u64,
    /// `0x02BB` say lines refused,
    /// split by which half refused them. Two counters beside
    /// [`Self::speech_lines_composed`], so "nobody is talking", "you have them squelched" and
    /// "they are out of earshot" are three readings and not one.
    pub speech_lines_squelched: u64,
    /// See [`Self::speech_lines_squelched`] — this is the *distance* half, whose absence would let
    /// an emote carry across the world.
    pub speech_lines_out_of_earshot: u64,
    /// `0x02BB` with `senderID == 0`, which
    /// the `Communication_HearSpeech` handler discards **before** `CanHear`
    /// is reached — so the id-0 escape inside `CanHear` is not a way to put system text on the
    /// speech channel. Counted rather than folded into the two above, because it is a different
    /// branch of a different function.
    pub speech_lines_with_no_speaker: u64,
    /// `0x02BB` / `0x02BC` / `0x02BD` lines discarded because the local player had
    /// no physics body. This gate precedes every other test in all three handlers. Its own counter
    /// is not folded into the two above, because
    /// "you have no body yet" is a different fact from "they are out of earshot" and it is the
    /// only one of the three that can be true of every line in a batch at once.
    pub speech_lines_with_no_player_body: u64,
    /// `0x02BC` lines refused at
    /// the ranged-talk handler does **not** call `CanHear`, so this is a different
    /// predicate over a different radius (the one on the wire) with the opposite boundary
    /// polarity, and it gets its own counter rather than sharing
    /// [`Self::speech_lines_out_of_earshot`].
    pub ranged_lines_out_of_range: u64,
    /// `0x01E0` / `0x01E2` emotes composed and pushed at chat type `0xC`.
    pub emote_lines_composed: u64,
    /// Emotes `(senderID, 0xC)` refused
    /// — the call that is emote hearing's **first** act.
    /// Split from [`Self::emote_lines_squelched`] the same way the say counters are, so "nobody
    /// emoted", "they are muted" and "they are too far away" are three readings.
    pub emote_lines_out_of_earshot: u64,
    /// See [`Self::emote_lines_out_of_earshot`] — the squelch half of the same
    /// `CanHear` call.
    pub emote_lines_squelched: u64,
    /// `0x01E2` soul emotes discarded by
    /// the soul-emote handler's sender-id self-echo test
    /// above the tail call into the emote handler, with no counterpart on the
    /// `0x01E0` path. This is the one behaviour that distinguishes the two opcodes, so it is
    /// counted rather than inferred from a missing line.
    pub soul_emote_self_echoes_discarded: u64,
    /// Emote lines that were actually garbled: **`0x01E0` / `0x01E2` lines** composed by
    /// [`dereth_client_model::chat::composition::garbled_line`] from a random 1-to-10 phrase
    /// ([`dereth_client_model::chat::composition::OLTHOI_TEXT`] / [`dereth_client_model::chat::composition::HUMAN_TEXT`]) instead of by
    /// [`dereth_client_model::chat::composition::hear_emote_line`]. A garbled line is *also* counted in
    /// [`Self::emote_lines_composed`], because it is a line that was drawn.
    ///
    /// It is zero on every recorded session: the arm needs a
    /// sender name the shard has marked with `&`, or an Olthoi listener
    /// ([`crate::hud::Hud::is_olthoi`], answered from `HeritageGroup`), and
    /// the corpus has neither. A non-zero reading is a **feature working**.
    pub emote_lines_untranslated: u64,
    /// The same, for `0x02BB` speech lines that take the language-garbling arm.
    ///
    /// Three counters and not one, because the three arms share [`crate::hud::Hud::garbled_or_plain`] and
    /// three arms through one producer is exactly where two get wired and one is forgotten. A
    /// test that only asks "did anything garble" cannot tell which of the three ran.
    pub speech_lines_untranslated: u64,
    /// The same, for `0x02BD` direct-speech lines. This garbling decision sits
    /// **outside** the
    /// `targetID == player_id` gate. See [`Self::speech_lines_untranslated`].
    pub direct_speech_lines_untranslated: u64,
    /// Chat lines that carried a `%#H:%M:%S ` timestamp prefix, and the ones that
    /// deliberately did not (the timestamp option clear, or chat type `0x1A`). Three
    /// states between them and [`Self::speech_lines_composed`], so "the option is off" cannot read
    /// as "the stamper never ran".
    pub timestamps_stamped: u64,
    /// See [`Self::timestamps_stamped`].
    pub timestamps_suppressed: u64,
    /// Combat notification-event lines that reached the chat scroll.
    pub combat_lines: u64,
    /// …and the ones `is_squelched(0, "", combat text type)` dropped. Two counters, not
    /// one, so "nothing was hit" cannot read as "the combat log is muted".
    pub combat_lines_squelched: u64,
    /// `0x02BD` tells that wrote the last-teller id and
    /// the last-teller-name setter — the sender was a **player** and the tell was addressed to us.
    pub last_teller_writes: u64,
    /// Count `0x02BD` tells that reached the scroll and deliberately wrote **nothing**:
    /// an NPC's tell, or a tell we merely overheard. Three states rather than two, because
    /// "no tells arrived" and "126 tells arrived and every one was an NPC" are different facts;
    /// the corpus only carries the second, so the instrument needs a third state.
    pub last_teller_declined: u64,
    /// Count `0x0147` broadcasts that wrote the last-@monarch or
    /// last-@patron user name. **Zero across the whole corpus** — no recording carries a
    /// channel broadcast at all.
    pub at_channel_name_writes: u64,
    /// `0x0147` broadcasts composed and handed to the scroll — the
    /// scroll append of the `Communication_ChannelBroadcast` handler. Without it `/v hi` and
    /// every vassal's line would reach no window.
    pub channel_broadcast_lines_composed: u64,
    /// …composed and then refused by `is_squelched(0, "", type)` —
    /// a globally squelched *type*. Counted apart from "composed" because a shard that sends a
    /// hundred fellowship lines to a player who has squelched Fellowship is a different fact from
    /// a shard that sent none.
    pub channel_broadcast_lines_squelched: u64,
    /// `0xF7E0 Communication_TextboxString` lines refused by
    /// `is_squelched(0, "", text type)` in the
    /// `Communication_TextboxString` handler. Without that gate every globally squelched craft,
    /// salvage or magic notice would still be drawn.
    pub textbox_lines_squelched: u64,
    /// `0xF7E0 Communication_TextboxString` player-killer death lines dropped because the
    /// hear-PK-deaths option is off.
    pub textbox_pk_deaths_dropped: u64,
    /// Frames on which the vitals update actually wrote a meter.
    pub vitals_written: u64,
    /// Frames on which the toolbar's selected-object read-out changed.
    pub toolbar_written: u64,
    /// Frames on which the radar's coordinates or compass moved.
    pub radar_written: u64,
    /// Calls that actually changed `options2_` bit 24 — the radar
    /// padlock reaching the player module. Zero is the normal number: it only moves on a click.
    pub lock_ui_writes: u64,
    /// The selection-changed handler's not-a-stack arm asked for
    /// a health query on the selected id.
    pub selection_health_queries: u64,
    /// …asked for an item-mana query on the selected id.
    pub selection_mana_queries: u64,
    /// …correctly asked for **neither**: a non-attackable, non-player, non-pet
    /// object the player does not own. A real outcome of the client's branch.
    pub selection_queries_declined: u64,
    /// …could not decide, because the view answered no
    /// [`dereth_client_contract::GameView::selection_query_facts`]. Must stay **zero** in this
    /// client: a non-zero is a missing producer wearing the same face as "asked for neither".
    pub selection_queries_unanswerable: u64,
    /// Edge blocks that raised a health query / item-mana query for id 0
    /// — the clear that tells the shard to stop sending updates about the old selection.
    pub selection_query_clears: u64,
    /// `METER_LEVEL` writes into the toolbar's two selected-object meters, from
    /// the `0x01C0` / `0x0264` replies. This is what "a target's health bar updates" is, counted.
    pub selection_meters_written: u64,
    /// Character Options page visibility edges drained when the page becomes
    /// visible.
    pub character_option_edges: u64,
    /// Rows those edges actually moved. Both, because an edge that re-read 49
    /// identical values and an edge that never ran are otherwise the same number.
    pub character_option_rows_reread: u64,
    /// Character Options rows redrawn by the per-frame
    /// equivalent of the client's option-changed notice — boxes whose bit moved under them
    /// (a fellowship exclusion, or a `0x0013`) rather than by their own click.
    pub character_option_rows_noticed: u64,
    /// Chat Options page events drained by `Hud::drive` — an Apply / Cancel /
    /// *Restore Defaults* press or a visibility edge.
    pub chat_option_edges: u64,
    /// Controls those events wrote or re-read. Both counters, for the reason
    /// [`Self::character_option_rows_reread`] gives: a Cancel that reverted nothing and a Cancel
    /// that never ran are otherwise the same number.
    pub chat_option_controls_reread: u64,
    /// Elements the indicator strip and the toolbar's stance icon wrote. It must
    /// be non-zero after the first frame: the link lamp has no picture until something puts it in a
    /// media state, and the four combat-mode buttons are all visible until something hides three.
    pub indicators_written: u64,
    /// Frames on which the inventory panel refilled a slot.
    pub inventory_written: u64,
    /// Frames on which the quickbar refilled a shortcut slot or rewrote a numeral.
    /// One on the frame `0x0013`'s player module lands, and again on every combat-mode change.
    pub shortcuts_written: u64,
    /// `ContentProfile` rows carried by `0x0013` and applied to visible object contents.
    pub content_profiles: u64,
    /// `InventoryPlacement` rows carried by `0x0013` and applied to the object inventory.
    pub inventory_placements: u64,
    /// Frames on which a skills or spellbook rebuild actually rewrote its list.
    pub panels_written: u64,
    /// Accepted ordered power-bar subscriber calls, excluding unrelated panel updates.
    pub power_bar_writes: u64,
    /// The row and slot counts the two panels ended up with, so "it drew nothing" is a number.
    pub skill_rows: u64,
    pub spell_slots: u64,
    /// Element messages offered to the stat-management / spellbook panels, and how many
    /// they took. **Both**, because "0 consumed" and "0 offered" are different failures — the first
    /// is a panel that is not listening and the second is a click that never arrived.
    pub panel_messages: u64,
    pub panel_messages_consumed: u64,
    /// Items dropped on the Trade panel's self list that reached
    /// `TradePanel::drop_item` -- the production producer of `UiRequest::TradeAddItem`.
    pub trade_drops: u64,
    /// Pointer drops delivered to the house-payment panel, including a
    /// partial drop that starts a split and a whole drop that adds a payment row.
    pub house_drops: u64,
    /// Items released over `0x100000CE` that reached
    /// `crate::hud::RemainingPanels::vendor`'s `drop_item`. A drop the splitter refused as a
    /// partial stack is not counted, which is the difference between "the gesture arrived" and
    /// "the gesture was taken".
    pub vendor_sell_drops: u64,

    // ---- quality updates ------------------------------------------------------------------------
    /// Quality-update events applied to the local player description — **all twenty-six forms**, not
    /// just the four vitals ones. [`Self::vital_updates`]
    /// counts the `Attribute2nd` subset of these and is kept because tests assert on it.
    pub quality_updates: u64,
    /// Events rejected as stale. A live server's own stream is in order,
    /// so a non-zero here means the sequence byte or the wrap rule is wrong — the same reasoning
    /// `dereth_client_model`'s corpus replay uses for its own gate.
    pub quality_updates_stale: u64,
    /// Events that passed the gate and had nowhere to land: one of the four partial forms naming
    /// a skill or attribute the local player description does not carry.
    pub quality_updates_unstorable: u64,
    /// Skill records the server changed — the answer to a `Train_TrainSkill`.
    pub skill_updates: u64,

    // ---- quality removes ------------------------------------------------------------------------
    /// `Qualities_*Remove*Event` messages that deleted a key off the player's qualities — the
    /// eight private forms, and the eight public ones that name the player.
    ///
    /// A **separate** counter from [`Self::quality_updates`] on purpose: stat removal is a
    /// different path from the stat update — no value, no player-description mirror, and
    /// the remove handler rather than
    /// the change handler — so folding the two would hide which one the shard sent.
    pub quality_removes: u64,
    /// Removes rejected as stale. The gate and its key
    /// are the update path's: a remove and an update of one property share one 8-bit counter.
    pub quality_removes_stale: u64,
    /// Removes that passed the gate and found nothing to delete — a NULL table, or a property the
    /// local player description was not carrying. The native delete path ignores the return value, so retail
    /// cannot tell this from a real delete, and neither can any caller
    /// here; the counter exists so a test can, and so that a shard removing properties this client
    /// never received is visible instead of silent.
    pub quality_removes_absent: u64,

    // ---- enchantments ---------------------------------------------------------------------------
    /// Enchantment-update messages applied to the **local player's** registry by this module.
    /// The messages are handled in `interaction.rs`, which writes the player's one registry (the
    /// one the buff, debuff and vitae lamps read), so nothing here increments this.
    pub player_enchantments_applied: u64,

    // ---- identify and book panels ---------------------------------------------------------------
    /// Frames on which the identify panel was rewritten.
    /// Zero for a session in which nothing was examined, which is most of them.
    pub examinations_filled: u64,
    /// Frames on which the Book panel wrote -- an `OpenBook`, or a `0x00B8` filling the
    /// page on screen. Zero for a session in which nothing was read.
    pub book_frames: u64,
    /// Attribute records the server changed — the answer to a `Train_TrainAttribute`.
    pub attribute_updates: u64,
    /// Times the stat-management panel's `0x10000004` arm ran: the latch cleared and the footer re-run
    /// because the changed quality came back.
    pub raises_answered: u64,

    // ---- splitter -------------------------------------------------------------------------------
    /// Times the selection-changed handler's splitter block ran — it is
    /// `dereth_ui_screens::screens::gameplay::ToolbarSelection::split_gate_ran`. It is a
    /// **settling** count, not a per-frame one: the
    /// client's own re-entry test (in its item-attributes-changed handler) stops it once
    /// the maximum split size agrees with the selection, so a scene where nothing is selected and nothing
    /// changes must leave this still. A number that climbs with the frame counter means the test
    /// never settles and the box is being rewritten under the player.
    pub split_gate_runs: u64,

    // ---- selection rings and cooldowns ----------------------------------------------------------
    /// Selection rings written by the item list's `set_selected_item` across every live item
    /// list. It moves only on a selection edge, so a number that climbs with the frame counter
    /// means the edge is not settling.
    pub item_slot_rings: u64,
    /// Item slots that re-ran `update_cooldown_display` off the once-a-second heartbeat.
    /// It climbs with wall-clock time and not with frames — roughly (live slots) per second — and
    /// **zero** means the heartbeat is not running at all, which is the state a frozen wedge is
    /// indistinguishable from.
    pub item_slot_heartbeats: u64,

    // ---- spell components -----------------------------------------------------------------------
    /// `0x0013` blobs pushed through `PlayerSystem::apply_player_module`; without it the player
    /// system holds constructor defaults for every session.
    pub player_module_applied: u64,
    /// Objects offered to the component tracker.
    /// The denominator: a tracker with nothing in it after a
    /// large number here means the player carries no components; after a **zero** it means the
    /// sweep never ran.
    pub components_offered: u64,
    /// Of those, how many returned something other than "no change".
    pub components_changed: u64,
    /// Rows the spell-component panel drew on its last rebuild.
    pub component_rows: u64,

    // ---- chat talk focuses and auto-target -----------------------------------------------------
    /// How many of the fourteen talk focuses `enable_chat_talk_focuses` last left enabled.
    /// Focuses 1 and 2 are always on, so **2** is the floor and means every Turbine channel is off
    /// — which is what a client with no Turbine-chat connection shows.
    pub talk_focuses_enabled: u64,
    /// Auto-target sweeps that changed the chat target: adopted
    /// or cleared. It moves at most once a second and **zero over a long session with a selection
    /// made** means the sweep is not reaching the screen.
    pub auto_target_changes: u64,
    /// Objects within the radar radius of the player on the last frame — the input the sweep
    /// consumes. Zero here with a non-empty radar means the range filter, not the sweep, is what
    /// is empty.
    pub auto_target_in_range: u64,

    // ---- spellbook ------------------------------------------------------------------------------
    /// Inbound `0x01A8 Magic_RemoveSpell` messages that reached
    /// [`crate::hud::Hud::handle_magic_remove_spell`] — the shard's answer to a confirmed spellbook DELETE.
    /// Zero over a session in which a spell was deleted means the reply is not being routed.
    pub spells_removed: u64,
    /// Inbound `0x02C1 Magic_UpdateSpell` messages that reached
    /// [`crate::hud::Hud::handle_magic_update_spell`] — the shard's answer to learning a spell from a scroll
    /// or a levelling reward. Zero over a session in which a spell was learned means the book will
    /// not show it until the next login.
    pub spells_added: u64,

    // ---- squelch database -----------------------------------------------------------------------
    /// Inbound `0x01F4 Communication_SetSquelchDB` databases applied to `ChatState::squelch`. The
    /// shard sends one unprompted at login and one after every change, so a zero here in a session
    /// where anything is squelched means the receiver is not being reached.
    pub squelch_db_applied: u64,
    /// Rows the **last** applied database carried -- squelched accounts plus squelched characters.
    /// A replacement is not a merge, so this is the size of the table now and not a running total;
    /// it goes back to zero when the player removes their last squelch, which is exactly the case
    /// [`Self::squelch_db_applied`] alone cannot distinguish from a receiver that never ran.
    pub squelch_rows_applied: u64,

    // ---- unhandled UI events --------------------------------------------------------------------
    /// UI-queue messages that reached the bottom of [`crate::hud::Hud::ui_event`] and were dropped.
    ///
    /// The sibling of `ObjectStream::stats.unhandled`: without it the count of messages the HUD
    /// threw away would not be recorded anywhere in the process. Which opcodes they were is in
    /// [`crate::dropped`]; this is the denominator beside it, kept here so a suite that asserts on
    /// `HudStats` alone can still see the seam move.
    pub ui_events_unhandled: u64,

    /// `0x02BE Fellowship_FullUpdate` messages applied. The recorded sessions carry 23 of these.
    pub fellowship_updates: u64,
    /// `0x0021 Social_FriendsUpdate` applied. The recorded sessions carry it **11 times** across
    /// the three `fellowship-*` captures. This is the arrival count of the message the Friends tab
    /// is drawn from.
    pub friends_updates: u64,
    /// Of these, the following counter records update types for which
    /// the client's friends-list update handler
    /// has no case for. Its own counter rather than `undecodable`: the body parsed, the client
    /// simply does nothing with it, and a non-zero here is a fact about the shard.
    pub friends_updates_unknown_type: u64,
    /// The size of the friends list after the most recent `0x0021` -- the observable a test can
    /// read without a UI.
    pub friends: u64,
    /// `0x0314 Social_SendClientContractTrackerTable` applied — the whole tracker
    /// table replaced. Earlier capture sweeps observed no arrivals, so a non-zero value here is a
    /// fact about the current session rather than a regression guard.
    pub contract_tables: u64,
    /// `0x0315 Social_SendClientContractTracker` applied, by arm — added, updated, removed.
    pub contract_trackers_added: u64,
    pub contract_trackers_updated: u64,
    pub contract_trackers_removed: u64,
    /// Of these, the following counter records requests carrying `setAsDisplayContract`. That flag
    /// pins one contract to the HUD and **this build has no consumer for that**, so it is counted
    /// rather than stored in a field nothing reads.
    pub contract_display_requests: u64,
    /// The size of the tracker table after the most recent `0x0314`/`0x0315`.
    pub contracts: u64,
    /// `0x02B4 Inventory_SalvageOperationsResultData` messages that reached the HUD.
    /// Earlier capture sweeps found no arrivals, against a control set that did contain `0x02BE`.
    /// A non-zero value here is therefore a fact about the current session.
    pub salvage_results: u64,
    /// Of these, the following counter records reports silenced by a squelch on text type `0x19`.
    pub salvage_results_squelched: u64,
    /// Lines the salvage report put in the scroll — between one and two per `0x02B4`.
    pub salvage_lines: u64,
    /// Salvage-panel notices delivered to the panel, and drops accepted by it.
    pub salvage_notices_delivered: u64,
    /// Chess events this build decoded -- the six inbound opcodes, summed.
    ///
    /// Three-state on purpose, and the three counters below it are why: zero here is "no chess
    /// traffic", non-zero with `minigame_guarded` equal to it is "every message was for a board
    /// this window had not joined", and non-zero with `minigame_lines` at zero would be a window
    /// that received and said nothing.
    pub minigame_events: u64,
    /// Chess events the Mini Game panel's own current-game / state guard refused.
    pub minigame_guarded: u64,
    /// Lines `set_info_text` put in the scroll.
    pub minigame_lines: u64,
    /// Notices that reached the Mini Game panel,
    /// i.e. chess boards actually used.
    ///
    /// The notice is produced by `use_object`'s arm 7; this is the counter that means the window
    /// opened.
    pub minigame_boards_used: u64,
    pub salvage_drops: u64,
    /// Material prefixes actually composed, i.e. rows
    /// [`crate::hud::Hud::refresh_display_names`] wrote or rewrote.
    ///
    /// Three-state on purpose. Zero with a salvage bag on screen is a defect;
    /// zero with nothing material-bearing in the world is correct; and a number that climbs
    /// every frame means the guard in [`crate::hud::DisplayName::matches`] has stopped holding, which would be
    /// a per-frame allocation nobody would otherwise see.
    pub display_names_composed: u64,
    /// Trackers dropped by [`dereth_client_contract::view::GameView::contracts`] because the contract table did not resolve their
    /// id. A denominator: an empty Contracts tab can then say whether the shard sent nothing or
    /// whether the dat did not carry what it sent. The reference client assumes this lookup succeeds.
    pub contracts_unresolved: u64,
    /// …of which created the fellowship, rather than replacing one already held. Retail repaints
    /// every member's blip on a creation and only the two leaders' on a leader change, so the two
    /// are different events and are counted separately.
    pub fellowships_created: u64,
    /// `0x02BE` replacements in which `_leader` moved.
    pub fellowship_leader_changes: u64,
    /// `0x02C0 Fellowship_UpdateFellow` applied. 39 in the three sessions — the single most
    /// frequent fellowship message.
    pub fellow_updates: u64,
    /// A `0x02C0` that arrived with **no fellowship held**, which retail reaches by dereferencing
    /// a null fellowship pointer. Non-zero means the shard sent an update before the full update, and
    /// is a fact about ordering rather than a decode failure — which is why it is not
    /// [`Self::undecodable`].
    pub fellow_updates_without_a_fellowship: u64,
    /// `0x02C0` naming somebody who was **not** already a member — retail's `is_fellow` taken
    /// before the update, and the condition on the fellow-added notice.
    pub fellows_added: u64,
    /// `0x02BF Fellowship_Disband` applied.
    pub fellowship_disbands: u64,
    /// `0x00A3 Fellowship_Quit` applied. 8 in the three sessions.
    pub fellowship_quits: u64,
    /// `0x00A4 Fellowship_Dismiss` applied. 3 in the three sessions.
    pub fellowship_dismissals: u64,
    /// Quits and dismissals whose subject was **the player** — the branch that deletes the whole
    /// fellowship and turns the Fellowship talk focus off, rather than removing one row.
    pub fellowship_departures_our_own: u64,
    /// Chat lines the Fellowship panel composes itself — the create / recruited /
    /// joined / left / dismissed / disbanded sentences — pushed by the five fellowship arms.
    /// Zero with a non-zero `fellowship_updates` means the receivers moved the world and told
    /// the player nothing.
    pub fellowship_lines_composed: u64,
    /// Members the fellowship holds **now**. A replacement is not a merge, so this is the size of
    /// the table and not a running total; it goes to zero on a disband. Two counters and not one,
    /// for the reason [`Self::squelch_rows_applied`] gives: "no fellowship" and "a receiver that
    /// never ran" are indistinguishable in the event counts alone.
    pub fellowship_members: u64,
    /// `0x01C9 Fellowship_FellowUpdateDone` consumed.
    ///
    /// A counter and nothing else, because this event has no state-changing handler body. It exists
    /// so that "the shard
    /// bracketed a burst of `0x02C0`" is *observable* rather than merely un-dropped: a receiver
    /// whose whole retail body is empty has no other trace.
    pub fellow_update_done: u64,
    /// `0x0226 House_HouseStatus` received — the answer to the `0x021E House_QueryHouse` retail
    /// sends during player initialization.
    ///
    /// This is the number `dereth_ui_screens::panels::house::HousePanel` watches: retail's
    /// house panel redraws on **every** notice, so it redraws whenever
    /// this moves. Three arrivals across the three `fellowship-*` recordings, and one in
    /// each of six of the seven original captures — it is on every login.
    pub house_status_notices: u64,
    /// The `u32` the last `0x0226` carried. **A measurement rather than state.**
    ///
    /// ACE writes a `WeenieError` here (`GameEventHouseStatus.cs`); every recorded arrival is
    /// **2** (`BadParam`, `HandleActionQueryHouse`'s *"no house owned"* default). Retail stores it
    /// nowhere at all: the house-transaction handler passes it to the failed-house-transaction
    /// notice and all three registered receivers ignore their
    /// parameter. It is here so a shard that ever sends a different code is visible in a counter
    /// instead of silently taking the same arm.
    pub house_status_last_notice: u64,
    /// `0x0225 House_HouseData` received and copied into the house-data state.
    ///
    /// Zero in all ten recordings, because the recorded character owns no house
    /// — ACE answers `0x021E House_QueryHouse` with `0x0226` in that case and never sends this
    /// one. It is the pane's second redraw edge; see
    /// `dereth_ui_screens::panels::house::HousePanel::update`.
    pub house_data_notices: u64,
    /// `0x021D House_HouseProfile` decoded and copied into the house-profile state, and the edge
    /// that raises the purchase / maintenance window.
    ///
    /// This is the answer to a **use on a slumlord** (`SlumLord.ActOnUse`); the server sends it on
    /// no other path.
    pub house_profile_notices: u64,
    /// `0x0227 House_UpdateRentTime` decoded.
    ///
    /// Counted, not watched: retail's rent-time handler redraws
    /// the house data directly rather than going through the panel update, so there is no
    /// notice for the pane to count and the redraw edge is `HousePanel::update`'s comparison
    /// against what it last drew. This pair of counters is how "arrived" and "arrived and changed
    /// something" stay separable.
    pub house_rent_time_updates: u64,
    /// …and how many of those found a house to apply themselves to. The two differ by exactly the
    /// arrivals for a houseless character, for whom the handler is a no-op with no redraw.
    pub house_rent_time_applied: u64,
    /// `0x0228 House_UpdateRentPayment` decoded.
    pub house_rent_payment_updates: u64,
    /// …and how many of those found a house; the other arm is a no-op.
    pub house_rent_payment_applied: u64,
    /// `0x0248 House_UpdateRestrictions` decoded.
    pub house_restriction_updates: u64,
    /// …and how many survived all four of
    /// the update-restrictions handler's gates. The two differ by the zero
    /// ids, the messages about the local player, the unknown objects and the stale timestamps —
    /// which is four distinct reasons for one number, and the point of having it is that
    /// "arrived" and "stored" stop being the same fact.
    pub house_restrictions_applied: u64,
    /// `0x0257 House_UpdateHAR` decoded and printed.
    ///
    /// The answer to `0x024D House_RequestFullGuestList`, which `@house guest list|show` and
    /// `@house storage list|show` send — and the one inbound house opcode of this pair
    /// observed in the capture sweep, where each arrival followed a `0x024D` request.
    pub house_har_updates: u64,
    /// Guest rows written. Zero alongside a non-zero
    /// [`Self::house_har_updates`] is the *"None"* arm, which is a real and common answer rather
    /// than a failure.
    pub house_har_guests: u64,
    /// `0x0271 House_AvailableHouses` decoded and printed. The answer to
    /// `0x0270`, which `@hslist <type>` and `@house available <type>` send.
    pub house_available_houses: u64,
    /// Coordinate lines the house-list coordinate display wrote. Always **0** for an apartment
    /// listing, whatever the count — retail skips the whole block on `house_type == 4`.
    pub house_available_coord_lines: u64,
    /// `0x019E Combat_HandlePlayerDeathEvent` decoded.
    pub player_deaths: u64,
    /// …and how many of those produced a scroll line. The two differ by exactly the deaths the
    /// player was part of, which retail deliberately says nothing about here: a zero
    /// [`Self::player_deaths_announced`] beside a non-zero [`Self::player_deaths`] means every
    /// death that arrived was your own or your kill, and is **not** a broken receiver. Two
    /// counters, not one, because the `!=` pair is the whole of the handler.
    pub player_deaths_announced: u64,
    /// `0x01AC`/`0x01AD` decoded; both opcodes share identical arguments and behavior.
    pub victim_notifications: u64,
    /// …and how many of those produced a scroll line. They differ by exactly the empty
    /// strings: the handler returns 1 either way and skips the scroll insertion only when
    /// the string's length (terminator included) is 1, so a zero announce beside a non-zero
    /// decode is the shard sending blanks
    /// and not a broken receiver.
    pub victim_notifications_announced: u64,
    /// `0x027A Allegiance_AllegianceLoginNotificationEvent` decoded.
    pub allegiance_logins: u64,
    /// …and how many of those produced a scroll line. They differ by exactly the members absent
    /// from the cached allegiance hierarchy, which is the handler's one guard.
    pub allegiance_logins_announced: u64,
    /// The allegiance data lookup's refusals themselves — a member the cached hierarchy does not
    /// hold. Three states and not two, for the reason [`Self::fellowship_members`] gives: a client
    /// that has never sent `0x001B` has an empty tree and shows **no** logon lines, and that is
    /// retail's behaviour rather than a receiver that did not run.
    pub allegiance_logins_without_a_member: u64,
}
