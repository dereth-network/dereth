//! Mana-stone and salvage callback-dialog subscriber.
//! The current framework owns lifetime; the existing App delivery boundary owns callbacks.
use dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_ui::{dialog, PropertyCollection, PropertyValue};
use dereth_ui_screens::view::AllegianceAction;

const SOURCE: u32 = 0x1000_003d;
const TARGET: u32 = 0x1000_003e;

/// Which callback the client associates with this dialog context.
///
/// The factory takes the callback as a function pointer, so the identity of the
/// answer's consumer is per-context and not per-queue: mana-stone and salvage
/// prompts share a usage callback, while the vendor close prompt uses a different
/// callback on the same default queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Callback {
    /// Player-killer altar, non-player-killer altar, and volatile-rare confirmations all pass the
    /// shared usage callback.
    Usage,
    /// The mana-stone and salvage paths' usage callback.
    TargetedUse,
    /// The vendor-close callback.
    CloseVendor,
    /// `@house abandon`'s first question. Its Yes arm raises the second question
    /// and sends nothing at all.
    HouseAbandonFirst,
    /// The second house-abandon question's callback, reachable only from the
    /// first's Yes arm, and the only caller of the `0x021F` abandon-house event.
    HouseAbandonSecond,
    /// The gameplay panel's own confirmation slot, not a per-context
    /// callback.
    ///
    /// The five `0x0274` types that reach the gameplay panel are answered when
    /// the close-dialog notice compares the closing context with
    /// the gameplay confirmation context rather than by a
    /// per-context function pointer, and the pair it answers with is the one the request stored in
    /// stored confirmation type and server context — which is why both
    /// travel in the variant instead of being re-read off the dialog.
    ServerConfirmation {
        confirmation_type: i32,
        context_id: u32,
    },
    /// The allegiance panel's own three questions, and the *client's* own, not the
    /// server's: no `0x0274` asked them and no `0x0275` answers them.
    ///
    /// The swear, break, and kick confirmation builders each store their
    /// dialog context on the panel and refuse to build a second one
    /// in the corresponding independent slot
    /// while it is non-zero; close-dialog handling compares the closing context
    /// against all three and runs the matching close path. The context lives here
    /// rather than on the panel because the panel is never told a dialog closed — see
    /// `AllegiancePanel::confirmations`.
    ///
    /// The target travels in the variant for the server-confirmation reason: the kick prompt latches
    /// the possible target when the question *opens*, so a roster that moves while it is
    /// on screen must not change who gets kicked.
    Allegiance {
        action: AllegianceAction,
        target: ObjectId,
    },
    /// The confirmation a patron is asked when somebody swears to them.
    ///
    /// This one *is* the server's: its close path clears the local context,
    /// sends the answer with the stored server context, and then clears that server context.
    /// It uses the same
    /// `0x0275` [`Callback::ServerConfirmation`] sends, and with **no branch on the answer**, so a
    /// refusal is a `0x0275` with `accepted = 0` rather than silence.
    ///
    /// It is a separate variant from `ServerConfirmation` because retail keeps it in a separate
    /// context slot on the panel: a gameplay question already on screen must not suppress
    /// it, and the gameplay confirmation's single-slot guard would.
    AcceptSwear { context_id: u32 },
    /// The `0x0274` type-4 invitation and the panel's **own** slot.
    ///
    /// It is a separate variant rather than a `ServerConfirmation` with `confirmation_type: 4`
    /// because the client keeps the fellowship-request context in a different field from the
    /// gameplay-confirmation context: the "one at a time" guard is per-slot, so an invitation
    /// and an "are you sure" can be on screen together, and the abort rule differs (see below).
    FellowshipRequest { context_id: u32 },
    /// The only callback dialog a *chat command* opens in the whole client.
    ///
    /// The client's own question, not the server's: the `@die` command builds the collection,
    /// and the callback reads property `0x92` and calls the character's suicide event only when
    /// it is set. There is no context slot on a panel to mirror — the `@die` command keeps none
    /// at all, so a second `@die` while the first question is up queues a second identical
    /// question behind it on the default dialog queue, asked once the first is answered.
    Die,
    /// The mini-game quit-dialog callback.
    ///
    /// The callback reads Confirmation property `0x92` and raises
    /// the quit-game notice with `answer`. Its Yes reaches `0x026A Game_Quit`; `0x0269`
    /// is the separate Game_Join sent when the board opens.
    MiniGameQuit,
    /// Routing to the landscape-buy or rent-by-proxy context slot. Retail receives this through the panel's
    /// close-dialog notice rather than a function-pointer callback; this service owns the shared
    /// factory lifetime and routes the same answer back to that panel.
    HousePayment { rent: bool },
}

#[derive(Default)]
pub(crate) struct TargetedDialogs {
    generation: Option<u64>,
    contexts: Vec<(u64, Callback)>,
    /// The `0x0004 Communication_PopUpString` boxes currently on screen.
    ///
    /// A separate list from [`Self::contexts`] rather than a [`Callback`] variant, because a popup
    /// has **no callback at all**: retail opens it as a plain dialog, not a callback dialog,
    /// and the message dialog writes no answer
    /// property (`DialogKind::answer_property` is `None` for it). Putting it
    /// in `contexts` would send it through the answer-harvest loop at the top of
    /// [`Self::service`], which is written around a dialog that answers something, and it would
    /// never close: that loop's `if answer.is_none() && ui.node(root).is_some() { continue }`
    /// holds for ever on a kind that has no answer.
    ///
    /// There is also no "one at a time" guard, for the reason [`Self::service`]'s popup block
    /// gives: these live on dialog queue **1**, the all-at-once list.
    popups: Vec<u64>,
}

impl TargetedDialogs {
    pub(crate) fn service(
        &mut self,
        shell: Option<&mut crate::ui::UiShell>,
        interaction: &mut crate::interaction::Interaction,
        world: &mut dereth_client_model::World,
        now: LocalTime,
    ) {
        let usage = interaction.take_usage_confirmations();
        let notices = interaction.take_targeted_confirmations();
        let closes = interaction.take_vendor_close_confirmations();
        let house_payments = interaction.take_house_payment_confirmations();
        let requests = interaction.take_server_confirmations();
        let aborts = interaction.take_confirmation_aborts();
        let fellowship = interaction.take_fellowship_requests();
        let Some(shell) = shell else {
            if self
                .contexts
                .iter()
                .any(|(_, callback)| *callback == Callback::MiniGameQuit)
            {
                world.minigame.cancel_resign_dialog();
            }
            self.contexts.clear();
            self.popups.clear();
            self.generation = None;
            return; // No subscriber or retained notice history.
        };
        let ui = &mut shell.ui;
        if self.generation != Some(shell.flow.switches) {
            // UI flow already reset the dialog as done. Unanswered data has no property 0x92;
            // do not manufacture a No event or replay a stale Yes into a new generation.
            ui.dialogs
                .completed
                .retain(|i| !self.contexts.iter().any(|(c, _)| *c == i.context));
            if self
                .contexts
                .iter()
                .any(|(_, callback)| *callback == Callback::MiniGameQuit)
            {
                world.minigame.cancel_resign_dialog();
            }
            self.contexts.clear();
            // A framework teardown flushes the non-queued dialog list with everything else, so the popups went with the framework and their contexts are stale.
            self.popups.clear();
            self.generation = Some(shell.flow.switches);
        }
        if shell.flow.current_mode() != Some(dereth_ui::framework::mode::GAME_PLAY)
            || shell.flow.current().is_none()
        {
            return;
        }
        // **The popup-string message handler, the modal every login opens.** The three
        // properties are decoded in
        // the `0x0004` arm in `crate::interaction`; what matters here is what makes this block
        // unlike every other one in this file:
        //
        // * **`0xC3 = (enum)1`** — dialog queue 1, `dialog::factory::NON_QUEUED`, the
        //   all-at-once list. `long-solo-play` carries **eighteen** `0x0004`s, so
        //   the difference is eighteen boxes shown against one box and seventeen waiting.
        // * **`0x8E = (enum)3`** — `DialogKind::Message`, the one-button box. Every other dialog
        //   in this file is `(enum)1`, a Yes/No.
        // * **no `note_callback`** — retail opens a plain dialog, not the callback variant, so
        //   there is nothing to run when it closes.
        // * **no one-at-a-time guard** — there is no context slot for it to occupy.
        //
        // The queue is drained **after** the mode gate on purpose: a `0x0004` that arrives while
        // the shell is still coming up stays in `Interaction` rather than being taken and thrown
        // away, which is what taking it beside `notices`/`requests` above would do. On a typical
        // shard the first one arrives within a second of entering the world.
        for text in interaction.take_pop_up_strings() {
            let mut data = PropertyCollection::new();
            data.set(dereth_ui::props::attr::DIALOG_KIND, PropertyValue::Enum(3));
            data.set(
                dereth_ui::props::attr::DIALOG_QUEUE_ID,
                PropertyValue::Enum(u32::try_from(dialog::factory::NON_QUEUED).unwrap_or(1)),
            );
            data.set(
                dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
                PropertyValue::String(text),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                self.popups.push(context);
                interaction.stats.pop_ups_shown += 1;
            }
        }
        // The message dialog closes its stored context when element message 0x26
        // carries code 1, then stops processing: no answer property, no callback, only the close.
        // `DialogElement::answer` being set *is* that element message having arrived, so this is
        // the same read the answer-harvest loop below makes, minus the harvest.
        for context in self.popups.clone() {
            let Some(info) = ui.dialogs.info(context) else {
                self.popups.retain(|c| *c != context);
                continue;
            };
            let Some(root) = info.element else { continue }; // still owed an element
            if !dialog::types::dialog_element(ui, root).is_some_and(|d| d.answer.is_some()) {
                continue;
            }
            let Some((queue, info)) = ui.dialogs.begin_close_dialog(context) else {
                continue;
            };
            ui.send_notice(
                dereth_ui::NoticeId::DialogClosed,
                &dereth_ui::NoticePayload {
                    a: u32::try_from(context).expect("32-bit dialog context"),
                    ..Default::default()
                },
            );
            ui.remove_and_delete_root(root);
            ui.dialogs.finish_close_dialog(queue, info, now.0);
            ui.dialogs.completed.retain(|i| i.context != context);
            self.popups.retain(|c| *c != context);
            interaction.stats.pop_ups_dismissed += 1;
        }
        // Only our exact roots: unrelated logout/character buttons also use ids17/19.
        for (context, callback) in self.contexts.clone() {
            let Some(info) = ui.dialogs.info(context) else {
                continue;
            }; // still queued
            let Some(root) = info.element else { continue };
            let answer = dialog::types::dialog_element(ui, root).and_then(|d| d.answer_property());
            if answer.is_none() && ui.node(root).is_some() {
                continue;
            }
            if let Some((key, value)) = answer {
                ui.dialogs.set_answer_property(context, key, value);
            }
            let (queue, info) = ui
                .dialogs
                .begin_close_dialog(context)
                .expect("open context");
            let data = &info.data;
            let answered = data.get_bool(0x92);
            if ui.dialogs.has_callback(context) {
                match callback {
                    // The usage callback reads property 0x92 first and does nothing on
                    // No. Its Yes reads the retained instance id from 0x1000003D, then performs
                    // the same use/busy/item-in-use sequence as the direct useable path.
                    Callback::Usage if answered == Some(true) => {
                        interaction.confirm_usage(world, iid(data, SOURCE), ServerTime(now.0))
                    }
                    // The two callback-dialog callbacks both return early when
                    // property `0x92` is absent and act only when it is true, so a No is nothing
                    // at all for either of them.
                    Callback::TargetedUse if answered == Some(true) => interaction
                        .confirm_targeted_usage(
                            world,
                            iid(data, SOURCE),
                            iid(data, TARGET),
                            ServerTime(now.0),
                        ),
                    // The vendor-close callback's only statement.
                    Callback::CloseVendor if answered == Some(true) => {
                        interaction.confirm_vendor_close(world);
                    }
                    // Closing a gameplay confirmation has **no branch on the answer**: it sends
                    // whatever byte property `0x92` held unconditionally. A refusal is a `0x0275`
                    // with `accepted = 0`, so the server learns of it at once instead of waiting
                    // out its confirmation timeout.
                    //
                    // `answered` is still required to be *present*: a dialog that died with its
                    // framework generation rather than under a button has no `0x92`, and the
                    // retained-collection guard above applies to it for the same reason it applies
                    // to the other two. The server's own abort is handled explicitly below, where
                    // retail's reentrant close-dialog notice is.
                    Callback::ServerConfirmation {
                        confirmation_type,
                        context_id,
                    } => {
                        if let Some(accepted) = answered {
                            interaction.confirm_server_confirmation(
                                confirmation_type,
                                context_id,
                                accepted,
                            );
                        }
                    }
                    // The swear, break and kick confirmation closers, all three of
                    // which read the answer
                    // and send **only** on yes. There is no confirmation response on this
                    // path: the question is the client's own.
                    Callback::Allegiance { action, target } if answered == Some(true) => {
                        interaction.confirm_allegiance(world, action, target);
                    }
                    // The die-dialog callback reads property `0x92` and sends the
                    // suicide event only when it is true. A No is nothing at all.
                    Callback::Die if answered == Some(true) => interaction.confirm_die(world),
                    // The mini-game quit callback has no branch on the Boolean: when
                    // property 0x92 exists it raises the notice for both Yes and No. A missing
                    // property means the current UI died rather than an answer being chosen; the
                    // replacement panel starts with a zero context and sends no notice.
                    Callback::MiniGameQuit => {
                        if let Some(confirmed) = answered {
                            ui.requests.emit(
                                dereth_ui_screens::view::UiRequest::MiniGameQuitAnswer(confirmed),
                            );
                        } else {
                            world.minigame.cancel_resign_dialog();
                        }
                    }
                    // The dialog-close handler checks 0x8E == Confirmation, then reads
                    // the Boolean answer from 0x92 and selects one of the panel's two contexts.
                    Callback::HousePayment { rent } => {
                        ui.requests.emit(
                            dereth_ui_screens::view::UiRequest::HousePaymentConfirmationAnswer {
                                rent,
                                confirmed: answered,
                            },
                        );
                    }
                    // **`@house abandon`'s two stages.**
                    //
                    // The first and second house-abandon callbacks are both
                    // the die-dialog callback's shape — read property `0x92` off the closing
                    // collection, act only when it is set — and they differ only in what the Yes
                    // arm does. The first raises the second question and **sends nothing**; the
                    // second is the only caller of the abandon-house event in retail. A
                    // No at either stage is silence, which is why neither arm has an `else`.
                    Callback::HouseAbandonFirst if answered == Some(true) => {
                        interaction.confirm_house_abandon_first();
                    }
                    Callback::HouseAbandonSecond if answered == Some(true) => {
                        interaction.confirm_house_abandon(world);
                    }
                    // The accept-swear confirmation close has no branch on the answer at all —
                    // it pushes the byte and calls the confirmation response — so a No is a
                    // `0x0275` with `accepted = 0`, and the server learns of the refusal at once
                    // instead of waiting out its confirmation timeout. `answered` still has
                    // to be *present*, for `ServerConfirmation`'s reason: a dialog that died with
                    // its framework generation carries no `0x92`.
                    Callback::AcceptSwear { context_id } => {
                        if let Some(accepted) = answered {
                            interaction.confirm_server_confirmation(1, context_id, accepted);
                        }
                    }
                    // The fellowship close handler first requires confirmation kind
                    // 1, treats an absent answer as No, checks that this is the active fellowship
                    // dialog, and then sends confirmation type 4 with the stored server context.
                    //
                    // No branch on the answer, for the same reason: a refusal is a `0x0275` with
                    // `accepted = 0`, so the server does not wait out its confirmation timeout.
                    Callback::FellowshipRequest { context_id } => {
                        if let Some(accepted) = answered {
                            interaction.confirm_server_confirmation(4, context_id, accepted);
                        }
                    }
                    Callback::Usage
                    | Callback::TargetedUse
                    | Callback::CloseVendor
                    | Callback::Allegiance { .. }
                    | Callback::Die
                    | Callback::HouseAbandonFirst
                    | Callback::HouseAbandonSecond => {}
                }
            }
            ui.dialogs.take_callback(context);
            // Dialog done: callback -> notice -> parent removal/delete; then open next.
            // The generic close's final-collection notice payload/recursive fanout is still
            // absent in UiSystem. This scoped callback reads the retained collection directly.
            ui.send_notice(
                dereth_ui::NoticeId::DialogClosed,
                &dereth_ui::NoticePayload {
                    a: u32::try_from(context).expect("32-bit dialog context"),
                    ..Default::default()
                },
            );
            ui.remove_and_delete_root(root);
            ui.dialogs.finish_close_dialog(queue, info, now.0);
            ui.dialogs.completed.retain(|i| i.context != context);
            self.contexts.retain(|(c, _)| *c != context);
        }
        // **Confirmation abort.** Return unless both the confirmation type
        // and server context match. Close the local dialog, then clear the type,
        // local dialog context, and server context.
        //
        // The dialog close runs while the context is **still set**, and
        // dialog closure reaches dialog completion, whose
        // close-dialog notice carrying the context and the dialog's data is
        // synchronous. So the abort re-enters the dialog-close handler, which reads property
        // `0x92` into a byte it pre-zeroed — absent on a dialog nobody answered —
        // and therefore closes the gameplay confirmation with a No. **The client
        // answers No on the way out.** A server that aborts one of these five types therefore
        // receives a response from the client; for other types it does not.
        for (confirmation_type, context_id) in aborts {
            // The type-1 arm: the allegiance abort has the same shape as the
            // gameplay abort: it closes the dialog while the accept-swear context is **still
            // set**, so the reentrant close runs the accept-swear close with `false` and the
            // client answers No on the way out
            // — identical to the five gameplay types, and what the loop body below already does.
            // The fellowship abort differs from the gameplay abort by one predicate:
            // it requires type 4 but never reads the supplied context. It closes the stored local
            // dialog and clears both fellowship slots. The gameplay path compares both type and
            // context, while this path compares the type alone, so an abort carrying **any** context takes the open
            // fellowship invitation down. The dialog close runs while the slot is still set, so
            // it re-enters the dialog-close handler with no `0x92` present and the client answers
            // **No** on the way out — which is the same tail the gameplay dialog has, reached the
            // same way, and is why `confirm_server_confirmation(.., false)` below is right for
            // both.
            // Types 1 and 4 are both special cases of this one lookup; each must keep its own
            // arm, or the accept-swear abort would be silently dropped.
            let found = if confirmation_type == 4 {
                self.contexts
                    .iter()
                    .position(|(_, c)| matches!(c, Callback::FellowshipRequest { .. }))
            } else {
                self.contexts.iter().position(|(_, c)| {
                    *c == Callback::ServerConfirmation {
                        confirmation_type,
                        context_id,
                    } || (confirmation_type == 1 && *c == Callback::AcceptSwear { context_id })
                })
            };
            let Some(at) = found else {
                interaction.stats.confirmations_aborts_unmatched += 1;
                continue;
            };
            let context = self.contexts[at].0;
            // The context answered with is the **slot's**, not the abort's: reads
            // the fellowship-request server context id, which is what the *request* stored.
            let answered_context = match self.contexts[at].1 {
                Callback::FellowshipRequest { context_id } => context_id,
                _ => context_id,
            };
            interaction.confirm_server_confirmation(confirmation_type, answered_context, false);
            interaction.stats.confirmations_aborted += 1;
            let root = ui.dialogs.info(context).and_then(|i| i.element);
            if let Some((queue, info)) = ui.dialogs.begin_close_dialog(context) {
                ui.send_notice(
                    dereth_ui::NoticeId::DialogClosed,
                    &dereth_ui::NoticePayload {
                        a: u32::try_from(context).expect("32-bit dialog context"),
                        ..Default::default()
                    },
                );
                if let Some(root) = root {
                    ui.remove_and_delete_root(root);
                }
                ui.dialogs.finish_close_dialog(queue, info, now.0);
            }
            ui.dialogs.take_callback(context);
            ui.dialogs.completed.retain(|i| i.context != context);
            self.contexts.retain(|(c, _)| *c != context);
        }
        // **The Resign arm.** The three property values matter: both `0x8E`
        // (Confirmation) and `0xC3` (queue id) receive enum 1, while `0xC5`
        // receives the exact shipped prompt. Queue 1 is the factory's all-at-once/non-queued list.
        // The model's synchronous sentinel is the native one-slot guard; only this host replaces
        // it with the context returned by opening the callback dialog.
        if world.minigame.resign_prompt_pending {
            world.minigame.resign_prompt_pending = false;
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc3, PropertyValue::Enum(1));
            data.set(
                0xc5,
                PropertyValue::String(dereth_client_model::minigame::RESIGN_PROMPT.to_owned()),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                world.minigame.resign_dialog =
                    u32::try_from(context).expect("32-bit dialog context");
                self.contexts.push((context, Callback::MiniGameQuit));
            } else {
                world.minigame.cancel_resign_dialog();
            }
        }
        // **The housing panel's two questions.** Both set property `0x8E` to enum 1
        // (Confirmation), then set `0xC5` to the exact prompt.
        // Neither function sets 0xC3, so both use the factory's default queue 2. The two context
        // slots are independent: one Buy and one proxy-Rent question may both exist, with the
        // second waiting on the same queue.
        for rent in house_payments {
            if self
                .contexts
                .iter()
                .any(|(_, callback)| *callback == Callback::HousePayment { rent })
            {
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(
                0xc5,
                PropertyValue::String(
                    if rent {
                        dereth_ui_screens::panels::slumlord::RENT_BY_PROXY_CONFIRMATION
                    } else {
                        dereth_ui_screens::panels::slumlord::BUY_CONFIRMATION
                    }
                    .to_owned(),
                ),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                // This is a panel close-notice subscriber in retail; note_callback gives the
                // shared Rust owner the same retained PropertyCollection/answer lifetime.
                ui.dialogs.note_callback(context);
                self.contexts
                    .push((context, Callback::HousePayment { rent }));
            } else {
                // The panel set its native non-zero-context equivalent before raising this
                // request. A failed factory creates no dialog/answer, but must release that guard
                // so a later physical click can try again.
                ui.requests.emit(
                    dereth_ui_screens::view::UiRequest::HousePaymentConfirmationAnswer {
                        rent,
                        confirmed: None,
                    },
                );
            }
        }
        // **The gameplay confirmation dialog builder**, reached from whichever of the five
        // `*_ConfirmationRequest` handlers the confirmation type selects. Each handler stores the
        // pair first (the type, then the context) and then builds the prompt. The active-slot guard is checked first, then the builder stores confirmation
        // kind 1, modal flag true, and the prompt before retaining the returned dialog context.
        //
        // `0xAC` is `true` here and absent on the other two dialogs in this file, which is the one
        // real difference between them: this one blocks clicks and comes to the front.
        for (confirmation_type, context_id, text) in requests {
            // The request's type and server context are stored **before** the active-dialog guard,
            // so a second question while one is up takes over the open dialog's pair. Nothing new
            // is shown and nothing is queued: the first question's words stay on screen, but
            // answering the dialog now answers the second question, a withdrawal of the first no
            // longer matches and leaves the dialog up, and a withdrawal of the second closes it and
            // refuses on the way out. The first question is left to the shard's timeout. This is
            // the retail client's own behaviour, odd as it is, and it is kept deliberately.
            //
            // A context of ours in `self.contexts` *is* the gameplay confirmation context being
            // non-zero, and its callback carries the stored pair.
            if let Some((_, slot)) = self
                .contexts
                .iter_mut()
                .find(|(_, c)| matches!(c, Callback::ServerConfirmation { .. }))
            {
                *slot = Callback::ServerConfirmation {
                    confirmation_type,
                    context_id,
                };
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xac, PropertyValue::Bool(true));
            data.set(
                0xc5,
                PropertyValue::String(prompt_for(confirmation_type, &text)),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((
                    context,
                    Callback::ServerConfirmation {
                        confirmation_type,
                        context_id,
                    },
                ));
            }
        }
        // **Fellowship-request dialog construction.**
        //
        // Two properties, not three: `0x8E = (enum)1` and `0xC5 = the StringInfo`, and **no**
        // `0xAC`. The gameplay dialog sets `0xAC = true` and this one does not, so
        // the invitation neither blocks clicks nor comes to the front — it can sit on screen
        // while the player carries on, which is the whole reason retail keeps two slots.
        //
        // The guard requires an empty fellowship-request context, and a context of ours in
        // `self.contexts` *is* that field being non-zero — the same stand-in `CloseVendor` and
        // `ServerConfirmation` use, and independent of theirs.
        for (context_id, name) in fellowship {
            if self
                .contexts
                .iter()
                .any(|(_, c)| matches!(c, Callback::FellowshipRequest { .. }))
            {
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(
                0xc5,
                PropertyValue::String(
                    dereth_ui_screens::panels::fellowship::fellowship_request_prompt(ui, &name),
                ),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts
                    .push((context, Callback::FellowshipRequest { context_id }));
            }
        }
        // The three ordinary-use confirmations have the same property collection shape in
        // the player-killer altar, non-player-killer altar and volatile-rare confirmations:
        // Confirmation kind 1, the literal prompt in 0xC5, and the object in
        // 0x1000003D. No modal 0xAC property is set. DialogController's default queue preserves a
        // second prompt until the first is answered, as retail's factory does.
        for (object, kind) in usage {
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(kind.prompt().to_string()));
            data.set(SOURCE, PropertyValue::InstanceId(object.0));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::Usage));
            }
        }
        for (source, target, kind) in notices {
            let (Some(src), Some(tgt)) = (world.weenie(source), world.weenie(target)) else {
                continue;
            };
            // These callers use raw PWD names, not the object-name lookup with name type 2.
            let prompt = match kind {
                TargetedUsageConfirmation::ManaStone => format!(
                    "\nAre you sure you want to attempt to destroy your {} and drain its mana into this stone?", tgt.pwd.name),
                TargetedUsageConfirmation::Salvage => format!(
                    "\nAre you sure you want to apply the {} to the {}? The {} may be destroyed.",
                    src.pwd.name.replace(" (100)", ""), tgt.pwd.name, tgt.pwd.name),
            };
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt));
            data.set(SOURCE, PropertyValue::InstanceId(source.0));
            data.set(TARGET, PropertyValue::InstanceId(target.0));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::TargetedUse));
            }
        }
        // **The vendor button handler's case `0x100000D6`.**
        //
        // The same two properties the block above builds, because retail builds the same two:
        // property `0x8E` receives enum value 1 and property `0xC5` receives the prompt. The prompt is
        // the client's own literal, carried across from `dereth_client_model`'s
        // `vendor::UNFINISHED_TRANSACTIONS`, and there are **no** instance-id properties on it —
        // the vendor-close callback reads nothing but the answer.
        //
        // An empty current-dialog context is the guard, and it is honored here by
        // refusing to make a second one while one of ours is still open rather than by a flag of
        // its own: a context in `self.contexts` means the current-dialog slot is non-zero.
        // **The three allegiance confirmation builders.** Each builds the same two
        // properties every other question in this file builds: `0x8E` = enum 1 (the Yes/No kind)
        // and `0xC5` = the prompt. `0xAC` (modal, click-blocking) is **absent**, as it is on the
        // vendor's and the targeted-use ones — only the gameplay confirmation
        // sets it.
        //
        // The empty-swear-context guard is here rather than on the panel because the panel
        // never learns that a dialog closed; a context of ours for that action *is* the panel's
        // context being non-zero, which is exactly how `Callback::CloseVendor` stands in for
        // the current-dialog context two blocks below.
        // **Receiving a swear-allegiance request leads to the accept-swear confirmation.** An empty accept-swear context is
        // the guard, and a context of ours with this callback is that slot being non-zero.
        for (context_id, name) in interaction.take_swear_requests() {
            if self
                .contexts
                .iter()
                .any(|(_, c)| matches!(c, Callback::AcceptSwear { .. }))
            {
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(
                0xc5,
                PropertyValue::String(dereth_ui_screens::panels::allegiance::accept_swear_prompt(
                    ui, &name,
                )),
            );
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts
                    .push((context, Callback::AcceptSwear { context_id }));
            }
        }
        for (action, target, prompt) in interaction.take_allegiance_confirmations() {
            if self
                .contexts
                .iter()
                .any(|(_, c)| matches!(c, Callback::Allegiance { action: a, .. } if *a == action))
            {
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts
                    .push((context, Callback::Allegiance { action, target }));
            }
        }
        // **The `@die` command's question.** The same two properties every other
        // question in this file builds: `0x8E` = enum 1 (Yes/No) and `0xC5` = the prompt.
        //
        // There is no "one at a time" guard here: the `@die` command keeps no context slot of its
        // own, so a second `@die` while the question is up makes a second identical question,
        // which waits behind the first on the default queue (the waiting count shows it) and is
        // asked as soon as the first is answered. The player is asked twice, and each Yes is its
        // own suicide request.
        for prompt in interaction.take_die_confirmations() {
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt.to_string()));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::Die));
            }
        }
        // **`@house abandon`.** The same two properties, and no guard either, for the same
        // reason: there is no context slot for either stage, so a second `@house abandon` queues a
        // second first-stage question behind the first, and a second-stage question raised while
        // another question is up waits behind it in turn.
        //
        // Stage two's queue is filled by stage one's *callback*, which runs earlier in this same
        // function, so the second question appears on the frame the first was answered (or joins
        // the queue, when another question is already waiting).
        for prompt in interaction.take_house_abandon_first() {
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt.to_string()));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::HouseAbandonFirst));
            }
        }
        for prompt in interaction.take_house_abandon_second() {
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt.to_string()));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::HouseAbandonSecond));
            }
        }
        for prompt in closes {
            if self
                .contexts
                .iter()
                .any(|(_, c)| *c == Callback::CloseVendor)
            {
                continue;
            }
            let mut data = PropertyCollection::new();
            data.set(0x8e, PropertyValue::Enum(1));
            data.set(0xc5, PropertyValue::String(prompt.to_string()));
            if let Some(context) = ui.dialogs.make_dialog(data, now.0) {
                ui.dialogs.note_callback(context);
                self.contexts.push((context, Callback::CloseVendor));
            }
        }
        for (context, kind) in ui.dialogs.pending_create() {
            // This includes the popup list. The guard is "a context this subscriber
            // raised" and not "a context with a callback": `0x0004`'s box has no callback and still
            // needs its element, and something else's dialog must still not be built here.
            if !self.contexts.iter().any(|(c, _)| *c == context) && !self.popups.contains(&context)
            {
                continue;
            }
            let data = ui
                .dialogs
                .info(context)
                .expect("factory pending create")
                .data
                .clone();
            let root = match ui.require_env().and_then(|e| {
                e.create_and_add_root_element(ui, dereth_ui::LayoutEnum(2), kind.root_element_id())
            }) {
                Ok(root) => root,
                Err(error) => {
                    tracing::warn!("targeted dialog creation failed: {error:?}");
                    continue;
                }
            };
            //  defaults modal=false; preserve shipped Yes/No captions.
            ui.set_attribute_bool(root, dereth_ui::props::attr::DIALOG_MODAL, false);
            if let Some(PropertyValue::String(text)) = data.get(0xc5) {
                if let Some(h) = ui.get_child_recursive(root, dialog::base::child::TEXT) {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(text);
                    }
                }
            }
            dialog::types::set_dialog_data(ui, root, &data);
            dialog::base::update_popup_size_and_position(ui, root);
            // The UI-flow factory reset deletes these with the framework.
            ui.bind_dialog_element(context, root);
        }
        // Pending-dialog refresh sets only child 0x33's state: pending -> 24, none -> 25.
        // The boolean-to-state arithmetic does NOT format the count into child 0x34.
        for (context, _) in &self.contexts {
            let Some(info) = ui.dialogs.info(*context) else {
                continue;
            };
            let Some(root) = info.element else { continue };
            let pending = info.dialog.as_ref().is_some_and(|d| d.pending_behind != 0);
            if let Some(h) = ui.get_child_recursive(root, dereth_ui::ElementId(0x33)) {
                let state = dereth_ui::StateId(if pending { 24 } else { 25 });
                if ui.node(h).is_some_and(|n| n.state != state) {
                    ui.set_state(h, state);
                }
            }
        }
    }
}

/// What the dialog's prompt line says, per `ConfirmationType`.
///
/// Four of the five gameplay UI handlers push the **same** literal — one shared string,
/// eleven bytes of `" Continue?\0"` — and append it to the server's text. Each one also stores
/// its own `ConfirmationType` in the gameplay confirmation state first, so the type and arm are
/// pinned together:
///
/// ```text
///   type 2: AlterSkill       -- append " Continue?"
///   type 3: AlterAttribute   -- append " Continue?"
///   type 6: Augmentation     -- append " Continue?"
///   type 5: CraftInteraction -- append " Continue?"
///   type 7: Yes_No           -- append nothing
/// ```
///
/// The Yes/No confirmation request is the exception and the only one: it appends
/// no literal, taking the request's text straight to the prompt.
/// That is why an NPC's question reads as the NPC wrote it and a
/// gem's warning gets a question mark bolted on.
fn prompt_for(confirmation_type: i32, text: &str) -> String {
    match confirmation_type {
        // `Yes_No` — verbatim.
        7 => text.to_string(),
        // `AlterSkill`, `AlterAttribute`, `CraftInteraction`, `Augmentation`.
        _ => format!("{text} Continue?"),
    }
}

fn iid(data: &PropertyCollection, key: u32) -> ObjectId {
    ObjectId(match data.get(key) {
        Some(PropertyValue::InstanceId(id)) => *id,
        _ => 0,
    })
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;

    #[test]
    fn absent_framework_discards_confirmation_notice_without_accepting_it() {
        let mut world = dereth_client_model::World::default();
        world.player = Some(ObjectId(3));
        for id in [1, 2, 3] {
            let mut weenie = dereth_client_model::Weenie::new(ObjectId(id));
            weenie.valid = true;
            weenie.pwd.name = format!("item{id}");
            weenie.pwd.obj_type = 1;
            world.tables.weenies.insert(weenie.id, weenie);
        }
        let source = world.weenie_mut(ObjectId(1)).unwrap();
        source.pwd.obj_type = 0x0008_0000;
        source.pwd.effects = Some(0);
        source.pwd.container_id = Some(ObjectId(3));
        source.pwd.useability = Some(0x0020_0008);
        source.pwd.target_type = Some(1);
        let mut interaction = crate::interaction::Interaction::default();
        interaction.queue(
            Vec::new(),
            vec![dereth_ui_screens::view::UiRequest::Use(ObjectId(1))],
        );
        interaction.run_ui_requests(&mut world, false, ServerTime(1.0));
        assert_eq!(world.targeting_object, ObjectId(1));
        interaction.queue(
            Vec::new(),
            vec![dereth_ui_screens::view::UiRequest::ExecuteTargetItem(
                ObjectId(2),
            )],
        );
        interaction.run_ui_requests(&mut world, false, ServerTime(1.01));
        assert_eq!(world.targeting_object, ObjectId(0));
        assert_eq!(
            interaction.take_targeted_confirmations(),
            vec![(
                ObjectId(1),
                ObjectId(2),
                TargetedUsageConfirmation::ManaStone
            )],
            "positive producer denominator"
        );
        interaction.queue(
            Vec::new(),
            vec![
                dereth_ui_screens::view::UiRequest::Use(ObjectId(1)),
                dereth_ui_screens::view::UiRequest::ExecuteTargetItem(ObjectId(2)),
            ],
        );
        interaction.run_ui_requests(&mut world, false, ServerTime(1.5));
        let mut dialogs = TargetedDialogs::default();
        dialogs.service(None, &mut interaction, &mut world, LocalTime(1.5));
        assert!(
            interaction.take_targeted_confirmations().is_empty(),
            "no notice history for a later UI"
        );
        assert!(interaction.take_pending_requests().is_empty());
        assert_eq!(world.magic.busy_count, 0);
        assert!(dialogs.contexts.is_empty());
    }
}
