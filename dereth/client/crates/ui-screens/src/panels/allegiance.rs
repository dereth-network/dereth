//! The allegiance panel's roster.
//!
//! `dereth-client-model` rebuilds an `AllegianceHierarchy` on every `0x0020`; this panel is what
//! walks it, through `get_patron`, `get_first_vassal`, `get_next_vassal`, `get_monarch_id` and
//! `set_may_passup_experience`.
//!
//! # What the corpus can and cannot witness
//!
//! **It cannot witness a roster, and this is measured, not assumed.** All three capture sessions
//! carry `0x0020 Allegiance_AllegianceUpdate` — twelve of them — and
//! **every one carries allegiance version 11, rank 0, `total_members = 0` and zero member
//! records**. The recorded account is in no allegiance. So the chrome has an oracle and the roster
//! does not, and the only oracle available for a populated tree is a **synthesised** `0x0020`,
//! declared as synthesised wherever it is used. A capture of a character in a real allegiance is
//! still needed because no recorded session contains a populated allegiance tree.
//!
//! That makes the **empty** case load-bearing rather than trivial: it is the one case the corpus
//! *can* witness, and a build that shows nothing must be distinguishable from a build that shows
//! nothing correctly. [`AllegiancePanel::update`] therefore reports whether it ran, and
//! [`AllegiancePanel::rows`] reports what it wrote, so "no members" and "never drove the panel"
//! are different observations.
//!
//! # The client
//!
//! The client binds thirteen children; the four data-update steps fill them:
//!
//! | function | walk | writes |
//! |---|---|---|
//! | player | the player's profile record | allegiance name (`0x10000251`), follower count (`0x10000252`) = the header's total vassals, rank (`0x10000253`) |
//! | monarch | the profile's monarch id, then that member's record | monarch name (`0x10000257`), follower count (`0x10000258`) = the header's total members − 1, label (`0x10000256`) |
//! | patron | the player's patron record | patron name (`0x1000025C`) |
//! | vassals | a first/next walk over the player's vassals | one row per vassal in the list (`0x10000260`) |
//!
//! Neither Followers box reads the tree: both come off the two header dwords of the profile (total
//! vassals and total members). That matters because the server sends only the player's
//! neighbourhood of the tree: `fellowship idx 351` has 3 records for an allegiance of 3 in which
//! the player has 1 follower.
//!
//! The monarch-data update picks its label by whether the monarch **is** the patron:
//! `ID_Allegiance_PatronSlashMonarchLabel` when they are the same node,
//! `ID_Allegiance_MonarchLabel` otherwise.

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, ElementId, ElementType, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{AllegianceAction, AllegianceRoster, GameView, UiRequest};

/// The allegiance panel's registered element type, `0x1000002C`, which the panel's own root
/// carries in the shipped layout.
///
/// This is the element whose visibility changes drive the retail handler represented by
/// [`AllegiancePanel::on_visibility_changed`], so its effective visibility
/// decides whether the roster is subscribed. It is found by **type** rather than by id for the
/// same reason [`crate::hud::powerbar::PowerBars::post_init`] uses: initialization binds thirteen
/// *children* but never names the panel's own id, which remains layout data.
pub const PANEL_TYPE: ElementType = ElementType(0x1000_002C);

/// The allegiance-name field.
pub const ALLEGIANCE_NAME: ElementId = ElementId(0x1000_0251);
/// The player's follower-count field.
pub const PLAYER_FOLLOWERS: ElementId = ElementId(0x1000_0252);
/// The player's rank field.
pub const PLAYER_RANK: ElementId = ElementId(0x1000_0253);
/// The monarch label.
pub const MONARCH_LABEL: ElementId = ElementId(0x1000_0256);
/// The monarch-name field.
pub const MONARCH_NAME: ElementId = ElementId(0x1000_0257);
/// The monarch's follower-count field.
pub const MONARCH_FOLLOWERS: ElementId = ElementId(0x1000_0258);
/// The MONARCH section's group box. The monarch-data update changes visibility on this element.
pub const MONARCH_FIELD: ElementId = ElementId(0x1000_0255);
/// The child of [`MONARCH_FIELD`] shown only when the player's patron and monarch are the same
/// person. It is the "this person is also your patron" block.
/// The block is hidden for every other monarch relationship.
pub const MONARCH_PATRON_BLOCK: ElementId = ElementId(0x1000_0490);
/// The `ID_Allegiance_VassalExperiencePassedUp` text nested in a visible patron block.
pub const EXPERIENCE_PASSED_UP: ElementId = ElementId(0x1000_0492);
/// The PATRON section's group box and the target of its visibility and state changes.
pub const PATRON_FIELD: ElementId = ElementId(0x1000_025A);
/// The patron-name field.
pub const PATRON_NAME: ElementId = ElementId(0x1000_025C);
/// The vassal list.
pub const VASSAL_LIST: ElementId = ElementId(0x1000_0260);

/// The *"Swear Allegiance"* button.
pub const SWEAR_BUTTON: ElementId = ElementId(0x1000_0263);
/// The *"Break Allegiance"* button.
pub const BREAK_BUTTON: ElementId = ElementId(0x1000_0264);
/// The *"Break Vassal's Allegiance"* button.
pub const KICK_BUTTON: ElementId = ElementId(0x1000_0265);

/// The vassal row's name text — the first recursive descendant lookup while building a new row,
/// and the **only** field written directly rather than through a `StringInfo`.
pub const ROW_NAME: ElementId = ElementId(0x1000_0268);
/// The vassal row's "experience passed up" text.
pub const ROW_EXPERIENCE: ElementId = ElementId(0x1000_0269);
/// The vassal row's logged-**out** marker.
///
/// The vassal-data update shows this marker when the member is **not** logged in and hides it
/// when the member is online. That is the opposite polarity to
/// the `" *"` suffix `allegiance_info_block` puts on an online one. \[verified\]
pub const ROW_LOGGED_OUT: ElementId = ElementId(0x1000_04AA);

/// The row attribute `0x10000001`, where the vassal id is stored for row selection and the kick
/// button to read back.
pub const ATTR_ROW_INSTANCE_ID: u32 = 0x1000_0001;

/// The template index used to add vassal rows.
/// The list box carries exactly one template, `(0x2100002F, 0x10000266)`.
pub const VASSAL_ROW_TEMPLATE: usize = 0;

/// The talk-focus rows written after patron, monarch and vassal updates: 4, 5 and 6 respectively.
/// They correspond to `dereth_client_model::chat::TalkFocus::{Patron, Monarch, Vassals}`; this crate cannot
/// depend on that enum directly, so the numbers are pinned here.
/// Their numeric ordering is part of the cross-crate request contract.
/// No other focus row is written by this panel.
pub const TALK_FOCUS_PATRON: u32 = 4;
pub const TALK_FOCUS_MONARCH: u32 = 5;
pub const TALK_FOCUS_VASSALS: u32 = 6;

/// The state each section's group box receives from whether its person is logged in: `0x0D`
/// when logged out, for both the patron and monarch fields.
#[must_use]
pub fn online_state(logged_in: bool) -> dereth_ui::StateId {
    dereth_ui::StateId(if logged_in { 1 } else { 0xd })
}

/// The table against which every string on this panel resolves, selected by id `0x10000001`.
/// It is the same mapped group-4 entry as
/// [`crate::chat::mainchat::CAPTION_STRING_TABLE`].
pub const STRING_TABLE: DataId = DataId(0x2300_0001);

/// `ID_Allegiance_CharacterName`, one [`var::NAME`] variable.
pub const ID_CHARACTER_NAME: &str = "ID_Allegiance_CharacterName";
/// `ID_Allegiance_Followers`, one [`var::FOLLOWERS`] variable.
pub const ID_FOLLOWERS: &str = "ID_Allegiance_Followers";
/// `ID_Allegiance_Rank`, two variables: [`var::TITLE`] then [`var::RANK`].
pub const ID_RANK: &str = "ID_Allegiance_Rank";
/// `ID_Allegiance_RankBuffed`, three variables: [`var::TITLE`], [`var::RANK`] (the enchanted
/// rank) and [`var::RANK_BUFF`] (how far the enchantments moved it).
pub const ID_RANK_BUFFED: &str = "ID_Allegiance_RankBuffed";
/// `ID_Allegiance_MonarchLabel`, no variables.
pub const ID_MONARCH_LABEL: &str = "ID_Allegiance_MonarchLabel";
/// `ID_Allegiance_PatronSlashMonarchLabel`, no variables — used when the monarch is also the
/// player's patron.
pub const ID_PATRON_SLASH_MONARCH_LABEL: &str = "ID_Allegiance_PatronSlashMonarchLabel";
/// `ID_Allegiance_VassalExperiencePassedUp`, one [`var::VALUE`] variable.
pub const ID_EXPERIENCE_PASSED_UP: &str = "ID_Allegiance_VassalExperiencePassedUp";

/// `ID_Allegiance_SwearConfirmation`, one [`var::PLAYER`] variable — the selected object's name.
pub const ID_SWEAR_CONFIRMATION: &str = "ID_Allegiance_SwearConfirmation";
/// `ID_Allegiance_BreakConfirmation`, one [`var::PLAYER`] variable — the **patron's** name.
pub const ID_BREAK_CONFIRMATION: &str = "ID_Allegiance_BreakConfirmation";
/// `ID_Allegiance_KickConfirmation`, one [`var::PLAYER`] variable — the selected **vassal's** name.
pub const ID_KICK_CONFIRMATION: &str = "ID_Allegiance_KickConfirmation";
/// `ID_Allegiance_AcceptSwearConfirmation`, one [`var::PLAYER`] variable — the name the **server**
/// sent in `0x0274`'s text field.
pub const ID_ACCEPT_SWEAR_CONFIRMATION: &str = "ID_Allegiance_AcceptSwearConfirmation";

/// The **variable names** this panel files its values under.
///
/// They are what the string renderer matches on: it walks the row's own variable list and looks
/// every id up in the caller's table, so these names — not the order of the `fill` arguments —
/// decide where a value lands. All measured off `0x23000001`.
pub mod var {
    /// A character's full name, in the three confirmations.
    pub const PLAYER: &str = "PLAYER";
    /// The allegiance's or a member's name, in `ID_Allegiance_CharacterName`.
    pub const NAME: &str = "NAME";
    /// The follower count.
    pub const FOLLOWERS: &str = "FOLLOWERS";
    /// The rank's title word, in `ID_Allegiance_Rank` (`TITLE, RANK`).
    pub const TITLE: &str = "TITLE";
    /// The rank number, in the same row.
    pub const RANK: &str = "RANK";
    /// The enchantments' change to the rank, in `ID_Allegiance_RankBuffed` (`TITLE, RANK, RANKBUFF`).
    pub const RANK_BUFF: &str = "RANKBUFF";
    /// `_cp_tithed`, in `ID_Allegiance_VassalExperiencePassedUp`.
    pub const VALUE: &str = "VALUE";
}

/// The client's prompt, for the host that owns the dialog factory.
///
/// It is a free function rather than a method because the question arrives from the *network* and
/// not from this panel: it is answered by a notice handler, and this build's notice bus for
/// `0x0274` lives in `dereth_client`. The string table it resolves
/// against is this crate's, which is the only reason the resolution happens here.
#[must_use]
pub fn accept_swear_prompt(ui: &UiSystem, name: &str) -> String {
    fill(ui, ID_ACCEPT_SWEAR_CONFIRMATION, &[(var::PLAYER, name)])
}

/// What an oath costs the player on a world that charges experience for one
/// ([`dereth_primitives::EraFeatures::swear_xp_cost`]): nothing before a first break from a patron,
/// otherwise the shared rule over the player's next-level step and its break count. `None` on a
/// world without the charge.
#[must_use]
pub fn oath_xp_cost(view: &dyn GameView) -> Option<u32> {
    view.oath_xp_cost()
}

/// One row as this panel wrote it, so a test can read back what a player would see without
/// re-walking the element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VassalRow {
    pub id: ObjectId,
    /// What went into [`ROW_NAME`].
    pub name: String,
    /// What went into [`ROW_EXPERIENCE`].
    pub experience: String,
    /// Whether [`ROW_LOGGED_OUT`] was made visible — true for an **offline** vassal.
    pub logged_out_marker: bool,
}

/// The allegiance panel, bound to a live tree.
#[derive(Debug, Default)]
pub struct AllegiancePanel {
    pub allegiance_name: Option<ElemHandle>,
    pub player_followers: Option<ElemHandle>,
    pub player_rank: Option<ElemHandle>,
    pub monarch_label: Option<ElemHandle>,
    pub monarch_name: Option<ElemHandle>,
    pub monarch_followers: Option<ElemHandle>,
    pub patron_name: Option<ElemHandle>,
    /// [`MONARCH_FIELD`] — the section the monarch-data update shows, hides and changes state on.
    pub monarch_field: Option<ElemHandle>,
    /// [`PATRON_FIELD`] — the section the patron-data update shows, hides and changes state on.
    pub patron_field: Option<ElemHandle>,
    /// How many talk-focus writes for rows 4, 5 or 6 this panel has raised — the third state that
    /// separates "the channels are off because nobody is online" from "nothing ever decided".
    pub talk_focus_writes: u32,
    /// The bound vassal list.
    pub list: Option<ListBoxWidget>,
    /// The Swear, Break and Kick buttons.
    pub swear_button: Option<ElemHandle>,
    pub break_button: Option<ElemHandle>,
    pub kick_button: Option<ElemHandle>,
    /// What [`Self::refresh_buttons`] last wrote, so a test can read the three enable decisions
    /// without walking the tree — and so an unbound panel is distinguishable from a panel that
    /// decided "disabled". `None` until the first refresh.
    pub button_states: Option<[bool; 3]>,
    /// The rows currently on show, in list order — newest vassal first.
    rows: Vec<VassalRow>,
    /// The selected vassal id, which the vassal-data update clears before rebuilding.
    pub selected_vassal: Option<ObjectId>,
    /// The last value written for talk-focus row 6 — the vassals chat channel is enabled
    /// only while at least one vassal is online. The last value this panel computed.
    pub vassal_chat_enabled: bool,
    /// The roster the tree was last built for, guarding against redundant rebuilds.
    last: Option<AllegianceRoster>,
    /// How many times [`Self::update`] has rebuilt the panel. **Three states, not two**: this
    /// separates "rebuilt and there were no members" from "never ran", which is the whole
    /// difference between an empty allegiance and an unwired panel.
    pub rebuilds: u32,
    /// The allegiance-panel element itself — the one [`PANEL_TYPE`] names.
    pub panel: Option<ElemHandle>,
    /// The panel's last observed **effective** visibility across the whole parent chain, which
    /// is what [`Self::poll_visibility`] takes edges of.
    visible: bool,
    /// The busy-count latch, set on the show path and
    /// cleared by the hide path or by the next [`Self::update`].
    ///
    /// **It is not a send gate.** All five update-request sites in
    /// the panel sit *outside* the block that arms the busy latch, so the request
    /// goes out whether or not one is already outstanding — which is why
    /// recorded fellowship traffic contains two `0x001F` requests within 0.1 ms at t=322.441.
    pub awaiting_update: bool,
    /// [`GameView::allegiance_update_aborts`] as of the last frame — the edge
    /// [`Self::poll_update_aborted`] takes.
    last_abort: u64,
    /// [`GameView::allegiance_updates`] as of the last [`Self::update`]: the edge that says an
    /// answer arrived. `None` until the first update after the panel is built.
    last_updates: Option<u64>,
    /// How many aborts this panel has acted on: an abort updates only while the panel is visible.
    /// Deliberately **not** the number that arrived, because retail's receiver runs nothing when
    /// the panel is down and this build must not either.
    pub aborts_handled: u32,
    /// How many `0x001F Allegiance_UpdateRequest` this panel has emitted. The observable that
    /// separates "the panel is empty because the allegiance is empty" from "the panel is empty
    /// because nobody ever asked".
    pub update_requests: u32,
    /// Whether a player description existed on the previous frame — the edge
    /// [`Self::poll_player_desc`] uses to detect receipt of the player description.
    player_desc: bool,
    /// The player's `InstanceID` quality 26 as of the previous frame — the edge
    /// [`Self::poll_player_desc`] takes.
    ///
    /// `Option<Option<_>>`: the outer layer is "has this panel ever looked", because a player who
    /// has no monarch and never gets one must not count as a change on the first frame.
    monarch_quality: Option<Option<ObjectId>>,
    /// How many confirmation dialogs this panel has asked the host for, indexed by
    /// [`AllegianceAction`].
    ///
    /// **The open-dialog guard is deliberately *not* here.** Retail keeps the current confirmation
    /// context on the panel and returns early from dialog creation while that context is
    /// non-zero; this build's panels raise requests into a one-way queue and are never told that a
    /// dialog closed, so a latch stored here would arm on the first press and never disarm —
    /// the button would work exactly once per screen rebuild. The guard therefore lives where the
    /// dialog does, in `dereth_client::target_confirmation`, which is where the two guards of the
    /// same shape that already exist for the vendor and the server's five gameplay-confirmation
    /// types live.
    pub confirmations: [u32; 3],
}

impl AllegiancePanel {
    /// The allegiance panel's post-init, minus the button and notice registrations.
    ///
    /// Bound off the screen **root** rather than off a panel page: every id here is unique in the
    /// shipped tree, and the page containing the allegiance panel is the toolbar's social page, which
    /// this crate's panel stack builds lazily.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        let get = |ui: &UiSystem, id: ElementId| ui.get_child_recursive(root, id);
        self.allegiance_name = get(ui, ALLEGIANCE_NAME);
        self.player_followers = get(ui, PLAYER_FOLLOWERS);
        self.player_rank = get(ui, PLAYER_RANK);
        self.monarch_label = get(ui, MONARCH_LABEL);
        self.monarch_name = get(ui, MONARCH_NAME);
        self.monarch_followers = get(ui, MONARCH_FOLLOWERS);
        self.patron_name = get(ui, PATRON_NAME);
        // The post-init bindings for the monarch (`0x10000255`) and patron (`0x1000025A`)
        // fields, whose ids `catalogue.rs` also carries.
        self.monarch_field = get(ui, MONARCH_FIELD);
        self.patron_field = get(ui, PATRON_FIELD);
        self.talk_focus_writes = 0;
        self.list = get(ui, VASSAL_LIST).map(|h| ListBoxWidget::bind(ui, h));
        // The last three recursive child bindings during post-initialization.
        self.swear_button = get(ui, SWEAR_BUTTON);
        self.break_button = get(ui, BREAK_BUTTON);
        self.kick_button = get(ui, KICK_BUTTON);
        self.button_states = None;
        self.rows.clear();
        self.last = None;
        self.selected_vassal = None;
        self.vassal_chat_enabled = false;
        // The panel's own element, and the visibility it starts at. The child
        // set-up's trailing hide loop has already run by the time this is
        // called, so a freshly built tree starts hidden and the player's first open is a rising
        // edge. Recording it here rather than defaulting to `false` is what stops a rebuilt screen
        // from emitting a spurious `0x001F` for a panel that was already up.
        self.panel = find_panel(ui, root);
        self.visible = self.panel.is_some_and(|h| ui.is_visible(h));
        self.player_desc = false;
        self.monarch_quality = None;
        self.confirmations = [0; 3];
        // **The post-initialization update request, the third of retail's five request sites and
        // the one that fires before the player has touched anything.**
        //
        // The client arms the busy latch and then emits request value 1 **unconditionally** — no
        // player-description query and no visibility test. It is the
        // last statement of the function, after the thirteen bindings, the two quality
        // registrations and the nine notice registrations.
        //
        // It is what puts the first `0x001F` of every recorded session in the second after the
        // `0xF745` object-create burst, before any tab is open, and it is why a retail player sees
        // a populated Allegiance tab the *first* time they open it rather than a frame later.
        // Retail asks twice before the tab first comes up.
        //
        // A new panel starts with the latch clear, so the first request always puts the busy
        // cursor up until the answer arrives.
        self.awaiting_update = false;
        self.last_updates = None;
        self.arm_busy_latch(&mut ui.requests);
        ui.requests
            .emit(UiRequest::AllegianceUpdateRequest { on: true });
        self.update_requests += 1;
    }

    /// Arm the busy latch when it is clear, which puts the busy cursor up: a request is
    /// outstanding.
    fn arm_busy_latch(&mut self, requests_out: &mut crate::requests::Outbox) {
        if !self.awaiting_update {
            self.awaiting_update = true;
            requests_out.emit(UiRequest::Busy { raised: true });
        }
    }

    /// Clear the busy latch when it is armed, which takes the busy cursor's raise back.
    fn clear_busy_latch(&mut self, requests_out: &mut crate::requests::Outbox) {
        if self.awaiting_update {
            self.awaiting_update = false;
            requests_out.emit(UiRequest::Busy { raised: false });
        }
    }

    /// The allegiance panel's player-description and quality-change edges account for
    /// the two remaining update-request sites,
    /// both of them edges this build has to take for itself.
    ///
    /// Both handlers are the same three statements:
    ///
    /// ```text
    /// if the busy latch is clear, set it and increment the busy count;
    /// emit update request 1;
    /// ```
    ///
    /// The send is unconditional, with the latch *outside* it exactly as at every other request
    /// site.
    ///
    /// * The player-description notice is raised when `0x0013` lands. Here
    ///   that is [`GameView::player`] going from `None` to `Some`, which is the same event this
    ///   crate already calls `LOGIN_COMPLETE`.
    /// * The quality-change handler is registered during panel initialization on the player's `InstanceID`
    ///   qualities 25 and 26. See [`GameView::allegiance_monarch_quality`] for why only 26 is
    ///   available and why that is narrower rather than different.
    ///
    /// Returns how many requests it emitted — 0, 1 or 2, because a login that also delivers the
    /// monarch quality in the same frame genuinely fires both handlers in retail.
    pub fn poll_player_desc(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        view: &dyn GameView,
    ) -> u32 {
        let mut sent = 0;
        let desc = view.player().is_some();
        if desc && !self.player_desc {
            self.emit_update_request(requests_out);
            sent += 1;
        }
        self.player_desc = desc;
        let monarch = view.allegiance_monarch_quality();
        if self.monarch_quality.is_some_and(|prev| prev != monarch) {
            self.emit_update_request(requests_out);
            sent += 1;
        }
        self.monarch_quality = Some(monarch);
        sent
    }

    /// The three-statement body every non-visibility call site shares.
    fn emit_update_request(&mut self, requests_out: &mut crate::requests::Outbox) {
        self.arm_busy_latch(requests_out);
        requests_out.emit(UiRequest::AllegianceUpdateRequest { on: true });
        self.update_requests += 1;
    }

    /// The allegiance panel's visibility-changed handler: the send that subscribes the roster.
    ///
    /// ```text
    /// when shown:
    ///     query the player description;
    ///     if it exists and the interface query succeeds:
    ///         if the busy latch is clear, set it and increment the busy count;
    ///         emit update request 1;
    /// when hidden:
    ///     if the busy latch is set, clear it and decrement the busy count;
    ///     emit update request 0 unconditionally, without a player-description guard;
    /// ```
    ///
    /// The successful show path therefore both accounts for the outstanding request and emits it.
    /// A failed player-description query emits nothing. The hide path always emits its value even
    /// when there is no busy count to release, preserving the original asymmetry rather than
    /// treating visibility as a single symmetric subscription toggle.
    ///
    /// The asymmetry is real and is kept rather than tidied: the **show** arm is guarded on a live
    /// player description and the **hide** arm is not. `player_desc` is this build's answer to
    /// that query — [`GameView::player`] is `Some` exactly once `LOGIN_COMPLETE` has fired, which
    /// is when the player system publishes the interface.
    ///
    /// Returns whether a request was emitted.
    pub fn on_visibility_changed(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        visible: bool,
        player_desc: bool,
    ) -> bool {
        if visible {
            if !player_desc {
                return false;
            }
            self.arm_busy_latch(requests_out);
        } else {
            self.clear_busy_latch(requests_out);
        }
        requests_out.emit(UiRequest::AllegianceUpdateRequest { on: visible });
        self.update_requests += 1;
        true
    }

    /// The edge detector the panel host drives once a frame.
    ///
    /// In the client the visibility change is delivered to the panel synchronously, so retail
    /// needs no poll. This crate's panels are plain structs bound
    /// to a subtree and receive no such callback, so the edge is taken here — off
    /// `UiSystem::is_visible`, which answers for the whole parent chain. That matters: the
    /// Allegiance tab only counts as shown when the
    /// social **page** is the current panel *and* the tab within it is up, and one `is_visible`
    /// call asks both questions.
    ///
    /// Returns whether a request was emitted.
    pub fn poll_visibility(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(h) = self.panel else { return false };
        let now = ui.is_visible(h);
        if now == self.visible {
            return false;
        }
        self.visible = now;
        self.on_visibility_changed(&mut ui.requests, now, view.player().is_some())
    }

    /// Whether the panel is currently up, as this module last saw it.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// True once the vassal list box was found — the one binding without which nothing can be
    /// drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// The rows this panel last wrote, newest vassal first.
    #[must_use]
    pub fn rows(&self) -> &[VassalRow] {
        &self.rows
    }

    /// The allegiance panel's allegiance-update-aborted notice.
    ///
    /// Check the visibility byte on the notice subobject. A hidden panel does nothing;
    /// otherwise adjust back to the containing panel and update it. The notice's integer
    /// argument is never read.
    ///
    /// **Why this cannot be left to [`Self::update`]'s own per-frame poll**, which is the obvious
    /// objection: `update` is guarded on the roster *snapshot*, and an aborted update is by
    /// definition one where the roster did not change. What matters here is the handler's
    /// **first** action: clear an armed busy latch and decrement the busy count. Without this poll,
    /// the latch set by every update request
    /// stays set for ever on a request the server aborts, and the panel is stuck busy with no
    /// answer coming. That is the whole player-visible content of `0x0003`.
    ///
    /// The snapshot is cleared too, so the next [`Self::update`] rewrites the four blocks rather
    /// than taking its no-op arm.
    ///
    /// Returns whether an abort was acted on.
    pub fn poll_update_aborted(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        view: &dyn GameView,
    ) -> bool {
        let now = view.allegiance_update_aborts();
        if now == self.last_abort {
            return false;
        }
        self.last_abort = now;
        // The visibility byte means "visible", not "bound". A panel that is down runs nothing,
        // and the latch it is not holding stays not held.
        if !self.visible {
            return false;
        }
        self.aborts_handled += 1;
        self.clear_busy_latch(requests_out);
        self.last = None;
        true
    }

    /// Run the four data updates in retail order. Returns true on a frame that actually rewrote
    /// the panel.
    ///
    /// An empty roster is **not** an early return: it flushes the list, clears the four text
    /// fields and counts a rebuild, because a character who leaves an allegiance must see the
    /// panel empty rather than see the previous allegiance's roster.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The client's update handler begins by clearing an armed busy latch, which takes the
        // busy cursor down: an answer has arrived. It runs on every answer, including one that
        // leaves the roster as it was, so it is keyed on the answers' count and not on the
        // roster snapshot below.
        let updates = view.allegiance_updates();
        if self.last_updates.is_some_and(|n| n != updates) {
            self.clear_busy_latch(&mut ui.requests);
        }
        self.last_updates = Some(updates);
        let r = view.allegiance_roster();
        if self.last.as_ref() == Some(&r) {
            return false;
        }
        // Only the player's enchanted rank moved: the enchantments-changed notice reruns the
        // player-data update alone. It is not an allegiance update, so the other three blocks, the
        // rebuild count and the busy latch are left as they are.
        if let Some(last) = &self.last {
            let rank_only = AllegianceRoster {
                player_rank_quality: last.player_rank_quality,
                ..r.clone()
            };
            if &rank_only == last {
                self.update_player_data(ui, &r);
                self.last = Some(r);
                return true;
            }
        }
        self.update_player_data(ui, &r);
        self.update_monarch_data(ui, &r);
        self.update_patron_data(ui, &r);
        self.update_vassals_data(ui, &r);
        self.last = Some(r);
        self.rebuilds += 1;
        true
    }

    /// Apply the allegiance panel's Swear, Break and Kick button rules.
    ///
    /// Retail evaluates the first two as dedicated updates and the third on list-selection
    /// message 4. This build has no selection-notice bus,
    /// so they are re-evaluated once a frame beside [`Self::update`] and written only when a
    /// decision moved.
    ///
    /// ```text
    /// Swear is enabled when:
    ///   the player has no patron, the selection is nonzero and is not the player,
    ///   the selected object exists and is a player, and it is not already in the allegiance;
    ///
    /// Break is enabled when the player has a patron;
    ///
    /// Kick is enabled when the selected-vassal id is nonzero.
    ///
    /// The selected object must satisfy every Swear predicate at once. A missing or non-player
    /// object, the player themself, or an existing allegiance member keeps the button disabled,
    /// while Break depends only on the patron relationship already in the snapshot.
    /// The method returns whether any of the three decisions changed.
    /// ```
    ///
    /// Note that Swear's first clause is the **negation** of Break's: the two buttons are never
    /// both live. Returns whether anything was written.
    pub fn refresh_buttons(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // Read the player's patron from the snapshot [`Self::update`] already took this frame rather
        // than by re-walking the hierarchy: `allegiance_roster` rebuilds every member's full name,
        // and this runs once a frame for the whole session. `update` is called first by
        // `RemainingPanels::update`, so the snapshot is this frame's; before the first `update`
        // there is none, and "no snapshot" is the same answer as "no patron" — both leave Swear
        // and Break in the state the child set-up's hide loop left them.
        let empty = AllegianceRoster::default();
        let roster = self.last.as_ref().unwrap_or(&empty);
        let has_patron = roster.patron.is_some();
        let swear = dereth_client_contract::social::swear_target(view, roster).is_some();
        let want = [swear, has_patron, self.selected_vassal.is_some()];
        if self.button_states == Some(want) {
            return false;
        }
        for (h, on) in [
            (self.swear_button, want[0]),
            (self.break_button, want[1]),
            (self.kick_button, want[2]),
        ] {
            if let Some(h) = h {
                set_enabled(ui, h, on);
            }
        }
        self.button_states = Some(want);
        true
    }

    /// The allegiance panel's element-message handler.
    ///
    /// Button message 1 opens the confirmation for element `0x10000263`, `0x10000264` or
    /// `0x10000265`. Selection message 4 reads attribute `0x10000001` from the selected list row,
    /// clears the selected-vassal id if no row exists, and enables Kick only for a nonzero id.
    ///
    /// The three button ids select Swear, Break and Kick respectively. Selection handling reads
    /// the row rather than the cached roster because the row attribute is the UI's authoritative
    /// selection payload. If the selected row is absent or lacks that attribute, the handler
    /// clears the target before refreshing the button state. This prevents a stale roster id
    /// from surviving a list rebuild. Other element messages are ignored, and neither selection
    /// nor the initial button press emits an allegiance action directly.
    ///
    /// The confirmation round trip remains the only path that emits the eventual action.
    ///
    /// **A press sends nothing.** All three arms raise a question and stop; the game action goes
    /// out of the dialog's close callback, which is the host's
    /// [`UiRequest::AllegianceConfirmation`] round trip. A build that sent from the button would
    /// break a player's allegiance on one misclick.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::LIST_SELECTION_CHANGED {
            let Some(list) = self.list.as_ref().map(|l| l.handle) else {
                return false;
            };
            if list != m.source {
                return false;
            }
            // The id comes off the **row element** through its instance-id attribute, not out
            // of `self.rows`: a row whose attribute never got written must not be kickable.
            self.selected_vassal = selected_index(ui, list)
                .and_then(|i| self.list.as_ref().and_then(|l| l.items.get(i).copied()))
                .and_then(|row| attr_instance_id(ui, row, ATTR_ROW_INSTANCE_ID))
                .filter(|v| *v != 0)
                .map(ObjectId);
            self.refresh_buttons(ui, view);
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        let action = match m.source_id {
            SWEAR_BUTTON => AllegianceAction::Swear,
            BREAK_BUTTON => AllegianceAction::Break,
            KICK_BUTTON => AllegianceAction::Kick,
            _ => return false,
        };
        self.make_confirmation_dialog(ui, view, action)
    }

    /// The swear confirmation dialog build, the break confirmation dialog build and
    /// the kick confirmation dialog build, which differ only in where the target and its name
    /// come from.
    ///
    /// All three require no confirmation already open, and all three refuse to build when the
    /// target-name lookup fails: the selected object for Swear, the player's patron for Break,
    /// and the selected vassal for Kick. The prompt is
    /// `ID_Allegiance_*Confirmation` with one [`var::PLAYER`] variable.
    fn make_confirmation_dialog(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        action: AllegianceAction,
    ) -> bool {
        let slot = action as usize;
        let r = view.allegiance_roster();
        let (target, name, token) = match action {
            AllegianceAction::Swear => {
                let Some(sel) = view.selected_object() else {
                    return false;
                };
                let Some(name) = view.name(sel).map(str::to_string) else {
                    return false;
                };
                (sel, name, ID_SWEAR_CONFIRMATION)
            }
            // The player's patron — and the dialog is not built at all when the player has none,
            // which is the same condition that leaves the button disabled.
            AllegianceAction::Break => {
                let Some(p) = r.patron.as_ref() else {
                    return false;
                };
                (p.id, p.full_name.clone(), ID_BREAK_CONFIRMATION)
            }
            // The selected-vassal id is latched as the possible kick target here rather than at
            // close time — the asymmetry with Break is retail's.
            AllegianceAction::Kick => {
                let Some(v) = self.selected_vassal else {
                    return false;
                };
                let Some(row) = r.vassals.iter().find(|e| e.id == v) else {
                    return false;
                };
                (v, row.full_name.clone(), ID_KICK_CONFIRMATION)
            }
        };
        let mut prompt = fill(ui, token, &[(var::PLAYER, &name)]);
        // A world that charges experience for an oath (an earlier era's rule) says what this one
        // costs; the end-of-retail prompt is unchanged.
        if action == AllegianceAction::Swear {
            if let Some(cost) = oath_xp_cost(view).filter(|&c| c > 0) {
                prompt.push_str(&format!(
                    "\n\nThis oath costs {} unassigned experience.",
                    super::examination::insert_commas(i32::try_from(cost).unwrap_or(i32::MAX))
                ));
            }
        }
        self.confirmations[slot] += 1;
        ui.requests.emit(UiRequest::AllegianceConfirmation {
            action,
            target,
            prompt,
        });
        true
    }

    /// The allegiance panel's player data update.
    ///
    /// **The Followers number.** The value is the allegiance profile header's total-vassals count,
    /// unchanged, passed to [`var::FOLLOWERS`] as an unsigned 64-bit value. It is **not** the
    /// record count: the records are the player's neighbourhood of the tree, not the allegiance.
    fn update_player_data(&mut self, ui: &mut UiSystem, r: &AllegianceRoster) {
        let name = fill(ui, ID_CHARACTER_NAME, &[(var::NAME, &r.allegiance_name)]);
        set_text(ui, self.allegiance_name, &name);
        let followers = fill(
            ui,
            ID_FOLLOWERS,
            &[(var::FOLLOWERS, &r.total_vassals.to_string())],
        );
        set_text(ui, self.player_followers, &followers);
        // The player's rank quality, read enchanted, against the tree's rank: equal, or `-1`, prints
        // the title and the tree's rank; anything else prints the title, the enchanted rank and
        // the difference. The shard sends the stored rank, so only a spell on the rank differs.
        let rank = match &r.subject {
            Some(s) => {
                let stored = i32::from(s.rank);
                let enchanted = r.player_rank_quality;
                if enchanted == -1 || enchanted == stored {
                    fill(
                        ui,
                        ID_RANK,
                        &[
                            (var::TITLE, title_of(&s.full_name)),
                            (var::RANK, &stored.to_string()),
                        ],
                    )
                } else {
                    fill(
                        ui,
                        ID_RANK_BUFFED,
                        &[
                            (var::TITLE, title_of(&s.full_name)),
                            (var::RANK, &enchanted.to_string()),
                            (var::RANK_BUFF, &enchanted.wrapping_sub(stored).to_string()),
                        ],
                    )
                }
            }
            None => String::new(),
        };
        set_text(ui, self.player_rank, &rank);
    }

    /// The allegiance panel's monarch data update.
    ///
    /// Besides the three texts it owns the section's visibility, state and talk focus; without
    /// them the MONARCH section would be drawn for a character in no allegiance at all. Retail's
    /// visibility and state behaviour:
    ///
    /// Hide the monarch section when the monarch is absent or is the player; replace
    /// its name and follower count with single spaces, leaving the label untouched.
    /// Otherwise show it, show child `0x10000490` only when the patron is the monarch,
    /// and display total membership minus one. Logged-in state selects media state 1,
    /// otherwise `0x0D`. Monarch talk focus (5) requires a nonzero monarch distinct
    /// from the player who is logged in.
    fn update_monarch_data(&mut self, ui: &mut UiSystem, r: &AllegianceRoster) {
        let player = r.subject.as_ref().map(|s| s.id);
        let shown = r.monarch.as_ref().filter(|m| Some(m.id) != player);
        let Some(m) = shown else {
            if let Some(h) = self.monarch_field {
                ui.set_visible(h, false);
            }
            set_text(ui, self.monarch_name, " ");
            set_text(ui, self.monarch_followers, " ");
            // The monarch must exist, differ from the player and be logged in. The first two tests are the
            // ones that brought us here, so the answer is already `false`.

            return;
        };
        if let Some(h) = self.monarch_field {
            ui.set_visible(h, true);
        }
        // The label is the one place the panel asks whether two walks landed on the same node.
        let same_as_patron = r.patron.as_ref().is_some_and(|p| p.id == m.id);
        let token = if same_as_patron {
            ID_PATRON_SLASH_MONARCH_LABEL
        } else {
            ID_MONARCH_LABEL
        };
        set_text(ui, self.monarch_label, &fill(ui, token, &[]));
        // `0x10000490` — the "also your patron" block inside the field, shown when the ids match
        // and hidden on the `!=` arm. (Its `0x10000492` text — `ID_Allegiance_
        // VassalExperiencePassedUp` with the player's own `_cp_tithed` now follows below.)
        if let Some(block) = self
            .monarch_field
            .and_then(|h| ui.get_child_recursive(h, MONARCH_PATRON_BLOCK))
        {
            ui.set_visible(block, same_as_patron);
            if same_as_patron {
                set_experience_passed_up(ui, block, r.own_cp_tithed);
            }
        }
        set_text(
            ui,
            self.monarch_name,
            &fill(ui, ID_CHARACTER_NAME, &[(var::NAME, &m.full_name)]),
        );
        // passed as an unsigned 64-bit value. `wrapping_sub` is the decrement — this arm runs
        // only with a monarch who is not the player, so the value is at least 1 in practice, and
        // a header of 0 would show retail's 4294967295 rather than a tidied 0.
        let monarch_followers = r.total_members.wrapping_sub(1);
        set_text(
            ui,
            self.monarch_followers,
            &fill(
                ui,
                ID_FOLLOWERS,
                &[(var::FOLLOWERS, &monarch_followers.to_string())],
            ),
        );
        if let Some(h) = self.monarch_field {
            ui.set_state(h, online_state(m.logged_in));
        }
    }

    /// The allegiance panel's patron data update.
    ///
    /// The same shape as [`Self::update_monarch_data`]:
    ///
    /// Hide the patron section when the patron is absent or is the monarch, and
    /// replace its name with a single space. Otherwise show it with media state 1
    /// when logged in, or `0x0D` when offline. Update patron talk focus (4) separately.
    ///
    /// The talk-focus test is "there is a patron and the patron is logged in" and **does not** repeat the
    /// monarch comparison: a patron who is the monarch hides the section and still enables the
    /// channel.
    fn update_patron_data(&mut self, ui: &mut UiSystem, r: &AllegianceRoster) {
        let monarch = r.monarch.as_ref().map(|m| m.id);
        match r.patron.as_ref().filter(|p| Some(p.id) != monarch) {
            None => {
                if let Some(h) = self.patron_field {
                    ui.set_visible(h, false);
                }
                set_text(ui, self.patron_name, " ");
            }
            Some(p) => {
                if let Some(h) = self.patron_field {
                    ui.set_visible(h, true);
                }
                set_text(
                    ui,
                    self.patron_name,
                    &fill(ui, ID_CHARACTER_NAME, &[(var::NAME, &p.full_name)]),
                );
                if let Some(h) = self.patron_field {
                    ui.set_state(h, online_state(p.logged_in));
                    set_experience_passed_up(ui, h, r.own_cp_tithed);
                }
            }
        }
    }

    /// The allegiance panel's vassals data update.
    fn update_vassals_data(&mut self, ui: &mut UiSystem, r: &AllegianceRoster) {
        self.rows.clear();
        self.selected_vassal = None;
        self.vassal_chat_enabled = false;
        let Some(list) = self.list.as_mut() else {
            return;
        };
        list.flush(ui);
        // `list` is re-borrowed inside the loop because each row needs `ui` mutably too.
        for v in &r.vassals {
            let Some(row) = self
                .list
                .as_mut()
                .and_then(|l| l.add_from_template(ui, VASSAL_ROW_TEMPLATE, None))
            else {
                continue;
            };
            set_attr_instance_id(ui, row, ATTR_ROW_INSTANCE_ID, v.id.0);
            let name = v.full_name.clone();
            if let Some(t) = ui
                .get_child_recursive(row, ROW_NAME)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&name);
            }
            let experience = fill(
                ui,
                ID_EXPERIENCE_PASSED_UP,
                &[(var::VALUE, &v.cp_cached.to_string())],
            );
            if let Some(t) = ui
                .get_child_recursive(row, ROW_EXPERIENCE)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&experience);
            }
            // The marker is visible for the vassal who is **not** online.
            let logged_out = !v.logged_in;
            if let Some(c) = ui.get_child_recursive(row, ROW_LOGGED_OUT) {
                ui.set_visible(c, logged_out);
            }
            self.vassal_chat_enabled |= v.logged_in;
            self.rows.push(VassalRow {
                id: v.id,
                name,
                experience,
                logged_out_marker: logged_out,
            });
        }
        // The function's last call before scrolling the selected row writes talk-focus row 6
        // from `vassal_chat_enabled`.

        // **The layout refresh.**
        //
        // Every other list-driven panel in this crate ends its rebuild with it
        // (`attributes.rs`, `house.rs`, `examination.rs`); the retail vassal rebuild instead ends
        // by scrolling the list box to the selected row, which reaches layout in the client through the
        // dirty-layout flag this build does not have.
        //
        // The hit-testing path does *not* depend on it, because `dereth_ui`'s own `ListBox`
        // behaviour lays the rows out for the element tree. What it fills is this **widget's**
        // cached geometry: `ListBoxWidget::item_widths` / `item_heights` / `cols` / `rows`, which
        // `cell_of`, `inq_item_index_at_point` and `scroll_to_view` read and which would otherwise
        // stay empty for the life of the panel.
        if let Some(list) = self.list.as_mut() {
            list.update_layout(ui);
        }
    }
}

/// Depth-first search for the one element of [`PANEL_TYPE`].
///
/// The same shape as `hud::powerbar::collect`, and for the same reason: the panel's own id is not
/// hard-coded in the client, only its registered element **type** (`0x1000002C`). Unlike
/// the power bar there is exactly one allegiance panel in the shipped tree, so the first hit wins.
fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}

/// The allegiance full-name read is `title + " " + name`, so the title is what precedes
/// the first space — and is empty when title lookup failed and the full name is the bare name.
///
/// The seam carries the joined name rather than the two halves because the title tables are
/// `dereth-client-model`'s; this recovers the half `ID_Allegiance_Rank`'s [`var::TITLE`] variable
/// wants.
fn title_of(full_name: &str) -> &str {
    dereth_presentation::social::title_and_name(full_name).0
}

/// Resolve a `StringInfo` token in [`STRING_TABLE`] and put `values` between its literal pieces.
///
/// The values go through
/// [`UiSystem::resolve_string_rendered`] with meta-language processing enabled, the same path the
/// string-info renderer takes, rather than being
/// interleaved by hand. The non-meta-language branch cannot render a
/// `{a|b}` block or perform the string renderer's final excess-space trimming.
///
/// Each value is named. The client matches a value to a variable by hashing its name against
/// the row's own variable list — `ID_Allegiance_Rank` is `TITLE, RANK` and
/// `ID_Allegiance_RankBuffed` is `TITLE, RANK, RANKBUFF` [measured, `0x23000001`] — so the order
/// these pairs are written in is not load-bearing.
///
/// The piece-count guard is kept: a row whose fragment count does not match the variables the
/// client passes is a string-table mismatch, not a formatting choice, and falling back names the
/// token rather than silently dropping a value. With no string table installed — as in some
/// headless tests — the fall-back is the token followed by the values, so a test can still see **which**
/// string was written and **what** went into it. An empty box would be indistinguishable from a
/// field that was never written.
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

/// Write the player's tithed experience as the [`var::VALUE`] variable in either patron display
/// arm.
fn set_experience_passed_up(ui: &mut UiSystem, parent: ElemHandle, value: u32) {
    let number = super::numfmt::number(i64::from(value));
    let text = fill(ui, ID_EXPERIENCE_PASSED_UP, &[(var::VALUE, &number)]);
    let child = ui.get_child_recursive(parent, EXPERIENCE_PASSED_UP);
    set_text(ui, child, &text);
}

/// Apply the enabled or disabled state used by all three button-update paths. These are the same
/// two states `titles::update_buttons` uses.
fn set_enabled(ui: &mut UiSystem, h: ElemHandle, on: bool) {
    ui.set_state(
        h,
        if on {
            dereth_ui::widgets::button::state::NORMAL
        } else {
            dereth_ui::widgets::button::state::DISABLED
        },
    );
}

/// Read the selected list index, using the same path as `titles::selected_index`.
fn selected_index(ui: &UiSystem, list: ElemHandle) -> Option<usize> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

/// Read the instance-id attribute written by [`set_attr_instance_id`].
/// Missing or differently typed attributes produce no selected id.
fn attr_instance_id(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<u32> {
    match ui.node(h)?.instance_properties.get(id) {
        Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => Some(*v),
        _ => None,
    }
}

fn set_text(ui: &mut UiSystem, h: Option<ElemHandle>, text: &str) {
    if let Some(t) = h.and_then(|h| ui.text_element_mut(h)) {
        t.set_text(text);
    }
}

/// Write an instance-id attribute and notify the bound element, the fourth property setter needed
/// by [`crate::bind`] and so far used only here.
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
    use crate::view::AllegianceEntry;

    /// Oracle: the allegiance data's full name read — `title + " " + name`.
    #[test]
    fn the_title_is_the_first_word_of_the_full_name() {
        assert_eq!(title_of("Yeoman Lark"), "Yeoman");
        assert_eq!(
            title_of("Lark"),
            "",
            "GetTitle failed, so there is no title to show"
        );
        assert_eq!(
            title_of("High King Bob"),
            "High",
            "two-word titles keep the first word only"
        );
    }

    /// Oracle: the live-run evidence requires the fallback to show *which* string was written
    /// and *what* went into it, so a headless run can tell an empty field from an unwritten one.
    #[test]
    fn a_missing_string_table_falls_back_to_the_token_and_its_values() {
        let ui = UiSystem::new((800, 600));
        assert_eq!(fill(&ui, ID_MONARCH_LABEL, &[]), ID_MONARCH_LABEL);
        assert_eq!(
            fill(&ui, ID_FOLLOWERS, &[(var::FOLLOWERS, "7")]),
            format!("{ID_FOLLOWERS} 7"),
            "the number survives the fallback"
        );
    }

    /// The element ids are transcribed constants, so one test states them as **literals** rather
    /// than reading them back through the same symbol that wrote them
    /// (the stated testability rule: *"a test that reads a constant through the same symbol it writes it
    /// through cannot detect a wrong constant"*). Source: the allegiance panel's post-init
    /// and the vassals data update.
    #[test]
    fn the_element_ids_match_post_init() {
        assert_eq!(ALLEGIANCE_NAME, ElementId(0x1000_0251));
        assert_eq!(PLAYER_FOLLOWERS, ElementId(0x1000_0252));
        assert_eq!(PLAYER_RANK, ElementId(0x1000_0253));
        assert_eq!(MONARCH_LABEL, ElementId(0x1000_0256));
        assert_eq!(MONARCH_NAME, ElementId(0x1000_0257));
        assert_eq!(MONARCH_FOLLOWERS, ElementId(0x1000_0258));
        assert_eq!(PATRON_NAME, ElementId(0x1000_025C));
        assert_eq!(VASSAL_LIST, ElementId(0x1000_0260));
        assert_eq!(ROW_NAME, ElementId(0x1000_0268));
        assert_eq!(ROW_EXPERIENCE, ElementId(0x1000_0269));
        assert_eq!(ROW_LOGGED_OUT, ElementId(0x1000_04AA));
        assert_eq!(ATTR_ROW_INSTANCE_ID, 0x1000_0001);
        assert_eq!(VASSAL_ROW_TEMPLATE, 0);
        assert_eq!(STRING_TABLE, DataId(0x2300_0001));
    }

    /// A panel with no tree bound writes nothing and says so — the "never ran" state that has to
    /// be distinguishable from "ran and the allegiance was empty".
    #[test]
    fn an_unbound_panel_reports_that_it_is_unbound() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = AllegiancePanel::default();
        assert!(!p.bound());
        assert_eq!(p.rebuilds, 0);
        // With no roster there is still a first frame, and it counts as a rebuild: the panel was
        // driven, and found nothing.
        assert!(p.update(&mut ui, &crate::view::EmptyGameView));
        assert_eq!(p.rebuilds, 1);
        assert_eq!(p.rows(), &[] as &[VassalRow]);
        assert!(
            !p.update(&mut ui, &crate::view::EmptyGameView),
            "and it is idempotent"
        );
        assert_eq!(p.rebuilds, 1);
    }

    /// The roster the panel writes, driven straight off a synthesised tree: patron first, then the
    /// vassals in the source hierarchy's first/next traversal order.
    #[test]
    fn the_rows_follow_the_walk_order_and_carry_the_full_name() {
        let entry = |id: u32, name: &str, on: bool| AllegianceEntry {
            id: ObjectId(id),
            full_name: name.to_string(),
            logged_in: on,
            rank: 3,
            cp_cached: 1234,
        };
        #[derive(Debug)]
        struct V(AllegianceRoster);
        impl GameView for V {
            fn allegiance_roster(&self) -> AllegianceRoster {
                self.0.clone()
            }
        }
        let view = V(AllegianceRoster {
            allegiance_name: "The Hand".into(),
            total_members: 4,
            total_vassals: 2,
            own_cp_tithed: 0,
            subject: Some(entry(1, "Baron Lark", true)),
            player_rank_quality: 3,
            monarch: Some(entry(9, "High King Bob", true)),
            patron: Some(entry(9, "High King Bob", true)),
            vassals: vec![
                entry(11, "Yeoman Cid", false),
                entry(10, "Yeoman Dee", true),
            ],
        });
        let mut ui = UiSystem::new((800, 600));
        let mut p = AllegiancePanel::default();
        // No tree, so no elements are written; the *model* half still runs, which is what the
        // walk order is asserted on. The element half is asserted in `dereth-client`'s
        // `o201_allegiance.rs`, against a real shipped layout.
        assert!(p.update(&mut ui, &view));
        assert_eq!(p.rebuilds, 1);
        assert!(
            !p.vassal_chat_enabled,
            "no list box bound, so no row was created"
        );
    }

    /// On a world that charges experience for an oath, the swear confirmation says what this oath
    /// costs (5% of a 20,000 step and a quarter more for one break: 1,250); a first oath, and
    /// every oath at the end of retail, keeps the plain prompt.
    #[test]
    fn the_swear_confirmation_names_the_oaths_cost_where_the_world_charges_one() {
        #[derive(Debug)]
        struct V {
            cost: Option<u32>,
        }
        impl GameView for V {
            fn oath_xp_cost(&self) -> Option<u32> {
                self.cost
            }
            fn player(&self) -> Option<ObjectId> {
                Some(ObjectId(1))
            }
            fn selected_object(&self) -> Option<ObjectId> {
                Some(ObjectId(9))
            }
            fn name(&self, _: ObjectId) -> Option<&str> {
                Some("Bob")
            }
        }
        let prompt = |view: &V| {
            let mut ui = UiSystem::new((800, 600));
            let mut p = AllegiancePanel::default();
            assert!(p.make_confirmation_dialog(&mut ui, view, AllegianceAction::Swear));
            match ui.requests.take().as_slice() {
                [UiRequest::AllegianceConfirmation { prompt, .. }] => prompt.clone(),
                other => panic!("{other:?}"),
            }
        };
        let plain = prompt(&V { cost: None });
        assert!(!plain.contains("experience"), "{plain}");
        let charged = prompt(&V { cost: Some(1250) });
        assert_eq!(
            charged,
            format!("{plain}\n\nThis oath costs 1,250 unassigned experience.")
        );
        let first = prompt(&V { cost: Some(0) });
        assert_eq!(first, plain, "a first oath is free");
    }
}
