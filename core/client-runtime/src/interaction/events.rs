//! Inventory and session event application.

use super::*;

/// The `Communication_ChannelList` / `ChannelIndex` handlers — the part the two share, with
/// the header as the one parameter.
///
/// The header is written **before** the list is looked at and unconditionally, so an empty reply
/// still prints one line; the rows are `"   " + name + "\n"`,
/// each its own scroll write of type 0 to window 0.
///
/// Returns how many rows went out, which is `names.len()` — returned rather than assumed so the
/// caller's counter has a producer it did not compute itself.
fn channel_report(game: &mut dereth_client_model::World, header: &str, names: &[String]) -> u64 {
    game.scroll.add_feedback_to_scroll(
        header,
        dereth_client_model::chat::text_type::DEFAULT,
        true,
        0,
        dereth_client_contract::feedback::Feedback::LOCAL,
    );
    for n in names {
        game.scroll.add_feedback_to_scroll(
            &format!("   {n}\n"),
            dereth_client_model::chat::text_type::DEFAULT,
            true,
            0,
            dereth_client_contract::feedback::Feedback::LOCAL,
        );
    }
    u64::try_from(names.len()).unwrap_or(u64::MAX)
}

/// App's one-message source boundary. UI0024 delivers its Remove callbacks before clearing
/// being_removed; standalone callers can retain a collected NoticeSink with `None` geometry.
pub fn apply_events_at_boundary(
    inter: &mut Interaction,
    events: &[dereth_client_net::client_session::SessionEvent],
    game: &mut dereth_client_model::World,
    geometry: Option<(&crate::selection_geometry::SceneSelectionPhysics, f32)>,
    panels: &mut dyn FnMut(&mut Interaction, &mut dereth_client_model::World, &Notice),
) {
    use dereth_protocol::{Message, Opcode};

    let mut out = Notices::default();
    // `handle_attack_done`'s auto-repeat re-fire *sends*, so the sink is not discarded at the
    // call to `absorb`.
    let mut req = RecordingRequests::default();
    // **The one arm in this function that is not a `UiEvent`.**
    //
    // Player-description handling calls `initialize_player` only while the player has not yet
    // been initialized. That operation finishes by sending `0x021E`, the request that makes the
    // House tab reachable at all.
    // It lands here rather than in `Hud::apply_events` because this is the function that has a
    // `RequestSink`; the model's `player_initialized` flag is the once-per-session guard, and
    // `ObjectStream::reset` clears with the rest of the world on `WorldReset`.
    for e in events {
        if matches!(
            e,
            dereth_client_net::client_session::SessionEvent::PlayerDescription(_)
        ) {
            game.initialize_player(&mut req);
        }
    }
    for e in events {
        let Some((opcode, body)) = e.ui_body() else {
            continue;
        };
        let mut r = dereth_protocol::archive::Reader::new(body);
        // Server-directed item movement raises a notice that drives
        // the player's unblock retry. All three message arms below use the same apply
        // operation and therefore the same notice listener. Record it here and act
        // after the match, rather than attaching another consumer only to `0x019A`.
        let mut moved: Option<(ObjectId, ObjectId)> = None;
        match opcode {
            Opcode::ITEM_SERVER_SAYS_CONTAIN_ID => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysContainId::read(&mut r) else {
                    continue;
                };
                // **The arm's second branch.** The client's `0x22` handling looks
                // the item up first and only moves an item it
                // already has; when it has none it pre-places the id in the container instead, so
                // that the `0xF745` still to come lands in the server's slot rather than at the
                // head of the pack. 29 of the corpus's 100 `0x0022` take this branch.
                if game.weenie(m.item).is_some() {
                    game.server_says_move_item(
                        m.item,
                        m.container,
                        m.slot,
                        ObjectId(0),
                        0,
                        true,
                        &mut out,
                    );
                } else {
                    if game.server_says_contain_id(
                        m.container,
                        m.item,
                        m.slot,
                        m.container_properties,
                    ) {
                        inter.stats.contain_ids_preplaced += 1;
                    }
                    // The server-says-move-item callback runs on the way out of this branch —
                    // unconditionally, and with every
                    // "old" field zero, because there is no object to read an old container off.
                    // The known-item branch `return`s before it and fires the same notice from
                    // inside the server-says-move-item handler.
                    out.emit(Notice::ItemMoved {
                        object: m.item,
                        old_container: ObjectId(0),
                        old_wielder: ObjectId(0),
                        old_location: 0,
                        container: m.container,
                        place: m.slot,
                        wielder: ObjectId(0),
                        location: 0,
                    });
                }
                // Counted for both branches: this is "the arm ran", and the
                // branch it took is `contain_ids_preplaced`.
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, m.container));
            }
            Opcode::ITEM_SERVER_SAYS_REMOVE => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysRemove::read(&mut r) else {
                    continue;
                };
                let now = inter.last_use_time;
                if let Some((geometry, radius)) = geometry {
                    game.server_says_remove_with_dispatch(
                        m.object,
                        ServerTime(now.0),
                        &mut |game, notice| {
                            inter.dispatch_object_notice(
                                game,
                                notice.clone(),
                                geometry,
                                radius,
                                now,
                            );
                            panels(inter, game, &notice);
                        },
                    );
                } else {
                    game.server_says_remove(m.object, ServerTime(now.0), &mut out);
                    moved = Some((m.object, ObjectId(0)));
                }
            }
            Opcode::ITEM_WEAR_ITEM => {
                let Ok(m) = dereth_protocol::objects::ItemWearItem::read(&mut r) else {
                    continue;
                };
                let player = game.player.unwrap_or_default();
                game.server_says_move_item(m.item, ObjectId(0), 0, player, m.slot, true, &mut out);
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, ObjectId(0)));
            }
            Opcode::ITEM_SERVER_SAYS_MOVE_ITEM => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysMoveItem::read(&mut r) else {
                    continue;
                };
                game.server_says_move_item(m.item, ObjectId(0), 0, ObjectId(0), 0, true, &mut out);
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, ObjectId(0)));
            }
            // **The source stack's half of a split.**
            //
            // The network-blob dispatcher's `0x197` case is the fourth inventory arm. Its body is
            // the sequence byte, the
            // (unaligned) object id, the amount and the value, handed to object maintenance.
            //
            // A drag-split's answer is three messages — `0xF745` create, `0x0022` for the **new**
            // object and `0x0197` for the **source** — and this is the only one that carries the
            // source stack's new count. Without it the source would keep its old count, keep
            // `set_waiting_state(1)`'s ghost, and — because the lock names the source while the
            // `0x0022` names the new object — hold the global inventory lock for the rest of the
            // session, so every later move would be refused with *"You can only move or use one
            // item at a time"*.
            //
            // The handler also releases the lock. The *optimistic* half, the local
            // insert the item list makes at the destination before any of this
            // arrives, is not done here.
            Opcode::ITEM_UPDATE_STACK_SIZE => {
                let Ok(m) = dereth_protocol::items::ItemUpdateStackSize::read(&mut r) else {
                    continue;
                };
                if game.server_says_set_stack_size(
                    m.item,
                    m.sequence,
                    m.amount,
                    m.new_value,
                    &mut out,
                ) {
                    inter.stats.stack_sizes_applied += 1;
                }
            }
            // **The id substitution.**
            //
            // The client's `0x00A0` arm does **not** run the handler on the object the
            // message names. It reads the message's id, then overwrites it with the previous
            // request's object id whenever the lock is held, and only then looks the object up:
            //
            // Retail retains the message's reason but replaces its object id with
            // the previous-request object id whenever that lock is nonzero. It then looks up the
            // chosen object and return without invoking the failure handler when the lookup fails.
            //
            // Three consequences:
            //
            // 1. **The handler's `this` is the substituted weenie**, so its `waiting` clear
            //    (a store of 0), the object **name** in the refusal line
            //    (the wide object-name lookup on that object) and the guard on the lock
            //    (comparing the object's id with the lock) all name the *locked* object.
            // 2. **The notice carries the substituted id too** — the handler's own object id
            //    is what the only producer of the attempt-failed notice passes.
            //    So `unblock_on_attempt_failed` must be asked about the substituted id as well;
            //    asking it about `m.object` compares the blocker against the wrong thing.
            // 3. **An unknown object runs nothing**: the null-object branch skips the call, so no
            //    text, no notice, and *the lock is not released*. This is not a tidy-up — it is
            //    the arm that decides whether a `0x00A0` for a stale id can wedge the inventory,
            //    and a recorded one names `object = 0` (a long solo session, `t = 812.106`).
            //
            // Retail's guard inside the attempt-failed handler can therefore never fail — the
            // function has exactly one caller and is never called indirectly — which is why
            // `RequestLock::clear` is unconditional here. That equivalence
            // holds only for the **lock**, not for the object the
            // handler runs on; tests pin the bytes and drive this arm through them.
            Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED => {
                let Ok(m) =
                    dereth_protocol::objects::CharacterServerSaysAttemptFailed::read(&mut r)
                else {
                    continue;
                };
                // A nonzero request-lock id replaces the message's object id.
                let object = game.request_lock.substitute(m.object);
                // The native null-object branch leaves the failure otherwise untouched.
                if game.weenie(object).is_none() {
                    inter.stats.attempts_failed_unknown_object += 1;
                } else {
                    game.server_says_attempt_failed(object, m.reason, &mut out);
                    inter.stats.attempts_failed += 1;
                    // The attempt-failed notice: the blocker's move was
                    // refused, so the item the player dropped un-ghosts and the state is dropped.
                    // Its notice is raised from inside the handler above, on the handler's own
                    // object -- the substituted one.
                    if game.unblock_on_attempt_failed(object) {
                        inter.stats.unblocks_abandoned += 1;
                    }
                }
                // The arm's second half, whether or not the object was known: every reason
                // but the seven whose object line already says it all is also handled as a
                // failure event, so a wield the shard refuses for the player's heritage
                // (`0x585`) says why as well as what -- "You are restricted to clothes and
                // armor created for your race." beside "The Academy Coat can't be wielded".
                // Window 0, as every failure-event line is.
                if !dereth_protocol::objects::CharacterServerSaysAttemptFailed::suppresses_generic_text(
                    m.reason,
                ) {
                    if let Some(c) =
                        dereth_client_contract::chat::failure::handle_failure_event(m.reason, "")
                    {
                        game.scroll.add_feedback_to_scroll(
                            dereth_client_model::chat::composition::add_text_to_scroll_trim(&c.body),
                            u32::from(c.ty),
                            true,
                            0, c.feedback);
                    }
                }
            }

            // -------------------------------------------------------------------------------
            // **Inbound arms that call the model's handlers.**
            //
            // Everything below is decoded by `dereth_protocol`, ordered by
            // `dereth_client_net::client_session` and delivered to this function; without an arm
            // here each would be dropped. Each one calls the model's implementation; where an
            // implementation had to be written the doc comment says so.
            // -------------------------------------------------------------------------------

            // The reply to `0x0195` or a container double-click fills that container's contents.
            // The model's contents operation is the only writer of that list for anything but
            // the **player's own** pack (which `0x0013`'s `content_profiles` fills), so without it
            // no chest, corpse or side pack could fill: `InventoryPanel`'s
            // `open_container` reads `GameView::container_contents`, which reads exactly the list
            // this writes.
            //
            // **This arm is the complete contents response, not only the contents list.** The
            // response handler's tail is what
            // *opens the window*, and it is the only place the opening
            // form of the set-ground-object notice is ever raised:
            //
            // when the container is the requested ground object, and is not the object of a
            // pending pick-up request, the ground-object notice is raised for it.
            //
            // Without it the client would ask the server for a corpse's contents, receive them,
            // and never show anything.
            Opcode::ITEM_ON_VIEW_CONTENTS => {
                let Ok(m) = dereth_protocol::objects::ItemOnViewContents::read(&mut r) else {
                    continue;
                };
                if game.on_view_contents(
                    m.container,
                    &m.contents,
                    &mut out,
                    ServerTime(inter.last_use_time.0),
                ) {
                    inter.stats.ground_panels_opened += 1;
                }
                inter.stats.contents_viewed += 1;
            }
            // The stop-viewing arm has two ordered effects. It first closes the ground container
            // when the server is the one ending the view,
            // with the notify-server argument `false` so that no `0x0195` is sent back at it.
            // It then clears the model's viewed-container contents.
            Opcode::ITEM_STOP_VIEWING_OBJECT_CONTENTS => {
                let Ok(m) = dereth_protocol::objects::ItemStopViewingObjectContents::read(&mut r)
                else {
                    continue;
                };
                if game.handle_stop_viewing_object_contents(
                    &mut req,
                    &mut out,
                    m.object,
                    ServerTime(inter.last_use_time.0),
                ) {
                    inter.stats.ground_panels_closed += 1;
                }
                inter.stats.contents_closed += 1;
            }
            // ---------------------------------------------------------------------------------
            // **The network dispatcher's `0x00B4` and `0x00B8` arms** — reading scrolls, letters
            // and signs. `dereth_protocol` decodes `Writing_BookOpen`, the corpus carries one (a
            // long solo session, `t = 611.154`), and `dereth_client_net::client_session` puts it on
            // the UI queue with `recv_queue: UiQueue`; without these arms nothing would be
            // displayed. See `dereth_client_model::book`.
            //
            // The client's `0xb4` arm is worth transcribing because the **second dword is not part
            // of the page list**, which a plainer reading of "book id, pages, inscription" loses:
            //
            // The `0x00B4` arm reads the book id and a separate maximum-page count before
            // unpacking the page list, inscription, scribe id and scribe name. It performs an
            // object lookup whose result is discarded, then raises the open-book notice with all
            // six decoded values in that order.
            //
            // **The object lookup is made and its answer thrown away** -- unlike `0x00A0`
            // just above, where the null test decides whether anything runs at all. So a `0x00B4`
            // for an object this client has never seen still opens the panel, and this arm has no
            // weenie guard for the same reason.
            Opcode::WRITING_BOOK_OPEN => {
                let Ok(m) = dereth_protocol::trade::WritingBookOpen::read(&mut r) else {
                    continue;
                };
                game.open_book(
                    m.book_id,
                    m.max_num_pages,
                    m.pages,
                    m.inscription,
                    m.scribe_id,
                    m.scribe_name,
                    &mut out,
                    ServerTime(inter.last_use_time.0),
                );
                inter.stats.books_opened += 1;
            }
            // The page-data response answers the
            // page-data request a page with no text included provokes. Wired with `0xb4`
            // because a book whose pages arrive empty is otherwise a window of blank pages, and
            // the two are one feature.
            Opcode::WRITING_BOOK_PAGE_DATA_RESPONSE => {
                let Ok(m) = dereth_protocol::trade::BookPageDataResponse::read(&mut r) else {
                    continue;
                };
                if game.book_page_data_response(m.object_id, m.page, m.data) {
                    inter.stats.book_pages_filled += 1;
                }
            }
            // **`0x00B6 Writing_BookAddPageResponse`, the third of the book's four.**
            //
            // The network dispatcher's `0x00B6` arm reads three consecutive dwords:
            // book id, page and success. Its notice reaches the book panel's add-page
            // response handler, the sole implementation among the 82 notice recipients.
            //
            // The handler is split across the seam — the two id refusals here,
            // the current-page check and the page-list insert in `dereth_ui_screens::panels::book`.
            Opcode::WRITING_BOOK_ADD_PAGE_RESPONSE => {
                let Ok(m) = dereth_protocol::trade::WritingBookAddPageResponse::read(&mut r) else {
                    continue;
                };
                inter.stats.book_add_page_responses += 1;
                if game.book_add_page_response(m.book_id, m.page_number, m.success != 0) {
                    inter.stats.book_add_pages_relayed += 1;
                }
            }
            // **`0x00B7 Writing_BookDeletePageResponse`: a counter, and the counter is
            // the transcription.**
            //
            // The `0x00B7` response carries the same three dwords as `0x00B6`, but its
            // notice has an empty implementation in **all 82** recipients. Retail receives
            // it without changing the page list; deleting a page here would invent behavior.
            //
            // The neighboring add-page notice is the positive control: it is empty in 81
            // recipients and reaches the book panel's real handler in the 82nd.
            Opcode::WRITING_BOOK_DELETE_PAGE_RESPONSE => {
                game.book_delete_page_response();
                inter.stats.book_delete_page_responses += 1;
            }

            // The answer to every `0x00C8 Item_Appraise`
            // this client sends, through the appraise handler and its highlight table.
            Opcode::ITEM_SET_APPRAISE_INFO => {
                let Ok(m) = dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut r) else {
                    continue;
                };
                game.set_appraise_info(m.object, m.profile, &mut out);
                inter.stats.appraisals_applied += 1;
            }
            // **`0x01CB Item_AppraiseDone`, and the answer to *"what retail does after
            // the appraisal"* is: nothing.**
            //
            // The dispatcher validates the opcode, passes the body to a three-byte function that
            // returns zero, and performs no other work. The same folded function backs the
            // fellowship-update-done no-op; neither path has a substantive handler.
            //
            // So the arm is a counter, and that **is** the transcription: the appraisal itself is
            // `0x00C9 Item_SetAppraiseInfo`'s (the arm just above). `0x01CB` is the server saying
            // "that was the last one", and the client's
            // examination pane is already correct when it lands — exactly `0x01C9`'s shape.
            Opcode::ITEM_APPRAISE_DONE => {
                inter.stats.appraise_done += 1;
            }
            // **`0x00C3 Item_GetInscriptionResponse`: a *dead handler*, which is not
            // the same thing as a folded one.**
            //
            // The arm contains real code rather than a folded empty function: it constructs
            // three strings, skips four bytes before each of the first two strings, unpacks all
            // three, destroys them and returns zero.
            //
            // No notice, no handler call, nothing stored. It differs from the community catalogue
            // for this reason. The inscription box is fed by `0x00C8`/`0x00C9`, and this
            // message — which
            // the community catalogue calls `Communication_HearRangedSpeech`-family — feeds it
            // nothing in retail. An arm that wrote an inscription from it would be this client
            // inventing behaviour.
            //
            // The body **is** decoded, because retail unpacks it: a blob that does not decode is
            // a different event from one that does, and `dereth_protocol::items` reproduces the two
            // four-byte skips deliberately.
            Opcode::ITEM_GET_INSCRIPTION_RESPONSE => {
                let Ok(_m) = dereth_protocol::items::ItemGetInscriptionResponse::read(&mut r)
                else {
                    continue;
                };
                inter.stats.inscription_responses += 1;
            }
            // **`0xF630 Character_SetPlayerVisualDesc`, the third stub and the only
            // one of these sixteen that is not a game event.**
            //
            // It arrives on the UI queue as a bare message rather than a game event. The
            // dispatcher unpacks one narrow string and passes its buffer to a three-byte
            // return-only receiver. The call is direct, and every relevant notice slot except the
            // persistent-data object's unrelated implementation resolves to that same no-op.
            //
            // The player's appearance arrives through the already-wired `0xF625`
            // visual-description update carrying `ObjDesc`. `0xF630` instead carries a
            // string that the original client discards; ACE never sends it.
            Opcode::CHARACTER_SET_PLAYER_VISUAL_DESC => {
                let Ok(_m) = dereth_protocol::login::PlayerAppearanceMessage::read(&mut r) else {
                    continue;
                };
                inter.stats.player_visual_descs += 1;
            }

            // Commence-attack handling is the server's acknowledgement that the swing began.
            Opcode::COMBAT_HANDLE_COMMENCE_ATTACK_EVENT => {
                game.handle_commence_attack();
                inter.stats.attacks_commenced += 1;
            }
            // The attack-done handler, given `result`. The requests this can raise — the
            // auto-repeat re-fire — go out through `absorb` below, which is the same wire slot
            // every other request in this file uses.
            Opcode::COMBAT_HANDLE_ATTACK_DONE_EVENT => {
                let Ok(m) = dereth_protocol::combat::CombatHandleAttackDoneEvent::read(&mut r)
                else {
                    continue;
                };
                // Every arm of the attack-done handler reaches a
                // call that passes 1, so this is the attack flavour. Computed here rather than
                // taken as a parameter because `game` is in hand and the two inputs
                // (`player_motions_pending`, the bridged stance) are the previous frame's, which
                // is what `last_use_time` on the same line already is.
                let ready_for_attack = inter.ready_for_attack(game);
                game.handle_attack_done(&mut req, m.error, ready_for_attack, inter.last_use_time);
                inter.stats.attacks_done += 1;
            }
            // **The production writer of `CombatState::last_attacked_time`.** Defender
            // notification (`0x01B2`)
            // and the evasion-defender notification handler (`0x01B4`) share a
            // tail: stamp the clock, then auto-target if the option is on and nothing
            // is selected. See the spell-selection branch for both and for why this is recorded
            // here and run in [`use_time`].
            //
            // The chat half belongs to `hud.rs`. Both routes deliberately decode the
            // body: original UI dispatch decodes before calling a handler, so malformed
            // input must not reach this timestamp/auto-target tail. Counting a failed
            // decode here would stamp a clock that the original leaves unchanged.
            Opcode::COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT => {
                if dereth_protocol::combat::DefenderNotification::read(&mut r).is_ok() {
                    inter.pending_defender_notifications += 1;
                }
            }
            Opcode::COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT => {
                if dereth_protocol::combat::EvasionDefenderNotification::read(&mut r).is_ok() {
                    inter.pending_defender_notifications += 1;
                }
            }
            // `Combat_QueryHealthResponse` — the selected creature's health bar.
            Opcode::COMBAT_QUERY_HEALTH_RESPONSE => {
                let Ok(m) = dereth_protocol::combat::CombatQueryHealthResponse::read(&mut r) else {
                    continue;
                };
                if game.update_object_health(m.object, m.health) {
                    inter.stats.selection_meters_written += 1;
                }
                inter.stats.health_responses += 1;
            }
            // `Item_QueryItemManaResponse` — the selected item's mana bar.
            Opcode::ITEM_QUERY_ITEM_MANA_RESPONSE => {
                let Ok(m) = dereth_protocol::items::ItemQueryItemManaResponse::read(&mut r) else {
                    continue;
                };
                if game.update_item_mana(m.object, m.mana, m.success != 0) {
                    inter.stats.selection_meters_written += 1;
                }
                inter.stats.mana_responses += 1;
            }
            // `Item_UseDone` — the universal "action finished" acknowledgement,
            // which is what takes the busy cursor back off. The failure half of the same handler
            // is a chat line and lives in `hud.rs`, which owns the chat scroll.
            Opcode::ITEM_USE_DONE => {
                let Ok(m) = dereth_protocol::objects::ItemUseDone::read(&mut r) else {
                    continue;
                };
                game.use_done(m.failure_type);
                inter.stats.uses_done += 1;
            }

            // `Allegiance_AllegianceUpdate` — a full replace of the tree.
            Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE => {
                let Ok(m) = dereth_protocol::social::AllegianceUpdate::read(&mut r) else {
                    continue;
                };
                let n = game.handle_allegiance_update(&m.profile);
                inter.stats.allegiance_members = n;
                inter.stats.allegiance_updates += 1;
            }
            // **`0x0148` and `0x0149`, and the names are the other way round from
            // what they read like.**
            //
            // Both dispatchers decode a count-prefixed list of narrow strings. Their
            // handlers differ only in the heading:
            //
            // * `0x0148 ChannelList`: "The following characters are currently listening on the channel:"
            // * `0x0149 ChannelIndex`: "The following channels are available to you:"
            //
            // Thus ChannelList lists characters, while ChannelIndex lists channels. Each
            // handler widens and prints the heading, then prints every entry with three
            // leading spaces and a trailing newline on chat type 0.
            //
            // These replies feed chat output, not a channel-selector widget. An empty
            // list still prints its heading, proving the command ran.
            Opcode::COMMUNICATION_CHANNEL_LIST => {
                let Ok(m) = dereth_protocol::comms::CommunicationChannelListRecv::read(&mut r)
                else {
                    continue;
                };
                inter.stats.channel_lists += 1;
                inter.stats.channel_rows += channel_report(
                    game,
                    "The following characters are currently listening on the channel:\n",
                    &m.names,
                );
            }
            Opcode::COMMUNICATION_CHANNEL_INDEX => {
                let Ok(m) = dereth_protocol::comms::CommunicationChannelIndexRecv::read(&mut r)
                else {
                    continue;
                };
                inter.stats.channel_indices += 1;
                inter.stats.channel_rows += channel_report(
                    game,
                    "The following channels are available to you:\n",
                    &m.names,
                );
            }
            // **`0x01C3 Character_QueryAgeResponse`, `/age`'s answer.**
            //
            // The query-age dispatcher unpacks **two** narrow strings and hands them to a
            // two-branch formatter. It tests the target-name buffer length against 1. An empty
            // target formats the self-age sentence; a nonempty target formats the named-player
            // sentence. Both are written to chat type 0.
            //
            // **Retail's stored-length-is-1 test is an emptiness test, not a length-one test**: the
            // stored length counts the terminator, so `1` is the empty string. That is
            // what `dereth_protocol::admin`'s *"an empty target name means self"* already says, and it
            // is the whole of the two-arm split — the server sends the name back only for somebody
            // else's `/age`.
            Opcode::CHARACTER_QUERY_AGE_RESPONSE => {
                let Ok(m) = dereth_protocol::admin::CharacterQueryAgeResponse::read(&mut r) else {
                    continue;
                };
                let line = if m.target_name.is_empty() {
                    format!("You have played for {}.\n", m.age)
                } else {
                    format!("{} has played for {}.\n", m.target_name, m.age)
                };
                game.scroll.add_feedback_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    0,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                inter.stats.age_responses += 1;
            }
            // **The four portal-storm notices, `0x02C9`…`0x02CC`.**
            //
            // The portal-storm brewing and imminent dispatchers pass the dword after the opcode as
            // a **float**
            // to handlers taking one argument; storm and subsided tail-call handlers taking
            // none. `dereth_client_model::portal_storm` carries the four bodies and the three
            // differences between them; this arm is the routing and the counters.
            //
            // Note the guard the four dispatchers share and this router already satisfies:
            // they compare the UI-system pointer and then the opcode.
            // A blob whose first dword is not the expected opcode returns 0 without
            // touching anything, which is `dereth_client_net::client_session`'s dispatch here.
            Opcode::MISC_PORTAL_STORM_BREWING => {
                let Ok(m) = dereth_protocol::trade::MiscPortalStormBrewing::read(&mut r) else {
                    continue;
                };
                game.portal_storm_brewing(m.extent);
                inter.stats.portal_storms_brewing += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM_IMMINENT => {
                let Ok(m) = dereth_protocol::trade::MiscPortalStormImminent::read(&mut r) else {
                    continue;
                };
                game.portal_storm_imminent(m.extent);
                inter.stats.portal_storms_imminent += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM => {
                game.portal_storm_struck();
                inter.stats.portal_storms_struck += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM_SUBSIDED => {
                game.portal_storm_subsided();
                inter.stats.portal_storms_subsided += 1;
                inter.stats.portal_storm_levels += 1;
            }
            // **`0x027C Allegiance_AllegianceInfoResponseEvent` is chat output, not the
            // allegiance panel.**
            //
            // The dispatcher reads a target id and complete allegiance profile, then
            // prints the five response lines as `(text, 0, true, 0)`. This is the chat
            // answer to `/allegiance info`; it changes neither the panel nor the stored
            // allegiance hierarchy.
            Opcode::ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT => {
                let Ok(m) = dereth_protocol::social::AllegianceInfoResponse::read(&mut r) else {
                    continue;
                };
                inter.stats.allegiance_info_responses += 1;
                if game.allegiance_info_response(m.target, &m.profile) {
                    inter.stats.allegiance_info_reports += 1;
                }
            }
            // **`0x0003 Allegiance_AllegianceUpdateAborted`.**
            //
            // The update-aborted dispatcher calls the handler, which raises a notice whose only
            // receiver in retail is the allegiance panel: it updates only when visible, the
            // `u32` ignored.
            //
            // **Not to be confused with `0x01C8 AllegianceAllegianceUpdateDone`**, which this
            // client has no switch arm for at all — ACE sends it, and the recorded sessions carry
            // 77 of them. `0x0003`
            // is a different message that retail *does* handle.
            Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED => {
                let Ok(m) = dereth_protocol::social::AllegianceUpdateAborted::read(&mut r) else {
                    continue;
                };
                game.allegiance_update_aborted(m.failure_type);
                inter.stats.allegiance_updates_aborted += 1;
            }

            // ---- the confirmation seam, `0x0274` / `0x0276` --------------------
            //
            // The `0x0274` dispatcher decodes the type, context and text, then hands them to a
            // seven-arm `switch` on the **type** with no `default`:
            //
            // ```text
            // case 1: raise an allegiance-swear request
            // case 2: raise an alter-skill confirmation
            // case 3: raise an alter-attribute confirmation
            // case 4: raise a fellowship request
            // case 5: raise a craft-interaction confirmation
            // case 6: raise an augmentation confirmation
            // case 7: raise a general yes/no confirmation
            // ```
            //
            // Without this arm the question never appears, nothing is sent, and ACE's
            // thirty-second `ConfirmationManager` timeout aborts it.
            // The answer half of the link-status panel's round trip.
            //
            // Return-ping dispatch raises the ping notice. Its only handler computes the
            // round-trip time from the current time and the last request time. The body is empty,
            // so the *arrival* is the whole message and a counter is a faithful carrier;
            // `crate::net::ping_holder`'s own doc says why it is a count and not a timestamp.
            //
            // The three `fellowship-*` captures carry this opcode 0/0/0 times, and the reason
            // is that it is "an answer to a request the retail client
            // never made in these three sessions". Retail only pings while the link-status panel
            // is open, and it was not.
            Opcode::CHARACTER_RETURN_PING => {
                crate::net::ping_holder::returned();
                continue;
            }
            Opcode::CHARACTER_CONFIRMATION_REQUEST => {
                let Ok(m) = dereth_protocol::comms::CharacterConfirmationRequest::read(&mut r)
                else {
                    continue;
                };
                // `is_handled` is `1..=7` — the `switch`'s arms, and nothing else.
                if !dereth_protocol::comms::CharacterConfirmationRequest::is_handled(
                    m.confirmation_type,
                ) {
                    inter.stats.confirmations_unknown_type += 1;
                    continue;
                }
                // **Type 1.** The allegiance panel turns it into an
                // accept-swear confirmation — the question a **monarch or
                // patron** is asked when somebody swears to them. Dropped, nobody could
                // ever gain a vassal in this client: the question would never appear and ACE's
                // thirty-second `ConfirmationManager` timeout would abort it.
                //
                // It goes to its own queue rather than to `pending_server_confirmations`, because
                // the dialog is the *panel's* and has its own context slot
                // (the accept-swear context): a gameplay confirmation already on screen must not
                // suppress it, which sharing the gameplay confirmation context would do.
                //
                // Type 4 is the fellowship invitation handled separately below.
                if m.confirmation_type == 1 {
                    inter.pending_swear_requests.push((m.context_id, m.text));
                    inter.stats.confirmations_raised += 1;
                    continue;
                }
                // **Type 4.** A fellowship invitation goes to its own
                // queue, separate from gameplay and allegiance confirmations. Their dialog
                // contexts are independent, so one can remain open while another is shown.
                if m.confirmation_type == 4 {
                    inter
                        .pending_fellowship_requests
                        .push((m.context_id, m.text));
                    inter.stats.fellowship_requests_raised += 1;
                    continue;
                }
                // Both panels exist, so no type is dropped as having an absent panel; control
                // falls through to the ordinary server-confirmation path. The
                // `confirmations_for_absent_panels` field is kept and simply stays 0, which a
                // test asserts.
                inter.stats.confirmations_raised += 1;
                inter.pending_server_confirmations.push((
                    m.confirmation_type,
                    m.context_id,
                    m.text,
                ));
            }

            // Confirmation-done handling raises one abort notice carrying `(type, context)`. It
            // represents the server taking
            // the question back, which on this shard is `ConfirmationManager.EnqueueAbort` after
            // thirty seconds. **The client has no timer of its own on this path**; this message is
            // the only thing that takes an unanswered confirmation down.
            Opcode::CHARACTER_CONFIRMATION_DONE => {
                let Ok(m) = dereth_protocol::comms::CharacterConfirmationDone::read(&mut r) else {
                    continue;
                };
                inter
                    .pending_confirmation_aborts
                    .push((m.confirmation_type, m.context_id));
            }

            // Vendor-info handling opens the shop. This is the active vendor id's writer;
            // without it `toolbar::splitter`'s vendor arm and `use_object`'s two vendor guards
            // would be inert.
            Opcode::VENDOR_VENDOR_INFO => {
                let Ok(m) = dereth_protocol::trade::VendorInfo::read(&mut r) else {
                    continue;
                };
                let n = game.handle_vendor_info(
                    &m,
                    &mut out,
                    &mut req,
                    ServerTime(inter.last_use_time.0),
                );
                inter.stats.vendor_stock = n;
                inter.stats.vendor_opens += 1;
            }

            // ---- the ten server-to-client trade messages -------------------
            //
            // Without these handlers the trade state would be a field nothing writes, and this
            // client could open a negotiation and then see nothing at all.
            //
            // **The recorded corpus carries none of these.** A calibrated scan of all seven
            // captures over 994 server game events and 2,564 client actions finds 0 of every
            // trade opcode, against the vendor family's 8 + 5 + 1 in the same scan. Everything
            // below is exercised from synthesised messages until a capture contains the trade flow.
            Opcode::TRADE_REGISTER_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeRegisterTrade::read(&mut r) else {
                    continue;
                };
                if game.handle_register_trade(&m, &mut out, ServerTime(inter.last_use_time.0)) {
                    inter.stats.trade_registers += 1;
                }
            }
            Opcode::TRADE_OPEN_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeOpenTrade::read(&mut r) else {
                    continue;
                };
                game.handle_open_trade(m.source, &mut out);
                inter.stats.trade_opens += 1;
            }
            Opcode::TRADE_CLOSE_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeCloseTrade::read(&mut r) else {
                    continue;
                };
                game.handle_close_trade(m.reason, ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_closes += 1;
            }
            Opcode::TRADE_ADD_TO_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeAddToTradeRecv::read(&mut r) else {
                    continue;
                };
                if game.handle_add_to_trade(&m, &mut out) {
                    inter.stats.trade_rows_changed += 1;
                }
            }
            Opcode::TRADE_REMOVE_FROM_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeRemoveFromTrade::read(&mut r) else {
                    continue;
                };
                if game.handle_remove_from_trade(&m, ServerTime(inter.last_use_time.0), &mut out) {
                    inter.stats.trade_rows_changed += 1;
                }
            }
            Opcode::TRADE_ACCEPT_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeAcceptTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_accept_trade(m.source, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_DECLINE_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeDeclineTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_decline_trade(m.source, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_RESET_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeResetTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_reset_trade(m.source, ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_TRADE_FAILURE => {
                let Ok(m) = dereth_protocol::trade::TradeTradeFailure::read(&mut r) else {
                    continue;
                };
                game.handle_trade_failure(&m, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            // The anti-scam message. Its handler raises the notice and **clears nothing**; the
            // `0x0202`s the server sends alongside it do that.
            Opcode::TRADE_CLEAR_TRADE_ACCEPTANCE => {
                game.handle_clear_trade_acceptance(ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_flag_messages += 1;
            }

            // Update and removal both operate on the **player's** enchantment registry; the
            // client keeps exactly one such registry.
            // Both use the counted forms:
            // adding an enchantment to the list ends by updating the spell totals by +1 and
            // removing one by -1, so the helpful and harmful enchantment counts have a
            // production writer and the buff/debuff indicator is not stuck at 0/0.
            Opcode::MAGIC_UPDATE_ENCHANTMENT => {
                let Ok(m) = dereth_protocol::qualities::MagicUpdateEnchantment::read(&mut r) else {
                    continue;
                };
                let now = inter.last_use_time;
                if game.update_player_enchantment(&m.0, now) {
                    inter.stats.enchantments_updated += 1;
                }
                // **The receiver for live enchantment changes.** After updating the registry,
                // native handling tests the enchantment's
                // `VITAE` bit. Vitae raises `VitaeChanged`; every other enchantment raises
                // `EnchantmentsChanged`. The stat-management panel listens to the latter and
                // refreshes every skill row. Without the notice, a buff that landed **live** would
                // update the registry while the Skills page went on drawing the join it had built
                // at `0x0013`. The attribute rows need no notice because `HudView::attribute` asks
                // the attribute lookup on every read; the skills page is a cached join and needs
                // the notice.
                //
                // Keep the original branch distinct: a vitae enchantment raises the other
                // notice, to which the stat-management panel does not subscribe.
                let vitae =
                    m.0.smod.kind & dereth_protocol::types::qualities::enchantment_type::VITAE != 0;
                let notice = if vitae {
                    Notice::VitaeChanged
                } else {
                    Notice::EnchantmentsChanged
                };
                panels(inter, game, &notice);
            }
            // Removal (`0x02C3`) and dispel (`0x02C7`)
            // have identical wire bodies, as the protocol round-trip test confirms, and share one
            // storage in the world, which avoids duplicate updates and spell-total recomputation.
            //
            // **The expiry line.** The shared removal operation
            // notifies only when its Boolean argument is true: removal passes true, while
            // dispel passes false. Expiry therefore prints a line such as
            // "Strength Self VI has expired." and dispel remains silent.
            Opcode::MAGIC_REMOVE_ENCHANTMENT | Opcode::MAGIC_DISPEL_ENCHANTMENT => {
                let announce = opcode == Opcode::MAGIC_REMOVE_ENCHANTMENT;
                let id = if announce {
                    let Ok(m) = dereth_protocol::qualities::MagicRemoveEnchantment::read(&mut r)
                    else {
                        continue;
                    };
                    m.layered_spell_id
                } else {
                    let Ok(m) = dereth_protocol::qualities::MagicDispelEnchantment::read(&mut r)
                    else {
                        continue;
                    };
                    m.layered_spell_id
                };
                // The `Magic_RemoveEnchantment` handler's notice half,
                // which is an **either/or** and not a pair. Retail reads the current
                // vitae enchantment before removing the requested id. A matching vitae id raises
                // `VitaeChanged`; every other id raises `EnchantmentsChanged`.
                //
                // The test is read **before** the removal, because afterwards the vitae is gone.
                let was_vitae = game
                    .player_qualities()
                    .and_then(|q| q.enchantments.vitae)
                    .is_some_and(|v| v.id == id);
                if game.remove_player_enchantment(id) {
                    inter.stats.enchantments_removed += 1;
                }
                let notice = if was_vitae {
                    Notice::VitaeChanged
                } else {
                    Notice::EnchantmentsChanged
                };
                panels(inter, game, &notice);
                if announce && game.notify_of_enchantment_removal(id) {
                    inter.stats.enchantment_expiry_lines += 1;
                }
            }
            // **`0x02C4 Magic_UpdateMultipleEnchantments`, the count-prefixed form of
            // the arm above.**
            //
            // The dispatcher validates `0x02C4`, unpacks a count-prefixed enchantment list and
            // passes that list to the update handler.
            //
            // **The layout is `PackableList`'s and nothing more** — a `u32` count followed by that
            // many `Enchantment`s. There
            // is no per-entry header and no trailing word; `0x02C4` is `0x02C2`'s body with a count
            // in front of it, and `dereth_protocol::qualities::MagicUpdateMultipleEnchantments` reads
            // it that way.
            Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS => {
                let Ok(m) =
                    dereth_protocol::qualities::MagicUpdateMultipleEnchantments::read(&mut r)
                else {
                    continue;
                };
                let now = inter.last_use_time;
                let applied = game.update_player_enchantments(&m.0, now, &mut out);
                inter.stats.multi_enchantment_updates += 1;
                inter.stats.multi_enchantments_applied += applied as u64;
                // The handler raises `EnchantmentsChanged` once, after
                // the whole list, unconditional, and never the vitae one however many vitae
                // entries the list held. That asymmetry against the single arm above belongs to
                // the retail handler.
                panels(inter, game, &Notice::EnchantmentsChanged);
            }
            // **`0x02C5` and `0x02C8`, one handler and one `bool`.**
            //
            // Both dispatchers read a count-prefixed list of `u32` enchantment ids and
            // call the same removal operation. `0x02C5` supplies notify=true; `0x02C8`
            // supplies notify=false through the dispel path.
            //
            // The `0x02C3`/`0x02C7` pair above has exactly this shape one layer down, which is why
            // the two arms are written the same way: the *silence* of a dispel is the transcription
            // and folding the two opcodes into one arm would lose it.
            Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS
            | Opcode::MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS => {
                let announce = opcode == Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS;
                let ids = if announce {
                    let Ok(m) =
                        dereth_protocol::qualities::MagicRemoveMultipleEnchantments::read(&mut r)
                    else {
                        continue;
                    };
                    m.0
                } else {
                    let Ok(m) =
                        dereth_protocol::qualities::MagicDispelMultipleEnchantments::read(&mut r)
                    else {
                        continue;
                    };
                    m.0
                };
                let (removed, announced) =
                    game.remove_player_enchantments(&ids, announce, &mut out);
                if announce {
                    inter.stats.multi_enchantment_removals += 1;
                } else {
                    inter.stats.multi_enchantment_dispels += 1;
                }
                inter.stats.multi_enchantments_removed += removed as u64;
                inter.stats.enchantment_expiry_lines += announced as u64;
                // `EnchantmentsChanged` is unconditional and occurs *before*
                // the per-id notification loop; `VitaeChanged` is a second, separate raise on
                // this handler rather than an
                // alternative to it.
                panels(inter, game, &Notice::EnchantmentsChanged);
            }
            // **The two purges' only trigger.**
            // The `0x02C6` and `0x0312` dispatcher arms reach the normal and harmful-only purge
            // handlers. Their bodies perform the purge and then raise
            // the enchantments-changed and vitae-changed notices. **Both messages have
            // empty bodies**, so there is nothing to decode and no `dereth_protocol` type is needed.
            //
            // There is no client-side death hook: the client purges when the **server** says so,
            // and these two opcodes are the whole mechanism.
            Opcode::MAGIC_PURGE_ENCHANTMENTS => {
                if game.purge_enchantments(&mut out) {
                    inter.stats.enchantments_purged += 1;
                }
                // Both handlers raise **both** notices, unconditionally — this is
                // the one place in the family where they are a pair rather than a choice.
                panels(inter, game, &Notice::EnchantmentsChanged);
                panels(inter, game, &Notice::VitaeChanged);
            }
            Opcode::MAGIC_PURGE_BAD_ENCHANTMENTS => {
                if game.purge_bad_enchantments(&mut out) {
                    inter.stats.bad_enchantments_purged += 1;
                }
                panels(inter, game, &Notice::EnchantmentsChanged);
                panels(inter, game, &Notice::VitaeChanged);
            }
            // **Public world-object quality updates.** The twenty-six update
            // opcodes in `0x02CD`..=`0x02EA` include public and private forms. This route
            // accepts only decoded public forms, identified by a present subject id.
            //
            // Without this route the player-only HUD route would leave other objects without an
            // update consumer, and a chest could remain locked locally after the server unlocked
            // it: mirroring Boolean quality 3 updates the public description's openable flag
            // to the inverse value, which the local use guard reads.
            //
            // Private forms have no subject id and are handled by the player route. Public
            // forms naming the player can be offered to both routes, but the
            // world owns one shared player-quality store and sequence gate, so there is no
            // separate HUD quality copy to maintain.
            op if dereth_client_model::qualities::update::is_update_opcode(op) => {
                let Some(u) = dereth_client_model::qualities::update::decode(op, body) else {
                    continue;
                };
                let Some(subject) = u.subject else { continue };
                if game.apply_stat_update(subject, u.key, u.value, u.sequence, &mut out) {
                    inter.stats.object_quality_updates += 1;
                } else {
                    inter.stats.object_quality_updates_stale += 1;
                }
            }
            // **The `Remove` half of the arm above, and the same partition.**
            //
            // Stat removal looks up the wire's object id, not just the player. The eight
            // public forms therefore enter this route; the eight private forms, with no
            // subject id, use `Hud::apply_quality_remove`. This is the world-removal
            // caller.
            //
            // Removal deliberately does not run the `PublicWeenieDesc` mirror. Unlike
            // updates, none of the eight observed removal paths invokes that mirror, so
            // removing ItemType or Burden leaves its mirrored value unchanged.
            // `apply_stat_remove` preserves that omission.
            op if dereth_client_model::qualities::remove::is_remove_opcode(op) => {
                let Some(r) = dereth_client_model::qualities::remove::decode(op, body) else {
                    continue;
                };
                let Some(subject) = r.subject else { continue };
                if game.apply_stat_remove(subject, r.key, r.sequence, &mut out) {
                    inter.stats.object_quality_removes += 1;
                } else {
                    inter.stats.object_quality_removes_stale += 1;
                }
            }
            // **Range registration after a slumlord profile.** Hud retains the
            // profile for display. This router also decodes the message, then uses its
            // covenant-crystal id and the world range list's clock to register the watch.
            Opcode::HOUSE_HOUSE_PROFILE => {
                let Ok(m) = dereth_protocol::trade::HouseProfileMessage::read(&mut r) else {
                    continue;
                };
                game.register_slumlord_range_check(
                    m.covenant_crystal,
                    ServerTime(inter.last_use_time.0),
                );
            }
            // **`0x0004 Communication_PopUpString` tutorial message boxes.**
            //
            // The recorded logins include three across the fellowship recordings and 26
            // across seven session captures.
            //
            // The dispatcher decodes one narrow string, widens it as literal text and
            // builds a property collection. The properties are:
            //
            // * `0x8E = 3`: `DialogKind::Message`, the one-button message dialog,
            //   element type `0x17`, rather than a yes/no question.
            // * `0xC3 = 1`: `dialog::factory::NON_QUEUED`, the all-at-once list, whose production
            //   caller this message is. A recorded long solo session contains 18 such prompts; five
            //   arriving together must show five boxes rather than queue four.
            // * `0xC5`: the prompt text displayed by child `0x3E`
            //   (`dialog::base::child::TEXT`), shared with other dialog builders.
            //
            // No callback or modal property `0xAC` is installed, so these boxes do not
            // block world clicks. Vendor, fellowship and allegiance questions are also
            // nonmodal; gameplay confirmations differ.
            //
            // This function has a mutable world borrow but no UI system. It queues the
            // text for [`crate::target_confirmation::TargetedDialogs`], which owns the
            // dialog controller. The router provides that factory path; the neighboring
            // communication handlers in `Hud::ui_event` do not.
            //
            // Event `0x0318` carries the same one string and reaches the same pop-up.
            Opcode::COMMUNICATION_POP_UP_STRING | Opcode::COMMUNICATION_POP_UP_STRING_0318 => {
                let Ok(m) = dereth_protocol::comms::CommunicationPopUpString::read(&mut r) else {
                    inter.stats.pop_up_strings_undecodable += 1;
                    continue;
                };
                inter.pending_pop_up_strings.push(m.message);
                inter.stats.pop_up_strings += 1;
            }
            // The third wildcard the UI queue can land in. `Hud::ui_event` and this function both
            // see every `SessionEvent::UiEvent`, so an opcode is only *dropped* when it falls
            // through **both**; the ledger records per site and a test takes the intersection.
            _ => {
                crate::dropped::record(crate::dropped::Site::Interaction, opcode);
            }
        }
        // Once the blocking move completes, send the deferred wield owed by the paper
        // doll. Any required split uses this interaction's retained splitter state.
        if let Some((object, container)) = moved {
            inter.on_item_moved(game, object, container, &mut out, &mut req);
        }
        for notice in std::mem::take(&mut out.moved_for_panels) {
            panels(inter, game, &notice);
        }
    }
    game.reconcile_vendor_transaction();
    inter.absorb(game, out, req);
}
