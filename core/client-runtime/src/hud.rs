//! The in-game HUD's host half: the three globals the gameplay screen's windows read.
//!
//! The HUD translates world state and UI actions into sixteen windows, vitals, placement,
//! the panel stack, chat routing through the final-string notice, and map/radar coordinates.
//!
//! **This module wires; it does not implement.** Everything the HUD *decides* is in
//! `dereth_ui_screens`: which windows are visible, what a meter's fill is, which chat window takes
//! a line, where a compass token sits. What this module owns is the three things the client reads
//! out of process globals and a rebuild has to hand over explicitly, exactly as
//! `crate::ui::HostState` does for the pre-game screens:
//!
//! 1. the player's module placement blob — read by each floating HUD window's placement update,
//!    plus `SideBySideVitals` and `LockUI`;
//! 2. the local player description — the qualities used for
//!    secondary-attribute queries, kept current by the `Qualities_*` event stream;
//! 3. the outbound final-string notice, which carries a final chat line into the UI.
//!
//! Every one of those arrives as a [`dereth_client_net::client_session::SessionEvent`]; this module decodes nothing that
//! [`dereth_protocol`] already decodes and computes nothing [`dereth_client_model`] already computes.
//!
//! # The two things it does compute, and why
//!
//! * `gid_to_lcoord` on the player's cell and then
//!   `(lcoord − 1024) × 0.1 + 0.5` per axis. The `gid_to_lcoord` half is the physics crate's
//!   ([`dereth_physics::landdefs`]); the two-line affine part belongs to player state and has no
//!   separate crate.
//! * the heading for the radar's compass, taken from the player's live frame.

mod chat;
mod events;
/// The property names inside one `Option_Placement` struct.
mod placement;
mod sync;
mod tables;
mod view;

use chat::{failure_line, fellowship_ui_line, speech, utc_offset_secs, wall_clock_unix};
pub use chat::{pk_death_filter, PkDeathLine, PK_DEATH_TAG};
pub use placement::{decode_chat_filters, decode_chat_opacity, decode_placements};
use tables::{component_categories, contract_location, contract_of, level_xp};

use dereth_assets::tables::{Attribute2ndTable, SkillTable, SpellTable};
use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::floaty::{WindowPlacement, WindowPlacements};
use dereth_client_contract::panels::characterinfo as charinfo;
use dereth_client_contract::panels::external_container::ExternalContainerNotice;
use dereth_client_contract::{
    GameView, RadarEntry, SelectionQueryFacts, SkillEntry, SpellEntry, Vital,
};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::login::PlayerModule;
use dereth_protocol::property::{BasePropertyValue, PropertyCollection};
use dereth_protocol::Message as _;
use dereth_rules::attributes::inq_attribute_2nd;

/// The `ObjectDescriptionFlag` bits and the `RadarEnum` values the radar reads, taken from
/// [`dereth_client_contract::radar`] rather than restated, so the seam and the rules that consume it can
/// never disagree about a mask. `dereth_ui_screens::mapradar::radar` re-exports them at their old
/// paths.
use dereth_client_contract::radar::bitfield as bits;
use dereth_client_contract::radar::radar_enum as radar_enum_value;

/// The panel driver lives in `crate::hud_drive`; the two fan-out helpers keep
/// their `crate::hud::…` path from here, because that is what `app.rs` and the test tree name.
pub use dereth_client_contract::panels::HudPanels;

/// A panel set with no panels: what the model runs with where no UI is attached (this crate's
/// own tests). Every receiver declines.
#[derive(Debug, Default)]
pub struct NoPanels;

impl HudPanels for NoPanels {
    fn spew_offer(
        &mut self,
        _ty: u8,
        _body: &str,
        _feedback: dereth_client_contract::feedback::Feedback,
    ) -> bool {
        false
    }
    fn spew_trace(&self) -> (bool, usize, u64) {
        (false, 0, 0)
    }
    fn spew_clear_pending(&mut self) {}
    fn abuse_response(&mut self, _code: u32) {}
}

/// `ITEM_TYPE::TYPE_CREATURE`, from the retail item-type enum. The creature predicate is
/// `item_type >> 4 & 1` on the object's type, i.e. this mask.
pub const ITEM_TYPE_CREATURE: u32 = 0x0000_0010;

/// `Attribute2ndTable`, `0x0E000003`. Its three `SkillFormula`s supply the base-level calculations
/// for MaxHealth, MaxStamina and MaxMana.
pub const ATTRIBUTE_2ND_TABLE: DataId = DataId(0x0E00_0003);
/// `SkillTable` — resolved by enum lookup `(4, 2, 0x10000004)`, walked by the skill panel,
/// and used for skill-name queries.
pub const SKILL_TABLE: DataId = DataId(0x0E00_0004);
/// `SpellTable` — the source for [`Hud::spells`], queried for every id returned by the
/// transcribed-spell list.
pub const SPELL_TABLE: DataId = DataId(0x0E00_000E);

/// `SpellComponentTable` — `0x0E00000F`, the SCID-to-`SpellComponentBase` hash used to classify
/// components and resolve their display names.
pub const SPELL_COMPONENT_TABLE: DataId = DataId(0x0E00_000F);

/// Contract table `0x0E00001D` — the **322** static contract descriptions joined with the live
/// tracker data.
///
/// Startup registers it in the object cache under `0x10000010`.
/// Readers resolve `(0x17, 2, 0x10000010)`; this build has no database cache to resolve that enum
/// through, so the file id is used directly — the same shortcut [`SKILL_TABLE`] and
/// [`SPELL_TABLE`] take.
///
/// This is `dereth_assets::tables::ContractTable`'s production loader; without it a contract's
/// name, description, NPCs and three positions are unreachable from the client.
pub const CONTRACT_TABLE: DataId = DataId(0x0E00_001D);

/// Experience table `0x0E000018` — the five XP curves.
///
/// **Without it every footer cost is 0**, which reads as "at the cap" and disables both raise
/// buttons: the cost to raise is `experience_to_skill_level(sac, level + 1) - _pp` and
/// there is nothing else it could be. A miss is reported, not fatal, like the other three.
pub const XP_TABLE: DataId = DataId(0x0E00_0018);

/// `PropertyInt` **24 `AvailableSkillCredits`** — the default footer's
/// integer-quality inquiry for `0x18`.
pub const AVAILABLE_SKILL_CREDITS: u32 = 0x18;

/// `PropertyInt64` **2 `AvailableExperience`** — the same function's int64 quality `2`, and the pot
/// both raise buttons are affordable against.
pub const AVAILABLE_EXPERIENCE: u32 = 2;

/// `PropertyInt64` **1 `TotalExperience`** — the experience read-out's int64 quality `1`.
pub const TOTAL_EXPERIENCE: u32 = 1;

/// `PropertyInt` **25 `Level`** — the same function's int quality `0x19`.
pub const LEVEL: u32 = 0x19;

/// `PropertyInt` **129 `VitaeCpPool`** — the experience already earned back against the current
/// vitae point.
///
/// It is also the quality the panel's
/// player-quality handler registration `(int stat type, 0x81)` watches, which is why the panel
/// redraws while the player earns. It is `PropertyInt` row 129.
pub const VITAE_CP_POOL: u32 = 0x81;

/// `PropertyInt` **139 `DeathLevel`** — the same function's int quality `0x8B`, and the level at
/// which the vitae recovery threshold is evaluated.
///
/// `Update` falls back to [`LEVEL`] when it is zero, which is the case for a character who has
/// never died — and also the case in every recorded session, none of which carries a vitae.
/// (Row 139 of the same table.)
pub const DEATH_LEVEL: u32 = 0x8B;

/// `PropertyInt` **181 `ChessRank`**. The query seeds its result with **1400**
/// (`0x578`), so an unranked character shows 1400 rather than 0.
pub const CHESS_RANK: u32 = 0xB5;
/// `PropertyInt` **192 `FakeFishingSkill`** — the same function's int quality `0xC0`, seeded 0.
pub const FAKE_FISHING_SKILL: u32 = 0xC0;
/// `PropertyInt` **43 `NumDeaths`** — the birth/age/deaths read-out's int quality `0x2B`.
pub const NUM_DEATHS: u32 = 0x2B;
/// `PropertyInt` **5 `EncumbranceVal`** — the numerator used to calculate the player's current
/// load.
pub const ENCUMBRANCE_VAL: u32 = 5;
/// `PropertyInt` **230 `AugmentationIncreasedCarryingCapacity`** — the same function's
/// int quality `0xE6`, and `EncumbranceCapacity`'s second argument.
pub const AUG_INCREASED_CARRYING_CAPACITY: u32 = 0xE6;

/// `PropertyString` **1 `Name`** — the character's own name, carried by `0x0013` and therefore
/// available before `0xF745` creates the player object.
pub const CHARACTER_NAME: u32 = 1;

// ---- the three header fields -----------------------------------------------------------------

/// `PropertyInt` **113 `Gender`** — the character-info read-out's int quality `0x71`.
///
/// **`0x71` is `Gender`, not `HeritageGroup`.** The two reads in
/// the character-info read-out cannot be told apart, but the appraisal path makes the same
/// call and assigns int quality `0x71` to gender and `0xBC` to heritage just before
/// asking for the gender/heritage display text from gender, heritage and creature type. The
/// property table agrees (rows 113 and 188).
pub const GENDER: u32 = 0x71;

/// `PropertyInt` **188 `HeritageGroup`** — the same function's int quality `0xBC`.
pub const HERITAGE_GROUP: u32 = 0xBC;

/// `PropertyInt64` **6 `AvailableLuminance`** — the experience read-out's int64 quality `6`,
/// the **first** of the two numbers the luminance text shows.
pub const AVAILABLE_LUMINANCE: u32 = 6;
/// `PropertyInt64` **7 `MaximumLuminance`** — int64 quality `7`, the second one, and also the value
/// whose being zero closes the whole luminance arm.
pub const MAXIMUM_LUMINANCE: u32 = 7;

/// The gender display lookup reads this mapper as table enum `0x10000001`.
/// Its rows are `Invalid`, `Male`, and `Female`.
pub const GENDER_ENUM_MAPPER: DataId = DataId(0x2200_000A);

/// The heritage-group display lookup reads this mapper as table enum `0x10000002`.
pub const HERITAGE_ENUM_MAPPER: DataId = DataId(0x2200_000B);

/// The creature-type mapper selected by table enum `0x10000005` for appraisal.
pub const CREATURE_TYPE_ENUM_MAPPER: DataId = DataId(0x2200_000E);

/// The character-title mapper selected by table enum `0x10000006`.
/// Its rows contain tokens to hash into the title string table.
pub const TITLE_ENUM_MAPPER: DataId = DataId(0x2200_0041);

/// The title string table selected by table enum `0x10000007`.
pub const TITLE_STRING_TABLE: DataId = DataId(0x2300_000E);

/// The material-name mapper used by salvage reports.
/// Names replace underscores with spaces before display.
/// This is a dual mapper; the group-1 enum lookup instead selects the gender mapper.
pub const MATERIAL_TYPE_NAMES: DataId = DataId(0x2700_0000);

/// Resolves one material name without imposing either caller's miss behavior. A missing mapper or
/// row returns `None`; a hit replaces underscores with spaces.
///
/// **The two callers answer a miss differently and it matters.**
/// The salvaged-materials text pre-loads `"Unknown"` and keeps it;
/// the inventory display path **drops the bool** and leaves the material name as the null
/// string, so its miss reads `" Salvage (100)"` with a leading space. `None` here is that fact,
/// and each caller spells its own answer -- a single `String` return would have silently given one
/// of them the other's.
#[must_use]
pub fn material_name_of(names: Option<&dereth_assets::DidMapper>, material: u32) -> Option<String> {
    names
        .and_then(|t| {
            t.enum_to_name
                .iter()
                .find(|(k, _)| *k == material)
                .map(|(_, v)| v)
        })
        .map(|v| v.replace('_', " "))
}

/// One object's composed display name, cached because the seam it is read through hands out a
/// `&str`.
///
/// The three inputs are kept beside the two answers so [`Hud::refresh_display_names`] can skip an
/// object nothing moved on: the composition allocates, and a frame that changed nothing must not.
/// That is the same shape `dereth_ui_screens::items::widget::TileInfo` uses for a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName {
    /// `pwd._material_type` this was composed from.
    material: u32,
    /// `pwd._name`.
    raw: String,
    /// `pwd._plural_name`.
    raw_plural: Option<String>,
    /// The object-name query's singular form.
    pub singular: String,
    /// The object-name query's plural form, and `None` when the object has no `_plural_name` --
    /// which is the condition `GameView::plural_name` has always answered `None` for, kept exactly
    /// so the widget's stack-name fallback does not move.
    pub plural: Option<String>,
}

impl DisplayName {
    /// Whether the three inputs still read the same, i.e. whether the cached answer stands.
    fn matches(&self, w: &dereth_client_model::weenie::Weenie, material: u32) -> bool {
        self.material == material
            && self.raw == w.pwd.name
            && self.raw_plural.as_deref() == w.pwd.plural_name.as_deref()
    }

    fn compose(w: &dereth_client_model::weenie::Weenie, material: u32, name: Option<&str>) -> Self {
        use dereth_client_model::weenie::NameType;
        Self {
            material,
            raw: w.pwd.name.clone(),
            raw_plural: w.pwd.plural_name.clone(),
            singular: w.display_name(NameType::Singular, name),
            plural: w
                .pwd
                .plural_name
                .as_deref()
                .filter(|p| !p.is_empty())
                .map(|_| w.display_name(NameType::Plural, name)),
        }
    }

    /// Whether an object needs an entry at all -- the two arms of the object-name query that can
    /// change the string. Everything else reads `pwd._name` straight off the weenie and allocates
    /// nothing.
    fn wanted(w: &dereth_client_model::weenie::Weenie) -> bool {
        w.pwd.material_type.unwrap_or(0) as i32 > 0
            || (w.pwd.bitfield & dereth_rules::weenie::bitfield::HIDDEN_ADMIN != 0
                && w.pwd.name.starts_with('+'))
    }
}

/// The two character-option bits this module reads.
///
/// The two accessors establish both masks and
/// their storage words: **`LockUI` is `options2_` bit 24 and `SideBySideVitals` is `options_` bit
/// 21**. The latter is followed by bits 22 and 23 for its two neighboring player options. Both
/// are verified against retail.
///
/// # `SideBySideVitals` must be `Some`
///
/// With this constant `None`, *"Side By Side Vitals"* does nothing: every other link in the
/// chain is present, so the preference would be written, stored, sent to
/// the shard, put in [`AppliedKey`] and read by nobody.
///
/// The chain, each end checkable in one grep, all of it already present:
///
/// | end | code |
/// |---|---|
/// | the page writes | `options/character.rs` — `UiRequest::SetPlayerOption(SideBySideVitals, v)` |
/// | the word stores it | `dereth_client_model::player::PlayerSystem::set_option`, `options_` bit 21 |
/// | the pass re-seeds | [`Hud::applied_key`] / `PlayerSettingsView::side_by_side_vitals` |
/// | the screen applies | `dereth_ui_screens::hud::vitals::apply_side_by_side_option` |
///
/// # What retail does with it
///
/// The option-change handler special-cases only player option 19. It reads bit 21, then shows one
/// of `window::STACKED_VITALS` and `window::SIDE_VITALS` while hiding the other. Thus *"Side By
/// Side Vitals"* selects between the stacked three-bar strip and the wide side-by-side window.
///
/// The player-option change handler first sends the option-change notice with `(option)` — before the
/// per-option dispatch and before the auto-save-option split. This build's equivalent is the
/// [`AppliedKey`] edge, which is retail's other producer:
/// the gameplay UI's player-description receiver.
pub mod character_option {
    /// `SideBySideVitals` — `options_` bit 21.
    ///
    /// See this module's note for what it does.
    pub const SIDE_BY_SIDE_VITALS: Option<u32> = Some(0x0020_0000);
    /// `LockUI` — `options2_` bit 24.
    pub const LOCK_UI: Option<u32> = Some(0x0100_0000);
}

/// `PlayerOption` — the view's enum on one side of the seam, the retail ordinal on the other.
///
/// The Character Options page names its rows with [`dereth_client_contract::PlayerOption`], a
/// `dereth-ui-screens` enum in the page's own row order.
/// `dereth_client_model::player::options::PLAYER_OPTIONS` is indexed by the **retail `PlayerOption`
/// ordinal**, which is a different order entirely and is what
/// the option accessors switch on. This is the bridge,
/// and it is the only one: no bit mask is repeated on this side of the seam, so a wrong ordinal
/// here is the only way a row can edit the wrong bit.
///
/// The numbers are the retail `PlayerOption` ordinals, transcribed as literals.
/// A test cross-checks every one of them against `PLAYER_OPTIONS`'s independently
/// written name table and pins a sample as literals, because a bridge checked only through the
/// symbols it is built from is unfalsifiable.
///
/// **One of retail's 53 has no variant**: `UseMouseTurning` (49). Two more are variants that are
/// **not** `InitOptions` rows — `LockUI` (51), set from the radar's padlock, and `AppearOffline`
/// (39), bound by the Friends page's one check box; without that variant the Friends tab's box
/// would edit nothing.
#[must_use]
pub const fn option_ordinal(o: dereth_client_contract::PlayerOption) -> usize {
    use dereth_client_contract::PlayerOption as P;
    match o {
        P::AutoRepeatAttack => 0,
        P::IgnoreAllegianceRequests => 1,
        P::IgnoreFellowshipRequests => 2,
        P::IgnoreTradeRequests => 3,
        P::DisableMostWeatherEffects => 4,
        P::PersistentAtDay => 5,
        P::AllowGive => 6,
        P::ViewCombatTarget => 7,
        P::ShowTooltips => 8,
        P::UseDeception => 9,
        P::ToggleRun => 10,
        P::StayInChatMode => 11,
        P::AdvancedCombatUI => 12,
        P::AutoTarget => 13,
        P::VividTargetingIndicator => 14,
        P::FellowshipShareXP => 15,
        P::AcceptLootPermits => 16,
        P::FellowshipShareLoot => 17,
        P::FellowshipAutoAcceptRequests => 18,
        P::SideBySideVitals => 19,
        P::CoordinatesOnRadar => 20,
        P::SpellDuration => 21,
        P::DisableHouseRestrictionEffects => 22,
        P::DragItemOnPlayerOpensSecureTrade => 23,
        P::DisplayAllegianceLogonNotifications => 24,
        P::UseChargeAttack => 25,
        P::UseCraftSuccessDialog => 26,
        P::HearAllegianceChat => 27,
        P::DisplayDateOfBirth => 28,
        P::DisplayAge => 29,
        P::DisplayChessRank => 30,
        P::DisplayFishingSkill => 31,
        P::DisplayNumberDeaths => 32,
        P::DisplayTimeStamps => 33,
        P::SalvageMultiple => 34,
        P::HearGeneralChat => 35,
        P::HearTradeChat => 36,
        P::HearLFGChat => 37,
        P::HearRoleplayChat => 38,
        // The client passes `0x27`. Not an `InitOptions` row; `PLAYER_OPTIONS[39]` is
        // `("AppearOffline", OptionWord::Two, 0x0000_1000)` and ordinal 39 is one of the twenty-one
        // auto-save option ordinals, so a tick here leaves on a `0x0005`.
        P::AppearOffline => 39,
        P::DisplayNumberCharacterTitles => 40,
        P::MainPackPreferred => 41,
        P::LeadMissileTargets => 42,
        P::UseFastMissiles => 43,
        P::FilterLanguage => 44,
        P::ConfirmVolatileRareUse => 45,
        P::HearSocietyChat => 46,
        P::ShowHelm => 47,
        P::DisableDistanceFog => 48,
        P::ShowCloak => 50,
        P::LockUI => 51,
        P::HearPKDeaths => 52,
    }
}

/// Query a player option, with the **third state** that the trait's
/// `bool` cannot carry.
///
/// `None` means no `0x0013` has landed, so there is no `PlayerModule` to read and every answer
/// would be the default character-option word's rather than this character's. That is exactly the
/// ambiguity `CharacterSettingsPage::values_seen` was written to expose from the other side: a
/// page of unticked rows is either a character with nothing set or a seam with no producer, and
/// only a three-state accessor can tell them apart.
///
/// The word read is [`dereth_client_model::player::PlayerSystem::options`], which
/// `apply_player_module` fills from the same `0x0013` blob and `set_option` read-modify-writes.
#[must_use]
pub fn character_option(
    world: &dereth_client_model::World,
    o: dereth_client_contract::PlayerOption,
) -> Option<bool> {
    world.player_system.module.as_ref()?;
    Some(world.player_system.options.get(option_ordinal(o)))
}

pub use crate::stats::HudStats;

/// The inputs for which the screen's `update_from_player_module` pass last ran.
///
/// # What this key is a superset OF
///
/// The gated work builds a `PlayerSettingsView` out of exactly three reads: [`Hud::placements`],
/// `option_bit(SIDE_BY_SIDE_VITALS)` (which is bit 21) and
/// [`Hud::lock_ui`] (`options2_` bit 24). Two of the three are in this key, and the third is
/// excluded on purpose — see below.
///
/// A `screen_serial`-only key would not be a superset of anything: the containment would live
/// outside the key, in hand-clears at the writers — a protocol between four sites rather than
/// a key, and exactly what goes stale when somebody adds a fifth writer. The values are in the
/// key, so the containment is structural. The two hand-clears are **kept** (`placements` and
/// `player_module` are written in the `0x0013` arm and reset at session end, and both of those
/// sites also set `applied_to = None`): they are the client's own
/// player-description-received edge, and a description that re-states the same values still
/// has to re-push them.
///
/// **`side_by_side_vitals` is in the key, and it is a live value.** A `screen_serial`-only
/// latch would freeze `side_by_side_vitals` at whatever it was when the latch was last cleared,
/// and ticking the row in the options page would move the word, send the `0x0005`, and change
/// nothing on screen until the next screen rebuild. A test drives [`Hud::applied_key`] with the
/// bit **forced** and with the constant itself.
///
/// **And its source is the model's option word.** [`Hud::player_module`]`.options` is only
/// written by the `0x0013` arm, while `dereth_client_model::player::PlayerSystem::options` is
/// what [`character_option`](fn@character_option) reads and what `PlayerSystem::set_option` read-modify-writes from
/// the Character Options page (`interaction.rs`'s `UiRequest::SetPlayerOption` arm). A player
/// ticking Side-by-Side Vitals moves the **second** copy only, so reading the first would
/// answer the login blob's value for the rest of the session. [`Hud::option_bit`] reads the
/// `dereth_client_model` copy, so there is **one** character-option word and this key is a
/// superset of it. See that method.
///
/// # `lock_ui` is deliberately NOT in this key
///
/// The padlock's visible mirror flows **screen → HUD**: `Hud::set_lock_ui` runs *after* this pass,
/// precisely so a toggle frame's mirror is not read back over by a re-seed. The authoritative
/// `PlayerSystem::set_option(51, ..)` write happens at the input/request boundary. Putting
/// `lock_ui` in the key would make the frame *after* every toggle a re-seed frame, and
/// `update_from_player_module` also applies the **window placements** — so a padlock click would
/// re-assert every window's stored visibility over whatever the player had since changed. The
/// exclusion is a declared behavioural decision, not an oversight, and it is the reason this key
/// is a superset of *what the module is the authority for* rather than of every field the pass
/// reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedKey {
    /// One construction of the element tree; a mode switch rebuilds it and re-seeds.
    screen_serial: u64,
    /// `Option_PlacementArray`, the pass's bulk payload.
    placements: WindowPlacements,
    /// See this type's own note.
    side_by_side_vitals: bool,
}

/// The host half of the HUD: the player module, the player's qualities and the chat
/// stream, plus the derived values the read-only [`GameView`] hands to the panels.
#[derive(Debug, Default)]
pub struct Hud {
    /// The player's qualities live in the object table's
    /// local-player row because that is the only place retail keeps them. Login creates a local
    /// player description for the player and no other object, then publishes that allocation as
    /// the player description. So the local player's object and "the object table's copy" are
    /// one thing.
    ///
    /// Whether the description has arrived is its own bit, exactly as it is in retail
    /// (set by the login description and read by the panels' own guards): the object row has empty
    /// qualities from `CreateObject` onwards, so an empty row does not mean "before `0x0013`".
    pub player_desc_received: bool,
    /// The UI time as of this frame's `Hud::drive`.
    ///
    /// The power-bar level is `(cur_time - build start time) / t`, and the combat window's
    /// meter needs it once a frame. `GameView` carries no clock (it is a snapshot), so the
    /// frame stamps it here on the way in, beside `link_status`, which is stamped for exactly the
    /// same reason.
    pub now: dereth_primitives::LocalTime,
    /// `0x0E000003`, loaded once. Without it the secondary-attribute query has no formula for a
    /// maximum.
    pub vitals_table: Option<Attribute2ndTable>,
    /// The player module from **the login blob, and not the option words.**
    ///
    /// This starts as a clone of the `PlayerModule` `0x0013` carried, kept for the **shortcut bar**
    /// (`GameView::shortcut`), the **spell filters** (`GameView::spell_filters`) and, through
    /// [`Hud::placements`], the window placements. Local numeric placement writes now mirror the
    /// retained PlayerSystem module's gameplay_options here; option words remain separate below.
    ///
    /// **Its `options` word has no reader.** The character-option word this client answers from is
    /// `dereth_client_model::player::PlayerSystem::options`, because that is the one
    /// `PlayerSystem::set_option` writes when the player ticks a row; reading `options` here
    /// answers login's value for the whole session. See [`Hud::option_bit`].
    ///
    /// **Its `options2` word has exactly one reader and one writer, and they are the same pair:**
    /// [`Hud::lock_ui`] / [`Hud::set_lock_ui`], the radar padlock's presentation mirror. Both the
    /// radar request and the lock-UI command call `PlayerSystem::set_option(51, ..)` first, which
    /// preserves neighboring bits, updates the retained module and emits native's immediate
    /// `0x0005`. See [`Hud::set_lock_ui`].
    pub player_module: Option<PlayerModule>,
    /// The gameplay-options collection's `Option_PlacementArray`, decoded.
    pub placements: WindowPlacements,
    /// Radar entries rebuilt each frame from the object table.
    pub radar: Vec<RadarEntry>,
    /// The visual environment-override flag, written by `Admin_Environs` options.
    /// The radar consumes this independently of its object snapshot; blanking therefore does not
    /// discard entries that must reappear when a later preset clears the flag.
    radar_blank: bool,
    /// The player id from `0xF746`, which arrives before the player's own `0xF745`.
    ///
    /// Kept here as well as in `dereth_client_model::World` because the vitals path reads the
    /// **local player description**, not the object table: the vitals bar is live
    /// from `0x0013` whether or not the player's physics object has been created yet.
    pub player: Option<ObjectId>,
    /// Last `0x0075 Character_StartBarber` payload. The Barber panel keeps its own private copy;
    /// this host-side slot is only the notice-to-screen seam.
    barber: Option<dereth_protocol::trade::BarberSettings>,
    /// Notice edge, because two byte-identical Starts are still two modal openings.
    barber_generation: u64,
    /// `None` until the player has a cell.
    pub coords: Option<(f32, f32)>,
    /// The journal path's three inputs, composed by `App::frame` out
    /// of the same three sources `auto_load_screen_layout` uses:
    /// the settings directory is `--prefs`' parent,
    /// the world name is `HostState::world_name` (`0xF7E1 Login_WorldInfo`) and the
    /// character is `HostState::entered_character`, which is set on all three log-on edges.
    /// `None` until all three exist, and the journal is then memory-only.
    pub journal_identity: Option<dereth_client_contract::journal::JournalIdentity>,
    /// The `(date, time-of-day name)` pair supplied by the simulation clock.
    ///
    /// Fed from `App::frame` out of `crate::world::WorldScene::game_date_time`, beside the
    /// `ViewerFrame` and for the same reason: the clock belongs to the scene, the panels read it
    /// through [`HudView`], and this is the one place both are in hand. Unlike [`Self::coords`] it
    /// is **not** derived from the player's cell and does not go away indoors — see
    /// [`dereth_client_contract::GameView::game_date_time`].
    pub game_date_time: Option<(String, String)>,
    /// The **player's** position cell id, not the camera's.
    ///
    /// The one input the outdoor-state query reads, and therefore the one input
    /// to the radar range. Kept beside [`Self::coords`] because both
    /// are derived from the same `ViewerFrame::position` in [`Self::sync`], and the coordinate
    /// read-out was already throwing the cell away after `player_coords` had used it.
    ///
    /// `None` is "no player", which the is-player-outside test answers with 0
    /// — **not outside** — so it selects the 25 arm. See [`Self::player_outside`].
    pub player_cell: Option<dereth_primitives::CellId>,
    /// Whether the player's physics object exists, not whether the player's id is nonzero.
    ///
    /// Three speech handlers open with this one test and draw **nothing at all** when it fails.
    /// Emote hearing does **not** have this gate; its first act is the `CanHear` call. That
    /// difference is the discriminating fact and is asserted by a test.
    ///
    /// **Why this field and not [`Self::player`] or [`Self::player_cell`].** Neither is this.
    /// `Hud::player` is the *sibling* field, and speech hearing reads **both** — the physics
    /// object for this gate and the id shortly after for the self-echo — so using the
    /// id here would collapse two different tests into one. `Hud::player_cell` is
    /// the player's position's `objcell_id`, derived from the object rather than being it, and it is a
    /// latch that survives the object going away.
    ///
    /// `App::frame` builds [`ViewerFrame`] from
    /// `WorldScene::character`, which is this client's one local `dereth_physics::PhysicsObj` and is
    /// therefore exactly the body whose presence this gate tests. [`Self::sync`] records whether
    /// it was there. It is
    /// **one frame old** for the same reason [`Self::speaker_player_space`] is — `sync` runs in
    /// step 7 and `apply_events` in step 3 — and it is `false` before the first `sync`, which is
    /// the state a `Hud` driven straight from a decoded message is in.
    pub player_body: bool,
    /// The player's heading in degrees.
    pub heading: f32,
    /// The final-string notice's queue. Displaying a final string reaches the chat interface during UI use
    /// time, which is step 7 and not step 4, so a
    /// line decoded while the network was pumped waits here until the UI ticks.
    pub pending_chat: Vec<(Option<u64>, ChatMessage)>,
    /// Incoming-only synchronous subscriber lifetime. No history for an absent/new screen.
    turbine_chat_generation: Option<u64>,
    /// What the placement blob was last pushed into the screen *for*. See [`AppliedKey`].
    pub applied_to: Option<AppliedKey>,

    // ---- connection status -------------------------------------------------------------------
    /// Connection status, read once per frame from
    /// [`crate::net::link_status_holder`] — the singleton kept in client state, whose only reader
    /// is the connection lamp.
    ///
    /// **The no-drop-kick clamp is not reproduced**: the connection-status query caps its answer at
    /// 15.0 s while that flag is set, and nothing in this rebuild models the no-drop kick. It is
    /// only observable between 15 s and 40 s of silence, where the lamp would show "poor" instead
    /// of "lost".
    pub link_status: Option<f64>,

    // ---- the paper doll ------------------------------------------------------------------------
    /// The inventory placements for the **player**, flattened to the
    /// `(iid, loc)` pairs the paper doll reads.
    ///
    /// Cached here rather than built in [`HudView::equipment`] because `GameView` hands out a
    /// borrowed slice and `ObjectInventory` stores a three-field struct. Refreshed once per frame
    /// in `Self::update_radar`'s pass, which is the one place the HUD already walks the object
    /// tables.
    pub equipment: Vec<(ObjectId, u32)>,

    // ---- the remaining panels -------------------------------------------------------------
    /// Skill table `0x0E000004`, loaded once. The Skills panel walks **this** and not the player's own
    /// skill hash, so without it the skills page has no rows at all — not even the four headers'
    /// worth of empty groups.
    pub skill_table: Option<SkillTable>,
    /// `SpellTable 0x0E00000E`, loaded once — the spell name, icon, school, display order and
    /// the formula the level heuristic reads.
    pub spell_table: Option<SpellTable>,
    /// Experience table `0x0E000018`, loaded once — every raise cost the footer shows.
    pub xp_table: Option<dereth_assets::tables::XpTable>,
    /// What the world's era means for what the front ends can show, filled as the tables load
    /// and when the character list names the account's Throne of Destiny flag.
    pub era: dereth_client_contract::EraView,
    /// Contract table `0x0E00001D`, loaded once — the 322 contract descriptions.
    ///
    /// The Contracts panel walks the **tracker** table and looks each id up in **this**, so without it
    /// the Contracts tab has no rows at all, whatever the shard has sent: the status child is
    /// written only when the contract lookup resolves.
    pub contract_table: Option<dereth_assets::tables::ContractTable>,

    // ---- the two dat objects the component tracker resolves through --------------------------
    /// Component definitions joined to the WCID-to-SCID mapping, loaded once and pushed into
    /// `.catalogue`. Empty when the dats are absent, which makes every category
    /// `Undef` and every component name empty — the client's own no-table arm.
    pub component_catalogue: dereth_client_model::magic::ComponentCatalogue,
    /// How many `0x27xxxxxx` mapping tables were examined to find the component mapping, and how
    /// many of its entries matched a component-definition SCID. A denominator, so a catalogue
    /// that comes back empty can say **why**.
    pub component_mapper_scan: (usize, usize),

    /// The trade-note enumeration: `(face value, WCID)`.
    /// Resolved by the literal native enum group/value and decoded with `DB_TYPE 0x28`, so neither
    /// the mapper DataID nor any note WCID is pinned in code.
    pub trade_note_values: Vec<(u32, u32)>,

    /// The spell-component table's school/foci mapping:
    /// `(school, foci WCID)`.
    ///
    /// Pushed into `.school_pack_wcid`, whose values school-pack selection compares
    /// against each of the player's side packs.
    /// Without it the map is empty, `school_of_magic_to_wcid` answers `INVALID_DID`
    /// for every school, and the **foci half** of `get_appropriate_spell_formula` can
    /// never fire — so a character carrying a Foci would be shown, and charged, the full formula
    /// instead of scarab + prismatic tapers. Resolved by the enum walk, never by a literal id.
    pub school_pack_wcid: Vec<(u32, u32)>,

    /// `QualityFilter` — the table selected by enum lookup `(0x10000002, 3, 0x1000000C)`.
    /// Integer and float enchantment queries ask
    /// the filter before they will enchant a property at all.
    pub quality_filter: Option<dereth_assets::tables::QualityFilter>,

    // ---- the header's three remaining sources ------------------------------------------------
    /// `EnumMapper 0x2200000A` — the gender names.
    pub gender_names: Option<dereth_assets::tables::EnumMapper>,
    /// `EnumMapper 0x2200000B` — the heritage-group names.
    pub heritage_names: Option<dereth_assets::tables::EnumMapper>,
    /// `EnumMapper 0x22000041` — the `ID_CharacterTitle_*` tokens.
    pub title_tokens: Option<dereth_assets::tables::EnumMapper>,
    /// `EnumMapper 0x2200000E` — the `CreatureType` names, for the identify panel.
    pub creature_type_names: Option<dereth_assets::tables::EnumMapper>,
    /// `StringTable 0x2300000E` — those tokens' display strings.
    pub title_strings: Option<dereth_assets::ui::StringTable>,
    /// [`MATERIAL_TYPE_NAMES`] — the salvage report's material names.
    pub material_names: Option<dereth_assets::DidMapper>,
    /// The composed display name per object, for objects
    /// where it differs from `pwd._name` — see [`DisplayName`] and
    /// [`Self::refresh_display_names`].
    ///
    /// The client recomposes on every call; this build caches because `GameView::name` answers a
    /// `&str` and the prefix needs the dats, which live on this side of the seam. Only objects
    /// [`DisplayName::wanted`] accepts get a row, so an inventory of ordinary items holds none.
    pub display_names: std::collections::HashMap<ObjectId, DisplayName>,
    /// Character titles from `0x0029 Social_CharacterTitleTable` and
    /// `0x002B Social_AddOrSetCharacterTitle`.
    ///
    /// The displayed title is written by two notices: the full-table update copies its display-title
    /// field, while the single-title update takes the id directly. Both then refresh the character
    /// information. 0 is "no title", so title lookup rejects it outright.
    pub display_title: u32,
    /// Skill-list rows, rebuilt when `0x0013` or a skill update
    /// changes something. Cached because `GameView` hands out a borrowed slice.
    pub skills: Vec<SkillEntry>,
    /// The player's spellbook joined to the `SpellTable`, same reason.
    pub spells: Vec<SpellEntry>,
    /// App's one-frame transit, discarded if the subscriber dies before delivery.
    pub pending_external_container: Vec<ExternalContainerNotice>,
    /// The same transit for the Salvage panel's three notices.
    pub pending_salvage: Vec<dereth_client_contract::panels::salvage::SalvageNotice>,
    /// The visibility this frame's ENV-page delivery last reported, per page, so the host is told
    /// on an **edge** rather than re-told every frame.
    ///
    /// Retail sends `SetPanelVisibility` from the child's visibility *callback*
    /// (the floaty ENV panel's listener), which by construction fires only when
    /// the flag changes. Polling would not be harmless with more than one page,
    /// because `recv_env_panel_visibility` emits a `SetPanelVisibility` for the page it displaces,
    /// and two pages re-asserting themselves every frame displace each other forever.
    ///
    /// There are three pages. The Salvage panel is the Environment panel stack's second page
    /// (`0x1000005E`), so a window that showed itself with no entry here would open **behind the
    /// hidden `<ENVP>` host**, with `UiSystem::is_visible` false while the panel's own flag is
    /// already `true`.
    pub env_page_visibility: [Option<bool>; 4],
    /// The `screen_serial` the front end's panels were bound against, so a mode switch that rebuilt the
    /// element tree re-binds instead of writing into freed handles.
    pub panels_bound_to: Option<u64>,

    // ---- quality updates ---------------------------------------------------------------------
    // There is no stamper here. Every `Qualities_*Update*` carries an 8-bit
    // per-`(StatType, propertyId)` sequence and the stat update gates on it *before*
    // writing — on the object's stamper, whose counter set is keyed by
    // `prop | 0x10000`. The private and the public form of one property share that one counter,
    // which is a fact two stores cannot reproduce: with a stamper each, a stale public update
    // would be accepted by one after the other had already superseded it.
    // `dereth_client_model::Weenie::setup_stamper` is the only one.
    /// Set when a quality landed that the stat-management panel redraws itself for, cleared by
    /// `Self::drive`. See [`dereth_client_model::qualities::update::answers_a_raise`].
    pub raise_answered: bool,

    /// **A diagnostic, not a client feature.** The last raise-trace watch
    /// line `crate::trace::plus_ten_lines` printed, so `Self::drive` logs the `+10` button
    /// on change rather than every frame.
    pub trace_plus_ten_last: Option<String>,

    /// The `(1, 10)` draw that picks which of
    /// [`dereth_client_model::chat::composition::OLTHOI_TEXT`] / [`dereth_client_model::chat::composition::HUMAN_TEXT`] a garbled line shows.
    ///
    /// `None` until the first garbled line, then seeded from [`crate::audio::ran2_seed`] — which
    /// is `(long)time(NULL)`, the same value seeds the random generator.
    ///
    /// Retail has **one** random generator for the whole
    /// process, shared with ambient-sound timing and character generation, so the exact phrase a
    /// garbled line draws depends on every other draw the session has made. This build has no
    /// such singleton (`dereth_primitives::num::rng` exists precisely so the two generators are never merged),
    /// so the chat path keeps its own. The observable difference is **which** of the ten phrases
    /// appears, never whether a line garbles, never the two tables' contents, and never the
    /// composition — see [`Self::garble_roll`], which is the only reader.
    garble_rng: Option<dereth_primitives::num::rng::Ran2>,

    pub stats: HudStats,
}

impl Hud {
    /// Mirrors the visual `Admin_Environs` handler's write to the radar-blank flag.
    pub fn set_admin_radar_blank(&mut self, blank: bool) {
        self.radar_blank = blank;
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The camera-side facts `WorldObjects` supplies once per frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewerFrame {
    /// The object's position — its cell and block-local frame.
    pub position: dereth_primitives::Position,
    /// The player's heading, in degrees.
    pub heading_degrees: f32,
}

/// The two channel bits for which the `Communication_ChannelBroadcast` handler remembers a
/// speaker; only these two store a user name.
pub mod channel {
    /// `0x00002000` `Patron` — `@patron`, `@p`. Reply key `0x10000021`, alias `pr `.
    pub const PATRON: u32 = 0x0000_2000;
    /// `0x00004000` `Monarch` — `@monarch`, `@m`. Reply key `0x10000020`, alias `mr `.
    pub const MONARCH: u32 = 0x0000_4000;
}

/// `BooleanPropertyID` values this file asks for by literal.
pub mod bool_property {
    /// `0x81` = 129, `NoOlthoiTalk`
    /// in the boolean-property table. Pushed as a literal
    /// inside the emote language branch: when the local player description has it set, the emote
    /// is composed with its own words whatever the `^`/`&`
    /// marker and the is-Olthoi test say.
    ///
    /// Written here as a number rather than reached through a symbol so that a transcription
    /// error in it has an independent literal to disagree with.
    pub const NO_OLTHOI_TALK: u32 = 0x81;
}

/// House-location conversion checks position validity, obtains an outside cell id, and converts it
/// with `gid_to_lcoord`.
///
/// `get_outside_cell_id` is on a copy, and it
/// is deliberately unconditional here: a house's `Position` is the dwelling's own cell, which for
/// a cottage or villa is an *interior* one, and "which land cell is this building standing in" is
/// exactly what this section wants. That is the opposite of teleport, where
/// the same call has to be gated on `is_outdoors` — and it is the same call, so the difference is
/// recorded here rather than looking like an oversight.
#[must_use]
fn house_lcoord(p: dereth_protocol::types::PositionWire) -> Option<(i32, i32)> {
    let pos: dereth_primitives::Position = p.into();
    if !dereth_physics::math::position_is_valid(&pos) {
        return None;
    }
    let outside = dereth_physics::landdefs::get_outside_cell_id(pos.cell, pos.frame.origin);
    dereth_physics::landdefs::gid_to_lcoord(outside)
}

/// Convert a cell id to landscape coordinates: north is `(ly - 1024) * 0.1 + 0.5` and east
/// is `(lx - 1024) * 0.1 + 0.5`.
///
/// The subtraction is a plain integer `- 0x400`. Which of the two goes
/// first is settled by the live run: over Holtburg (`0xA9B4`, block 169/180) the retail client shows
/// **42.2N, 33.8E**, and only `north` from `ly` produces 42.
///
/// Returns `None` for a cell id `gid_to_lcoord` rejects, which is the player-coordinates query returning false
/// and the radar hiding its coordinate elements.
#[must_use]
pub fn player_coords(cell: dereth_primitives::CellId) -> Option<(f32, f32)> {
    let (lx, ly) = dereth_physics::landdefs::gid_to_lcoord(cell)?;
    #[allow(clippy::cast_precision_loss)] // both are 0..0x7F8; exact in f32
    Some((
        (ly - 1024) as f32 * 0.1 + 0.5,
        (lx - 1024) as f32 * 0.1 + 0.5,
    ))
}

/// What a front end keeps beside the HUD model: the model itself, and the panels the model offers
/// lines and answers to as events land ([`HudPanels`]), lent for the length of a call.
pub trait HudSlot: std::ops::DerefMut<Target = Hud> {
    /// The model and the front end's receivers, both borrowed.
    fn split(&mut self) -> (&mut Hud, &mut dyn HudPanels);
}

/// The read-only view the HUD panels read. Every method is one client accessor.
#[derive(Debug)]
pub struct HudView<'a> {
    pub hud: &'a Hud,
    pub world: &'a dereth_client_model::World,
}

/// The integer-quality query's presence result, rather than its numeric value: `None` when absent.
///
/// [`dereth_client_model::qualities::Qualities::inq_int`] folds absent into `0`, which is the shape
/// almost every call site wants and is exactly wrong for the two character-pane lines that
/// branch on the return: a
/// character with no `CreationTimestamp` shows **no** birth line, and one stamped at zero
/// shows the epoch.
fn int_opt(q: &dereth_client_model::qualities::Qualities, property: u32) -> Option<i32> {
    q.ints.as_ref().and_then(|h| h.get(&property)).copied()
}

#[cfg(test)]
mod tests;
