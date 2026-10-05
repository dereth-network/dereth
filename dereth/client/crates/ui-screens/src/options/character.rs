//! The Character Options page builds its headings and available rows from the shared sheet.
//!
//! # This page does not caption from the preference registry
//!
//! **Adding a toggle option has two forms and this page uses the other one.**
//!
//! The Client Options page adds each toggle by **preference name**.
//! Its check-box preference binding asks the preference registry for a label; that is the path
//! [`super::preferences`] fills.
//! The Character Options page adds each toggle by **`PlayerOption`** instead.
//! That form consults no preference registry at all: it sets the control's player option and
//! reads the player module's default option value. The caption comes from the very next step in
//! option initialization, the toggle-label write, which takes
//!
//! ```text
//! label   = hash("ID_PlayerOption_<Name>")
//! tooltip = hash("ID_PlayerOption_<Name>_Help")
//! ```
//!
//! This is a literal token pair per row, spelled mechanically from the option's own enum name,
//! hashed once into a file-static and reused. The first token supplies the string info **on the
//! check box itself** (the check box also carries its caption), while the second supplies
//! the tooltip; both come from string-table enum
//! `0x10000003`.
//!
//! Player-option rows caption themselves from tokens derived from the option's name,
//! independently of the preference registry.
//!
//! # Where the value comes from, and why no option word is ever rebuilt here
//!
//! The check-box option control's value read is the module's getter for its one player option,
//! and its apply is the module's setter for that option with the control's current value —
//! **one option at a time, through the module**, never a word composed from the page. That
//! distinction is load bearing: both option words carry bits no row on this page edits (the
//! lock-UI and appear-offline bits, set from the radar and the friends panel, and every bit no
//! option names), and the client round-trips each of them untouched. A page that rebuilt the
//! word from its own check boxes would silently clear them.
//!
//! **This page cannot make that mistake, by construction.** It reads through
//! [`crate::view::GameView::player_option`] and writes
//! [`crate::view::UiRequest::SetPlayerOption`], both of which name **one option**; the two dwords
//! are not reachable from this crate and the bit table is not duplicated here — it already exists,
//! once, as `dereth_client_model::player::options::Options::{get, set}`, which is a read-modify-write of the
//! word it was given.
//!
//! # The three ends of the seam
//!
//! Each end is checkable in one grep:
//!
//! | end | producer |
//! |---|---|
//! | read | the HUD view's `player_option` implementation |
//! | write | the client's `UiRequest::SetPlayerOption` arm |
//! | wire | `dereth_client_model::player`'s `CharacterCharacterOptionsEvent { module }` send |
//!
//! Player-option rows read and write through this seam; the option-wire tests hold it.

use dereth_ui::{ElemHandle, UiSystem};

use crate::panels::listbox::ListBoxWidget;
use crate::view::{GameView, PlayerOption, UiRequest};

/// The Character Options element in the shipped `classic_gameplay` tree.
///
/// Measured off the built tree rather than taken from a document: it is the one element of type
/// `0x10000027` under the panel stack, distinct from the Client Options page at `0x10000213`. The three
/// option pages carry the same Apply/Cancel/Defaults child ids, so a click is attributed to a page
/// by which page element it sits under — exactly as `super::config::CONFIG_PAGE_ELEMENT`'s note
/// says.
pub const CHARACTER_PAGE_ELEMENT: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0211);

/// Find the Character Options page under `root`.
///
/// **A recursive child lookup of `0x10000211` from `root` is not enough, and this is measured, not
/// defensive.**
/// The shipped `classic_gameplay` tree carries the id `0x10000211` **twice**: once as the
/// keyboard tab `KEYBOARD_TAB_PAGES[4]` (engine type 3, a plain page) and once as this
/// page (type `0x10000027`). A depth-first walk from the screen root reaches the keyboard's copy
/// **first**, so binding by id alone binds the wrong element, finds no `0x100001FA` under it and
/// leaves the page silently empty — which is indistinguishable from "the layout changed".
///
/// The client cannot hit this because the element manager hands post-initialization its own
/// element. The equivalent guard here is the runtime type check the client would perform, so
/// the type is checked as well as the id. Keyboard-page initialization scopes its own lookup
/// to that page for the mirror-image reason.
#[must_use]
pub fn find_page(ui: &UiSystem, root: ElemHandle) -> Option<ElemHandle> {
    ui.element_list().iter().copied().find(|h| {
        ui.node(*h).is_some_and(|n| {
            n.element_id() == CHARACTER_PAGE_ELEMENT
                && n.ty() == crate::element_types::ty::CHARACTER_SETTINGS
        }) && is_under(ui, *h, root)
    })
}

fn is_under(ui: &UiSystem, mut h: ElemHandle, root: ElemHandle) -> bool {
    loop {
        if h == root {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

/// The character settings panel's post-init's one child — the option box.
///
/// It is **not** the Client Options list `0x10000200`: each option page names its own list box.
pub const OPTION_BOX: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_01FA);

/// The string table enum writes both of its ids in.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0003;

/// The prefix builds every caption token from.
pub const TOKEN_PREFIX: &str = "ID_PlayerOption_";
/// The suffix that turns a caption token into its tooltip token.
pub const HELP_SUFFIX: &str = "_Help";

/// The name the option initialisation spells into each token, per [`PlayerOption`].
///
/// This is a **transcription of the client's 50 option-name strings**, not a `Debug` rendering:
/// they are the client's own spellings and one of them differs from the obvious one
/// (`ID_PlayerOption_FellowshipShareXP`, capital `XP`). `tests::the_page_names_every_option_the_way_the_client_spells_it`
/// pins a sample of them as literals.
#[must_use]
pub const fn option_name(o: PlayerOption) -> &'static str {
    use PlayerOption as P;
    match o {
        P::ViewCombatTarget => "ViewCombatTarget",
        P::SalvageMultiple => "SalvageMultiple",
        P::MainPackPreferred => "MainPackPreferred",
        P::VividTargetingIndicator => "VividTargetingIndicator",
        P::ShowTooltips => "ShowTooltips",
        P::CoordinatesOnRadar => "CoordinatesOnRadar",
        P::SideBySideVitals => "SideBySideVitals",
        P::SpellDuration => "SpellDuration",
        P::DisableMostWeatherEffects => "DisableMostWeatherEffects",
        P::DisableDistanceFog => "DisableDistanceFog",
        P::PersistentAtDay => "PersistentAtDay",
        P::DisableHouseRestrictionEffects => "DisableHouseRestrictionEffects",
        P::UseCraftSuccessDialog => "UseCraftSuccessDialog",
        P::ConfirmVolatileRareUse => "ConfirmVolatileRareUse",
        P::DisplayTimeStamps => "DisplayTimeStamps",
        P::FilterLanguage => "FilterLanguage",
        P::ShowHelm => "ShowHelm",
        P::ShowCloak => "ShowCloak",
        P::IgnoreAllegianceRequests => "IgnoreAllegianceRequests",
        P::IgnoreFellowshipRequests => "IgnoreFellowshipRequests",
        P::DisplayAllegianceLogonNotifications => "DisplayAllegianceLogonNotifications",
        P::FellowshipShareXP => "FellowshipShareXP",
        P::FellowshipShareLoot => "FellowshipShareLoot",
        P::FellowshipAutoAcceptRequests => "FellowshipAutoAcceptRequests",
        P::AcceptLootPermits => "AcceptLootPermits",
        P::UseDeception => "UseDeception",
        P::AllowGive => "AllowGive",
        P::IgnoreTradeRequests => "IgnoreTradeRequests",
        P::DragItemOnPlayerOpensSecureTrade => "DragItemOnPlayerOpensSecureTrade",
        P::DisplayDateOfBirth => "DisplayDateOfBirth",
        P::DisplayAge => "DisplayAge",
        P::DisplayChessRank => "DisplayChessRank",
        P::DisplayFishingSkill => "DisplayFishingSkill",
        P::DisplayNumberDeaths => "DisplayNumberDeaths",
        P::DisplayNumberCharacterTitles => "DisplayNumberCharacterTitles",
        P::ToggleRun => "ToggleRun",
        P::AdvancedCombatUI => "AdvancedCombatUI",
        P::AutoTarget => "AutoTarget",
        P::AutoRepeatAttack => "AutoRepeatAttack",
        P::UseChargeAttack => "UseChargeAttack",
        P::LeadMissileTargets => "LeadMissileTargets",
        P::UseFastMissiles => "UseFastMissiles",
        P::StayInChatMode => "StayInChatMode",
        P::HearAllegianceChat => "HearAllegianceChat",
        P::HearGeneralChat => "HearGeneralChat",
        P::HearTradeChat => "HearTradeChat",
        P::HearLFGChat => "HearLFGChat",
        P::HearRoleplayChat => "HearRoleplayChat",
        P::HearSocietyChat => "HearSocietyChat",
        P::HearPKDeaths => "HearPKDeaths",
        P::LockUI => "LockUI",
        // Not an option-initialisation row. The name is the client's enum name, and it is carried here so
        // the one producer of option names stays one -- but `ID_PlayerOption_AppearOffline` is
        // **not in string table `0x10000003`**, and the friends panel never hashes
        // it: that box's caption is authored in the layout. See
        // [`super::toggle::Caption::FromLayout`].
        P::AppearOffline => "AppearOffline",
    }
}

/// `"ID_PlayerOption_<Name>"`, hashed for the caption — the caption token.
#[must_use]
pub fn label_token(o: PlayerOption) -> String {
    format!("{TOKEN_PREFIX}{}", option_name(o))
}

/// `"ID_PlayerOption_<Name>_Help"`, hashed for the tooltip — the tooltip token.
#[must_use]
pub fn help_token(o: PlayerOption) -> String {
    format!("{TOKEN_PREFIX}{}{HELP_SUFFIX}", option_name(o))
}

/// One built check-box row.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterOptionRow {
    /// Which bit this row edits.
    pub option: PlayerOption,
    /// The check-box option (`0x10000219`) inside the template-2 row.
    pub element: ElemHandle,
    /// The list-box row it sits in.
    pub row: ElemHandle,
    /// The control's current value.
    pub current: bool,
    /// The control's saved value — what Cancel restores.
    pub saved: bool,
    /// The control's default — the player module's default option value,
    /// read by the client at bind time.
    ///
    /// Still an `Option`, and still `None` when the host cannot answer: the function lives in
    /// `PlayerModule`, this crate has no edge to `dereth-client-model`, and *Restore Defaults* has to be
    /// able to say "not available on this host" rather than write `false` into every check box.
    /// Its producer is [`GameView::player_option_default`], implemented by
    /// `dereth_client_runtime::hud::HudView` over
    /// `dereth_client_model::player::options::default_option_value`, which *is* the client's.
    pub default: Option<bool>,
    /// What the toggle-label write's first id resolved to, or `None` when there is no string resolver.
    pub label: Option<String>,
    /// The caption token, always recorded, so a headless run can tell *"no string table"* from
    /// *"no caption"*.
    pub label_token: String,
    /// The tooltip token — recorded and not shown, exactly as [`super::page`] records the option
    /// pages' tooltips: the hover surface is `dereth_ui`'s and no option control carries one here.
    pub help_token: String,
    /// What of the world's era the option needs ([`dereth_client_contract::options::sheet::Needs`]).
    pub needs: dereth_client_contract::options::sheet::Needs,
}

impl CharacterOptionRow {
    /// The checkbox option control's change handler.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.saved != self.current
    }
}

/// The built Character Options page (type `0x10000027`).
#[derive(Debug, Clone, Default)]
pub struct CharacterSettingsPage {
    /// The page element.
    pub page: Option<ElemHandle>,
    /// The option box — `0x100001FA`.
    pub option_box: Option<ListBoxWidget>,
    /// The page's option controls, in option-initialisation order.
    pub rows: Vec<CharacterOptionRow>,
    hidden_rows: Vec<CharacterOptionRow>,
    row_order: Vec<ElemHandle>,
    /// [`Self::add_header`] calls that produced a row.
    pub headers: usize,
    /// [`Self::add_separator`] calls that produced a row. **Six**, not five: there is a trailing one.
    pub separators: usize,
    /// How many section headers resolved to text.
    pub header_captions: usize,
    /// How many row captions resolved to text — the numerator of *"how many rows caption
    /// themselves"*.
    pub row_captions: usize,
    /// Template-row insertions, or child lookups with their runtime type check, that produced
    /// nothing.
    pub failures: usize,
    /// How many rows got a default out of [`GameView::player_option_default`].
    /// The denominator for *Restore Defaults*: the rows answered by
    /// [`GameView::player_option_default`], **0** without one, and
    /// [`Self::restore_default_values`] writes nothing in that case rather than unticking the
    /// page.
    pub defaults_seen: usize,
    /// How many rows were given a value by a [`GameView`] that answered at all.
    ///
    /// **The denominator matters more than the value here.** `GameView::player_option` has a
    /// defaulted implementation returning `false` — so a page whose rows are all unticked may be a
    /// character with no options set *or* a host with no producer, and this counter is the only
    /// thing that separates them. See the module docs.
    pub values_seen: usize,
}

impl CharacterSettingsPage {
    /// The character settings panel's post-init — a recursive child lookup of `0x100001FA`,
    /// a runtime list-box type check, then option-changed delivery.
    #[must_use]
    pub fn bind(ui: &UiSystem, page: ElemHandle) -> Self {
        let option_box = ui
            .get_child_recursive(page, OPTION_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        Self {
            page: Some(page),
            option_box,
            ..Self::default()
        }
    }

    /// The rows currently in the option box — headers, separators and check boxes together.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.option_box.as_ref().map_or(0, |b| b.items.len())
    }

    /// The character settings panel's option build.
    ///
    /// Each shared heading has a header before it and a separator after it, including the last.
    /// Every player option gets a toggle and its caption.
    ///
    /// Returns how many check-box rows were built.
    pub fn init_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        use dereth_client_contract::options::interface::Interface;
        use dereth_client_contract::options::sheet::{self, PageId, Value};
        for (heading, rows) in sheet::headings_for(PageId::Character, Interface::Modern) {
            self.add_literal_header(ui, &super::config::resolve_text(ui, heading.text));
            for r in rows {
                let Value::Option(o) = r.value else { continue };
                if let Some(i) = self.add_toggle_option(ui, o, view) {
                    self.rows[i].needs = r.needs;
                }
            }
            // A separator follows **every** section, the last one included, as retail's list
            // ends with a rule.
            self.add_separator(ui);
        }
        self.apply_era(ui, Some(view.era_features()));
        if let Some(b) = self.option_box.as_mut() {
            b.update_layout(ui);
        }
        self.rows.len()
    }

    /// A header with literal text: one of the headings both interfaces' pages share.
    pub fn add_literal_header(&mut self, ui: &mut UiSystem, caption: &str) -> bool {
        let Some(row) = self.add_row(ui, super::page::template::HEADER) else {
            return false;
        };
        self.headers += 1;
        super::page::set_literal_text(ui, row, caption);
        self.header_captions += 1;
        true
    }

    /// Hide unavailable options and compact the list, retaining their controls so a later
    /// capability update can restore them in their original order. Returns the number hidden.
    pub fn apply_era(
        &mut self,
        ui: &mut UiSystem,
        features: Option<dereth_primitives::EraFeatures>,
    ) -> usize {
        if self.row_order.is_empty() {
            self.row_order = self
                .option_box
                .as_ref()
                .map_or_else(Vec::new, |b| b.items.clone());
        }
        let previously_visible = self.rows.len();
        self.rows.append(&mut self.hidden_rows);
        self.rows
            .sort_by_key(|r| self.row_order.iter().position(|h| *h == r.row));
        let features = features.unwrap_or(dereth_primitives::EraFeatures::END_OF_RETAIL);
        let rows = std::mem::take(&mut self.rows);
        for row in rows {
            let visible = row.needs.met(Some(&features));
            ui.set_visible(row.row, visible);
            ui.set_mouse_visible(row.row, visible);
            ui.set_mouse_visible(row.element, visible);
            if visible {
                self.rows.push(row);
            } else {
                self.hidden_rows.push(row);
            }
        }
        if let Some(b) = self.option_box.as_mut() {
            b.items = self
                .row_order
                .iter()
                .copied()
                .filter(|h| !self.hidden_rows.iter().any(|r| r.row == *h))
                .collect();
            b.update_layout(ui);
        }
        previously_visible.saturating_sub(self.rows.len())
    }

    fn add_row(&mut self, ui: &mut UiSystem, index: usize) -> Option<ElemHandle> {
        let b = self.option_box.as_mut()?;
        let h = b.add_from_template(ui, index, None);
        if h.is_none() {
            self.failures += 1;
        }
        h
    }

    /// Add a header `(stringId)` — template 0, the row itself is the text.
    pub fn add_header(&mut self, ui: &mut UiSystem, token: &str) -> bool {
        let Some(row) = self.add_row(ui, super::page::template::HEADER) else {
            return false;
        };
        self.headers += 1;
        let sid = dereth_primitives::num::hash::str_hash(token.as_bytes());
        let ok = super::page::set_string_info(ui, row, table(ui), sid).is_some();
        self.header_captions += usize::from(ok);
        ok
    }

    /// The player-option page's separator insert — template 1.
    pub fn add_separator(&mut self, ui: &mut UiSystem) -> bool {
        let ok = self.add_row(ui, super::page::template::SEPARATOR).is_some();
        self.separators += usize::from(ok);
        ok
    }

    /// Add a toggle by `PlayerOption`, followed by
    /// the toggle-label write.
    ///
    /// Template **2** binds descendant `0x10000219` as the check-box option control,
    /// assigns its player option, and registers it — the same shape as the preference overload
    /// with the preference half replaced. The runtime type check is not a formality:
    /// the client registers nothing when it fails.
    pub fn add_toggle_option(
        &mut self,
        ui: &mut UiSystem,
        option: PlayerOption,
        view: &dyn GameView,
    ) -> Option<usize> {
        let row = self.add_row(ui, super::page::template::TOGGLE)?;
        let Some(element) = ui.get_child_recursive(row, super::page::child::CHECKBOX) else {
            self.failures += 1;
            return None;
        };
        if ui.node(element).map(|n| n.ty()) != Some(crate::element_types::ty::OPTION_CHECKBOX) {
            self.failures += 1;
            return None;
        }
        // The value is the module's from the moment the
        // control is bound, not something the page invents.
        let current = view.player_option(option);
        self.values_seen += 1;
        // The checkbox option control's player option write's second line — the default is the
        // module's default option value, refreshed with subsequent player-view reads.
        let default = view.player_option_default(option);
        self.defaults_seen += usize::from(default.is_some());
        let lt = label_token(option);
        let ht = help_token(option);
        // The toggle-label write puts the caption on the check box itself because
        // the check-box control carries button behavior and its own text caption.
        let label = if let Some(caption) =
            crate::panels::era::option_caption(ui, option, view.era_features())
        {
            super::page::set_literal_text(ui, element, &caption);
            Some(caption)
        } else {
            super::page::set_string_info(
                ui,
                element,
                table(ui),
                dereth_primitives::num::hash::str_hash(lt.as_bytes()),
            )
        };
        self.row_captions += usize::from(label.is_some());
        let idx = self.rows.len();
        self.rows.push(CharacterOptionRow {
            option,
            element,
            row,
            current,
            saved: current,
            default,
            label,
            label_token: lt,
            help_token: ht,
            needs: dereth_client_contract::options::sheet::Needs::Nothing,
        });
        self.refresh(ui, idx);
        Some(idx)
    }

    /// Write the row's current value to attribute `0x0E`.
    pub fn refresh(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(r) = self.rows.get(i) else { return };
        let (h, v) = (r.element, r.current);
        ui.set_attribute_bool(h, super::page::ATTR_CHECKED, v);
    }

    /// The client's message-1 arm: read attribute
    /// `0x0E`, then apply.
    ///
    /// Returns the row that took the message.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> Option<usize> {
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return None;
        }
        let i = self.rows.iter().position(|r| r.element == m.source)?;
        let v = crate::bind::attr_bool(ui, self.rows[i].element, super::page::ATTR_CHECKED)
            .unwrap_or(false);
        self.rows[i].current = v;
        self.apply(&mut ui.requests, i);
        Some(i)
    }

    /// The checkbox option control's apply's player-option arm — set the row's option to its
    /// current value.
    ///
    /// **One option, not a word.** The request names the bit and the value; whoever owns the
    /// `PlayerModule` sets that bit in the word it already has. See the module docs on why a page
    /// that composed the word instead would clear bit 25 of the second option word.
    pub fn apply(&self, requests_out: &mut crate::requests::Outbox, i: usize) {
        let Some(r) = self.rows.get(i) else { return };
        requests_out.emit(UiRequest::SetPlayerOption(r.option, r.current));
    }

    fn refresh_defaults(&mut self, view: &dyn GameView) {
        self.defaults_seen = 0;
        for row in &mut self.rows {
            row.default = view.player_option_default(row.option);
            self.defaults_seen += usize::from(row.default.is_some());
        }
    }

    /// Re-read every row from the module.
    ///
    /// Returns how many moved, which is the observable half of the read: a `save_current_values`
    /// that silently did nothing and one that re-read identical values for every row are otherwise the same.
    pub fn save_current_values(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.refresh_defaults(view);
        let mut moved = 0;
        for i in 0..self.rows.len() {
            let v = view.player_option(self.rows[i].option);
            if v != self.rows[i].current {
                moved += 1;
            }
            self.rows[i].current = v;
            self.rows[i].saved = v;
            self.refresh(ui, i);
        }
        moved
    }

    /// The check-box option control's player-option-changed notice, for every row at once.
    ///
    /// ```text
    /// if notice.option == row.option:
    ///     row.current = module.get_option(row.option)
    ///     refresh_checkbox(row.current)
    /// ```
    ///
    /// The saved value is **not** touched — this is not [`Self::save_current_values`], and the
    /// difference is what Cancel is made of. The player module raises the notice as its
    /// first step for *every* option change, and the two fellowship exclusions re-enter the change
    /// for the other option (the auto-accept setter raises a change for `0x12`),
    /// so ticking *Ignore Fellowship Requests* on this page unticks
    /// *Auto-Accept* two rows down in retail — and, because the Auto-Accept row's current value
    /// then differs from its saved value, Cancel puts it back **and re-sends it**. A page that
    /// re-read only on show and on Apply would leave the other row lit until the page was closed,
    /// and Cancel would skip it, leaving the module and the shard at Auto-Accept off.
    ///
    /// This build has no notice bus, so the host polls once a frame and this writes only the rows
    /// whose module bit differs from the current value — which is exactly the set of boxes a notice
    /// would have reached, one frame late, the same as the five panel-bound boxes in
    /// [`super::toggle::PanelOptionBoxes::refresh`]. Returns how many rows moved.
    pub fn on_player_option_changed(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.refresh_defaults(view);
        let mut moved = 0;
        for i in 0..self.rows.len() {
            if let Some(caption) =
                crate::panels::era::option_caption(ui, self.rows[i].option, view.era_features())
            {
                if self.rows[i].label.as_ref() != Some(&caption) {
                    super::page::set_literal_text(ui, self.rows[i].element, &caption);
                    self.rows[i].label = Some(caption);
                }
            }

            let v = view.player_option(self.rows[i].option);
            if v == self.rows[i].current {
                continue;
            }
            self.rows[i].current = v;
            self.refresh(ui, i);
            moved += 1;
        }
        moved
    }

    /// Cancel, and the hide path. Only the rows that
    /// changed are written back, which is why cancelling does not re-send unchanged options.
    pub fn restore_saved_values(&mut self, ui: &mut UiSystem) -> usize {
        let mut n = 0;
        for i in 0..self.rows.len() {
            if !self.rows[i].changed() {
                continue;
            }
            self.rows[i].current = self.rows[i].saved;
            self.refresh(ui, i);
            self.apply(&mut ui.requests, i);
            n += 1;
        }
        n
    }

    /// The *Restore Defaults* button and global
    /// message `0x0C`.
    ///
    /// Unconditional in the client: every option, changed or not, gets
    /// current = default, refresh, apply. Here a row whose default is `None`
    /// is **skipped**, which is the one divergence and it is the safe direction — see
    /// [`CharacterOptionRow::default`]. The return is how many rows were actually written, so a
    /// host with no default-option-value producer reports **0**. The values themselves must
    /// also match the supplied defaults; writing every row alone does not establish that.
    pub fn restore_default_values(&mut self, ui: &mut UiSystem) -> usize {
        let mut n = 0;
        for i in 0..self.rows.len() {
            let Some(v) = self.rows[i].default else {
                continue;
            };
            self.rows[i].current = v;
            self.refresh(ui, i);
            self.apply(&mut ui.requests, i);
            n += 1;
        }
        n
    }

    /// The player-option page's visibility-changed handler — [`Self::save_current_values`] on
    /// show, [`Self::restore_saved_values`] on hide.
    ///
    /// **Known defect.** The *first* show of the page on an ordinary panel page never delivers
    /// its `0x18`, because the panel update raises `TAB_PAGE_CHANGED` with a nested
    /// `broadcast_element_message` that takes the next serial and stamps every ancestor, so
    /// `claim_serial` refuses the outer message at each one. The fix is to queue that message
    /// (`queue_element_message`) in `dereth_ui::widgets` instead. Until then this show arm does
    /// not run on a first visit — the same holds for the Client Options page.
    pub fn on_visibility_changed(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        visible: bool,
    ) -> usize {
        if visible {
            self.save_current_values(ui, view)
        } else {
            self.restore_saved_values(ui)
        }
    }

    /// The option page base's change handler.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.rows.iter().any(CharacterOptionRow::changed)
    }

    /// The row bound to one option.
    #[must_use]
    pub fn row_of(&self, o: PlayerOption) -> Option<usize> {
        self.rows.iter().position(|r| r.option == o)
    }
}

/// Bind the option box, then [`CharacterSettingsPage::init_options`].
///
/// `None` when the option box is not under the page, which is the loud form of *"the layout
/// changed"* rather than an empty page.
#[must_use]
pub fn character_settings_post_init(
    ui: &mut UiSystem,
    page: ElemHandle,
    view: &dyn GameView,
) -> Option<CharacterSettingsPage> {
    let mut p = CharacterSettingsPage::bind(ui, page);
    p.option_box.as_ref()?;
    p.init_options(ui, view);
    Some(p)
}

/// The `DataId` string table enum [`STRING_TABLE_ENUM`] resolves to.
///
/// The same table the preference registry uses — [`super::preferences::table`] — which is why the
/// captions land in this build even though the *producer* is a different one.
#[must_use]
pub fn table(ui: &UiSystem) -> dereth_primitives::DataId {
    super::preferences::table(ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{rows_for, PageId, Value};

    /// Oracle: the 50 string literals retail hashes in
    /// The character settings panel's option build, in page order.
    ///
    /// A sample is quoted as literals rather than assembled from the enum, because a token built
    /// by concatenation and checked by concatenation cannot catch a wrong spelling —
    /// the stated testability rule. `FellowshipShareXP` is here specifically because its capitalisation is the
    /// one that is not obvious.
    #[test]
    fn the_page_names_every_option_the_way_the_client_spells_it() {
        use PlayerOption as P;
        assert_eq!(
            label_token(P::ViewCombatTarget),
            "ID_PlayerOption_ViewCombatTarget"
        );
        assert_eq!(
            help_token(P::ViewCombatTarget),
            "ID_PlayerOption_ViewCombatTarget_Help"
        );
        assert_eq!(
            label_token(P::FellowshipShareXP),
            "ID_PlayerOption_FellowshipShareXP"
        );
        assert_eq!(
            help_token(P::DisplayNumberCharacterTitles),
            "ID_PlayerOption_DisplayNumberCharacterTitles_Help"
        );
        assert_eq!(label_token(P::HearLFGChat), "ID_PlayerOption_HearLFGChat");
        assert_eq!(
            label_token(P::AdvancedCombatUI),
            "ID_PlayerOption_AdvancedCombatUI"
        );
        assert_eq!(
            label_token(P::DragItemOnPlayerOpensSecureTrade),
            "ID_PlayerOption_DragItemOnPlayerOpensSecureTrade"
        );
        assert_eq!(TOKEN_PREFIX, "ID_PlayerOption_");
        assert_eq!(HELP_SUFFIX, "_Help");
        //  writes both ids in table enum 0x10000003 — the same one the
        // preference registry uses, and **not** the 0x10000004 the key-binding rows use.
        assert_eq!(STRING_TABLE_ENUM, 0x1000_0003);
        assert_eq!(
            STRING_TABLE_ENUM,
            super::super::preferences::STRING_TABLE_ENUM
        );
        assert_ne!(
            STRING_TABLE_ENUM,
            super::super::keybinding::STRING_TABLE_ENUM
        );
        // The Character Options list box, distinct from the Client Options list.
        assert_eq!(OPTION_BOX.0, 0x1000_01FA);
        assert_ne!(OPTION_BOX, super::super::config::OPTION_BOX);
        // Every option on the page has a name, and no two share one.
        let mut names: Vec<&str> = rows_for(PageId::Character, Interface::Modern)
            .filter_map(|r| match r.value {
                Value::Option(o) => Some(o),
                _ => None,
            })
            .map(option_name)
            .collect();
        assert_eq!(names.len(), 50);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 50, "no two rows share a caption token");
        assert_eq!(label_token(P::HearPKDeaths), "ID_PlayerOption_HearPKDeaths");
        assert_eq!(
            help_token(P::HearPKDeaths),
            "ID_PlayerOption_HearPKDeaths_Help"
        );
    }
}
