//! The key-binding page: its rows, its buttons and its file dialogs.

use super::*;

// ---------------------------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------------------------

/// What [`KeyBindingPage::on_page_element_message`] did — one variant per arm of
/// the keyboard panel's element-message handler, so a caller can assert on the arm rather
/// than on a side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    /// The *OK* button — `save_current_values` over `rows` rows, having raised
    /// [`crate::UiRequest::SaveKeyMap`] iff `saved` (changed).
    Applied { rows: usize, saved: bool },
    /// *Cancel* or *Revert to Saved* — `restore_saved_values` wrote this many rows.
    RestoredSaved(usize),
    /// *Restore Defaults* — `restore_default_values` wrote this many rows.
    RestoredDefaults(usize),
    /// The load-keymap dialog. Not raised by this build; recorded so the arm is
    /// attributed rather than silent.
    LoadKeymapDialog,
    /// The save keymap dialog build. As above.
    SaveKeymapDialog,
    /// A non-cancel answer the close load keymap dialog handling.
    LoadKeymap(String),
    /// A non-empty answer the close save keymap dialog handling.
    SaveKeymap(String),
    /// Yes; No produces no host event.
    OverwriteKeymap(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDialogKind {
    Load,
    Save,
    Overwrite,
    ReadOnly,
}

#[derive(Debug, Clone)]
struct FileDialogState {
    context: u64,
    kind: FileDialogKind,
    files: Vec<String>,
    selected: i32,
    name: Option<String>,
    popup: Option<ElemHandle>,
}

/// State `0x0D` — the disabled-state guard applies to
/// *OK* and *Cancel* and to nothing else on this page.
#[must_use]
fn button_enabled(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).is_some_and(|n| n.state != DISABLED_STATE)
}

/// The keyboard page (type `0x1000000E`) — six list boxes of key-binding rows.
#[derive(Debug, Clone, Default)]
pub struct KeyBindingPage {
    /// The keyboard-page element.
    pub page: Option<ElemHandle>,
    /// The list-box table — `(action class, the list box on that class's tab page)`.
    pub list_boxes: Vec<(u32, ListBoxWidget)>,
    /// The option rows, one entry per bindable `(input map, action)`.
    pub rows: Vec<ActionKeyMapRow>,
    /// How many template-0 section headers were added — one per `(action class, input map)` pair.
    pub headers: usize,
    /// Template-list inserts that produced nothing, and rows whose post-init failed.
    pub failures: usize,
    /// The Load Keymap / Save Keymap buttons.
    pub load_button: Option<ElemHandle>,
    pub save_button: Option<ElemHandle>,
    /// The keymap filename label, attribute [`page_attr::KEYMAP_FILENAME_LABEL`].
    pub filename_label: Option<ElemHandle>,
    /// The Reset-to-Defaults button — attribute
    /// [`page_attr::RESET_TO_DEFAULTS_BUTTON`].
    pub reset_defaults_button: Option<ElemHandle>,
    /// The Revert-to-Saved button.
    pub revert_to_saved_button: Option<ElemHandle>,
    /// The OK button.
    pub ok_button: Option<ElemHandle>,
    /// The Cancel button.
    pub cancel_button: Option<ElemHandle>,
    /// The delimiter [`binding_label`] joins with, resolved once from
    /// [`token::KEY_DESC_DELIMITER`] or [`DEFAULT_KEY_DESC_DELIMITER`].
    pub delimiter: String,
    /// The text element inside each template-0 section header, in the order
    /// [`Self::init_options`] added them.
    ///
    /// Kept so the titles can be read **off the arena** rather than off a cached string: if the
    /// caption written there were the `ID_InputMap_*` token itself, a test that asserted the value
    /// this page had computed would still pass.
    pub header_elements: Vec<ElemHandle>,
    file_dialog: Option<FileDialogState>,
}

impl KeyBindingPage {
    /// The mouse-turning settings' key half, offered to every row: the two camera-zoom rows take
    /// the mouse wheel and the rest do nothing. Returns the chat lines the two print.
    pub fn set_mouse_turning_defaults(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
    ) -> Vec<&'static str> {
        let delimiter = self.delimiter.clone();
        let mut lines = Vec::new();
        for row in &mut self.rows {
            lines.extend(row.set_mouse_turning_defaults(ui, m, &delimiter));
        }
        lines
    }

    /// Instantiate the pending dialog elements owned by this page's rows.
    ///
    /// `ActionKeyMapRow::open_dialog` creates the retail factory context and queue entry. The
    /// factory deliberately cannot create the element itself because it has no asset source;
    /// the element half belongs to the current framework, and this is it. Without it a click
    /// registers capture while putting nothing on screen.
    ///
    /// The same dialog-creation path serves the no-button map warning, the overwrite
    /// Confirmation and the can't-overwrite Message.
    pub(crate) fn service_dialog_elements(&mut self, ui: &mut UiSystem) -> Vec<ElemHandle> {
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        let mut contexts: Vec<u64> = self
            .rows
            .iter()
            .flat_map(|row| row.dialogs.iter().map(|(_, context)| *context))
            .filter(|context| owed.contains(context))
            .collect();
        if let Some(context) = self.file_dialog.as_ref().map(|dialog| dialog.context) {
            if owed.contains(&context) {
                contexts.push(context);
            }
        }
        let mut roots = Vec::new();
        for context in contexts {
            let Some(info) = ui.dialogs.info(context).cloned() else {
                continue;
            };
            let Ok(h) = ui.require_env().and_then(|e| {
                e.create_and_add_root_element(ui, DIALOG_LAYOUT, info.kind.root_element_id())
            }) else {
                continue;
            };
            ui.set_attribute_bool(
                h,
                dereth_ui::props::attr::DIALOG_MODAL,
                info.data
                    .get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                    .unwrap_or(false),
            );
            if let Some(dereth_assets::ui::PropertyValue::String(text)) =
                info.data.get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
            {
                if let Some(t) = ui
                    .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                    .and_then(|child| ui.text_element_mut(child))
                {
                    t.set_text(text);
                }
            }
            dereth_ui::dialog::types::set_dialog_data(ui, h, &info.data);
            if let Some(dialog) = self.file_dialog.as_mut().filter(|d| d.context == context) {
                if dialog.kind == FileDialogKind::Load {
                    let menu = ui
                        .get_child_recursive(h, dereth_ui::dialog::base::child::CONFIRM_MENU_MENU);
                    if let Some(menu) = menu {
                        let popup = ui.env().cloned().and_then(|e| {
                            e.with_assets(|assets| {
                                let popup = dereth_ui::widgets::menu::make_popup(ui, assets, menu)?;
                                dereth_ui::widgets::menu::initialize_popup(ui, menu)?;
                                for name in &dialog.files {
                                    dereth_ui::widgets::menu::add_text_item(
                                        ui, assets, menu, name,
                                    )?;
                                }
                                Some(popup)
                            })
                        });
                        if let Some(popup) = popup {
                            dereth_ui::widgets::menu::set_selected_index(ui, menu, dialog.selected);
                            dialog.popup = Some(popup);
                            roots.push(popup);
                        }
                    }
                }
            }
            dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
            if ui.bind_dialog_element(context, h) {
                roots.push(h);
            } else {
                ui.remove_and_delete_root(h);
            }
        }
        roots
    }

    /// The load-keymap dialog: kind 7, queue `0x10000001`, modal, with one menu row
    /// per `*.keymap` basename and the current file preselected.
    pub fn open_load_keymap_dialog(
        &mut self,
        ui: &mut UiSystem,
        files: Vec<String>,
        current: Option<&str>,
    ) -> bool {
        let selected = current
            .and_then(|name| {
                files
                    .iter()
                    .position(|file| file.eq_ignore_ascii_case(name))
            })
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1);
        self.open_file_dialog(ui, FileDialogKind::Load, files, selected, None)
    }

    /// The save-keymap dialog: kind 5 on the same modal queue.
    pub fn open_save_keymap_dialog(&mut self, ui: &mut UiSystem) -> bool {
        self.open_file_dialog(ui, FileDialogKind::Save, Vec::new(), -1, None)
    }

    /// The overwrite-keymap dialog: kind 1 Confirmation with `KEYMAP = name`.
    pub fn open_overwrite_keymap_dialog(&mut self, ui: &mut UiSystem, name: String) -> bool {
        self.open_file_dialog(ui, FileDialogKind::Overwrite, Vec::new(), -1, Some(name))
    }

    /// The cant overwrite read only keymap dialog build: kind 3 one-button Message.
    pub fn open_read_only_keymap_dialog(&mut self, ui: &mut UiSystem, name: String) -> bool {
        self.open_file_dialog(ui, FileDialogKind::ReadOnly, Vec::new(), -1, Some(name))
    }

    fn open_file_dialog(
        &mut self,
        ui: &mut UiSystem,
        kind: FileDialogKind,
        files: Vec<String>,
        selected: i32,
        name: Option<String>,
    ) -> bool {
        if self.file_dialog.is_some() || ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
            return false;
        }
        let (dialog_kind, prompt) = match kind {
            FileDialogKind::Load => (
                dereth_ui::dialog::DialogKind::ConfirmationMenu,
                token::LOAD_KEYMAP_LABEL,
            ),
            FileDialogKind::Save => (
                dereth_ui::dialog::DialogKind::ConfirmationTextInput,
                token::SAVE_KEYMAP_LABEL,
            ),
            FileDialogKind::Overwrite => (
                dereth_ui::dialog::DialogKind::Confirmation,
                token::OVERWRITE_KEYMAP_LABEL,
            ),
            FileDialogKind::ReadOnly => (
                dereth_ui::dialog::DialogKind::Message,
                token::CANT_OVERWRITE_READ_ONLY_KEYMAP_LABEL,
            ),
        };
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(dialog_kind.property()),
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
        let prompt = match name.as_deref() {
            Some(name) => {
                let id = dereth_primitives::num::hash::str_hash(prompt.as_bytes());
                // The two prompts that take a file name — `ID_KeyMapOverwriteKeymap_Label` and
                // `ID_KeyMapCantOverwriteReadOnlyKeymap_Label` — both list `KEYMAP` [measured,
                // 0x23000004].
                ui.resolve_string_named(
                    string_table(ui, STRING_TABLE_ENUM),
                    id,
                    &[(var::KEYMAP, name)],
                )
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| name.to_owned())
            }
            None => {
                resolve_token(ui, STRING_TABLE_ENUM, prompt).unwrap_or_else(|| prompt.to_owned())
            }
        };
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(prompt),
        );
        if kind == FileDialogKind::Load {
            data.set(
                dereth_ui::props::attr::DIALOG_CONFIRM_MENU_ANSWER,
                dereth_assets::ui::PropertyValue::Integer(selected),
            );
        }
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        self.file_dialog = Some(FileDialogState {
            context,
            kind,
            files,
            selected,
            name,
            popup: None,
        });
        true
    }

    /// Harvest the client's kind-5/kind-7 answer arms.
    pub(crate) fn service_file_dialog_answer(&mut self, ui: &mut UiSystem) -> Option<PageEvent> {
        let state = self.file_dialog.as_ref()?;
        let root = ui
            .dialogs
            .info(state.context)
            .and_then(|info| info.element)?;
        let dialog = dereth_ui::dialog::types::dialog_element(ui, root)?;
        let answered = dialog.answer.is_some();
        let answer = dialog.answer_property();
        let event = match (state.kind, answer.map(|answer| answer.1)) {
            (FileDialogKind::Save, Some(dereth_assets::ui::PropertyValue::String(name)))
                if !name.is_empty() =>
            {
                Some(PageEvent::SaveKeymap(name))
            }
            (FileDialogKind::Load, Some(dereth_assets::ui::PropertyValue::Integer(index)))
                if index >= 0 =>
            {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| state.files.get(index).cloned())
                    .map(PageEvent::LoadKeymap)
            }
            (FileDialogKind::Overwrite, Some(dereth_assets::ui::PropertyValue::Bool(true))) => {
                state.name.clone().map(PageEvent::OverwriteKeymap)
            }
            _ => None,
        };
        if !answered {
            return None;
        }
        let state = self
            .file_dialog
            .take()
            .expect("the file-dialog state still exists");
        if let Some(root) = ui.dialogs.close_dialog(state.context, ui.now.0) {
            ui.remove_and_delete_root(root);
        }
        if let Some(popup) = state.popup {
            ui.remove_and_delete_root(popup);
        }
        event
    }

    /// Put the keymap file name on the page's filename label.
    pub fn refresh_keymap_file_name(&self, ui: &mut UiSystem, name: Option<&str>) {
        if let Some(label) = self.filename_label {
            set_literal(ui, label, name.unwrap_or_default());
        }
    }

    /// Deliver physical conflict-dialog answers to the client's row.
    ///
    /// The confirmation dialog records property `0x92`; the message dialog has no answer property and
    /// its one button is itself the completion. MapWarn is closed by the captured key path and is
    /// intentionally not harvested here.
    pub(crate) fn service_dialog_answers(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
    ) -> usize {
        let mut answers = Vec::new();
        for (row_index, row) in self.rows.iter().enumerate() {
            for &(which, context) in &row.dialogs {
                let Some(root) = ui.dialogs.info(context).and_then(|info| info.element) else {
                    continue;
                };
                let Some(dialog) = dereth_ui::dialog::types::dialog_element(ui, root) else {
                    continue;
                };
                let answer = match which {
                    RowDialog::Overwrite => dialog.answer_property().and_then(|(_, value)| {
                        if let dereth_assets::ui::PropertyValue::Bool(answer) = value {
                            Some(answer)
                        } else {
                            None
                        }
                    }),
                    RowDialog::CantOverwrite => dialog.answer.is_some().then_some(false),
                    RowDialog::MapWarn => None,
                };
                if let Some(answer) = answer {
                    answers.push((row_index, context, which, answer));
                }
            }
        }
        let answered = answers.len();
        let delimiter = self.delimiter.clone();
        for (row_index, context, which, answer) in answers {
            if which == RowDialog::Overwrite {
                ui.dialogs.set_answer_property(
                    context,
                    dereth_ui::props::attr::DIALOG_ANSWER,
                    dereth_assets::ui::PropertyValue::Bool(answer),
                );
            }
            if self.rows[row_index].close_dialog(ui, m, context, answer, &delimiter) {
                self.on_option_changed(ui);
            }
        }
        // The client's Yes arm calls, whose last
        // act is the refresh notice. Without this the row that lost the key keeps drawing it.
        self.drain_refresh_notices(ui, m);
        answered
    }

    /// The keyboard panel's post-init.
    ///
    /// Reads [`page_attr::LIST_BOX_CHILD`] **once**, then walks the six tab pages of
    /// [`KEYBOARD_TAB_PAGES`] in the client's order, looking that child up inside each and
    /// registering it under one action class. Every page in the shipped tree carries the same
    /// child ids, which is exactly why the lookup is scoped to the page and not to the window:
    /// a recursive child search from the window would answer the first page six times.
    #[must_use]
    pub fn bind(ui: &UiSystem, page: ElemHandle) -> Self {
        let mut p = Self {
            page: Some(page),
            delimiter: DEFAULT_KEY_DESC_DELIMITER.to_string(),
            ..Self::default()
        };
        p.load_button = crate::bind::attr_enum(ui, page, page_attr::LOAD_KEYMAP_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.save_button = crate::bind::attr_enum(ui, page, page_attr::SAVE_KEYMAP_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.filename_label = crate::bind::attr_enum(ui, page, page_attr::KEYMAP_FILENAME_LABEL)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        // The four buttons the element-message handler compares against and that post-init
        // binds. Without them Apply, Cancel, *Restore Defaults* and *Revert to Saved* have nothing
        // to be compared against and the page cannot answer any of them.
        p.reset_defaults_button =
            crate::bind::attr_enum(ui, page, page_attr::RESET_TO_DEFAULTS_BUTTON)
                .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.revert_to_saved_button =
            crate::bind::attr_enum(ui, page, page_attr::REVERT_TO_SAVED_BUTTON)
                .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.ok_button = crate::bind::attr_enum(ui, page, page_attr::OK_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        p.cancel_button = crate::bind::attr_enum(ui, page, page_attr::CANCEL_BUTTON)
            .and_then(|id| ui.get_child_recursive(page, ElementId(id)));
        // The client reads `ID_KeyDescDelimiter` out of string
        // table enum 3 at every call. Resolving it once here is the only deviation, and the value
        // it resolves to against the shipped dats is `"+"` — the same string
        // [`DEFAULT_KEY_DESC_DELIMITER`] falls back to, so a host with no asset source is not
        // silently joining `Shift` and `W` with nothing.
        if let Some(d) = resolve_token(ui, table_enum::KEY_DESC, token::KEY_DESC_DELIMITER) {
            p.delimiter = d;
        }
        let Some(child) = crate::bind::attr_enum(ui, page, page_attr::LIST_BOX_CHILD) else {
            return p;
        };
        for (tab, (class, _)) in KEYBOARD_TAB_PAGES.iter().zip(ACTION_CLASSES) {
            let Some(tab_h) = ui.get_child_recursive(page, *tab) else {
                continue;
            };
            let Some(lb) = ui.get_child_recursive(tab_h, ElementId(child)) else {
                continue;
            };
            p.list_boxes.push((class, ListBoxWidget::bind(ui, lb)));
        }
        p
    }

    /// The keyboard panel's option build.
    ///
    /// 1. Flush every list box and reset the option rows.
    /// 2. Walk the input maps, keeping only actions
    ///    accepts, and group them `action class -> input map -> [action]` in walk order.
    /// 3. For each group, add template 0 as the section header and set its text with
    ///    ([`super::super::pages::input_map_caption`]), then one
    ///    template-1 row per action through.
    ///
    /// The bindings each row starts with come from `InputManager::find_keys_for_action` on the
    /// **merged** master map — so the rows show what the
    /// player's keymap says and not what the shipped default says, and that is the acceptance's
    /// oracle.
    ///
    /// Returns how many rows were built.
    pub fn init_options(&mut self, ui: &mut UiSystem, m: &InputManager) -> usize {
        for (_, lb) in &mut self.list_boxes {
            lb.flush(ui);
        }
        self.rows.clear();
        self.headers = 0;
        self.failures = 0;
        self.header_elements.clear();
        // `action class -> [(input map, [action])]`, in input-map walk order.
        // `entries()` is the input maps' own walk order — map by map, action by action — which
        // is the order the option build fills its nested hash in and therefore the order the rows
        // appear in.
        //
        // The rows are the ones both interfaces' key pages list
        // ([`dereth_input::presentation`]): the bindable entries but the hidden quickslots and
        // the quest detail panel, with Disable Most Weather Effects among the character options.
        // A row this interface does nothing with is not listed.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        let mut groups: Vec<(u32, Vec<(InputMapId, Vec<ActionId>)>)> = Vec::new();
        for (map, action, v) in m.action_map.entries().collect::<Vec<_>>() {
            let Some(row) = presentation::find(map, action) else {
                continue;
            };
            let class = presentation::modern_class(map, action).unwrap_or(v.action_class);
            if !row.shown(Interface::Modern) {
                continue;
            }
            let ci = match groups.iter().position(|(c, _)| *c == class) {
                Some(i) => i,
                None => {
                    groups.push((class, Vec::new()));
                    groups.len() - 1
                }
            };
            let maps = &mut groups[ci].1;
            match maps.iter_mut().find(|(k, _)| *k == map) {
                Some((_, v)) => v.push(action),
                None => maps.push((map, vec![action])),
            }
        }
        for (class, maps) in groups {
            let Some(bi) = self.list_boxes.iter().position(|(c, _)| *c == class) else {
                continue;
            };
            for (map, actions) in maps {
                // Reading string info for an input-map id returns a string reference —
                // the hashed `ID_InputMap_…` token in table enum 7 — and the page's option
                // initialisation resolves it and sets it as the header text.
                // Writing the **token** onto the header instead would make a section title read
                // `ID_InputMap_MovementCommands`. An id with no switch arm leaves
                // the string reference empty, it is not valid and **no text is
                // set at all** — which is why the miss below writes nothing rather than a fallback.
                let caption = if map == dereth_input::dereth::INPUT_MAP {
                    // This client's own actions, under a heading of their own.
                    Some(dereth_input::dereth::SECTION_NAME.to_owned())
                } else {
                    super::super::pages::input_map_caption(map.0)
                        .and_then(|tok| resolve_token(ui, table_enum::INPUT_MAP, tok))
                };
                match self.list_boxes[bi]
                    .1
                    .add_from_template(ui, TEMPLATE_HEADER, None)
                {
                    Some(h) => {
                        if let Some(c) = caption {
                            set_literal(ui, h, &c);
                        }
                        self.header_elements.push(h);
                        self.headers += 1;
                    }
                    None => self.failures += 1,
                }
                for action in actions {
                    self.add_action_key_map(ui, m, bi, map, action);
                }
            }
            self.list_boxes[bi].1.update_layout(ui);
        }
        // Saving current values here is what makes `changed()` false on a freshly built page, and
        // therefore what makes *Revert to Saved* open greyed. Without it every row's saved list is
        // whatever initialisation left and the button's lit state is never computed at all.
        self.save_current_values();
        self.on_option_changed(ui);
        self.rows.len()
    }

    /// The **only** thing that lights or greys
    /// *Revert to Saved*.
    ///
    /// With no revert button nothing is written; otherwise the button gets state 1 when the page
    /// has changed and state `0x0D` when it has not.
    ///
    /// Two call sites reach this through the change handler installed during panel setup.
    /// This build owns that edge directly,
    /// so the page calls it after every gesture that
    /// can change a row instead; same edge, one indirection fewer. Returns the state written.
    pub fn on_option_changed(&mut self, ui: &mut UiSystem) -> Option<StateId> {
        let h = self.revert_to_saved_button?;
        let state = if self.changed() {
            ENABLED_STATE
        } else {
            DISABLED_STATE
        };
        ui.set_state(h, state);
        Some(state)
    }

    /// The keyboard panel's action key map insert — template 1, runtime type check,
    /// row initialisation, option registration.
    fn add_action_key_map(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        bi: usize,
        map: InputMapId,
        action: ActionId,
    ) {
        let Some(h) = self.list_boxes[bi]
            .1
            .add_from_template(ui, TEMPLATE_ROW, None)
        else {
            self.failures += 1;
            return;
        };
        // The runtime type check the insert performs before initialisation: a template whose root is not
        // an action-key-map option registers nothing, which is the loud form of "the layout
        // changed".
        if ui.node(h).map(|n| n.ty()) != Some(crate::element_types::ty::OPTION_ACTION_KEY_MAP) {
            self.failures += 1;
            return;
        }
        let Some(mut row) = ActionKeyMapRow::post_init(ui, h) else {
            self.failures += 1;
            return;
        };
        let (name, tip) = action_description(ui, m, map, action);
        set_literal(ui, h, &name);
        // Initialization takes the defaults as its fifth argument and fills the current list
        // itself from `find_keys_for_action` on the *merged* map. The two lists are different
        // maps; using one for both would make *Restore Defaults* restore whatever the page was
        // opened with.
        let defaults = m.default_keys_for_action(action, map);
        row.init(
            action,
            map,
            name,
            tip,
            m.find_keys_for_action(action, map),
            defaults,
        );
        row.refresh(ui, m, &self.delimiter);
        self.rows.push(row);
    }

    /// Fan the element message out to the row that owns the element, which is
    /// the client's walk of the option rows.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
    ) -> Option<(usize, RowEvent)> {
        let d = self.delimiter.clone();
        for i in 0..self.rows.len() {
            if let Some(e) = self.rows[i].on_element_message(ui, m, msg, &d) {
                // The client's last two acts are the refresh-action-key-mapping notice for the
                // chord, then the change handler's option-changed call.
                self.drain_refresh_notices(ui, m);
                self.on_option_changed(ui);
                return Some((i, e));
            }
        }
        None
    }

    /// The refresh-action-key-mapping notice — every row re-reads the map.
    ///
    /// This is the **unkeyed** reload-options sweep. The notice the two binding call sites
    /// actually send carries a control and is
    /// handled by
    /// [`Self::send_notice_refresh_action_key_mapping`].
    pub fn on_refresh_action_key_mapping(&mut self, ui: &mut UiSystem, m: &InputManager) {
        let d = self.delimiter.clone();
        for r in &mut self.rows {
            r.refresh_mappings(ui, m, &d);
        }
    }

    /// The refresh-action-key-mapping notice, as far as this page is
    /// concerned: offer `control` to every row, and let
    /// [`ActionKeyMapRow::on_refresh_action_key_mapping`] decide which of them is
    /// showing it.
    ///
    /// The client walks the global notice-handler registry, skips engine handlers,
    /// and delivers the refresh to each option handler. Every live
    /// action-key-map row is one of those handlers, including the row that sent it.
    ///
    /// Returns how many rows redrew.
    pub fn send_notice_refresh_action_key_mapping(
        &mut self,
        ui: &mut UiSystem,
        m: &InputManager,
        control: &ControlChord,
    ) -> usize {
        let d = self.delimiter.clone();
        let mut n = 0;
        for r in &mut self.rows {
            if r.on_refresh_action_key_mapping(ui, m, control, &d) {
                n += 1;
            }
        }
        n
    }

    /// Deliver every notice the rows raised during this gesture.
    ///
    /// Setting and erasing a binding send synchronously, before returning; the row
    /// cannot reach its siblings here, so it queues in
    /// [`ActionKeyMapRow::pending_refresh_notices`] and the page flushes that at the end of the
    /// same page-level call. The gesture that raised it and the redraw are one frame either way.
    ///
    /// Returns how many row redraws the notices caused.
    pub fn drain_refresh_notices(&mut self, ui: &mut UiSystem, m: &InputManager) -> usize {
        let mut pending: Vec<ControlChord> = Vec::new();
        for r in &mut self.rows {
            pending.append(&mut r.pending_refresh_notices);
        }
        let mut n = 0;
        for control in pending {
            n += self.send_notice_refresh_action_key_mapping(ui, m, &control);
        }
        n
    }

    /// The keyboard panel's restore-defaults operation — the
    /// *Restore Defaults* button.
    ///
    /// The override first clears the keymap and then performs two default-map additions, which
    /// rebuild the master input map from the shipped defaults; what reaches the rows is
    /// the option page's restore-defaults, an **unconditional** walk of
    /// the option array calling each option's restore-default operation. Every row,
    /// changed or not — the same shape `PlayerOptionPage`'s Defaults has.
    ///
    /// Returns how many rows were written, which is a denominator rather than a bare "it ran".
    ///
    /// The override's own two lines clear the keymap, then add maps `0x10000001` and `1`,
    /// i.e. [`InputManager::reload_defaults`]. Without them the walk below would write
    /// each row's default list back over the merged map and leave every binding the shipped maps
    /// do *not* mention exactly where the player had put it.
    pub fn restore_default_values(&mut self, ui: &mut UiSystem, m: &mut InputManager) -> usize {
        m.reload_defaults();
        let d = self.delimiter.clone();
        for r in &mut self.rows {
            r.restore_default_value(ui, m, &d);
        }
        self.rows.len()
    }

    /// The option page's save-current-values operation — the *OK* button's
    /// second half.
    ///
    /// A snapshot, not a write: each row copies its current list
    /// into its saved list, which is what gives Cancel something to revert to. Returns how many rows
    /// were snapshotted.
    pub fn save_current_values(&mut self) -> usize {
        for r in &mut self.rows {
            r.save_current_value();
        }
        self.rows.len()
    }

    /// The option page's restore-saved-values operation — *Cancel* and
    /// *Revert to Saved*, which invoke the same behavior from two different buttons.
    ///
    /// **Conditional, unlike Defaults**: the walk checks [`ActionKeyMapRow::changed`] first and
    /// only then restores the saved value, so a row that never moved is not rebound. Returns how
    /// many rows were written.
    pub fn restore_saved_values(&mut self, ui: &mut UiSystem, m: &mut InputManager) -> usize {
        let d = self.delimiter.clone();
        let mut n = 0;
        for r in &mut self.rows {
            if r.changed() {
                r.restore_saved_value(ui, m, &d);
                n += 1;
            }
        }
        n
    }

    /// *Any* row's [`ActionKeyMapRow::changed`], short-circuiting on the first.
    ///
    /// This is the flag the *OK* arm gates the keymap save on: an unchanged page is applied
    /// without rewriting the keymap file.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.rows.iter().any(ActionKeyMapRow::changed)
    }

    /// The **page's own** four buttons, as
    /// opposed to [`Self::on_element_message`]'s fan-out to the rows.
    ///
    /// Nothing happens while a dialog is open on queue `0x10000001`. Message 1 on Load Keymap or
    /// Save Keymap opens that dialog, on Reset to Defaults restores defaults, and on Revert to
    /// Saved restores the saved values. Message `0x19` with first parameter 7 on OK (not in state
    /// `0x0D`) saves the keymap file under its current name, without the overwrite prompt, if
    /// anything changed, then saves the current values; on Cancel (not in state `0x0D`) it
    /// restores the saved values.
    ///
    /// The asymmetry is the client's and is load-bearing: *Restore Defaults* and *Revert to Saved*
    /// answer message **1** (`BUTTON_CLICKED`), while *OK* and *Cancel* answer message **0x19**
    /// with first parameter 7 (`PRIMARY_CLICK`) and are additionally refused while their element is
    /// in state `0x0D`.
    ///
    /// The arm is raised as [`crate::UiRequest::SaveKeyMap`] rather than
    /// written here: the writer is the input manager's keymap save over the keymap file, which
    /// lives in the host (`dereth_client_shell::input::InputShell::save_keymap`) and not in a `Screen`.
    /// The overwrite prompt the keymap save can raise belongs to the *Save Keymap As* path, which
    /// asks for it; this arm does not. The read-only refusal is not gated that way: retail checks
    /// that first on every save, so OK raises the can't-overwrite dialog too when the current
    /// keymap file exists and is not writable.
    pub fn on_page_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        msg: &dereth_ui::ElementMessage,
    ) -> Option<PageEvent> {
        use dereth_ui::msg::element::id;
        if msg.id == id::BUTTON_CLICKED {
            if ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
                return None;
            }
            if Some(msg.source) == self.load_button {
                return Some(PageEvent::LoadKeymapDialog);
            }
            if Some(msg.source) == self.save_button {
                return Some(PageEvent::SaveKeymapDialog);
            }
            if Some(msg.source) == self.reset_defaults_button {
                let n = self.restore_default_values(ui, m);
                self.on_option_changed(ui);
                return Some(PageEvent::RestoredDefaults(n));
            }
            if Some(msg.source) == self.revert_to_saved_button {
                let n = self.restore_saved_values(ui, m);
                self.on_option_changed(ui);
                return Some(PageEvent::RestoredSaved(n));
            }
            return None;
        }
        if msg.id != dereth_ui::msg::element::id::MOUSE_CLICK
            || msg.p1 != dereth_ui::focus::action::PRIMARY_CLICK
        {
            return None;
        }
        if ui.dialogs.is_dialog_open(DIALOG_QUEUE) {
            return None;
        }
        if Some(msg.source) == self.ok_button {
            if !button_enabled(ui, msg.source) {
                return None;
            }
            let changed = self.changed();
            if changed {
                ui.requests.emit(crate::UiRequest::SaveKeyMap);
            }
            let rows = self.save_current_values();
            self.on_option_changed(ui);
            return Some(PageEvent::Applied {
                rows,
                saved: changed,
            });
        }
        if Some(msg.source) == self.cancel_button {
            if !button_enabled(ui, msg.source) {
                return None;
            }
            let n = self.restore_saved_values(ui, m);
            self.on_option_changed(ui);
            return Some(PageEvent::RestoredSaved(n));
        }
        None
    }

    /// The input manager's key-hit handler call, routed to the row that asked for
    /// it, with the option-changed callback on the far side.
    ///
    /// The client reaches it through the row's registered key-hit handler, which is the *row's*
    /// own input handler; this build keeps one
    /// exclusive slot on the manager and finds the capturing row by asking, which is the same
    /// exclusivity. The option-changed notification half is here too, because the binding write
    /// calls it.
    pub fn key_hit(
        &mut self,
        ui: &mut UiSystem,
        m: &mut InputManager,
        control: ControlChord,
    ) -> Option<Capture> {
        let i = self.rows.iter().position(ActionKeyMapRow::capturing)?;
        let d = self.delimiter.clone();
        let verdict = self.rows[i].key_hit(ui, m, control, &d);
        self.drain_refresh_notices(ui, m);
        self.on_option_changed(ui);
        Some(verdict)
    }

    /// The row that carries `element`, if any.
    #[must_use]
    pub fn row_for(&self, element: ElemHandle) -> Option<usize> {
        self.rows.iter().position(|r| r.element == Some(element))
    }

    /// The row bound to one `(input map, action)`.
    #[must_use]
    pub fn row_of(&self, map: InputMapId, action: ActionId) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.input_map == map && r.action == action)
    }
}
