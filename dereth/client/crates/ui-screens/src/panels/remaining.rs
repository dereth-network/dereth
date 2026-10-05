//! The toolbar, environment and HUD panels beyond the inventory pages, driven from one place.
//!
//! The panel *mechanism* — the tab table and the item-list slot elements — lives in the panel
//! stack, and the inventory pages have their own holder. This is the holder for the rest: it binds
//! off the gameplay screen's root and drives each panel from the same `&dyn GameView` seam the
//! inventory uses.
//!
//! # What is populated, and what is not
//!
//! | panel | source | state |
//! |---|---|---|
//! | attributes | the same `0x0013`'s attribute cache | **populated** |
//! | skills | the skill table × `0x0013`'s skill-stat table | **populated** |
//! | spellbook | the spell table × `0x0013`'s spellbook, filtered by spell filters | **populated** |
//! | allegiance | `0x0020 Allegiance_AllegianceUpdate` | **populated**; the corpus carries twelve `0x0020` and **no members** — see below |
//! | fellowship | `0x02BE Fellowship_FullUpdate` | **populated**; the corpus carries 94 fellowship messages — see below |
//! | friends | `0x0021 Social_FriendsUpdate` | **populated**; it arrives unprompted on every login, 11 times across three captures, and five of six recorded sessions carry records |
//! | journal | client-local notes; no message carries them | not populated |
//! | character options | options | its own module |
//!
//! **Allegiance.** The corpus holds twelve `0x0020`, and **all twelve carry allegiance version 11,
//! rank 0, `total_members = 0` and zero member records**: the character those captures were
//! recorded with is in no allegiance, so the roster has no oracle in the corpus.
//! [`super::allegiance`] is driven from a **synthesised** tree and says so. Its list box carries
//! exactly **one** row template — `0x10000260` → `(0x2100002F, 0x10000266)` — so the row factory
//! it needs is [`super::listbox`].
//!
//! **Fellowship.** The `fellowship*` recordings carry **23 `0x02BE`, 39 `0x02C0`, 8 `0x00A3`,
//! 3 `0x00A4`, 2 `0x02BF` and 2 `0x01C9`** — 77 server-to-client fellowship messages — and **27
//! client-to-server fellowship events**, which are retail's own client and therefore a byte-level
//! oracle for every button on the tab. Its list box likewise carries one row template,
//! `0x10000279` → `(0x21000030, 0x10000281)`. [`super::fellowship`] is driven from that corpus and
//! says so.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use super::allegiance::AllegiancePanel;
use super::attributes::AttributesPanel;
use super::skills::SkillsPanel;
use super::spellbook::SpellbookPanel;
use crate::view::GameView;

/// The character page — the eleventh of `PANEL_PAGES`, holding the attribute, skill,
/// and character-title panels.
pub const CHARACTER_PAGE: ElementId = ElementId(0x1000_018E);
/// The spell page, holding the spellbook, spell-component panel, and the rest.
pub const SPELL_PAGE: ElementId = ElementId(0x1000_0190);

/// The panels this holder populates, bound off one screen root.
#[derive(Debug, Default)]
pub struct RemainingPanels {
    /// The abuse-report panel, including response text for failures. Its three server replies are
    /// consumed as native notices and stay silent in ordinary chat.
    pub abuse: super::abuse::AbusePanel,
    pub external_container: super::external_container::ExternalContainerPanel,
    /// The attribute panel — the character page's **default** tab, so this is the one a player sees
    /// first when they open that panel.
    pub attributes: AttributesPanel,
    pub skills: SkillsPanel,
    /// The character-title panel — the character page's third tab.
    ///
    /// It binds off [`CHARACTER_PAGE`] beside [`Self::attributes`] and [`Self::skills`] because
    /// that is where the shipped tree puts it: the panel element is `0x10000539`, a child of the
    /// character page.
    pub titles: super::titles::TitlesPanel,
    pub spellbook: SpellbookPanel,
    /// The on-screen spew strip that draws chat type `0x1A`.
    ///
    /// It is a HUD element, not a toolbar page, and it hangs off the screen root. It rides here
    /// because this is the one struct `Hud::drive` already binds off the gameplay root and updates
    /// once a frame; a second such struct would be a second seam to keep in step.
    pub spew: crate::hud::speech_bubbles::SpeechBubblePanel,
    /// The allegiance panel.
    ///
    /// It binds off the screen **root** for the same reason [`Self::spew`] does: its thirteen
    /// element ids are unique in the shipped tree and the social page they sit on is built by the
    /// panel stack rather than by this holder.
    pub allegiance: AllegiancePanel,
    /// The fellowship panel — the social page's other tab. It binds off the screen **root** beside
    /// [`Self::allegiance`] and for the same reason: its twelve element ids are unique in the
    /// shipped tree.
    pub fellowship: super::fellowship::FellowshipPanel,
    /// The friends panel -- the social page's third tab. It binds off the screen **root** beside
    /// [`Self::allegiance`] and [`Self::fellowship`] for the same reason: its six element ids are
    /// unique in the shipped tree. Its data is `0x0021 Social_FriendsUpdate`, which arrives 11
    /// times across the recorded captures.
    pub friends: super::friends::FriendsPanel,
    /// The squelch panel — the social page's **fourth** tab. It binds off the screen **root**
    /// beside [`Self::allegiance`], [`Self::fellowship`] and [`Self::friends`] for the same reason:
    /// its five element ids are unique in the shipped tree. Its data is `0x01F4
    /// Communication_SetSquelchDB`, received by
    /// `dereth_client_model::chat::ChatState::recv_set_squelch_db`, whose update-panel notice this
    /// panel consumes.
    pub squelch: super::squelch::SquelchPanel,
    /// The urgent-assistance panel. It binds off the screen **root** beside [`Self::squelch`] and
    /// for the same reason: the window is the one element of type `0x1000001F` in the shipped
    /// tree. It is not driven by an inbound message — its trigger is the shipped input action
    /// `ToggleUrgentAssistancePanel 0x1000000B` — and its Send button is the client's only
    /// producer of a `0x0147` broadcast on the Help channel `0x400`.
    pub urgent_assistance: super::urgent_assistance::UrgentAssistancePanel,
    /// The vendor panel. The vendor window is not a toolbar page at all: it is a page of the
    /// `<ENVP>` environment window, and its whole subtree is already live in the shipped gameplay
    /// tree under `0x100000B8` while its declared root `0x100000B7` is not instantiated. It rides
    /// here for the reason [`Self::spew`] does: this is the struct `Hud::drive` already binds off
    /// the gameplay root and updates once a frame.
    pub vendor: super::vendor::VendorPanel,
    /// The spell-component panel — the spell page's other panel. It binds off [`SPELL_PAGE`]
    /// beside [`Self::spellbook`], which is where its one child is found.
    pub spell_components: super::spellcomponent::SpellComponentPanel,
    /// The spell page's Create Spell tab, built in code on a world with spell research. It binds
    /// off [`SPELL_PAGE`], whose tab table it joins.
    pub research: super::research::ResearchPanel,
    /// The spellcasting bar. Like [`Self::vendor`] it is an `<ENVP>` environment element, not a
    /// toolbar page, so it binds off the screen root.
    ///
    /// It is the client's only production caller of `cast_spell`.
    pub spellcasting: super::spellcasting::SpellcastingPanel,
    /// The secure-trade panel — another `<ENVP>` environment element, riding here for the reason
    /// [`Self::vendor`] does; retail groups the two.
    ///
    /// It is the production caller of `dereth_client_model::trade` and the writer of the object's
    /// trade-state field. Without it `Request::OpenTradeNegotiations` would start a trade the
    /// client could not then display.
    pub trade: super::trade::TradePanel,
    /// The combat window's controls.
    ///
    /// It rides here for the reason [`Self::spellcasting`] does: `0x1000005C` is a page of
    /// `<COMB>` (`0x100006B5`) rather than a toolbar page, so it binds off the screen root, and
    /// this is the struct that is already bound off that root once a frame **with a `GameView` in
    /// hand** — which the `0x0A` arm's skill-advancement-class read for skill `0x32` needs and
    /// `Screen::on_element_message` cannot supply.
    pub combat_window: crate::hud::combat_window::CombatWindow,
    /// The **standalone** charge bar, in both instances.
    ///
    /// It rides here for the reason [`Self::combat_window`] does. The window's meter takes
    /// `PowerBarMode::Combat` only (through the combat panel's set-powerbar-level notice); with the
    /// **Advanced Combat Interface** option on, the power-bar mode becomes
    /// `PowerBarMode::AdvancedCombat` at `combat.rs`'s `attempt_start_building_attack`, and without
    /// this field the client would draw **no power bar at all**: the meter refuses the level and
    /// nothing else calls `PowerBar::set_level`.
    pub power_bar: crate::hud::powerbar::PowerBars,
    /// The House tab of the map page.
    ///
    /// It rides here for the reason [`Self::spew`] does, and for one more: with no house data its
    /// whole input is "there is no house", so it needs a frame loop and nothing else. See
    /// [`super::house`] for why that is a stand-in for a login round trip rather than retail's
    /// own trigger.
    pub house: super::house::HousePanel,
    /// The effects panel, **both instances** — the buff and debuff panels the two magic lamps open.
    ///
    /// Two fields rather than one because the shipped tree holds two elements of type
    /// `0x1000001B` and the layout attribute `0x1000000C` is what tells them apart; see
    /// [`super::effects`]. They ride here for the reason [`Self::house`] does: both are pages of
    /// `<PANS>`, their ids are unique in the tree, and their whole drive is one frame pull with a
    /// `GameView` in hand.
    pub effects_helpful: super::effects::EffectsPanel,
    /// The harmful half of the pair — `0x10000185`, effects type 2.
    pub effects_harmful: super::effects::EffectsPanel,
    /// The panel the vitae lamp opens.
    ///
    /// It rides here for the reason [`Self::effects_helpful`] does. It is the only caller of
    /// `vitae_cp_pool_threshold`.
    pub vitae: super::vitae::VitaePanel,
    /// The character-info panel — the panel the **burden** lamp opens, despite its broader scope.
    pub character_info: super::characterinfo::CharacterInfoPanel,
    /// The link-status panel — the connection lamp's panel, and the **only** client path that
    /// sends `0x01E9 Character_RequestPing`.
    pub link_status: super::linkstatus::LinkStatusPanel,
    /// The journal panel. It carries no protocol work at all: no message anywhere carries a
    /// journal page.
    pub journal: super::journal::JournalPanel,
    /// The page-list panel — the other half of the same mechanism. It holds a filtered copy of
    /// [`Self::journal`]'s pages and calls back into it for both of its gestures, which is what
    /// the client stores through its global journal-panel pointer.
    pub page_list: super::pagelist::PageListPanel,
    /// The contracts panel -- the quest page's third tab. It binds off the screen **root**
    /// beside [`Self::journal`] and [`Self::page_list`] for the same reason: it is a sub-panel of
    /// the `<QUES>` page and its ten element ids are unique in the shipped tree.
    ///
    /// Its data is `0x0314` / `0x0315`, which never arrives in the recorded captures; this field
    /// is the reader for it.
    pub contracts: super::contracts::ContractsPanel,
    /// What the world's era lacks, taken off the screens (the Contracts tab, ...).
    pub era: super::era::EraPanels,
    /// The salvage panel. It binds off the screen **root** like every other `<ENVP>` window: its
    /// instance is `0x1000005E`, the second of the environment panel's five pages.
    ///
    /// It joins four pieces: the open-panel notice, `dereth_client_model::inventory::salvage`,
    /// the `0x02B4` result, and this window.
    pub salvage: super::salvage::SalvagePanel,
    /// The **house purchase and maintenance window**, which appears when the player interacts with
    /// a housing vendor.
    ///
    /// It binds off the screen **root** for the reason the four `<ENVP>` windows above do:
    /// recursive child lookup finds `0x10000060`, which is unique in the shipped tree.
    pub slumlord: super::slumlord::SlumlordPanel,
    /// The **chess window**.
    ///
    /// It binds off the screen **root** like every other `<PANS>` page: `0x10000188` is unique in
    /// the shipped tree. It joins `Notice::BeginGame`, `dereth_client_model::chess` in both
    /// directions, the five outbound chess requests and the six inbound chess messages.
    pub minigame: super::minigame::MiniGamePanel,
    /// The allegiance panel's post-init check box — *Ignore Allegiance
    /// Requests*.
    ///
    /// It rides here rather than on [`Self::allegiance`] for the reason [`Self::combat_window`]
    /// does, and one more: the caption, the value and the click are the *option* mechanism, not
    /// the allegiance panel's. See [`crate::options::toggle`].
    pub allegiance_options: crate::options::toggle::PanelOptionBoxes,
    /// The fellowship panel's four check boxes, all of them inside the Create screen's
    /// not-in-fellowship frame. Held here for the same reason as [`Self::allegiance_options`].
    pub fellowship_options: crate::options::toggle::PanelOptionBoxes,
    /// True once [`Self::post_init`] found both pages.
    pub bound: bool,
}

impl RemainingPanels {
    /// Bind every panel from the gameplay screen's root.
    ///
    /// Runs at the same point retail's panel set-up does: after the tree exists and **before** the
    /// panel stack's start visibility is applied, because the client's list fill returns
    /// immediately on an invisible list and the pages are still up at that point. The caller
    /// re-runs it whenever the screen is rebuilt.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.abuse.post_init(ui, root);
        self.external_container.post_init(ui, root);
        let mut ok = self.abuse.bound();
        if let Some(page) = ui.get_child_recursive(root, CHARACTER_PAGE) {
            self.attributes.post_init(ui, page);
            ok |= self.attributes.list.is_some();
            self.skills.post_init(ui, page);
            ok |= self.skills.list.is_some();
            // The title panel, off the same page: the panel element `0x10000539` is a sibling of
            // the two above.
            self.titles.post_init(ui, page);
            ok |= self.titles.bound();
        }
        if let Some(page) = ui.get_child_recursive(root, SPELL_PAGE) {
            self.spellbook.post_init(ui, page);
            ok |= self.spellbook.list.is_some();
            // The spell page's other panel. Its one binding is the list box
            // `0x10000464`; `spellbook::PANEL`'s own doc names it as the subtree a search that
            // starts too high finds first, which is why both are bound from the page rather than
            // from the root.
            self.spell_components.post_init(ui, page);
            ok |= self.spell_components.bound();
            self.research.post_init(ui, page);
        }
        // Off the screen **root**, not off a page: the spew strip is HUD chrome
        // positioned over the world, and its list box `0x10000049` sits at (175, 20)-(624, 91) in
        // the shipped gameplay layout.
        self.spew.post_init(ui, root);
        ok |= self.spew.list.is_some();
        // The allegiance panel, off the root as well; see the field comment.
        self.allegiance.post_init(ui, root);
        ok |= self.allegiance.bound();
        // The fellowship panel, off the root for the same reason.
        self.fellowship.post_init(ui, root);
        ok |= self.fellowship.bound();
        // The friends panel, off the root for the same reason.
        self.friends.post_init(ui, root);
        ok |= self.friends.bound();
        // The squelch panel, off the root for the same reason.
        self.squelch.post_init(ui, root);
        ok |= self.squelch.bound();
        // The urgent-assistance panel, off the root as well: the window is page `0x10000189` of the
        // toolbar panel stack and the one element of its type in the tree.
        self.urgent_assistance.post_init(ui, root);
        ok |= self.urgent_assistance.bound();
        // The vendor panel, off the root as well: `0x100000B8` is three levels inside `<ENVP>`.
        self.vendor.post_init(ui, root);
        ok |= self.vendor.bound();
        // The spellcasting bar, off the root for the same reason — `0x100000A2` is an `<ENVP>`
        // child.
        self.spellcasting.post_init(ui, root);
        ok |= self.spellcasting.bound();
        // The trade panel, off the root for the same reason again -- its nine child ids are
        // unique in the shipped tree.
        self.trade.post_init(ui, root);
        ok |= self.trade.bound();
        // The combat window, off the root for the same reason again: `0x1000005C` is three
        // levels inside `<COMB>`.
        self.combat_window.post_init(ui, root);
        ok |= self.combat_window.bound();
        // The standalone power bars, once per instance. There are two in
        // the shipped `0x21000005` tree and they are found by **type** rather than by id,
        // because the element type is what tells the floaty power bar panel's two apart.
        self.power_bar.post_init(ui, root);
        ok |= self.power_bar.bound() > 0;
        // The house panel's post-init, the journal panel's post-init and
        // the page-list panel's post-init, all three off the root for the reason
        // [`super::book`]'s is: each is a sub-panel of a `<PANS>` page and recursive lookup is
        // recursive. Their element ids are unique in the shipped tree.
        self.house.post_init(ui, root);
        ok |= self.house.bound();
        // The effects panels, once per instance. Both are found off
        // the root because each is a page of `<PANS>` and their ids are unique in the tree; the
        // polarity is read out of each element's own `0x1000000C` rather than passed in.
        self.effects_helpful
            .post_init(ui, root, super::effects::HELPFUL_PANEL);
        self.effects_harmful
            .post_init(ui, root, super::effects::HARMFUL_PANEL);
        ok |= self.effects_helpful.bound() || self.effects_harmful.bound();
        // The vitae panel, off the root for the same reason.
        self.vitae.post_init(ui, root);
        ok |= self.vitae.bound();
        // The character info panel's post-init.
        self.character_info.post_init(ui, root);
        ok |= self.character_info.bound();
        // The link status panel's post-init.
        self.link_status.post_init(ui, root);
        ok |= self.link_status.bound();
        self.journal.post_init(ui, root);
        ok |= self.journal.bound();
        self.page_list.post_init(ui, root);
        ok |= self.page_list.bound();
        // The contracts panel, off the root for the same reason the two above are: `0x100005D4` is
        // a sub-panel of `<QUES>` and recursive lookup is recursive.
        self.contracts.post_init(ui, root);
        ok |= self.contracts.bound();
        self.era.post_init(ui, root);
        // The salvage panel, off the root for the reason the four `<ENVP>` windows above are:
        // recursive lookup finds the unique `0x1000005E`.
        self.salvage.post_init(ui, root);
        ok |= self.salvage.bound();
        // The housing window, off the root for the same reason: `0x10000060` is the environment
        // panel's fourth page and its ten child ids are unique.
        self.slumlord.post_init(ui, root);
        ok |= self.slumlord.bound();
        // The chess window, plus the sixty-four cells the client adds to `0x10000174`, off the
        // root for the same reason.
        self.minigame.post_init(ui, root);
        ok |= self.minigame.bound();
        // The five check boxes the two social panels bind: recursive child lookup,
        // type validation, player-option binding, caption setting, and tooltip setting run at
        // the head of the allegiance panel's post-init and the tail of
        // the fellowship panel's. They sit here rather than in the two panels
        // because the caption, the value and the click are the character-options mechanism and
        // this is the one holder that has the root, a `GameView` and the element messages.
        // Off the root for the same reason the panels themselves are: the five ids are unique in
        // the shipped tree.
        self.allegiance_options.post_init(
            ui,
            root,
            &crate::options::toggle::ALLEGIANCE_OPTION_BOXES,
        );
        ok |= self.allegiance_options.bound();
        self.fellowship_options.post_init(
            ui,
            root,
            &crate::options::toggle::FELLOWSHIP_OPTION_BOXES,
        );
        ok |= self.fellowship_options.bound();
        self.bound = ok;
    }

    /// One frame's drive. Each panel guards on its own snapshot, so this is a no-op on a frame
    /// where nothing changed.
    ///
    /// Returns how many panels rewrote themselves.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> u32 {
        let abuse = self.abuse.update(ui);
        let external_container = u32::from(self.external_container.update(ui, view));
        let attributes = self.attributes.update(ui, view);
        let skills = self.skills.update(ui, view);
        // The title panel's update, which the client re-runs from each of its three notice
        // handlers. Guarded on the `CharacterTitles` snapshot, so it is a
        // no-op on every frame where no `0x0029` / `0x002B` has moved the table — which is every
        // frame of every recorded session after the login one.
        let titles = u32::from(self.titles.update(ui, view));
        let spellbook = self.spellbook.update(ui, view);
        // Skill-list rebuilding ends by clearing the selected index and
        // then runs the panel update, whose tail refreshes the selection.
        // That is what puts the **default footer** ("Select a Skill to
        // Improve", the credit and experience totals) on screen. Without this the footer is three
        // stacked containers of empty text.
        if skills {
            self.skills.update_selection(ui, view);
        }
        // The same argument on the other sub-panel -- and it is **not** conditional on
        // the list rebuilding, because the attribute rows are structural: post-init creates
        // them once and `attributes` above is true only on a frame where a *value* changed. The
        // default footer ("Select an Attribute to Improve", the credits and the experience) has
        // to be put up on the first frame after `post_init`, and a character standing still
        // changes no attribute value at all.
        if self.attributes.footer_content == crate::panels::statmgmt::FooterContent::default() {
            self.attributes.update_selection(ui, view);
        }
        // The stat-management header is shared by the skill and attribute panels, and each
        // subclass has its own copy of the eight elements, so both are driven. Without this every
        // value in the header is blank on screen while the list below draws correctly.
        let header = u32::from(self.skills.update_header(ui, view))
            + u32::from(self.attributes.update_header(ui, view));
        // The spew-box update is driven by **global message 3**, the per-frame broadcast — the
        // same tick this function is.
        // It is a no-op on a frame with nothing pending, like every other panel here.
        let spew = self.spew.update(ui);
        // The allegiance update runs the four data-update steps; it is guarded on the roster
        // snapshot, so it is a no-op on a frame where the allegiance did not change — including
        // every frame of a character who is in none.
        //
        // The visibility poll runs **before** the panel's own update, because in the client the
        // visibility handler calls the update; it has already happened by the time any later
        // frame work runs. The visibility poll is what sends `0x001F`: without that request the
        // shard has no reason to send `0x0020`, and the tab stays empty.
        //
        // The player-description poll covers the other two allegiance update-request (`0x001F`)
        // call sites — the player-description-received notice and the quality-changed handler. The
        // third, the post-init one, is in `AllegiancePanel::post_init` where retail has it.
        //
        // The aborted-update poll also runs **before** the update for the same
        // reason the visibility handler does: in the client the notice *is* the call to the
        // update, so anything it clears is cleared by the time the frame's own
        // per-frame stand-in runs. See [`super::allegiance::AllegiancePanel::poll_update_aborted`].
        let aborted = u32::from(self.allegiance.poll_update_aborted(&mut ui.requests, view));
        let subscribe = u32::from(self.allegiance.poll_visibility(ui, view))
            + self.allegiance.poll_player_desc(&mut ui.requests, view)
            + aborted;
        let allegiance = u32::from(self.allegiance.update(ui, view))
            + subscribe
            // The Swear and Break button updates. Retail runs them from selection-changed and
            // abort notices; this build has no selection notice bus, and the Swear rule reads the
            // selected object, so the decision is taken once a frame and written only when it
            // moves.
            + u32::from(self.allegiance.refresh_buttons(ui, view));
        // The fellowship panel's visibility-changed handler then its update, in that order and
        // for the reason the pair above is in that order. The visibility poll is the client's
        // **only** producer of `0x00A6 Fellowship_UpdateRequest`, and `0x02C0
        // Fellowship_UpdateFellow` -- 39 arrivals in the fellowship captures -- is what the shard
        // answers it with.
        let fellowship = u32::from(self.fellowship.poll_visibility(ui))
            + u32::from(self.fellowship.update(ui, view));
        // The friends panel, guarded on the friends snapshot. There is no visibility poll beside
        // it because the friends panel has **no** visibility handler at all -- unlike its two
        // neighbours it subscribes to nothing and sends nothing on being opened; the shard pushes
        // `0x0021` unprompted at login and again on every change, so it arrives without the tab
        // ever being up.
        let friends = u32::from(self.friends.update(ui, view));
        // The squelch panel, guarded on the squelch snapshot. There is no visibility poll beside it
        // for the reason the friends panel has none: the squelch panel has no visibility-change
        // callback and sends nothing on being opened. Its one retail trigger is the
        // update-squelch-panel notice, raised by the squelch-DB set and clear; both are writes to
        // the model this snapshot is taken from, so a change of either kind rebuilds here.
        let squelch = u32::from(self.squelch.update(ui, view));
        // The vendor panel's update, guarded on the shop snapshot: a no-op on every
        // frame of a session with no vendor open, which is every frame of six of the seven
        // captures. `long-solo-play` opens two.
        let vendor = u32::from(self.vendor.update(ui, view));
        // The spell component panel handles the update-spell-components notice,
        // which is raised from every component update. Guarded on the seven-category snapshot, so it is a
        // no-op on every frame where no component moved.
        let components = u32::from(self.spell_components.update(ui, view))
            // The spell component panel's selection-changed notice — the world selection
            // driving the list's highlight. It runs **after** the rebuild above and never before
            // it, because its walk is over the rows that rebuild just drew; and it is edge-guarded
            // on its own memo of the last selection, so on every frame where the selected object
            // has not moved it is the same no-op the notice not arriving would be. Same treatment,
            // and the same reason, as `allegiance.refresh_buttons` twenty lines up.
            + u32::from(self.spell_components.on_selection_changed(ui, view));
        // The Create Spell tab follows the world's spell research; its page redraws when the
        // formula or the carried components change.
        let research = u32::from(self.research.update(ui, view));
        // The spell-cast sub-menu's update from the player module: populate actual rows before
        // endowment selection and the queued magic notices read them.
        let spellcasting = u32::from(self.spellcasting.update(ui, view));
        // The endowment state, which the client re-runs on every server-move-item notice, tab
        // change, and player-description-received notice. It is what decides whether the Cast button *uses the
        // wand* rather than casting the selected spell, so it has to run before a click can arrive.
        let endowment = u32::from(self.spellcasting.update_endowment(ui, view));
        // The trade window's per-frame rebuild, guarded on its own snapshot: a
        // no-op on every frame of every recorded session, because none of them contains a trade.
        let trade = u32::from(self.trade.update(ui, view));
        // Refresh the combat window's three boxes. The client reads them once when binding each
        // player option, because a player-option change raises a notice; this build has no notice
        // bus, so the three are polled and written only when the bit moved. Zero on every frame of
        // a session where no combat option changes, which is every recorded one.
        let combat_window = self.combat_window.refresh(ui, view)
            // The combat window's three notice read-backs — the attack-height media state, the
            // power meter and the desired-power notch. Without this call the window never
            // reflects any of the three.
            + self.combat_window.update(ui, view);
        // Power-bar notices are delivered by the host after this update, in producer order.
        // Reconstructing Begin/Finish from the final mode loses same-frame hide/restart edges.
        // **The spell bar's ten notice handlers, and the consumer end of the combat system's
        // magic-action handling.** Eighteen bound keys reach dispatch and are answered here; this
        // is the production caller of `cast_current_spell` / `cast_quickslot_spell`.
        //
        // Drained here rather than in the panel's own `on_element_message` because a notice is not
        // an element message: it has no source element, and this is the one slot that holds the
        // spell bar, a `UiSystem` and a `GameView` at once. See [`dereth_client_contract::notices`].
        let magic: u32 = ui
            .notice_inbox
            .take()
            .into_iter()
            .map(|n| u32::from(self.spellcasting.recv_magic_notice(ui, n, view)))
            .sum();
        // The house panel, with its real trigger, all of its sections and `0x0225`.
        //
        // Both updates have a receiver behind them. This models
        // the update-house-data notice and the failed-house-transaction notice
        // as a pull: the pane redraws every time either count moves, which is what
        // retail's redraw-on-every-notice comes to. Both counts being 0 — a build with no shard
        // attached — is still the stand-in's case, and `HousePanel::update` takes the arms in the
        // order that makes the stand-in unreachable whenever a real answer arrived. See
        // [`super::house`]'s header.
        let house = u32::from(self.house.update(ui, view));
        // The effects panel's update, for both instances. Each guards
        // on its own visibility first — that update's own first line — so this is a no-op on
        // every frame where neither magic panel is up, which is every frame until a lamp is
        // clicked.
        let effects = u32::from(self.effects_helpful.update(ui, view))
            + u32::from(self.effects_harmful.update(ui, view));
        // The vitae panel, guarded on its own visibility and then on
        // the (multiplier, pool, threshold) snapshot: a no-op on every frame of a character with
        // no vitae, which is every frame of every recorded session.
        let vitae = u32::from(self.vitae.update(ui, view));
        // The character-info panel -- six sections into one
        // text element, guarded on visibility and then on the whole `CharacterInfo` snapshot.
        let character_info = u32::from(self.character_info.update(ui, view));
        // The link-status panel -- five lines, a five-second redraw
        // throttle and the 120-second ping gate. It emits `UiRequest::RequestPing` and sends
        // nothing itself.
        let link_status = u32::from(self.link_status.update(ui, view));
        // The journal's global message `0x0B` (once) and then both
        // panels' global message `3` — the half-second countdown redraw, which is a no-op on
        // every frame where the panel is not visible, and for the page list also on every frame
        // where the row set and the page set have gone out of step.
        // Journal loading takes the `view` because it needs the journal path's three
        // globals, which reach this crate through `GameView::journal_identity`.
        self.journal.load(ui, view);
        self.page_list.sync(ui, &self.journal);
        let journal = u32::from(self.journal.tick(ui)) + u32::from(self.page_list.tick(ui));
        // The two contract notices, the visibility-changed handler's shown edge
        // and the global-message handler's half-second redraw, folded into one call. It
        // is a no-op on every frame where the tracker table has not moved and the panel is not
        // visible -- which is every frame of every recorded session, because neither `0x0314` nor
        // `0x0315` appears in the corpus.
        let contracts = u32::from(self.contracts.update(ui, view));
        // The era's features, applied once each time they change: before any of the era's
        // panels could be opened, since the era is known by the time the world is entered.
        let era = u32::from(self.era.update(ui, view));
        // The salvage update combines the client's hidden edge with the
        // window's fill, folded into one call. A no-op on every frame where the window holds the
        // same rows -- which is every frame until a tinkering tool is used, because nothing but
        // `Notice::OpenSalvagePanel` ever opens it.
        let salvage = u32::from(self.salvage.update(ui, view));
        // The slumlord panel's update-house-profile notice's raise
        // edge and the client's redraw, folded into one call. A no-op on every
        // frame where no `0x021D` has landed and no row has moved — which is every frame of every
        // recorded session except the one housing trace that carries such a notice.
        let slumlord = u32::from(self.slumlord.update(ui, view));
        // The begin-game / end-game notices'
        // visibility edge, pulled rather than pushed for the reason `house` is. A no-op on
        // every frame until a chessboard is double-clicked, because nothing else raises it.
        self.minigame.update(ui, view);
        // The checkbox option control's value read + the refresh for the
        // five boxes the social panels bind. In the client each box re-reads on a notice
        // (raised by each panel and received through each option control's two notice subscriptions); this
        // build has no notice bus, so they are polled and written only when the bit moved — the
        // same treatment [`Self::combat_window`]'s three get, for the same reason. The **first**
        // poll always writes, which is what puts *Ignore Fellowship Requests* and *Share XP* up
        // ticked on a shipped character instead of leaving the Create screen all-off.
        let panel_options =
            self.allegiance_options.refresh(ui, view) + self.fellowship_options.refresh(ui, view);
        abuse
            + u32::from(attributes)
            + panel_options
            + house
            + effects
            + vitae
            + character_info
            + link_status
            + journal
            + contracts
            + era
            + salvage
            + slumlord
            + u32::from(skills)
            + titles
            + u32::from(spellbook)
            + header
            + spew
            + allegiance
            + fellowship
            + friends
            + squelch
            + vendor
            + components
            + research
            + spellcasting
            + endowment
            + trade
            + combat_window
            + magic
            + external_container
    }

    /// One element message, offered to each panel in turn.
    ///
    /// The client has no such fan-out: every native panel is itself an element and receives its own
    /// subtree's messages through element-message dispatch. This crate's panels are
    /// plain structs bound to a subtree, so the screen that *is* the listener hands the message
    /// down. The dispatch order is the tree's: a message belongs to at most one panel, and each
    /// `on_element_message` returns false for anything outside its own ids.
    ///
    /// Returns true when a panel consumed it.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        self.on_element_message_with_split(
            ui,
            m,
            view,
            u32::try_from(view.split_size()).unwrap_or(0),
            u32::try_from(view.max_split_size()).unwrap_or(0),
        )
    }

    /// The live HUD's form of [`Self::on_element_message`], carrying the toolbar's current
    /// selected-item holder pair to consumers which read those globals synchronously.
    pub fn on_element_message_with_split(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
        split: u32,
        max_split: u32,
    ) -> bool {
        self.external_container.on_element_message(ui, m, view)
            // The spell component panel's element-message handler's `4` arm — a press on a
            // component row selects the row's object in the world; a header clears it.
            || self.spell_components.on_element_message(ui, m)
            // The Create Spell page: its slots' presses, Test, Clear and its scrollbar. Every id
            // it answers is its own, so it shadows no shipped panel.
            || self.research.on_element_message(ui, m, view)
            || self.attributes.on_element_message(ui, m, view)
            || self.skills.on_element_message(ui, m, view)
            // The character title panel's element-message handler — a press
            // on the title list, and the *"Set as Display Title"* button. It sits after the two
            // stat panels because it is the third sub-panel of the same page and the dispatch
            // order here is the tree's; its own ids (`0x10000532`, `0x10000535`) are shared with
            // neither of them, and its press arm refuses anything its list box does not own.
            || self.titles.on_element_message(ui, m, view)
            // The effects panel's element-message handler's `0x1C` arm — a
            // press on the effects list selects a row. Both instances are offered it and each
            // refuses a press that is not inside its own list box, which is what keeps the two
            // apart: the two panels share every child id, exactly as the skill and
            // attribute panels do.
            || self.effects_helpful.on_element_message(ui, m)
            || self.effects_harmful.on_element_message(ui, m)
            // The checkbox option control's element-message handler's message-1
            // arm, for the five boxes the two social panels bind. It sits **before** both panels
            // because that is the child-to-parent bubble order — each box is a child of the panel
            // whose post-init bound it — and because a check-box click and a button click are
            // the same message id on different objects in the client. Neither panel's own arm
            // carries any of these five ids, so the order is belt and braces rather than a fix.
            || self.panel_option_message(ui, m)
            // The allegiance panel's element-message handler — the Swear, Break and Kick
            // buttons and the vassal list's selection (keyed on the `0x10000001` row attribute).
            || self.allegiance.on_element_message(ui, m, view)
            // The fellowship panel's element-message handler — the seven
            // buttons, the fellows list's selection, and the name box's keystrokes. It sits here
            // because its gate is narrow: the button arm refuses any id outside `0x10000274` and
            // `0x1000027B`..`0x10000280`, and the other two refuse any source that is not its own
            // list box or its own entry box.
            || self.fellowship.on_element_message(ui, m, view)
            // The friends panel's element-message handler -- the Add, Remove
            // and Tell buttons, the friends list's selection and the name box's keystrokes. Its
            // gate is the same shape as its neighbour's: the button arm refuses any id outside
            // `0x10000514`..`0x10000516`, and the other two refuse any source that is not its own
            // list box or its own entry box.
            || self.friends.on_element_message(ui, m, view)
            // The abuse panel's element-message handler -- its page buttons and the two text
            // boxes. The response notice is delivered by `update`; this is the input and request
            // half of the same panel.
            || self.abuse.on_element_message(ui, m, view)
            // The squelch panel's element-message handler -- the Remove,
            // Squelch Character and Squelch Account buttons, the squelch list's selection and the
            // name box's keystrokes. Its gate is the same shape as its three neighbours': the
            // button arm refuses any id outside `0x10000547`, `0x1000054B` and `0x1000054C`, and
            // the other two refuse any source that is not its own list box or its own entry box.
            // It takes no `view`, because the client's cap reads the panel's own squelch list and
            // not the model -- and retail never fills that list.
            || self.squelch.on_element_message(ui, m)
            // The urgent assistance panel's element-message handler — the
            // three-page wizard and the Help-channel broadcast its Send button raises. Its gate
            // is the narrowest here: the button arm answers only `0x100001B6`, `0x100001B7`,
            // `0x100001BD` and `0x100001C0`, and the typing arms only its own entry box.
            || self.urgent_assistance.on_element_message(ui, m)
            // The contracts panel -- the two sort buttons, the Abandon button, and the contracts
            // list's selection and activation. Its gate is the same shape as its two neighbours':
            // the button arm refuses any id outside `0x100005CE`, `0x100005D6` and `0x100005DC`,
            // and the two list arms refuse any source that is not its own list box.
            || self.contracts.on_element_message(ui, m, view)
            // A rejected generic drag on a salvage row belongs to that row's
            // parent item list, just as the inventory and shortcut lists handled by the gameplay
            // screen's drag-start path do. The salvage panel then consumes the successful
            // item list's begin-drag notice and removes the picked row locally. This panel owns the
            // list, so the already-forwarded `0x21` lands here rather than widening the screen's
            // inventory-only registry.
            || self.salvage.begin_item_drag(ui, m).is_some()
            // The salvage list's drag-over, reached through the item-list drag handler the
            // panel's post-init registers on it. Separate from the line below for the reason the
            // vendor pair is: the drag-acceptable test asks whether the player owns the object and
            // whether it is suitable for salvage, about an object that is in the **pack**, not in
            // this window's list, so it needs the whole view. Without it a drag over the salvage
            // window shows no accept hint.
            || self.salvage.on_drag_cursor_over(ui, m, view)
            // The item list element's element-message handler's `0x15` arm for the same list --
            // the clear the hint above needs on the drop. Same split as `vendor.on_drop_release`:
            // keyed on the message rather than on a button id,
            // and it must run whether or not the drop is taken.
            || self.salvage.on_drop_release(ui, m)
            // The salvage panel's element-message handler -- the Salvage
            // button, the close X, and the salvage list's **double**-click row removal. Its gate
            // is the same shape as its neighbours': the button arm refuses any id outside
            // `0x10000076` and `0x10000078`, and the `0x1C` arm refuses any `p1` but `10`
            // and any source that is not one of its own slots.
            || self.salvage.on_element_message(ui, m, view)
            // The housing window's three handlers, in the same split its two
            // `<ENVP>` neighbours use: the hover hint over either of its two item lists, the
            // clear on the drop, and the client's button and
            // visibility arms. The visibility arm is the one that is *not* like its neighbours':
            // the current house operation is written by a page becoming visible, so the panel must see
            // `0x18` for `0x10000090` / `0x10000097` or every drop is refused silently.
            || self.slumlord.on_drag_cursor_over(ui, m, view)
            || self.slumlord.on_drop_release(ui, m)
            || self.slumlord.on_element_message(ui, m, view)
            // The mini game panel's element-message handler -- the three
            // buttons on message `1` and the board's left **press** on `0x1C`. Its gate is the
            // same shape as its neighbours': the button arm refuses any id outside `0x10000175`,
            // `0x10000176` and `0x10000177`, and the press arm refuses any `p1` but `7` and
            // any source that is not inside its own `0x10000174`.
            || self.minigame.on_element_message(ui, m)
            // The spellbook, plus the one hop the client does not need: its double-click arm
            // raises the add-spell-shortcut notice and the spell bar is a *listener*, so the
            // notice is carried across here rather than through a broadcast neither struct can
            // register for. See [`spellbook_message`](Self::spellbook_message).
            || self.spellbook_message(ui, m, view)
            // The vendor panel's eleven buttons. The selection and split are the two global
            // values its button handler reads before dispatch; they reach this crate through the
            // view rather than through the message. The handler takes `ui` because two of its
            // four arms — the filter strip's `7` and the tab panel's `0x2C` — reach the
            // items-list update, which reads the menu's selected row and re-fills the stock list.
            // The vendor sell page's item-list drag-over, reached through
            // its drag handler. Separate from the line below because it needs the whole view --
            // it asks whether the item is player-owned, how many items it contains, and
            // whether it is acceptable, about an object in the *pack*, not in any
            // of this window's three lists.
            || self.vendor.begin_item_drag(ui, m, split, max_split).is_some()
            || self.vendor.on_drag_cursor_over(ui, m, view)
            // The item list element's element-message handler's `0x15` arm
            // clears the drag-accept icon on the tile the drop landed on, before
            // drop-release handling runs. The gameplay screen has that arm for its own four lists;
            // this is the same clear for the three that live here. Separate from the line below
            // because it is keyed on the message rather than on a button id, and it must run
            // whether or not the drop is taken.
            || self.vendor.on_drop_release(ui, m)
            || self.vendor.on_element_message(ui, m, view)
            // The spell-cast sub-menu's item-list drag-over handler, reached through the
            // item-list drag handler registered on **every one** of the eight spell-item lists
            // (`0x100000B6`). It takes no `view`: the whole decision is a nonzero spell id read off
            // the drag proxy, so unlike its four neighbours here it asks the model nothing. This
            // is the only path by which `0x3E` reaches the spell bar -- the gameplay-screen
            // drag-over path sweeps inventory and its eighteen shortcut lists, and the spell bar's
            // eight are neither -- so without it a spell carried over the bar paints nothing.
            || self.spellcasting.on_drag_cursor_over(ui, m)
            // The item list element's element-message handler's `0x15`
            // arm for the same eight lists -- the clear the hint above needs on the drop. It
            // returns false on purpose, so the `0x15` still reaches drop-release handling below.
            || self.spellcasting.on_drop_release(ui, m)
            // The spellcasting bar's select and cast arms — the cast button, the endowment icon and
            // a double-click on a bar row.
            || self.spellcasting.on_element_message(ui, m, view)
            // The secure trade panel's item-list drag-over handler, reached through the one trade
            // drag handler registered on the self-item list (`0x10000088`) and on nothing else,
            // plus the client's default handling for the partner-item list (`0x10000081`), which
            // has no handler and which the shipped layout flags `UI_ItemList_IsVendor`, so that
            // default is the client's early-out and the partner's half of the table paints
            // nothing. Separate from the line below for the same reason as the vendor and salvage
            // pairs: it asks whether the player owns an object that is in the **pack**, not in
            // either of this window's lists, so it needs the whole view. Without it a drag over
            // the trade table shows no accept hint.
            || self.trade.on_drag_cursor_over(ui, m, view)
            // The item list element's element-message handler's `0x15`
            // arm for the same two lists -- the clear the hint above needs on the drop. Same
            // split as `salvage.on_drop_release`: keyed on the message rather than on a button
            // id, and it must run whether or not the drop is taken.
            || self.trade.on_drop_release(ui, m)
            || self.trade.on_item_list_press(ui, m)
            // The secure trade panel's element-message handler's
            // message-1 arm: the trade toggle, "Clear All Items" and the close button.
            || self.trade.on_element_message(&mut ui.requests, m)
            // The combat window's three arms — a press or a click on one of the attack-height
            // buttons, and the power gauge's own scrollbar position. Each becomes a request because
            // the combat system lives across the seam in `dereth_client_model`; see
            // [`crate::hud::combat_window`].
            || self.combat_window_message(ui, m, view)
            // The spew box panel's element-message handler — one over-head bubble removing
            // itself five seconds after it went up. Last, because its gate is the narrowest in
            // this list: one element id (`0x1000004A`) *and* one message id (`0x10000003`),
            // neither of which any other panel here uses, so nothing above can be shadowed by it.
            // Without this arm a bubble would stay up for the rest of the session.
            || self.spew.on_element_message(ui, m)
            // The journal panel's element-message handler and
            // the page list's. The page list is handed the
            // journal because that is what the client's global journal panel is: both of its gestures —
            // double-click a row, delete the selected page — are calls on the journal panel.
            || self.journal.on_element_message(ui, m, view)
            || self.page_list.on_element_message(ui, m, &mut self.journal)
    }

    /// The spellbook's element-message handler and the notice it raises.
    ///
    /// The add-spell-shortcut notice is a client-internal broadcast
    /// with exactly one receiver in the client, the spellcasting panel's add-spell-shortcut
    /// notice. Both ends are plain structs here, so the notice is taken off the spellbook and
    /// handed to the bar in the same call — the same one-step hop `crate::requests` and the
    /// screen's `panel_messages` use, and for the same borrow reason.
    ///
    /// The bar's answer is deliberately **not** or-ed into the return: the client's spellbook
    /// consumes the double-click whether or not the bar accepted the spell (adding a favourite
    /// refuses a duplicate silently), and a panel dispatch that fell through to the next panel because the
    /// tab already held that spell would be a different bug.
    fn spellbook_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        let consumed = self.spellbook.on_element_message(ui, m, view);
        if let Some(spell) = self.spellbook.take_add_shortcut_notice() {
            self.spellcasting.on_add_spell_shortcut(ui, view, spell);
        }
        consumed
    }

    /// The combat window's two handlers, in the client's own order: the window's own
    /// element-message handler and then the three check boxes' own
    /// element-message handler.
    ///
    /// **They are two objects in the client and are kept two here.** Both switch on message `1`;
    /// the window's arm is keyed on the three attack-height button ids and the check boxes'
    /// on their own element, so a single `match` on the message id would let a check-box click be
    /// read as an attack. The window runs first because that is the child-to-parent bubble order:
    /// the check box is a child of the page.
    /// The same separation applies to the five panel-bound boxes, with
    /// the client's option write raised as a request.
    ///
    /// Both holders are offered the message and each refuses a source that is not one of its own
    /// elements, which is what keeps the allegiance box and the fellowship four apart without a
    /// second id table.
    fn panel_option_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        if let Some(r) = self.allegiance_options.on_element_message(ui, m) {
            ui.requests.emit(r);
            return true;
        }
        if let Some(r) = self.fellowship_options.on_element_message(ui, m) {
            ui.requests.emit(r);
            return true;
        }
        false
    }

    fn combat_window_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        if let Some(r) = self.combat_window.on_element_message(ui, m, view) {
            ui.requests.emit(r);
            return true;
        }
        if let Some(r) = self.combat_window.on_checkbox_message(ui, m) {
            ui.requests.emit(r);
            return true;
        }
        false
    }
}

/// The HUD model's two direct receivers on this panel set.
impl dereth_client_contract::panels::HudPanels for RemainingPanels {
    fn spew_offer(
        &mut self,
        ty: u8,
        body: &str,
        _feedback: dereth_client_contract::feedback::Feedback,
    ) -> bool {
        self.spew.recv_display_final_string_info(ty, body)
    }
    fn spew_trace(&self) -> (bool, usize, u64) {
        (
            self.spew.list.is_some(),
            self.spew.model.pending.len(),
            self.spew.drawn,
        )
    }
    fn spew_clear_pending(&mut self) {
        self.spew.model.pending.clear();
    }
    fn abuse_response(&mut self, code: u32) {
        self.abuse.recv_response(code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `crate::panels::catalogue::PANEL_PAGES` — the two page ids this module starts its
    /// searches from are real pages of the toolbar stack and not sub-panel ids.
    #[test]
    fn the_two_pages_are_toolbar_stack_pages() {
        let pages = crate::panels::catalogue::PANEL_PAGES;
        assert!(pages.contains(&CHARACTER_PAGE.0), "{CHARACTER_PAGE:?}");
        assert!(pages.contains(&SPELL_PAGE.0), "{SPELL_PAGE:?}");
        // And the sub-panels are *not* pages — they are children of one.
        assert!(!pages.contains(&super::super::skills::PANEL.0));
        assert!(!pages.contains(&super::super::attributes::PANEL.0));
        assert!(!pages.contains(&super::super::spellbook::PANEL.0));
    }
}
