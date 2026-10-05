//! The wizard's dialogs, from the click to the answer.

use super::*;

impl CharGenScreen {
    // -------------------------------------------------------------------------------------------
    // The wizard's dialogs, from the click to the answer
    //
    // The Exit confirmation and the other contexts the client can actually raise, using the same
    // dialog shape as character management. The moving parts are [`CharGenScreen::make_dialog`]
    // (the element), `service_dialogs` (raising recorded dialogs and taking down answered ones),
    // and `answer_dialog`, which handles the close-dialog notice.
    // -------------------------------------------------------------------------------------------

    /// The live element behind one of the six dialog contexts.
    #[must_use]
    pub const fn dialog_element(&self, ctx: CharGenDialog) -> Option<ElemHandle> {
        match ctx {
            CharGenDialog::Exit => self.exit_dialog,
            CharGenDialog::RandomizeWarning => self.randomize_warning_dialog,
            CharGenDialog::CreditWarning => self.credit_warning_dialog,
            CharGenDialog::ErrorMessage => self.error_message_dialog,
            CharGenDialog::ToDRequired => self.tod_warning_dialog,
            // The please-wait context is written only by the constructor and the destructor, both
            // with zero.
            CharGenDialog::PleaseWait => None,
        }
    }

    /// The factory context returned for a context, which is what the screen's context field
    /// actually holds. **`Some` with no [`Self::dialog_element`] is a dialog waiting its turn on
    /// queue 2** -- the state this screen could not represent at all before it used the factory.
    #[must_use]
    pub const fn dialog_context(&self, ctx: CharGenDialog) -> Option<u64> {
        self.dialog_queue.get(ctx)
    }

    fn dialog_slot(&mut self, ctx: CharGenDialog) -> Option<&mut Option<ElemHandle>> {
        Some(match ctx {
            CharGenDialog::Exit => &mut self.exit_dialog,
            CharGenDialog::RandomizeWarning => &mut self.randomize_warning_dialog,
            CharGenDialog::CreditWarning => &mut self.credit_warning_dialog,
            CharGenDialog::ErrorMessage => &mut self.error_message_dialog,
            CharGenDialog::ToDRequired => &mut self.tod_warning_dialog,
            CharGenDialog::PleaseWait => return None,
        })
    }

    /// The character-generation exit path:
    ///
    /// Refused when an exit warning is already recorded. Otherwise it builds a property set with
    /// `0x8E = 1` (confirmation), `0xAC = true` (modal) and `0xC5 =` the `ID_CharGen_ExitWarning`
    /// string from table `0x10000002`, creates the dialog in the current UI, and stores its
    /// context.
    ///
    /// Returns whether a dialog was raised, as the client's `bool` does.
    pub fn do_exit(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, EXIT_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::Exit, &text)
    }

    /// The randomize warning dialog build.
    ///
    /// The same shape as the exit warning: refused when already recorded, otherwise a modal
    /// confirmation (`0x8E = 1`, `0xAC = 1`) whose `0xC5` is `ID_CharGen_RandomizeWarning`, with
    /// its context stored.
    ///
    /// A yes answer runs character randomization.
    pub fn make_randomize_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, RANDOMIZE_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::RandomizeWarning, &text)
    }

    /// The screen's credit warning dialog build — `0x8E = 1`, `0xAC = 1`, `0xC5 =
    /// ID_CharGen_CreditWarning`. *Yes* re-enters the finish with the credit check off.
    pub fn make_credit_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, CREDIT_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::CreditWarning, &text)
    }

    /// The screen's error-message dialog build — `0x8E = 3` (`Message`, one button `0x26`). Its
    /// text is the `StringInfo` the caller supplied, which this build carries as
    /// [`CharGenScreen::error_string_id`] because none of the callers has a `UiSystem` to resolve
    /// one with.
    pub fn make_error_message_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let Some(token) = self.error_string_id else {
            return false;
        };
        let text = self.string(ui, token);
        self.make_dialog(ui, CharGenDialog::ErrorMessage, &text)
    }

    /// The screen's expansion warning dialog build — `0x8E = 3`, `0xC5 =
    /// ID_CharGen_ToDRequiredWarning`. Raised from element-message handling when an account without
    /// a *Throne of Destiny* takes a heritage or a start area that needs one.
    pub fn make_tod_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, TOD_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::ToDRequired, &text)
    }

    /// Shared dialog creation: fill the `PropertyCollection` and hand it to
    /// the current UI's dialog factory, which allocates
    /// the global dialog context.
    ///
    /// **The `PropertyCollection` goes through the factory.** The factory lives on `UiSystem` --
    /// which is where the client keeps it, as statics on its own element-manager singleton -- so a
    /// [`Screen`] can reach it. Applying the properties straight to the element instead would make
    /// the five dialogs **stack** where retail queues them. See [`Self::service_dialog_queue`].
    ///
    /// Returns whether a dialog is **on screen** for this context afterwards, which is every dialog
    /// build's own `bool`: `false` for "one is already up" (the context-is-zero check each of them
    /// opens with), for a host with no dat behind it, and also for a dialog that was **queued**
    /// behind another on queue 2 -- it has a context and no element yet.
    fn make_dialog(&mut self, ui: &mut UiSystem, ctx: CharGenDialog, text: &str) -> bool {
        // The context-is-zero check, on the **context** and not on the element, which is what the
        // client tests: a dialog still waiting its turn on the queue has a context and no element,
        // and a second press must be refused then too.
        if self.dialog_queue.get(ctx).is_some() {
            return false;
        }
        let Some(kind) = ctx.kind() else { return false };
        // Recorded **before** the element exists, so a host with no environment installed still
        // sees the arm that was reached. Tests with no environment depend on this ordering.
        self.open_dialog = Some(ctx);
        // Each dialog path fills a `PropertyCollection` and hands it to the "make dialog in the
        // current UI" call. Property `0x8E` is the kind, `0xAC` is modality, and `0xC5` is the
        // prompt. **None of the five sets `0x8D` or `0xC3`**, so all five share queue **2** and a
        // second one waits behind the first rather than stacking on top of it, which is the whole
        // of what the queue is for.
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            PropertyValue::Integer(kind.property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            PropertyValue::String(text.to_string()),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        if let Some(slot) = self.dialog_queue.slot(ctx) {
            *slot = Some(context);
        }
        // Dialog creation either built this context immediately or queued it behind the
        // one already open on queue 2. Building the element is this side's work either way; a
        // queued one is built on the frame its turn comes.
        self.service_dialog_queue(ui);
        self.dialog_element(ctx).is_some()
    }

    /// The client's **element** half, run for every context that the factory has opened for this
    /// screen and that has no element yet.
    ///
    /// `DialogController` lives on `UiSystem`, exactly as its statics live on the client's own
    /// singleton, so the queue is reachable from a [`Screen`] and the wizard's five dialogs queue
    /// as retail's do instead of stacking. This is the queue's second production caller. The order
    /// below is the client's: create the element for the kind's root, set its data, size and
    /// position it, bring it forward, then send the open-dialog notice -- the last two inside
    /// [`dereth_ui::UiSystem::bind_dialog_element`].
    fn service_dialog_queue(&mut self, ui: &mut UiSystem) {
        // The factory is asked which contexts are owed an element rather than being
        // second-guessed here; re-implementing the `element.is_none()` filter inline would be a
        // second copy of a rule that must not drift.
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        for ctx in CharGenDialog::RAISED {
            let Some(context) = self.dialog_queue.get(ctx) else {
                continue;
            };
            if !owed.contains(&context) {
                continue;
            }
            let Some(info) = ui.dialogs.info(context) else {
                continue;
            };
            let data = info.data.clone();
            let root = info.kind.root_element_id();
            let Ok(h) = ui
                .require_env()
                .and_then(|e| e.create_and_add_root_element(ui, DIALOG_LAYOUT, root))
            else {
                continue;
            };
            // The dialog's modal-property (`0xAC`) arm blocks clicks with mask `0x40`.
            ui.set_attribute_bool(
                h,
                dereth_ui::props::attr::DIALOG_MODAL,
                data.get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                    .unwrap_or(false),
            );
            // The dialog's text update: property `0xC5` goes on child **`0x3E`**
            // (recursive lookup for `0x3E`); `0x3D` is the panel around it.
            if let Some(PropertyValue::String(text)) = data
                .get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
                .cloned()
            {
                if let Some(t) = ui
                    .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                    .and_then(|c| ui.text_element_mut(c))
                {
                    t.set_text(&text);
                }
            }
            // The per-dialog data application sets the button captions.
            dereth_ui::dialog::types::set_dialog_data(ui, h, &data);
            // The answer route, **by element id and not by bubbling**: `DialogElement`'s own
            // `listen_to_element_message` stops processing for the ids it recognises, so a listener
            // on the dialog root would never be reached. The same route character management and
            // the logout confirmation take.
            let (accept, cancel) = ctx.answer_children();
            for id in [accept, cancel].into_iter().flatten() {
                ui.register_for_element_message(id, MessageId(1), ME);
            }
            // Grow the shipped 400 x 95 panel to fit the prompt and centres it. Without it the
            // warning shows its first line in the top-left corner of the screen.
            dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
            // Recorded as one of this screen's roots so use_new_mode deletes it with the screen,
            // which is the destructor calling the reset.
            self.roots.push(h);
            // Bring to front, then the open-dialog notice for the context.
            ui.bind_dialog_element(context, h);
            if let Some(slot) = self.dialog_slot(ctx) {
                *slot = Some(h);
            }
        }
    }

    /// The client's visible half: delete the element and forget it.
    fn close_dialog_element(&mut self, ui: &mut UiSystem, ctx: CharGenDialog) {
        let context = self.dialog_queue.slot(ctx).and_then(Option::take);
        let element = self.dialog_slot(ctx).and_then(Option::take);
        // The dialog factory's close frees the queue slot and, through
        // its "open next dialog", promotes whatever was waiting behind it. Told before the
        // element goes, so the promoted dialog is already current when the sweep below runs.
        if let Some(context) = context {
            ui.dialogs.close_dialog(context, ui.now.0);
        }
        if let Some(h) = element {
            for id in {
                let (a, c) = ctx.answer_children();
                [a, c]
            }
            .into_iter()
            .flatten()
            {
                ui.unregister_for_element_message(id, MessageId(1), ME);
            }
            self.roots.retain(|r| *r != h);
            ui.remove_and_delete_root(h);
        }
        // A dialog that was waiting behind this one is now the open one on queue 2 and is owed
        // an element. Without this a queued warning would never appear at all, which is the
        // failure mode a queue has and a stack does not.
        self.service_dialog_queue(ui);
    }

    /// Raise the element for a context that has been recorded and has none, and take down any
    /// element whose context is no longer the open one.
    ///
    /// The second half is what makes [`CharGenScreen::close_dialog`] safe to call **without** a
    /// `UiSystem`, which the host does from `app.rs`'s `--create` drive and which the
    /// verification-response handling does when a server reply dismisses the dialog that was
    /// waiting for it: `close_dialog` clears `open_dialog`, and the next frame with a `UiSystem` in
    /// it deletes the orphan.
    ///
    /// Called from both [`Screen::update`] and the tail of [`Screen::on_element_message`], for the
    /// same reason as on the character-management screen: the second is reached on a frame whose
    /// own element traffic recorded the context, and the first on a frame with no element traffic
    /// at all.
    pub(super) fn service_dialogs(&mut self, ui: &mut UiSystem) {
        // An answer taken on a call that had no `UiSystem`, honoured on the first frame that
        // does. This also frees the factory's queue slot, so whatever was waiting behind it is
        // promoted and built below.
        if let Some(ctx) = self.pending_close.take() {
            self.close_dialog_element(ui, ctx);
        }
        // An element for a context the factory no longer has open -- a property-`0x8D` replacement,
        // or a close this screen did not make. The factory is the authority on which dialog is
        // current; guessing from `open_dialog` instead goes wrong the moment two are recorded at
        // once.
        for ctx in CharGenDialog::RAISED {
            if self.dialog_element(ctx).is_none() {
                continue;
            }
            let still_open = self
                .dialog_context(ctx)
                .is_some_and(|c| ui.dialogs.info(c).is_some());
            if !still_open {
                self.close_dialog_element(ui, ctx);
            }
        }
        // Anything the factory has made current and not yet given an element.
        self.service_dialog_queue(ui);
        let Some(ctx) = self.open_dialog else { return };
        if self.dialog_element(ctx).is_some() {
            return;
        }
        match ctx {
            CharGenDialog::Exit => self.do_exit(ui),
            CharGenDialog::RandomizeWarning => self.make_randomize_warning_dialog(ui),
            CharGenDialog::CreditWarning => self.make_credit_warning_dialog(ui),
            CharGenDialog::ErrorMessage => self.make_error_message_dialog(ui),
            CharGenDialog::ToDRequired => self.make_tod_warning_dialog(ui),
            // No character-generation dialog path raises one; see [`CharGenDialog::kind`].
            CharGenDialog::PleaseWait => false,
        };
    }

    /// Whether an element id is the *accept* half of a context's pair — the one statement of
    /// "which button is yes", taken from [`CharGenDialog::answer_children`] rather than repeated
    /// at the call site, so that transposing that table is a behaviour change.
    pub(super) fn is_accept(ctx: CharGenDialog, id: ElementId) -> bool {
        ctx.answer_children().0 == Some(id)
    }

    /// The char-gen screen's close-dialog notice, reached from the dialog's own buttons
    /// rather than from a notice, because nothing in this build raises
    /// the UI close-dialog notice.
    ///
    /// The element goes first — that is the dialog close, which deletes it and *then* notifies —
    /// and the context is named explicitly rather than read off `open_dialog`, because the client
    /// demultiplexes on which of its six handles matched the incoming one.
    pub(super) fn answer_dialog(&mut self, ui: &mut UiSystem, ctx: CharGenDialog, yes: bool) {
        self.close_dialog_element(ui, ctx);
        self.open_dialog = Some(ctx);
        if let Some(m) = self.close_dialog(yes) {
            ui.requests.emit(crate::view::UiRequest::QueueMode(m));
        }
        self.service_dialogs(ui);
    }
}
