//! UI request dispatch and inventory drag delivery.

use super::*;

impl Interaction {
    /// App's synchronous UI subscriber boundary. A listen-option change may disable the
    /// current chat focus, and that fallback must happen before the next queued ChatLine.
    pub fn run_ui_requests_with_chat_focus(
        &mut self,
        game: &mut dereth_client_model::World,
        player_desc_received: bool,
        now: ServerTime,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) -> Vec<UiRequest> {
        let mut unowned = Vec::new();
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        // The combat window's arms need what the keyboard's arms need.
        //
        // The two arms below are `set_requested_attack_height` and
        // `end_attack_request`, and both reach a readiness query with the true argument --
        // through attack-charge startup and attack execution respectively. So this is the
        // **attack** flavour, and it is answered by [`Self::ready_for_attack`] from the combat
        // mode, the combat table DataID and the six missile stances.
        //
        // It is read here rather than passed in because `use_time` calls this **after**
        // [`Self::note_player_physics`] and the style bridge, so the two inputs are already this
        // frame's; a parameter would only let a caller disagree with the frame it is in.
        //
        // `ServerTime` and `LocalTime` are the same here, which is what
        // `use_time`'s own `ServerTime(now.0)` already assumes.
        let ready = self.ready_for_attack(game);
        let local_now = dereth_primitives::LocalTime(now.0);
        let mut pending = std::collections::VecDeque::from(std::mem::take(&mut self.ui_requests));
        while let Some(r) = pending.pop_front() {
            if let UiRequest::Journal(action) = r {
                game.journal.apply(action, now.0, self.journal_coords);
                self.stats.ui_requests_handled += 1;
                continue;
            }
            if let UiRequest::Book(action) = r {
                game.book_action(action);
                for effect in game.take_book_requests().into_iter().rev() {
                    pending.push_front(effect);
                }
                self.stats.ui_requests_handled += 1;
                continue;
            }
            let effects = match &r {
                UiRequest::PaymentList(action) => {
                    let values = &self.trade_note_values;
                    Some(
                        game.payment_action(*action, |wcid| {
                            values
                                .iter()
                                .find(|(_, mapped)| *mapped == wcid)
                                .and_then(|(value, _)| i32::try_from(*value).ok())
                        })
                        .into_iter()
                        .map(|effect| match effect {
                            dereth_client_model::housing::PaymentEffect::Notice(text, feedback) => {
                                UiRequest::DisplayChatText {
                                    channel: 0x1A,
                                    text,
                                    feedback,
                                }
                            }
                            dereth_client_model::housing::PaymentEffect::Split {
                                item,
                                split,
                                max,
                            } => UiRequest::HouseSplitItem { item, split, max },
                            dereth_client_model::housing::PaymentEffect::Submit {
                                slumlord,
                                rent,
                                items,
                            } => UiRequest::HousePayment {
                                slumlord,
                                rent,
                                items,
                            },
                        })
                        .collect::<Vec<_>>(),
                    )
                }
                UiRequest::SalvageList(action) => {
                    let multiple = crate::hud::character_option(
                        game,
                        dereth_client_contract::PlayerOption::SalvageMultiple,
                    )
                    .unwrap_or(false);
                    Some(game.salvage_action(*action, multiple).into_iter().map(|effect| match effect {
                        dereth_client_model::inventory::salvage::SalvageEffect::Notice(text, feedback) => UiRequest::DisplayChatText {
feedback,channel: 0x1A, text},
                        dereth_client_model::inventory::salvage::SalvageEffect::Submit {tool, items} => UiRequest::SalvageItems {tool, items},
                    }).collect::<Vec<_>>())
                }
                _ => None,
            };
            if let Some(effects) = effects {
                for effect in effects.into_iter().rev() {
                    pending.push_front(effect);
                }
                self.stats.ui_requests_handled += 1;
                continue;
            }
            match r {
                UiRequest::Select(id) => {
                    // Selection assignment with `(ulong id, int force)` takes
                    // the id as a plain `ulong`, and **0 is how retail says "nothing"** -- the
                    // Escape key's `(0, 0)` call below is one caller, and
                    // the toolbar header's `(0, 0)` tail call is another. The world's selection is
                    // an `Option`, so the `ulong` 0 is `None` here.
                    let id = (id != ObjectId(0)).then_some(id);
                    game.set_selected_object(id, false, &mut out);
                    self.stats.selections += 1;
                }
                // The paper-doll message handler's `0x1C` tail — the half of
                // paper-doll item-under-mouse lookup that needs the player.
                //
                // It finds the upper inventory object for the mask and returns if there is none.
                // On a primary click (7) with a target mode up it executes the target mode on
                // **the player**; otherwise a primary click selects the object. A secondary click
                // (8) selects and examines it.
                //
                // The target-mode arm really does pass the player's id and not the found object.
                // Reproduced, not corrected.
                UiRequest::PaperDollRegion { mask, secondary } => {
                    let Some(player) = game.player else {
                        self.stats.ui_requests_handled += 1;
                        continue;
                    };
                    // The upper-inventory fallback is the player id when nothing
                    // the player is wearing covers that colour. Clicking a bare shoulder selects
                    // you, and it is the same statement that makes an empty doll clickable at all.
                    let id = game
                        .inventory(player)
                        .and_then(|inv| inv.upper_inv_obj(mask))
                        .unwrap_or(player);
                    if !secondary && self.target_mode != TargetMode::None {
                        self.execute_target_mode_for_item(player, game, &mut req, &mut out, now);
                        self.stats.ui_requests_handled += 1;
                        continue;
                    }
                    game.set_selected_object(Some(id), false, &mut out);
                    self.stats.selections += 1;
                    if secondary {
                        game.examine_object(&mut req, id);
                    }
                }
                // The busy-count increment has one owner and is also read
                // by enable_selection's current-focus fallback. Nothing is sent here.
                UiRequest::SetTalkFocus { focus } => {
                    if let Some(focus) = dereth_client_model::chat::TalkFocus::from_raw(focus) {
                        if focus == dereth_client_model::chat::TalkFocus::Selected {
                            if let Some(target) = game.selected_chat_player() {
                                game.chat.set_speakable_target(Some(target), true);
                                game.chat.set_talk_focus(focus);
                            }
                        } else {
                            game.chat.set_talk_focus(focus);
                        }
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The communication system's last speakable target, which the main chat window
                // writes: the tell destination of talk focus 2, and the object the squelch row
                // and the window's sweep ask about.
                UiRequest::SetLastSpeakableTarget { object } => {
                    game.chat.last_speakable_target = (object.0 != 0).then_some(object);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The chat-focus enable notice carries `(n, on)` and is raised by the
                // allegiance panel's three data-update tails. The write lands on the one
                // `ChatState`; its `TalkFocusNotice` then reaches the menu row as a talk-focus
                // notice offered to the UI, the same path `0x0295 ChatRoomTracker` and the
                // fellowship messages already take. Nothing is sent.
                UiRequest::SetTalkFocusEnabled { focus, enabled } => {
                    if let Some(focus) = dereth_client_model::chat::TalkFocus::from_raw(focus) {
                        game.chat.set_talk_focus_enabled(focus, enabled);
                        // **This does not queue; it tail-jumps.**
                        //
                        // Retail updates the communication system's talk-focus mask
                        // (`mask |= 1 << focus`) and tail-call the enable-chat notice broadcaster,
                        // which invokes every registered listener synchronously.
                        // Thus the row's state change has already happened when this function
                        // returns. The two `SetPlayerOption` arms below drain the queue
                        // here for the same reason. Without it the three notices the
                        // allegiance panel's data-update tails raise would stay in
                        // `ChatState::talk_focus_notices` until the **next** frame's
                        // `App::ui_use_time`. A one-frame-late row state is invisible in a
                        // screenshot, which is why a test pins it.
                        chat_focus(&mut game.chat);
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The toolbar's Use button sends
                // `(id, 0, 0)` behind a null check — the same entry point the
                // double-click uses, so it is routed the same way.
                UiRequest::Use(id) => self.use_object(id, game, &mut req, &mut out, now),
                UiRequest::ExecuteTargetItem(id) => {
                    self.execute_target_mode_for_item(id, game, &mut req, &mut out, now);
                }
                UiRequest::CloseExternalContainer(id) => {
                    self.use_object(id, game, &mut req, &mut out, now);
                    game.object_range_checks.unregister(
                        dereth_client_model::range::RangeHandler::ExternalContainer,
                        id,
                    );
                }
                UiRequest::UnregisterSlumlordRange => game.unregister_slumlord_range_checks(),
                UiRequest::UnregisterBookRange => game.unregister_book_range_checks(),
                // The toolbar's Examine button: the complete examination operation.
                //
                // The *panel* half of this route has its writer on the
                // screen (`GamePlayScreen`'s `UiRequest::Examine` arm calls
                // `panels::examination::ExaminationPanel::examine_object`); this is the game half.
                // Going through [`Self::examine_object`] means the toolbar button
                // pressed with nothing selected arms the cursor the way the native toolbar's
                // examination of the selected id does. The `UiRequest::SetTargetMode` arm below is
                // *the toolbar's other* producer and stays as it is; this one is the id path.
                UiRequest::Examine(id) => self.examine_object(game, &mut req, id),
                // Spell examination's first block:
                // item appraisal with a literal zero argument.
                // The **cancel**, and the only zero-id appraise in retail — the
                // other two appraisal senders both guard the id non-zero.
                //
                // It is deliberately **not** the arm above with a zero id. Spell examination calls
                // the packer directly and never goes through ordinary object examination,
                // whose zero arm means the opposite thing — arm `TargetMode::Examine`
                // — and is what `Self::examine_object`'s `id.0 == 0` leg is. Two functions in
                // retail, two variants here; routing the cancel through `examine_object` would
                // put the pointer into examine mode on every spell right-click.
                //
                // The *whether* is the panel's (`ExaminationPanel::examine_spell`, guarding on its
                // own two appraisal ids as retail does); this arm is the unconditional half.
                UiRequest::CancelAppraisal => game.cancel_appraisal(&mut req),
                UiRequest::VendorFilter(index) => {
                    game.vendor_filter = index;
                }
                UiRequest::StackSliderChanged { split, max } => {
                    game.split = SplitState {
                        split_size: split,
                        max_split_size: max,
                    };
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::ClearItemWaiting(item) => game.set_waiting_state(item, false),
                // The object-state half of the item-list drag-start handler's
                // `set_waiting_state(1)` call.
                // The pick-up ghost lives on the object in retail, which is what lets
                // the item list's flush and slot clear run over the slot
                // without losing it.
                UiRequest::SetItemWaiting(item) => game.set_waiting_state(item, true),
                // The toolbar's Use / Examine
                // button pressed with **nothing selected**. This is the only writer of
                // `target_mode` in the client. The field is read at
                // three sites (`wrapper_mouse`'s `SearchReason::TargetedUse` arm, `use_shortcut`'s
                // and `execute_target_mode_for_item`), all three dead without this writer.
                UiRequest::SetTargetMode(m) => {
                    self.set_target_mode(match m {
                        dereth_client_contract::view::TargetMode::None => TargetMode::None,
                        dereth_client_contract::view::TargetMode::Use => TargetMode::Use,
                        dereth_client_contract::view::TargetMode::Examine => TargetMode::Examine,
                    });
                    self.stats.target_modes_armed += 1;
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::DragDrop { item, target } => {
                    self.drag_drop(item, target, game, &mut req, &mut out, now);
                }
                // The toolbar's item-list drag-start notice has one
                // effect: remove the shortcut with server notification, which updates the retained
                // player module and sends the shortcut-removal event (`0x019D`)
                // under the same notify-server flag.
                //
                // The screen raises it the moment the icon leaves the tile, not when it lands —
                // that is the client's order (the item-list drag start's closing notice)
                // and it is the whole of *"dragging items FROM the shortcut bar to remove them"*:
                // there is no removal anywhere on the drop path.
                UiRequest::RemoveShortcut(item) => {
                    self.remove_shortcut(item, game, &mut req);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The make-shortcut key on the selected object: a shortcut in the first empty slot,
                // or one of the refusals, with the same pick-up of an object the player is not
                // carrying that a drag onto the bar makes.
                UiRequest::CreateShortcut(item) => {
                    if !self.create_shortcut_to_item(item, None, game, &mut req, &mut out, now) {
                        self.stats.requests_refused += 1;
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // Inscription focus loss sends the set-inscription request.
                // The panel has already made both
                // of the client's decisions — is there anything to say, and has it changed — so
                // this arm is the world's inscription attempt and the counter.
                UiRequest::SetInscription { object, text } => {
                    game.attempt_set_inscription(&mut req, object, &text);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The abuse-report panel has already refused an
                // empty field. Retail supplies literal status 1 and sends this ordered 0x0140
                // without an optimistic result; the 0x04B8..0x04BA reply owns that text.
                UiRequest::AbuseLog { target, complaint } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::AbuseLog(
                            dereth_protocol::admin::CharacterAbuseLogRequest {
                                target,
                                status: 1,
                                complaint,
                            },
                        ),
                    );
                }
                // The book panel's five writing requests. It has already checked
                // authorship and pending-request guards. Like inscription requests, these are
                // ordered game actions; no optimistic world write belongs here.
                UiRequest::BookAddPage { book } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookAddPage(
                            dereth_protocol::trade::WritingBookAddPage { book_id: book },
                        ),
                    );
                }
                UiRequest::BookModifyPage { book, page, text } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookModifyPage(
                            dereth_protocol::trade::WritingBookModifyPage {
                                book_id: book,
                                page,
                                text,
                            },
                        ),
                    );
                }
                UiRequest::BookDeletePage { book, page } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookDeletePage(
                            dereth_protocol::trade::WritingBookDeletePage {
                                book_id: book,
                                page,
                            },
                        ),
                    );
                }
                UiRequest::BookPageData { book, page } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookPageData(
                            dereth_protocol::trade::WritingBookPageData {
                                book_id: book,
                                page,
                            },
                        ),
                    );
                }
                UiRequest::BookData { book } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookData(
                            dereth_protocol::trade::WritingBookData { book_id: book },
                        ),
                    );
                }
                // The original barber panel prepares all sixteen values, closes
                // the modal, then waits for authoritative appearance data. This arm preserves
                // that field order and queues `0x0311`.
                UiRequest::BarberFinish(b) => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::FinishBarber(
                            dereth_protocol::trade::CharacterFinishBarber(
                                dereth_protocol::trade::BarberSettings {
                                    base_palette: b.base_palette,
                                    head_object: b.head_object,
                                    head_texture: b.head_texture,
                                    default_head_texture: b.default_head_texture,
                                    eyes_texture: b.eyes_texture,
                                    default_eyes_texture: b.default_eyes_texture,
                                    nose_texture: b.nose_texture,
                                    default_nose_texture: b.default_nose_texture,
                                    mouth_texture: b.mouth_texture,
                                    default_mouth_texture: b.default_mouth_texture,
                                    skin_palette: b.skin_palette,
                                    hair_palette: b.hair_palette,
                                    eyes_palette: b.eyes_palette,
                                    setup_id: b.setup_id,
                                    option1: b.option1,
                                    option2: b.option2,
                                },
                            ),
                        ),
                    );
                }
                // The character ping request -- `0x01E9`, with an
                // empty body. The panel decides *when* (on open, then every 120 s); this arm is
                // the one hop from its request to the ordered send queue.
                //
                // Without it `dereth_protocol::admin::CharacterRequestPing` has no production
                // caller, and `0x01EA` never arrives.
                UiRequest::RequestPing => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        Request::RequestPing(dereth_protocol::admin::CharacterRequestPing),
                    );
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // Chat command submission's only external call
                // carries `(text, window id)`, and that is
                // `dereth_client_model::cmd::CommandInterp::on_chat_command`, tested against
                // a live `@acehelp` observation. Its two server-bound outcomes both become
                // `Communication_Talk` (0x0015): `ForwardVerbatim` is the client's own
                // "unrecognised `@`-command goes to the server `@` and all, no allow-list", and a
                // plain line with the default talk focus is speech.
                //
                // `Handled` is a **locally** handled command (`@help`, `@quit`, …). One whose
                // handler is not built is counted and reported rather than turned into speech
                // — sending `@help` to the shard as a spoken line would be both wrong and rude.
                UiRequest::ChatEntry {
                    text,
                    window,
                    action,
                } => {
                    self.edit_chat_entry(game, window, text, action);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::StartTell { name } => {
                    let window = dereth_client_contract::chat::interface::window::MAIN;
                    let text = game
                        .chat
                        .entries
                        .get(&window)
                        .map_or_else(String::new, |e| e.text.clone());
                    self.edit_chat_entry(
                        game,
                        window,
                        text,
                        dereth_client_contract::chat::entry::EntryAction::StartTell { name },
                    );
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::ChatLine { text, window } => {
                    // Convert submitted chat from wide to narrow text BEFORE command parsing and
                    // every destination. Input history remains the original wide text.
                    if text.is_empty() {
                        continue;
                    }
                    game.chat
                        .entries
                        .entry(window)
                        .or_default()
                        .submit(&text, self.chat_interface);
                    let Some(narrow) = game
                        .chat
                        .text_conversion
                        .narrow(&text.encode_utf16().collect::<Vec<_>>())
                    else {
                        // Explicit unsupported host/count, not guessed ANSI bytes or Say fallback.
                        self.stats.chat_commands_refused += 1;
                        self.stats.ui_requests_handled += 1;
                        continue;
                    };
                    // Existing narrow-message String storage is a bijective BYTE SPELLING:
                    // cp1252::encode writes these exact bytes, even when ACP is not1252.
                    // This is NOT ACP decoding to Unicode, and must not be used for display.
                    let text = dereth_primitives::text::cp1252::decode(&narrow);
                    let focus =
                        dereth_client_model::cmd::TalkFocus::from_raw(game.chat.talk_focus as u32)
                            .unwrap_or(dereth_client_model::cmd::TalkFocus::Say);
                    let outcome = self.chat.on_chat_command(&text, window, focus);
                    self.dispatch_chat_outcome(
                        outcome,
                        window,
                        game,
                        &mut req,
                        &mut out,
                        now,
                        player_desc_received,
                        chat_focus,
                    );
                    self.stats.chat_lines_sent += 1;
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // ---- the three advancement requests -----------------------------------------
                //
                // Each arm is one call into `dereth_client_model::advancement`; without these
                // arms a raise would spend nothing and change nothing.
                //
                // No local stat is incremented here. `SkillsPanel` sets its awaiting-answer
                // latch before emitting the request, then waits for a quality update. This
                // prevents spending again before the server's response.
                //
                // The sender re-reads the skill advancement class immediately before sending,
                // rather than trusting the panel's copy. It uses the same player qualities as
                // `HudView::skill_advancement`, so its gate and the footer cost agree.
                UiRequest::TrainSkill { skill, xp } => {
                    let sent = Self::player_desc(game, player_desc_received).is_some_and(|q| {
                        dereth_client_model::advancement::send_train_skill(q, &mut req, skill, xp)
                    });
                    if !sent {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TrainSkillAdvancementClass { skill, credits } => {
                    let sent = Self::player_desc(game, player_desc_received).is_some_and(|q| {
                        dereth_client_model::advancement::send_train_skill_advancement_class(
                            q, &mut req, skill, credits,
                        )
                    });
                    if !sent {
                        self.stats.requests_refused += 1;
                    }
                }
                // ---- attribute raises: the other half of the same seam ----------------------
                //
                // Attribute selection, both footers and both raise buttons emit these requests;
                // this connects them to their senders.
                //
                // Unlike skill raising, attribute raising has no advancement-class gate: an
                // attribute cannot be untrained. The player-description lookup is the only
                // availability precondition; failure is counted as a refusal and sends nothing.
                //
                // `AttributeRow::wire_stat` already maps vital ids to `1/3/5`, not `2/4/6`,
                // before constructing the request. No local stat changes here; the panel owns
                // the awaiting-answer latch and the server response changes the value.
                UiRequest::TrainAttribute { attribute, xp } => {
                    match Self::player_desc(game, player_desc_received) {
                        Some(q) => {
                            dereth_client_model::advancement::send_train_attribute(
                                q, &mut req, attribute, xp,
                            );
                        }
                        None => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::TrainAttribute2nd { vital, xp } => {
                    match Self::player_desc(game, player_desc_received) {
                        Some(q) => {
                            dereth_client_model::advancement::send_train_attribute_2nd(
                                q, &mut req, vital, xp,
                            );
                        }
                        None => self.stats.requests_refused += 1,
                    }
                }
                // The filter changes locally before the server receives its new mask.
                UiRequest::SetSpellbookFilter { mask } => {
                    game.player_system.spell_filters = mask;
                    if let Some(module) = &mut game.player_system.module {
                        module.spell_filters = mask;
                    }
                    dereth_client_model::advancement::send_spellbook_filter(&mut req, mask);
                }
                // **The Titles tab reaches the shard.**
                //
                // The title panel's button arm sends the display-title request
                // with the selected title id and nothing else: no local
                // write, no gate, no reply waited for. The worn title is server state, and the
                // answer — `0x002B` with `set_as_display_title` set, or a fresh `0x0029` — is
                // what moves the header. A client that also wrote its own copy here would show
                // a title the shard had refused.
                UiRequest::SetDisplayCharacterTitle { title_id } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::SetDisplayCharacterTitle(
                            dereth_protocol::social::SocialSetDisplayCharacterTitle { title_id },
                        ),
                    );
                }
                // **The spell bar reaches the shard.**
                //
                // Adding and removing a spell from the player module are one shape: write
                // the retained spell-favorite list and send, with no gate and no reply to wait for.
                // The panel has already decided the index (`add_favorite`); nothing is
                // recomputed here, because a second opinion about the index would be a second
                // implementation of the same arithmetic.
                //
                // The model write is what makes the row appear: `SpellcastingPanel::update`
                // rebuilds a tab out of `GameView::spell_tab`, which is `spell_tabs` below.
                UiRequest::AddSpellFavorite {
                    spell_id,
                    index,
                    tab,
                } => {
                    if game.player_system.add_spell_favorite(spell_id, index, tab) {
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            dereth_client_model::Request::AddSpellFavorite(
                                dereth_protocol::combat::CharacterAddSpellFavorite {
                                    spell_id,
                                    index,
                                    spell_bank: i32::try_from(tab).unwrap_or(0),
                                },
                            ),
                        );
                        self.stats.spell_favorites_changed += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::RemoveSpellFavorite { spell_id, tab } => {
                    // Spell-favorite removal is unconditional in the
                    // client — removal from a list that has no such node is a no-op —
                    // and the event goes out either way, because the caller
                    // (`remove_spell_from_menu`) has already established that the *row*
                    // exists. So the send is not gated on the model having found it.
                    game.player_system.remove_spell_favorite(spell_id, tab);
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::RemoveSpellFavorite(
                            dereth_protocol::combat::CharacterRemoveSpellFavorite {
                                spell_id,
                                spell_bank: i32::try_from(tab).unwrap_or(0),
                            },
                        ),
                    );
                    self.stats.spell_favorites_changed += 1;
                }
                // The spellbook's delete-confirmation callback. **Nothing local**: the
                // spell leaves the book when `0x01A8` comes back the other way, and a client that
                // dropped the row here would show a spell gone that the shard had refused to
                // remove.
                UiRequest::RemoveSpell { spell_id } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::RemoveSpell(
                            dereth_protocol::qualities::MagicRemoveSpell {
                                layered_spell_id: spell_id,
                            },
                        ),
                    );
                    self.stats.spells_deleted += 1;
                }
                // A parent-container change stores the local pickup destination, so
                // `place_in_backpack` uses the pack the grid shows rather than always falling
                // back to the player.
                //
                // Opening a container does change that local
                // destination. This notice sends nothing; the server learns the selected
                // destination only from the subsequent `0x0019` or `0x0035`.
                UiRequest::NewParentContainer(id) => {
                    if game.on_new_parent_container(id) {
                        self.stats.open_containers_changed += 1;
                    }
                }
                // ---- vendor buttons ------------------------------------------------------
                //
                // The vendor button handler's eleven cases, arriving as the six
                // things they do. The two that **send** (`buy_single_item`, `sell_single_item`) put
                // their message in `req` and it goes out through `absorb` below, which is the
                // same wire slot every other request in this file uses.
                UiRequest::VendorBuySingle { item, split } => {
                    match game.buy_single_item(item, split, &mut req, &mut out, now) {
                        Ok(true) => self.stats.vendor_buys += 1,
                        Ok(false) | Err(_) => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::VendorAddToBuyList { item, split } => {
                    if game.add_to_buy_list(item, split) {
                        game.set_selected_object(Some(item), false, &mut out);
                        self.stats.vendor_basket_rows += 1;
                    }
                }
                // Apply the vendor panel's stack-size decision to the
                // `PublicWeenieDesc` owned by the world.
                //
                // The Add to List handler (`0x100000C3`) uses the split-slider amount when
                // stack size is at least 2, otherwise 1. Without this write, a stackable stock
                // row adds exactly one item per press regardless of the slider.
                UiRequest::VendorSetObjectStackSize { item, size } => {
                    if game.set_object_stack_size(item, size) {
                        self.stats.vendor_stack_sizes_set += 1;
                    }
                }
                UiRequest::VendorBuyAll => match game.buy_all(&mut req, &mut out, now) {
                    Ok(true) => self.stats.vendor_buys += 1,
                    Ok(false) | Err(_) => self.stats.requests_refused += 1,
                },
                UiRequest::VendorSellSingle { item } => {
                    match game.sell_single_item(item, game.split, &mut req, &mut out, now) {
                        Ok(true) => self.stats.vendor_sells += 1,
                        Ok(false) | Err(_) => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::VendorSellAll => {
                    if game.sell_all(&mut req, &mut out, now) {
                        self.stats.vendor_sells += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::VendorClearList { sell, item } => {
                    match (sell, item) {
                        (true, Some(id)) => {
                            game.remove_from_sell_list(id);
                        }
                        (true, None) => {
                            game.flush_sell_list_sell_state();
                        }
                        (false, Some(id)) => game.shop.buy_list.retain(|(i, _)| *i != id),
                        (false, None) => game.shop.buy_list.clear(),
                    }
                    game.prune_vendor_basket_descriptions();
                }
                // Vendor-close case `0x100000D6` raises the native confirmation dialog when a
                // basket is not empty; the close arm below handles that path.
                // The vendor sell-drop handler's tail.
                //
                // The world's sell-list addition is the vendor-list add operation plus
                // the vendor item's sell-state mark, and it already carries the whole of
                // the drag-acceptability check: the owned-by-player refusal, the four
                // acceptability messages and the add-item contained-count guard.
                // Without this arm its only production caller would be the `VendorInfo` handler's
                // sell-mode arm — a shop opened *by* dropping an item on the vendor in the
                // world — and the window's own drop target would have no producer at all.
                //
                // Local only: nothing goes on the wire until "Sell Item" or "Sell All".
                UiRequest::VendorAddToSell { item } => {
                    if game.add_item_to_sell(item, &mut out) {
                        self.stats.vendor_basket_rows += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                // Part of a stack: the split is asked for here, which is a real request, and the
                // row the split will take is held by the source until the new object arrives
                // (`absorb`'s attribute-change arm).
                UiRequest::VendorSplitToSell { item, split, max } => {
                    if game.split_item_to_sell(
                        item,
                        SplitState {
                            split_size: split,
                            max_split_size: max,
                        },
                        now,
                        &mut out,
                        &mut req,
                    ) {
                        self.stats.vendor_basket_rows += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                // **Retail asks, it does not announce.**
                //
                // Retail tests the buy list first and then the sell list. When both
                // are empty, it hides the window and returns. Otherwise, an existing dialog
                // context prevents a second prompt. With no dialog open, it sets property
                // `0x8E` to enum 1 and property `0xC5` to the unfinished-transactions prompt,
                // create a current-UI dialog with the close-vendor callback, and retain its context.
                //
                // Channel `0x1A` is **not** wrong in general: the two
                // `(0x1a, …)` calls in this very function
                // carry `L"You don't have enough money"` and `L"You must empty some slots in your
                // backpack first"` — both **buy** failures. A close with a full basket asks with
                // a dialog rather than printing a message at the top of the screen.
                UiRequest::VendorClose => match game.close_vendor_button(&mut out) {
                    Ok(true) => self.stats.vendor_closes += 1,
                    Ok(false) => {}
                    Err(text) => self.pending_vendor_close_confirmations.push(text),
                },
                // ---- secure trade --------------------------------------------------------
                //
                // The secure-trade panel's three buttons and drop target produce five request
                // forms. `dereth_client_model::trade` owns the rules: ownership, containment, anti-scam
                // comparison and the trade payload placed in `0x01FA`.
                //
                // **These requests are not exercised against a live shard.** A trade moves items
                // between characters irreversibly, so this arm is exercised from a loopback the
                // test process owns and the requests are asserted as bytes -- the same standard
                // held for buying and selling.
                UiRequest::TradeAddItem { item, position } => {
                    if game.trade_add_item(item, position, &mut out, &mut req) {
                        self.stats.trade_adds_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TradeSplitItem { item, split, max } => {
                    if game.split_item_for_trade(
                        item,
                        SplitState {
                            split_size: split,
                            max_split_size: max,
                        },
                        now,
                        &mut out,
                        &mut req,
                    ) {
                        self.stats.trade_splits_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TradeAccept {
                    displayed_self,
                    displayed_partner,
                } => match game.accept_trade(displayed_self, displayed_partner, &mut req) {
                    dereth_client_model::trade::AcceptDecision::Accept => {
                        self.stats.trade_accepts_sent += 1;
                    }
                    dereth_client_model::trade::AcceptDecision::OutOfSync => {
                        self.stats.trade_out_of_sync_sent += 1;
                    }
                },
                UiRequest::TradeDecline => {
                    game.decline_trade(&mut req);
                    self.stats.trade_control_sent += 1;
                }
                UiRequest::TradeReset => {
                    game.reset_trade_request(&mut req);
                    self.stats.trade_control_sent += 1;
                }
                // The close **button** (`0x1000008B`) only hides the window; the message goes out
                // because hiding raises the visibility-changed handler. One request either way.
                //
                // The two are kept apart: `close_trade_negotiations` writes `trade.open =
                // false`, so calling it directly here would take the window down on the
                // *range-exit* path and on `handle_close_trade`'s `0x01FF` too, and neither of
                // those hides anything in retail. The hide is
                // the button-message handler's hide call and belongs to
                // this gesture alone; `close_trade_window` is that call plus the
                // visibility-changed tail it raises.
                UiRequest::TradeClose => {
                    game.close_trade_window(now, &mut out, &mut req);
                    self.stats.trade_control_sent += 1;
                }
                UiRequest::ToggleCharacterSquelch(object) => {
                    if game.selected_chat_player() == Some(object) {
                        let squelched = game.chat.is_squelched(object, "", 1);
                        game.modify_character_squelch(&mut req, object, !squelched, "", 1);
                        self.stats.squelch_requests += 1;
                    }
                }
                UiRequest::ModifyCharacterSquelch {
                    object,
                    add,
                    account,
                    message_type,
                } => {
                    game.modify_character_squelch(&mut req, object, add, &account, message_type);
                    self.stats.squelch_requests += 1;
                }
                // `0x0059 Communication_ModifyAccountSquelch`. The original UI sends it only
                // from Squelch Account and Remove; this is that sending path.
                //
                // Counted on the same `squelch_requests` as `0x0058` because the two are one
                // gesture at two granularities and every reader of the stat wants the pair.
                UiRequest::ModifyAccountSquelch { add, name } => {
                    game.modify_account_squelch(&mut req, add, &name);
                    self.stats.squelch_requests += 1;
                }
                // The spell-component panel's
                // message `0x2F` arm — the set-desired-component-level event and the local mirror, in
                // that order. The bound is checked again here because
                // the player-module setter checks it too and this crate is
                // not the authority on it.
                UiRequest::SetDesiredComponentLevel { wcid, level } => {
                    let before = game.player_system.desired_comp_level(wcid);
                    if game.set_desired_component_level(&mut req, wcid, level) == level
                        && level != before
                    {
                        self.stats.desired_comp_sets += 1;
                    } else if !(0..dereth_client_model::magic::MAX_DESIRED_COMP_LEVEL)
                        .contains(&level)
                    {
                        self.stats.requests_refused += 1;
                    }
                }
                // Fill missing spell components — the one call site
                // of `shop_has_item` and `add_missing_comp`.
                UiRequest::VendorFillComponents {
                    category,
                    max_price,
                } => {
                    let vendor = game.shop.vendor_id;
                    let r = game.fill_component_list(category, max_price, &mut out);
                    self.stats.fill_components_rows += u64::try_from(r.added).unwrap_or(0);
                    self.stats.fill_components_missing +=
                        u64::try_from(r.not_stocked + r.short_stocked).unwrap_or(0);
                    if vendor.is_some() {
                        self.vendor_buying_tab_requested = vendor;
                    }
                }
                // **The wire that makes a player able to cast a spell.**
                //
                // The spellcasting panel is the client's **only** caller of
                // the spell-cast operation, which is the only caller
                // of `get_appropriate_spell_formula`.
                //
                // Nothing is decided here: the component check, the self-targeted branch, the
                // target compatibility test and the choice between `0x0048` and `0x004A` are all
                // decided by the spell-cast command. Its refusal string has **already** been
                // emitted as a `Notice::DisplayString` on channel `0x1A`, which `absorb` routes to
                // chat like every other refusal in this file. So the `Err` is only counted.
                // The spell research page's test. The model refuses it outside magic mode, with
                // nothing selected, or in a world without spell research; otherwise it is sent.
                UiRequest::TestSpellFormula { components } => {
                    match game.test_spell_formula(
                        &mut req,
                        &mut out,
                        &components,
                        self.era_features.spell_research,
                    ) {
                        Ok(()) => {
                            if !self.controlled_by_server {
                                self.stop_completely_requested = true;
                            }
                        }
                        Err(_) => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::GiveTo {
                    item,
                    target,
                    amount,
                } => {
                    if game.is_owned_by_player(item)
                        && game
                            .attempt_give(&mut req, &mut out, item, target, amount, now, false)
                            .is_err()
                    {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::PutInWorld(item) => {
                    if game.is_owned_by_player(item)
                        && game
                            .attempt_put_in_3d(&mut req, &mut out, item, now, false)
                            .is_err()
                    {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::SetCombatMode(raw) => {
                    let mode = if raw == 0 {
                        let (mode, refusal) = game.get_default_combat_mode(false);
                        if let Some(text) = refusal {
                            out.emit(dereth_client_model::Notice::DisplayString {
                                channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                                text,
                                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                            });
                        }
                        mode
                    } else {
                        dereth_client_model::combat::CombatMode::from_raw(raw)
                    };
                    if matches!(mode.raw(), 1 | 2 | 4 | 8) {
                        let ready = self.ready_for_mode_change(game);
                        if game
                            .set_combat_mode(&mut req, &mut out, mode, true, ready, false)
                            .is_err()
                        {
                            self.stats.requests_refused += 1;
                        }
                    }
                }
                UiRequest::AutoWear(item) => {
                    if game
                        .auto_wear(&mut req, &mut out, item, game.split, now, false)
                        .is_err()
                    {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::AutoWield { item, side } => {
                    use dereth_rules::slots::SlotSide;
                    let side = match side {
                        1 => SlotSide::Left,
                        2 => SlotSide::Right,
                        _ => SlotSide::Null,
                    };
                    if !game.auto_wield(
                        &mut req, &mut out, item, side, false, true, false, game.split, now,
                    ) {
                        self.stats.requests_refused += 1;
                    }
                }
                // The map teleport of a privileged player: the middle of the outdoor block.
                UiRequest::MapTeleport { x, y } => {
                    let cell = i32::try_from(x)
                        .ok()
                        .zip(i32::try_from(y).ok())
                        .map(|(x, y)| dereth_physics::landdefs::lcoord_to_gid(x, y))
                        .filter(|cell| cell.0 != 0);
                    if let Some(cell) = cell {
                        let mut destination = dereth_protocol::types::space::PositionWire {
                            objcell_id: cell.0,
                            ..Default::default()
                        };
                        destination.frame.origin.x = 10.;
                        destination.frame.origin.y = 10.;
                        destination.frame.orientation.w = 1.;
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            dereth_client_model::Request::AdvocateTeleport(
                                dereth_protocol::trade::AdvocateTeleport {
                                    target_name: String::new(),
                                    destination,
                                },
                            ),
                        );
                    }
                }
                UiRequest::QueryHouse => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::QueryHouse(
                            dereth_protocol::trade::HouseQueryHouse,
                        ),
                    );
                }
                UiRequest::OpenTrade(partner) => {
                    if partner.0 == 0 {
                    } else if game.combat.combat_mode
                        != dereth_client_model::combat::CombatMode::NonCombat
                    {
                        out.emit(dereth_client_model::Notice::DisplayString {
                            feedback: dereth_client_contract::feedback::Feedback::WARNING,
                            channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                            text: "You need to be in peace mode to trade.".into(),
                        });
                        self.stats.requests_refused += 1;
                    } else {
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            dereth_client_model::Request::OpenTradeNegotiations(
                                dereth_protocol::trade::TradeOpenTradeNegotiations { partner },
                            ),
                        );
                    }
                }
                UiRequest::AbuseLogStatus {
                    target,
                    enabled,
                    complaint,
                } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::AbuseLog(
                            dereth_protocol::admin::CharacterAbuseLogRequest {
                                target,
                                status: u32::from(enabled),
                                complaint,
                            },
                        ),
                    );
                }
                // Both option words at once: the classic Character page. The second word keeps
                // whatever the page does not show above its low byte.
                UiRequest::SetOptionWords {
                    options,
                    options2,
                    timestamp_format,
                    save,
                } => {
                    let player = &mut game.player_system;
                    player.options.options = options;
                    player.options.options2 = options2;
                    if let Some(module) = &mut player.module {
                        module.options = options;
                        module.options2 = options2;
                        if timestamp_format.is_some() {
                            module.timestamp_format = timestamp_format;
                        }
                    }
                    if save {
                        player.save_to_server(&mut req, true);
                    }
                }
                UiRequest::CastSpell { spell_id } => {
                    let sent = req.0.len();
                    let already_casting = game.magic.casting;
                    match game.cast_spell(&mut req, &mut out, spell_id) {
                        Ok(()) => {
                            // A spell the spell table does not know returns `Ok` and sends
                            // nothing, so "handled" is not "cast" and the two are counted apart.
                            if req.0.len() > sent {
                                self.stats.spells_cast += 1;
                                // **The free-hands-and-cast step's first statement.**
                                //
                                // Retail obtains the viewport's command interpreter
                                // and calls its free-hands operation. This tests `controlled_by_server`
                                // and, when control is local, tail-calls `stop_completely`, which
                                // stops the player completely. The spell-cast operation
                                // inlines the same call in its untargeted arm, so
                                // every cast the client actually sends runs it — and only a cast
                                // it sends, because each local refusal `return`s before
                                // the free-hands-and-cast step is reached.
                                //
                                // **It is here rather than in the game-side spell-cast operation**
                                // because the body is `Character`'s and `dereth-client-model` holds no
                                // physics object; `App::interaction_use_time` already drains this
                                // exact flag into `Character::stop_completely_from_action` for
                                // `EscapeKey`, which is the same call.
                                //
                                // The retail order is stop-then-send, and this is send-then-stop:
                                // the flag is not read until the frame's drain, which is after
                                // both, so no observer can tell. `req` is the outbound queue and
                                // is not flushed inside this arm either.
                                //
                                // Under the Horizon camera a cast asked for while a spell is still
                                // being cast, which the game refuses as too busy, leaves the
                                // player moving: the keys held through a cast go on backing up
                                // and stepping however often the cast key is pressed.
                                let keep_moving = self.casts_keep_moving && already_casting;
                                if !self.controlled_by_server && !keep_moving {
                                    self.stop_completely_requested = true;
                                }
                            }
                        }
                        Err(_) => self.stats.requests_refused += 1,
                    }
                }
                // ---- the combat window's own three controls -----------------------------------
                //
                // The combat panel's message handler reaches exactly the two combat
                // functions the *keys* reach (`handle_combat_action`), which is why
                // there is one implementation here and not a parallel mouse path: the press sets
                // the height and starts the build, the click releases at `-1.0`.
                // Under an interface whose presses are whole attacks the button's press is the
                // whole attack, as the key's is.
                UiRequest::CombatSetAttackHeight { height } if self.press_attacks => {
                    let h = attack_height_from_raw(height);
                    self.press_attack(game, &mut req, h, ready, local_now);
                }
                UiRequest::CombatSetAttackHeight { height } => {
                    let h = attack_height_from_raw(height);
                    if let Err(text) = game.set_requested_attack_height(h, ready, local_now) {
                        self.refuse(game, text);
                        self.stats.requests_refused += 1;
                    } else {
                        self.stats.attack_height_changes += 1;
                    }
                }
                UiRequest::CombatEndAttack { height } => {
                    let h = attack_height_from_raw(height);
                    let before = req.0.len();
                    game.end_attack_request(&mut req, h, None, ready, local_now);
                    if req.0.len() > before {
                        self.stats.attacks_released += 1;
                    }
                }
                // The UI-requested power is `position * 0.001` clamped to `[0, 1]`. Nothing goes
                // on the wire: the cap is read again when the attack request ends, which is where
                // it changes what the swing sends. The desired-attack-power-changed notice is the
                // window's own read-back and is applied by the frame's
                // `on_desired_attack_power_changed`.
                UiRequest::CombatSetDesiredPower { position } => {
                    game.combat.set_ui_requested_power_from_scrollbar(position);
                    self.stats.desired_power_changes += 1;
                }
                // The radar padlock's request reaches the same native setter as the
                // lock-UI command: bit 24 of the second options word, then broadcast option
                // `0x33`. The latter emits one immediate 0x0005 because LockUI is auto-save. Queue the
                // global-0D presentation half only after this authoritative write, matching the
                // native button's set_lock_ui-then-global-broadcast order.
                UiRequest::SetLockUi(locked) => {
                    self.apply_lock_ui_option(game, &mut req, locked, now);
                    self.pending_ui_layout_commands
                        .push(UiLayoutCommand::SetLockUi(locked));
                }
                // ---- the option wire's three ends ---------------------------------------------
                //
                // The option-change path applies `(option, current)` to the player module.
                // The page named one option
                // and its value; `PlayerSystem::set_option` is the option-change handler's whole
                // body — the two
                // fellowship mutual exclusions, the four engine side effects, and the
                // `is_auto_save_option` split between an immediate `0x0005` and the dirty flag.
                //
                // **The word is read-modify-written, never composed.** `Options::set` flips one
                // mask in the word `apply_player_module` took off `0x0013`, and
                // `mirror_options_into_module` copies both words back into the retained blob, so
                // every bit this build does not model survives — bit 25 of the second word included.
                UiRequest::SetPlayerOption(option, value) => {
                    let ordinal = crate::hud::option_ordinal(option);
                    let change = game.player_system.set_option(ordinal, value, now);
                    // Player-option change handling: exactly these five listening
                    // options recompute all six Turbine rows. Do not collapse the notices.
                    if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
                        let heritage = Self::player_desc(game, player_desc_received)
                            .map_or(0, |q| q.inq_int(0xBC));
                        game.enable_chat_talk_focuses(
                            dereth_client_model::chat::composition::is_olthoi(heritage),
                        );
                        chat_focus(&mut game.chat);
                    }
                    // Player-option-changed event `0x0005`,
                    // one per option change whose option is one of the twenty-one
                    // `is_auto_save_option` ordinals.
                    //
                    // **Sent at once, not only at the next save.** Without this send the change
                    // would reach the shard only at the next options save or 480-second flush.
                    // That is not a cosmetic delay: the five listening options
                    // are **chat channel subscriptions**, and ACE's
                    // `GameActionSetSingleCharacterOption` answers `ListenToAllegianceChat`
                    // turning on with `Player.JoinTurbineChatChannel("Allegiance")` and a fresh
                    // `0x0295`. Without `0x0005` there is no way at all for a player to (re)join
                    // a Turbine chat room in a live session — which is what a monarch who has
                    // just gained a first vassal needs, because ACE's `SwearAllegiance` sends the
                    // tracker to the **vassal** only.
                    //
                    // **A loop, not one send.** A fellowship exclusion re-enters
                    // the option-change handler for the *other* option (the auto-accept setter
                    // tail-calls the change handler with option `0x12`), and that inner call
                    // sends its own `0x0005` before the outer one does. `change.sends` is that
                    // wire order; sending only the clicked option would leave the shard holding
                    // `AutomaticallyAcceptFellowshipRequests` under an unlit box.
                    for (ordinal, value) in &change.sends {
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            Request::PlayerOptionChanged(
                                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                    option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                    value: u32::from(*value),
                                },
                            ),
                        );
                        self.stats.option_changes_sent += 1;
                    }
                    if change.deferred {
                        self.stats.option_changes_deferred += 1;
                    }
                    let effect = change.effect;
                    // The player-option handler's four engine effects — daylight,
                    // fog enablement, weather enablement and target tracking.
                    //
                    // **All four reach the scene, and none of them reaches it from here.**
                    // They are edge
                    // detectors in `App::frame` (`update_target_tracking`,
                    // `apply_player_option_effects`) rather than consumers of this edge, because
                    // this arm holds no scene and because a whole-module load (`0x0013` at login,
                    // `0x01A1` on a round trip) raises no option change at all — which is exactly
                    // why retail has player-module loading as a second
                    // producer running the same four calls.
                    //
                    // The counter therefore keeps counting and keeps its name: it is the number
                    // of **edges this arm declined**, not the number of effects nobody applied.
                    if effect.is_some() {
                        self.stats.option_side_effects_unapplied += 1;
                    }
                }
                // Saving the option page starts by sending `0x01A1`. The codec round-trips all
                // five recorded blobs byte-identically, including the stamp. `save_to_server`
                // re-packs the module the server sent — it is not
                // rebuilt from this crate's model of it.
                // An interface's own setting kept in the player module: written into the same
                // module every other save re-packs, then sent once the edits pause.
                UiRequest::SetPlayerModuleString { property, value } => {
                    if game
                        .player_system
                        .set_gameplay_option_string(property, value, now)
                    {
                        self.module_save_at = Some(ServerTime(now.0 + MODULE_SAVE_DELAY));
                    }
                }
                UiRequest::SavePlayerOptions => {
                    if game.player_system.save_to_server(&mut req, false) {
                        self.stats.player_modules_sent += 1;
                    }
                }
                // ---- the two vital queries a selection change sends ---------------------------
                //
                // Toolbar selection-change handling is the only producer of either
                // message in the client, and it reaches `dereth_client_model::World`'s
                // `query_health` / `query_item_mana` senders; without it the toolbar would draw a
                // health meter and ask nobody to fill it.
                UiRequest::QueryHealth(id) => {
                    game.query_health(&mut req, id);
                    self.stats.vital_queries += 1;
                }
                UiRequest::QueryItemMana(id) => {
                    game.query_item_mana(&mut req, id);
                    self.stats.vital_queries += 1;
                }
                // ---- the allegiance panel's update request -------------------------------------
                //
                // The display path from `0x0020` to the roster depends on this send: nothing
                // else produces `0x001F`, and the shard answers
                // this request, or pushes on change to an allegiance it has already been asked
                // about, and is otherwise silent. All 31 `0x001F` messages in the three
                // observed fellowship sessions are the retail client sending them.
                UiRequest::AllegianceUpdateRequest { on } => {
                    game.allegiance_update_request(&mut req, on);
                    self.stats.allegiance_update_requests += 1;
                }
                // A window's own busy latch: the count the busy cursor reads.
                UiRequest::Busy { raised } => {
                    let busy = &mut game.magic.busy_count;
                    *busy = if raised {
                        busy.saturating_add(1)
                    } else {
                        busy.saturating_sub(1)
                    };
                }
                // ---- the allegiance panel's three buttons ---------------------------------
                //
                // The allegiance panel's message-1 handler raises a dialog
                // and stops; `0x001D` / `0x001E` leave from the dialog's close callback. So this
                // arm **queues a question**, it does not send. See `confirm_allegiance`.
                UiRequest::AllegianceConfirmation {
                    action,
                    target,
                    prompt,
                } => {
                    self.pending_allegiance_confirmations
                        .push((action, target, prompt));
                    self.stats.allegiance_confirmations_raised += 1;
                }
                // ---- the fellowship panel's seven request forms -------------------------------
                //
                // Without these the Fellowship tab could draw nothing and
                // ask for nothing. `0x00A6` below is the one that matters most: the shard sends
                // `0x02C0 Fellowship_UpdateFellow` -- 39 arrivals across three recorded
                // sessions -- only while a client has subscribed with it.
                UiRequest::FellowshipUpdateRequest { on } => {
                    game.fellowship_update_request(&mut req, on);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipCreate { name, share_xp } => {
                    game.create_fellowship(&mut req, &name, share_xp);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipQuit { disband } => {
                    game.leave_fellowship(&mut req, &mut out, disband);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipDismiss { target } => {
                    game.dismiss_fellow(&mut req, &mut out, target);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipRecruit { target } => {
                    game.recruit_fellow(&mut req, &mut out, target);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipAssignNewLeader { target } => {
                    game.assign_fellow_leader(&mut req, &mut out, target);
                    self.stats.fellowship_requests += 1;
                }
                // The one arm with local state behind it: `listen_to_element_message`
                // case `0x1000027D` flips the open-fellowship flag **before** it sends, so the button's
                // caption changes on the click rather than on the shard's answer.
                UiRequest::FellowshipToggleOpenness => {
                    if game.fellowship_toggle_openness(&mut req).is_some() {
                        self.stats.fellowship_requests += 1;
                    }
                }
                // ---- the friends panel's add/remove requests ----------------------
                //
                // The 100-friend cap belongs to the shared
                // add-friend operation: `@friends add` reaches it without visiting the panel.
                // Duplicating the refusal in UI and command paths would let them drift.
                UiRequest::AddFriend { name } => {
                    if game.add_friend(&mut req, &name).is_some() {
                        self.stats.friends_requests += 1;
                    } else {
                        self.stats.friends_list_full_refusals += 1;
                    }
                }
                UiRequest::RemoveFriend { target } => {
                    game.remove_friend(&mut req, target);
                    self.stats.friends_requests += 1;
                }
                // ---- the contracts panel's abandon request ------------------------
                //
                // The panel requires a selected row and nonzero contract
                // id. The shared operation repeats the nonzero-id guard because a command can
                // reach it without a panel selection.
                UiRequest::AbandonContract { contract_id } => {
                    if game.abandon_contract(&mut req, contract_id).is_some() {
                        self.stats.contract_requests += 1;
                    }
                }
                // ---- the salvage window's two requests ---------------------------
                //
                // `0x027D Inventory_CreateTinkeringTool`'s only caller in
                // retail is the salvage panel.
                UiRequest::SalvageItems { tool, items } => {
                    if game.create_tinkering_tool(&mut req, tool, &items).is_some() {
                        self.stats.salvage_requests += 1;
                    }
                }
                // `(channel, text)` from a panel. It goes
                // through the same `Scroll` entry point the notice sink's own `DisplayString` arm
                // uses, so a panel's line and a game-side refusal land in one ordered stream.
                UiRequest::DisplayChatText {
                    channel,
                    text,
                    feedback,
                } => {
                    game.scroll
                        .add_feedback_to_scroll(&text, channel, true, 0, feedback);
                    self.stats.panel_notice_strings += 1;
                }
                UiRequest::ChannelBroadcast { channel, text } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        Request::ChannelBroadcast(
                            dereth_protocol::comms::CommunicationChannelBroadcast {
                                channel,
                                message: text,
                            },
                        ),
                    );
                }
                // The urgent-assistance window's Send button.
                //
                // Retail returns without sending or advancing when the text box
                // is empty; otherwise it broadcasts the text on Help channel `0x400`.
                //
                // The channel broadcast `(channel, text)` is `0x0147
                // Communication_ChannelBroadcast`, client-to-server, `{ channel, message }` — no
                // sender name, which is [`dereth_protocol::comms::CommunicationChannelBroadcast`]'s
                // own correction to the community catalogue. This is the same request the `@f`
                // family builds in [`Self::chat_command`]'s channel-command arm, and it is
                // deliberately built **without** that arm's
                // `dereth_client_model::chat::channel_command_broadcasts_on` gate: the channel-command handler
                // refuses `0x400` by value, which is what
                // makes `@help` a help *command* rather than a channel, so the window is the only
                // producer of a Help-channel broadcast in the client and a gate written for the
                // chat commands would refuse the one caller that is allowed.
                //
                // Every guard is already the panel's: the empty-box test is
                // `UrgentAssistancePanel::send`'s, and the channel is retail's literal `0x400`.
                // There is nothing left for this arm to decide, so it decides nothing.
                // Housing payment's two sends and
                // the failed-house-transaction handler's retry. All three name the
                // slumlord and nothing else; the guards are the panel's, because the panel is the
                // only thing in retail that constructs any of them.
                UiRequest::HousePayment {
                    slumlord,
                    rent,
                    items,
                } => {
                    if game.house_payment(&mut req, slumlord, rent, &items) {
                        self.stats.house_payments_sent += 1;
                    }
                }
                UiRequest::HouseSplitItem { item, split, max } => {
                    if game.split_item_for_house(
                        item,
                        SplitState {
                            split_size: split,
                            max_split_size: max,
                        },
                        now,
                        &mut out,
                        &mut req,
                    ) {
                        self.stats.house_splits_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::HousePaymentConfirmation { rent } => {
                    self.pending_house_payment_confirmations.push(rent);
                }
                UiRequest::HouseQueryLord { slumlord } => {
                    if game.query_lord(&mut req, slumlord) {
                        self.stats.house_lord_queries += 1;
                    }
                }
                // ---- the chess window's three gestures ---------------------------
                //
                // The chess panel's two element-message paths and its resign-dialog answer
                // all use model-owned guards. Those guards read the joined game, current state
                // and board; illegal moves must be refused before reaching the wire.
                //
                // Drain text into the scroll here rather than inside the handlers so they can
                // operate with only a mutable borrow of `World.minigame`.
                UiRequest::MiniGameButton(id) => {
                    if game.minigame_button(id, &mut req) {
                        self.stats.minigame_gestures += 1;
                    }
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                UiRequest::MiniGameBoardPress(cell) => {
                    if game.minigame_board_press(cell, &mut req) {
                        self.stats.minigame_gestures += 1;
                    }
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                UiRequest::MiniGameQuitAnswer(confirmed) => {
                    game.minigame_quit_answer(confirmed, &mut req);
                    self.stats.minigame_gestures += 1;
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                other => {
                    unowned.push(other);
                    continue;
                }
            }
            self.stats.ui_requests_handled += 1;
        }
        self.absorb(game, out, req);
        unowned
    }

    /// A completed drag, from an item list or the paper doll onto somewhere.
    ///
    /// **Nothing moves here.** The panel ghosted the icon with `set_waiting_state(1)` and
    /// the item stays where it is until `Item_ServerSaysMoveItem` arrives.
    fn drag_drop(
        &mut self,
        item: ObjectId,
        target: DropTarget,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        let whole = game.split.is_whole_stack();
        let amount = game.split.split_size;
        let r = match target {
            // Toolbar backpack-button drop handling, exact catcher id `0x100001B1`.
            // The ownership fork and all five arguments are literal:
            //
            // * unowned -> `attempt_to_place_in_container(item, player id, 0, true, 0)`;
            // * owned   -> place in backpack `(item, false)`;
            // * false   -> `set_waiting_state(item, 0)`.
            //
            // The screen cannot make this decision because it deliberately has no object table.
            DropTarget::BackpackButton => {
                let accepted = if game.is_owned_by_player(item) {
                    game.place_in_backpack(req, out, item, false, game.split, now)
                } else if let Some(player) = game.player {
                    game.attempt_to_place_in_container(
                        req,
                        out,
                        item,
                        player,
                        ObjectId(0),
                        true,
                        0,
                        game.split,
                        now,
                    )
                } else {
                    false
                };
                if !accepted {
                    self.stats.requests_refused += 1;
                    game.set_waiting_state(item, false);
                }
                return;
            }
            // The viewport forwards drop message `0x15`. Handling reads the shared cursor
            // and arms a Drop search; the subsequent 3-D pick decides the target, so this
            // step does not send a request.
            DropTarget::World => {
                self.drop_item = item;
                self.reason = SearchReason::Drop;
                self.stats.picks_requested += 1;
                // **This arm arms the pick.** `handle_drop_release`
                // is four statements and the fourth is the one that matters:
                //
                // Read the live mouse coordinates, retain the dropped item id, set the search
                // reason to Drop, and call object search at those coordinates with the final
                // argument true. That last call is what arms the pick.
                //
                // Without it the arm would set a reason nothing would ever answer. `use_time`'s
                // step 3 is gated on `WorldPicker::looking_for_object`, so `place_in_3d` ->
                // `attempt_place_in_3d` -> `attempt_give` -> `0x00CD` would be unreachable by any
                // gesture, and — because `search_reason` is cleared **only** at the tail of
                // `on_world_object_found` — `SearchReason::Drop` (5) would then stay set and
                // outrank every `< SearchReason::Examine` / `< SearchReason::Use` gate in
                // [`Self::wrapper_mouse`], so the next left click, double-click and right release
                // in the viewport would do nothing either. One drop would disable the viewport for
                // the rest of the session.
                //
                // The increment above counts the request, not the arm, which is why the tests
                // assert `looking_for_object` and not the counter.
                //
                // The `false` leg — object search's unsigned viewport compare rejecting
                // the point — is the client's own, and the client does nothing about it either. It
                // is not reachable from here on the live tree: a `DropTarget::World` is produced
                // only by a release whose drop catcher is `<SBOX>`, and `<SBOX>`'s box **is** the
                // viewport (measured: `0,0..799,599` at 800x600). Counted rather than asserted, so
                // a layout that ever makes it reachable shows up as a number instead of a wedge.
                //
                // **That "`<SBOX>`'s box *is* the viewport" clause is literal rather
                // than incidental**: the rectangle here is `<SBOX>`'s own screen box, pushed in by
                // [`Self::note_game_viewport`], so a drop inside a *resized* smart box is measured
                // against the box it landed in. Measuring against the whole window would accept
                // the point and then aim the ray at the wrong place.
                let rect = self.device_viewport(self.screen);
                if !self.pick.find_object(self.cursor.0, self.cursor.1, rect) {
                    self.stats.drops_outside_the_viewport += 1;
                }
                return;
            }
            DropTarget::Container(c) => {
                if whole {
                    game.attempt_put_in_container(req, out, item, c, 0, now, false)
                } else {
                    game.attempt_split_to_container(req, out, item, c, 0, amount, now, false)
                }
            }
            DropTarget::EquipLocation { .. } | DropTarget::EquipCanvas => {
                use dereth_client_model::inventory::equip::EquipmentDropOutcome;
                let outcome = match target {
                    DropTarget::EquipLocation { mask, side } => {
                        game.drop_equipment_at_location(req, out, item, mask, side, game.split, now)
                    }
                    _ => game.drop_equipment_on_canvas(req, out, item, game.split, now),
                };
                match outcome {
                    EquipmentDropOutcome::Refused => {
                        self.stats.requests_refused += 1;
                        game.set_waiting_state(item, false);
                    }
                    EquipmentDropOutcome::Wield => self.stats.wields_requested += 1,
                    EquipmentDropOutcome::Wear => self.stats.wears_requested += 1,
                    EquipmentDropOutcome::Unblock => self.stats.unblocks_started += 1,
                }
                return;
            }
            // The toolbar-drop shortcut arm, for a drag that is a real
            // item rather than a shortcut alias: remove the shortcut in slot `n`, create one to
            // the item there, and move a different displaced object to the first empty slot to
            // the right of `n`, if there is one.
            //
            // **The wire half is not the 480-second options flush.** Shortcut addition's
            // notify-server flag guards two consecutive
            // calls — the add-shortcut event (`0x019C`) and then
            // the retained-module addition — and shortcut removal
            // pairs the remove-shortcut event (`0x019D`) with retained-module removal
            // the same way. The server is told at the moment of the drop; the
            // flush is for the *options*, which is a different section of the same blob.
            //
            // `create_shortcut_to_item`'s own gates, in its order: an unknown weenie
            // returns; the shortcut-eligibility check refuses with a message; an object that is
            // not the player's is picked up first, because this call came from a drag
            // (place in backpack `(item, false)`, and a refusal there ends it); and then
            // the shortcut removal `(item, true)` so that one object never occupies two slots.
            //
            // The first two eligibility tests combine an unresolved
            // per-object predicate with description bit 4 and item-type bit `0x10`. Those
            // tests are not implemented here, so this path is more permissive for a
            // creature or `0x10`-typed object. The third test, whether the object belongs
            // to the open vendor, is implemented from `vendor_id`. The omitted tests can
            // allow extra drops; their unresolved meaning must not be mistaken for parity.
            DropTarget::ShortcutSlot(n) => {
                let Ok(slot) = usize::try_from(n) else {
                    self.stats.requests_refused += 1;
                    return;
                };
                // `handle_drop_release` never calls `set_waiting_state`, and there is no server reply
                // that would clear one: whatever ghosted the source icon must be undone here.
                if let Some(w) = game.tables.weenies.get_mut(item) {
                    w.waiting = false;
                }
                if !self.shortcut_drop(item, slot, game, req, out, now) {
                    self.stats.requests_refused += 1;
                }
                return;
            }
            // The toolbar-drop handler's *other* arm — reached once `0x21` on a shortcut tile
            // starts a drag:
            //
            // For a shortcut drop it removes whatever shortcut is in slot `n`, adds the dragged
            // item there, and — if a different object was displaced and the slot the drag came
            // from is available — puts the displaced object back in that slot, all with server
            // notification.
            //
            // A plain add, **not** a create-shortcut-to-item: the object was already in a slot a
            // moment ago, so its eligibility and ownership/backpack gates have already been
            // passed once and the client does not re-run them.
            //
            // Slot availability is three tests and nothing else — `n` is non-negative, below the
            // slot count, and its item list is empty —
            // i.e. "that slot number exists and its list is empty". The slot the drag left is
            // empty by construction (the pick-up removed it), *unless* the drop landed back on the
            // slot it came from, in which case the displaced id is the dragged item and the
            // displacement test does not run at all.
            DropTarget::ShortcutAlias { slot, from } => {
                let Ok(slot) = usize::try_from(slot) else {
                    self.stats.requests_refused += 1;
                    return;
                };
                let displaced = self.remove_shortcut_in_slot_num(slot, game, req);
                if !self.add_shortcut(item, slot, game, req, now) {
                    self.stats.requests_refused += 1;
                    return;
                }
                if let Some(old) = displaced.filter(|old| *old != item) {
                    // Is the slot the drag came from available?
                    let back = usize::try_from(from).ok().filter(|n| {
                        *n < dereth_client_model::player::SHORTCUT_SLOTS
                            && game.player_system.shortcut_at(*n).is_none()
                    });
                    if let Some(n) = back {
                        self.add_shortcut(old, n, game, req, now);
                    }
                }
                return;
            }
            // This unresolved item-list form names a list element, not its container
            // object. That mapping belongs to the inventory panel and is unavailable here.
            // Drop and count the request rather than guess a destination.
            DropTarget::ItemList { .. } => {
                self.stats.requests_refused += 1;
                return;
            }
            // The item-list slot-drop arm
            // decides *where in the pack* a dragged item lands. The screen has resolved the four
            // element-tree facts (the parent container id, the object under the pointer, the slot's
            // index and the list's item count); everything left needs the object table and is
            // `dereth_client_model`'s.
            //
            // `false` is `accept_drag_object`'s own refusal, and `handle_drop_release`
            // answers it with `set_waiting_state(0)` — the same un-ghost the paper-doll arm above
            // does, and the reason a refused drop does not leave the icon greyed for the rest of
            // the session.
            DropTarget::ItemListSlot {
                container,
                under,
                index,
                num_ui_items,
                dragged_is_container,
                container_list,
            } => {
                let drop = dereth_client_model::inventory::ItemListDrop {
                    parent_container: container,
                    under,
                    index,
                    num_ui_items,
                    dragged_is_container,
                    container_list,
                };
                // No `ui_requests_handled += 1` here: the `UiRequest::DragDrop` arm that called
                // this function falls into the loop's own shared increment, and counting it twice
                // would break `drag.rs`'s "interaction.rs consumed the drop" assertion.
                if !game.item_list_accept_drag(req, out, item, drop, game.split, now) {
                    self.stats.requests_refused += 1;
                    game.set_waiting_state(item, false);
                }
                return;
            }
        };
        if r.is_err() {
            self.stats.requests_refused += 1;
        }
    }
}
