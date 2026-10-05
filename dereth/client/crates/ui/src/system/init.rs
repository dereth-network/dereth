//! Construction and installed services.

use crate::{
    dialog, env, factory, focus, media, msg, text, Box2D, DescLibrary, ElemHandle, PropertyTypes,
    UiError, UiSystem,
};
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{DataId, LocalTime};
use std::collections::BTreeMap;

impl UiSystem {
    /// Give this UI the asset source and resolver its screens build from, or take it away.
    pub fn set_env(&mut self, env: Option<env::Env>) {
        self.env = env;
    }

    /// The environment [`Self::set_env`] installed, if any.
    #[must_use]
    pub fn env(&self) -> Option<&env::Env> {
        self.env.as_ref()
    }

    /// A clone of the installed environment, or the loud [`UiError::Persist`] a screen
    /// constructed before the dat is open has always answered with. A clone, so the caller can go
    /// on to borrow this `UiSystem` mutably (every `Env` builder takes `&mut UiSystem`); the clone
    /// shares the original's asset source, resolver and memo.
    pub fn require_env(&self) -> Result<env::Env, UiError> {
        self.env.clone().ok_or_else(env::no_env)
    }

    /// The element manager's init, minus the parts that belong to other tracks (the
    /// preference registration, the render callback, the input-map push and the console command).
    ///
    /// Step 5 of `Init` creates the root as a hollow element with no parent, marks it activatable
    /// and pushes it onto the activatable-element list.
    #[must_use]
    pub fn new(display: (i32, i32)) -> Self {
        let mut s = Self {
            slots: Vec::new(),
            free: Vec::new(),
            root: ElemHandle::for_test(0),
            element_list: Vec::new(),
            delete_queue: Vec::new(),
            factories: BTreeMap::new(),
            lib: DescLibrary::new(),
            property_types: PropertyTypes::new(),
            property_defaults: BTreeMap::new(),
            display,
            element_listeners: BTreeMap::new(),
            global_listeners: BTreeMap::new(),
            notices: msg::notice::NoticeBus::new(),
            serial: 0,
            serial_seen: BTreeMap::new(),
            broadcasting: false,
            lifted: 0,
            deferred_states: Vec::new(),
            deferred_mouse_visibility: Vec::new(),
            deferred_relinquish: Vec::new(),
            removals: Vec::new(),
            outbox: Vec::new(),
            mouse: focus::MouseState::default(),
            queued_messages: Vec::new(),
            queued_visibility: Vec::new(),
            tooltip: dialog::tooltip::TooltipState::default(),
            dialogs: dialog::DialogController::new(),

            drag: focus::DragState::default(),
            focus_element: None,
            active_element: None,
            activatable: Vec::new(),
            input_action_listeners: BTreeMap::new(),
            default_cursor: None,
            last_cursor: None,
            pending_cursor: None,
            shift_key_down: false,
            clipboard: String::new(),
            pending_clipboard: None,
            rng: Ran2::new(1),
            easing: media::level_array(),
            strings: None,
            fonts: None,
            assets: None,
            dropped: Vec::new(),
            media_effects: Vec::new(),
            now: LocalTime(0.0),
            caret_blink_time: text::CARET_BLINK_PERIOD,
            requests: dereth_client_contract::requests::Outbox::owned(),
            notice_inbox: dereth_client_contract::notices::NoticeInbox::owned(),
            env: None,
        };
        factory::register_engine_classes(&mut s);
        let root = s.create_hollow(None);
        s.root = root;
        if let Some(n) = s.node_mut(root) {
            n.flags.set_activatable(true);
            n.flags.set_is_root_element(true);
        }
        s.activatable.push(root);
        s
    }

    /// The root element.
    #[must_use]
    pub fn root(&self) -> ElemHandle {
        self.root
    }

    /// The `TextElement` base of an element, for a caller that wants to set its text.
    ///
    /// This is the rebuild's typed receiver for a text child resolved during screen binding,
    /// corresponding to the original client's stored text-child pointer.
    pub fn text_element_mut(&mut self, h: ElemHandle) -> Option<&mut text::TextElement> {
        self.node_mut(h)?.behaviour.as_mut()?.as_text_mut()
    }

    /// `StringInfo` resolution, through whatever the host installed in [`UiSystem::strings`].
    ///
    /// This resolves `(out, id, vars, use-meta-language)`
    /// with **no** variables, and the arm it takes is the one
    /// the string info's internal query takes: `true`. So a row with no substitutions
    /// still goes through the meta-language render, and that render's `flags & 1` tail
    /// collapses every run of spaces in the whole output
    /// (the excess-space trim).
    ///
    /// That tail is the only thing that separates the two arms for a markup-free row, and it is
    /// **visible**: 62 of the 6 899 shipped rows ship a double space that retail never draws, and
    /// one more (`{{keepspaces}}Lore Master  Quiz Night`) ships the option that keeps it
    /// [measured over every shipped string table]. Skipping the renderer would draw every one of
    /// those rows with the extra space.
    ///
    /// [`Self::resolve_string_rendered`] is the same call with values. This one renders **variant
    /// 0 alone**, which is the row a caller with no values is asking for; handing `render` all the
    /// fragments of a multi-variant row would concatenate the pieces around a variable that the
    /// caller has nothing to put in.
    #[must_use]
    pub fn resolve_string(&self, table: DataId, string_id: u32) -> Option<String> {
        let raw = self.strings.as_ref()?.resolve_raw(table, string_id)?;
        Some(dereth_assets::escape::unescape(text::metalanguage::render(
            &[raw],
            &[],
        )))
    }

    /// How many fragments a row holds — one more than the number of variables the client passes
    /// it. A panel that guards on the count (a fragment count that disagrees with the values is a
    /// string-table mismatch, not a formatting choice) needs this and nothing else.
    #[must_use]
    pub fn resolve_string_variant_count(&self, table: DataId, string_id: u32) -> Option<usize> {
        Some(
            self.strings
                .as_ref()?
                .resolve_variants_raw(table, string_id)?
                .len(),
        )
    }

    /// The `bUseMetaLanguage == false` arm of the same row, **unrendered** — variant 0 exactly as
    /// the dat stores it, without meta-language rendering or unescaping.
    ///
    /// Kept for the one caller that genuinely wants the stored text rather than the drawn text:
    /// a census or a probe asking what the dat holds. Production text goes through
    /// [`Self::resolve_string`].
    #[must_use]
    pub fn resolve_string_unrendered(&self, table: DataId, string_id: u32) -> Option<String> {
        self.strings.as_ref()?.resolve(table, string_id)
    }

    /// The same row as [`UiSystem::resolve_string`], but **all** its variants.
    ///
    /// A `StringTableEntry` holds a list: variant 0 is the singular/default form, and a row with
    /// substitutions stores the literal pieces around its variables — which is the only way to
    /// rebuild `ID_DataPatch_PatchProgress`.
    #[must_use]
    pub fn resolve_string_variants(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.strings.as_ref()?.resolve_variants(table, string_id)
    }

    /// The row's **own** variable list — [`text::StringResolver::resolve_variables`], i.e. the
    /// hashed ids used to look up the caller's values. `None` means the installed resolver cannot
    /// say.
    #[must_use]
    pub fn resolve_string_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        self.strings.as_ref()?.resolve_variables(table, string_id)
    }

    /// `(out, id, vars)`, meta-language on, **as retail
    /// calls it**: the caller names each value and the *row* decides where it goes.
    ///
    /// # The matching rule
    ///
    /// Each named value is filed under the string hash of `"KEY"` (`dereth_primitives::num::hash::str_hash`), not a slot number. Resolution
    /// then walks the **row's** variable ids and looks up each matching value. Order at the call
    /// site is therefore invisible; only the row's order is real. Two shipped rows prove it
    /// matters: `ID_ActionKeyMap_OverwriteExistingBinding` lists `KEY, ACTION` and
    /// `ID_ActionKeyMap_Binding` lists `ACTION, KEY` — the same two values, the other way round —
    /// and `ID_CharacterInfo_Resists` lists `RESIST, RESIST, REGEN`, the same id twice, which a
    /// name lookup satisfies with one value and a positional list only satisfies by accident.
    ///
    /// # A variable the caller did not name
    ///
    /// A failed lookup sets the *missing* flag, and the metalanguage arm's early return of the
    /// missing-variable string runs **before** the render — so
    /// nothing is rendered and the string info's outer getter hands back the empty string it
    /// started with. This answers `None` for that, which every caller here already turns into its
    /// own fall-back. Retail never relies on it: the duration formatter adds all
    /// seven of `ID_DurationFormat`'s variables on every call and passes `""` for the components
    /// that are zero, rather than leaving them out. A value the row does **not** name is simply
    /// never looked up, exactly as an extra entry in the client's hash table would be.
    ///
    /// When the installed resolver cannot name the row's variables — every fixture that holds
    /// fragments and nothing else — this falls back to [`Self::resolve_string_rendered`]'s
    /// positional order.
    #[must_use]
    pub fn resolve_string_named(
        &self,
        table: DataId,
        string_id: u32,
        values: &[(&str, &str)],
    ) -> Option<String> {
        text::string_table::render_named(self.strings.as_deref()?, table, string_id, values)
    }

    /// The meta-language resolve -- the row's
    /// pieces and the caller's values put through
    /// [`text::metalanguage::render`] and then
    /// [`dereth_assets::escape::unescape`], which is the order
    /// the string info's internal query uses.
    ///
    /// [`UiSystem::resolve_string_variants`] is the non-meta-language path: it hands
    /// back the pieces unescaped and leaves the interleave to the caller, which is right for a
    /// row with no markup and wrong for one with a `{a|b}` block in it. Prefer this whenever the
    /// values are known at the call site.
    ///
    /// # This is the positional shim, not the client's call
    ///
    /// Retail matches a value to a variable by the string hash of its **name**, never by
    /// position (see [`Self::resolve_string_named`], which is the real thing). This entry point
    /// assumes the caller knows the row's variable order, so a localised dat that reordered
    /// a row would silently swap its values. It survives for two kinds of caller: a fixture
    /// resolver that cannot name a row's variables at all, and a row with exactly **one**
    /// variable, where the two rules cannot disagree.
    #[must_use]
    pub fn resolve_string_rendered(
        &self,
        table: DataId,
        string_id: u32,
        values: &[String],
    ) -> Option<String> {
        text::string_table::render_positional(self.strings.as_deref()?, table, string_id, values)
    }

    /// The font lookup, through [`UiSystem::fonts`].
    #[must_use]
    pub fn font_metrics(&self, did: DataId) -> Option<std::sync::Arc<dyn text::FontMetrics>> {
        self.fonts.as_ref()?.metrics(did)
    }

    #[must_use]
    pub fn display(&self) -> (i32, i32) {
        self.display
    }

    /// The live display reference box, `(0, 0, w-1, h-1)`.
    #[must_use]
    pub fn display_box(&self) -> Box2D {
        Box2D::new(0, 0, self.display.0 - 1, self.display.1 - 1)
    }
}
