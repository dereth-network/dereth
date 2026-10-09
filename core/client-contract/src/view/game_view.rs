//! The read-only view of the game model the interface draws from.

use super::*;

pub trait GameView: std::fmt::Debug {
    /// What the world's era means for what can be shown ([`EraView`]); `None` from a view that
    /// has no world.
    fn era(&self) -> Option<&EraView> {
        None
    }

    /// Resolved world systems; a view without a world uses the default profile.
    fn era_features(&self) -> dereth_primitives::EraFeatures {
        self.era().map_or(
            dereth_primitives::EraFeatures::END_OF_RETAIL,
            EraView::features,
        )
    }
    fn era_ui(&self) -> crate::era::EraUiFacts {
        crate::era::EraUiFacts::for_profile(
            self.era().map_or(dereth_primitives::EraId::Eor, |e| e.era),
        )
    }
    fn aetheria_slots(&self) -> u8 {
        let unlocks = self
            .player()
            .and_then(|p| self.int_stat(p, 0x142))
            .unwrap_or(0);
        crate::era::aetheria_slots(self.era_features(), unlocks)
    }

    /// The player object, once `LOGIN_COMPLETE` (global message `0x0B`) has fired.
    fn player(&self) -> Option<ObjectId> {
        None
    }
    fn name(&self, _id: ObjectId) -> Option<&str> {
        None
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        None
    }
    /// The requested integer quality.
    fn int_stat(&self, _id: ObjectId, _prop: u32) -> Option<i32> {
        None
    }
    /// The queried secondary attribute as `(current, max)`.
    fn vital(&self, _id: ObjectId, _which: Vital) -> Option<(u32, u32)> {
        None
    }
    /// The shared inventory parent shown by either interface and used for pickups.
    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.player()
    }
    fn container_contents(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// The contained-containers list — the **side packs**, which are a
    /// separate ordered list from [`Self::container_contents`] and are what a
    /// `UI_ItemList_IsContainer` list shows.
    fn contained_containers(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// The inventory-placement list — `(iid, loc)` per wielded or worn item.
    ///
    /// The paper doll's inventory rebuild walks exactly this list and places each `(iid, loc)`
    /// into its doll location, which is the only source the equipment doll has.
    fn equipment(&self, _id: ObjectId) -> &[(ObjectId, u32)] {
        &[]
    }
    /// The loose-item count used by the item list's container-size update
    /// writes into `UI_ItemList_FixedListSize` for a non-container list. `None` when the object is
    /// unknown; a negative value is the client's "unbounded" and selects the empty-slot update.
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        None
    }
    /// The subcontainer count used by a container-list update.
    fn containers_capacity(&self, _id: ObjectId) -> Option<i32> {
        None
    }
    /// An inventory request naming this object is outstanding, so its icon is ghosted.
    fn item_waiting(&self, _id: ObjectId) -> bool {
        false
    }
    /// The item-list insertion index for the list identified by
    /// its parent container id and container-list flag — the optimistic row a drop draws before the
    /// shard answers, together with its insertion index.
    ///
    /// Retail holds this on the list element, so the panel that owns the list also owns the row.
    /// This build rebuilds every list from the world on each changed frame, so the row travels the
    /// other way: it lives beside the object table and the panels splice it into the ids they
    /// fill from, at this index. `None` is the ordinary case — at most one list in the whole UI
    /// has a pending row at a time, because the player has one pointer.
    fn pending_row(
        &self,
        _container: Option<ObjectId>,
        _containers_list: bool,
    ) -> Option<(ObjectId, u32)> {
        None
    }
    /// The selected object's id.
    fn selection(&self) -> Option<ObjectId> {
        None
    }
    /// Radar objects already converted to player space by the smart box.
    fn radar_objects(&self) -> &[RadarEntry] {
        &[]
    }
    /// The burden indicator's only input.
    fn load(&self) -> Option<f32> {
        None
    }
    /// The helpful and harmful enchantment collections.
    fn enchantment_counts(&self) -> (u32, u32) {
        (0, 0)
    }
    /// Active enchantments joined to the `SpellTable`, in no particular order; the
    /// effects panel sorts them by name.
    ///
    /// The default is empty, matching the absent-player-description guard before
    /// `0x0013`: rebuilding leaves the list cleared rather than drawing an empty row.
    fn active_effects(&self) -> Vec<EffectEntry> {
        Vec::new()
    }
    /// The vitae multiplier, with `< 1.0` meaning a penalty is active.
    fn vitae(&self) -> Option<f32> {
        None
    }
    /// The three vitae-display inputs, or `None` when no player description is
    /// available; that case writes nothing.
    fn vitae_display(&self) -> Option<VitaeDisplay> {
        None
    }
    /// The character sheet's inputs, or `None` when no player description is available;
    /// that case writes nothing.
    fn character_info(&self) -> Option<CharacterInfo> {
        None
    }
    /// The frame clock needed by panels that own a timer.
    ///
    /// Carried on the view because `dereth_ui_screens::panels::linkstatus::LinkStatusPanel`
    /// tracks its last ping request, next update and round trip, as the original panel
    /// does. The default `0.0` freezes those timers, which is what an unclocked host
    /// should get.
    fn now(&self) -> f64 {
        0.0
    }
    /// How many `0x01EA Character_ReturnPing` have arrived.
    ///
    /// A **count**, not a timestamp: the client computes
    /// the current time less the last ping request's time at notice-delivery time, and this build
    /// has no notice bus, so the panel does the subtraction on the frame it first sees the count
    /// move. The difference is at most one frame of the round trip and is stated rather than
    /// hidden.
    fn ping_returns(&self) -> u64 {
        0
    }
    /// The link-status holder's packet-loss figure.
    ///
    /// A **ratio**, not a percentage, in spite of the accessor's name and in spite of the shipped
    /// string putting a `%` after it: the average-packet-loss accessor is
    /// `2 * (NAKed + retransmitted) / (received + sent)` and nothing multiplies it by a hundred.
    ///
    /// The default is `1.0` because that is the packet-loss field's initial value
    /// (the client's constructor writes `1.0f` into that field), i.e. a client that
    /// has heard nothing from a server reports total loss. It is **not** an `Option`: in retail
    /// the link-status panel's update sets its packet-loss line's float variable unconditionally —
    /// a `????` belongs to the ping line alone.
    fn packet_loss_percent(&self) -> f32 {
        crate::linkstatus::INITIAL_PACKET_LOSS
    }
    /// The portal-storm level, as the portal-storm-level notice carries it.
    ///
    /// **A `f32`, not an integer.**
    /// The portal-storm-level notice takes an `M` (a `float`) and
    /// the storm indicator's update compares it with `0.0` as a float.
    /// The level on the wire
    /// is `0x02C9`/`0x02CA`'s `extent`, which ACE sends as a fraction — so truncating it to an
    /// integer would turn every storm below 1.0 into "no storm".
    fn portal_storm_level(&self) -> f32 {
        0.0
    }
    /// A character option bit read by the Character Options page.
    ///
    /// `dereth_client_runtime::hud::HudView` implements it. With the trait's `false` as the only answer,
    /// all 50 rows of the Character Options page would open unticked whatever the server had
    /// sent, which is why `crate::options::character::CharacterSettingsPage::values_seen`
    /// exists.
    ///
    /// The default is still `false` rather than an `Option`, because that is what the *page* can
    /// use; a host that wants to tell "off" from "not asked" apart asks
    /// `dereth_client_runtime::hud::character_option`, which answers `Option<bool>`.
    fn player_option(&self, _o: PlayerOption) -> bool {
        false
    }

    /// The player module's default option value — the value *Restore Defaults* writes into
    /// one character option, which the check box reads at bind
    /// time and the page's restore-defaults writes back.
    ///
    /// **`Option`, not `bool`, and that is the whole point.** The function is a membership test
    /// against a compiled-in true-list, so *every* option has an answer in retail; a host that
    /// cannot supply one has to say so rather than answer `false`, because a Defaults button that
    /// silently unticked 49 boxes would look like it worked.
    /// `crate::options::character::CharacterSettingsPage::restore_default_values` counts the
    /// rows that got an answer, so "Defaults is not available on this host" and "Defaults set
    /// every option off" are different numbers.
    ///
    /// The table is `dereth_client_model::player::options::DEFAULT_TRUE_ORDINALS`, and this crate has no
    /// edge to `dereth-client-model`; `dereth_client_runtime::hud::HudView` does, and is the implementor.
    fn player_option_default(&self, _o: PlayerOption) -> Option<bool> {
        None
    }

    /// Read a floating-point gameplay option by property id — the read
    /// half of the client's named-property arm.
    ///
    /// The only two callers are the Chat Options page's opacity sliders, `0x10000080` and
    /// `0x10000081`, and both are **top-level** properties of the gameplay options — no window
    /// index, because the general section is one pair of sliders for all five chat windows.
    ///
    /// `None` is a module that carries no value, which is not a module that carries `0.0`: the
    /// client's fallback is the option's default, which is `0.5` and `1.0` out of
    /// property collection `0x78000001`, and a zero would make the chat window invisible.
    fn gameplay_option_float(&self, _property: u32) -> Option<f32> {
        None
    }

    /// Read property `0x1000007F` for one chat window — that window's
    /// 64-bit text-type filter, the read half of the 64-bit check box's
    /// value getter.
    ///
    /// The chat-option structure lookup indexes the gameplay options' `0x1000008C` array by
    /// the window id less 1, so this is per window and a window with no row in that array answers
    /// `None` — which the page turns into the window's own default mask, exactly as
    /// the value getter's default-preloaded result does.
    fn chat_window_filter(&self, _window: u32) -> Option<u64> {
        None
    }

    /// The four facts the client's not-a-stack arm branches
    /// on before it sends a health query or an item-mana query.
    ///
    /// `None` is *"this host does not answer the question"* and is not the same as a selection
    /// that asks for neither query: the first must not read as the second, or a build with no
    /// producer looks exactly like a build whose selection genuinely wanted nothing. See
    /// `dereth_ui_screens::screens::gameplay::ToolbarSelection::query`.
    fn selection_query_facts(&self, _id: ObjectId) -> Option<SelectionQueryFacts> {
        None
    }

    /// The toolbar's selected-object meters: `(health, mana)`, each 0.0–1.0 or `None`
    /// for no answer yet.
    ///
    /// These are the *only* place the retail client keeps either fraction:
    /// the object-health notice and the item-mana notice write
    /// `METER_LEVEL` straight from the `0x01C0` / `0x0264` reply and nothing stores it per object.
    /// `dereth_client_model::combat::SelectedMeters` holds them, and this is their reader.
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        (None, None)
    }
    /// the AC N/E coordinate pair.
    fn player_coords(&self) -> Option<(f32, f32)> {
        None
    }
    /// The game-time string off the client's current game time —
    /// `(date, time-of-day name)`, already formatted, for the map panel's first
    /// block.
    ///
    /// `dereth_scene::sky::GameClock` drives the sky off that object, and this seam is how the
    /// map panel asks it: the element is one retail updates unconditionally every five seconds.
    ///
    /// `None` is retail's no-current-game-time arm, which is **not** a blank
    /// element: see `crate::mapradar::map::date_time_text`.
    ///
    /// This is *not* gated on being outdoors. The is-outside call sits below this
    /// block and guards only [`Self::player_coords`]' two consumers.
    fn game_date_time(&self) -> Option<(String, String)> {
        None
    }
    /// The is-player-outside test, which is
    /// also what picks the radar's 75-vs-25 range and the chat sweep's radius.
    ///
    /// `dereth_client_runtime::hud::HudView` implements it off the player's own `objcell_id`. With the
    /// trait's `true` as the only answer, the radar would draw at the outdoor 75 inside every
    /// building and every dungeon.
    ///
    /// The default stays `true` rather than becoming an `Option`: a host that cannot answer is
    /// outdoors as far as every screen is concerned, and the two arms are the only two the client
    /// has. What separates "outdoors" from "not asked" is whether the host implements it, which
    /// is a compile-time question and not a runtime one.
    fn player_outside(&self) -> bool {
        true
    }
    /// The player's heading, in degrees.
    fn player_heading(&self) -> f32 {
        0.0
    }
    /// the `/radar off` state, separate from window visibility.
    fn radar_blank(&self) -> bool {
        false
    }

    /// The text properties at the top of the player module's gameplay-options collection, which
    /// the server keeps for the character and returns at login: `None` before the module has
    /// arrived, and an empty list when it holds none.
    fn player_module_strings(&self) -> Option<Vec<(u32, String)>> {
        None
    }

    /// The link-status holder's connection status — the link lamp's only input.
    ///
    /// The connected state carries **seconds since the last datagram from the current
    /// server** and not a round-trip time. `None` is the not-connected case, where the client
    /// leaves the elapsed time at 0.0 and the flag clear — which
    /// `crate::hud::indicators::link_status::link_state` turns into the "lost" lamp.
    ///
    /// One further quirk is reproduced there rather than here: the connection-status query clamps its
    /// answer to 15.0 while the no-drop-kick flag is set.
    fn link_status(&self) -> Option<f64> {
        None
    }

    /// The object id held by quickbar slot `slot`.
    ///
    /// A player-description update walks all eighteen shortcut entries and adds every
    /// nonempty entry to its corresponding slot. These are `PlayerModule` state, carried
    /// by `0x0013 Login_PlayerDescription` and saved by the 480-second flush, rather
    /// than object-table state. A shortcut may name an object the client has never
    /// seen; that produces a delayed slot rather than an empty one.
    fn shortcut(&self, _slot: u32) -> Option<ObjectId> {
        None
    }

    /// The `SkillTable` joined to the player's quality state.
    ///
    /// One entry per `SkillTable` key, in the table's own key order; the panel does the grouping
    /// and the alphabetical sort rather than requiring the host to provide display order.
    fn skills(&self) -> &[SkillEntry] {
        &[]
    }

    /// The player's known spell IDs joined to the spell table.
    fn spellbook(&self) -> &[SpellEntry] {
        &[]
    }

    /// The most recent newly learned spell, as a receipt serial and spell id.
    fn last_learned_spell(&self) -> Option<(u64, u32)> {
        None
    }

    /// The most recent confirmed successful formula test.
    fn research_success(&self) -> Option<crate::research::ResearchSuccess> {
        None
    }

    /// Look up `spell_id` in the spell table — **one row, whether
    /// or not the player knows the spell.**
    ///
    /// This is deliberately not `spellbook().iter().find(...)`, for the reason
    /// [`Self::is_spell_known`] gives in the other direction: `spellbook()` is the player's
    /// spell book joined to the table, and the two questions have different answers exactly
    /// where it matters here. The spellcasting panel's cast-button tooltip and
    /// its endowment icon both ask the spell table about
    /// **the endowment spell** — the spell on the wand in the player's hand — and a wand's spell
    /// is ordinarily not in the player's book at all. Answering from the book would leave the
    /// caption and the endowment icon blank for every caster in the game.
    ///
    /// The row is reduced to [`SpellEntry`] because that is already what a spell-bar row carries
    /// and what `crate::items::widget::spell_recipe` needs; `id` is echoed back so a caller can
    /// pass the entry on unchanged.
    ///
    /// The default is the join, which is all a host without the table can say.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.spellbook().iter().find(|s| s.id == spell_id).cloned()
    }

    /// Is `spell` a key of the player's spell book?
    ///
    /// This is *not* [`Self::spellbook`]`.contains`, and the difference is the whole reason it
    /// exists. `spellbook()` is the book **joined to the `SpellTable`**, so a spell the character
    /// knows whose id has no table row is absent from it. The runtime
    /// prunes a favourite that fails the known-spell test **and tells the shard**
    /// (`0x01E4`), so answering that question with the join would put an unrequested edit on the
    /// wire for a spell whose only problem is missing metadata.
    ///
    /// The default is the join, which is all a host without a `PlayerDesc` can say.
    fn is_spell_known(&self, spell_id: u32) -> bool {
        self.spellbook().iter().any(|s| s.id == spell_id)
    }

    /// The spell id joined to the component table — everything
    /// the spell-examine pane reads for one spell.
    ///
    /// `None` is the client's no-magic-system / unknown-spell arm: the pane is not
    /// filled and the window is not shown.
    fn spell_examine(&self, _spell_id: u32) -> Option<SpellExamineView> {
        None
    }

    /// The component-object lookup — which object in the player's own
    /// inventory a component row in the spell examine pane names.
    ///
    /// The argument is the row's `0x10000010` component SCID. The host maps it to a
    /// WCID and searches the seven component-tracker categories for the first carried
    /// object of that class. `None` means no tracker, no such class or no carried
    /// instance; the click then does nothing.
    fn component_object_id(&self, _scid: u32) -> Option<dereth_primitives::ObjectId> {
        None
    }

    /// The component tracker's is-owned test for a formula slot's SCID — whether the
    /// player holds any of that component.
    ///
    /// The default is `false`, which is deliberately the *marked* answer: a host with no
    /// component tracker takes the client's show-the-mark arm, so a view
    /// that cannot answer shows the "you do not have this" mark rather than hiding it.
    fn component_is_owned(&self, _scid: u32) -> bool {
        false
    }

    /// The update-spell-components notice, as a serial.
    ///
    /// The client is pushed that notice whenever a component object is offered to the tracker;
    /// this build pulls, so a reader that sees a different number does what the listener does —
    /// the same shape as [`Self::examine_request`]'s serial. `0` from a host that has no tracker
    /// at all, which never changes and therefore never refreshes.
    fn component_serial(&self) -> u64 {
        0
    }

    /// The non-raw attribute query for `id` — the number
    /// the attribute row writes into its value text, for one of the six
    /// primary attributes (1 Strength ... 6 Self). `None` renders as `"???"`.
    ///
    /// Not `raw`: the displayed value is the enchanted one, which is why a buffed character's
    /// attributes page reads higher than their creation profile.
    fn attribute(&self, _id: u32) -> Option<i32> {
        None
    }

    /// The skill footer's inputs for one skill; see [`SkillAdvancement`].
    fn skill_advancement(&self, _id: u32) -> Option<SkillAdvancement> {
        None
    }

    /// The attribute footer's inputs for one of nine rows; see
    /// [`AttributeAdvancement`].
    ///
    /// `secondary` distinguishes the three vital rows from the six primary attributes.
    /// The original row-kind query returns 8 for a primary and a different value for
    /// a vital. The flag cannot be derived from the stat id because the id spaces
    /// overlap: stat 2 is Endurance as a primary and Health as a secondary.
    fn attribute_advancement(&self, _id: u32, _secondary: bool) -> Option<AttributeAdvancement> {
        None
    }

    /// The local player character's name for the stat-panel header.
    ///
    /// Separate from [`Self::name`] because it specifically selects the local player.
    /// The host can answer from the received player description before the player's
    /// world object exists; the vitals panel uses the same early-availability seam.
    fn character_name(&self) -> Option<&str> {
        None
    }

    /// The journal path builder's three globals.
    ///
    /// The settings directory, current world name, and local player's singular object name — the same
    /// three `dereth_client_shell::ui::UiShell::screen_layout_path` needs for
    /// the screen-layout path builder, which is the precedent this follows. None of them is
    /// reachable from this crate, and the journal is the one panel whose model outlives the
    /// process.
    ///
    /// `None` means "no journal file", and the panel then keeps its pages in memory only: a build
    /// with no preferences file, no world name or no character yet has nowhere to write and must
    /// not guess. See [`crate::journal::JournalIdentity`].
    fn journal(&self) -> crate::journal::JournalView {
        crate::journal::JournalView::default()
    }
    fn journal_identity(&self) -> Option<crate::journal::JournalIdentity> {
        None
    }

    /// The client's inputs.
    fn experience_header(&self) -> Option<crate::statmgmt::XpHeader> {
        None
    }

    /// Resolve the display text from gender, heritage, and creature type for the
    /// player -- the heritage text's first half.
    ///
    /// The join is the host's for the same reason [`SkillEntry`]'s name is: the gender and
    /// heritage display-name lookups (enum `0x10000001` for gender, `0x10000002` for
    /// heritage group) against dat objects this crate must not know about. `None` is
    /// the gender/heritage display lookup returning 0, which leaves the field empty.
    fn gender_heritage_display(&self) -> Option<String> {
        None
    }

    /// The title-table lookup for the title id -- the title
    /// the stat panel's character-info update appends to the heritage text after a single space, and
    /// only when the lookup succeeds.
    fn display_title(&self) -> Option<String> {
        None
    }

    /// The titles panel's whole refresh input; see [`CharacterTitles`].
    ///
    /// Separate from [`Self::display_title`] because the header needs one resolved
    /// string while the Titles tab needs every id with its string. The tab also needs
    /// the current displayed title id to decide whether to offer the Display button.
    fn character_titles(&self) -> CharacterTitles {
        CharacterTitles::default()
    }

    /// The PK-status line -- see [`PkStatus`].
    fn pk_status(&self) -> PkStatus {
        PkStatus::default()
    }

    /// Integer qualities 6 and 7 -- `AvailableLuminance` and
    /// `MaximumLuminance`, the luminance pair's only inputs.
    ///
    /// The client reads both unconditionally and then decides whether to draw them; the level
    /// gate is the experience update's and lives in the panel, not here.
    fn luminance(&self) -> (i64, i64) {
        (0, 0)
    }

    /// Integer quality `0x18` — **available skill credits**, the number the skill panel's
    /// default footer and its untrained-selection footer both
    /// put on the footer's second and first lines respectively.
    fn skill_credits(&self) -> i64 {
        0
    }

    /// 64-bit integer quality 2 — **unassigned experience**, the other footer number and the
    /// one the raise buttons are affordable against.
    fn available_experience(&self) -> i64 {
        0
    }

    /// The fourteen filter bits
    /// the spellbook's filter test tests, **not** the button states.
    /// The player-module decoder defaults it to `0x3FFF` when section `0x0020` is absent, which is
    /// every school and every level.
    fn spell_filters(&self) -> u32 {
        crate::spellbook::DEFAULT_SPELL_FILTERS
    }

    /// The eight spell bars, which are the contents of the spell-casting submenu's item list.
    ///
    /// The spell bar's player-module refresh fills each tab's item list from this
    /// list, and the quick-cast notice indexes the *list*, so this is the
    /// order a quick-cast key resolves against. A tab outside `0..8` is empty.
    fn spell_tab(&self, _tab: usize) -> &[u32] {
        &[]
    }

    /// The client's three-condition test, answered by the
    /// host because the `ITEM_TYPE` constants live below this crate.
    ///
    /// `(item, spellID)` for the object wielded at equipment location `0x1000000` whose
    /// `ITEM_TYPE` carries `Caster (0x8000)` and whose public-description spell id is non-zero;
    /// `None` when any of the three fails, which is the arm that clears the endowment item and the
    /// endowment-selected flag.
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        None
    }

    /// The UI item's decoration fields for one object — everything
    /// the UI item's per-frame update reads off the weenie **after** the icon, and
    /// nothing else.
    ///
    /// One method instead of separate field accessors because all four decoration
    /// updates run together for every list slot. A single `PublicWeenieDesc` lookup
    /// avoids four hash probes.
    fn slot_decoration(&self, _id: ObjectId) -> Option<SlotDecoration> {
        None
    }

    /// Where the object can be worn or wielded: its valid-locations mask from its description,
    /// `0` for a thing that cannot be (or an object unknown).
    fn equip_locations(&self, _id: ObjectId) -> u32 {
        0
    }

    /// Whether the object is attuned (it cannot be given away or dropped), as far as its
    /// appraisal is known; `false` before it has been appraised.
    fn item_attuned(&self, _id: ObjectId) -> bool {
        false
    }

    /// The object's plural name, which item lists request for a stack.
    ///
    /// The client falls back to the object's name when the plural buffer is empty (length 1, i.e.
    /// the terminator alone), so `None` here means "use [`Self::name`]" and not "no name".
    fn plural_name(&self, _id: ObjectId) -> Option<&str> {
        None
    }

    /// The object's `AppraisalProfile` projected for the examination panel.
    ///
    /// `None` means no `0x00C9 Item_SetAppraiseInfo` has arrived for that object — which is what
    /// makes the pull in `dereth_ui_screens::panels::examination::ExaminationPanel::update` equivalent to the
    /// client's push: the profile cannot be answered before the reply lands.
    ///
    /// It carries only the six fields the panel reads, for the reason [`SlotDecoration`] does the
    /// same for a slot: the profile is a hundred-odd properties, this crate cannot see
    /// `dereth_protocol`, and inventing a wider seam would only hide which of them anything uses.
    fn appraisal(&self, _id: ObjectId) -> Option<AppraisalView> {
        None
    }

    /// Live `PublicWeenieDesc` facts for the item-examine inscription mouse handler:
    /// the inscribable bit and current equipment location. `None` means the object
    /// lookup missed. Hook-appraisal facts are deliberately not substituted; this
    /// handler reads the live object itself.
    fn inscription_mouse_facts(&self, _id: ObjectId) -> Option<(bool, u32)> {
        None
    }

    /// Send the examine-object notice for `id`, whose one listener is the examination panel.
    ///
    /// `(id, serial)`: the object the player last asked about, and how many times anything has
    /// asked. **The serial is the whole of why this is a pair rather than an `Option<ObjectId>`** —
    /// examining the *same* object twice must re-arm the awaited appraisal id and clear the
    /// current appraisal id, which is what re-opens a panel the player closed, and an
    /// id-only pull cannot see that. It is the counterpart of [`AppraisalView::delivery`] in the
    /// other direction and is a seam artefact of the pull model, declared as that one is.
    ///
    /// Three of the four routes that examine something live in `dereth_client_runtime::interaction`, which
    /// has no screen to call: the examine **cursor**'s pick, the examine cursor's second click, and
    /// the `0x2B SELECTION_EXAMINE` key. The fourth, the toolbar's identify button, reaches the
    /// panel on the screen directly as well — the examine notice handler is idempotent for the same
    /// id, so arriving twice is harmless.
    fn examine_request(&self) -> Option<(ObjectId, u64)> {
        None
    }

    /// The open book's id, page list and inscription.
    ///
    /// `None` is `bookID == 0`: no `0x00B4 Writing_BookOpen` has arrived, or the last one was
    /// closed. The panel opens on a change of [`BookView::opening`] and on nothing else, which is
    /// the same pull-instead-of-push seam [`Self::appraisal`] carries.
    fn book_session(&self) -> crate::book::BookSessionView {
        crate::book::BookSessionView::default()
    }
    fn open_book(&self) -> Option<BookView> {
        None
    }

    /// The last `0x0075 Character_StartBarber` notice, projected with the local player's two
    /// appearance-table selectors.
    ///
    /// `None` means no barber modal is active. The generation is deliberately separate from the
    /// payload because retail opens a fresh modal for every notice, even when the values match.
    fn barber(&self) -> Option<BarberView> {
        None
    }

    /// The allegiance hierarchy, already walked.
    ///
    /// The default is an empty roster, which is what a character in no allegiance has — and what
    /// all twelve `0x0020 Allegiance_AllegianceUpdate` in the capture corpus produce, since every
    /// one of them carries `total_members = 0` and no member records.
    fn oath_xp_cost(&self) -> Option<u32> {
        None
    }
    fn allegiance_roster(&self) -> AllegianceRoster {
        AllegianceRoster::default()
    }

    /// The game world's allegiance-abort counter — how many `0x0003
    /// Allegiance_AllegianceUpdateAborted` have arrived.
    ///
    /// The **panel's edge** for the one notice this build's per-frame pull cannot otherwise see:
    /// the allegiance panel's update-aborted notice updates the panel
    /// only when it is visible, and the update's first statement is the busy-latch clear that
    /// `dereth_ui_screens::panels::allegiance::AllegiancePanel::awaiting_update` carries. Without this the
    /// latch, set on every allegiance update request, would stay set for ever on a request the server
    /// aborts — the panel stuck "busy" with no answer coming.
    fn allegiance_update_aborts(&self) -> u64 {
        0
    }

    /// The game world's allegiance-update counter — how many `0x0020
    /// Allegiance_AllegianceUpdate` have arrived.
    ///
    /// The panel's edge for an answer that leaves the roster as it was: every answer runs the
    /// panel's update, whose first statement clears the busy latch, so an unchanged roster must
    /// still be seen to have arrived.
    fn allegiance_updates(&self) -> u64 {
        0
    }

    /// The current fellowship, when one exists.
    ///
    /// The default is `None` — *no fellowship* — which is what every host that has never received
    /// a `0x02BE Fellowship_FullUpdate` answers, and it is a **different** state from a
    /// fellowship with one member: the panel branches on exactly this
    /// pointer to choose which of its two frames is on screen.
    fn fellowship(&self) -> Option<FellowshipView> {
        None
    }

    /// The friends list in model order.
    ///
    /// The default is **empty**, which is the same answer a host that has received a `0x0021`
    /// carrying no records gives -- and that is not a gap: all five recorded `0x0021` are
    /// exactly that, an empty list with type `Full`, so the empty case is the one the recorded
    /// corpus witnesses and the panel has to be able to say it drew it.
    /// `dereth_ui_screens::panels::friends::FriendsPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel puts the
    /// online friends first and sorts each block by name, and that walk is the panel's.
    fn friends(&self) -> Vec<FriendEntry> {
        Vec::new()
    }

    /// The communication system's squelch iteration — the squelch DB's **character** hash,
    /// walked, with the
    /// empty entries and the account hash left out.
    ///
    /// It is the *communication system's* walk and not the panel's, which is why it crosses this
    /// seam already done rather than as a `SquelchDb`: the walk reaches into the communication
    /// system's own instance and asks each entry whether it is zoned and what name it carries,
    /// none of which this crate may know about.
    ///
    /// The default is **empty**, which is the same answer a host whose shard sent a `0x01F4` with
    /// an empty DB gives -- and that is the ordinary case, not a gap: every login clears the DB
    /// (the client clears it on login) and no recorded capture carries a populated one.
    /// `dereth_ui_screens::panels::squelch::SquelchPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel's sorted insert sorts the
    /// rows by name as they are inserted, and that walk is the panel's.
    fn squelch_list(&self) -> Vec<SquelchEntry> {
        Vec::new()
    }

    /// Tracked contracts in tracker-table order, ascending by contract id.
    ///
    /// The default is empty, which is what every host that has received no `0x0314`/`0x0315`
    /// answers — and that is the overwhelmingly common case: neither opcode appears anywhere in
    /// the recorded corpus. So
    /// `dereth_ui_screens::panels::contracts::ContractsPanel::rebuilds` is what separates "a character with
    /// no contracts" from "the panel never ran".
    ///
    /// This is not display order: the panel re-sorts by name or status on every rebuild.
    fn contracts(&self) -> Vec<ContractEntry> {
        Vec::new()
    }

    /// How many `0x0226 House_HouseStatus` notices this session has received.
    ///
    /// The house panel's failed-transaction notice is a bare jump to its
    /// update, and the update redraws the pane unconditionally — it reads nothing out of
    /// the notice, whose one `u32` all three registered receivers ignore. So what crosses this
    /// seam is not the message's *content* (there is none the client keeps) but the fact that one
    /// **arrived**: `dereth_ui_screens::panels::house::HousePanel` redraws whenever this number moves, which
    /// is retail's "redraw on every notice" expressed in a pull.
    ///
    /// The default is `0`, which is what every host that has received no `0x0226` answers — and
    /// that is the case a build with no shard is in, where the pane falls back to
    /// `dereth_ui_screens::panels::house::HousePanel::stand_in_for_the_login_query`.
    fn house_status_notices(&self) -> u64 {
        0
    }

    /// House data shaped for the six sections that read it.
    ///
    /// `None` is retail's null pointer, which is both "no house" and "the last thing that arrived
    /// was a `0x0226`" — the status handler deletes the object.
    fn house_data(&self) -> Option<HouseDataView> {
        None
    }

    /// How many `0x0225 House_HouseData` have arrived.
    ///
    /// The house-data redraw edge, alongside [`Self::house_status_notices`]. The
    /// house pane redraws on every notice, so it redraws whenever this count changes.
    fn house_data_notices(&self) -> u64 {
        0
    }

    /// The client's two host-side inputs.
    fn house_purchase(&self) -> HousePurchaseView {
        HousePurchaseView::default()
    }

    /// What the CRT's `localtime` would add to a Unix instant before any of this crate's date
    /// formatters sees it, in seconds.
    ///
    /// Six functions in the retail image format a date and **all six** run their instant through
    /// `localtime` first (the house panel's purchase-time line, the character sheet's
    /// birth/age/deaths section, the account-banned handler, the chat scroll's own timestamp,
    /// the load-file variable substitution and the house system's time conversion). MSVCR70's
    /// `localtime` reads `TZ` if it is set and
    /// otherwise calls `GetTimeZoneInformation`, so the number is the **operating system's**,
    /// daylight rule included, and nothing in the client's own configuration takes part.
    ///
    /// It is an accessor and not a constant because the answer depends on the instant: a house
    /// bought in January and one bought in July are an hour apart in the same zone. Hosts that
    /// cannot answer return `0` and render in UTC; `dereth_client_runtime::hud::HudView` implements it off
    /// `dereth_desktop::platform::local_utc_offset_secs`.
    ///
    /// The default is `0` rather than a panic because a screen must draw for a host that knows
    /// nothing — [`EmptyGameView`] is exactly that host.
    fn utc_offset_secs(&self) -> crate::ctime::UtcOffsetSecs {
        0
    }

    /// Vendor-window state. The default is a closed shop, which a host
    /// without an open vendor returns and the panel must draw as hidden.
    fn shop(&self) -> ShopView {
        ShopView::default()
    }

    /// Secure-trade state. The default is a closed window, as for a host
    /// without an open negotiation. The model is `dereth_client_model::trade`;
    /// `dereth_client_runtime::trade_view::trade` converts it.
    fn trade(&self) -> TradeView {
        TradeView::default()
    }

    /// Ask quietly whether `id` is acceptable to the vendor — **the drag cursor's
    /// question**, and the only thing the sell list's drag-over asks.
    ///
    /// It cannot be answered from [`Self::shop`]: a `ShopView` describes the three lists, and the
    /// object being dragged is in the *pack*. The decision needs ownership by the player,
    /// the contained-item count and the vendor profile's acceptability test against the open
    /// shop's own profile, which the game world's drag-acceptance query provides.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses —
    /// a host with no shop open shows the red circle, and a host with no vendor at all never raises
    /// the message, because the handler is bound to the sell list alone.
    ///
    /// It cannot be answered from [`Self::slot_decoration`]: the test reads the item's material
    /// type and bit `0x01000000` of `_bitfield`, neither of which is
    /// on that seam, **and** the `SalvageMultiple` character option, which is the player's and not
    /// the item's. The host answers it with
    /// `dereth_client_model::inventory::salvage::is_item_suitable`.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses:
    /// a host with no world refuses every drop rather than accepting every one.
    fn payment_lists(&self) -> crate::panels::slumlord::PaymentListsView {
        crate::panels::slumlord::PaymentListsView::default()
    }
    fn salvage_list(&self) -> crate::panels::salvage::SalvageListView {
        crate::panels::salvage::SalvageListView::default()
    }
    fn salvage_item_suitable(&self, _item: ObjectId, _panel_material: u32) -> bool {
        false
    }

    /// Whether the item is owned by the player, the client's first test and the one that produces
    /// the panel's only refusal string.
    fn item_owned_by_player(&self, _item: ObjectId) -> bool {
        false
    }

    /// `None` until a `0x021D House_HouseProfile` arrives.
    fn slumlord(&self) -> Option<SlumlordView> {
        None
    }

    /// Chess-panel state. `None` means the host has no chess model.
    fn minigame(&self) -> Option<MiniGameView> {
        None
    }

    /// How many `0x021D House_HouseProfile` have arrived — and the edge that
    /// **raises** the window: the slumlord window's house-profile notice ends in
    /// making it visible, through the element itself rather than by name.
    ///
    /// A **count**, not a flag, for the reason [`Self::house_data_notices`] is one: two profiles
    /// in one frame are two house updates in retail, and a second use of the same slumlord
    /// must re-raise a window the player closed.
    fn slumlord_notices(&self) -> u64 {
        0
    }

    /// The payment strings and paid-in-full result after this window's current drops.
    ///
    /// Each `drops` row carries `(wcid, amount, trade-note face value)` in drop order.
    /// The host replays them over a pristine profile copy. This matches the original
    /// window's accumulated payment state, which adds payment on insertion and removes
    /// it on removal.
    ///
    /// The arithmetic stays in `dereth-client-model` with `HousePaymentList`, rather than being
    /// duplicated here, as with [`Self::salvage_item_suitable`].
    fn slumlord_payment(&self, _rent: bool, _drops: &[(u32, i32, Option<i32>)]) -> SlumlordPayment {
        SlumlordPayment::default()
    }

    /// Check whether the selected rent or purchase payment still needs class `wcid` over the same
    /// replayed state — the client's last test, and the one that decides whether a drop is
    /// taken at all.
    fn slumlord_needs_more(
        &self,
        _rent: bool,
        _drops: &[(u32, i32, Option<i32>)],
        _wcid: u32,
        _trade_note_value: Option<i32>,
    ) -> bool {
        false
    }

    /// Apply `{wcid, num}` to the selected rent or purchase payment over the same replayed state —
    /// the boolean the slumlord window's add-payment gates its add-item on.
    ///
    /// It is **not** the same question as [`Self::slumlord_needs_more`], and the two are a pair
    /// because the client asks both: the drag-acceptance test asks whether more is needed about the *drag*, and
    /// the add then asks the payment step about the *insert*, so an item that passed the hover test can
    /// still be refused at the drop when the payment attempt finds the row already full.
    ///
    /// It cannot be inferred from [`Self::slumlord_payment`] either: the buy tab's text is
    /// `"<num> <name>"`, which carries **no paid count at all**, so a partial
    /// payment changes nothing visible and a panel that diffed the two strings would refuse every
    /// drop against the purchase price.
    fn slumlord_pay(
        &self,
        _rent: bool,
        _drops: &[(u32, i32, Option<i32>)],
        _wcid: u32,
        _amount: i32,
        _trade_note_value: Option<i32>,
    ) -> bool {
        false
    }

    /// The item's public weenie class id, used by the window's remaining-payment
    /// check and by each payment entry it adds.
    fn item_wcid(&self, _item: ObjectId) -> u32 {
        0
    }

    /// Reverse-lookup `wcid` in the dual enum map selected by group 10, enum `0x10000001`, and
    /// type `0x28`, yielding the trade-note value when present.
    fn item_trade_note_value(&self, _item: ObjectId) -> Option<i32> {
        None
    }

    /// The item's payment quantity: its stack size when nonzero, otherwise 1.
    /// This is the amount added to a house-payment entry.
    fn item_house_payment(&self, _item: ObjectId) -> i32 {
        1
    }

    /// The stack slider's top. The panel compares [`Self::split_size`] against it and takes the
    /// whole-stack arm when they are equal.
    fn max_split_size(&self) -> i32 {
        1
    }

    /// The object's material type, which latches into
    /// the window's material on its first row.
    ///
    /// `0` is the client's "no material", which is also the window material's empty value, so a miss
    /// leaves the window unlocked rather than locked to a material nothing can match.
    fn item_material_type(&self, _item: ObjectId) -> u32 {
        0
    }

    fn vendor_drag_item_accepted(&self, _item: ObjectId) -> bool {
        false
    }

    /// The secure-trade window's drag test, **ownership half**, asked with
    /// `quiet = 1` from its item-list drag-over.
    ///
    /// An unknown id is refused silently. An object the player does not own is refused, and
    /// unless `quiet` the client displays "You can only trade items you are carrying" as type
    /// `0x1A`. Otherwise the answer is whether the item is not already in the self items list.
    ///
    /// This is everything except the last step; the already-in-the-list half is the panel's own
    /// list and stays in `dereth_ui_screens::panels::trade::TradePanel::drag_accept_state`. The host
    /// answers it with the game world's trade-item acceptance query, which the **drop** path
    /// also uses; this is the hover's.
    ///
    /// Default **false**: the client's own answer when the weenie lookup misses is
    /// a silent refusal, so a host with no world refuses every drag rather than accepting one.
    fn trade_drag_item_acceptable(&self, _item: ObjectId) -> bool {
        false
    }

    /// The external-container window's drag test, asked with `quiet = 1` from
    /// its item-list drag-over -- **the hover hint over an open chest, corpse, ground
    /// pack or hook.**
    ///
    /// The drag test accepts when the window has no ground-object id or when that
    /// container cannot be found. If the container exists but the dragged item does
    /// not, it refuses. Otherwise it performs the hook-status check below.
    ///
    /// `quiet` is never read: every refusal is silent, unlike vendor or trade refusals.
    ///
    /// The hook check first clears its output flag and refuses an absent item or
    /// container. A container with hook-type mask 0 **or** accepted-item-type mask 0
    /// is not a hook and accepts. A hook with owner id 0 sets the output flag to 1
    /// and refuses. It also refuses an item whose hook-type mask is 0 or has no bit
    /// in common with the hook's mask. Otherwise it accepts exactly when the item's
    /// type intersects the hook's accepted-item-type mask.
    ///
    /// **So every container that is not a hook answers `1`, and the hint over a chest or a corpse
    /// is the plain green `0x10000040` for every item you can carry** -- the window reads neither
    /// an is-corpse test nor `ITEM_TYPE` nor ownership. The red `0x10000041` exists, but only
    /// for a hook whose location mask or item-type mask the carried object misses, and for a hook
    /// in nobody's house.
    ///
    /// Default **true**, which is the client's own answer when the window has no ground object:
    /// a host with no world paints the same green every non-hook container paints, rather than a
    /// red the drop would then contradict.
    fn external_container_drag_item_acceptable(&self, _item: ObjectId, _ground: ObjectId) -> bool {
        true
    }

    /// The item's public valid-location mask — what the paper
    /// doll's item-list drag-over reads.
    ///
    /// `None` is the client's own answer when the weenie lookup misses: the handler
    /// returns without writing any state, which is not the same as refusing.
    fn item_valid_locations(&self, _item: ObjectId) -> Option<u32> {
        None
    }

    /// Shared slot and canvas drag feedback. Missing slot objects and already-worn canvas
    /// items leave the existing hint unchanged.
    fn equipment_hover(&self, _item: ObjectId) -> EquipmentHover {
        EquipmentHover::default()
    }

    /// The spell's is-untargeted test — the first of the two tests the spellcasting panel's
    /// cast-button tooltip makes before it enables the Cast
    /// button.
    ///
    /// ```text
    /// f = the spell's formula
    /// t = the formula's targeting type
    /// return t == 0
    /// ```
    ///
    /// where an incomplete formula (any of its first five slots empty) has type 0, and otherwise the
    /// targeting type is read from one component: the walk goes up from slot **5** while the slot is
    /// filled and takes the last filled one (slot 4 when slot 5 is empty), and that component's
    /// targeting type is the answer —
    /// so this is a property of the *formula*, not a flag on the base. It is deliberately **not**
    /// the same question as `_bitfield & SelfTargeted (8)`, which
    /// [`SpellEntry::bitfield`](crate::view::SpellEntry::bitfield) already carries and which
    /// the tooltip tests separately: a spell can be either, both or neither, and each alone enables
    /// the button.
    ///
    /// Default false — a host with no spell table cannot say a spell needs no target, and the
    /// safe direction is the one that leaves the button greyed.
    fn spell_is_untargeted(&self, _spell_id: u32) -> bool {
        false
    }

    /// Whether the selected object is compatible with the spell, queried quietly.
    ///
    /// The function reads the spell base only to reach its target type and then delegates to
    /// the target-type compatibility test, which is the same predicate
    /// the game world's spell-casting operation runs before it sends. **The call here is quiet** where
    /// `CastSpell`'s is loud: this one runs to build a *tooltip*, and a
    /// chatty version would spam the feedback channel every time the selection moved.
    ///
    /// The object asked about is always [`Self::selected_object`], which is why it is not a parameter.
    ///
    /// Default false, for [`Self::spell_is_untargeted`]'s reason.
    fn spell_target_compatible(&self, _spell_id: u32) -> bool {
        false
    }

    /// Whether the item can be used with self as its target.
    ///
    /// The original client shifts the public useability word's high half down sixteen
    /// places with a sixteen-iteration loop, then tests bit 1. An endowed wand usable
    /// on the player needs no selection, the first and most common endowment-tooltip
    /// arm.
    ///
    /// Default false.
    fn item_useable_self_target(&self, _item: ObjectId) -> bool {
        false
    }

    /// Where `object` may be used from, its useability word's source half (usable at a distance,
    /// while viewed, from a container, wielded, on itself); `None` when the object or the word is
    /// not known.
    fn useability(&self, _object: ObjectId) -> Option<u32> {
        None
    }

    /// Whether the selected target is compatible with the item, queried quietly.
    ///
    /// The argument order is **target first, source second**. The game-layer compatibility query
    /// uses the same `(target, source)` order.
    ///
    /// Quiet, for [`Self::spell_target_compatible`]'s reason. The target is always
    /// [`Self::selected_object`].
    ///
    /// Default false.
    fn item_target_compatible(&self, _item: ObjectId) -> bool {
        false
    }

    /// The selected object: the vendor button handler reads it once before
    /// dispatching, and eight of its eleven cases use it.
    fn selected_object(&self) -> Option<ObjectId> {
        None
    }

    /// Is `id` anywhere in the allegiance **hierarchy the shard sent**, at any depth?
    ///
    /// [`Self::allegiance_roster`] cannot answer this: it carries the monarch, the patron and the
    /// player's own vassals, which is what the panel *draws*, while `GetData` walks the whole
    /// tree. The allegiance panel needs the whole tree: the Swear
    /// button is offered only for a selected player who is **not already a member** — so this is
    /// its own question rather than a derivation of the roster.
    fn allegiance_has_member(&self, _id: ObjectId) -> bool {
        false
    }

    /// The player's `InstanceID` quality **26 (`Monarch`)**, which is what the allegiance
    /// panel's quality-changed handler watches.
    ///
    /// Its post-init registers two handlers on the player,
    /// for group 7, quality `0x19` and quality `0x1A` — group 7 is the
    /// `InstanceID` table and `0x19` / `0x1A` are `Patron` (25) and `Monarch` (26).
    ///
    /// The recorded captures confirm the direction: every `0x02DA Qualities_UpdateInstanceID` with
    /// key `0x1A` on the recorded character is followed within milliseconds by a `0x001F`.
    ///
    /// **Only the monarch half is available here**: `dereth_client_model::weenie::mirror_stat_update` has an
    /// arm for key 26 and none for key 25, so a patron change that does not also change the
    /// monarch is invisible to this build. That is a narrower trigger than retail's, not a
    /// different one, and it is stated rather than papered over.
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        None
    }

    /// The stack slider's current value; the default is 1, which is what a shop's own splitter
    /// seeds to (the toolbar's selection-changed handler, vendor arm).
    fn split_size(&self) -> i32 {
        1
    }

    /// The component tracker's seven category lists joined to the player's desired quantities.
    ///
    /// The spell-component panel walks all seven tracker categories in order, emitting
    /// a header per category and a row per component. Each tracker list is already
    /// sorted by object name on insertion. Neither the panel nor this seam sorts it.
    ///
    /// The default is **seven empty categories, not an empty vector**: a character with no
    /// components still sees seven headers, and a host that has not filled the tracker must not be
    /// indistinguishable from one whose player carries nothing.
    fn spell_components(&self) -> Vec<ComponentCategory> {
        (0..7)
            .map(|category| ComponentCategory {
                category,
                rows: Vec::new(),
            })
            .collect()
    }

    /// Ask whether `obj` is an owned component and return its class id — is this
    /// **object** one of the components the player is carrying, and if so which class is it?
    ///
    /// The spell-component panel's selection-changed notice is its only caller in the
    /// panel layer, and the question it asks cannot be answered from
    /// [`Self::spell_components`]: that snapshot carries one object per row
    /// while the tracker's object-id hash carries **every**
    /// object of every stack. Selecting the third of five piles of Lead Scarabs in the world
    /// still highlights the Lead Scarab row in retail, and matching on the row's first object
    /// would miss four of the five.
    ///
    /// `None` is the client's `FALSE` return: not a component, or not owned.
    fn object_is_owned_component(&self, _obj: ObjectId) -> Option<u32> {
        None
    }

    /// The enchantment registry's cooldown test on the **player's own** registry, for the
    /// cooldown id an item carries.
    ///
    /// ```text
    /// for each entry e in the cooldown list:
    ///     if ((e.id & 0xFFFF) == key) {
    ///         remaining = (e.duration + e.start_time) - now;
    ///         if (remaining <= 0) { remove e; return 0; }
    ///         return 1;
    ///     }
    /// return 0;
    /// ```
    ///
    /// `key` is what the caller passes, and the item slot's cooldown display passes
    /// **the cooldown id `+ 0x8000`** — the offset that puts an item cooldown above the spell-id
    /// range the registry's spell totals count. Adding it is the *host's* job here, exactly as
    /// it is the caller's in the client, so the widget hands over the raw cooldown id.
    ///
    /// `now` is the frame clock, which reaches an item slot as `UiSystem::now`. It is a
    /// parameter rather than something the host samples for itself because every other value on
    /// this seam is a fact about a frozen world and this one is not: it changes between two calls
    /// in the same frame, and a cached copy would make the wedge jitter.
    ///
    /// `None` is the client's `return 0` — no registry, no such cooldown, or one that has expired.
    fn cooldown_remaining(&self, _cooldown_id: u32, _now: f64) -> Option<f64> {
        None
    }

    /// The `COMBAT_MODE` the toolbar's stance icon shows.
    ///
    /// The default is `NONCOMBAT_COMBAT_MODE`, which is what a character enters the world in and
    /// what retail's in-game screen shows on entry; it is deliberately not `UNDEF_COMBAT_MODE`,
    /// because with `UNDEF` all four buttons are hidden and the toolbar has a hole in it.
    fn combat_mode(&self) -> u32 {
        crate::combat_mode::NONCOMBAT
    }

    /// The player module's advanced-combat-UI option — bit 12 of the first player-option word.
    ///
    /// The combat window's set-combat-mode notice calls it **itself**,
    /// rather than reading the copy the combat mode setter keeps of it, so this is
    /// the option and not the combat system's cached word. When it is on, the classic combat
    /// cluster stays down in every mode: the advanced combat interface is a different window.
    ///
    /// Default `false` — `PLAYER_OPTIONS`' bit 12 is clear in the client's own defaults.
    fn advanced_combat_ui(&self) -> bool {
        false
    }

    /// The combat window's three read-back notices, as one snapshot. See
    /// [`CombatBar`], and `crate::hud::combat_window::CombatWindow::update`, which applies it.
    ///
    /// The default is the client's reset state, so a host that does not implement this
    /// leaves the window showing a medium attack height and a half-full notch, which is what a
    /// character who has just logged in shows.
    fn combat_bar(&self) -> CombatBar {
        CombatBar::default()
    }

    /// The player's advancement class for Recklessness, skill `0x32`. This alone
    /// decides whether the combat cluster draws its recklessness meter in melee.
    ///
    /// The default is 0 (`Undef`), below 2, so the meter stays hidden before the player
    /// description arrives.
    fn recklessness_advancement_class(&self) -> u32 {
        0
    }
}

/// A `GameView` that knows nothing, for constructing a screen with no world attached.
///
/// This is what the acceptance gate builds every panel against before it feeds one the frozen
/// snapshot: a panel that cannot be constructed against an empty world has state it should not
/// have.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyGameView;
impl GameView for EmptyGameView {}
