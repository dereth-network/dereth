//! One action's row: its key buttons and the dialogs it raises.

use super::*;

// ---------------------------------------------------------------------------------------------
// One row
// ---------------------------------------------------------------------------------------------

/// Which of the row's three dialogs a context belongs to —
/// dispatches on exactly this, comparing the closing context against its three fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDialog {
    /// The open map warn dialog / the close map warn dialog — "press a key".
    MapWarn,
    /// "That key is already used by X; replace?".
    Overwrite,
    /// The existing binding is not user-bindable.
    CantOverwrite,
}

/// What [`ActionKeyMapRow::on_element_message`] did, so a caller can assert on an arm rather than
/// on a side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowEvent {
    /// Message 1 on the clear button — every binding freed to [`DO_NOTHING`].
    ClearedAll(usize),
    /// Message 1 on key button `slot` — capture entered.
    CaptureStarted { slot: usize },
    /// Message 1 on a key button while a dialog is open on [`DIALOG_QUEUE`]: refused, silently, by
    /// the client's own is-dialog-open guard.
    Refused,
    /// Message `0x19` with `p1 == 8` (secondary click) on key button `slot`.
    Erased { slot: usize },
}

/// One bindable action's key-map row.
#[derive(Debug, Clone, Default)]
pub struct ActionKeyMapRow {
    /// The row element itself carries the action's name as text, which
    /// is what the binding initiation reads back.
    pub element: Option<ElemHandle>,
    /// The clear button, from attribute [`attr::CLEAR_BUTTON`]. `None` in the shipped layout.
    pub clear_button: Option<ElemHandle>,
    /// The key buttons, from attribute [`attr::KEY_BUTTONS`]. Three in the shipped template.
    pub key_buttons: Vec<ElemHandle>,
    /// The action this row binds.
    pub action: ActionId,
    /// The input map the action lives in.
    pub input_map: InputMapId,
    /// The default keys — the keys the action had when [`Self::init`] ran.
    pub defaults: Vec<ControlChord>,
    /// The saved keys, what Cancel reverts to.
    pub saved: Vec<ControlChord>,
    /// The current keys.
    pub current: Vec<ControlChord>,
    /// The chord being bound — `None` is the client's invalid key `0xFFFFFFFF`.
    pub binding_being_changed: Option<ControlChord>,
    /// The slot being bound — `None` is the client's `-1`.
    pub slot_being_changed: Option<usize>,
    /// Skip the overwrite confirmation; set only when restoring mouse-turning defaults.
    pub skip_confirmation: bool,
    /// The contexts of the row's three dialogs: map-warn, overwrite, can't-overwrite.
    pub dialogs: Vec<(RowDialog, u64)>,
    /// The action's name, as resolved it.
    pub label: String,
    /// Its description, the same way — the client's id.
    pub tooltip: String,
    /// What [`Self::refresh`] last wrote onto each key button, in key-button order. Empty
    /// where the button is past the end of the current list. Recorded because a caption that landed
    /// and a caption that did not are otherwise indistinguishable headless.
    pub button_labels: Vec<String>,
    /// The value [`Self::refresh`] last wrote to the clear button's [`attr::CLEAR_DISABLED`], or
    /// `None` when there is no clear button — which is the shipped case, and is a fact about the
    /// data rather than a failure.
    pub clear_disabled: Option<bool>,
    /// The arguments of the calls
    /// this row has made and [`KeyBindingPage::drain_refresh_notices`] has not delivered yet.
    ///
    /// Retail sends that notice from exactly **two** places — erasing and setting a binding —
    /// and in both it is the last thing done before returning. The notice is how a rebind reaches
    /// the *other* rows: [`Self::set_binding`] displaces every conflicting control out of every map that can be
    /// registered beside this one, and the row those controls belonged to is only told this way.
    ///
    /// Each action-key-map row listens on the global event handler, so the
    /// client sends straight from the row; here the rows are owned by [`KeyBindingPage`], so the
    /// row records the call and the page performs the broadcast in the same gesture.
    pub pending_refresh_notices: Vec<ControlChord>,
}

impl ActionKeyMapRow {
    /// Bind the clear button and the key buttons.
    ///
    /// The client bails out of the whole of the row's post-init when `0x1000002A` is present and
    /// its child does not resolve, which is why the attribute is read first and
    /// why a row whose layout names a clear button that is not there registers no notice. Absent
    /// attribute is **not** that case: the enum read answers false and the function carries
    /// on to the array.
    #[must_use]
    pub fn post_init(ui: &UiSystem, element: ElemHandle) -> Option<Self> {
        let mut row = Self {
            element: Some(element),
            ..Self::default()
        };
        if let Some(id) = crate::bind::attr_enum(ui, element, attr::CLEAR_BUTTON) {
            let h = ui.get_child_recursive(element, ElementId(id))?;
            row.clear_button = Some(h);
        }
        for id in key_button_ids(ui, element) {
            if let Some(h) = ui.get_child_recursive(element, id) {
                row.key_buttons.push(h);
            }
        }
        Some(row)
    }

    /// The action key map option control's initialisation, as the keyboard panel's action key
    /// map insert calls it.
    ///
    /// `keys` is `InputManager::find_keys_for_action`'s answer on the **merged** master map, which is
    /// what makes the default, saved and current lists all start equal.
    /// `keys` is the current list, returned by `find_keys_for_action` on the **merged** map;
    /// `defaults` is the default list, passed as the fifth argument and copied from the
    /// keys of the **shipped** default maps. They are a different map from `keys`; using one for
    /// both would make *Restore Defaults* restore whatever the page was opened with.
    ///
    /// The saved list is **not** written by the client's initialisation: it is left empty and the
    /// closing `save_current_values` of `init_options` fills it. Seeded from `keys` anyway, because
    /// a row built outside `init_options` would otherwise read as changed the moment it
    /// appeared.
    pub fn init(
        &mut self,
        action: ActionId,
        input_map: InputMapId,
        label: String,
        tooltip: String,
        keys: Vec<ControlChord>,
        defaults: Vec<ControlChord>,
    ) {
        self.action = action;
        self.input_map = input_map;
        self.label = label;
        self.tooltip = tooltip;
        self.defaults = defaults;
        self.saved.clone_from(&keys);
        self.current = keys;
    }

    /// The action key map option control's refresh.
    ///
    /// Two loops and a tail, in the client's order:
    ///
    /// 1. walk the current list alongside the key buttons, stopping at whichever runs out first,
    ///    and for each write the control's name onto the button and a tooltip beside it;
    /// 2. every remaining button has its text cleared and gets the *new binding* tooltip;
    /// 3. the clear button's bool attribute `0x0D` is set to "the current list is empty".
    ///
    /// Resolve the caption's `LABEL` and the bound tooltip's nested `VALUE` through the existing
    /// StringTable renderer. The resulting literals keep the shipped decoration and meta-language;
    /// no tooltip prose or key names are supplied by this control.
    pub fn refresh(&mut self, ui: &mut UiSystem, m: &InputManager, delimiter: &str) {
        self.button_labels.clear();
        self.button_labels
            .resize(self.key_buttons.len(), String::new());
        for (i, h) in self.key_buttons.clone().into_iter().enumerate() {
            let text = match self.current.get(i) {
                Some(qc) => binding_label(ui, m, qc, delimiter),
                None => String::new(),
            };
            let table = string_table(ui, STRING_TABLE_ENUM);
            let (caption, help) = if text.is_empty() {
                (
                    text,
                    resolve_token(ui, STRING_TABLE_ENUM, token::TT_NEW_BINDING),
                )
            } else {
                let caption = ui
                    .resolve_string_named(
                        table,
                        dereth_primitives::num::hash::str_hash(token::BUTTON_LABEL.as_bytes()),
                        &[(var::LABEL, text.as_str())],
                    )
                    .unwrap_or(text);
                let help = ui.resolve_string_named(
                    table,
                    dereth_primitives::num::hash::str_hash(token::TT_EXISTING_BINDING.as_bytes()),
                    &[(var::VALUE, caption.as_str())],
                );
                (caption, help)
            };
            set_literal(ui, h, &caption);
            ui.set_tooltip(h, help);
            ui.set_tooltip_on(h, true);
            self.button_labels[i] = caption;
        }
        self.clear_disabled = self.clear_button.map(|h| {
            let v = self.current.is_empty();
            crate::bind::set_attr_bool(ui, h, attr::CLEAR_DISABLED, v);
            v
        });
    }

    /// The action key map option control's element-message handler.
    ///
    /// Both arms do nothing while a dialog is open on queue `0x10000001`. Message 1 on the clear
    /// button clears every binding; message 1 on key button `i` starts a capture for slot `i`.
    /// Message `0x19` with first parameter 8 on key button `i` erases binding `i`, but only when
    /// the button is not in state `0x0D` and `i` is inside the current list.
    ///
    /// Parameter 8 is `dereth_ui::focus::action::SECONDARY_CLICK`: **right-clicking a key button
    /// frees it**, and the state `0x0D` and in-range guards are why right-clicking an empty
    /// or greyed slot does nothing.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
        delimiter: &str,
    ) -> Option<RowEvent> {
        use dereth_ui::msg::element::id;
        let blocked = ui.dialogs.is_dialog_open(DIALOG_QUEUE);
        if msg.id == id::BUTTON_CLICKED {
            if blocked {
                return self
                    .key_buttons
                    .contains(&msg.source)
                    .then_some(RowEvent::Refused);
            }
            if Some(msg.source) == self.clear_button {
                let n = self.clear_all_bindings(ui, m, delimiter);
                return Some(RowEvent::ClearedAll(n));
            }
            let slot = self.key_buttons.iter().position(|b| *b == msg.source)?;
            self.initiate_binding(ui, slot);
            return Some(RowEvent::CaptureStarted { slot });
        }
        if msg.id == id::MOUSE_CLICK && msg.p1 == dereth_ui::focus::action::SECONDARY_CLICK {
            if blocked {
                return self
                    .key_buttons
                    .contains(&msg.source)
                    .then_some(RowEvent::Refused);
            }
            let slot = self.key_buttons.iter().position(|b| {
                *b == msg.source && ui.node(*b).is_some_and(|n| n.state != DISABLED_STATE)
            })?;
            if slot >= self.current.len() {
                return None;
            }
            self.erase_binding(ui, m, slot, delimiter);
            return Some(RowEvent::Erased { slot });
        }
        None
    }

    /// The action-key-map option control's binding initialization: record the slot, open the
    /// map-warn dialog with `ID_ActionKeyMap_MapInstructions` (`ACTION` = the row's text), and on
    /// success register the row as the input handler.
    ///
    /// Returns whether the dialog was raised, which is the client's own return and the flag it
    /// gates the input-handler registration on.
    ///
    /// **The dialog's text is the instruction, not the row's bare action label** — a modal saying
    /// only *"Move Forward"* reads as "clicking a cell does nothing". The client sequence is
    /// the string `ID_ActionKeyMap_MapInstructions` from table enum `0x10000004`, with variable
    /// `ACTION` set to the row's own label.
    pub fn initiate_binding(&mut self, ui: &mut UiSystem, slot: usize) -> bool {
        self.slot_being_changed = Some(slot);
        let text = map_instructions(ui, &self.label);
        self.open_dialog(ui, RowDialog::MapWarn, &text)
    }

    /// The capture ended, however it ended.
    pub fn close_map_warn_dialog(&mut self, ui: &mut UiSystem) {
        self.close_dialog_context(ui, RowDialog::MapWarn);
    }

    /// Whether a capture is in flight — the client's *"is the key-hit handler registered"*, which
    /// is "the map-warn dialog context is set".
    #[must_use]
    pub fn capturing(&self) -> bool {
        self.dialog_context(RowDialog::MapWarn).is_some()
    }

    /// The capture, end to end.
    ///
    /// The **policy** is [`InputManager::capture_key_hit`], transcribed from this same function;
    /// what lives here is everything around it that is the *row's*: unregistering
    /// the handler, closing the map-warn dialog, opening one of the two overwrite dialogs, and
    /// calling [`Self::set_binding`] when there is nothing to ask about.
    ///
    /// `control` is the **event**, so its activation is `Down` or `Up`; step 4 of the policy is
    /// what turns it into a `Click` binding. Handing it a binding-shaped control would trip
    /// `activation & 0x81` and be [`Capture::Ignored`], which is why the client passes the raw
    /// event and so does this.
    pub fn key_hit(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        control: ControlChord,
        delimiter: &str,
    ) -> Capture {
        let verdict =
            m.capture_key_hit(self.input_map, self.action, control, self.skip_confirmation);
        match &verdict {
            // The handler stays registered and the dialog stays up: these are not answers.
            Capture::Ignored | Capture::Rejected => return verdict,
            _ => {}
        }
        // Unregister the input handler, then close the map-warn dialog — the client's own order,
        // and it happens before every remaining arm including `Cancelled`.
        self.close_map_warn_dialog(ui);
        match verdict.clone() {
            Capture::Cancelled | Capture::Unchanged => {
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            Capture::Refused(_) => {
                // The chord being bound is written **before** the conflict scan — step 4's
                // key the player just pressed. [`Capture::Refused`] does not carry it, so the
                // same normalisation is repeated here rather than changing that enum.
                let pending =
                    ControlChord::new(control.control, control.meta_mode, activation::CLICK);
                let text = non_user_bindable_prompt(ui, m, &pending, delimiter);
                self.open_dialog(ui, RowDialog::CantOverwrite, &text);
            }
            Capture::NeedsConfirmation { control, conflicts } => {
                self.binding_being_changed = Some(control);
                let text = overwrite_prompt(ui, m, &control, &conflicts, delimiter);
                self.open_dialog(ui, RowDialog::Overwrite, &text);
            }
            Capture::Ready { control, .. } => {
                let slot = self.slot_being_changed;
                self.set_binding(m, control, slot);
                self.binding_being_changed = None;
                self.slot_being_changed = None;
                self.refresh(ui, m, delimiter);
            }
            Capture::Ignored | Capture::Rejected => unreachable!("handled above"),
        }
        verdict
    }

    /// The action key map option control's close dialog notice.
    ///
    /// The overwrite dialog reads property `0x92` (`dereth_ui::props::attr::DIALOG_ANSWER`) out of
    /// the returned collection and, **only on yes**, calls
    /// [`Self::set_binding`] with the pending chord and slot. Both the yes and the no path
    /// then clear the pending control and slot; the can't-overwrite dialog clears them and does
    /// nothing else. A context that is neither is ignored, which is what stops one row answering
    /// another's dialog.
    ///
    /// Returns whether a binding was made.
    pub fn close_dialog(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        context: u64,
        answer: bool,
        delimiter: &str,
    ) -> bool {
        let Some(which) = self
            .dialogs
            .iter()
            .find(|(_, c)| *c == context)
            .map(|(w, _)| *w)
        else {
            return false;
        };
        self.close_dialog_context(ui, which);
        let mut bound = false;
        match which {
            RowDialog::Overwrite => {
                if answer {
                    if let Some(qc) = self.binding_being_changed {
                        bound = self.set_binding(m, qc, self.slot_being_changed);
                        self.refresh(ui, m, delimiter);
                    }
                }
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            RowDialog::CantOverwrite => {
                self.binding_being_changed = None;
                self.slot_being_changed = None;
            }
            RowDialog::MapWarn => {}
        }
        bound
    }

    /// The page's half of it.
    ///
    /// [`InputManager::set_binding`] is the map surgery, deliberately without the row's two
    /// pieces of state: *how many key buttons this row has* and *what to do when they are all
    /// full*. Both are here.
    ///
    /// * `slot` inside the current list — a **replacement**: the old control stops firing the action
    ///   and is bound to [`DO_NOTHING`] so a save-and-reload does not hand it back (see that
    ///   constant; this is the whole reason a "clear" is not a delete).
    /// * `slot` outside it, with a button free — an **addition**: the shipped rows carry several
    ///   keys per action and this is how the second one is made.
    /// * `slot` outside it, with **no** button free — a **recycle**: the head of the current list is
    ///   unbound, bound to `DoNothing` unless the incoming control conflicts with it, and dropped.
    pub fn set_binding(
        &mut self,
        m: &mut InputManager,
        control: ControlChord,
        slot: Option<usize>,
    ) -> bool {
        use dereth_input::spec::ControlCode;
        if control.control == ControlCode::INVALID || control.activation == 0 {
            return false;
        }
        let replacing = slot.filter(|i| *i < self.current.len());
        if replacing.is_none() && self.current.len() >= self.key_buttons.len() {
            // As many current bindings as key buttons — the row is full.
            let Some(head) = self.current.first().copied() else {
                return false;
            };
            m.unbind_by_key(&head, self.input_map);
            if !control.is_conflicting(&head) {
                m.bind_action(head, DO_NOTHING, self.input_map);
            }
            self.current.remove(0);
        }
        let ok = m.set_binding(self.input_map, self.action, replacing, control);
        if !ok {
            return false;
        }
        // The current list is the client's own mirror of what `find_keys_for_action` would answer; the
        // client keeps it by hand (an append, or an in-place overwrite of slot `i`) rather than
        // re-reading, and so does this, so that a row with a control the map cannot name still
        // shows what the player put there.
        match replacing {
            Some(i) => self.current[i] = control,
            None => self.current.push(control),
        }
        // Send the refresh-action-key-mapping notice — with the control
        // that was *just bound*, not with any of the controls it displaced. That is what makes the
        // one notice reach both sides: the row losing the key still lists that same control.
        self.pending_refresh_notices.push(control);
        true
    }

    /// The action key map option control's erase binding: unbind the chord, bind it to action 1,
    /// send the refresh-action-key-mapping notice, then tell the option-change handler.
    ///
    /// **Action 1 is `DoNothing`, and the binding is written rather than deleted.** That is the
    /// whole of why "clear" is not "remove": loading adds what is *absent* and merges the user
    /// file **before** the shipped default map, so a control the user file does not mention gets
    /// its shipped default back on the next run — and the player would find both keys firing.
    ///
    /// Note what it does **not** do: it does not remove the entry from the current list. The client
    /// does not either — it erases every index and *then* flushes the list, which would
    /// double-erase if each erase shortened it.
    pub fn erase_binding(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        i: usize,
        delimiter: &str,
    ) -> bool {
        let Some(qc) = self.current.get(i).copied() else {
            return false;
        };
        m.unbind_by_key(&qc, self.input_map);
        m.bind_action(qc, DO_NOTHING, self.input_map);
        self.current.remove(i);
        // Send the refresh-action-key-mapping notice.
        self.pending_refresh_notices.push(qc);
        self.refresh(ui, m, delimiter);
        true
    }

    /// [`Self::erase_binding`] over every index, then empty the current list and
    /// [`Self::refresh`].
    ///
    /// Returns how many bindings were freed.
    pub fn clear_all_bindings(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) -> usize {
        let n = self.current.len();
        for qc in std::mem::take(&mut self.current) {
            m.unbind_by_key(&qc, self.input_map);
            m.bind_action(qc, DO_NOTHING, self.input_map);
            // Retail's clear-all reaches every index through its single-binding erase, so it raises one
            // notice per freed control rather than one for the row.
            self.pending_refresh_notices.push(qc);
        }
        self.refresh(ui, m, delimiter);
        n
    }

    /// The action key map option control's refresh mappings and
    /// The refresh-action-key-mapping notice, sent when a binding is set or erased — re-read the
    /// map and redraw.
    ///
    /// This is what makes *another* row's rebind show up on this one: setting a binding displaces
    /// conflicting controls in every map that can be registered at the same time, so a row whose
    /// key was taken must be told.
    pub fn refresh_mappings(&mut self, ui: &mut UiSystem, m: &InputManager, delimiter: &str) {
        self.current = m.find_keys_for_action(self.action, self.input_map);
        self.refresh(ui, m, delimiter);
    }

    /// The action key map option control's refresh action key mapping notice — the *keyed* arm,
    /// which is the one both senders of the notice actually use.
    ///
    /// A control with the invalid key or a zero activation is ignored; otherwise the row looks the
    /// control up in its own current list and refreshes its mappings only when it is there.
    ///
    /// So a row redraws **only when the notified control is one of the controls it is currently
    /// showing** — the row that just lost the key, and the row that just took it. Every other row
    /// on the page is left alone, which is why this is not a page rebuild.
    ///
    /// Returns whether this row redrew.
    pub fn on_refresh_action_key_mapping(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        control: &ControlChord,
        delimiter: &str,
    ) -> bool {
        use dereth_input::spec::ControlCode;
        if control.control == ControlCode::INVALID || control.activation == 0 {
            return false;
        }
        if !self.current.iter().any(|c| c == control) {
            return false;
        }
        self.refresh_mappings(ui, m, delimiter);
        true
    }

    /// Same length, and exactly equal pairwise.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.saved.len() != self.current.len()
            || self
                .saved
                .iter()
                .zip(&self.current)
                .any(|(a, b)| !a.is_exactly_equal(b))
    }

    /// The save current value.
    pub fn save_current_value(&mut self) {
        self.saved.clone_from(&self.current);
    }

    /// Rebind the action to exactly the saved list.
    pub fn restore_saved_value(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) {
        self.apply_list(ui, m, self.saved.clone(), delimiter);
    }

    /// The restore default value.
    pub fn restore_default_value(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) {
        self.apply_list(ui, m, self.defaults.clone(), delimiter);
    }

    fn apply_list(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        want: Vec<ControlChord>,
        delimiter: &str,
    ) {
        for qc in self.current.clone() {
            if !want.iter().any(|w| w.is_exactly_equal(&qc)) {
                m.unbind_by_key(&qc, self.input_map);
                m.bind_action(qc, DO_NOTHING, self.input_map);
            }
        }
        m.unbind_all_by_action(self.action, self.input_map);
        for qc in &want {
            m.bind_action(*qc, self.action, self.input_map);
        }
        self.current = want;
        self.refresh(ui, m, delimiter);
    }

    /// The action key map option control's mouse turning defaults write, reached from the
    /// global-message handler's global message `0x0C`.
    ///
    /// Two rows and only two: **action `0x33` in map 5** takes `{key 0x00080101, meta 0,
    /// activation 2}` and **action `0x34` in map 5** takes `{key 0x00080201, meta 0, activation
    /// 2}` — the mouse wheel up and down, offered to the row's own key-hit handler with
    /// skip-confirmation set so the overwrite dialog is skipped. Every other row does nothing at
    /// all.
    ///
    /// Returns the chat lines the client prints, which are
    /// [`super::super::config::MOUSE_TURNING_KEY_MESSAGES`]; this is their producer.
    pub fn set_mouse_turning_defaults(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        delimiter: &str,
    ) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.input_map != InputMapId(5) {
            return out;
        }
        let row = match self.action.0 {
            0x33 => Some((dereth_input::spec::ControlCode(0x0008_0101), 0usize)),
            0x34 => Some((dereth_input::spec::ControlCode(0x0008_0201), 1usize)),
            _ => None,
        };
        let Some((cs, line)) = row else { return out };
        self.skip_confirmation = true;
        let qc = ControlChord::new(cs, 0, activation::UP);
        self.key_hit(ui, m, qc, delimiter);
        self.binding_being_changed = None;
        self.slot_being_changed = None;
        self.skip_confirmation = false;
        out.push(super::super::config::MOUSE_TURNING_KEY_MESSAGES[line]);
        out
    }

    /// The open context for one of this row's three dialogs.
    #[must_use]
    pub fn dialog_context(&self, which: RowDialog) -> Option<u64> {
        self.dialogs
            .iter()
            .find(|(w, _)| *w == which)
            .map(|(_, c)| *c)
    }

    /// The `Make*Dialog` body every one of the three shares: refuse when this row already has one
    /// of that kind open, otherwise fill a `PropertyCollection` and hand
    /// it to the dialog factory.
    ///
    /// `0xC3` is [`DIALOG_QUEUE`], which is the property set
    /// second and the one the element-message handler's open-dialog check reads.
    fn open_dialog(&mut self, ui: &mut UiSystem, which: RowDialog, text: &str) -> bool {
        if self.dialog_context(which).is_some() {
            return false;
        }
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(dialog_kind(which).property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_QUEUE_ID,
            dereth_assets::ui::PropertyValue::Integer(
                i32::try_from(DIALOG_QUEUE).unwrap_or(i32::MAX),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            dereth_assets::ui::PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(text.to_string()),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        self.dialogs.push((which, context));
        true
    }

    fn close_dialog_context(&mut self, ui: &mut UiSystem, which: RowDialog) {
        let Some(i) = self.dialogs.iter().position(|(w, _)| *w == which) else {
            return;
        };
        let (_, context) = self.dialogs.remove(i);
        if let Some(h) = ui.dialogs.close_dialog(context, ui.now.0) {
            ui.remove_and_delete_root(h);
        }
    }
}

/// The kind each of the three dialogs is.
///
/// The map warn is a `Wait`: it has no buttons, it is modal, and the way out of it is a key press
/// or Escape, which is the key-hit handler's `Cancelled` arm. The overwritable-conflict dialog is a
/// `Confirmation`; it reads its answer from property `0x92`.
/// The can't-overwrite notice is a one-button `Message` and carries no answer property.
///
/// Each dialog's property collection carries four properties, in this order: `0x8E` (the kind),
/// `0xC3` (the queue), `0xAC` (modal) and `0xC5` (the text).
///
/// So `MapWarn` is `Wait`, and the four properties this build sets are exactly the
/// four the client sets. The overwritable-conflict dialog uses kind `1`,
/// while the cannot-overwrite dialog uses kind `3`.
#[must_use]
pub const fn dialog_kind(which: RowDialog) -> dereth_ui::dialog::DialogKind {
    match which {
        RowDialog::MapWarn => dereth_ui::dialog::DialogKind::Wait,
        RowDialog::Overwrite => dereth_ui::dialog::DialogKind::Confirmation,
        RowDialog::CantOverwrite => dereth_ui::dialog::DialogKind::Message,
    }
}
