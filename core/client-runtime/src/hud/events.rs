//! Session events, qualities, and research receipts.

use super::*;

impl Hud {
    /// The host's transit for the Salvage panel's notices -- see
    /// [`dereth_client_contract::panels::salvage::SalvageNotice`].
    ///
    /// Two methods rather than a `pub` field, because the two calls mean different things:
    /// `App` clears unconditionally at the head of the frame (an undelivered batch must not
    /// survive a screen change and open a window for a tool used two screens ago) and queues only
    /// when the panel generation that will receive them already existed.
    pub fn clear_salvage_notices(&mut self) {
        self.pending_salvage.clear();
    }

    /// See [`Self::clear_salvage_notices`].
    pub fn queue_salvage_notices(
        &mut self,
        notices: Vec<dereth_client_contract::panels::salvage::SalvageNotice>,
    ) {
        self.pending_salvage.extend(notices);
    }

    /// One frame's worth of session events, applied.
    ///
    /// Returns the chat lines that arrived, in order — for the caller to log. They are also queued
    /// for `Self::drive`, because routing is `ChatInterface`'s and the screen owns the five
    /// interfaces.
    pub fn apply_events(
        &mut self,
        events: &[SessionEvent],
        world: &mut dereth_client_model::World,
    ) -> Vec<ChatMessage> {
        self.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut NoPanels,
            None,
            &mut |_, _| {},
        )
    }

    /// Apply HUD events and synchronously deliver the combat system's player-quality callback.
    /// `App::apply_hud_events` supplies the production handler; HUD-only consumers can continue
    /// using `apply_events`. Delivery is per accepted update, not a poll of the batch's final
    /// quality value, so stale updates and intervening mode changes keep their retail ordering.
    pub fn apply_events_with_combat_mode_handler(
        &mut self,
        events: &[SessionEvent],
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        mut ui_requests: Option<&mut dereth_client_contract::requests::Outbox>,
        on_combat_mode: &mut dyn FnMut(
            &mut dereth_client_model::World,
            dereth_client_model::combat::CombatMode,
        ),
    ) -> Vec<ChatMessage> {
        let mut chat = Vec::new();
        // **The two inputs scroll insertion reads that this crate owns.**
        // The timestamp option comes off the same option word every other option does,
        // and the clock is read inside
        // scroll insertion at the instant the line lands, which no pure function can do. Pushed
        // here, at the head of the batch, for the same reason `set_lock_ui` and `reply_targets` are
        // pushed once a frame: the two owners cannot see each other.
        world.scroll.display_time_stamps = world
            .player_system
            .options
            .get(dereth_client_model::player::options::option::DISPLAY_TIME_STAMPS);
        world.scroll.now_unix = wall_clock_unix();
        world.scroll.utc_offset_secs = utc_offset_secs(world.scroll.now_unix);
        self.drain_scroll(world, panels, &mut chat);
        // The spell component table, before any object of the batch: a component the world meets
        // without it is filed under no category.
        world.install_component_catalogue(&self.component_catalogue);
        for e in events {
            match e {
                // Character startup is gated by the event's enable flag. Handle it in stream order
                // so a same-batch PlayerDescription
                // sees startup before its talk-focus-enable notice.
                SessionEvent::CharacterSet(set) => {
                    // Character startup
                    // copies the unpacked account name into the player system
                    // **before** it looks at the Turbine-chat flag, and that field is
                    // the one `get_appropriate_spell_formula` hashes to pick a spell's
                    // tapers. Without this the model's account stays empty and every eight-slot
                    // formula lists the empty-name tapers instead of the player's.
                    world.player_system.account.clone_from(&set.account);
                    if set.use_turbine_chat != 0 {
                        world.chat.startup_turbine_chat();
                    }
                }
                SessionEvent::TurbineChat(raw) => {
                    // Preserve ordinary notices already queued by an earlier event separately.
                    self.drain_scroll(world, panels, &mut chat);
                    if !world.recv_turbine_chat(raw) {
                        self.stats.undecodable += 1;
                    }
                    let lines = self.collect_scroll(world, panels);
                    if let Some(generation) = self.turbine_chat_generation {
                        self.pending_chat
                            .extend(lines.iter().cloned().map(|m| (Some(generation), m)));
                    }
                    chat.extend(lines);
                }
                // `0x0013 Login_PlayerDescription` replaces the current player description. Both
                // halves land here: the qualities the vitals read and the
                // player module every window's player-module refresh reads.
                SessionEvent::PlayerDescription(d) => {
                    // Placement calls queued before this authoritative replacement belong to
                    // the old local module. Retail performed them synchronously and then replaced
                    // that module. Drop only that delayed numeric subset, never unrelated actions.
                    if let Some(q) = ui_requests.as_deref_mut() {
                        let _ = q.take_placement_updates();
                    }
                    // **`0x0013` fills the one store.**
                    //
                    // The login handler copies qualities straight into the player description that
                    // every later `Qualities_*Update*` writes and every panel reads. There
                    // is no staging buffer and no second copy, so there is none here either:
                    // the private integer-quality handler is the public handler with the player id
                    // substituted, and both reach the stat update, which writes the weenie's
                    // qualities. One owner; every update message maintains it.
                    //
                    // The park inside `apply_player_desc_qualities` handles `0x0013` arriving
                    // before the player's weenie row: `long-solo-play` has it at idx 9 on queue 9 and
                    // `0xF746` at idx 22 on queue 10. Retail does not need the park because
                    // it drains queue 10 before queue 9 — and neither, in
                    // practice, does this build — but the two halves can land in different receives
                    // and ACE never resends a `CoinValue` it thinks unchanged
                    // (`Player_Commerce.cs:326`), so a dropped login purse would stay dropped.
                    //
                    // **Receipt time, not `0.0`.**
                    //
                    // Qualities decoding reaches enchantment decoding,
                    // and `_start_time` and `_last_time_degraded` arrive **relative to receipt**:
                    // the client stores `current_time + value` (see the
                    // note at the head of `dereth_client_model::enchant`). The panel then draws
                    // the remaining duration, `(_duration + _start_time) - current_time`.
                    //
                    // Rebasing the login blob against **zero** would make that difference
                    // `wire_start + duration - cur_time`, i.e. the whole elapsed session too
                    // negative, and `format_duration` clamps at zero: every timed buff the
                    // `0x0013` carried would read **0:00**, while still expiring at the right
                    // moment because the expiry is the server's `0x02C3 Magic_RemoveEnchantment`
                    // and not this number. `0x0013` is the
                    // only place a re-login learns its enchantments from.
                    //
                    // The same rule applies to the AC qualities
                    // (`Qualities::apply_ac_qualities`).
                    //
                    // `Hud::now` is the receipt time: `App::deliver_session_events` stamps it
                    // from the same `LocalTime(self.clock.cur_time)` it hands
                    // `ObjectStream::apply_event` and `Interaction::last_use_time`, so the
                    // `0x02C2` path (`interaction.rs`, `inter.last_use_time`) and this one
                    // rebase against one clock. `Hud::drive` re-stamps it every frame from
                    // `UiSystem::now`, which is the same value.
                    //
                    // `UiSystem::now` carries a note about precisely this mistake one
                    // layer down: "Passing a zero here instead ... makes every one of them expire
                    // on its first tick".
                    let dids = world.apply_player_desc_qualities(&d.qualities, self.now);
                    self.player_desc_received = true;
                    if crate::trace::raise() {
                        // The `Int64` table exactly as the live `0x0013`
                        // delivered it, and where it landed (the row, or the park).
                        let q = world
                            .player_qualities()
                            .or_else(|| world.login_player_desc());
                        tracing::debug!(
                            target: "dereth::trace::raise",
                            "raise-trace 0x0013 player descriptor: player_row={} parked={} \
                             int64s={:?} strength(attr 1)={:?} level(int 25)={:?}",
                            world.player_qualities().is_some(),
                            world.login_player_desc().is_some(),
                            q.and_then(|q| q.int64s.as_ref()),
                            q.and_then(|q| q.attribute(1)),
                            q.map(|q| q.inq_int(25)),
                        );
                    }
                    tracing::debug!(
                        "0x0013 player DataIDs: {dids}, CombatTable {:?}",
                        world.combat_table_did()
                    );
                    self.placements = decode_placements(&d.player_module);
                    self.stats.placements_decoded =
                        u64::try_from(self.placements.rows.len()).unwrap_or(0);
                    // The blob is what decides which windows the player sees, so it is logged in
                    // full: a HUD that comes up wrong is almost always this table being wrong.
                    for (id, row) in &self.placements.rows {
                        tracing::debug!("HUD placement window {id}: {row:?}");
                    }
                    self.player_module = Some(d.player_module.clone());
                    self.stats.player_desc_applied += 1;
                    // The skills and spellbook views both rebuild their whole list from
                    // the player description that just landed.
                    self.rebuild_panel_tables(world);
                    tracing::debug!(
                        "0x0013 panels: {} skill rows, {} spells in the book",
                        self.skills.len(),
                        self.spells.len()
                    );
                    // Login-description handling ends
                    // by loading the player's content profiles and inventory placements. `0x0013`
                    // is the **only** message that tells the client
                    // what is in the player's pack at login — the individual `0xF745` creates that
                    // follow carry each item's container id but never the list — so without
                    // these two calls the backpack would be empty however full the pack was.
                    //
                    // The id is the world's player id, falling back to the one recorded here,
                    // which is `0xF746`'s and arrives before `0x0013`.
                    let player = world.player.or(self.player);
                    if let Some(p) = player {
                        world.view_object_contents(
                            p,
                            &d.content_profiles,
                            &mut dereth_client_model::NullSink,
                        );
                        world.update_object_inventory(
                            p,
                            d.inventory_placements
                                .iter()
                                .map(|q| dereth_client_model::objects::InventoryPlacement {
                                    iid: q.iid,
                                    loc: q.location,
                                    priority: q.priority,
                                })
                                .collect(),
                        );
                        self.stats.content_profiles +=
                            u64::try_from(d.content_profiles.len()).unwrap_or(0);
                        self.stats.inventory_placements +=
                            u64::try_from(d.inventory_placements.len()).unwrap_or(0);
                        tracing::debug!(
                            "0x0013 inventory for {p:?}: {} contents, {} placements",
                            d.content_profiles.len(),
                            d.inventory_placements.len()
                        );
                    }
                    // ---- the things `0x0013` drives ----
                    //
                    // **`PlayerSystem::apply_player_module`** is the
                    // player-module initialization seam, and without it the player system
                    // stays at its constructor defaults for the whole
                    // session: `desired_comps_` empty (so the component drain has
                    // nothing to walk), `spell_filters_` at the default, and the character options
                    // — which four `dereth-client-model` decisions read — never the player's own. This crate
                    // decodes the same blob into `Hud::player_module` for the window placements,
                    // which is why a missing call here would be invisible: the blob would be in the
                    // client, in a second copy, and the model half of it never filled.
                    let effects = world.player_system.apply_player_module(&d.player_module);
                    self.stats.player_module_applied += 1;
                    tracing::debug!(
                        "0x0013 player module: {} desired comps, spell filters {:#x}, \
                         side effects {effects:?}",
                        world.player_system.desired_comps.len(),
                        world.player_system.spell_filters,
                    );

                    // Player-description receipt also enables the chat talk focuses.
                    // `is_olthoi` is
                    // the player-system creature-type query this function calls.
                    //
                    // The is-Olthoi test is `PropertyInt 0xBC HeritageGroup`
                    // against 12 and 13, read from the canonical local-player object row that the
                    // arm above has just filled from this message. The value is therefore this
                    // character's and not the previous one's. The
                    // corpus answers `false` for every recorded character.
                    let olthoi = self.is_olthoi(world);
                    let focuses = world.enable_chat_talk_focuses(olthoi);
                    self.stats.talk_focuses_enabled =
                        u64::try_from(focuses.iter().filter(|e| **e).count()).unwrap_or(0);

                    // Player initialization's component drain and the
                    // catalogue it needs.
                    world.install_component_catalogue(&self.component_catalogue);
                    // `school_of_magic_to_wcid`'s mapper, the other half of
                    // `get_appropriate_spell_formula`'s foci test. Without it
                    // `magic_pack_is_owned` is asked about WCID 0 and always refuses.
                    if world.magic.school_pack_wcid.is_empty() && !self.school_pack_wcid.is_empty()
                    {
                        world.magic.school_pack_wcid =
                            self.school_pack_wcid.iter().copied().collect();
                    }
                    // The same lazy spell-table load
                    // `(6, 2, 0x10000005)` that casting does on its
                    // first call, hoisted to the point the table is already in hand. Without it
                    // `cast_spell` takes its "no table" arm and every cast is a silent no-op.
                    if world.magic.spell_table.is_none() {
                        if let Some(t) = self.spell_table.as_ref() {
                            world.magic.spell_table = Some(std::sync::Arc::new(t.clone()));
                        }
                    }
                    if world.magic.spell_beneficial.is_empty() {
                        if let Some(t) = self.spell_table.as_ref() {
                            // The spell-totals count's one table read: `_bitfield & 4`,
                            // the spell's beneficial flag.
                            world.magic.spell_beneficial = t
                                .spells
                                .iter()
                                .map(|(id, b)| (*id, b.bitfield & 4 != 0))
                                .collect();
                        }
                    }
                    let (offered, changed) = world.initialize_spell_components();
                    self.stats.components_offered += u64::try_from(offered).unwrap_or(0);
                    self.stats.components_changed += u64::try_from(changed).unwrap_or(0);
                    // Recount after unpacking — the enchantment registry that just
                    // landed has never been counted.
                    let (helpful, harmful) = world.recount_spell_totals();
                    tracing::debug!(
                        "0x0013 magic: {offered} objects offered to the component \
                         tracker, {changed} changed, {} components owned; enchantments \
                         {helpful} helpful / {harmful} harmful",
                        world.magic.components.tracked_objects()
                    );

                    // A new description is a new screen state: re-apply on the next frame.
                    self.applied_to = None;
                }
                // The player id arrives before the player object.
                SessionEvent::PlayerCreated(id) => self.player = Some(*id),
                SessionEvent::UiEvent { .. } => {
                    let (opcode, body) = e.ui_body().expect("UI event body");
                    let before = chat.len();
                    self.ui_event(opcode, body, world, panels, &mut chat, on_combat_mode);
                    // These handlers compose the same complete body that
                    // retail hands to the censor filter. Filter only
                    // this direct slice before either registered receiver sees it. Lines from
                    // `world.scroll` were filtered by that producer and never enter this slice,
                    // so no line passes the censor filter twice.
                    for line in &mut chat[before..] {
                        line.body = world.scroll.filter_text(&line.body);
                    }
                    // The second of the two places a `ChatMessage` is born in this
                    // build; the first is `drain_scroll`. In the client both are the *same* place —
                    // every one of these handlers ends in the scroll insertion, which is
                    // what stamps — so the stamp is applied here rather than at each of the fifteen
                    // `chat.push` sites above. Routing these arms through `dereth_client_model::scroll` so
                    // there is one seam instead of two would move the existing queueing.
                    self.stamp_timestamps(world, &mut chat[before..]);
                    // These lines take the native scroll-insertion route but are still composed
                    // directly in `Hud`. Append only this new finalized
                    // slice: `world.scroll` lines before `before` wrote at their producer, so this
                    // cannot double-log either route.
                    for line in &chat[before..] {
                        let _ = world.scroll.copy_final_to_log(
                            u32::from(line.ty),
                            line.prefix.as_deref(),
                            &line.body,
                        );
                    }
                    // The other half of the final-string display notice's
                    // fan-out: the spew receiver takes every notice whose chat type is
                    // `0x1A`, and it is **not an alternative**
                    // to the chat window -- both receivers are offered every line. A network line
                    // (a `0x02EB Communication_TransientString`, e.g. ACE's "The <chest> is
                    // locked") is type `0x1A`, which the main window's default filter `0xFBFFFFFF`
                    // drops (bit 26 clear), so without this the line is filtered out of the
                    // scrollback and never reaches the bubble strip -- dropped entirely. Client-
                    // side notices escape only because they flow through `collect_scroll`, which
                    // makes the same offer; this is that offer for the network arm.
                    for m in &chat[before..] {
                        let took = panels.spew_offer(m.ty, &m.body, m.feedback);
                        if took {
                            self.stats.spew_lines += 1;
                        }
                        if crate::trace::notice() {
                            // The first of the two receivers, answered at
                            // arrival; the chat windows answer at render (`drive`).
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace line from {:#06x}: type={:#x} window={} \
                                 text={:?} -> spew box took={took} (list bound={} pending={} \
                                 drawn so far={}); queued for the chat windows",
                                opcode.0,
                                m.ty,
                                m.window,
                                m.body,
                                panels.spew_trace().0,
                                panels.spew_trace().1,
                                panels.spew_trace().2,
                            );
                        }
                    }
                    // Queued as they arrive, so a `LoggedOff` later in the same batch clears them:
                    // logging off takes the chat windows down with the screen, and a line still in
                    // flight is gone with them.
                    self.pending_chat
                        .extend(chat[before..].iter().cloned().map(|m| (None, m)));
                }
                SessionEvent::StateChanged(
                    dereth_client_net::client_session::SessionState::CharacterSelect
                    | dereth_client_net::client_session::SessionState::Disconnected(_),
                )
                | SessionEvent::LoggedOff => {
                    // The local player description belongs to the player object, so logging off
                    // drops it with the weenie rather than here; what this module owns is the bit
                    // that says whether a description has been unpacked into it, and the parked
                    // login snapshot that would otherwise be re-installed onto the next character's
                    // row. Player-description release is part of weenie teardown for exactly this
                    // reason. It occurs only when qualities are present, and that guard is
                    // load-bearing here, not decoration. This arm matches `CharacterSelect` as well
                    // as `LoggedOff`, and a **login** passes through `CharacterSelect` on its way
                    // in -- the recorded `early-inventory-and-casting` session does it before its
                    // own `0x0013`. Releasing there would strip the local player description that
                    // the next `0x02CD` is entitled to land in, making it `Unstorable`. A session
                    // that has received no description has nothing to release.
                    if std::mem::replace(&mut self.player_desc_received, false) {
                        world.release_player_desc();
                    }
                    // The joins are qualities-derived, so they go with them.
                    self.skills.clear();
                    self.spells.clear();
                    self.player_module = None;
                    self.player = None;
                    self.barber = None;
                    self.placements = WindowPlacements::default();
                    self.pending_chat.clear();
                    // The same argument one queue upstream: a notice still in the
                    // scroll at log-off goes down with the windows.
                    world.scroll.clear();
                    panels.spew_clear_pending();
                    self.applied_to = None;
                    // The three remembered names go with the session, the way
                    // every other communication-system member does: the last teller's pair is
                    // cleared by `ObjectStream::reset`'s `world = ()`, and these two
                    // have no other owner. A name that survived a log-off would compose a tell to
                    // somebody the next character has never spoken to.
                    world.chat.last_monarch_sender.clear();
                    world.chat.last_patron_sender.clear();
                }
                _ => {}
            }
        }
        chat
    }

    /// How many live trackers name a contract [`Hud::contract_table`] does not carry.
    ///
    /// Counted at receipt rather than inside [`HudView::contracts`] because the view is `&self`
    /// and cannot write a counter. With no
    /// table loaded at all the answer is the whole table, which is the honest reading: the tab
    /// would draw nothing.
    fn count_unresolved_contracts(&mut self, world: &dereth_client_model::World) {
        let n = match self.contract_table.as_ref() {
            Some(t) => world
                .contract_trackers()
                .keys()
                .filter(|id| !t.contracts.contains_key(id))
                .count(),
            None => world.contract_trackers().len(),
        };
        self.stats.contracts_unresolved = u64::try_from(n).unwrap_or(u64::MAX);
    }

    /// The current time as a [`dereth_primitives::ServerTime`] — what the contract-tracker update
    /// stamps `_time_of_server_update` with.
    ///
    /// The same clock [`Self::now`] carries; the two types are the same number and the conversion
    /// is named rather than sprinkled, so the contract receiver and
    /// `dereth_client_model::quests::fill_progress_string` cannot end up on different ones.
    fn server_now(&self) -> dereth_primitives::ServerTime {
        dereth_primitives::ServerTime(self.now.0)
    }

    /// Incoming communication updates the shared remembered speakers and chat state.
    fn ui_event(
        &mut self,
        opcode: dereth_protocol::Opcode,
        body: &[u8],
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        chat: &mut Vec<ChatMessage>,
        on_combat_mode: &mut dyn FnMut(
            &mut dereth_client_model::World,
            dereth_client_model::combat::CombatMode,
        ),
    ) {
        use dereth_protocol::comms;
        use dereth_protocol::Opcode;

        let mut r = dereth_protocol::archive::Reader::new(body);
        if crate::trace::notice()
            && matches!(
                opcode,
                Opcode::COMMUNICATION_TRANSIENT_STRING
                    | Opcode::COMMUNICATION_TRANSIENT_STRING_0317
                    | Opcode::COMMUNICATION_WEENIE_ERROR
                    | Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING
                    | Opcode::COMMUNICATION_TEXTBOX_STRING
                    | Opcode::ITEM_USE_DONE
            )
        {
            // The event as `dereth_client_net::client_session` handed it over (the `0xF7B0`
            // wrapper already stripped; `opcode` is the game-event id).
            tracing::debug!(
                target: "dereth::trace::notice",
                "notice-trace event {:#06x} ({}) body {} bytes: {}",
                opcode.0,
                opcode.name().unwrap_or("?"),
                body.len(),
                crate::trace::hex(body, 96),
            );
        }

        match opcode {
            // Character customization begins with the following sixteen fields, retained
            // verbatim; the screen performs retail's table-index inversion.
            Opcode::CHARACTER_START_BARBER => {
                match dereth_protocol::read_body::<dereth_protocol::trade::CharacterStartBarber>(
                    body,
                ) {
                    Ok(m) => {
                        self.barber = Some(m.0);
                        self.barber_generation = self.barber_generation.wrapping_add(1);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMMUNICATION_CHAT_ROOM_TRACKER => {
                match dereth_protocol::read_body::<comms::ChatRoomMembership>(body) {
                    Ok(m) => world.chat.recv_chat_room_tracker(m),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0xF7E0 Communication_TextboxString` — system chat. No speaker, so no prefix.
            Opcode::COMMUNICATION_TEXTBOX_STRING => {
                match comms::CommunicationTextboxString::read(&mut r) {
                    Ok(mut m) => {
                        // A player-killer death broadcast carries the tag `[PKDe]`. With the
                        // hear-PK-deaths option off the line is dropped before anything else;
                        // with it on, the tag is taken out and the line goes on to the squelch
                        // gate like any other.
                        match pk_death_filter(&m.text, world.player_system.options.hear_pk_deaths())
                        {
                            PkDeathLine::Dropped => {
                                self.stats.textbox_pk_deaths_dropped += 1;
                                return;
                            }
                            PkDeathLine::Stripped(text) => m.text = text,
                            PkDeathLine::Untagged => {}
                        }
                        // The `Communication_TextboxString` handler continues
                        // with a squelch gate; without it a global
                        // squelch of Craft, Magic, Salvaging or Fellowship would leave every system
                        // line of that type on screen.
                        //
                        // The squelch query uses id zero and an empty account name before any line
                        // is drawn. The **zero id** is the whole nuance: no character or account entry can
                        // reach this line, only the global per-type table, and only for a type
                        // `IsLegalChannel` accepts. `ChatState::is_squelched` is both
                        // halves, which is why the check is that call and not a table lookup.
                        if world.chat.is_squelched(ObjectId(0), "", m.text_type) {
                            if crate::trace::notice() {
                                tracing::debug!(
                                    target: "dereth::trace::notice",
                                    "notice-trace 0xF7E0 TextboxString type={:#x} \
                                     text={:?} -> squelched by the notice filter",
                                    m.text_type,
                                    m.text
                                );
                            }
                            self.stats.textbox_lines_squelched += 1;
                            return;
                        }
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0xF7E0 TextboxString type={:#x} text={:?}",
                                m.text_type,
                                m.text
                            );
                        }
                        chat.push(ChatMessage {
                            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
                            ty: u8::try_from(m.text_type).unwrap_or(0),
                            body: m.text,
                            prefix: None,
                            window: 0,
                        });
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x028B Communication_WeenieErrorWithString` — without this arm a line like
            // "You have entered the Trade channel." would never show.
            // Failure formatting turns the code and its `%s` argument into a chat line; see
            // `chat::failure`.
            //
            // The fellowship codes (`0x50B`–`0x50E`, `0x518`, `0x528`, …)
            // have arms in `chat::failure`, each verified against retail; a code with no arm is
            // retail's miss exit and prints nothing. The newline the arm's literal
            // ends in is trimmed here, as scroll insertion's first act would.
            Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING => {
                match comms::CommunicationWeenieErrorWithString::read(&mut r) {
                    Ok(m) => {
                        // Same failure-event arm and same notice as the stringless 0x028A
                        // sibling. The Abuse panel ignores this message's otherwise-substituted text.
                        panels.abuse_response(m.error_type);
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0x028B WeenieErrorWithString code={:#x} \
                                 text={:?} -> line={:?}",
                                m.error_type,
                                m.text,
                                dereth_client_contract::chat::failure::handle_failure_event(
                                    m.error_type,
                                    &m.text
                                )
                                .map(|c| c.body),
                            );
                        }
                        if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                            m.error_type,
                            &m.text,
                        ) {
                            chat.push(failure_line(c));
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The title-table and display-title notices are the two writes to
            // the title id, which is the only input the header's appended title has.
            //
            // The client's title-table handler does two
            // things: it copies
            // the display title **and** the title list, and then refreshes the panel,
            // whose middle third adds every entry of the title list to the list box. Keeping
            // only the first dword would leave the character wearing a title in the header while
            // the Titles tab had nothing to draw.
            //
            // The list goes into `player_system.social`, where the title table is modelled.
            Opcode::SOCIAL_CHARACTER_TITLE_TABLE => {
                match dereth_protocol::social::CharacterTitlesMessage::read(&mut r) {
                    Ok(m) => {
                        self.display_title = m.display_title;
                        world.player_system.social.display_title = m.display_title;
                        world.player_system.social.titles = m.titles;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // Adding or setting a character title only moves the display title when the server
            // says to; the client's is the same test.
            //
            // **The add is not gated.** The event adds the character title unconditionally and then
            // sends the set-display-title notice only when the flag is set; the two
            // add-title and set-display-title receivers each walk the title list for the id and insert
            // only when it is absent, which is the `contains` below.
            Opcode::SOCIAL_ADD_OR_SET_CHARACTER_TITLE => {
                match dereth_protocol::social::SocialAddOrSetCharacterTitle::read(&mut r) {
                    Ok(m) => {
                        let list = &mut world.player_system.social.titles;
                        if !list.contains(&m.new_title) {
                            list.push(m.new_title);
                        }
                        if m.set_as_display_title != 0 {
                            self.display_title = m.new_title;
                            world.player_system.social.display_title = m.new_title;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x02BB` / `0x02BC` / `0x02BD` — the three "hear" messages.
            //
            // Each one composes its whole line with one fixed formatting template and hands it to
            // the scroll as a **single body**; the notice's `prefix` argument is the
            // timestamp, never the speaker. The templates and the branch order live in
            // [`dereth_client_model::chat::composition`], pinned as literals against the retail client.
            //
            // There is no separator from a string table between name and message — there are seven
            // fixed format strings, and putting the bare name in `prefix` and the bare message in
            // `body` would produce the wrong split.
            //
            // **`0x02BB` and `0x02BC` are two functions, not one.**
            // The `Communication_HearSpeech` handler tests `senderID == player_id` and echoes
            // `You say, "%s"`; the ranged-talk handler has no such arm. The self-echo template has
            // exactly **one** use, inside
            // the first of the two.
            Opcode::COMMUNICATION_HEAR_SPEECH => {
                match comms::CommunicationHearSpeech::read(&mut r) {
                    Ok(m) => {
                        // **The hearing gate.**
                        //
                        // The handler first requires a local physics body and a nonzero sender.
                        // It then tests the local player's id before consulting `CanHear`.
                        // The self-echo arm is **above** that call, so your own line is never
                        // squelched and never out of earshot. `CanHear`'s account argument is the
                        // empty string it builds itself, not a name from the wire.
                        //
                        // **The player-null gate.**
                        //
                        // The null comparison tests the player's physics
                        // object, and it is the handler's first test. See
                        // [`Self::player_body`] for why that is `viewer.is_some()` and not
                        // `Hud::player` (which is `player_id`, read again shortly after for the
                        // self-echo).
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        let ps = self.speaker_player_space(m.sender_id);
                        let radius =
                            dereth_client_contract::radar::radar_range(self.player_outside());
                        let is_self = self.player == Some(m.sender_id);
                        if m.sender_id.0 == 0 {
                            self.stats.speech_lines_with_no_speaker += 1;
                        } else if is_self {
                            // The `You say, "…"` arm is **above** the `CanHear` call,
                            // above the Olthoi test and above the marker
                            // split — so your own line is never squelched, never out of earshot
                            // and never garbled, and its name is never trimmed because it is
                            // never used.
                            chat.push(speech(
                                m.text_type,
                                dereth_client_model::chat::composition::hear_speech_line(
                                    m.sender_id.0,
                                    self.player.map(|p| p.0),
                                    &m.sender_name,
                                    &m.message,
                                ),
                            ));
                            self.stats.speech_lines_composed += 1;
                        } else if world
                            .chat
                            .can_hear(m.sender_id, "", m.text_type, ps, radius)
                        {
                            // **The `^`/`&` split and the garble.** The handler searches the sender
                            // name for `^` and then for `&`, each followed by `trim(leading = 0,
                            // trailing = 1, marker)`; the trimmed name is what the
                            // `says` templates are given, so a shard that marks a name does not
                            // render `Bob^ says, "…"`.
                            let (marker, name) =
                                dereth_client_model::chat::composition::language_marker(
                                    &m.sender_name,
                                );
                            let name = name.to_owned();
                            let line = match self.garbled_or_plain(marker, &name, world) {
                                Some(g) => {
                                    self.stats.speech_lines_untranslated += 1;
                                    g
                                }
                                None => dereth_client_model::chat::composition::hear_speech_line(
                                    m.sender_id.0,
                                    self.player.map(|p| p.0),
                                    &name,
                                    &m.message,
                                ),
                            };
                            chat.push(speech(m.text_type, line));
                            self.stats.speech_lines_composed += 1;
                        } else if world.chat.is_squelched(m.sender_id, "", m.text_type) {
                            self.stats.speech_lines_squelched += 1;
                        } else {
                            self.stats.speech_lines_out_of_earshot += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02BC`'s own two gates, which are NOT `CanHear`'s.**
            //
            // Ranged speech uses a distinct handler and never calls `CanHear`. In order, it
            // requires the local physics body and a nonzero sender, applies the speaker squelch,
            // then tests 3-D distance against the range carried on the wire.
            //
            // **Three ways it differs from the say path, each easy to paper over.** The radius is
            // a `float` on the message rather than the radar radius, so it is neither 25 nor 75
            // and the indoor/outdoor choice does not enter into it. The distance is
            // **3-D**, where `CanHear`'s is `x² + y²`. And the boundary is
            // **inclusive**: this path refuses only when `d > range`, whereas `CanHear` refuses at
            // the boundary too. Two range tests in one
            // subsystem with opposite polarity at the edge.
            Opcode::COMMUNICATION_HEAR_RANGED_SPEECH => {
                match comms::CommunicationHearRangedSpeech::read(&mut r) {
                    Ok(m) => {
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        if m.sender_id.0 == 0 {
                            self.stats.speech_lines_with_no_speaker += 1;
                            return;
                        }
                        // The squelch query uses an empty account name, exactly as `CanHear` does;
                        // no account value comes from the wire.
                        if world.chat.is_squelched(m.sender_id, "", m.text_type) {
                            self.stats.speech_lines_squelched += 1;
                            return;
                        }
                        let in_range = dereth_client_model::range::objects_in_range_distance(
                            self.speaker_distance(m.sender_id),
                            f64::from(m.range),
                        );
                        if !in_range {
                            self.stats.ranged_lines_out_of_range += 1;
                            return;
                        }
                        chat.push(speech(
                            m.text_type,
                            dereth_client_model::chat::composition::hear_ranged_speech_line(
                                m.sender_id.0,
                                &m.sender_name,
                                &m.message,
                            ),
                        ));
                        self.stats.speech_lines_composed += 1;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---------------------------------------------------------------------------------
            // **The combat scroll.**
            //
            // `dereth_client_model::combat` carries `attacker_notification_line`,
            // `defender_notification_line`, `evasion_attacker_line` and `evasion_defender_line` —
            // with the hit-adjective table, the damage-type list, the body-part list and both of
            // the client's shipped trailing-space bugs. Without these arms every blow
            // struck in this client would be silent.
            //
            // All four handlers share one gate and one sink:
            // a combat-text squelch check and then adding the text to the scroll.
            Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::AttackerNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_SELF,
                        dereth_client_model::combat::attacker_notification_line(
                            &m.defender_name,
                            m.damage_type,
                            m.percent,
                            m.damage,
                            m.critical != 0,
                            m.attack_conditions,
                        ),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::DefenderNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_ENEMY,
                        dereth_client_model::combat::defender_notification_line(
                            &m.attacker_name,
                            m.damage_type,
                            m.percent,
                            m.damage,
                            m.damage_location,
                            m.critical != 0,
                            m.attack_conditions,
                        ),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The evasion-attacker notification — a scroll line of text type `0x16`.
            Opcode::COMBAT_HANDLE_EVASION_ATTACKER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::EvasionAttackerNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_SELF,
                        dereth_client_model::combat::evasion_attacker_line(&m.defender_name),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The evasion-defender notification — a scroll line of text type `0x15`.
            Opcode::COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::EvasionDefenderNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_ENEMY,
                        dereth_client_model::combat::evasion_defender_line(&m.attacker_name),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The victim notification — reached by **both** `0x01AC` and
            // `0x01AD` with identical arguments; the client draws no distinction. The server's
            // string is printed verbatim with a `"\n"` appended and **text type 0**, which is the
            // literal in the scroll insertion's `(..., 0, true, 0)` — not one of the combat types,
            // and not squelch-gated either: there is no squelch call
            // anywhere in the handler, unlike the four sibling
            // notification-event arms [`Self::push_combat_line`] serves.
            //
            // **The tail is scroll insertion, not a direct push.** Scroll insertion trims both
            // ends, stamps the timestamp prefix, and copies the line into
            // the chat log file; a line that skips it is a different line. `0x019E`'s arm below has
            // the same `(msg, 0, true, 0)` scroll-insertion tail in retail and also goes through
            // the scroll, so the two sibling handlers draw alike.
            Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF
            | Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER => {
                match dereth_protocol::combat::VictimNotificationOther::read(&mut r) {
                    Ok(m) => {
                        self.stats.victim_notifications += 1;
                        // The stored length counts the terminator, so a length of 1 is empty.
                        if !m.message.is_empty() {
                            world.scroll.add_text_to_scroll(
                                &format!("{}\n", m.message),
                                0,
                                true,
                                0,
                            );
                            self.stats.victim_notifications_announced += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x028A Communication_WeenieError` is
            // handled as a failure event with an empty string — the *same* path `0x028B` above
            // reaches, with an empty `%s`.
            // The `Communication_TransientString` handler is three lines:
            // widen the string and add it to the scroll as type `0x1A`. Type `0x1A` is the one the
            // scroll special-cases — no timestamp prefix and nothing written
            // to the chat log file — which `dereth_client_model::chat::route` already encodes as
            // `timestamped: false`.
            // `0x0317` carries the same one string and takes the same three lines.
            Opcode::COMMUNICATION_TRANSIENT_STRING
            | Opcode::COMMUNICATION_TRANSIENT_STRING_0317 => {
                match comms::CommunicationTransientString::read(&mut r) {
                    Ok(m) => {
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace {:#06x} TransientString text={:?} -> \
                                 chat type 0x1A (LOCAL_ERROR), window 0",
                                opcode.0,
                                m.text
                            );
                        }
                        chat.push(ChatMessage {
                            feedback: dereth_client_contract::feedback::Feedback::SERVER_TRANSIENT,
                            ty: u8::try_from(dereth_client_model::chat::text_type::LOCAL_ERROR)
                                .unwrap_or(0),
                            body: m.text,
                            prefix: None,
                            window: 0,
                        });
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMMUNICATION_WEENIE_ERROR => {
                match comms::CommunicationWeenieError::read(&mut r) {
                    Ok(m) => {
                        // The shared 0x04B8..0x04BA arm does not draw a failure
                        // line. It sends an abuse-report response notice, whose receiver writes
                        // the result field even while
                        // the panel is hidden. Keep the Silent chat arm below as the second half.
                        panels.abuse_response(m.error_type);
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0x028A WeenieError code={:#x} -> line={:?}",
                                m.error_type,
                                dereth_client_contract::chat::failure::handle_failure_event(
                                    m.error_type,
                                    ""
                                )
                                .map(|c| c.body),
                            );
                        }
                        if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                            m.error_type,
                            "",
                        ) {
                            chat.push(failure_line(c));
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The `Item_UseDone` handler's second half: a non-zero
            // `WeenieError` is handled as a failure event with an empty string. The first half —
            // the busy-count decrement — is `interaction::apply_events`', because it is world state and this
            // function owns the chat scroll. Both arms read the same blob; neither can see the
            // other's effect.
            Opcode::ITEM_USE_DONE => match dereth_protocol::objects::ItemUseDone::read(&mut r)
                .inspect(|m| {
                    world.research_use_done(m.failure_type);
                }) {
                Ok(m) if crate::trace::notice() && m.failure_type == 0 => {
                    tracing::debug!(
                        target: "dereth::trace::notice",
                        "notice-trace 0x01C7 UseDone code=0 (WeenieError.None): no line"
                    );
                }
                Ok(m) if m.failure_type != 0 => {
                    if crate::trace::notice() {
                        tracing::debug!(
                            target: "dereth::trace::notice",
                            "notice-trace 0x01C7 UseDone code={:#x} -> line={:?}",
                            m.failure_type,
                            dereth_client_contract::chat::failure::handle_failure_event(
                                m.failure_type,
                                ""
                            )
                            .map(|c| c.body),
                        );
                    }
                    if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                        m.failure_type,
                        "",
                    ) {
                        chat.push(c);
                    }
                }
                Ok(_) => {}
                Err(_) => self.stats.undecodable += 1,
            },
            Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH => {
                match comms::CommunicationHearDirectSpeech::read(&mut r) {
                    Ok(m) => {
                        // The same null-player gate as the two
                        // handlers above — the equality branch, and again
                        // the handler's first test. All three speech handlers carry it and
                        // the emote handler does not; transcribing it into only one of the three
                        // would leave the other two silently inconsistent, so the cluster is kept
                        // aligned.
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        // The `Communication_HearDirectSpeech` handler
                        // draws **nothing** for a tell the shard copied to us but addressed to
                        // somebody else: the `targetID == player_id` gate skips display.
                        // `hear_direct_speech_line`
                        // answers `None` for exactly that case, so the push is conditional and the
                        // two outcomes are counted separately — "no tells arrived" and "a tell
                        // arrived and was correctly not drawn" are different facts.
                        //
                        // **The garble arm sits OUTSIDE that gate.** It composes
                        // `GARBLED` and writes the line directly, so a tell in a language you
                        // cannot understand is shown **whoever it was addressed to** — an
                        // asymmetry in the client, not a simplification here. It is also below
                        // `senderID == targetID`, which takes the `You think, "…"` branch before the
                        // language gate, so a self-tell
                        // is never garbled either.
                        let self_tell = m.sender_id == m.target_id;
                        let (marker, name) =
                            dereth_client_model::chat::composition::language_marker(&m.sender_name);
                        let name = name.to_owned();
                        let garbled = if self_tell {
                            None
                        } else {
                            self.garbled_or_plain(marker, &name, world)
                        };
                        if let Some(line) = garbled {
                            chat.push(speech(m.text_type, line));
                            self.stats.speech_lines_composed += 1;
                            self.stats.direct_speech_lines_untranslated += 1;
                            return;
                        }
                        // The tail of
                        // the `Communication_HearDirectSpeech` handler, which is the only
                        // writer of the last teller in the client.
                        // It runs *before* the line is pushed only because `speech` consumed the
                        // name; the client does it after scroll insertion and neither can see
                        // the other's effect.
                        //
                        // **It is below the garble arm**, which is where the client
                        // has it: the last-teller id and name setters are called inside the
                        // `targetID == player_id` branch of the *understood* else — and those two
                        // are the **only** calls to either function in retail. The garble arm
                        // skips them entirely, so a
                        // tell you cannot understand does **not** arm `@r`; running the write
                        // first would point a reply at a speaker whose words were never
                        // shown.
                        self.note_last_teller(world, &m);
                        match dereth_client_model::chat::composition::hear_direct_speech_line(
                            m.sender_id.0,
                            m.target_id.0,
                            self.player.map(|p| p.0),
                            &name,
                            &m.message,
                        ) {
                            Some(line) => {
                                chat.push(speech(m.text_type, line));
                                self.stats.speech_lines_composed += 1;
                            }
                            None => self.stats.speech_lines_not_addressed_to_us += 1,
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---------------------------------------------------------------------------------
            // **`0x01E0` and `0x01E2`, the emote messages.**
            //
            // Without these arms **every emote any player performed would be silently discarded**
            // and `CanHear`'s second caller would have nothing to gate. They are two opcodes and
            // two functions and are wired separately here: one wired and one dead would pass any
            // test that only asks whether "an emote appeared".
            //
            // The emote handler calls `CanHear(senderID, EMOTE)` first. A refusal returns before
            // marker handling or composition; a success composes the line and writes it as emote text.
            //
            // **The gate is ahead of everything.** Composing first and gating after would draw the
            // same pixels and is not what retail does; more usefully, it would make the
            // untranslated substitution and the marker trim run for a speaker who is not heard,
            // which is observable in the counters below.
            //
            // Note what the emote handler does *not* have, both asserted by tests: no
            // null-object gate (the three speech handlers all open with one), and
            // no `senderID == player_id` self-echo arm — so your **own** acted emote is drawn,
            // while your own soul emote is discarded by the soul-emote handler before it ever gets
            // here.
            Opcode::COMMUNICATION_HEAR_EMOTE => match comms::CommunicationHearEmote::read(&mut r) {
                Ok(m) => self.hear_emote(world, chat, m.sender, &m.sender_name, &m.text),
                Err(_) => self.stats.undecodable += 1,
            },
            // Soul emotes first compare the sender with the local player. A self-emote returns
            // without drawing; any other sender has `^` appended to the name and continues through
            // the ordinary emote path.
            //
            // So the pose you performed yourself is **not** echoed to you by this path — the
            // client already printed it locally when the pose command sent `0x01E1`
            // (`dereth_client_model::emotes::pose`), and this is the branch that stops it appearing
            // twice. The caret it appends is then trimmed straight back off by the emote handler,
            // and its only lasting effect is to make the line one that is never garbled.
            Opcode::COMMUNICATION_HEAR_SOUL_EMOTE => {
                match comms::CommunicationHearSoulEmote::read(&mut r) {
                    Ok(m) => {
                        if self.player == Some(m.sender) {
                            self.stats.soul_emote_self_echoes_discarded += 1;
                            return;
                        }
                        let marked = dereth_client_model::chat::composition::soul_emote_sender_name(
                            &m.sender_name,
                        );
                        self.hear_emote(world, chat, m.sender, &marked, &m.text);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The `Communication_ChannelBroadcast` handler's user-name stores: the `0x4000` arm
            // ends by storing
            // `senderName` as the last `@monarch` user name and the `0x2000` arm ends
            // by storing `senderName` as the last `@patron` user name, and those two names are what `@mr `/`@pr `
            // and reply keys `0x10000020`/`0x10000021` expand to. Both are guarded by the same
            // test the rest of the function branches on: `senderName` empty means the broadcast
            // is **our own** line coming back (`"You say to your ..."`), and the client stores
            // nothing for it.
            //
            // **The function's other half, the formatted line.**
            // The formatting operands and the per-branch
            // text types are
            // represented by [`dereth_client_model::chat::composition::channel_broadcast_line`]. The order here is the
            // client's: the user-name store sits inside the format branch, then the line is
            // finished, then `is_squelched(0, "", type)` gates adding it to the chat scroll as
            // `(line, 0, type, 1)`. So a squelched line still arms `@pr` / `@mr`, exactly as in
            // retail.
            //
            // No gate: unlike the three speech handlers this one goes
            // straight into the channel-name lookup, so a broadcast that
            // arrives before the body exists is still drawn.
            Opcode::COMMUNICATION_CHANNEL_BROADCAST => {
                match comms::CommunicationChannelBroadcastRecv::read(&mut r) {
                    Ok(m) => {
                        self.note_at_channel_speaker(&m, world);
                        let (ty, line) =
                            dereth_client_model::chat::composition::channel_broadcast_line(
                                m.channel,
                                &m.sender_name,
                                &m.message,
                            );
                        // A scroll entry `(0, "", type)`: character 0 has no
                        // entry, so only the global per-type table can answer "yes" — and only
                        // for a type `IsLegalChannel` accepts, which of this handler's seven is
                        // Fellowship (`0x13`) alone.
                        if world.chat.is_squelched(ObjectId(0), "", ty) {
                            self.stats.channel_broadcast_lines_squelched += 1;
                            return;
                        }
                        chat.push(speech(
                            ty,
                            dereth_client_model::chat::composition::add_text_to_scroll_trim(&line)
                                .to_owned(),
                        ));
                        self.stats.channel_broadcast_lines_composed += 1;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // All twenty-six `Qualities_*Update*` forms, not only the four vitals ones.
            // The two **private** forms of each pair are about the player
            // by construction; the **public** ones carry an object id, and only the player's own
            // reaches the local player description this module keeps.
            //
            // Without the other twenty-two the character panel would show the character's stats
            // **as of login, for ever**.
            op if dereth_client_model::qualities::update::is_update_opcode(op) => {
                if let Some(mode) = self.apply_quality_update(op, body, world) {
                    on_combat_mode(world, mode);
                }
            }
            // The sixteen `Qualities_*Remove*Event` forms, `0x01D1`..=`0x01DE` plus
            // the late Int64 pair at `0x02B8`/`0x02B9`.
            //
            // The client has all sixteen private and public remove handlers over **eight**
            // remove-stat templates, one per generic quality
            // table. There is no skill, attribute or secondary-attribute remove.
            //
            // **This family is dead against ACE and the arm is still right.** ACE
            // carries the sixteen names in `PacketOpCodeNames.cs` and has no sender class for any
            // of them, and all twelve recorded `netblobs` scenarios (13,535 blobs) contain zero of
            // them against 1,443 `0x02CD`..`0x02EA` in the same scan. Parity with retail is the
            // reason, and the tests synthesise the bodies rather than replaying any.
            op if dereth_client_model::qualities::remove::is_remove_opcode(op) => {
                self.apply_quality_remove(op, body, world);
            }
            // The enchantment-registry messages have no arm here. The client has one enchantment
            // registry inside the player's qualities, and so does this build: `interaction.rs`
            // handles all five opcodes and writes it, and `HudView::enchantment_counts` and
            // `HudView::vitae` read it. A second arm here would be a **double apply**, counting the
            // spell totals twice per insert and making the buff and debuff lamps read double.
            // `0x01A8 Magic_RemoveSpell` *inbound*. The client sends this
            // opcode too for a confirmed DELETE, and the shard answers with the same opcode.
            // Without this arm a deleted spell would stay in the book and on the bars for ever
            // while the shard refused to cast it.
            Opcode::MAGIC_REMOVE_SPELL => {
                match dereth_protocol::qualities::MagicRemoveSpell::read(&mut r) {
                    Ok(m) => self.handle_magic_remove_spell(m.layered_spell_id, world),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x01F4 Communication_SetSquelchDB`.**
            //
            // Without this arm `dereth_client_model::chat::ChatState::squelch` has **no production
            // writer**, while incoming speech and emote handling read it on every line
            // from the three speech arms and the two emote arms above: each login would decode the
            // player's whole squelch list and discard it, and the player would hear everyone they
            // had ever muted.
            //
            // The message unpacks a whole squelch database and replaces the chat state's existing
            // database. See
            // [`dereth_client_model::chat::ChatState::recv_set_squelch_db`] for that handler read at the
            // bytes, including why its second call is dead.
            //
            // `read_body_padded` and not `read_body`: the retail reader ends with
            // `return (consumed <= size)`, so the retail receiver simply stops reading and
            // trailing alignment bytes are invisible to it. ACE's `GameMessage` constructors end
            // with `Writer.Align()`, so those bytes are on the wire from a real shard.
            Opcode::COMMUNICATION_SET_SQUELCH_DB => {
                match dereth_protocol::read_body_padded::<comms::CommunicationSetSquelchDb>(body) {
                    Ok(m) => {
                        world.chat.recv_set_squelch_db(m);
                        self.stats.squelch_db_applied += 1;
                        self.stats.squelch_rows_applied = u64::try_from(
                            world.chat.squelch.accounts.len() + world.chat.squelch.characters.len(),
                        )
                        .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **The fellowship family, all five of them.**
            //
            // These five opcodes are most of the inbound messages in the recorded sessions that
            // would otherwise have no receiver. Without them the fellowship state has **no
            // production writer**: `selection_type_rejects` reads it on every tab-target, so
            // `select_next`'s `Monster` arm would pick your own fellows, and the Fellowship entry
            // in the chat drop-down would never enable.
            //
            // See `dereth_client_model::fellowship`'s `impl World` block for the five fellowship handlers,
            // including the one retail radar-look update this
            // build reaches by polling instead of by notice.
            //
            // **And the chat line each one of them ends in**, the Fellowship panel's own
            // scroll insertion; without it a fellowship could be created, opened, closed and left
            // without the log saying a word. The sentences and their conditions are in
            // [`dereth_client_model::chat::composition`]'s fellowship section.
            Opcode::FELLOWSHIP_FULL_UPDATE => {
                match dereth_protocol::social::FellowshipFullUpdate::read(&mut r) {
                    Ok(m) => {
                        // A full update counts as creation when there is no fellowship copy yet or
                        // its member table is empty.
                        let table_was_empty = world
                            .fellowship
                            .as_ref()
                            .is_none_or(|f| f.members.is_empty());
                        let (created, leader_changed) = world.recv_fellowship_full_update(&m.0);
                        self.stats.fellowship_updates += 1;
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                        if created {
                            self.stats.fellowships_created += 1;
                        }
                        if leader_changed {
                            self.stats.fellowship_leader_changes += 1;
                        }
                        if let Some(f) = world.fellowship.as_ref() {
                            let leader_name =
                                f.members.get(&f.leader).map_or("", |l| l.name.as_str());
                            if let Some(line) =
                                dereth_client_model::chat::composition::fellowship_update_line(
                                    table_was_empty,
                                    world.is_the_player(f.leader),
                                    &f.name,
                                    f.open_fellow,
                                    leader_name,
                                )
                            {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::FELLOWSHIP_UPDATE_FELLOW => {
                match dereth_protocol::social::FellowshipUpdateFellow::read(&mut r) {
                    Ok(m) => {
                        let update =
                            dereth_client_model::fellowship::FellowUpdate::from_raw(m.update_type);
                        if world.recv_fellowship_update_fellow(m.fellow_id, &m.fellow, update) {
                            self.stats.fellows_added += 1;
                            // Once the update adds a previously absent member, the client looks up
                            // that member and composes `"%hs is now a member of your Fellowship.\n"`.
                            chat.push(fellowship_ui_line(
                                dereth_client_model::chat::composition::fellow_added_line(
                                    &m.fellow.name,
                                ),
                            ));
                            self.stats.fellowship_lines_composed += 1;
                        }
                        // The refusal branch: retail would have dereferenced a null
                        // fellowship pointer. A non-zero count here means a `0x02C0` reached this
                        // client before any `0x02BE` did, which is an ordering fact about the
                        // shard and not a decode failure -- hence its own counter rather than
                        // `undecodable`.
                        if world.fellowship.is_none() {
                            self.stats.fellow_updates_without_a_fellowship += 1;
                        } else {
                            self.stats.fellow_updates += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x02BF` carries nothing but its opcode. `dereth_protocol`'s `empty_message!` read is
            // infallible and **does not** check that the body is empty, so the `Err` arm here can
            // never fire; it is written out anyway so this arm keeps the shape of its four
            // neighbours and so a future `FellowshipDisband` with a body needs no rewrite.
            // The retail `Fellowship_Disband` handler takes no argument either.
            Opcode::FELLOWSHIP_DISBAND => {
                match dereth_protocol::social::FellowshipDisband::read(&mut r) {
                    Ok(_) => {
                        // Fellowship disbanded: nothing at all without a fellowship copy;
                        // otherwise the line is chosen on
                        // `_leader == player_id` and read *before* the copy is deleted.
                        if let Some(f) = world.fellowship.as_ref() {
                            let leader_name =
                                f.members.get(&f.leader).map_or("", |l| l.name.as_str());
                            let line =
                                dereth_client_model::chat::composition::fellowship_disbanded_line(
                                    world.is_the_player(f.leader),
                                    leader_name,
                                );
                            chat.push(fellowship_ui_line(line));
                            self.stats.fellowship_lines_composed += 1;
                        }
                        world.recv_fellowship_disband();
                        self.stats.fellowship_disbands += 1;
                        self.stats.fellowship_members = 0;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x00A3` and `0x00A4` are the same handler in retail but for the notice each raises,
            // so they share `recv_fellowship_member_left` and differ only in the counter. Both
            // carry one `ObjectID`: the member who left, which may be the player.
            Opcode::FELLOWSHIP_QUIT => {
                match dereth_protocol::social::FellowshipQuitNotice::read(&mut r) {
                    Ok(m) => {
                        // A fellow quit: the line is composed off the panel's copy before
                        // it is deleted (the name is read first, then the copy deleted).
                        if let Some(f) = world.fellowship.as_ref() {
                            let fellow_name =
                                f.members.get(&m.member).map_or("", |l| l.name.as_str());
                            if let Some(line) =
                                dereth_client_model::chat::composition::fellow_quit_line(
                                    world.is_the_player(m.member),
                                    f.is_fellow(m.member),
                                    &f.name,
                                    fellow_name,
                                )
                            {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                        let was_us = world.recv_fellowship_member_left(m.member, wall_clock_unix());
                        self.stats.fellowship_quits += 1;
                        if was_us {
                            self.stats.fellowship_departures_our_own += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::FELLOWSHIP_DISMISS => {
                match dereth_protocol::social::FellowshipDismiss::read(&mut r) {
                    Ok(m) => {
                        // A fellow dismissed, before the copy is deleted or the row
                        // removed. The leader test picks the second-person line.
                        if let Some(f) = world.fellowship.as_ref() {
                            let name_of = |id: dereth_primitives::ObjectId| {
                                f.members.get(&id).map_or("", |l| l.name.as_str())
                            };
                            if let Some(line) =
                                dereth_client_model::chat::composition::fellow_dismissed_line(
                                    world.is_the_player(m.target),
                                    f.is_fellow(m.target),
                                    world.is_the_player(f.leader),
                                    name_of(f.leader),
                                    name_of(m.target),
                                )
                            {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                        let was_us = world.recv_fellowship_member_left(m.target, wall_clock_unix());
                        self.stats.fellowship_dismissals += 1;
                        if was_us {
                            self.stats.fellowship_departures_our_own += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x01C9 Fellowship_FellowUpdateDone`, and retail does NOTHING.**
            //
            // `0x01C9` does not trigger a roster redraw after a
            // burst of `0x02C0` updates: after the opcode and subsystem guards, the
            // handler returns without stores or calls. The neighboring `0x01CA` fellow-stats-done
            // message shares the same empty behavior. Two "done" markers, two empty handlers.
            //
            // `dereth_protocol::social`'s own doc says the same (*"the client does
            // **nothing** with it; it exists so the server can bracket a burst of `0x02C0`
            // updates"*). So the arm is a **counter and
            // a `continue`**, and that is not a stub: the roster is already correct when this
            // arrives, because every `0x02C0` before it moved on its own. An
            // arm here that redrew anything would be inventing behaviour.
            //
            // One oddity is deliberately not reproduced: the dispatcher reads a dword past the
            // end of this empty message and passes it to the handler, which ignores the argument.
            Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE => {
                match dereth_protocol::social::FellowshipFellowUpdateDone::read(&mut r) {
                    Ok(_) => self.stats.fellow_update_done += 1,
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0226 House_HouseStatus`, the login `QueryHouse` answer.**
            //
            // The dispatcher reads the message's one `u32` and sends a failed-house-transaction
            // notice. Three receivers take that notice, and none reads the value: the house panel
            // redraws, the purchase panel clears its pending state, and the allegiance house flow
            // may repeat its lord query.
            //
            // The house panel closes its pending dialog and, when house data exists, re-runs all
            // seven display sections. So the
            // observable effect of `0x0226` is **a redraw of the House pane**, not a value stored
            // anywhere — which is why the pane's answer for a
            // houseless character is a constant rather than a field.
            //
            // The word on the wire is ACE's `WeenieError`: `GameEventHouseStatus.cs` writes
            // `(uint)weenieError`, `HandleActionQueryHouse` sends it with the default `BadParam`
            // (2) for *"no house owned"*, and eviction sends `HouseEvicted`. All three recorded
            // `0x0226`s carry the value 2. It is kept in [`HudStats::house_status_last_notice`]
            // as a **measurement**, not as state the panel reads — retail stores it nowhere and a
            // field the panel consulted would be this client inventing a behaviour.
            //
            // `0x0259 House_HouseTransaction` is the same handler in retail
            // (its UI dispatch calls the same one), so it shares this arm. It did not occur in the
            // capture corpus. The two opcodes
            // share one arm because retail shares one *function*, not because they look alike:
            // both dispatchers call the house-transaction handler, whose whole body raises the
            // failed-house-transaction notice with the word.
            //
            // ACE never sends `0x0259` — there is no `GameEvent` for it in
            // `ACE.Server/Network/GameEvent/Events` — so this arm is unreachable against the
            // tested shard and is asserted from a synthesised payload.
            Opcode::HOUSE_HOUSE_STATUS | Opcode::HOUSE_HOUSE_TRANSACTION => {
                match dereth_protocol::trade::HouseHouseStatus::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_status_notices += 1;
                        self.stats.house_status_last_notice = u64::from(m.notice_type);
                        // The house panel's update first deletes its house data
                        // and clears the pointer, because `0x0225` can have set it and a shard
                        // that evicts a player sends exactly this message
                        // (`Player_House.cs:566`, `GameEventHouseStatus(HouseEvicted)`).
                        world.clear_house_data();
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0225 House_HouseData`, the message that the purchase-time text
            // and five other sections of the House tab wait for.**
            //
            // The dispatcher unpacks a `HouseData` and calls
            // a handler that forwards `&data` in the house-data notice.
            // Its one registered receiver is the house UI; its update body
            // keeps a copy, then redraws. This build retains the copy for the pane to read.
            //
            // ACE sends it from `Player_House.cs::HandleActionQueryHouse` — the reply to the
            // `0x021E House_QueryHouse` that the player system sends
            // at login — whenever the account owns a house; a houseless account gets `0x0226`
            // instead, which is the arm above and why every one of the ten recordings has
            // the status and none has the data.
            Opcode::HOUSE_HOUSE_DATA => {
                match dereth_protocol::trade::HouseDataMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_data_notices += 1;
                        world.recv_house_data(&m);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x021D House_HouseProfile`, the message that OPENS the purchase /
            // maintenance window.**
            //
            // A slumlord use (`0x0036 Inventory_UseEvent`) is answered by a `0x021D` house profile;
            // no other server path sends one. Receipt of that profile raises the purchase or
            // maintenance window.
            //
            // The request half is `dereth_client_model::inventory::use_object`, which constructs
            // `0x0036`.
            //
            // The decoder uses the twelve-field order. The captured profile decodes to
            // dwelling `0x0592`, owner 0,
            // bitmask 1, min level 35, three `-1`s, type 2 (villa), an empty owner name, three buy
            // lines (2,000,000 Pyreal / 5 Writ of Refuge / 1 Crude Lockpick) and two rent lines,
            // and a test asserts exactly that against the recorded bytes.
            Opcode::HOUSE_HOUSE_PROFILE => {
                match dereth_protocol::trade::HouseProfileMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_profile_notices += 1;
                        world.recv_house_profile(&m);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0227 House_UpdateRentTime` and `0x0228 House_UpdateRentPayment`,
            // the two messages that move the House pane without a `0x0225`.**
            //
            // Both travel the same road as `0x0225`: their dispatchers unpack and call
            // handlers that forward the corresponding purchase/rent payment notice.
            // The one registered receiver is the house UI.
            //
            // What makes them *different* from `0x0225`, and the reason they need their own arm
            // rather than a shared one, is that the House panel's handlers do **not** go through
            // the update: they mutate the house data in place and redisplay it directly. So there
            // is no house notice to count, no house-data replacement, and — the part that is easy
            // to miss — **no arm at all for a houseless character**: both handlers return
            // straight away when there is no house data, with no redraw. `World` carries that guard
            // and reports it, which is what the second counter of each pair is for.
            //
            // The redraw is `HousePanel::update`'s third arm, which
            // compares the `HouseDataView` against the one it last drew. That is not a
            // retail shape — retail is pushed and redraws unconditionally — but it is
            // observationally identical here, because a redraw from an unchanged `HouseData`
            // produces the same eight rows.
            //
            // Both are corpus zeros (0 of 13,535 recorded blobs in all three
            // spaces), like every inbound house message but `0x0226`. ACE sends `0x0227` from
            // `GameEventHouseUpdateRentTime` and never sends `0x0228` at all, re-deriving the
            // whole rent list into a fresh `0x0225` instead; the tests synthesise both in
            // ACE's writer order rather than through our own encoder.
            Opcode::HOUSE_UPDATE_RENT_TIME => {
                match dereth_protocol::trade::HouseUpdateRentTime::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_rent_time_updates += 1;
                        if world.recv_update_rent_time(m.rent_time) {
                            self.stats.house_rent_time_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::HOUSE_UPDATE_RENT_PAYMENT => {
                match dereth_protocol::trade::HouseUpdateRentPayment::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_rent_payment_updates += 1;
                        if world.recv_update_rent_payment(&m.payments) {
                            self.stats.house_rent_payment_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0248 House_UpdateRestrictions`.** The house-data state
            // carries the update-restrictions handler's four gates in the right order;
            // this arm is its production caller.
            //
            // The four gates are the handler, and they are the reason this cannot be a bare
            // store: a zero id is ignored, the **local player is never given restrictions**
            // (the object id must not be the player's), an unknown object is ignored, and a
            // stale sequence byte is rejected by the per-object house-restriction timestamp. All
            // four live in `World`.
            //
            // The message is byte-packed and **unaligned from offset 5** — one sequence byte then
            // an unaligned object id, which retail reads as a byte at offset 4 and a u32 at
            // offset 5 before handing on the rest from offset 9.
            // `dereth_protocol::trade::HouseUpdateRestrictions` reads it that way.
            Opcode::HOUSE_UPDATE_RESTRICTIONS => {
                match dereth_protocol::trade::HouseUpdateRestrictions::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_restriction_updates += 1;
                        if world.recv_update_restrictions(m.sequence, m.sender, m.restrictions) {
                            self.stats.house_restrictions_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0257 House_UpdateHAR`, the answer `@house guest list` is for.**
            //
            // The message unpacks a house access record, formats its guest rows, and writes them to the scroll as
            // text type zero. The roommate pass uses the same shape with its selector set.
            //
            // Chat type **0** and window **0** — not the current command source, even though the
            // thing that asked for it was a typed command. That asymmetry is retail's, and the
            // scroll is a live consumer: `Hud::drain_scroll` turns it into `ChatMessage`s at the
            // head of the very next batch.
            //
            // **The arrival is proved rather than assumed.** The house-request capture has each
            // `0x024D` answered by this message; ACE sends it from
            // `Player_House.HandleActionGuestList` through `GameEventUpdateHAR`.
            Opcode::HOUSE_UPDATE_HAR => {
                match dereth_protocol::trade::HouseUpdateHar::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_har_updates += 1;
                        self.stats.house_har_guests +=
                            u64::try_from(m.0.guest_table.entries.len()).unwrap_or(u64::MAX);
                        let text = dereth_client_model::housing::har_dump(&m.0, false);
                        world.scroll.add_text_to_scroll(&text, 0, true, 0);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0271 House_AvailableHouses`, the answer to `@hslist`.**
            //
            // Available-house output is a header line, then coordinates, then a cut-off line. The
            // formatting literals are represented by `dereth_client_model::housing::available_houses_header`
            // and `coord_line`.
            //
            // Two branches are easy to miss: an **apartment** listing prints the header and stops,
            // with no coordinates, while a non-apartment listing adds the cut-off line only when
            // the count exceeds 400.
            //
            // The count in the header is the message's `num_houses`, which ACE sets to
            // `locations.Count` **before** `Distinct()`, while the coordinate list is the
            // deduplicated one. The two numbers legitimately disagree and neither is wrong.
            //
            // This one is a corpus zero — the capture sweep never sent `@hslist` — so unlike
            // `0x0257` its arrival is established from ACE's writer
            // (`GameEventHouseAvailableHouses`, enqueued unconditionally by
            // `Player_House.HandleActionListAvailable` for every `0x0270`) rather than from a
            // recording, and the tests drive it through the replay transport.
            Opcode::HOUSE_AVAILABLE_HOUSES => {
                match dereth_protocol::trade::HouseAvailableHouses::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_available_houses += 1;
                        let header = dereth_client_model::housing::available_houses_header(
                            m.house_type,
                            m.num_houses,
                        );
                        world.scroll.add_text_to_scroll(&header, 0, true, 0);
                        if dereth_client_model::housing::available_houses_lists_coords(m.house_type)
                        {
                            for cell in &m.landcells {
                                let Some((ew, ns)) = dereth_physics::landdefs::gid_to_lcoord(
                                    dereth_primitives::CellId(*cell),
                                ) else {
                                    continue;
                                };
                                let line = dereth_client_model::housing::coord_line(ew, ns);
                                world.scroll.add_text_to_scroll(&line, 0, true, 0);
                                self.stats.house_available_coord_lines += 1;
                            }
                            if dereth_client_model::housing::available_houses_truncated(
                                m.num_houses,
                            ) {
                                world.scroll.add_text_to_scroll(
                                    dereth_client_model::housing::TOO_MANY_HOUSES,
                                    0,
                                    true,
                                    0,
                                );
                            }
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x019E Combat_HandlePlayerDeathEvent`.**
            //
            // The handler unpacks the message, then emits its line only when it is non-empty and
            // the local player is neither the victim nor the killer.
            //
            // **The `!=` pair is the handler, not decoration.** A death you were part of — as the
            // victim or as the killer — produces **no** line here; those are carried by the combat
            // and victim-notification messages that arrive with it, and wiring this one
            // unconditionally would double them. It is the *third-party* deaths, the ones that
            // make the "So-and-so has been slain by…" traffic of a busy landblock, that only this
            // message carries. The ids are read **after** the string, because unpacking the string
            // advances the read pointer before the first id is taken.
            //
            // A stored length of 1 is the empty-string case because the length counts the
            // terminator. The trailing `"\n"` is retail's; `add_text_to_scroll` trims
            // it straight back off, and it is written here anyway so the call site stays honest.
            //
            // The scroll is a live consumer: `Hud::drain_scroll` turns it into `ChatMessage`s at
            // the head of the very next batch and `App` delivers those to the chat windows.
            Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT => {
                match dereth_protocol::combat::CombatHandlePlayerDeathEvent::read(&mut r) {
                    Ok(m) => {
                        self.stats.player_deaths += 1;
                        let ours = world.is_the_player(m.killed) || world.is_the_player(m.killer);
                        if !ours && !m.message.is_empty() {
                            world.scroll.add_text_to_scroll(
                                &format!("{}\n", m.message),
                                0,
                                true,
                                0,
                            );
                            self.stats.player_deaths_announced += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02C1 Magic_UpdateSpell`, the sibling of the arm above.**
            //
            // Without it a newly learned spell would not appear in the book until the player
            // relogged: the book is filled from `0x0013` at login, and `0x01A8` is only the
            // *remove* half.
            //
            // The add path mirrors spell removal: when a player description exists, update its
            // spellbook and then announce the spellbook change.
            //
            // Adding creates the spellbook when the character has
            // none and inserts a `SpellBookPage` whose `_casting_likelihood` is **0.0** — the
            // value is not on the wire, only the id is. It reaches
            // insertion into the packed spell-book page table, which does **not** overwrite an
            // existing key, so a repeat leaves the page the login description delivered alone.
            //
            // The rebuild is [`Self::handle_magic_remove_spell`]'s, for the same reason: the
            // panel's join is a pure function of the qualities, so a changed book rebuilds it.
            // The receipt serial lets each panel select a newly learned spell if its current
            // filter shows it, without mistaking a duplicate book update for a new spell.
            Opcode::MAGIC_UPDATE_SPELL => {
                match dereth_protocol::qualities::MagicUpdateSpell::read(&mut r) {
                    Ok(m) => self.handle_magic_update_spell(m.layered_spell_id, world),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0021 Social_FriendsUpdate`.**
            //
            // The message unpacks a counted list of friend records and then the update type; the
            // five-arm switch is `dereth_client_model::friends::FriendsUpdate`.
            //
            // It arrives **unprompted**: there is no subscribe, the Friends panel has no
            // visibility-changed handler at all, and the shard pushes one per login and one per
            // change. That is why the recorded captures carry 11 of them with the tab never once
            // opened.
            //
            // The reader is `dereth_ui_screens::panels::friends`.
            Opcode::SOCIAL_FRIENDS_UPDATE => {
                match dereth_protocol::social::SocialFriendsUpdate::read(&mut r) {
                    Ok(m) => {
                        let kind = world.recv_friends_update(&m);
                        self.stats.friends_updates += 1;
                        if !kind.is_handled() {
                            self.stats.friends_updates_unknown_type += 1;
                        }
                        self.stats.friends =
                            u64::try_from(world.friends().len()).unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0314` and `0x0315`, the contract tracker messages.**
            //
            // A corpus scan finds neither anywhere, so there is **no capture
            // oracle for the contract record** and the layout was checked against
            // `ACE/Source/ACE.Server/Network/Structure/ContractTracker.cs`'s `Write` extension
            // instead -- `Version`, `ContractId`, `(uint)Stage`, `TimeWhenDone`, `TimeWhenRepeats`,
            // then the two flags written by the *event* and not by the record. That is exactly
            // what `dereth_protocol::social` has.
            //
            // The reader is `dereth_ui_screens::panels::contracts`, through [`HudView::contracts`].
            Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER_TABLE => {
                match dereth_protocol::social::SocialSendClientContractTrackerTable::read(&mut r) {
                    Ok(m) => {
                        let n = world.recv_contract_tracker_table(&m, self.server_now());
                        self.stats.contract_tables += 1;
                        self.stats.contracts = u64::try_from(n).unwrap_or(u64::MAX);
                        self.count_unresolved_contracts(world);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER => {
                match dereth_protocol::social::SocialSendClientContractTracker::read(&mut r) {
                    Ok(m) => {
                        use dereth_client_model::quests::ContractUpdate;
                        match world.recv_contract_tracker(&m, self.server_now()) {
                            ContractUpdate::Added => self.stats.contract_trackers_added += 1,
                            ContractUpdate::Updated => self.stats.contract_trackers_updated += 1,
                            ContractUpdate::Removed => self.stats.contract_trackers_removed += 1,
                        }
                        if m.set_as_display_contract != 0 {
                            self.stats.contract_display_requests += 1;
                        }
                        self.stats.contracts =
                            u64::try_from(world.contract_trackers().len()).unwrap_or(u64::MAX);
                        self.count_unresolved_contracts(world);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x027A Allegiance_AllegianceLoginNotificationEvent`.**
            //
            // *"Your vassal has logged on."* The recorded captures have none.
            // **That zero is not evidence it never arrives**: no allegiance member logged on or
            // off during the capture sessions. It is the same kind of zero `0x019E` above has — an event nothing
            // asks for that fires the first time it happens — and not the kind `0x02C9`–`0x02CC`
            // have, which need an ACE developer command to exist at all.
            //
            // The dispatcher reads the member id and a status integer, then applies
            // `(id, status != 0)` to the logon-status handler.
            //
            // The chat scroll is the **only** receiver that handles that notice: every other
            // notice receiver ignores it. So the consumer really is the chat scroll and nothing
            // else.
            // In particular the roster's `" *"` logged-in marker is **not** refreshed here — retail
            // leaves it stale until the next `0x0020`, so this arm writes no `AllegianceData`.
            //
            // The cached member name is the append target and the logon/logoff phrase is the
            // suffix, so the resulting line is `name + suffix`.
            //
            // **The allegiance data lookup's refusal is the handler's only guard, and it is a real
            // one.** A member the cached tree does not hold prints *nothing at all*. There is no id
            // fallback and no "someone".
            // A client that has never sent `0x001B Allegiance_AllegianceUpdate` therefore shows no
            // logon lines whatsoever, which is retail's behaviour and not a gap in this arm.
            //
            // **Neither side gates on `DisplayAllegianceLogonNotifications`.** That option (retail
            // ordinal 24) is the *server's* test: `Player_Allegiance.cs:455` / `:467` send this
            // event only to online allegiance members whose `ShowAllegianceLogons` is set, and
            // never to the member who is logging in. The receiver reads no option word, so
            // gating here would drop lines the shard had already decided to send.
            //
            // The literals include their leading space and trailing newline:
            // `" is logged in.\n"` (15 bytes) and `" has logged out.\n"` (17).
            // `add_text_to_scroll` trims the newline straight back off, as it does for `0x019E`.
            Opcode::ALLEGIANCE_ALLEGIANCE_LOGIN_NOTIFICATION_EVENT => {
                match dereth_protocol::social::AllegianceLoginNotification::read(&mut r) {
                    Ok(m) => {
                        self.stats.allegiance_logins += 1;
                        // The bare name, not the full name: retail takes the stored name directly
                        // and never asks for the rank title.
                        let name = world.allegiance.look_up(m.member).map(|d| d.name.clone());
                        match name {
                            Some(name) => {
                                let suffix = if m.now_logged_in == 0 {
                                    " has logged out.\n"
                                } else {
                                    " is logged in.\n"
                                };
                                world.scroll.add_text_to_scroll(
                                    &format!("{name}{suffix}"),
                                    0,
                                    true,
                                    0,
                                );
                                self.stats.allegiance_logins_announced += 1;
                            }
                            None => self.stats.allegiance_logins_without_a_member += 1,
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02B4`, the salvage results.**
            //
            // The recorded captures have no arrivals of it, against a control of 23 `0x02BE` in the
            // same sweep, so the instrument was demonstrably looking. The layout
            // was therefore checked against `ACE.Server/Network/GameEvent/Events/
            // GameEventSalvageOperationsResult.cs` -- `(uint)skill`, a count-0 not-salvagable
            // list, the results list, then the augmentation bonus -- which is exactly what
            // `dereth_protocol::items::SalvageResultMessage` has, with the packed list
            // a `u32` count plus elements in both.
            //
            // `dereth_client_model::inventory::salvage` is the reader, and
            // `dereth_ui_screens::panels::salvage` is the sender of the `0x027D` it answers.
            //
            // The three name resolvers are supplied here because `dereth-client-model` has no dat access:
            // the material names are [`MATERIAL_TYPE_NAMES`] with `_` mapped to a space, the
            // skill name already comes from `panels::examination`, and the not-salvagable object
            // names come from the world.
            Opcode::INVENTORY_SALVAGE_OPERATIONS_RESULT_DATA => {
                match dereth_protocol::items::SalvageResultMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.salvage_results += 1;
                        let names = self.material_names.as_ref();
                        // The lookup is [`material_name_of`], which the
                        // display-name composer shares; the `"Unknown"` stays here because it is
                        // *this* caller's miss arm. The salvaged-materials text
                        // writes it into the string **before** it asks the mapper, so it is
                        // retail's own answer and not a placeholder -- and it is **not**
                        // the object-name query's answer to the same miss.
                        let material = |id: u32| {
                            material_name_of(names, id).unwrap_or_else(|| "Unknown".to_owned())
                        };
                        let skill = |id: u32| {
                            dereth_presentation::appraisal::skill_to_string(id).map(str::to_owned)
                        };
                        // Resolved up front, because the call below needs `&mut World` and a
                        // closure reading `weenie()` would hold an immutable borrow across it.
                        // An empty name is the object lookup missing as far
                        // as the non-suitables text is concerned: both skip
                        // the row without bumping its separator counter.
                        let resolved: Vec<(dereth_primitives::ObjectId, String)> = m
                            .not_salvagable
                            .iter()
                            .filter_map(|id| {
                                let n = world.weenie(*id)?.pwd.name.as_str();
                                (!n.is_empty()).then(|| (*id, n.to_owned()))
                            })
                            .collect();
                        let item = |id: dereth_primitives::ObjectId| {
                            resolved
                                .iter()
                                .find(|(i, _)| *i == id)
                                .map(|(_, n)| n.clone())
                        };
                        // `None` is the squelch; `Some(0)` is "both lists said nothing", which
                        // is not the same event and must not share a counter.
                        match world.recv_salvage_operations_result(&m, &material, &skill, &item) {
                            None => self.stats.salvage_results_squelched += 1,
                            Some(lines) => self.stats.salvage_lines += u64::from(lines),
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---- the six inbound chess messages, all of them corpus zeros ----
            //
            // The recorded corpus has **0** arrivals for each of
            // `0x0281`, `0x0282`, `0x0283`, `0x0284`, `0x0285` and `0x028C` in all three spaces
            // over 13,535 blobs across ten sessions, and the same for the five outbound ones. No
            // recorded session plays chess, so the shapes below are ACE's writer order
            // (`GameEventJoinGameResponse` and its five siblings, every one `Write(boardGuid.Full)`
            // then the `int`s) checked against the global sequence-stamp table.
            //
            // All six travel the identical road in retail --
            // the mini-game event through the game UI dispatcher and mini-game receiver (each a
            // one-line forward) to the single handler, which is the
            // single receiver registered for all of them (the window registers nine
            // notices in total at setup).
            //
            // Every one of the six is **guarded on the game id** and two of them on the game state as
            // well, so a message for a board this window did not join changes nothing and says
            // nothing; `minigame_guarded` is that guard's counter and must not read the same as
            // "never arrived".
            Opcode::GAME_JOIN_GAME_RESPONSE => {
                match dereth_protocol::trade::GameJoinGameResponse::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_join_game_response(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_START_GAME => match dereth_protocol::trade::GameStartGame::read(&mut r) {
                Ok(m) => {
                    self.stats.minigame_events += 1;
                    if !world.recv_start_game(&m) {
                        self.stats.minigame_guarded += 1;
                    }
                    self.stats.minigame_lines += world.drain_minigame_text() as u64;
                }
                Err(_) => self.stats.undecodable += 1,
            },
            Opcode::GAME_MOVE_RESPONSE => {
                match dereth_protocol::trade::GameMoveResponse::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_move_response(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_OPPONENT_TURN => {
                match dereth_protocol::trade::GameOpponentTurn::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_opponent_turn(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_OPPONENT_STALEMATE_STATE => {
                match dereth_protocol::trade::GameOpponentStalemateState::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_opponent_stalemate(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_GAME_OVER => match dereth_protocol::trade::GameGameOver::read(&mut r) {
                Ok(m) => {
                    self.stats.minigame_events += 1;
                    if !world.recv_game_over(&m) {
                        self.stats.minigame_guarded += 1;
                    }
                    self.stats.minigame_lines += world.drain_minigame_text() as u64;
                }
                Err(_) => self.stats.undecodable += 1,
            },
            // **Count every unhandled message.** Every `COMMUNICATION_*`, `FELLOWSHIP_*`,
            // `SOCIAL_*`, `HOUSE_*` and `GAME_*` message the shard sends that this build has no arm
            // for reaches here; without a counter it would vanish without leaving a number anywhere
            // in the process, and the unhandled set could only be found by reading source rather
            // than by running a client.
            _ => {
                self.stats.ui_events_unhandled += 1;
                crate::dropped::record(crate::dropped::Site::UiEvent, opcode);
            }
        }
    }

    /// `0x02C1 Magic_UpdateSpell` — the add
    /// half of the pair whose remove half is [`Self::handle_magic_remove_spell`].
    ///
    /// Add one spellbook page with casting likelihood `0.0`, creating the book if necessary.
    ///
    /// Three things that are easy to get wrong and are taken off that body:
    ///
    /// * **The book is created if the character has none.** A character who has learned no spell
    ///   at all has no `_spell_book`, and the first `0x02C1` is what makes one.
    /// * **`_casting_likelihood` is `0.0`.** The wire carries the id and nothing else.
    /// * **A duplicate keeps the page already there.** Hash-table insertion returns false without
    ///   overwriting, so a `0x02C1` for a spell
    ///   the login description already delivered must not reset its likelihood to zero.
    ///
    /// Retail raises the notice unconditionally, ignoring the add's return, so the rebuild is
    /// not gated on the insert having happened — exactly as the remove half is not.
    fn handle_magic_update_spell(&mut self, spell_id: u32, world: &mut dereth_client_model::World) {
        self.stats.spells_added += 1;
        let Some(q) = world.player_qualities_mut() else {
            return;
        };
        let book = q.spell_book.get_or_insert_with(Default::default);
        let newly_learned = !book.contains_key(&spell_id);
        book.entry(spell_id).or_insert_with(Default::default);
        world.research_spell_update(spell_id, newly_learned);
        self.spells = self.build_spells(world);
    }

    /// When a player description exists, remove the spell from its book and
    /// announce the spellbook change. Removal is one hash-table operation guarded by the book's
    /// presence. There is no layer mask: the
    /// key is the `layered_spell_id` as it arrived, which for a spellbook page is the plain id.
    ///
    /// The notice reaches exactly two handlers, and **neither reads the spell id it is handed**:
    ///
    /// * the spellbook view rebuilds the whole book from the qualities. That is
    ///   [`Self::build_spells`], the same join `PlayerDescReceived` uses.
    /// * the spell-casting view refreshes all eight tabs. That half lives in
    ///   `dereth_ui_screens::panels::spellcasting`, which reads `GameView::spell_tab` and
    ///   `GameView::is_spell_known` — both of which move as soon as the book does.
    ///
    /// The Skills panel is **not** a subscriber, so this rebuilds the spell list alone rather than
    /// calling [`Self::rebuild_panel_tables`]; the skills join is unchanged by a spell removal.
    ///
    /// Retail raises the notice unconditionally — the `Magic_RemoveSpell` handler ignores the
    /// removal's return, and the removal returns 0 when the character has no spellbook at
    /// all — so the rebuild here is not gated on the key having been present either.
    fn handle_magic_remove_spell(&mut self, spell_id: u32, world: &mut dereth_client_model::World) {
        self.stats.spells_removed += 1;
        let Some(q) = world.player_qualities_mut() else {
            return;
        };
        if let Some(book) = q.spell_book.as_mut() {
            book.remove(&spell_id);
        }
        self.spells = self.build_spells(world);
    }
    /// A stat update applied to the local player description — the whole family, in
    /// one body, the way the client has it.
    ///
    /// [`dereth_client_model::qualities::update`] carries the setters, so there is no hand
    /// decode of the `Attribute2nd` opcodes here and every one of them passes the
    /// `PropertySequenceGate` gate.
    ///
    /// Three things this does:
    ///
    /// 1. **The sequence gate.** The stat update runs it *before* the
    ///    setter, so a re-ordered or replayed update is dropped rather than applied.
    /// 2. **Skills and attributes land.** Those are the messages a raise comes back as.
    /// 3. **The `0x10000004` arm.** A quality the stat-management panel draws from sets
    ///    [`Self::raise_answered`], which `Self::drive` turns into
    ///    `SkillsPanel::clear_awaiting_raise` plus a footer re-run.
    fn apply_quality_update(
        &mut self,
        opcode: dereth_protocol::Opcode,
        body: &[u8],
        world: &mut dereth_client_model::World,
    ) -> Option<dereth_client_model::combat::CombatMode> {
        use dereth_client_model::qualities::update::{self, Outcome};

        let Some(u) = update::decode(opcode, body) else {
            self.stats.undecodable += 1;
            return None;
        };
        // There is no subject guard here: the arbiter is the store's own stat-update subject
        // lookup with the message's object id, so the identity that decides is the store's
        // and not a second copy of it kept here. The player-scoped update refuses a
        // public form naming anybody else, which is the same test against the one owner.
        // The store, sequence gate, and player-scoped handlers all belong to
        // `dereth_client_model::World`. Retail checks the `(property, sequence)` pair and then
        // writes the qualities on the same player object that the public form addresses. A second
        // `Qualities` and `PropertySequenceGate` kept here would let `Shop::update_total_value`
        // read `0` while this module held 9,995.
        let vital = u.key.stat_type() == dereth_client_model::StatType::Attribute2nd;
        // `if (!obj) return 0` — no player, no row, or a public form naming somebody else.
        let outcome = world.apply_player_quality_update(&u);
        if crate::trace::raise() && u.key.stat_type() == dereth_client_model::StatType::Int64 {
            // `AvailableExperience` / `TotalExperience` as the live shard
            // sends them after login, and what the stat update's gate said.
            tracing::debug!(
                target: "dereth::trace::raise",
                "raise-trace quality {:#06x} ({}) subject={:?} seq={} Int64/{} \
                 value={:?} -> outcome={:?} answers_a_raise={}",
                opcode.0,
                opcode.name().unwrap_or("?"),
                u.subject,
                u.sequence,
                u.key.property(),
                u.value,
                outcome,
                update::answers_a_raise_update(&u),
            );
        }
        let outcome = outcome?;
        match outcome {
            Outcome::Stale => {
                self.stats.quality_updates_stale += 1;
                return None;
            }
            Outcome::Unstorable => {
                self.stats.quality_updates_unstorable += 1;
                // The vitals opcodes must leave this counter at zero, so
                // the vitals subset keeps its own counter and its own meaning.
                if vital {
                    self.stats.vital_updates_unstorable += 1;
                }
                return None;
            }
            Outcome::Applied => {}
        }
        self.stats.quality_updates += 1;
        match u.key.stat_type() {
            dereth_client_model::StatType::Attribute2nd => self.stats.vital_updates += 1,
            dereth_client_model::StatType::Skill => self.stats.skill_updates += 1,
            dereth_client_model::StatType::Attribute => self.stats.attribute_updates += 1,
            _ => {}
        }
        // The skill and attribute joins read only qualities, so either kind of change rebuilds
        // them. The vitals are not among them — the Vitals panel reads the derived attribute live, and
        // rebuilding 38 skill rows on every regeneration tick would be 55 rebuilds a session.
        if matches!(
            u.key.stat_type(),
            dereth_client_model::StatType::Skill | dereth_client_model::StatType::Attribute
        ) {
            self.rebuild_panel_tables(world);
        }
        // `answers_a_raise_update` rather than `answers_a_raise`. The
        // key-only predicate cannot separate `Qualities_*UpdateAttribute2nd` (`0x02E7`, the whole
        // record, which is what `Train_TrainAttribute2nd` is answered with) from
        // `…Attribute2ndLevel` (`0x02E9`, a regeneration tick) because the two share tag 9, so it
        // would refuse both and a **vital raise's answer would never clear the latch**. The value
        // variant separates them; see
        // `dereth_client_model::qualities::update::answers_a_raise_update`.
        if update::answers_a_raise_update(&u) {
            self.raise_answered = true;
        }
        // **The purse delivery belongs to the writer.** The container UI registers
        // the player's `0x14 CoinValue` integer quality, and the player-quality notification
        // fires that handler set from inside
        // the stat update whenever the changed weenie is the player. So the subscription
        // belongs to the *writer*, not to this dispatcher: it is
        // the world's player-quality handlers, reached from
        // the quality-update path above and from the login-description path's own
        // `QualityScope::Player` arm — which is why a public `CoinValue` for the player moves
        // the purse too, and why the login description does.
        //
        // The six recorded `CoinValue` updates (9999, 9998, 9997, 9995 at the grocer, **9930** at
        // idx 4744 for a purchase and **9988** at idx 4788 for a sale) arrive in one store and are
        // not copied into another.
        //
        // The combat UI subscribes to the player's Int 0x28.
        // The quality-changed callback reads that quality and sets the combat mode (not forced).
        // This must follow the timestamped update, not merely decoding the packet or observing
        // the value.
        if u.key == dereth_client_model::StatKey::new(dereth_client_model::StatType::Int, 0x28) {
            if let dereth_client_model::StatValue::Int(mode) = u.value {
                return Some(dereth_client_model::combat::CombatMode::from_raw(
                    mode as u32,
                ));
            }
        }
        None
    }

    /// Remove a stat from the player's qualities — the
    /// sixteen `Qualities_*Remove*Event` forms in one body, the way the client has them.
    ///
    /// This is [`Self::apply_quality_update`]'s twin and is deliberately a *separate* function,
    /// because the two retail functions are separate and differ in three places that matter:
    ///
    /// 1. **No value.** The private remove form is `[u8 seq][u32 property]`, nine bytes with the
    ///    opcode.
    /// 2. **No mirror.** The stat update mirrors into the player description;
    ///    none of the eight remove-stat templates does.
    /// 3. **The remove handler**, not the change handler. That is what
    ///    decides everything below, because **only two panels in the whole client override the
    ///    quality-removal callback** — the Radar and Vendor panels, against
    ///    thirteen that override the quality-changed callback. Everybody else inherits the base no-op.
    ///
    /// So three things this does **not** do, each because retail does not:
    ///
    /// * **it does not clear the raise latch.** The stat-management panel overrides
    ///   the quality-changed callback and *not* the quality-removed one, so
    ///   [`dereth_client_model::qualities::update::answers_a_raise_update`] has no business on this path.
    /// * **it does not rebuild the skills or spells tables.** The Skills panel has no
    ///   quality-removed callback either — and there is no remove opcode that could name a skill or an
    ///   attribute in the first place, since stat removal has only the eight generic templates.
    /// * **it does not change the combat mode.**
    ///   The combat UI is the `Int 0x28` subscriber and has no quality-removed callback, so a
    ///   removed combat mode leaves the client's stance where it was. `apply_quality_update`
    ///   returns a [`dereth_client_model::combat::CombatMode`] for exactly that reason and this returns
    ///   nothing.
    ///
    /// What it *does* raise is the player-scope handler set, which is
    /// the container-update path's job because that is where retail keeps
    /// it: the container UI registers `Int 0x14 CoinValue` for the player,
    /// and the quality-removed handler's first arm (property `0x14`) reaches
    /// the total-value update. So a shard that clears the purse redraws it.
    fn apply_quality_remove(
        &mut self,
        opcode: dereth_protocol::Opcode,
        body: &[u8],
        world: &mut dereth_client_model::World,
    ) {
        use dereth_client_model::qualities::remove::{self, Outcome};

        let Some(r) = remove::decode(opcode, body) else {
            self.stats.undecodable += 1;
            return;
        };
        // `if (!obj) return 0` — no player, no row, or a public form naming somebody else. The
        // public forms for *other* objects are `interaction::apply_events_at_boundary`'s, which is
        // the same partition `apply_quality_update` already draws.
        let Some(outcome) = world.apply_player_quality_remove(&r) else {
            return;
        };
        match outcome {
            Outcome::Stale => self.stats.quality_removes_stale += 1,
            Outcome::Absent => {
                self.stats.quality_removes_absent += 1;
                self.stats.quality_removes += 1;
            }
            Outcome::Removed => self.stats.quality_removes += 1,
        }
    }
}
