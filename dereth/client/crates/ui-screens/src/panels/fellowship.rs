//! `FellowshipPanel` — the Fellowship tab: the roster, the per-fellow vitals, and the seven
//! buttons that are the whole client-to-server half of the fellowship protocol.
//!
//! `panels::catalogue` carries a row for it, `element_types` registers `0x1000002D` and
//! `panels::rows` names its row attribute and its eight vital element ids; this module is the
//! constructor and behaviour behind them.
//!
//! # Both halves of the protocol
//!
//! The five **receivers** — `0x02BE`, `0x02C0`, `0x02BF`, `0x00A3`, `0x00A4` — write the
//! fellowship state; this module draws it. It is also the client-to-server half: the seven
//! fellowship client events that create, join, leave, promote, dismiss or open a fellowship.
//! The panel emits semantic requests; the runtime validates and orders their wire actions.
//!
//! # The corpus is the oracle, and it is not empty
//!
//! The client half of the three `fellowship*` recordings is **retail's client**, so every button
//! below has a byte-level answer rather than a reasoned one. Across the three sessions:
//!
//! | event | opcode | count | body seen |
//! |---|---|---|---|
//! | `Fellowship_UpdateRequest` | `0x00A6` | 8 | `01000000` / `00000000` |
//! | `Fellowship_Recruit` | `0x00A5` | 9 | `1e000050`, `1f000050`, `20000050` |
//! | `Fellowship_Quit` | `0x00A3` | 5 | `00000000` ×4, `01000000` ×1 |
//! | `Fellowship_Create` | `0x00A2` | 2 | `0b00` `"Of The ring"` `0000` `01000000` |
//! | `Fellowship_AssignNewLeader` | `0x0290` | 2 | `1e000050`, `20000050` |
//! | `Fellowship_Dismiss` | `0x00A4` | 1 | `1f000050` |
//! | `Fellowship_ChangeFellowOpenness` | `0x0291` | 1 | `01000000` |
//!
//! **Recruit, Dismiss and AssignNewLeader all carry a real object id.** Every one of the twelve
//! is a `0x5000001E`/`1F`/`20` — the selected player's instance id. Nothing on the fellowship
//! path identifies anybody by name, and nothing sends a character id of `0`.
//!
//! The `0x0290` + `0x00A3` pair at **the same timestamp** in `fellowship-two-monarch.jsonl`
//! (t=366.009, seq 105 then 106) is case `0x1000027C` visible on the
//! wire: a leader who quits hands leadership to the first non-leader fellow first.
//!
//! # The client's own functions, and where they are here
//!
//! | client | here |
//! |---|---|
//! | — twelve child lookups | [`FellowshipPanel::post_init`] |
//! | — the update request with the visibility, one statement | [`FellowshipPanel::on_visibility_changed`] |
//! | — frame swap, name, flush, one row per member | [`FellowshipPanel::update`] |
//! | — the level / share line | `FellowshipPanel::update_fellow_stats` |
//! | — three meters and three labels | `FellowshipPanel::update_fellow_vitals` |
//! | — reconcile with the world selection | `FellowshipPanel::update_fellow_selection` |
//! | — six states and one caption | [`FellowshipPanel::update_buttons`] |
//! | — seven buttons, the list, the entry box | [`FellowshipPanel::on_element_message`] |
//! | — recruit, dismiss, assign leadership | the three guarded senders |
//! | the fellow request dialog build, the close dialog notice | the `0x0274` type-4 pair |
//!
//! # Two frames, not a hidden panel
//!
//! The update branches on whether there is a fellowship: with none it shows the
//! not-in-a-fellowship frame (`0x1000026B` — the name box and the Create button) and hides the
//! in-a-fellowship frame (`0x10000275` — the roster and the six buttons), and it *registers for
//! global message 3* so it keeps polling; with one it does the reverse and unregisters. Both are
//! plain visibility writes, so the tab itself is up either way and
//! [`FellowshipPanel::poll_visibility`] is unaffected by which frame is showing.
//!
//! # The four check boxes are not this panel's
//!
//! Initialization also binds `0x10000270`..`0x10000273` — *Ignore Fellowship Requests*,
//! *Auto-Accept*, *Share XP*, *Share Loot* — as option check boxes over player options. They are
//! the character-options mechanism (the player-option setter), not fellowship state, and
//! they belong with the options work. The one this panel *reads* is `FellowshipShareXP`, at the
//! moment the Create button is clicked, through [`GameView::player_option`].
//!
//! `0x10000270` and `0x10000271` are bound in
//! [`crate::options::toggle::FELLOWSHIP_OPTION_BOXES`] and polled every frame. The two options
//! exclude each other, and retail runs the exclusion through the other option's setter, which
//! goes straight back into the option-changed handler — one click, two `0x0005`s. Clearing the
//! other bit without sending it leaves the box drawn unlit while the shard auto-accepts. See
//! `dereth_client_model::player::OptionChange`.

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, ElementId, ElementType, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{FellowEntry, FellowshipView, GameView, PlayerOption, UiRequest};

/// The fellowship panel's element class.
///
/// Registration associates the fellowship factory with literal element class `0x1000002D`.
///
/// Found by **type** rather than by id for the reason [`super::allegiance::PANEL_TYPE`] is:
/// initialization binds twelve *children* and never names its own id, which is layout data.
pub const PANEL_TYPE: ElementType = ElementType(0x1000_002D);

/// The not-in-a-fellowship frame — shown while there is no fellowship.
pub const NOT_IN_FELLOWSHIP_FRAME: ElementId = ElementId(0x1000_026B);
/// The fellowship-name entry box.
pub const NAME_ENTRY_BOX: ElementId = ElementId(0x1000_026F);
/// The Create button.
pub const CREATE_BUTTON: ElementId = ElementId(0x1000_0274);
/// The in-a-fellowship frame — shown while there is one.
pub const IN_FELLOWSHIP_FRAME: ElementId = ElementId(0x1000_0275);
/// The fellowship's name text.
pub const FELLOWSHIP_NAME: ElementId = ElementId(0x1000_0276);
/// The fellows list box.
pub const FELLOWS_LIST: ElementId = ElementId(0x1000_0279);
/// The Leader button.
pub const LEADER_BUTTON: ElementId = ElementId(0x1000_027B);
/// The Quit button — quit with body `0`, after handing leadership on if the player holds it.
pub const QUIT_BUTTON: ElementId = ElementId(0x1000_027C);
/// The Open button.
pub const OPEN_BUTTON: ElementId = ElementId(0x1000_027D);
/// The Recruit button.
pub const RECRUIT_BUTTON: ElementId = ElementId(0x1000_027E);
/// The Dismiss button.
pub const DISMISS_BUTTON: ElementId = ElementId(0x1000_027F);
/// The Disband button — quit with body `1`. The **same opcode** as Quit; only the body differs.
pub const DISBAND_BUTTON: ElementId = ElementId(0x1000_0280);

/// The row's name text, looked up by the client on each new row.
pub const ROW_NAME: ElementId = ElementId(0x1000_0283);
/// The row's level / experience-share line — the fellow-stats update's one child.
pub const ROW_STATS: ElementId = ElementId(0x1000_0284);
/// The row's health **meter**. Its `0x69` is `current / max`.
pub const ROW_HEALTH_METER: ElementId = ElementId(0x1000_0285);
/// The health label, a child **of the meter**, not of the row.
pub const ROW_HEALTH_TEXT: ElementId = ElementId(0x1000_0286);
/// The row's stamina meter.
pub const ROW_STAMINA_METER: ElementId = ElementId(0x1000_0287);
/// The stamina label, a child of the stamina meter.
pub const ROW_STAMINA_TEXT: ElementId = ElementId(0x1000_0288);
/// The row's mana meter.
pub const ROW_MANA_METER: ElementId = ElementId(0x1000_0289);
/// The mana label, a child of the mana meter.
pub const ROW_MANA_TEXT: ElementId = ElementId(0x1000_028A);

/// The instance-id attribute each row carries, the fellow's id — the id every read-back in
/// this file goes through, and the one [`super::rows::ROW_ATTRIBUTES`] already named.
pub const ATTR_ROW_INSTANCE_ID: u32 = 0x1000_000D;
/// The meter-fill float attribute, `cur / max`. The same attribute
/// for all three meters.
pub const ATTR_METER: u32 = 0x69;
/// Rows are added from template 0, at the tail. The list box carries exactly one
/// template, `(0x21000030, 0x10000281)` in the shipped `classic_gameplay` tree.
pub const ROW_TEMPLATE: usize = 0;

/// Table enum `0x10000001` — the table every string on this
/// panel resolves against, the same one `AllegiancePanel` uses.
pub const STRING_TABLE: DataId = DataId(0x2300_0001);

/// `ID_Fellowship_FellowName`, one [`var::NAME`] variable.
pub const ID_FELLOW_NAME: &str = "ID_Fellowship_FellowName";
/// `ID_Fellowship_FellowStats`, two variables: [`var::LEVEL`] then [`var::EXPERIENCE`].
pub const ID_FELLOW_STATS: &str = "ID_Fellowship_FellowStats";
/// `ID_Fellowship_FellowHealthStatus`, two variables: [`var::CUR`] then [`var::MAX`].
pub const ID_HEALTH_STATUS: &str = "ID_Fellowship_FellowHealthStatus";
/// `ID_Fellowship_FellowStaminaStatus`.
pub const ID_STAMINA_STATUS: &str = "ID_Fellowship_FellowStaminaStatus";
/// `ID_Fellowship_FellowManaStatus`.
pub const ID_MANA_STATUS: &str = "ID_Fellowship_FellowManaStatus";
/// `ID_Fellowship_OpenFellowshipButtonText` — the caption while the fellowship is **closed**.
pub const ID_OPEN_BUTTON_TEXT: &str = "ID_Fellowship_OpenFellowshipButtonText";
/// `ID_Fellowship_CloseFellowshipButtonText` — the caption while it is **open**.
pub const ID_CLOSE_BUTTON_TEXT: &str = "ID_Fellowship_CloseFellowshipButtonText";

/// The **variable names** this panel files its string values under.
///
/// The string renderer matches them against each string-table row's own variables by name hash
/// (`dereth_primitives::num::hash::str_hash`), so these names decide where a value lands and the argument order
/// does not. Measured off `0x23000001`: `ID_Fellowship_FellowStats` is `LEVEL, EXPERIENCE` and the
/// three `ID_Fellowship_Fellow*Status` rows are `CUR, MAX`.
pub mod var {
    /// The fellow's name.
    pub const NAME: &str = "NAME";
    /// The recruiting player's name, in `ID_Fellowship_FellowshipRequest`.
    pub const PLAYER: &str = "PLAYER";
    /// The fellow's level.
    pub const LEVEL: &str = "LEVEL";
    /// The fellow's experience percentage — **not** `PERCENT`.
    pub const EXPERIENCE: &str = "EXPERIENCE";
    /// A vital's current value.
    pub const CUR: &str = "CUR";
    /// A vital's maximum.
    pub const MAX: &str = "MAX";
}

/// One row as this panel wrote it, so a test reads back what a player would see rather than
/// re-walking the element tree.
#[derive(Debug, Clone, PartialEq)]
pub struct FellowRow {
    pub id: ObjectId,
    /// What went into [`ROW_NAME`].
    pub name: String,
    /// What went into [`ROW_STATS`].
    pub stats: String,
    /// What went into [`ROW_HEALTH_TEXT`].
    pub health: String,
    /// What went into [`ROW_STAMINA_TEXT`].
    pub stamina: String,
    /// What went into [`ROW_MANA_TEXT`].
    pub mana: String,
    /// The three `0x69` fills, in health / stamina / mana order.
    pub meters: [f32; 3],
    pub element: ElemHandle,
}

/// `FellowshipPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct FellowshipPanel {
    /// The `FellowshipPanel` element itself — the one [`PANEL_TYPE`] names.
    pub panel: Option<ElemHandle>,
    pub not_in_frame: Option<ElemHandle>,
    pub in_frame: Option<ElemHandle>,
    pub name_entry: Option<ElemHandle>,
    pub create_button: Option<ElemHandle>,
    pub fellowship_name: Option<ElemHandle>,
    /// The fellows list box.
    pub list: Option<ListBoxWidget>,
    pub leader_button: Option<ElemHandle>,
    pub quit_button: Option<ElemHandle>,
    pub open_button: Option<ElemHandle>,
    pub recruit_button: Option<ElemHandle>,
    pub dismiss_button: Option<ElemHandle>,
    pub disband_button: Option<ElemHandle>,
    /// The rows on screen, in list order.
    rows: Vec<FellowRow>,
    /// The selected fellow — `0` in the client, `None` here.
    pub selected_fellow: Option<ObjectId>,
    /// The state the tree was last built for, so an unchanged frame rebuilds nothing. `None` is *never built*,
    /// `Some((None, _))` is *built, and there is no fellowship*.
    ///
    /// **The world selection is in the guard, and it has to be.** In the client the roster is
    /// rebuilt from a notice and the buttons are re-run from a *second* one —
    /// the selection-changed notice, which runs the fellow selection update and nothing else,
    /// raised whenever the world selection moves. This build has no notice bus at this seam, so
    /// the selection is polled; guarding on the fellowship alone would leave the Recruit button
    /// showing the answer for whoever was selected when the roster last changed. A test that
    /// offers Recruit only for a selected non-member cannot pass without it.
    last: Option<(Option<FellowshipView>, Option<ObjectId>)>,
    /// How many times [`Self::update`] rebuilt. **Three states, not two**: it separates "ran and
    /// the player is in no fellowship" from "never ran", which is the whole difference between
    /// an empty tab and an unwired one.
    pub rebuilds: u32,
    /// The panel's last observed **effective** visibility across the whole parent chain, which
    /// is what [`Self::poll_visibility`] takes edges of.
    visible: bool,
    /// How many `0x00A6 Fellowship_UpdateRequest` this panel has emitted. Without it the vitals
    /// feed is never subscribed and a `0x02C0` only arrives because some *other* client asked.
    pub update_requests: u32,
}

impl FellowshipPanel {
    /// The fellowship panel's post-init, minus the four option check-box binds and the
    /// nine notice registrations — this build has no notice bus at this seam and the
    /// same writes arrive as [`Self::update`]'s snapshot.
    ///
    /// Bound off the screen **root** for the reason [`super::allegiance::AllegiancePanel::post_init`]
    /// is: every id here is unique in the shipped tree, and the page `FellowshipPanel` sits on is
    /// the toolbar's social page, which this crate's panel stack builds lazily.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        let me = find_panel(ui, root);
        let from = me.unwrap_or(root);
        self.not_in_frame = ui.get_child_recursive(from, NOT_IN_FELLOWSHIP_FRAME);
        self.in_frame = ui.get_child_recursive(from, IN_FELLOWSHIP_FRAME);
        self.name_entry = ui.get_child_recursive(from, NAME_ENTRY_BOX);
        self.create_button = ui.get_child_recursive(from, CREATE_BUTTON);
        self.fellowship_name = ui.get_child_recursive(from, FELLOWSHIP_NAME);
        self.leader_button = ui.get_child_recursive(from, LEADER_BUTTON);
        self.quit_button = ui.get_child_recursive(from, QUIT_BUTTON);
        self.open_button = ui.get_child_recursive(from, OPEN_BUTTON);
        self.recruit_button = ui.get_child_recursive(from, RECRUIT_BUTTON);
        self.dismiss_button = ui.get_child_recursive(from, DISMISS_BUTTON);
        self.disband_button = ui.get_child_recursive(from, DISBAND_BUTTON);
        self.list = ui
            .get_child_recursive(from, FELLOWS_LIST)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.panel = me;
        // The client's trailing hide-everything loop has already run by the
        // time this is called, so a freshly built tree starts hidden and the player's first open
        // is a rising edge. Recording it rather than defaulting to `false` is what stops a
        // rebuilt screen emitting a spurious `0x00A6` for a tab that was already up.
        self.visible = me.is_some_and(|h| ui.is_visible(h));
    }

    /// True once the fellows list box was found — the binding without which no roster can exist.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// The rows this panel last wrote, in list order.
    #[must_use]
    pub fn rows(&self) -> &[FellowRow] {
        &self.rows
    }

    /// Whether the panel is currently up, as this module last saw it.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The visibility-changed handler. **One statement, and a send this build
    /// has never made**: the update request, with the new visibility as its flag.
    ///
    /// There is **no `PlayerDesc` guard** here, unlike the allegiance panel's
    /// visibility-changed show arm. The two panels sit on the same page and only one checks;
    /// the asymmetry is transcribed rather than tidied.
    ///
    /// Returns whether a request was emitted, which is always.
    pub fn on_visibility_changed(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        visible: bool,
    ) -> bool {
        requests_out.emit(UiRequest::FellowshipUpdateRequest { on: visible });
        self.update_requests += 1;
        true
    }

    /// The edge detector the panel host drives once a frame.
    ///
    /// In the client the visibility-changed handler is called by the element system
    /// synchronously; this crate's panels are plain structs bound to a subtree and receive no such
    /// callback, so the edge is taken here off `UiSystem::is_visible`, which answers for the whole
    /// parent chain. That matters: the Fellowship tab counts as shown only when the social page is
    /// the current panel *and* the tab within it is up, and one call asks both questions.
    ///
    /// Returns whether a request was emitted.
    pub fn poll_visibility(&mut self, ui: &mut UiSystem) -> bool {
        let Some(h) = self.panel else { return false };
        let now = ui.is_visible(h);
        if now == self.visible {
            return false;
        }
        self.visible = now;
        self.on_visibility_changed(&mut ui.requests, now)
    }

    /// The fellowship panel's update, guarded on the snapshot.
    ///
    /// The panel copies the fellowship system's current fellowship (or drops its copy when there
    /// is none). With no fellowship it registers for global message 3, hides the in-a-fellowship
    /// frame, shows the not-in-a-fellowship frame and stops. Otherwise it unregisters, swaps the
    /// two frames, writes the fellowship's name as a literal, clears the list, and for each member
    /// adds a row from template 0, stores the member's id in attribute `0x1000000D`, writes
    /// `ID_Fellowship_FellowName` into the `0x10000283` child and runs the stats and vitals
    /// updates; then it runs the fellow selection update.
    ///
    /// The two frame visibility writes are what make an empty tab *look* like "you are in no
    /// fellowship" rather than like a panel that failed to draw, so that path runs them and still
    /// counts as a rebuild.
    ///
    /// Returns whether the tree was rewritten.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let f = view.fellowship();
        let selection = view.selected_object();
        if self
            .last
            .as_ref()
            .is_some_and(|(l, s)| *l == f && *s == selection)
        {
            return false;
        }
        // A frame on which only the **selection** moved is the selection-changed notice:
        // the fellow selection update alone, no flush and no rows. Rebuilding the list
        // there would throw the player's list pick away every time they clicked on the world.
        if self.last.as_ref().is_some_and(|(l, _)| *l == f) {
            self.update_fellow_selection(ui, view, f.as_ref());
            self.last = Some((f, selection));
            return true;
        }
        match f.as_ref() {
            None => {
                set_visible(ui, self.in_frame, false);
                set_visible(ui, self.not_in_frame, true);
                self.rows.clear();
                self.selected_fellow = None;
                if let Some(mut l) = self.list.take() {
                    l.flush(ui);
                    self.list = Some(l);
                }
                set_text(ui, self.fellowship_name, "");
            }
            Some(v) => {
                set_visible(ui, self.not_in_frame, false);
                set_visible(ui, self.in_frame, true);
                // A literal value, not a string-table token: the fellowship's name
                // is the player's own text and goes into the field as it stands.
                set_text(ui, self.fellowship_name, &v.name);
                self.rebuild(ui, v);
            }
        }
        self.update_fellow_selection(ui, view, f.as_ref());
        self.last = Some((f, selection));
        self.rebuilds += 1;
        true
    }

    /// The update's loop body: flush, then one row per member.
    fn rebuild(&mut self, ui: &mut UiSystem, v: &FellowshipView) {
        self.rows.clear();
        let Some(mut list) = self.list.take() else {
            return;
        };
        list.flush(ui);
        for m in &v.members {
            let Some(row) = list.add_from_template(ui, ROW_TEMPLATE, None) else {
                continue;
            };
            set_attr_instance_id(ui, row, ATTR_ROW_INSTANCE_ID, m.id.0);
            let name = fill(ui, ID_FELLOW_NAME, &[(var::NAME, &m.name)]);
            set_child_text(ui, row, ROW_NAME, &name);
            let stats = update_fellow_stats(ui, row, m);
            let (health, stamina, mana, meters) = update_fellow_vitals(ui, row, m);
            self.rows.push(FellowRow {
                id: m.id,
                name,
                stats,
                health,
                stamina,
                mana,
                meters,
                element: row,
            });
        }
        list.update_layout(ui);
        self.list = Some(list);
    }

    /// The fellowship panel's fellow selection update.
    ///
    /// It walks the rows reading each one's `0x1000000D` id. A row matching the world selection
    /// becomes the selected fellow and the list selection, and the update returns there. Otherwise
    /// the row matching the previously selected fellow (if any) is selected, and the buttons are
    /// updated.
    ///
    /// The early `return` matters and is kept: when the **world** selection is a fellow, that row
    /// becomes the list selection and the button update is *not* run from here. The fallback path
    /// keeps the previously selected fellow highlighted across a rebuild, which is the only reason
    /// the list survives a `0x02BE` without losing the player's pick.
    ///
    /// The list selection is set without broadcast, so no `4` is raised.
    fn update_fellow_selection(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        f: Option<&FellowshipView>,
    ) {
        let selected = view.selected_object();
        let mut fallback = None;
        for (i, r) in self.rows.iter().enumerate() {
            if Some(r.id) == selected {
                self.selected_fellow = selected;
                let list = self.list.as_ref().map(|l| l.handle);
                set_list_selection(ui, list, Some(i));
                return;
            }
            if Some(r.id) == self.selected_fellow {
                fallback = Some(i);
            }
        }
        if fallback.is_none() {
            self.selected_fellow = None;
        }
        let list = self.list.as_ref().map(|l| l.handle);
        set_list_selection(ui, list, fallback);
        self.update_buttons(ui, view, f);
    }

    /// The fellowship panel's button update.
    ///
    /// With no fellowship it does nothing. When the player leads, Disband and Open are enabled,
    /// Recruit is enabled when the world selection is a player who is not a fellow and the
    /// fellowship is not full, and Dismiss and Leader when a selected fellow other than the player
    /// is picked. When the player does not lead, Leader, Disband, Dismiss and Open are disabled and
    /// Recruit additionally needs the fellowship to be open. The Open button's caption is *Close…*
    /// while the fellowship is open and *Open…* while it is not.
    ///
    /// State `1` is enabled and state `0x0D` is disabled.
    ///
    /// **The Recruit button reads the *world* selection, not the list's.** That is
    /// the world selection — whoever the player clicked on in the world — and it is why
    /// recruiting is "select them, then press Recruit" and never involves typing a name.
    pub fn update_buttons(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        f: Option<&FellowshipView>,
    ) {
        let Some(v) = f else { return };
        set_enabled(ui, self.quit_button, true);
        let controls =
            dereth_client_contract::social::fellowship_controls(view, v, self.selected_fellow);
        set_enabled(ui, self.disband_button, controls.leads);
        set_enabled(ui, self.open_button, controls.leads);
        set_enabled(ui, self.dismiss_button, controls.selected_member);
        set_enabled(ui, self.leader_button, controls.selected_member);
        if let Some(enabled) = controls.recruit {
            set_enabled(ui, self.recruit_button, enabled);
        }
        let token = if v.open_fellow {
            ID_CLOSE_BUTTON_TEXT
        } else {
            ID_OPEN_BUTTON_TEXT
        };
        let caption = fill(ui, token, &[]);
        set_text(ui, self.open_button, &caption);
    }

    /// The fellowship panel's element-message handler.
    ///
    /// - Message 1, a button: Create (`0x10000274`) creates the fellowship; Leader (`0x1000027B`)
    ///   assigns leadership to the selected fellow; Quit (`0x1000027C`) first hands leadership to
    ///   the first non-leader fellow if the player leads, then quits with body `0`; Open
    ///   (`0x1000027D`) flips the openness flag, sends `Fellowship_ChangeFellowOpenness` and
    ///   updates the buttons; Recruit (`0x1000027E`) recruits the world selection; Dismiss
    ///   (`0x1000027F`) dismisses the selected fellow; Disband (`0x10000280`) quits with body `1`.
    /// - Messages 4 and `0x43`, the list: the selected row's `0x1000000D` id becomes the selected
    ///   fellow (`0` for no row); a non-zero id is also selected in the world, and used as the
    ///   target when target mode is on. Then the buttons are updated.
    /// - Messages `0x12` and `0x44`, the name entry box: Create is disabled while the box is empty
    ///   and enabled otherwise.
    ///
    /// The emptiness test is a length-of-1 check on a string whose length includes the
    /// terminator, so it means *the box is empty*.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::LIST_SELECTION_CHANGED || m.id == msg::LIST_ITEM_ACTIVATED {
            let Some(list) = self.list.as_ref().map(|l| l.handle) else {
                return false;
            };
            if list != m.source {
                return false;
            }
            // The selected index, then its row, then the row's own `0x1000000D`. The id is read
            // off the **element**, not out of `self.rows`, because that is where the client reads
            // it and because a row whose attribute never got written must not be pickable.
            let picked = list_selection(ui, list)
                .and_then(|i| self.rows.get(i))
                .map(|r| r.element)
                .and_then(|e| attr_instance_id(ui, e, ATTR_ROW_INSTANCE_ID))
                .filter(|v| *v != 0)
                .map(ObjectId);
            self.selected_fellow = picked;
            // The world selection write — picking a fellow in the list selects
            // them in the **world** too, which is what makes the Recruit button and the toolbar's
            // vital meters follow the list.
            if let Some(id) = picked {
                ui.requests.emit(UiRequest::Select(id));
            }
            let f = view.fellowship();
            self.update_buttons(ui, view, f.as_ref());
            return true;
        }
        if m.id == msg::CHARACTER || m.id == msg::TEXT_CHANGED {
            let Some(entry) = self.name_entry else {
                return false;
            };
            if entry != m.source {
                return false;
            }
            let empty = entry_text(ui, entry).is_empty();
            set_enabled(ui, self.create_button, !empty);
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        let f = view.fellowship();
        match m.source_id {
            CREATE_BUTTON => self.create_fellowship(ui, view),
            LEADER_BUTTON => {
                assign_leadership(&mut ui.requests, f.as_ref(), self.selected_fellow, view)
            }
            QUIT_BUTTON => quit(&mut ui.requests, f.as_ref(), view),
            OPEN_BUTTON => {
                let Some(v) = f.as_ref() else { return false };
                ui.requests.emit(UiRequest::FellowshipToggleOpenness);
                // The button update runs on the click, off the flag the host is about to flip, so
                // the caption changes now rather than on the next frame.
                let flipped = FellowshipView {
                    open_fellow: !v.open_fellow,
                    ..v.clone()
                };
                self.update_buttons(ui, view, Some(&flipped));
                true
            }
            RECRUIT_BUTTON => recruit(&mut ui.requests, f.as_ref(), view.selected_object(), view),
            DISMISS_BUTTON => dismiss(&mut ui.requests, f.as_ref(), self.selected_fellow, view),
            DISBAND_BUTTON => {
                ui.requests
                    .emit(UiRequest::FellowshipQuit { disband: true });
                true
            }
            _ => false,
        }
    }

    /// The fellowship panel's create-fellowship step.
    ///
    /// The name box's text → [`format_name`] → **written back into the box** → the create request
    /// with that name and the `FellowshipShareXP` option. The write-back is why a
    /// player sees their typing tidied the instant they press Create.
    fn create_fellowship(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(entry) = self.name_entry else {
            return false;
        };
        let name = format_name(&entry_text(ui, entry));
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text(&name);
        }
        ui.requests.emit(UiRequest::FellowshipCreate {
            name,
            share_xp: view.player_option(PlayerOption::FellowshipShareXP),
        });
        true
    }
}

/// `ID_Fellowship_FellowshipRequest`, one [`var::PLAYER`] variable — the prompt the fellow request
/// dialog builds for a `0x0274` of type **4**.
pub const ID_FELLOWSHIP_REQUEST: &str = "ID_Fellowship_FellowshipRequest";

/// The invitation prompt, with the inviter's name in it. **The `0x0274` type-4 half.**
///
/// The fellow request dialog build refuses while a fellow request dialog is already up (one at a
/// time, and the first wins). Otherwise it fills `ID_Fellowship_FellowshipRequest` with the
/// player's name, sets dialog property `0x8E` to enum `1` and `0xC5` to that string, makes the
/// dialog in the current UI (refusing if that fails), and records the server's context id to
/// answer with.
///
/// **It sets two properties where the gameplay screen's own confirmation dialog (the one that
/// also allows only one at a time) sets three**: `0xAC` (`true`) is absent, so the fellowship invitation does **not** block clicks or
/// come to the front. That is the one real difference between the two prompts, and it is why an
/// invitation can sit on screen while the player carries on.
///
/// The one-at-a-time slot itself lives with the dialog machinery in
/// `dereth_client_shell::target_confirmation`, which is where this build's `DialogController` is; a second
/// copy here could only disagree with it.
#[must_use]
pub fn fellowship_request_prompt(ui: &UiSystem, player: &str) -> String {
    fill(ui, ID_FELLOWSHIP_REQUEST, &[(var::PLAYER, player)])
}

/// The fellowship panel's fellow stats update — the one child `0x10000284`.
///
/// The percentage arithmetic is [`FellowEntry::xp_percent`]'s; this writes the string. Returns
/// what it wrote, so a test can read it without walking the tree.
fn update_fellow_stats(ui: &mut UiSystem, row: ElemHandle, m: &FellowEntry) -> String {
    let s = fill(
        ui,
        ID_FELLOW_STATS,
        &[
            (var::LEVEL, &m.level.to_string()),
            (var::EXPERIENCE, &m.xp_percent.to_string()),
        ],
    );
    set_child_text(ui, row, ROW_STATS, &s);
    s
}

/// Three meters and three labels.
///
/// The structure is the same three times and is **not** flat: the label is a child of the
/// *meter*, not of the row (the meter `0x10000285` is looked up from the row, then the label
/// `0x10000286` from the meter), so a build that writes the meter and skips the label
/// does not match, and a label searched for from the row would be found either way.
///
/// The meter's `0x69` fill is `cur / max` as floats; see [`meter_fill`] for the one
/// divergence.
fn update_fellow_vitals(
    ui: &mut UiSystem,
    row: ElemHandle,
    m: &FellowEntry,
) -> (String, String, String, [f32; 3]) {
    let one = |ui: &mut UiSystem, meter, text, token, cur: u32, max: u32| {
        let v = meter_fill(cur, max);
        let s = fill(
            ui,
            token,
            &[(var::CUR, &cur.to_string()), (var::MAX, &max.to_string())],
        );
        if let Some(h) = ui.get_child_recursive(row, meter) {
            ui.set_attribute_float(h, ATTR_METER, v);
            if let Some(t) = ui
                .get_child_recursive(h, text)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&s);
            }
        }
        (s, v)
    };
    let (health, h) = one(
        ui,
        ROW_HEALTH_METER,
        ROW_HEALTH_TEXT,
        ID_HEALTH_STATUS,
        m.current_health,
        m.max_health,
    );
    let (stamina, s) = one(
        ui,
        ROW_STAMINA_METER,
        ROW_STAMINA_TEXT,
        ID_STAMINA_STATUS,
        m.current_stamina,
        m.max_stamina,
    );
    let (mana, n) = one(
        ui,
        ROW_MANA_METER,
        ROW_MANA_TEXT,
        ID_MANA_STATUS,
        m.current_mana,
        m.max_mana,
    );
    (health, stamina, mana, [h, s, n])
}

fn quit(
    requests: &mut crate::requests::Outbox,
    _: Option<&FellowshipView>,
    _: &dyn GameView,
) -> bool {
    requests.emit(UiRequest::FellowshipQuit { disband: false });
    true
}
fn recruit(
    requests: &mut crate::requests::Outbox,
    _: Option<&FellowshipView>,
    target: Option<ObjectId>,
    _: &dyn GameView,
) -> bool {
    let Some(target) = target else { return false };
    requests.emit(UiRequest::FellowshipRecruit { target });
    true
}
fn dismiss(
    requests: &mut crate::requests::Outbox,
    _: Option<&FellowshipView>,
    target: Option<ObjectId>,
    _: &dyn GameView,
) -> bool {
    let Some(target) = target else { return false };
    requests.emit(UiRequest::FellowshipDismiss { target });
    true
}
fn assign_leadership(
    requests: &mut crate::requests::Outbox,
    _: Option<&FellowshipView>,
    target: Option<ObjectId>,
    _: &dyn GameView,
) -> bool {
    let Some(target) = target else { return false };
    requests.emit(UiRequest::FellowshipAssignNewLeader { target });
    true
}

/// Depth-first search for the one element of [`PANEL_TYPE`], the same shape
/// `allegiance::find_panel` has and for the same reason: the panel's own id is layout data; only
/// its element **type** is fixed by the client.
fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}

/// `cur / max` as floats, clamped where the client would divide by zero.
///
/// The client's fellow vitals update divides with no guard, so a `Fellow` whose record
/// has not arrived yet gives the meter an infinity. Zero is what an empty meter should draw; the
/// clamp is this build's and is named rather than hidden.
#[must_use]
pub fn meter_fill(cur: u32, max: u32) -> f32 {
    if max == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let v = cur as f32 / max as f32;
    v
}

pub use dereth_presentation::social::format_name;

/// Resolve a `StringInfo` token in [`STRING_TABLE`] and put `values` between its literal pieces.
///
/// The same renderer and the same fallback `allegiance`'s has, and for the same reason: with no
/// string table installed — every headless test — this answers the token followed by the values,
/// so a test can see **which** string was written and **what** went into it. An empty box would be
/// indistinguishable from a field that was never written. It renders through
/// [`UiSystem::resolve_string_rendered`], the string lookup's meta-language arm, rather than a
/// hand interleave.
fn fill(ui: &UiSystem, token: &str, values: &[(&str, &str)]) -> String {
    let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
    match ui.resolve_string_variant_count(STRING_TABLE, hash) {
        Some(n) if n == values.len() + 1 => ui
            .resolve_string_named(STRING_TABLE, hash, values)
            .unwrap_or_else(|| token.to_string()),
        _ if values.is_empty() => token.to_string(),
        _ => format!(
            "{token} {}",
            values.iter().map(|(_, v)| *v).collect::<Vec<_>>().join(" ")
        ),
    }
}

/// The name entry box's text, with the tags stripped the way every other
/// reader in this crate takes it (`panels::journal::get_text`, `options::page`).
fn entry_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, text: &str) {
    if let Some(t) = h.and_then(|h| ui.text_element_mut(h)) {
        t.set_text(text);
    }
}

fn set_child_text(ui: &mut UiSystem, row: ElemHandle, id: ElementId, text: &str) {
    if let Some(t) = ui
        .get_child_recursive(row, id)
        .and_then(|c| ui.text_element_mut(c))
    {
        t.set_text(text);
    }
}

fn set_visible(ui: &mut UiSystem, h: Option<ElemHandle>, v: bool) {
    if let Some(h) = h {
        ui.set_visible(h, v);
    }
}

/// State 1 / state `0x0D` — the enabled/disabled pair the button update uses.
fn set_enabled(ui: &mut UiSystem, h: Option<ElemHandle>, on: bool) {
    if let Some(h) = h {
        ui.set_state(
            h,
            if on {
                dereth_ui::widgets::button::state::NORMAL
            } else {
                dereth_ui::widgets::button::state::DISABLED
            },
        );
    }
}

/// Whether a bound button is currently offered — the observable half of
/// [`FellowshipPanel::update_buttons`], read back off the element rather than off a mirror.
///
/// A free function taking the handle rather than a method, for the reason
/// `titles::button_enabled` is: a caller holding the panel through `&App` cannot then borrow the
/// `UiSystem` out of the same `App`.
#[must_use]
pub fn button_enabled(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h)
        .is_some_and(|n| n.state != dereth_ui::widgets::button::state::DISABLED)
}

/// The list box's selected index, read off the live widget.
fn list_selection(ui: &UiSystem, list: ElemHandle) -> Option<usize> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

/// Sets the list selection without broadcast.
///
/// Written straight into the widget because `broadcast` is `false` at both of
/// the fellow selection update's call sites: with no broadcast the call *is* the field write, and
/// going through the behaviour would need `UiSystem::take_behaviour`, which is `pub(crate)` in
/// `dereth-ui` (see `panels::listbox::update_layout`'s note).
fn set_list_selection(ui: &mut UiSystem, list: Option<ElemHandle>, index: Option<usize>) {
    let Some(h) = list else { return };
    if let Some(l) = ui.node_mut(h).and_then(|n| {
        n.behaviour
            .as_mut()?
            .as_any_mut()?
            .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
    }) {
        l.selected = index.filter(|i| *i < l.items.len());
    }
}

/// Reads instance-id attribute `id` off `h`.
///
/// `dereth_ui::props::PropertyCollection` has no `get_instance_id`, so the match is written here;
/// the property this reads is the one [`FellowshipPanel::rebuild`] wrote with
/// `PropertyValue::InstanceId`, and a row carrying any other type answers `None` exactly as the
/// client's dynamic cast does.
fn attr_instance_id(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<u32> {
    match ui.node(h)?.merged_properties().get(id)? {
        dereth_assets::ui::PropertyValue::InstanceId(v) => Some(*v),
        _ => None,
    }
}

/// Writes `v` as instance-id attribute `id` on `h`.
fn set_attr_instance_id(ui: &mut UiSystem, h: ElemHandle, id: u32, v: u32) {
    let value = dereth_assets::ui::PropertyValue::InstanceId(v);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, value.clone());
    }
    ui.on_set_attribute(h, id, Some(&value));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The element ids are transcribed constants, so one test states them as **literals** rather
    /// than reading them back through the same symbol that wrote them (the stated testability rule: *"a test
    /// that reads a constant through the same symbol it writes it through cannot detect a wrong
    /// constant"*). Source: the fellowship panel's initialization, the update and
    /// the fellow vitals update.
    #[test]
    fn the_element_ids_match_post_init() {
        assert_eq!(PANEL_TYPE, ElementType(0x1000_002D));
        assert_eq!(NOT_IN_FELLOWSHIP_FRAME, ElementId(0x1000_026B));
        assert_eq!(NAME_ENTRY_BOX, ElementId(0x1000_026F));
        assert_eq!(CREATE_BUTTON, ElementId(0x1000_0274));
        assert_eq!(IN_FELLOWSHIP_FRAME, ElementId(0x1000_0275));
        assert_eq!(FELLOWSHIP_NAME, ElementId(0x1000_0276));
        assert_eq!(FELLOWS_LIST, ElementId(0x1000_0279));
        assert_eq!(LEADER_BUTTON, ElementId(0x1000_027B));
        assert_eq!(QUIT_BUTTON, ElementId(0x1000_027C));
        assert_eq!(OPEN_BUTTON, ElementId(0x1000_027D));
        assert_eq!(RECRUIT_BUTTON, ElementId(0x1000_027E));
        assert_eq!(DISMISS_BUTTON, ElementId(0x1000_027F));
        assert_eq!(DISBAND_BUTTON, ElementId(0x1000_0280));
        assert_eq!(ATTR_ROW_INSTANCE_ID, 0x1000_000D);
        assert_eq!(ATTR_METER, 0x69);
        assert_eq!(STRING_TABLE, DataId(0x2300_0001));
    }

    /// Oracle: `panels::rows::FELLOWSHIP_VITAL_ELEMENTS`, which has named `0x10000283`..`0x1000028A`
    /// since before this panel existed and had **no production consumer** until now. The eight are
    /// stated here in the order the update and the fellow vitals update walk them.
    #[test]
    fn the_eight_row_elements_are_the_dead_tables_range() {
        let mine = [
            ROW_NAME.0,
            ROW_STATS.0,
            ROW_HEALTH_METER.0,
            ROW_HEALTH_TEXT.0,
            ROW_STAMINA_METER.0,
            ROW_STAMINA_TEXT.0,
            ROW_MANA_METER.0,
            ROW_MANA_TEXT.0,
        ];
        assert_eq!(
            mine,
            [
                0x1000_0283,
                0x1000_0284,
                0x1000_0285,
                0x1000_0286,
                0x1000_0287,
                0x1000_0288,
                0x1000_0289,
                0x1000_028A,
            ]
        );
        let range: Vec<u32> = super::super::rows::FELLOWSHIP_VITAL_ELEMENTS.collect();
        assert_eq!(
            range,
            mine.to_vec(),
            "the catalogue's dead table is exactly these eight"
        );
        assert_eq!(
            super::super::rows::row_attribute("FellowshipPanel"),
            Some(ATTR_ROW_INSTANCE_ID)
        );
    }

    /// Oracle: the client's name formatter, and the one example the corpus carries —
    /// `fellowship-two-monarch.jsonl` t=338.829 sends the `PString` `"Of The ring"`, twice.
    ///
    /// The first assertion is the one that matters: a title-caser gives `"Of The Ring"` and the
    /// wire says otherwise. The character after a space keeps the case it was typed in.
    #[test]
    fn format_name_leaves_the_letter_after_a_space_alone_and_lowers_the_rest() {
        assert_eq!(
            format_name("of The ring"),
            "Of The ring",
            "the capture's own name, and the reason this is not a title-caser"
        );
        assert_eq!(
            format_name("OF THE RING"),
            "Of The Ring",
            "the rest of each word lowers"
        );
        assert_eq!(format_name("  bob   the  builder "), "Bob the builder");
        assert_eq!(format_name(""), "");
        // The filter: digits and punctuation other than `'` and `-` are dropped outright, a run
        // of spaces collapses, and a trailing space goes.
        assert_eq!(format_name("ab3c!d "), "Abcd");
        // A hyphen needs a letter either side; an apostrophe needs one on one side.
        assert_eq!(
            format_name("-bob"),
            "Bob",
            "a leading hyphen has no letter before it"
        );
        assert_eq!(
            format_name("macdonald"),
            "Macdonald",
            "`Mac` leaves the next letter as typed"
        );
        assert_eq!(
            format_name("macDonald"),
            "MacDonald",
            "…which is what makes this survive"
        );
        assert_eq!(
            format_name("mc'moo"),
            "Mc'moo",
            "the letter after the apostrophe is as typed"
        );
        // The Roman-numeral run is **upper-case only** — the client compares against `0x49`,
        // `0x56` and `0x58` and nothing else, so a typed `iii` is not one.
        assert_eq!(format_name("bob III"), "Bob III");
        assert_eq!(format_name("bob iii"), "Bob iii");
    }

    /// Oracle: the client's division, and this build's one named divergence.
    #[test]
    fn the_meter_fill_is_the_ratio_and_zero_when_max_is_zero() {
        assert!((meter_fill(50, 100) - 0.5).abs() < 1e-6);
        assert!((meter_fill(100, 100) - 1.0).abs() < 1e-6);
        assert_eq!(
            meter_fill(0, 0),
            0.0,
            "the client divides by zero here and this does not"
        );
        assert_eq!(meter_fill(7, 0), 0.0);
    }
}
