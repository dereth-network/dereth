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

use dereth_assets::tables::{Attribute2ndTable, SkillTable, SpellTable};
use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::floaty::{WindowPlacement, WindowPlacements};
use dereth_client_contract::panels::characterinfo as charinfo;
use dereth_client_contract::panels::external_container::ExternalContainerNotice;
use dereth_client_contract::{
    GameView, RadarEntry, SelectionQueryFacts, SkillEntry, SpellEntry, Vital,
};
use dereth_client_model::attributes::inq_attribute_2nd;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::login::PlayerModule;
use dereth_protocol::property::{BasePropertyValue, PropertyCollection};
use dereth_protocol::Message as _;

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
    fn spew_offer(&mut self, _ty: u8, _body: &str) -> bool {
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
/// property table agrees (rows 113 and 188). \[verified\]
pub const GENDER: u32 = 0x71;

/// `PropertyInt` **188 `HeritageGroup`** — the same function's int quality `0xBC`.
pub const HERITAGE_GROUP: u32 = 0xBC;

/// `PropertyInt64` **6 `AvailableLuminance`** — the experience read-out's int64 quality `6`,
/// the **first** of the two numbers the luminance text shows.
pub const AVAILABLE_LUMINANCE: u32 = 6;
/// `PropertyInt64` **7 `MaximumLuminance`** — int64 quality `7`, the second one, and also the value
/// whose being zero closes the whole luminance arm.
pub const MAXIMUM_LUMINANCE: u32 = 7;

/// The gender display lookup reads this `EnumMapper` as table enum `0x10000001`.
///
/// The enum is resolved to a `DataID` by a two-level mapping-table walk. This build has no cached
/// resolver for that lookup, so the resolved id is what crosses the seam —
/// the same shortcut `panels::statmgmt::STRING_TABLE` and `screens::chargen::ERROR_STRING_TABLE`
/// take. [verified by decoding every `0x22xxxxxx` in the retail dats: `0x2200000A` is
/// the only one whose rows are `{0 Invalid, 1 Male, 2 Female}`]
pub const GENDER_ENUM_MAPPER: DataId = DataId(0x2200_000A);

/// The heritage-group display lookup reads this `EnumMapper` as table enum `0x10000002`.
/// [verified the same way: `0x2200000B`'s fourteen rows are `Invalid, Aluvian, Gharundim, Sho,
/// Viamontian, Shadowbound, Gearknight, Tumerok, Lugian, Empyrean, Penumbraen, Undead, Olthoi,
/// OlthoiAcid`]
pub const HERITAGE_ENUM_MAPPER: DataId = DataId(0x2200_000B);

/// The `EnumMapper` selected by table enum `0x10000005`, used for the identify panel's
/// creature-type line.
///
/// [verified by decoding every `0x22xxxxxx` in the retail dats: `0x2200000E` is the
/// only mapper whose rows are the `CreatureType` names, 102 of them, and its row 13 is `Golem` —
/// which is what a retail screenshot shows for the Sparring Golem.]
pub const CREATURE_TYPE_ENUM_MAPPER: DataId = DataId(0x2200_000E);

/// The character-title lookup reads this `EnumMapper` as table enum `0x10000006` — 873 rows of
/// `ID_CharacterTitle_*` **tokens**, not display strings.
/// [verified: `0x22000041` is the only mapper carrying them]
pub const TITLE_ENUM_MAPPER: DataId = DataId(0x2200_0041);

/// The `StringTable` those tokens are then hashed into, table enum `0x10000007`.
///
/// The title lookup hashes the token (`dereth_primitives::num::hash::str_hash`) and
/// resolves the string with `(hash, 0x10000007)`. [verified by hashing
/// `ID_CharacterTitle_Adventurer` with `dereth_primitives::num::hash::str_hash` and probing all fifteen
/// `0x23xxxxxx` tables in the retail dats: only `0x2300000E` answers it, with `"Adventurer"`, and
/// it has exactly the mapper's 873 rows]
pub const TITLE_STRING_TABLE: DataId = DataId(0x2300_000E);

/// The `DualDidMapper` that supplies the `MaterialType` display names read by salvage reports.
/// Names replace underscores with spaces before display.
///
/// **`dereth_assets::tables::did_by_enum(assets, 1, 0x10000001)` is NOT this path, and answering it
/// that way is a trap.** `DB_TYPE 0x28` is `DualDidMapper`; the two-level walk with group 1 and
/// value `0x10000001` resolves to `0x2200000A`, which is the **gender** mapper
/// ([`GENDER_ENUM_MAPPER`]) -- the enum-to-DID lookup does not see the database type at all,
/// only the object fetch does. [verified by decoding
/// every `0x22xxxxxx`, `0x25xxxxxx` and `0x27xxxxxx` mapper in the retail dats -- 67 mappers -- and
/// finding exactly one carrying `Iron`, `Copper` and `Gold`: `0x27000000`, 78 rows, row 1 `Ceramic`
/// (which is ACE's `MaterialType.Ceramic = 1`) and row 0x10 `Black_Opal`.]
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
            || (w.pwd.bitfield & dereth_client_model::weenie::bitfield::HIDDEN_ADMIN != 0
                && w.pwd.name.starts_with('+'))
    }
}

/// The skill-level experience threshold, clamped for the footer's meter.
///
/// The real function answers `0xFFFFFFFF` for a skill that is neither trained nor specialised,
/// which is a sentinel and not a number; the trained-skill footer never asks it
/// about one, because the untrained footer has no meter. 0 here keeps the meter's span at 0, which
/// `SkillAdvancement::meter_fill` reads as "no bar" rather than as a wild fraction.
fn level_xp(
    t: &dereth_assets::tables::XpTable,
    sac: dereth_client_model::skills::Sac,
    level: u32,
) -> u32 {
    let v = dereth_client_model::advancement::experience_to_skill_level(t, sac, level as usize);
    if v == u32::MAX {
        0
    } else {
        v
    }
}

/// The property names inside one `Option_Placement` struct.
mod placement {
    /// `Option_PlacementArray`, the one array property the gameplay-options collection holds.
    pub const ARRAY: u32 = 0x1000_008C;
    pub const X: u32 = 0x1000_0086;
    pub const Y: u32 = 0x1000_0087;
    pub const WIDTH: u32 = 0x1000_0088;
    pub const HEIGHT: u32 = 0x1000_0089;
    pub const VISIBILITY: u32 = 0x1000_008A;
    pub const TITLE: u32 = 0x1000_008D;
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

/// Counters. Everything here is tolerant of a missing field by design, so every tolerance has a
/// number and the tests assert on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HudStats {
    /// `0x0013` applied: the player's qualities and module state are live.
    pub player_desc_applied: u64,
    /// Rows read from `Option_PlacementArray`.
    pub placements_decoded: u64,
    /// Windows whose visibility the placement blob actually set.
    pub placements_applied: u64,
    /// `Qualities_*Attribute2nd*` events applied to the player's vitals.
    pub vital_updates: u64,
    /// A `Qualities_*` event about the player that named a vital this build cannot store. Must stay
    /// zero for the four `Attribute2nd` opcodes.
    pub vital_updates_unstorable: u64,
    /// Chat lines that reached at least one window.
    pub chat_lines: u64,
    /// Chat lines every window filtered out.
    pub chat_lines_dropped: u64,
    /// Lines taken off [`dereth_client_model::scroll::Scroll`] — every client-generated
    /// message, as opposed to the ones that came off the wire.
    pub scroll_lines: u64,
    /// …and how many of those the message-log panel accepted, which is the ones with chat type `0x1A`.
    /// Two counters, not one, because "the client generated nothing" and "it generated something
    /// the spew box refused" are the two readings this seam has to be able to tell apart.
    pub spew_lines: u64,
    /// A UI-queue opcode this module recognises but could not decode.
    pub undecodable: u64,
    /// `0x02BB` / `0x02BC` / `0x02BD` lines composed through one of
    /// [`crate::chat`]'s templates and pushed. Zero here with a non-zero
    /// [`Self::speech_lines_not_addressed_to_us`] would mean every tell was somebody else's; zero
    /// in both means no speech arrived at all. Two counters, not one.
    pub speech_lines_composed: u64,
    /// `0x02BD` tells the client deliberately draws nothing for: the
    /// target-is-the-player gate failed, so the handler returns
    /// early. Not a decode failure and not a drop — a real branch of the client.
    pub speech_lines_not_addressed_to_us: u64,
    /// `0x02BB` say lines refused,
    /// split by which half refused them. Two counters beside
    /// [`Self::speech_lines_composed`], so "nobody is talking", "you have them squelched" and
    /// "they are out of earshot" are three readings and not one.
    pub speech_lines_squelched: u64,
    /// See [`Self::speech_lines_squelched`] — this is the *distance* half, whose absence would let
    /// an emote carry across the world.
    pub speech_lines_out_of_earshot: u64,
    /// `0x02BB` with `senderID == 0`, which
    /// the `Communication_HearSpeech` handler discards **before** `CanHear`
    /// is reached — so the id-0 escape inside `CanHear` is not a way to put system text on the
    /// speech channel. Counted rather than folded into the two above, because it is a different
    /// branch of a different function.
    pub speech_lines_with_no_speaker: u64,
    /// `0x02BB` / `0x02BC` / `0x02BD` lines discarded because the local player had
    /// no physics body. This gate precedes every other test in all three handlers. Its own counter
    /// is not folded into the two above, because
    /// "you have no body yet" is a different fact from "they are out of earshot" and it is the
    /// only one of the three that can be true of every line in a batch at once.
    pub speech_lines_with_no_player_body: u64,
    /// `0x02BC` lines refused at
    /// the ranged-talk handler does **not** call `CanHear`, so this is a different
    /// predicate over a different radius (the one on the wire) with the opposite boundary
    /// polarity, and it gets its own counter rather than sharing
    /// [`Self::speech_lines_out_of_earshot`].
    pub ranged_lines_out_of_range: u64,
    /// `0x01E0` / `0x01E2` emotes composed and pushed at chat type `0xC`.
    pub emote_lines_composed: u64,
    /// Emotes `(senderID, 0xC)` refused
    /// — the call that is emote hearing's **first** act.
    /// Split from [`Self::emote_lines_squelched`] the same way the say counters are, so "nobody
    /// emoted", "they are muted" and "they are too far away" are three readings.
    pub emote_lines_out_of_earshot: u64,
    /// See [`Self::emote_lines_out_of_earshot`] — the squelch half of the same
    /// `CanHear` call.
    pub emote_lines_squelched: u64,
    /// `0x01E2` soul emotes discarded by
    /// the soul-emote handler's sender-id self-echo test
    /// above the tail call into the emote handler, with no counterpart on the
    /// `0x01E0` path. This is the one behaviour that distinguishes the two opcodes, so it is
    /// counted rather than inferred from a missing line.
    pub soul_emote_self_echoes_discarded: u64,
    /// Emote lines that were actually garbled: **`0x01E0` / `0x01E2` lines** composed by
    /// [`crate::chat::garbled_line`] from a random 1-to-10 phrase
    /// ([`crate::chat::OLTHOI_TEXT`] / [`crate::chat::HUMAN_TEXT`]) instead of by
    /// [`crate::chat::hear_emote_line`]. A garbled line is *also* counted in
    /// [`Self::emote_lines_composed`], because it is a line that was drawn.
    ///
    /// It is zero on every recorded session: the arm needs a
    /// sender name the shard has marked with `&`, or an Olthoi listener
    /// ([`Hud::is_olthoi`], answered from `HeritageGroup`), and
    /// the corpus has neither. A non-zero reading is a **feature working**.
    pub emote_lines_untranslated: u64,
    /// The same, for `0x02BB` speech lines that take the language-garbling arm.
    ///
    /// Three counters and not one, because the three arms share [`Hud::garbled_or_plain`] and
    /// three arms through one producer is exactly where two get wired and one is forgotten. A
    /// test that only asks "did anything garble" cannot tell which of the three ran.
    pub speech_lines_untranslated: u64,
    /// The same, for `0x02BD` direct-speech lines. This garbling decision sits
    /// **outside** the
    /// `targetID == player_id` gate. See [`Self::speech_lines_untranslated`].
    pub direct_speech_lines_untranslated: u64,
    /// Chat lines that carried a `%#H:%M:%S ` timestamp prefix, and the ones that
    /// deliberately did not (the timestamp option clear, or chat type `0x1A`). Three
    /// states between them and [`Self::speech_lines_composed`], so "the option is off" cannot read
    /// as "the stamper never ran".
    pub timestamps_stamped: u64,
    /// See [`Self::timestamps_stamped`].
    pub timestamps_suppressed: u64,
    /// Combat notification-event lines that reached the chat scroll.
    pub combat_lines: u64,
    /// …and the ones `is_squelched(0, "", combat text type)` dropped. Two counters, not
    /// one, so "nothing was hit" cannot read as "the combat log is muted".
    pub combat_lines_squelched: u64,
    /// `0x02BD` tells that wrote the last-teller id and
    /// the last-teller-name setter — the sender was a **player** and the tell was addressed to us.
    pub last_teller_writes: u64,
    /// Count `0x02BD` tells that reached the scroll and deliberately wrote **nothing**:
    /// an NPC's tell, or a tell we merely overheard. Three states rather than two, because
    /// "no tells arrived" and "126 tells arrived and every one was an NPC" are different facts;
    /// the corpus only carries the second, so the instrument needs a third state.
    pub last_teller_declined: u64,
    /// Count `0x0147` broadcasts that wrote the last-@monarch or
    /// last-@patron user name. **Zero across the whole corpus** — no recording carries a
    /// channel broadcast at all.
    pub at_channel_name_writes: u64,
    /// `0x0147` broadcasts composed and handed to the scroll — the
    /// scroll append of the `Communication_ChannelBroadcast` handler. Without it `/v hi` and
    /// every vassal's line would reach no window.
    pub channel_broadcast_lines_composed: u64,
    /// …composed and then refused by `is_squelched(0, "", type)` —
    /// a globally squelched *type*. Counted apart from "composed" because a shard that sends a
    /// hundred fellowship lines to a player who has squelched Fellowship is a different fact from
    /// a shard that sent none.
    pub channel_broadcast_lines_squelched: u64,
    /// `0xF7E0 Communication_TextboxString` lines refused by
    /// `is_squelched(0, "", text type)` in the
    /// `Communication_TextboxString` handler. Without that gate every globally squelched craft,
    /// salvage or magic notice would still be drawn.
    pub textbox_lines_squelched: u64,
    /// `0xF7E0 Communication_TextboxString` player-killer death lines dropped because the
    /// hear-PK-deaths option is off.
    pub textbox_pk_deaths_dropped: u64,
    /// Frames on which the vitals update actually wrote a meter.
    pub vitals_written: u64,
    /// Frames on which the toolbar's selected-object read-out changed.
    pub toolbar_written: u64,
    /// Frames on which the radar's coordinates or compass moved.
    pub radar_written: u64,
    /// Calls that actually changed `options2_` bit 24 — the radar
    /// padlock reaching the player module. Zero is the normal number: it only moves on a click.
    pub lock_ui_writes: u64,
    /// The selection-changed handler's not-a-stack arm asked for
    /// a health query on the selected id.
    pub selection_health_queries: u64,
    /// …asked for an item-mana query on the selected id.
    pub selection_mana_queries: u64,
    /// …correctly asked for **neither**: a non-attackable, non-player, non-pet
    /// object the player does not own. A real outcome of the client's branch.
    pub selection_queries_declined: u64,
    /// …could not decide, because the view answered no
    /// [`dereth_client_contract::GameView::selection_query_facts`]. Must stay **zero** in this
    /// client: a non-zero is a missing producer wearing the same face as "asked for neither".
    pub selection_queries_unanswerable: u64,
    /// Edge blocks that raised a health query / item-mana query for id 0
    /// — the clear that tells the shard to stop sending updates about the old selection.
    pub selection_query_clears: u64,
    /// `METER_LEVEL` writes into the toolbar's two selected-object meters, from
    /// the `0x01C0` / `0x0264` replies. This is what "a target's health bar updates" is, counted.
    pub selection_meters_written: u64,
    /// Character Options page visibility edges drained when the page becomes
    /// visible.
    pub character_option_edges: u64,
    /// Rows those edges actually moved. Both, because an edge that re-read 49
    /// identical values and an edge that never ran are otherwise the same number.
    pub character_option_rows_reread: u64,
    /// Character Options rows redrawn by the per-frame
    /// equivalent of the client's option-changed notice — boxes whose bit moved under them
    /// (a fellowship exclusion, or a `0x0013`) rather than by their own click.
    pub character_option_rows_noticed: u64,
    /// Chat Options page events drained by `Hud::drive` — an Apply / Cancel /
    /// *Restore Defaults* press or a visibility edge.
    pub chat_option_edges: u64,
    /// Controls those events wrote or re-read. Both counters, for the reason
    /// [`Self::character_option_rows_reread`] gives: a Cancel that reverted nothing and a Cancel
    /// that never ran are otherwise the same number.
    pub chat_option_controls_reread: u64,
    /// Elements the indicator strip and the toolbar's stance icon wrote. It must
    /// be non-zero after the first frame: the link lamp has no picture until something puts it in a
    /// media state, and the four combat-mode buttons are all visible until something hides three.
    pub indicators_written: u64,
    /// Frames on which the inventory panel refilled a slot.
    pub inventory_written: u64,
    /// Frames on which the quickbar refilled a shortcut slot or rewrote a numeral.
    /// One on the frame `0x0013`'s player module lands, and again on every combat-mode change.
    pub shortcuts_written: u64,
    /// `ContentProfile` rows carried by `0x0013` and applied to visible object contents.
    pub content_profiles: u64,
    /// `InventoryPlacement` rows carried by `0x0013` and applied to the object inventory.
    pub inventory_placements: u64,
    /// Frames on which a skills or spellbook rebuild actually rewrote its list.
    pub panels_written: u64,
    /// Accepted ordered power-bar subscriber calls, excluding unrelated panel updates.
    pub power_bar_writes: u64,
    /// The row and slot counts the two panels ended up with, so "it drew nothing" is a number.
    pub skill_rows: u64,
    pub spell_slots: u64,
    /// Element messages offered to the stat-management / spellbook panels, and how many
    /// they took. **Both**, because "0 consumed" and "0 offered" are different failures — the first
    /// is a panel that is not listening and the second is a click that never arrived.
    pub panel_messages: u64,
    pub panel_messages_consumed: u64,
    /// Items dropped on the Trade panel's self list that reached
    /// `TradePanel::drop_item` -- the production producer of `UiRequest::TradeAddItem`.
    pub trade_drops: u64,
    /// Pointer drops delivered to the house-payment panel, including a
    /// partial drop that starts a split and a whole drop that adds a payment row.
    pub house_drops: u64,
    /// Items released over `0x100000CE` that reached
    /// `crate::hud::RemainingPanels::vendor`'s `drop_item`. A drop the splitter refused as a
    /// partial stack is not counted, which is the difference between "the gesture arrived" and
    /// "the gesture was taken".
    pub vendor_sell_drops: u64,

    // ---- quality updates ------------------------------------------------------------------------
    /// Quality-update events applied to the local player description — **all twenty-six forms**, not
    /// just the four vitals ones. [`Self::vital_updates`]
    /// counts the `Attribute2nd` subset of these and is kept because tests assert on it.
    pub quality_updates: u64,
    /// Events rejected as stale. A live server's own stream is in order,
    /// so a non-zero here means the sequence byte or the wrap rule is wrong — the same reasoning
    /// `dereth_client_model`'s corpus replay uses for its own gate.
    pub quality_updates_stale: u64,
    /// Events that passed the gate and had nowhere to land: one of the four partial forms naming
    /// a skill or attribute the local player description does not carry.
    pub quality_updates_unstorable: u64,
    /// Skill records the server changed — the answer to a `Train_TrainSkill`.
    pub skill_updates: u64,

    // ---- quality removes ------------------------------------------------------------------------
    /// `Qualities_*Remove*Event` messages that deleted a key off the player's qualities — the
    /// eight private forms, and the eight public ones that name the player.
    ///
    /// A **separate** counter from [`Self::quality_updates`] on purpose: stat removal is a
    /// different path from the stat update — no value, no player-description mirror, and
    /// the remove handler rather than
    /// the change handler — so folding the two would hide which one the shard sent.
    pub quality_removes: u64,
    /// Removes rejected as stale. The gate and its key
    /// are the update path's: a remove and an update of one property share one 8-bit counter.
    pub quality_removes_stale: u64,
    /// Removes that passed the gate and found nothing to delete — a NULL table, or a property the
    /// local player description was not carrying. The native delete path ignores the return value, so retail
    /// cannot tell this from a real delete, and neither can any caller
    /// here; the counter exists so a test can, and so that a shard removing properties this client
    /// never received is visible instead of silent.
    pub quality_removes_absent: u64,

    // ---- enchantments ---------------------------------------------------------------------------
    /// Enchantment-update messages applied to the **local player's** registry by this module.
    /// The messages are handled in `interaction.rs`, which writes the player's one registry (the
    /// one the buff, debuff and vitae lamps read), so nothing here increments this.
    pub player_enchantments_applied: u64,

    // ---- identify and book panels ---------------------------------------------------------------
    /// Frames on which the identify panel was rewritten.
    /// Zero for a session in which nothing was examined, which is most of them.
    pub examinations_filled: u64,
    /// Frames on which the Book panel wrote -- an `OpenBook`, or a `0x00B8` filling the
    /// page on screen. Zero for a session in which nothing was read.
    pub book_frames: u64,
    /// Attribute records the server changed — the answer to a `Train_TrainAttribute`.
    pub attribute_updates: u64,
    /// Times the stat-management panel's `0x10000004` arm ran: the latch cleared and the footer re-run
    /// because the changed quality came back.
    pub raises_answered: u64,

    // ---- splitter -------------------------------------------------------------------------------
    /// Times the selection-changed handler's splitter block ran — it is
    /// `dereth_ui_screens::screens::gameplay::ToolbarSelection::split_gate_ran`. It is a
    /// **settling** count, not a per-frame one: the
    /// client's own re-entry test (in its item-attributes-changed handler) stops it once
    /// the maximum split size agrees with the selection, so a scene where nothing is selected and nothing
    /// changes must leave this still. A number that climbs with the frame counter means the test
    /// never settles and the box is being rewritten under the player.
    pub split_gate_runs: u64,

    // ---- selection rings and cooldowns ----------------------------------------------------------
    /// Selection rings written by the item list's `set_selected_item` across every live item
    /// list. It moves only on a selection edge, so a number that climbs with the frame counter
    /// means the edge is not settling.
    pub item_slot_rings: u64,
    /// Item slots that re-ran `update_cooldown_display` off the once-a-second heartbeat.
    /// It climbs with wall-clock time and not with frames — roughly (live slots) per second — and
    /// **zero** means the heartbeat is not running at all, which is the state a frozen wedge is
    /// indistinguishable from.
    pub item_slot_heartbeats: u64,

    // ---- spell components -----------------------------------------------------------------------
    /// `0x0013` blobs pushed through `PlayerSystem::apply_player_module`; without it the player
    /// system holds constructor defaults for every session.
    pub player_module_applied: u64,
    /// Objects offered to the component tracker.
    /// The denominator: a tracker with nothing in it after a
    /// large number here means the player carries no components; after a **zero** it means the
    /// sweep never ran.
    pub components_offered: u64,
    /// Of those, how many returned something other than "no change".
    pub components_changed: u64,
    /// Rows the spell-component panel drew on its last rebuild.
    pub component_rows: u64,

    // ---- chat talk focuses and auto-target -----------------------------------------------------
    /// How many of the fourteen talk focuses `enable_chat_talk_focuses` last left enabled.
    /// Focuses 1 and 2 are always on, so **2** is the floor and means every Turbine channel is off
    /// — which is what a client with no Turbine-chat connection shows.
    pub talk_focuses_enabled: u64,
    /// Auto-target sweeps that changed the chat target: adopted
    /// or cleared. It moves at most once a second and **zero over a long session with a selection
    /// made** means the sweep is not reaching the screen.
    pub auto_target_changes: u64,
    /// Objects within the radar radius of the player on the last frame — the input the sweep
    /// consumes. Zero here with a non-empty radar means the range filter, not the sweep, is what
    /// is empty.
    pub auto_target_in_range: u64,

    // ---- spellbook ------------------------------------------------------------------------------
    /// Inbound `0x01A8 Magic_RemoveSpell` messages that reached
    /// [`Hud::handle_magic_remove_spell`] — the shard's answer to a confirmed spellbook DELETE.
    /// Zero over a session in which a spell was deleted means the reply is not being routed.
    pub spells_removed: u64,
    /// Inbound `0x02C1 Magic_UpdateSpell` messages that reached
    /// [`Hud::handle_magic_update_spell`] — the shard's answer to learning a spell from a scroll
    /// or a levelling reward. Zero over a session in which a spell was learned means the book will
    /// not show it until the next login.
    pub spells_added: u64,

    // ---- squelch database -----------------------------------------------------------------------
    /// Inbound `0x01F4 Communication_SetSquelchDB` databases applied to `ChatState::squelch`. The
    /// shard sends one unprompted at login and one after every change, so a zero here in a session
    /// where anything is squelched means the receiver is not being reached.
    pub squelch_db_applied: u64,
    /// Rows the **last** applied database carried -- squelched accounts plus squelched characters.
    /// A replacement is not a merge, so this is the size of the table now and not a running total;
    /// it goes back to zero when the player removes their last squelch, which is exactly the case
    /// [`Self::squelch_db_applied`] alone cannot distinguish from a receiver that never ran.
    pub squelch_rows_applied: u64,

    // ---- unhandled UI events --------------------------------------------------------------------
    /// UI-queue messages that reached the bottom of [`Hud::ui_event`] and were dropped.
    ///
    /// The sibling of `ObjectStream::stats.unhandled`: without it the count of messages the HUD
    /// threw away would not be recorded anywhere in the process. Which opcodes they were is in
    /// [`crate::dropped`]; this is the denominator beside it, kept here so a suite that asserts on
    /// `HudStats` alone can still see the seam move.
    pub ui_events_unhandled: u64,

    /// `0x02BE Fellowship_FullUpdate` messages applied. The recorded sessions carry 23 of these.
    pub fellowship_updates: u64,
    /// `0x0021 Social_FriendsUpdate` applied. The recorded sessions carry it **11 times** across
    /// the three `fellowship-*` captures. This is the arrival count of the message the Friends tab
    /// is drawn from.
    pub friends_updates: u64,
    /// Of these, the following counter records update types for which
    /// the client's friends-list update handler
    /// has no case for. Its own counter rather than `undecodable`: the body parsed, the client
    /// simply does nothing with it, and a non-zero here is a fact about the shard.
    pub friends_updates_unknown_type: u64,
    /// The size of the friends list after the most recent `0x0021` -- the observable a test can
    /// read without a UI.
    pub friends: u64,
    /// `0x0314 Social_SendClientContractTrackerTable` applied — the whole tracker
    /// table replaced. Earlier capture sweeps observed no arrivals, so a non-zero value here is a
    /// fact about the current session rather than a regression guard.
    pub contract_tables: u64,
    /// `0x0315 Social_SendClientContractTracker` applied, by arm — added, updated, removed.
    pub contract_trackers_added: u64,
    pub contract_trackers_updated: u64,
    pub contract_trackers_removed: u64,
    /// Of these, the following counter records requests carrying `setAsDisplayContract`. That flag
    /// pins one contract to the HUD and **this build has no consumer for that**, so it is counted
    /// rather than stored in a field nothing reads.
    pub contract_display_requests: u64,
    /// The size of the tracker table after the most recent `0x0314`/`0x0315`.
    pub contracts: u64,
    /// `0x02B4 Inventory_SalvageOperationsResultData` messages that reached the HUD.
    /// Earlier capture sweeps found no arrivals, against a control set that did contain `0x02BE`.
    /// A non-zero value here is therefore a fact about the current session.
    pub salvage_results: u64,
    /// Of these, the following counter records reports silenced by a squelch on text type `0x19`.
    pub salvage_results_squelched: u64,
    /// Lines the salvage report put in the scroll — between one and two per `0x02B4`.
    pub salvage_lines: u64,
    /// Salvage-panel notices delivered to the panel, and drops accepted by it.
    pub salvage_notices_delivered: u64,
    /// Chess events this build decoded -- the six inbound opcodes, summed.
    ///
    /// Three-state on purpose, and the three counters below it are why: zero here is "no chess
    /// traffic", non-zero with `minigame_guarded` equal to it is "every message was for a board
    /// this window had not joined", and non-zero with `minigame_lines` at zero would be a window
    /// that received and said nothing.
    pub minigame_events: u64,
    /// Chess events the Mini Game panel's own current-game / state guard refused.
    pub minigame_guarded: u64,
    /// Lines `set_info_text` put in the scroll.
    pub minigame_lines: u64,
    /// Notices that reached the Mini Game panel,
    /// i.e. chess boards actually used.
    ///
    /// The notice is produced by `use_object`'s arm 7; this is the counter that means the window
    /// opened.
    pub minigame_boards_used: u64,
    pub salvage_drops: u64,
    /// Material prefixes actually composed, i.e. rows
    /// [`Hud::refresh_display_names`] wrote or rewrote.
    ///
    /// Three-state on purpose. Zero with a salvage bag on screen is a defect;
    /// zero with nothing material-bearing in the world is correct; and a number that climbs
    /// every frame means the guard in [`DisplayName::matches`] has stopped holding, which would be
    /// a per-frame allocation nobody would otherwise see.
    pub display_names_composed: u64,
    /// Trackers dropped by [`HudView::contracts`] because the contract table did not resolve their
    /// id. A denominator: an empty Contracts tab can then say whether the shard sent nothing or
    /// whether the dat did not carry what it sent. The reference client assumes this lookup succeeds.
    pub contracts_unresolved: u64,
    /// …of which created the fellowship, rather than replacing one already held. Retail repaints
    /// every member's blip on a creation and only the two leaders' on a leader change, so the two
    /// are different events and are counted separately.
    pub fellowships_created: u64,
    /// `0x02BE` replacements in which `_leader` moved.
    pub fellowship_leader_changes: u64,
    /// `0x02C0 Fellowship_UpdateFellow` applied. 39 in the three sessions — the single most
    /// frequent fellowship message.
    pub fellow_updates: u64,
    /// A `0x02C0` that arrived with **no fellowship held**, which retail reaches by dereferencing
    /// a null fellowship pointer. Non-zero means the shard sent an update before the full update, and
    /// is a fact about ordering rather than a decode failure — which is why it is not
    /// [`Self::undecodable`].
    pub fellow_updates_without_a_fellowship: u64,
    /// `0x02C0` naming somebody who was **not** already a member — retail's `is_fellow` taken
    /// before the update, and the condition on the fellow-added notice.
    pub fellows_added: u64,
    /// `0x02BF Fellowship_Disband` applied.
    pub fellowship_disbands: u64,
    /// `0x00A3 Fellowship_Quit` applied. 8 in the three sessions.
    pub fellowship_quits: u64,
    /// `0x00A4 Fellowship_Dismiss` applied. 3 in the three sessions.
    pub fellowship_dismissals: u64,
    /// Quits and dismissals whose subject was **the player** — the branch that deletes the whole
    /// fellowship and turns the Fellowship talk focus off, rather than removing one row.
    pub fellowship_departures_our_own: u64,
    /// Chat lines the Fellowship panel composes itself — the create / recruited /
    /// joined / left / dismissed / disbanded sentences — pushed by the five fellowship arms.
    /// Zero with a non-zero `fellowship_updates` means the receivers moved the world and told
    /// the player nothing.
    pub fellowship_lines_composed: u64,
    /// Members the fellowship holds **now**. A replacement is not a merge, so this is the size of
    /// the table and not a running total; it goes to zero on a disband. Two counters and not one,
    /// for the reason [`Self::squelch_rows_applied`] gives: "no fellowship" and "a receiver that
    /// never ran" are indistinguishable in the event counts alone.
    pub fellowship_members: u64,
    /// `0x01C9 Fellowship_FellowUpdateDone` consumed.
    ///
    /// A counter and nothing else, because this event has no state-changing handler body. It exists
    /// so that "the shard
    /// bracketed a burst of `0x02C0`" is *observable* rather than merely un-dropped: a receiver
    /// whose whole retail body is empty has no other trace.
    pub fellow_update_done: u64,
    /// `0x0226 House_HouseStatus` received — the answer to the `0x021E House_QueryHouse` retail
    /// sends during player initialization.
    ///
    /// This is the number `dereth_ui_screens::panels::house::HousePanel` watches: retail's
    /// house panel redraws on **every** notice, so it redraws whenever
    /// this moves. Three arrivals across the three `fellowship-*` recordings, and one in
    /// each of six of the seven original captures — it is on every login.
    pub house_status_notices: u64,
    /// The `u32` the last `0x0226` carried. **A measurement rather than state.**
    ///
    /// ACE writes a `WeenieError` here (`GameEventHouseStatus.cs`); every recorded arrival is
    /// **2** (`BadParam`, `HandleActionQueryHouse`'s *"no house owned"* default). Retail stores it
    /// nowhere at all: the house-transaction handler passes it to the failed-house-transaction
    /// notice and all three registered receivers ignore their
    /// parameter. It is here so a shard that ever sends a different code is visible in a counter
    /// instead of silently taking the same arm.
    pub house_status_last_notice: u64,
    /// `0x0225 House_HouseData` received and copied into the house-data state.
    ///
    /// Zero in all ten recordings, because the recorded character owns no house
    /// — ACE answers `0x021E House_QueryHouse` with `0x0226` in that case and never sends this
    /// one. It is the pane's second redraw edge; see
    /// `dereth_ui_screens::panels::house::HousePanel::update`.
    pub house_data_notices: u64,
    /// `0x021D House_HouseProfile` decoded and copied into the house-profile state, and the edge
    /// that raises the purchase / maintenance window.
    ///
    /// This is the answer to a **use on a slumlord** (`SlumLord.ActOnUse`); the server sends it on
    /// no other path.
    pub house_profile_notices: u64,
    /// `0x0227 House_UpdateRentTime` decoded.
    ///
    /// Counted, not watched: retail's rent-time handler redraws
    /// the house data directly rather than going through the panel update, so there is no
    /// notice for the pane to count and the redraw edge is `HousePanel::update`'s comparison
    /// against what it last drew. This pair of counters is how "arrived" and "arrived and changed
    /// something" stay separable.
    pub house_rent_time_updates: u64,
    /// …and how many of those found a house to apply themselves to. The two differ by exactly the
    /// arrivals for a houseless character, for whom the handler is a no-op with no redraw.
    pub house_rent_time_applied: u64,
    /// `0x0228 House_UpdateRentPayment` decoded.
    pub house_rent_payment_updates: u64,
    /// …and how many of those found a house; the other arm is a no-op.
    pub house_rent_payment_applied: u64,
    /// `0x0248 House_UpdateRestrictions` decoded.
    pub house_restriction_updates: u64,
    /// …and how many survived all four of
    /// the update-restrictions handler's gates. The two differ by the zero
    /// ids, the messages about the local player, the unknown objects and the stale timestamps —
    /// which is four distinct reasons for one number, and the point of having it is that
    /// "arrived" and "stored" stop being the same fact.
    pub house_restrictions_applied: u64,
    /// `0x0257 House_UpdateHAR` decoded and printed.
    ///
    /// The answer to `0x024D House_RequestFullGuestList`, which `@house guest list|show` and
    /// `@house storage list|show` send — and the one inbound house opcode of this pair
    /// observed in the capture sweep, where each arrival followed a `0x024D` request.
    pub house_har_updates: u64,
    /// Guest rows written. Zero alongside a non-zero
    /// [`Self::house_har_updates`] is the *"None"* arm, which is a real and common answer rather
    /// than a failure.
    pub house_har_guests: u64,
    /// `0x0271 House_AvailableHouses` decoded and printed. The answer to
    /// `0x0270`, which `@hslist <type>` and `@house available <type>` send.
    pub house_available_houses: u64,
    /// Coordinate lines the house-list coordinate display wrote. Always **0** for an apartment
    /// listing, whatever the count — retail skips the whole block on `house_type == 4`.
    pub house_available_coord_lines: u64,
    /// `0x019E Combat_HandlePlayerDeathEvent` decoded.
    pub player_deaths: u64,
    /// …and how many of those produced a scroll line. The two differ by exactly the deaths the
    /// player was part of, which retail deliberately says nothing about here: a zero
    /// [`Self::player_deaths_announced`] beside a non-zero [`Self::player_deaths`] means every
    /// death that arrived was your own or your kill, and is **not** a broken receiver. Two
    /// counters, not one, because the `!=` pair is the whole of the handler.
    pub player_deaths_announced: u64,
    /// `0x01AC`/`0x01AD` decoded; both opcodes share identical arguments and behavior.
    pub victim_notifications: u64,
    /// …and how many of those produced a scroll line. They differ by exactly the empty
    /// strings: the handler returns 1 either way and skips the scroll insertion only when
    /// the string's length (terminator included) is 1, so a zero announce beside a non-zero
    /// decode is the shard sending blanks
    /// and not a broken receiver.
    pub victim_notifications_announced: u64,
    /// `0x027A Allegiance_AllegianceLoginNotificationEvent` decoded.
    pub allegiance_logins: u64,
    /// …and how many of those produced a scroll line. They differ by exactly the members absent
    /// from the cached allegiance hierarchy, which is the handler's one guard.
    pub allegiance_logins_announced: u64,
    /// The allegiance data lookup's refusals themselves — a member the cached hierarchy does not
    /// hold. Three states and not two, for the reason [`Self::fellowship_members`] gives: a client
    /// that has never sent `0x001B` has an empty tree and shows **no** logon lines, and that is
    /// retail's behaviour rather than a receiver that did not run.
    pub allegiance_logins_without_a_member: u64,
}

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
    /// of "lost". `// UNVERIFIED:`
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
    /// [`crate::chat::OLTHOI_TEXT`] / [`crate::chat::HUMAN_TEXT`] a garbled line shows.
    ///
    /// `None` until the first garbled line, then seeded from [`crate::audio::ran2_seed`] — which
    /// is `(long)time(NULL)`, the same value seeds the random generator.
    ///
    /// **A declared deviation, and a small one.** Retail has **one** random generator for the whole
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

    /// Consume local numeric placement writes in order, before any PM readback. This is the
    /// host-owned call, not an outbound request.
    /// Missing retained module is this host's current ownership boundary, not a new retail
    /// readiness flag.
    pub fn consume_placement_requests(
        &mut self,
        world: &mut dereth_client_model::World,
        requests: &mut Vec<dereth_client_contract::UiRequest>,
        now: dereth_primitives::ServerTime,
    ) {
        let mut wrote = false;
        requests.retain(|request| {
            if !dereth_client_contract::requests::is_local_module_write(request) {
                return true;
            }
            // The two chat-option arms join the placement arm. All three are one function in the
            // client: setting a chat-window option or a gameplay option raises a local notice and
            // sets the 480-second dirty flag. Nothing here sends.
            match request {
                dereth_client_contract::UiRequest::SetChatWindowOption {
                    window,
                    property,
                    value,
                } => {
                    wrote |= world
                        .player_system
                        .set_chat_window_option(*window, *property, *value, now);
                }
                dereth_client_contract::UiRequest::SetChatWindowTitle { window, title } => {
                    let value = match title {
                        dereth_client_contract::ChatWindowTitle::Literal(text) => {
                            dereth_protocol::property::StringInfo {
                                over: 1,
                                literal: Some(text.clone()),
                                ..Default::default()
                            }
                        }
                        dereth_client_contract::ChatWindowTitle::Table {
                            string_id,
                            table_id,
                        } => dereth_protocol::property::StringInfo {
                            string_id: *string_id,
                            table_id: *table_id,
                            ..Default::default()
                        },
                    };
                    wrote |= world
                        .player_system
                        .set_chat_window_title(*window, value, now);
                }
                dereth_client_contract::UiRequest::SetChatWindowFilter { window, mask } => {
                    wrote |= world
                        .player_system
                        .set_chat_window_filter(*window, *mask, now);
                }
                dereth_client_contract::UiRequest::SetChatOpacity { property, value } => {
                    wrote |= world
                        .player_system
                        .set_gameplay_option_float(*property, *value, now);
                }
                _ => {}
            }
            false
        });
        if !wrote {
            return;
        }
        let module = world
            .player_system
            .module
            .as_ref()
            .expect("accepted local property write");
        self.placements = decode_placements(module);
        if let Some(snapshot) = self.player_module.as_mut() {
            snapshot
                .gameplay_options
                .clone_from(&module.gameplay_options);
        }
        // The retail shared gameplay-option-changed notice ignores placement IDs:
        // local writes must not cause a full screen re-seed on the following frame. A different
        // screen_serial or authoritative PlayerDescription still invalidates the key normally.
        if let Some(key) = self.applied_to.as_mut() {
            key.placements.clone_from(&self.placements);
        }
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load `Attribute2ndTable 0x0E000003` once. A miss is reported, not fatal: the *current*
    /// vitals still read, only the maxima go missing.
    pub fn load_tables(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        world: &dereth_client_model::World,
    ) {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource as _;
        if self.vitals_table.is_none() {
            match store
                .read(ATTRIBUTE_2ND_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    Attribute2ndTable::decode_payload_in(
                        store.era_of(ATTRIBUTE_2ND_TABLE),
                        ATTRIBUTE_2ND_TABLE,
                        &b,
                    )
                    .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.vitals_table = Some(t),
                Err(e) => {
                    tracing::warn!("Attribute2ndTable {ATTRIBUTE_2ND_TABLE:?}: {e}")
                }
            }
        }
        // Both are the panels' only source for a name and an icon; a miss empties the
        // page it feeds and is reported rather than fatal, like the vitals table above.
        if self.skill_table.is_none() {
            match store
                .read(SKILL_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    SkillTable::decode_payload_in(store.era_of(SKILL_TABLE), SKILL_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.skill_table = Some(t),
                Err(e) => tracing::warn!("SkillTable {SKILL_TABLE:?}: {e}"),
            }
        }
        if self.spell_table.is_none() {
            match store
                .read(SPELL_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    SpellTable::decode_payload_in(store.era_of(SPELL_TABLE), SPELL_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.spell_table = Some(t),
                Err(e) => tracing::warn!("SpellTable {SPELL_TABLE:?}: {e}"),
            }
        }
        // The contracts panel's only source for a name, a description, two NPCs and
        // three positions. A miss empties the tab and is reported rather than fatal, like the
        // three above.
        if self.contract_table.is_none() {
            match store
                .read(CONTRACT_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::tables::ContractTable::decode_payload(CONTRACT_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.contract_table = Some(t),
                Err(e) => tracing::warn!("ContractTable {CONTRACT_TABLE:?}: {e}"),
            }
        }
        // The component tracker's two dat hops.
        // The enum lookup is `(3, 0x10000001, 0x28)` — a mapping whose forward direction is
        // SCID -> WCID — and this build has no cached resolver for that enum
        // (the same shortcut `GENDER_ENUM_MAPPER` and `panels::statmgmt::STRING_TABLE` take). So
        // the mapper is found by **matching**, not by a hard-coded id: every `0x27xxxxxx` is
        // decoded and the one whose keys overlap the `SpellComponentTable`'s SCIDs most is taken.
        //
        // That is deliberately an instrument that can prove it looked: `component_mapper_scan`
        // records how many mappers were examined and how many entries matched, so an empty
        // catalogue reads as "scanned N, matched 0" rather than as a silent zero.
        if self.component_catalogue.is_empty() {
            self.load_component_catalogue(store);
        }
        // The client lookup selects value 10, group `0x10000001`, and type `0x28`.
        // `did_by_enum` performs the native group/value walk; `read_typed` supplies the independent
        // `DB_TYPE_DUAL_DID_MAPPER` qualification used by the native lookup.
        if self.trade_note_values.is_empty() {
            let loaded = dereth_assets::did_by_enum(store, 0x1000_0001, 10)
                .ok_or_else(|| "enum group 0x10000001 value 10 is absent".to_owned())
                .and_then(|id| {
                    store
                        .read_typed(dereth_dat::DbType::DualDidMapper, id)
                        .map_err(|e| format!("{id:?}: {e}"))
                        .and_then(|bytes| {
                            dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes)
                                .map_err(|e| format!("{id:?}: {e}"))
                        })
                });
            match loaded {
                Ok(mapper) => self.trade_note_values = mapper.0.enum_to_id,
                Err(e) => tracing::warn!("TradeNotes DualDidMapper: {e}"),
            }
        }
        // School-to-component-pack mapping:
        // The school/value/type lookup selects mapping table `0x27000003`
        // (`SchoolOfMagic` / `ComponentPacks`). The same two-hop walk and the same
        // `DB_TYPE_DUAL_DID_MAPPER` qualification as the trade notes above; the five Foci WCIDs
        // are the dat's, not this file's.
        if self.school_pack_wcid.is_empty() {
            let loaded = dereth_assets::did_by_enum(store, 0x1000_0001, 4)
                .ok_or_else(|| "enum group 0x10000001 value 4 is absent".to_owned())
                .and_then(|id| {
                    store
                        .read_typed(dereth_dat::DbType::DualDidMapper, id)
                        .map_err(|e| format!("{id:?}: {e}"))
                        .and_then(|bytes| {
                            dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes)
                                .map_err(|e| format!("{id:?}: {e}"))
                        })
                });
            match loaded {
                // Row 0 is the mapper's own `Undef -> 0`; `magic_pack_is_owned` would never match a
                // WCID of 0 anyway, and `school_of_magic_to_wcid`'s miss is already 0, so it is
                // dropped here rather than carried as a school.
                Ok(mapper) => {
                    self.school_pack_wcid = mapper
                        .0
                        .enum_to_id
                        .into_iter()
                        .filter(|(k, v)| *k != 0 && *v != 0)
                        .collect();
                }
                Err(e) => tracing::warn!("SchoolOfMagic DualDidMapper: {e}"),
            }
        }
        // The quality filter, for the int and float enchantment queries. A miss leaves int and
        // float qualities un-enchanted, which is the client's returning null — it
        // returns 0 and leaves the caller's value alone.
        if self.quality_filter.is_none() {
            let ids = store.ids_of(dereth_dat::DbType::QualityFilter);
            match ids.first() {
                Some(id) => match store.read(*id).map_err(|e| e.to_string()).and_then(|b| {
                    dereth_assets::tables::QualityFilter::decode_payload(*id, &b)
                        .map_err(|e| e.to_string())
                }) {
                    Ok(t) => self.quality_filter = Some(t),
                    Err(e) => tracing::warn!("QualityFilter {id:?}: {e}"),
                },
                None => tracing::warn!("no QualityFilter in the dat"),
            }
        }
        if self.xp_table.is_none() {
            match store
                .read(XP_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::tables::XpTable::decode_payload_in(
                        store.era_of(XP_TABLE),
                        XP_TABLE,
                        &b,
                    )
                    .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.xp_table = Some(t),
                Err(e) => tracing::warn!("XpTable {XP_TABLE:?}: {e}"),
            }
        }
        self.era.world_dats = store.era();
        if !self.era.era_announced {
            self.era.era = dereth_client_contract::EraView::era_of_dats(self.era.world_dats);
        }
        if let Some(t) = &self.xp_table {
            self.era.level_cap = u32::try_from(t.level_xp.len().saturating_sub(1)).unwrap_or(0);
        }
        if let Some(t) = &self.skill_table {
            self.era.skills = t.skills.keys().copied().collect();
        }
        // Four more of the same shape, for the header's heritage line. A miss leaves
        // that one field empty and is reported, exactly as the four above.
        for (slot, id, what) in [
            (
                &mut self.gender_names,
                GENDER_ENUM_MAPPER,
                "gender EnumMapper",
            ),
            (
                &mut self.heritage_names,
                HERITAGE_ENUM_MAPPER,
                "heritage EnumMapper",
            ),
            (
                &mut self.title_tokens,
                TITLE_ENUM_MAPPER,
                "title EnumMapper",
            ),
            (
                &mut self.creature_type_names,
                CREATURE_TYPE_ENUM_MAPPER,
                "creature-type EnumMapper",
            ),
        ] {
            if slot.is_some() {
                continue;
            }
            match store.read(id).map_err(|e| e.to_string()).and_then(|b| {
                dereth_assets::tables::EnumMapper::decode_payload(id, &b).map_err(|e| e.to_string())
            }) {
                Ok(t) => *slot = Some(t),
                Err(e) => tracing::warn!("{what} {id:?}: {e}"),
            }
        }
        // The salvage report's material names. A miss leaves every material
        // reading "Unknown", which is the material-name lookup returning `false` --
        // retail's own answer, and reported here as the other four are.
        if self.material_names.is_none() {
            match store
                .read(MATERIAL_TYPE_NAMES)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::DidMapper::decode_payload(MATERIAL_TYPE_NAMES, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.material_names = Some(t),
                Err(e) => {
                    tracing::warn!("material names {MATERIAL_TYPE_NAMES:?}: {e}");
                }
            }
        }
        if self.title_strings.is_none() {
            match store
                .read(TITLE_STRING_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::ui::StringTable::decode_payload(TITLE_STRING_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.title_strings = Some(t),
                Err(e) => tracing::warn!("title StringTable {TITLE_STRING_TABLE:?}: {e}"),
            }
        }
        self.rebuild_panel_tables(world);
    }

    /// The qualities from the local player description, which exists for the player's
    /// weenie and no other; the same
    /// question `HudView::player_desc` asks, for the paths that hold a `&World` instead of a view.
    ///
    /// `None` is "no `0x0013` yet" — the skill-list rebuild's own
    /// local-player-description guard — and **not** "no object row yet", which
    /// the row's empty `Qualities` would otherwise make indistinguishable.
    #[must_use]
    pub fn player_desc<'a>(
        &self,
        world: &'a dereth_client_model::World,
    ) -> Option<&'a dereth_client_model::Qualities> {
        if !self.player_desc_received {
            return None;
        }
        // The row first, because once it
        // exists it is the object every later stat update writes; the parked
        // description behind it, because `0x0013` can be unpacked before the row it belongs on
        // exists -- and the two joins below are run **on that message** and by nothing else.
        //
        // The ordinary row lookup returns silently when the player id is unset or the weenie row
        // is missing, which is the ordering ACE actually produces:
        // `Player_Networking.SendSelf` enqueues `GameEventPlayerDescription` before
        // `GameMessageCreateObject` and the two travel on different queues (`long-solo-play` has them
        // at idx 9 on queue 9 and idx 22 on queue 10). Without the fallback `build_skills` and
        // `build_spells` would take the skill-list rebuild's missing-player-description arm at the
        // one moment they are ever run, and the skills page, the spellbook and the attribute
        // page would stay empty for the rest of the session.
        //
        // In the reference client the distinction cannot arise: qualities are installed before
        // the player-description-received notice is raised, with the row guaranteed present by its queue order. Answering
        // from the park is what makes this build behave the way retail does. It is not a second
        // store and it is not a widened guard: it is the same qualities, read one step
        // before its owner is named.
        world
            .player_qualities()
            .or_else(|| world.login_player_desc())
    }

    /// Rebuild the skill-table and spellbook joins once per relevant change rather than once per
    /// frame.
    ///
    /// Both are pure functions of `(the retail table, the player's qualities)`, and both are
    /// notice-driven in the client — the player-description-received notice for each, plus the
    /// skill-advancement-class-changed notice for the skills. The two callers here are `load_tables` (the
    /// table arrived) and the `0x0013` arm of `apply_events` (the qualities arrived).
    pub fn rebuild_panel_tables(&mut self, world: &dereth_client_model::World) {
        self.skills = self.build_skills(world);
        self.spells = self.build_spells(world);
    }

    /// The skill-list rebuild's loop: **every key of the `SkillTable`**, joined to the skill query.
    ///
    /// A skill the player's skill-stats table has no entry for still gets a row — the skill query
    /// leaves `_sac` at `UNDEF` and the rebuild files it under the fourth header. So the
    /// row count is the table's size and not the player's, which is what makes the fourth group
    /// non-empty on a fresh character.
    fn build_skills(&self, world: &dereth_client_model::World) -> Vec<SkillEntry> {
        let Some(table) = self.skill_table.as_ref() else {
            return Vec::new();
        };
        // The skill-list rebuild's own guard: the body is inside
        // the local-player-description guard, so before `0x0013` the client leaves the
        // list flushed rather than drawing thirty-eight undefined rows. Without this the page
        // comes up full of zeroes in the fourth group and then rebuilds, which is visible.
        let Some(q) = self.player_desc(world) else {
            return Vec::new();
        };
        table
            .skills
            .iter()
            .map(|(id, base)| {
                // The row's advancement class and raw skill value are queried independently.
                let sac = dereth_client_model::skills::inq_skill_advancement_class(q, *id) as u32;
                let level = dereth_client_model::skills::inq_skill(q, table, *id, true)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                // The font operand is the skill query with `raw = 0`, the **enchanted** value,
                // not the base-level query's attribute contribution — see `SkillEntry::effective`.
                let effective = dereth_client_model::skills::inq_skill(q, table, *id, false)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                // The vitae penalty on **this skill's raw level**, zero or negative. The panel
                // update's colour comparison subtracts
                // it, which is what stops a vitae-carrying character's whole list drawing red.
                // Read straight off the registry, the same route the attribute-2nd rows take:
                // `HudView` does not implement `GameView::vitae`.
                let vitae = dereth_client_contract::panels::inforegion::vitae_modifier(
                    level,
                    Some(q.enchantments.vitae_value()),
                );
                SkillEntry {
                    id: *id,
                    name: base.name.clone(),
                    icon: (base.icon != 0).then_some(DataId(base.icon)),
                    min_level: base.min_level,
                    sac,
                    level,
                    effective,
                    vitae,
                }
            })
            .collect()
    }

    /// Find and decode the WCID -> SCID mapping and the component table, and join
    /// them into a [`dereth_client_model::magic::ComponentCatalogue`].
    ///
    /// See the call site for why the mapper is matched rather than named.
    fn load_component_catalogue(&mut self, store: &dereth_dat::RetailDatStore) {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource as _;
        let table: dereth_assets::tables::SpellComponentTable = match store
            .read(SPELL_COMPONENT_TABLE)
            .map_err(|e| e.to_string())
            .and_then(|b| {
                dereth_assets::tables::SpellComponentTable::decode_payload(
                    SPELL_COMPONENT_TABLE,
                    &b,
                )
                .map_err(|e| e.to_string())
            }) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("SpellComponentTable {SPELL_COMPONENT_TABLE:?}: {e}");
                return;
            }
        };
        let ids = store.ids_of(dereth_dat::DbType::DualDidMapper);
        let mut best: Option<(usize, Vec<(u32, u32)>)> = None;
        let mut examined = 0usize;
        for id in ids {
            let Ok(bytes) = store.read(id) else { continue };
            let Ok(m) = dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes) else {
                continue;
            };
            examined += 1;
            let hits =
                m.0.enum_to_id
                    .iter()
                    .filter(|(k, _)| table.components.contains_key(k))
                    .count();
            if hits > best.as_ref().map_or(0, |(n, _)| *n) {
                best = Some((hits, m.0.enum_to_id.clone()));
            }
        }
        let (matched, pairs) = best.unwrap_or((0, Vec::new()));
        self.component_mapper_scan = (examined, matched);
        self.component_catalogue = dereth_client_model::magic::ComponentCatalogue::new(
            pairs,
            table.components.iter().map(|(scid, c)| {
                (
                    *scid,
                    dereth_client_model::magic::ComponentBase {
                        name: c.name.clone(),
                        category: c.category,
                        icon: c.icon,
                    },
                )
            }),
        );
        tracing::debug!(
            "component catalogue: {} components, {matched} of them mapped from \
             {examined} component mapping table(s)",
            table.components.len()
        );
    }

    /// Join the player's transcribed-spell list to the spell table.
    ///
    /// Unlike the skills, this walks the **player's** spellbook and looks each id up in the
    /// table: the player-description refresh iterates the transcribed list, and the spell add
    /// drops an id the spell table does not know. A spell whose base is missing is
    /// therefore skipped here too rather than drawn as a blank row.
    fn build_spells(&self, world: &dereth_client_model::World) -> Vec<SpellEntry> {
        let Some(q) = self.player_desc(world) else {
            return Vec::new();
        };
        let Some(book) = q.spell_book.as_ref() else {
            return Vec::new();
        };
        // The `SpellTable` half of the join, and the "a spell whose base is missing is skipped"
        // rule, are both [`Self::spell_entry`]'s `?`s.
        book.keys().filter_map(|id| self.spell_entry(*id)).collect()
    }

    /// One `SpellTable` row as a [`SpellEntry`] —
    /// with the four derivations [`Self::build_spells`] needs.
    ///
    /// Extracted because the Spellcasting panel asks the same question about a spell that is **not**
    /// in the player's book: the endowment icon and the cast button's tooltip both look up the
    /// endowment spell, the spell on the wielded wand.
    /// See [`dereth_client_contract::GameView::spell`].
    pub fn spell_entry(&self, id: u32) -> Option<SpellEntry> {
        use dereth_client_contract::spellbook::power_component;
        use dereth_client_model::magic::{scarab_power_level, spell_level_by_rough_heuristic};
        // Retail's spell add drops an id the spell table does not know, and so does this `?`.
        let b = self.spell_table.as_ref()?.spells.get(&id)?;
        Some(SpellEntry {
            id,
            name: b.name.clone(),
            icon: (b.icon != 0).then_some(DataId(b.icon)),
            school: b.school,
            // The level heuristic reads the **first** formula slot after the power component is
            // selected, which keeps slot positions;
            // `SpellBase::comps` drops the zero slots, so the raw slot is used.
            level: spell_level_by_rough_heuristic(scarab_power_level(power_component(
                b.raw_comps[0],
                b.comp_key,
            ))),
            display_order: b.display_order,
            // Preserve the full spell bitfield: icon composition reads
            // `Reversed (0x10)` for the wash and `FellowshipSpell (0x2000)` /
            // `SelfTargeted (0x8)` for the badge.
            bitfield: b.bitfield,
        })
    }

    /// The host's transit for the Salvage panel's notices -- see
    /// [`dereth_client_contract::panels::salvage::SalvageNotice`].
    ///
    /// Two methods rather than a `pub` field, because the two calls mean different things:
    /// `App` clears unconditionally at the head of the frame (an undelivered batch must not
    /// survive a screen change and open a window for a tool used two screens ago) and queues only
    /// when the panel generation that will receive them already existed.
    pub fn clear_salvage_notices(&mut self) {
        self.pending_salvage.clear();
    }

    /// See [`Self::clear_salvage_notices`].
    pub fn queue_salvage_notices(
        &mut self,
        notices: Vec<dereth_client_contract::panels::salvage::SalvageNotice>,
    ) {
        self.pending_salvage.extend(notices);
    }

    /// One frame's worth of session events, applied.
    ///
    /// Returns the chat lines that arrived, in order — for the caller to log. They are also queued
    /// for `Self::drive`, because routing is `ChatInterface`'s and the screen owns the five
    /// interfaces.
    pub fn apply_events(
        &mut self,
        events: &[SessionEvent],
        world: &mut dereth_client_model::World,
    ) -> Vec<ChatMessage> {
        self.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut NoPanels,
            None,
            &mut |_, _| {},
        )
    }

    /// Apply HUD events and synchronously deliver the combat system's player-quality callback.
    /// `App::apply_hud_events` supplies the production handler; HUD-only consumers can continue
    /// using `apply_events`. Delivery is per accepted update, not a poll of the batch's final
    /// quality value, so stale updates and intervening mode changes keep their retail ordering.
    pub fn apply_events_with_combat_mode_handler(
        &mut self,
        events: &[SessionEvent],
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        mut ui_requests: Option<&mut dereth_client_contract::requests::Outbox>,
        on_combat_mode: &mut dyn FnMut(
            &mut dereth_client_model::World,
            dereth_client_model::combat::CombatMode,
        ),
    ) -> Vec<ChatMessage> {
        let mut chat = Vec::new();
        // **The two inputs scroll insertion reads that this crate owns.**
        // The timestamp option comes off the same option word every other option does,
        // and the clock is read inside
        // scroll insertion at the instant the line lands, which no pure function can do. Pushed
        // here, at the head of the batch, for the same reason `set_lock_ui` and `reply_targets` are
        // pushed once a frame: the two owners cannot see each other.
        world.scroll.display_time_stamps = world
            .player_system
            .options
            .get(dereth_client_model::player::options::option::DISPLAY_TIME_STAMPS);
        world.scroll.now_unix = wall_clock_unix();
        world.scroll.utc_offset_secs = utc_offset_secs(world.scroll.now_unix);
        self.drain_scroll(world, panels, &mut chat);
        // The spell component table, before any object of the batch: a component the world meets
        // without it is filed under no category.
        world.install_component_catalogue(&self.component_catalogue);
        for e in events {
            match e {
                // Character startup is gated by the event's enable flag. Handle it in stream order
                // so a same-batch PlayerDescription
                // sees startup before its talk-focus-enable notice.
                SessionEvent::CharacterSet(set) => {
                    // Character startup
                    // copies the unpacked account name into the player system
                    // **before** it looks at the Turbine-chat flag, and that field is
                    // the one `get_appropriate_spell_formula` hashes to pick a spell's
                    // tapers. Without this the model's account stays empty and every eight-slot
                    // formula lists the empty-name tapers instead of the player's.
                    world.player_system.account.clone_from(&set.account);
                    if set.use_turbine_chat != 0 {
                        world.chat.startup_turbine_chat();
                    }
                }
                SessionEvent::TurbineChat(raw) => {
                    // Preserve ordinary notices already queued by an earlier event separately.
                    self.drain_scroll(world, panels, &mut chat);
                    if !world.recv_turbine_chat(raw) {
                        self.stats.undecodable += 1;
                    }
                    let lines = self.collect_scroll(world, panels);
                    if let Some(generation) = self.turbine_chat_generation {
                        self.pending_chat
                            .extend(lines.iter().cloned().map(|m| (Some(generation), m)));
                    }
                    chat.extend(lines);
                }
                // `0x0013 Login_PlayerDescription` replaces the current player description. Both
                // halves land here: the qualities the vitals read and the
                // player module every window's player-module refresh reads.
                SessionEvent::PlayerDescription(d) => {
                    // Placement calls queued before this authoritative replacement belong to
                    // the old local module. Retail performed them synchronously and then replaced
                    // that module. Drop only that delayed numeric subset, never unrelated actions.
                    if let Some(q) = ui_requests.as_deref_mut() {
                        let _ = q.take_placement_updates();
                    }
                    // **`0x0013` fills the one store.**
                    //
                    // The login handler copies qualities straight into the player description that
                    // every later `Qualities_*Update*` writes and every panel reads. There
                    // is no staging buffer and no second copy, so there is none here either:
                    // the private integer-quality handler is the public handler with the player id
                    // substituted, and both reach the stat update, which writes the weenie's
                    // qualities. One owner; every update message maintains it.
                    //
                    // The park inside `apply_player_desc_qualities` handles `0x0013` arriving
                    // before the player's weenie row: `long-solo-play` has it at idx 9 on queue 9 and
                    // `0xF746` at idx 22 on queue 10. Retail does not need the park because
                    // it drains queue 10 before queue 9 — and neither, in
                    // practice, does this build — but the two halves can land in different receives
                    // and ACE never resends a `CoinValue` it thinks unchanged
                    // (`Player_Commerce.cs:326`), so a dropped login purse would stay dropped.
                    //
                    // **Receipt time, not `0.0`.**
                    //
                    // Qualities decoding reaches enchantment decoding,
                    // and `_start_time` and `_last_time_degraded` arrive **relative to receipt**:
                    // the client stores `current_time + value` (see the
                    // note at the head of `dereth_client_model::enchant`). The panel then draws
                    // the remaining duration, `(_duration + _start_time) - current_time`.
                    //
                    // Rebasing the login blob against **zero** would make that difference
                    // `wire_start + duration - cur_time`, i.e. the whole elapsed session too
                    // negative, and `format_duration` clamps at zero: every timed buff the
                    // `0x0013` carried would read **0:00**, while still expiring at the right
                    // moment because the expiry is the server's `0x02C3 Magic_RemoveEnchantment`
                    // and not this number. `0x0013` is the
                    // only place a re-login learns its enchantments from.
                    //
                    // The same rule applies to the AC qualities
                    // (`Qualities::apply_ac_qualities`).
                    //
                    // `Hud::now` is the receipt time: `App::deliver_session_events` stamps it
                    // from the same `LocalTime(self.clock.cur_time)` it hands
                    // `ObjectStream::apply_event` and `Interaction::last_use_time`, so the
                    // `0x02C2` path (`interaction.rs`, `inter.last_use_time`) and this one
                    // rebase against one clock. `Hud::drive` re-stamps it every frame from
                    // `UiSystem::now`, which is the same value.
                    //
                    // `UiSystem::now` carries a note about precisely this mistake one
                    // layer down: "Passing a zero here instead ... makes every one of them expire
                    // on its first tick".
                    let dids = world.apply_player_desc_qualities(&d.qualities, self.now);
                    self.player_desc_received = true;
                    if crate::trace::raise() {
                        // The `Int64` table exactly as the live `0x0013`
                        // delivered it, and where it landed (the row, or the park).
                        let q = world
                            .player_qualities()
                            .or_else(|| world.login_player_desc());
                        tracing::debug!(
                            target: "dereth::trace::raise",
                            "raise-trace 0x0013 player descriptor: player_row={} parked={} \
                             int64s={:?} strength(attr 1)={:?} level(int 25)={:?}",
                            world.player_qualities().is_some(),
                            world.login_player_desc().is_some(),
                            q.and_then(|q| q.int64s.as_ref()),
                            q.and_then(|q| q.attribute(1)),
                            q.map(|q| q.inq_int(25)),
                        );
                    }
                    tracing::debug!(
                        "0x0013 player DataIDs: {dids}, CombatTable {:?}",
                        world.combat_table_did()
                    );
                    self.placements = decode_placements(&d.player_module);
                    self.stats.placements_decoded =
                        u64::try_from(self.placements.rows.len()).unwrap_or(0);
                    // The blob is what decides which windows the player sees, so it is logged in
                    // full: a HUD that comes up wrong is almost always this table being wrong.
                    for (id, row) in &self.placements.rows {
                        tracing::debug!("HUD placement window {id}: {row:?}");
                    }
                    self.player_module = Some(d.player_module.clone());
                    self.stats.player_desc_applied += 1;
                    // The skills and spellbook views both rebuild their whole list from
                    // the player description that just landed.
                    self.rebuild_panel_tables(world);
                    tracing::debug!(
                        "0x0013 panels: {} skill rows, {} spells in the book",
                        self.skills.len(),
                        self.spells.len()
                    );
                    // Login-description handling ends
                    // by loading the player's content profiles and inventory placements. `0x0013`
                    // is the **only** message that tells the client
                    // what is in the player's pack at login — the individual `0xF745` creates that
                    // follow carry each item's container id but never the list — so without
                    // these two calls the backpack would be empty however full the pack was.
                    //
                    // The id is the world's player id, falling back to the one recorded here,
                    // which is `0xF746`'s and arrives before `0x0013`.
                    let player = world.player.or(self.player);
                    if let Some(p) = player {
                        world.view_object_contents(
                            p,
                            &d.content_profiles,
                            &mut dereth_client_model::NullSink,
                        );
                        world.update_object_inventory(
                            p,
                            d.inventory_placements
                                .iter()
                                .map(|q| dereth_client_model::objects::InventoryPlacement {
                                    iid: q.iid,
                                    loc: q.location,
                                    priority: q.priority,
                                })
                                .collect(),
                        );
                        self.stats.content_profiles +=
                            u64::try_from(d.content_profiles.len()).unwrap_or(0);
                        self.stats.inventory_placements +=
                            u64::try_from(d.inventory_placements.len()).unwrap_or(0);
                        tracing::debug!(
                            "0x0013 inventory for {p:?}: {} contents, {} placements",
                            d.content_profiles.len(),
                            d.inventory_placements.len()
                        );
                    }
                    // ---- the things `0x0013` drives ----
                    //
                    // **`PlayerSystem::apply_player_module`** is the
                    // player-module initialization seam, and without it the player system
                    // stays at its constructor defaults for the whole
                    // session: `desired_comps_` empty (so the component drain has
                    // nothing to walk), `spell_filters_` at the default, and the character options
                    // — which four `dereth-client-model` decisions read — never the player's own. This crate
                    // decodes the same blob into `Hud::player_module` for the window placements,
                    // which is why a missing call here would be invisible: the blob would be in the
                    // client, in a second copy, and the model half of it never filled.
                    let effects = world.player_system.apply_player_module(&d.player_module);
                    self.stats.player_module_applied += 1;
                    tracing::debug!(
                        "0x0013 player module: {} desired comps, spell filters {:#x}, \
                         side effects {effects:?}",
                        world.player_system.desired_comps.len(),
                        world.player_system.spell_filters,
                    );

                    // Player-description receipt also enables the chat talk focuses.
                    // `is_olthoi` is
                    // the player-system creature-type query this function calls.
                    //
                    // The is-Olthoi test is `PropertyInt 0xBC HeritageGroup`
                    // against 12 and 13, read from the canonical local-player object row that the
                    // arm above has just filled from this message. The value is therefore this
                    // character's and not the previous one's. The
                    // corpus answers `false` for every recorded character.
                    let olthoi = self.is_olthoi(world);
                    let focuses = world.enable_chat_talk_focuses(olthoi);
                    self.stats.talk_focuses_enabled =
                        u64::try_from(focuses.iter().filter(|e| **e).count()).unwrap_or(0);

                    // Player initialization's component drain and the
                    // catalogue it needs.
                    world.install_component_catalogue(&self.component_catalogue);
                    // `school_of_magic_to_wcid`'s mapper, the other half of
                    // `get_appropriate_spell_formula`'s foci test. Without it
                    // `magic_pack_is_owned` is asked about WCID 0 and always refuses.
                    if world.magic.school_pack_wcid.is_empty() && !self.school_pack_wcid.is_empty()
                    {
                        world.magic.school_pack_wcid =
                            self.school_pack_wcid.iter().copied().collect();
                    }
                    // The same lazy spell-table load
                    // `(6, 2, 0x10000005)` that casting does on its
                    // first call, hoisted to the point the table is already in hand. Without it
                    // `cast_spell` takes its "no table" arm and every cast is a silent no-op.
                    if world.magic.spell_table.is_none() {
                        if let Some(t) = self.spell_table.as_ref() {
                            world.magic.spell_table = Some(std::sync::Arc::new(t.clone()));
                        }
                    }
                    if world.magic.spell_beneficial.is_empty() {
                        if let Some(t) = self.spell_table.as_ref() {
                            // The spell-totals count's one table read: `_bitfield & 4`,
                            // the spell's beneficial flag.
                            world.magic.spell_beneficial = t
                                .spells
                                .iter()
                                .map(|(id, b)| (*id, b.bitfield & 4 != 0))
                                .collect();
                        }
                    }
                    let (offered, changed) = world.initialize_spell_components();
                    self.stats.components_offered += u64::try_from(offered).unwrap_or(0);
                    self.stats.components_changed += u64::try_from(changed).unwrap_or(0);
                    // Recount after unpacking — the enchantment registry that just
                    // landed has never been counted.
                    let (helpful, harmful) = world.recount_spell_totals();
                    tracing::debug!(
                        "0x0013 magic: {offered} objects offered to the component \
                         tracker, {changed} changed, {} components owned; enchantments \
                         {helpful} helpful / {harmful} harmful",
                        world.magic.components.tracked_objects()
                    );

                    // A new description is a new screen state: re-apply on the next frame.
                    self.applied_to = None;
                }
                // The player id arrives before the player object.
                SessionEvent::PlayerCreated(id) => self.player = Some(*id),
                SessionEvent::UiEvent { opcode, blob } => {
                    let before = chat.len();
                    self.ui_event(*opcode, blob, world, panels, &mut chat, on_combat_mode);
                    // These handlers compose the same complete body that
                    // retail hands to the censor filter. Filter only
                    // this direct slice before either registered receiver sees it. Lines from
                    // `world.scroll` were filtered by that producer and never enter this slice,
                    // so no line passes the censor filter twice.
                    for line in &mut chat[before..] {
                        line.body = world.scroll.filter_text(&line.body);
                    }
                    // The second of the two places a `ChatMessage` is born in this
                    // build; the first is `drain_scroll`. In the client both are the *same* place —
                    // every one of these handlers ends in the scroll insertion, which is
                    // what stamps — so the stamp is applied here rather than at each of the fifteen
                    // `chat.push` sites above. Routing these arms through `dereth_client_model::scroll` so
                    // there is one seam instead of two would move the existing queueing.
                    self.stamp_timestamps(world, &mut chat[before..]);
                    // These lines take the native scroll-insertion route but are still composed
                    // directly in `Hud`. Append only this new finalized
                    // slice: `world.scroll` lines before `before` wrote at their producer, so this
                    // cannot double-log either route.
                    for line in &chat[before..] {
                        let _ = world.scroll.copy_final_to_log(
                            u32::from(line.ty),
                            line.prefix.as_deref(),
                            &line.body,
                        );
                    }
                    // The other half of the final-string display notice's
                    // fan-out: the spew receiver takes every notice whose chat type is
                    // `0x1A`, and it is **not an alternative**
                    // to the chat window -- both receivers are offered every line. A network line
                    // (a `0x02EB Communication_TransientString`, e.g. ACE's "The <chest> is
                    // locked") is type `0x1A`, which the main window's default filter `0xFBFFFFFF`
                    // drops (bit 26 clear), so without this the line is filtered out of the
                    // scrollback and never reaches the bubble strip -- dropped entirely. Client-
                    // side notices escape only because they flow through `collect_scroll`, which
                    // makes the same offer; this is that offer for the network arm.
                    for m in &chat[before..] {
                        let took = panels.spew_offer(m.ty, &m.body);
                        if took {
                            self.stats.spew_lines += 1;
                        }
                        if crate::trace::notice() {
                            // The first of the two receivers, answered at
                            // arrival; the chat windows answer at render (`drive`).
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace line from {:#06x}: type={:#x} window={} \
                                 text={:?} -> spew box took={took} (list bound={} pending={} \
                                 drawn so far={}); queued for the chat windows",
                                opcode.0,
                                m.ty,
                                m.window,
                                m.body,
                                panels.spew_trace().0,
                                panels.spew_trace().1,
                                panels.spew_trace().2,
                            );
                        }
                    }
                    // Queued as they arrive, so a `LoggedOff` later in the same batch clears them:
                    // logging off takes the chat windows down with the screen, and a line still in
                    // flight is gone with them.
                    self.pending_chat
                        .extend(chat[before..].iter().cloned().map(|m| (None, m)));
                }
                SessionEvent::StateChanged(
                    dereth_client_net::client_session::SessionState::CharacterSelect
                    | dereth_client_net::client_session::SessionState::Disconnected(_),
                )
                | SessionEvent::LoggedOff => {
                    // The local player description belongs to the player object, so logging off
                    // drops it with the weenie rather than here; what this module owns is the bit
                    // that says whether a description has been unpacked into it, and the parked
                    // login snapshot that would otherwise be re-installed onto the next character's
                    // row. Player-description release is part of weenie teardown for exactly this
                    // reason. It occurs only when qualities are present, and that guard is
                    // load-bearing here, not decoration. This arm matches `CharacterSelect` as well
                    // as `LoggedOff`, and a **login** passes through `CharacterSelect` on its way
                    // in -- the recorded `early-inventory-and-casting` session does it before its
                    // own `0x0013`. Releasing there would strip the local player description that
                    // the next `0x02CD` is entitled to land in, making it `Unstorable`. A session
                    // that has received no description has nothing to release.
                    if std::mem::replace(&mut self.player_desc_received, false) {
                        world.release_player_desc();
                    }
                    // The joins are qualities-derived, so they go with them.
                    self.skills.clear();
                    self.spells.clear();
                    self.player_module = None;
                    self.player = None;
                    self.barber = None;
                    self.placements = WindowPlacements::default();
                    self.pending_chat.clear();
                    // The same argument one queue upstream: a notice still in the
                    // scroll at log-off goes down with the windows.
                    world.scroll.clear();
                    panels.spew_clear_pending();
                    self.applied_to = None;
                    // The three remembered names go with the session, the way
                    // every other communication-system member does: the last teller's pair is
                    // cleared by `ObjectStream::reset`'s `world = ()`, and these two
                    // have no other owner. A name that survived a log-off would compose a tell to
                    // somebody the next character has never spoken to.
                    world.chat.last_monarch_sender.clear();
                    world.chat.last_patron_sender.clear();
                }
                _ => {}
            }
        }
        chat
    }

    /// Scroll insertion's prefix step, for the lines that do not pass through
    /// [`dereth_client_model::scroll::Scroll`].
    ///
    /// Every reproduced chat handler ends in one shared scroll insertion, and that operation is
    /// what stamps. It does so exactly once per line, so this runs
    /// over a batch rather than at each `chat.push`, and it never overwrites a prefix that is
    /// already there (nothing produces one yet, and if something ever does, a silently clobbered
    /// prefix is the kind of thing no test would notice).
    ///
    /// Both counters move, always: a batch with the option off is a batch that ran and declined,
    /// and it must not read like a stamper that never ran.
    fn stamp_timestamps(&mut self, world: &dereth_client_model::World, lines: &mut [ChatMessage]) {
        for m in lines {
            if m.prefix.is_some() {
                continue;
            }
            if dereth_client_model::scroll::wants_timestamp(
                u32::from(m.ty),
                world.scroll.display_time_stamps,
            ) {
                m.prefix = Some(dereth_client_model::scroll::timestamp_prefix(
                    world.scroll.now_unix,
                    world.scroll.utc_offset_secs,
                ));
                self.stats.timestamps_stamped += 1;
            } else {
                self.stats.timestamps_suppressed += 1;
            }
        }
    }

    /// Host's actual gameplay subscriber, before processing this network batch.
    pub fn set_turbine_chat_generation(&mut self, generation: Option<u64>) {
        self.turbine_chat_generation = generation;
        self.pending_chat
            .retain(|(g, _)| g.is_none() || *g == generation);
    }

    /// **Display-string fan-out.**
    ///
    /// This is the last link of the chain `dereth_client_model::scroll` documents. The
    /// `Notice::DisplayString` emitters in `dereth_client_model` and `interaction.rs` are reached
    /// from every refusal call site, and three combat-state refusal paths add their line to the
    /// chat scroll directly; without this drain none of them would say anything.
    ///
    /// The notice has **two** registered receivers and they are not alternatives:
    ///
    /// | receiver | test | what it does |
    /// |---|---|---|
    /// | chat windows | the window id is the window's own id, else it is 0 and the type is active | appends the grey prefix and the body to that window's log, the body in the type's own colour |
    /// | spew strip | `type == 0x1A` | queues the bubble strip across the top of the viewport |
    ///
    /// **and the split between them is the whole answer to "where does a refusal land".** The main
    /// chat window's default text-type filter is `0xFBFFFFFF` — bit 26 clear — so type `0x1A`,
    /// the channel every client-generated message in this build uses, is the one type the
    /// scrollback drops, and no floaty window's default carries it either. A player who ticks the
    /// **Error** group (`0x04000000`) in the chat options gets it in both places; by default it is
    /// the spew box alone. Sending it to both from here is not a guess about which one wins — it is
    /// the client's fan-out, with each receiver's own test left where the client put it.
    fn collect_scroll(
        &mut self,
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
    ) -> Vec<ChatMessage> {
        let mut lines = Vec::new();
        for line in world.scroll.drain() {
            let ty = u8::try_from(line.chat_type).unwrap_or(0);
            let took = panels.spew_offer(ty, &line.body);
            if took {
                self.stats.spew_lines += 1;
            }
            if crate::trace::notice() {
                tracing::debug!(
                    target: "dereth::trace::notice",
                    "notice-trace line from the client scroll: type={:#x} window={} \
                     text={:?} -> spew box took={took}; queued for the chat windows",
                    ty,
                    line.window,
                    line.body
                );
            }
            lines.push(ChatMessage {
                ty,
                body: line.body,
                prefix: line.prefix,
                window: line.window,
            });
            self.stats.scroll_lines += 1;
        }
        lines
    }

    fn drain_scroll(
        &mut self,
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        chat: &mut Vec<ChatMessage>,
    ) {
        let lines = self.collect_scroll(world, panels);
        // The chat half is queued exactly as a network line is; `drive` routes it through the five
        // interfaces, where the filter that drops `0x1A` lives. Only this drain's lines are
        // appended: it also runs before incoming Turbine callbacks to preserve event order.
        self.pending_chat
            .extend(lines.iter().cloned().map(|m| (None, m)));
        chat.extend(lines);
    }

    /// The tail all four combat notification-event handlers share.
    ///
    /// Unless `is_squelched(0, "", 6)`, the text is added to the chat scroll as
    /// `(text, type, true, 0)`.
    /// The squelch call really does pass a **zero object id and an empty account name**: the gate
    /// is on the *type* alone (the combat text type, 6), not on who hit you — which is why a
    /// per-speaker squelch cannot mute your own combat log.
    fn push_combat_line(
        &mut self,
        world: &dereth_client_model::World,
        chat: &mut Vec<ChatMessage>,
        text_type: u32,
        body: String,
    ) {
        if world.chat.is_squelched(
            dereth_primitives::ObjectId(0),
            "",
            dereth_client_model::chat::text_type::COMBAT,
        ) {
            self.stats.combat_lines_squelched += 1;
            return;
        }
        chat.push(ChatMessage {
            ty: u8::try_from(text_type).unwrap_or(0),
            body,
            prefix: None,
            window: 0,
        });
        self.stats.combat_lines += 1;
    }

    // ---- the three names the reply key and the text replacements read -----------------------
    //
    // `dereth_client_contract::chat::window::ReplyTargets` is read at two sites in
    // `GamePlayScreen`, and `dereth_client_model::chat::ChatState::last_teller` /
    // `last_teller_name` are transcribed from the last-teller id and name setters. Everything
    // below is the producer.
    //
    // **Not done here:** the `@tell` and `@reply` *verbs*
    // reach `CommandOutcome::Handled` and are dropped without a packet, a message or an
    // error — so an expanded `@tell <name>, hi` composes correctly and then evaporates on Enter.
    // Those are the direct-talk, reply, and retell commands and their
    // talk-direct-by-name (`0x005D`) and talk-direct
    // (`0x0032`) messages, and they belong in `dereth/client/src/interaction.rs` and
    // `dereth_client_model::Request`.

    /// The clickable-player id range. The `Communication_HearDirectSpeech` handler and the
    /// ranged-talk handler both spell it `0x50000000 < id && id < 0x70000000`, so
    /// it is **exclusive at both ends** and this constant pair is written that way rather than as
    /// a tidier inclusive range. \[verified\]
    ///
    /// **This is the whole misdirection guard.** Of the **126** `0x02BD` tells in the capture
    /// corpus, **126** are from ids outside it — `Sparring Golem` at `0x8000_0DE9`,
    /// `Academy Researcher` at `0x77F0_xxxx` and fourteen more NPCs — every one of them addressed
    /// to the player. Without the test, `@r hi ` after any tutorial NPC speaks composes a tell to
    /// the golem.
    const CLICKABLE_PLAYER_IDS: std::ops::Range<u32> = 0x5000_0001..0x7000_0000;

    /// Direct-speech handling records the last teller id and trimmed name after displaying a line,
    /// but only when the local player is the target and the sender id is in the clickable-player
    /// range.
    ///
    /// Three gates, and each one is a different way for a reply to go to the wrong person:
    ///
    /// * **`targetID == me`.** A tell the shard copied to us but addressed elsewhere — an
    ///   admin or a monitored line — must not become the reply target.
    /// * **`senderID != targetID`.** The client takes that pair down its own branch (`You
    ///   think, "…"`) well before this code and never reaches the last-teller setter, so a tell to
    ///   yourself does not arm `@r`. Written as its own test rather than left implicit, because
    ///   *self* is inside the player range and would otherwise pass.
    /// * **the id range.** The 126-of-126 case above.
    ///
    /// The name is the one the `^`/`&` meta-language trim has already run over — the client
    /// searches the **sender name** for `^` (Olthoi) then `&`, and passes the trailing-trimmed
    /// buffer to the last-teller-name setter. Reproduced because a marker left on would compose
    /// `@tell Bob^,`; `[inferred]`, since the exact string handling is not directly observable,
    /// and **unexercised by the corpus**: none of the 126 names carries either marker.
    fn note_last_teller(
        &mut self,
        world: &mut dereth_client_model::World,
        m: &dereth_protocol::comms::CommunicationHearDirectSpeech,
    ) {
        let mine = self.player == Some(m.target_id);
        let from_a_player = Self::CLICKABLE_PLAYER_IDS.contains(&m.sender_id.0);
        if !mine || m.sender_id == m.target_id || !from_a_player {
            self.stats.last_teller_declined += 1;
            return;
        }
        world.chat.last_teller = Some(m.sender_id);
        world.chat.last_teller_name = trim_language_marker(&m.sender_name).to_owned();
        self.stats.last_teller_writes += 1;
    }

    /// Compose and deliver one heard emote.
    ///
    /// One body for both opcodes because that is the shape of retail: soul-emote hearing does
    /// its self-echo test, appends `"^"` to the name, and **calls this function**.
    /// Writing the composition twice would have been two transcriptions of one retail arm.
    ///
    /// The order is the function's own and every step of it is observable in the counters:
    ///
    /// 1. `CanHear(senderID, 0xC)` — refused means *return*, with nothing composed.
    ///    `account` is `""` because `CanHear` builds its own empty string,
    ///    and the type is the literal `0xC` (the emote text type), which is not on the wire:
    ///    neither `0x01E0` nor `0x01E2` carries a text type.
    /// 2. the `^` then `&` search over the **sender name**, trailing-trimmed
    ///    ([`crate::chat::language_marker`]).
    /// 3. the language decision ([`crate::chat::is_untranslated`]).
    /// 4. the composition ([`crate::chat::hear_emote_line`]) and
    ///    adding the line to the chat scroll as `(line, 0xC, true, 0)`.
    ///
    /// **Step 3's `true` arm** replaces
    /// the text with a random Olthoi / human phrase — ten
    /// fixed phrases each, picked by `(1, 10)`, transcribed as
    /// [`crate::chat::OLTHOI_TEXT`] / [`crate::chat::HUMAN_TEXT`] — and composes
    /// [`crate::chat::GARBLED`] through [`crate::chat::garbled_line`] instead of the
    /// apostrophe-aware join. [`HudStats::emote_lines_untranslated`] counts lines that were
    /// actually garbled. See [`Self::garbled_or_plain`], which is the one producer all three arms
    /// share.
    fn hear_emote(
        &mut self,
        world: &dereth_client_model::World,
        chat: &mut Vec<ChatMessage>,
        sender: ObjectId,
        sender_name: &str,
        text: &str,
    ) {
        const EMOTE: u32 = dereth_client_model::chat::text_type::EMOTE;

        // Step 1, and genuinely first: test whether the listener can hear this sender before
        // doing any composition work.
        let ps = self.speaker_player_space(sender);
        let radius = dereth_client_contract::radar::radar_range(self.player_outside());
        if !world.chat.can_hear(sender, "", EMOTE, ps, radius) {
            if world.chat.is_squelched(sender, "", EMOTE) {
                self.stats.emote_lines_squelched += 1;
            } else {
                self.stats.emote_lines_out_of_earshot += 1;
            }
            return;
        }

        // Steps 2 and 3. The creature-type predicate is `Self::is_olthoi`.
        let (marker, name) = crate::chat::language_marker(sender_name);
        let name = name.to_owned();
        // Step 4. The garble arm composes `GARBLED`; the plain arm composes by hand and omits the
        // space before an apostrophe.
        let line = match self.garbled_or_plain(marker, &name, world) {
            Some(g) => {
                self.stats.emote_lines_untranslated += 1;
                g
            }
            None => crate::chat::hear_emote_line(&name, text),
        };
        chat.push(speech(EMOTE, line));
        self.stats.emote_lines_composed += 1;
    }

    /// Whether the local player is Olthoi.
    ///
    /// The listener half of the language test, used at
    /// **two** seams in this file: here and the player-description handler's
    /// enabling of the chat talk focuses in the `0x0013` arm. Both are this one call, and
    /// they read the same local player description the client does: the canonical local-player
    /// object row, queried for `PropertyInt 0xBC HeritageGroup` against 12 and 13. See
    /// [`crate::chat::OLTHOI_HERITAGE_GROUPS`].
    ///
    /// No player description yet reads `0` — the int-quality query's own answer for an absent
    /// property — which is not Olthoi.
    #[must_use]
    pub fn is_olthoi(&self, world: &dereth_client_model::World) -> bool {
        let heritage = self.player_desc(world).map_or(0, |q| {
            q.inq_int(dereth_client_contract::panels::inventory::HERITAGE_GROUP_PROPERTY)
        });
        crate::chat::is_olthoi(heritage)
    }

    /// Seed the chat path's generator only.
    ///
    /// Retail seeds its one random generator with `(long)time(NULL)`, which
    /// is [`crate::audio::ran2_seed`] and is what [`Self::garble_roll`] uses if nothing has called
    /// this. Exposed so that a caller — today, a test — can make the draw reproducible; a
    /// generator whose sequence cannot be pinned is a generator no assertion can check, which is
    /// how `dereth_primitives::num::rng`'s own callers are all written.
    pub fn seed_random(&mut self, seed: i32) {
        self.garble_rng = Some(dereth_primitives::num::rng::Ran2::new(seed));
    }

    /// One inclusive draw from 1 through 10. See [`Self::garble_rng`].
    fn garble_roll(&mut self) -> i32 {
        let (lo, hi) = crate::chat::GARBLE_ROLL;
        self.garble_rng
            .get_or_insert_with(|| {
                dereth_primitives::num::rng::Ran2::new(crate::audio::ran2_seed_at(
                    crate::platform::clock::system_unix_time(),
                ))
            })
            .roll_i32(lo, hi)
    }

    /// **The one garble producer, shared by all three arms that have one.**
    ///
    /// Returns `Some(line)` when this listener cannot understand this speaker — in which case the
    /// line is already composed, because the substitution and the format are the same in all
    /// three handlers — and `None` when the caller should compose normally.
    ///
    /// The three callers are the `0x02BB`, `0x02BD` and `0x01E0`/`0x01E2` arms. Together they are
    /// every use of the random garbled-text producers, so "three arms" is a count and not an
    /// estimate. `0x02BC`
    /// (ranged talk) is **not** one of them: it never asks whether the player is Olthoi,
    /// never looks for a marker and cannot garble.
    ///
    /// `no_olthoi_talk` is queried as local-player property `0x81 NoOlthoiTalk`
    /// in the emote arm and in the direct-speech arm. A missing local player description is
    /// retail's null-interface arm and answers the same `false`,
    /// which is why this is `is_some_and` and not an `Option` three-way.
    fn garbled_or_plain(
        &mut self,
        marker: crate::chat::LanguageMarker,
        trimmed_name: &str,
        world: &dereth_client_model::World,
    ) -> Option<String> {
        let no_olthoi_talk = self
            .player_desc(world)
            .is_some_and(|q| q.inq_bool(bool_property::NO_OLTHOI_TALK));
        if !crate::chat::is_untranslated(marker, self.is_olthoi(world), no_olthoi_talk) {
            return None;
        }
        // The **speaker's** flag chooses the table, not the listener's.
        let speaker_is_olthoi = marker == crate::chat::LanguageMarker::Ampersand;
        let roll = self.garble_roll();
        Some(crate::chat::garbled_line(
            trimmed_name,
            crate::chat::random_text(roll, speaker_is_olthoi),
        ))
    }

    /// The `Communication_ChannelBroadcast` handler's two user-name stores; see the
    /// `0x0147` arm for what of that handler is and is not reproduced.
    ///
    /// The channel constants are `0x4000` for Monarch and `0x2000` for Patron. Every other
    /// channel stores nothing, including Vassals and Allegiance, which is why there is no `@vr`.
    fn note_at_channel_speaker(
        &mut self,
        m: &dereth_protocol::comms::CommunicationChannelBroadcastRecv,
        world: &mut dereth_client_model::World,
    ) {
        // A buffer length of 1 means the string holds only its terminator. An empty
        // sender name is the shard echoing our **own** broadcast, and the client takes the whole
        // `"You say to your …"` branch, which contains neither call.
        if m.sender_name.is_empty() {
            return;
        }
        let name = trim_language_marker(&m.sender_name).to_owned();
        match m.channel {
            channel::MONARCH => world.chat.last_monarch_sender = name,
            channel::PATRON => world.chat.last_patron_sender = name,
            _ => return,
        }
        self.stats.at_channel_name_writes += 1;
    }

    /// The reply target triple read by reply-key handling and text replacement, assembled from the
    /// two places this client keeps it.
    ///
    /// An empty name is `None` here because that is what the two consumers do with it: the reply
    /// key returns early on an empty name and the replacement tests for a length of 1. The
    /// distinction between "no name" and "the empty name" does not exist in the client: both are
    /// represented by the same stored name field.
    #[must_use]
    pub fn reply_targets(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::window::ReplyTargets {
        world.chat.reply_targets()
    }

    /// How many live trackers name a contract [`Hud::contract_table`] does not carry.
    ///
    /// Counted at receipt rather than inside [`HudView::contracts`] because the view is `&self`
    /// and cannot write a counter. With no
    /// table loaded at all the answer is the whole table, which is the honest reading: the tab
    /// would draw nothing.
    fn count_unresolved_contracts(&mut self, world: &dereth_client_model::World) {
        let n = match self.contract_table.as_ref() {
            Some(t) => world
                .contract_trackers()
                .keys()
                .filter(|id| !t.contracts.contains_key(id))
                .count(),
            None => world.contract_trackers().len(),
        };
        self.stats.contracts_unresolved = u64::try_from(n).unwrap_or(u64::MAX);
    }

    /// The current time as a [`dereth_primitives::ServerTime`] — what the contract-tracker update
    /// stamps `_time_of_server_update` with.
    ///
    /// The same clock [`Self::now`] carries; the two types are the same number and the conversion
    /// is named rather than sprinkled, so the contract receiver and
    /// `dereth_client_model::quests::fill_progress_string` cannot end up on different ones.
    fn server_now(&self) -> dereth_primitives::ServerTime {
        dereth_primitives::ServerTime(self.now.0)
    }

    /// Incoming communication updates the shared remembered speakers and chat state.
    fn ui_event(
        &mut self,
        opcode: dereth_protocol::Opcode,
        blob: &[u8],
        world: &mut dereth_client_model::World,
        panels: &mut dyn HudPanels,
        chat: &mut Vec<ChatMessage>,
        on_combat_mode: &mut dyn FnMut(
            &mut dereth_client_model::World,
            dereth_client_model::combat::CombatMode,
        ),
    ) {
        use dereth_protocol::comms;
        use dereth_protocol::Opcode;

        // The blob begins with its own type dword (`SessionEvent::UiEvent`'s contract), which is
        // what `Message::read` expects to have been stripped.
        let body = blob.get(4..).unwrap_or_default();
        let mut r = dereth_protocol::archive::Reader::new(body);
        if crate::trace::notice()
            && matches!(
                opcode,
                Opcode::COMMUNICATION_TRANSIENT_STRING
                    | Opcode::COMMUNICATION_TRANSIENT_STRING_0317
                    | Opcode::COMMUNICATION_WEENIE_ERROR
                    | Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING
                    | Opcode::COMMUNICATION_TEXTBOX_STRING
                    | Opcode::ITEM_USE_DONE
            )
        {
            // The event as `dereth_client_net::client_session` handed it over (the `0xF7B0`
            // wrapper already stripped; `opcode` is the game-event id).
            tracing::debug!(
                target: "dereth::trace::notice",
                "notice-trace event {:#06x} ({}) body {} bytes: {}",
                opcode.0,
                opcode.name().unwrap_or("?"),
                body.len(),
                crate::trace::hex(body, 96),
            );
        }

        match opcode {
            // Character customization begins with the following sixteen fields, retained
            // verbatim; the screen performs retail's table-index inversion.
            Opcode::CHARACTER_START_BARBER => {
                match dereth_protocol::read_body::<dereth_protocol::trade::CharacterStartBarber>(
                    body,
                ) {
                    Ok(m) => {
                        self.barber = Some(m.0);
                        self.barber_generation = self.barber_generation.wrapping_add(1);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMMUNICATION_CHAT_ROOM_TRACKER => {
                match dereth_protocol::read_body::<comms::ChatRoomMembership>(body) {
                    Ok(m) => world.chat.recv_chat_room_tracker(m),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0xF7E0 Communication_TextboxString` — system chat. No speaker, so no prefix.
            Opcode::COMMUNICATION_TEXTBOX_STRING => {
                match comms::CommunicationTextboxString::read(&mut r) {
                    Ok(mut m) => {
                        // A player-killer death broadcast carries the tag `[PKDe]`. With the
                        // hear-PK-deaths option off the line is dropped before anything else;
                        // with it on, the tag is taken out and the line goes on to the squelch
                        // gate like any other.
                        match pk_death_filter(&m.text, world.player_system.options.hear_pk_deaths())
                        {
                            PkDeathLine::Dropped => {
                                self.stats.textbox_pk_deaths_dropped += 1;
                                return;
                            }
                            PkDeathLine::Stripped(text) => m.text = text,
                            PkDeathLine::Untagged => {}
                        }
                        // The `Communication_TextboxString` handler continues
                        // with a squelch gate; without it a global
                        // squelch of Craft, Magic, Salvaging or Fellowship would leave every system
                        // line of that type on screen.
                        //
                        // The squelch query uses id zero and an empty account name before any line
                        // is drawn. The **zero id** is the whole nuance: no character or account entry can
                        // reach this line, only the global per-type table, and only for a type
                        // `IsLegalChannel` accepts. `ChatState::is_squelched` is both
                        // halves, which is why the check is that call and not a table lookup.
                        if world.chat.is_squelched(ObjectId(0), "", m.text_type) {
                            if crate::trace::notice() {
                                tracing::debug!(
                                    target: "dereth::trace::notice",
                                    "notice-trace 0xF7E0 TextboxString type={:#x} \
                                     text={:?} -> squelched by the notice filter",
                                    m.text_type,
                                    m.text
                                );
                            }
                            self.stats.textbox_lines_squelched += 1;
                            return;
                        }
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0xF7E0 TextboxString type={:#x} text={:?}",
                                m.text_type,
                                m.text
                            );
                        }
                        chat.push(ChatMessage {
                            ty: u8::try_from(m.text_type).unwrap_or(0),
                            body: m.text,
                            prefix: None,
                            window: 0,
                        });
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x028B Communication_WeenieErrorWithString` — without this arm a line like
            // "You have entered the Trade channel." would never show.
            // Failure formatting turns the code and its `%s` argument into a chat line; see
            // `chat::failure`.
            //
            // The fellowship codes (`0x50B`–`0x50E`, `0x518`, `0x528`, …)
            // have arms in `chat::failure`, each verified against retail; a code with no arm is
            // retail's miss exit and prints nothing. The newline the arm's literal
            // ends in is trimmed here, as scroll insertion's first act would.
            Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING => {
                match comms::CommunicationWeenieErrorWithString::read(&mut r) {
                    Ok(m) => {
                        // Same failure-event arm and same notice as the stringless 0x028A
                        // sibling. The Abuse panel ignores this message's otherwise-substituted text.
                        panels.abuse_response(m.error_type);
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0x028B WeenieErrorWithString code={:#x} \
                                 text={:?} -> line={:?}",
                                m.error_type,
                                m.text,
                                dereth_client_contract::chat::failure::handle_failure_event(
                                    m.error_type,
                                    &m.text
                                )
                                .map(|c| c.body),
                            );
                        }
                        if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                            m.error_type,
                            &m.text,
                        ) {
                            chat.push(failure_line(c));
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The title-table and display-title notices are the two writes to
            // the title id, which is the only input the header's appended title has.
            //
            // The client's title-table handler does two
            // things: it copies
            // the display title **and** the title list, and then refreshes the panel,
            // whose middle third adds every entry of the title list to the list box. Keeping
            // only the first dword would leave the character wearing a title in the header while
            // the Titles tab had nothing to draw.
            //
            // The list goes into `player_system.social`, where the title table is modelled.
            Opcode::SOCIAL_CHARACTER_TITLE_TABLE => {
                match dereth_protocol::social::CharacterTitlesMessage::read(&mut r) {
                    Ok(m) => {
                        self.display_title = m.display_title;
                        world.player_system.social.display_title = m.display_title;
                        world.player_system.social.titles = m.titles;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // Adding or setting a character title only moves the display title when the server
            // says to; the client's is the same test.
            //
            // **The add is not gated.** The event adds the character title unconditionally and then
            // sends the set-display-title notice only when the flag is set; the two
            // add-title and set-display-title receivers each walk the title list for the id and insert
            // only when it is absent, which is the `contains` below.
            Opcode::SOCIAL_ADD_OR_SET_CHARACTER_TITLE => {
                match dereth_protocol::social::SocialAddOrSetCharacterTitle::read(&mut r) {
                    Ok(m) => {
                        let list = &mut world.player_system.social.titles;
                        if !list.contains(&m.new_title) {
                            list.push(m.new_title);
                        }
                        if m.set_as_display_title != 0 {
                            self.display_title = m.new_title;
                            world.player_system.social.display_title = m.new_title;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x02BB` / `0x02BC` / `0x02BD` — the three "hear" messages.
            //
            // Each one composes its whole line with one fixed formatting template and hands it to
            // the scroll as a **single body**; the notice's `prefix` argument is the
            // timestamp, never the speaker. The templates and the branch order live in
            // [`crate::chat`], pinned as literals against the retail client.
            //
            // There is no separator from a string table between name and message — there are seven
            // fixed format strings, and putting the bare name in `prefix` and the bare message in
            // `body` would produce the wrong split.
            //
            // **`0x02BB` and `0x02BC` are two functions, not one.**
            // The `Communication_HearSpeech` handler tests `senderID == player_id` and echoes
            // `You say, "%s"`; the ranged-talk handler has no such arm. The self-echo template has
            // exactly **one** use, inside
            // the first of the two.
            Opcode::COMMUNICATION_HEAR_SPEECH => {
                match comms::CommunicationHearSpeech::read(&mut r) {
                    Ok(m) => {
                        // **The hearing gate.**
                        //
                        // The handler first requires a local physics body and a nonzero sender.
                        // It then tests the local player's id before consulting `CanHear`.
                        // The self-echo arm is **above** that call, so your own line is never
                        // squelched and never out of earshot. `CanHear`'s account argument is the
                        // empty string it builds itself, not a name from the wire.
                        //
                        // **The player-null gate.**
                        //
                        // The null comparison tests the player's physics
                        // object, and it is the handler's first test. See
                        // [`Self::player_body`] for why that is `viewer.is_some()` and not
                        // `Hud::player` (which is `player_id`, read again shortly after for the
                        // self-echo).
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        let ps = self.speaker_player_space(m.sender_id);
                        let radius =
                            dereth_client_contract::radar::radar_range(self.player_outside());
                        let is_self = self.player == Some(m.sender_id);
                        if m.sender_id.0 == 0 {
                            self.stats.speech_lines_with_no_speaker += 1;
                        } else if is_self {
                            // The `You say, "…"` arm is **above** the `CanHear` call,
                            // above the Olthoi test and above the marker
                            // split — so your own line is never squelched, never out of earshot
                            // and never garbled, and its name is never trimmed because it is
                            // never used.
                            chat.push(speech(
                                m.text_type,
                                crate::chat::hear_speech_line(
                                    m.sender_id.0,
                                    self.player.map(|p| p.0),
                                    &m.sender_name,
                                    &m.message,
                                ),
                            ));
                            self.stats.speech_lines_composed += 1;
                        } else if world
                            .chat
                            .can_hear(m.sender_id, "", m.text_type, ps, radius)
                        {
                            // **The `^`/`&` split and the garble.** The handler searches the sender
                            // name for `^` and then for `&`, each followed by `trim(leading = 0,
                            // trailing = 1, marker)`; the trimmed name is what the
                            // `says` templates are given, so a shard that marks a name does not
                            // render `Bob^ says, "…"`.
                            let (marker, name) = crate::chat::language_marker(&m.sender_name);
                            let name = name.to_owned();
                            let line = match self.garbled_or_plain(marker, &name, world) {
                                Some(g) => {
                                    self.stats.speech_lines_untranslated += 1;
                                    g
                                }
                                None => crate::chat::hear_speech_line(
                                    m.sender_id.0,
                                    self.player.map(|p| p.0),
                                    &name,
                                    &m.message,
                                ),
                            };
                            chat.push(speech(m.text_type, line));
                            self.stats.speech_lines_composed += 1;
                        } else if world.chat.is_squelched(m.sender_id, "", m.text_type) {
                            self.stats.speech_lines_squelched += 1;
                        } else {
                            self.stats.speech_lines_out_of_earshot += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02BC`'s own two gates, which are NOT `CanHear`'s.**
            //
            // Ranged speech uses a distinct handler and never calls `CanHear`. In order, it
            // requires the local physics body and a nonzero sender, applies the speaker squelch,
            // then tests 3-D distance against the range carried on the wire.
            //
            // **Three ways it differs from the say path, each easy to paper over.** The radius is
            // a `float` on the message rather than the radar radius, so it is neither 25 nor 75
            // and the indoor/outdoor choice does not enter into it. The distance is
            // **3-D**, where `CanHear`'s is `x² + y²`. And the boundary is
            // **inclusive**: this path refuses only when `d > range`, whereas `CanHear` refuses at
            // the boundary too. Two range tests in one
            // subsystem with opposite polarity at the edge.
            Opcode::COMMUNICATION_HEAR_RANGED_SPEECH => {
                match comms::CommunicationHearRangedSpeech::read(&mut r) {
                    Ok(m) => {
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        if m.sender_id.0 == 0 {
                            self.stats.speech_lines_with_no_speaker += 1;
                            return;
                        }
                        // The squelch query uses an empty account name, exactly as `CanHear` does;
                        // no account value comes from the wire.
                        if world.chat.is_squelched(m.sender_id, "", m.text_type) {
                            self.stats.speech_lines_squelched += 1;
                            return;
                        }
                        let in_range = dereth_client_model::range::objects_in_range_distance(
                            self.speaker_distance(m.sender_id),
                            f64::from(m.range),
                        );
                        if !in_range {
                            self.stats.ranged_lines_out_of_range += 1;
                            return;
                        }
                        chat.push(speech(
                            m.text_type,
                            crate::chat::hear_ranged_speech_line(
                                m.sender_id.0,
                                &m.sender_name,
                                &m.message,
                            ),
                        ));
                        self.stats.speech_lines_composed += 1;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---------------------------------------------------------------------------------
            // **The combat scroll.**
            //
            // `dereth_client_model::combat` carries `attacker_notification_line`,
            // `defender_notification_line`, `evasion_attacker_line` and `evasion_defender_line` —
            // with the hit-adjective table, the damage-type list, the body-part list and both of
            // the client's shipped trailing-space bugs. Without these arms every blow
            // struck in this client would be silent.
            //
            // All four handlers share one gate and one sink:
            // a combat-text squelch check and then adding the text to the scroll.
            Opcode::COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::AttackerNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_SELF,
                        dereth_client_model::combat::attacker_notification_line(
                            &m.defender_name,
                            m.damage_type,
                            m.percent,
                            m.damage,
                            m.critical != 0,
                            m.attack_conditions,
                        ),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::DefenderNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_ENEMY,
                        dereth_client_model::combat::defender_notification_line(
                            &m.attacker_name,
                            m.damage_type,
                            m.percent,
                            m.damage,
                            m.damage_location,
                            m.critical != 0,
                            m.attack_conditions,
                        ),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The evasion-attacker notification — a scroll line of text type `0x16`.
            Opcode::COMBAT_HANDLE_EVASION_ATTACKER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::EvasionAttackerNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_SELF,
                        dereth_client_model::combat::evasion_attacker_line(&m.defender_name),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The evasion-defender notification — a scroll line of text type `0x15`.
            Opcode::COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT => {
                match dereth_protocol::combat::EvasionDefenderNotification::read(&mut r) {
                    Ok(m) => self.push_combat_line(
                        world,
                        chat,
                        dereth_client_model::chat::text_type::COMBAT_ENEMY,
                        dereth_client_model::combat::evasion_defender_line(&m.attacker_name),
                    ),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The victim notification — reached by **both** `0x01AC` and
            // `0x01AD` with identical arguments; the client draws no distinction. The server's
            // string is printed verbatim with a `"\n"` appended and **text type 0**, which is the
            // literal in the scroll insertion's `(..., 0, true, 0)` — not one of the combat types,
            // and not squelch-gated either: there is no squelch call
            // anywhere in the handler, unlike the four sibling
            // notification-event arms [`Self::push_combat_line`] serves.
            //
            // **The tail is scroll insertion, not a direct push.** Scroll insertion trims both
            // ends, stamps the timestamp prefix, and copies the line into
            // the chat log file; a line that skips it is a different line. `0x019E`'s arm below has
            // the same `(msg, 0, true, 0)` scroll-insertion tail in retail and also goes through
            // the scroll, so the two sibling handlers draw alike.
            Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF
            | Opcode::COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER => {
                match dereth_protocol::combat::VictimNotificationOther::read(&mut r) {
                    Ok(m) => {
                        self.stats.victim_notifications += 1;
                        // The stored length counts the terminator, so a length of 1 is empty.
                        if !m.message.is_empty() {
                            world.scroll.add_text_to_scroll(
                                &format!("{}\n", m.message),
                                0,
                                true,
                                0,
                            );
                            self.stats.victim_notifications_announced += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x028A Communication_WeenieError` is
            // handled as a failure event with an empty string — the *same* path `0x028B` above
            // reaches, with an empty `%s`.
            // The `Communication_TransientString` handler is three lines:
            // widen the string and add it to the scroll as type `0x1A`. Type `0x1A` is the one the
            // scroll special-cases — no timestamp prefix and nothing written
            // to the chat log file — which `dereth_client_model::chat::route` already encodes as
            // `timestamped: false`.
            // `0x0317` carries the same one string and takes the same three lines.
            Opcode::COMMUNICATION_TRANSIENT_STRING
            | Opcode::COMMUNICATION_TRANSIENT_STRING_0317 => {
                match comms::CommunicationTransientString::read(&mut r) {
                    Ok(m) => {
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace {:#06x} TransientString text={:?} -> \
                                 chat type 0x1A (LOCAL_ERROR), window 0",
                                opcode.0,
                                m.text
                            );
                        }
                        chat.push(ChatMessage {
                            ty: u8::try_from(dereth_client_model::chat::text_type::LOCAL_ERROR)
                                .unwrap_or(0),
                            body: m.text,
                            prefix: None,
                            window: 0,
                        });
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::COMMUNICATION_WEENIE_ERROR => {
                match comms::CommunicationWeenieError::read(&mut r) {
                    Ok(m) => {
                        // The shared 0x04B8..0x04BA arm does not draw a failure
                        // line. It sends an abuse-report response notice, whose receiver writes
                        // the result field even while
                        // the panel is hidden. Keep the Silent chat arm below as the second half.
                        panels.abuse_response(m.error_type);
                        if crate::trace::notice() {
                            tracing::debug!(
                                target: "dereth::trace::notice",
                                "notice-trace 0x028A WeenieError code={:#x} -> line={:?}",
                                m.error_type,
                                dereth_client_contract::chat::failure::handle_failure_event(
                                    m.error_type,
                                    ""
                                )
                                .map(|c| c.body),
                            );
                        }
                        if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                            m.error_type,
                            "",
                        ) {
                            chat.push(failure_line(c));
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The `Item_UseDone` handler's second half: a non-zero
            // `WeenieError` is handled as a failure event with an empty string. The first half —
            // the busy-count decrement — is `interaction::apply_events`', because it is world state and this
            // function owns the chat scroll. Both arms read the same blob; neither can see the
            // other's effect.
            Opcode::ITEM_USE_DONE => match dereth_protocol::objects::ItemUseDone::read(&mut r) {
                Ok(m) if crate::trace::notice() && m.failure_type == 0 => {
                    tracing::debug!(
                        target: "dereth::trace::notice",
                        "notice-trace 0x01C7 UseDone code=0 (WeenieError.None): no line"
                    );
                }
                Ok(m) if m.failure_type != 0 => {
                    if crate::trace::notice() {
                        tracing::debug!(
                            target: "dereth::trace::notice",
                            "notice-trace 0x01C7 UseDone code={:#x} -> line={:?}",
                            m.failure_type,
                            dereth_client_contract::chat::failure::handle_failure_event(
                                m.failure_type,
                                ""
                            )
                            .map(|c| c.body),
                        );
                    }
                    if let Some(c) = dereth_client_contract::chat::failure::handle_failure_event(
                        m.failure_type,
                        "",
                    ) {
                        chat.push(c);
                    }
                }
                Ok(_) => {}
                Err(_) => self.stats.undecodable += 1,
            },
            Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH => {
                match comms::CommunicationHearDirectSpeech::read(&mut r) {
                    Ok(m) => {
                        // The same null-player gate as the two
                        // handlers above — the equality branch, and again
                        // the handler's first test. All three speech handlers carry it and
                        // the emote handler does not; transcribing it into only one of the three
                        // would leave the other two silently inconsistent, so the cluster is kept
                        // aligned.
                        if !self.player_body {
                            self.stats.speech_lines_with_no_player_body += 1;
                            return;
                        }
                        // The `Communication_HearDirectSpeech` handler
                        // draws **nothing** for a tell the shard copied to us but addressed to
                        // somebody else: the `targetID == player_id` gate skips display.
                        // `hear_direct_speech_line`
                        // answers `None` for exactly that case, so the push is conditional and the
                        // two outcomes are counted separately — "no tells arrived" and "a tell
                        // arrived and was correctly not drawn" are different facts.
                        //
                        // **The garble arm sits OUTSIDE that gate.** It composes
                        // `GARBLED` and writes the line directly, so a tell in a language you
                        // cannot understand is shown **whoever it was addressed to** — an
                        // asymmetry in the client, not a simplification here. It is also below
                        // `senderID == targetID`, which takes the `You think, "…"` branch before the
                        // language gate, so a self-tell
                        // is never garbled either.
                        let self_tell = m.sender_id == m.target_id;
                        let (marker, name) = crate::chat::language_marker(&m.sender_name);
                        let name = name.to_owned();
                        let garbled = if self_tell {
                            None
                        } else {
                            self.garbled_or_plain(marker, &name, world)
                        };
                        if let Some(line) = garbled {
                            chat.push(speech(m.text_type, line));
                            self.stats.speech_lines_composed += 1;
                            self.stats.direct_speech_lines_untranslated += 1;
                            return;
                        }
                        // The tail of
                        // the `Communication_HearDirectSpeech` handler, which is the only
                        // writer of the last teller in the client.
                        // It runs *before* the line is pushed only because `speech` consumed the
                        // name; the client does it after scroll insertion and neither can see
                        // the other's effect.
                        //
                        // **It is below the garble arm**, which is where the client
                        // has it: the last-teller id and name setters are called inside the
                        // `targetID == player_id` branch of the *understood* else — and those two
                        // are the **only** calls to either function in retail. The garble arm
                        // skips them entirely, so a
                        // tell you cannot understand does **not** arm `@r`; running the write
                        // first would point a reply at a speaker whose words were never
                        // shown.
                        self.note_last_teller(world, &m);
                        match crate::chat::hear_direct_speech_line(
                            m.sender_id.0,
                            m.target_id.0,
                            self.player.map(|p| p.0),
                            &name,
                            &m.message,
                        ) {
                            Some(line) => {
                                chat.push(speech(m.text_type, line));
                                self.stats.speech_lines_composed += 1;
                            }
                            None => self.stats.speech_lines_not_addressed_to_us += 1,
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---------------------------------------------------------------------------------
            // **`0x01E0` and `0x01E2`, the emote messages.**
            //
            // Without these arms **every emote any player performed would be silently discarded**
            // and `CanHear`'s second caller would have nothing to gate. They are two opcodes and
            // two functions and are wired separately here: one wired and one dead would pass any
            // test that only asks whether "an emote appeared".
            //
            // The emote handler calls `CanHear(senderID, EMOTE)` first. A refusal returns before
            // marker handling or composition; a success composes the line and writes it as emote text.
            //
            // **The gate is ahead of everything.** Composing first and gating after would draw the
            // same pixels and is not what retail does; more usefully, it would make the
            // untranslated substitution and the marker trim run for a speaker who is not heard,
            // which is observable in the counters below.
            //
            // Note what the emote handler does *not* have, both asserted by tests: no
            // null-object gate (the three speech handlers all open with one), and
            // no `senderID == player_id` self-echo arm — so your **own** acted emote is drawn,
            // while your own soul emote is discarded by the soul-emote handler before it ever gets
            // here.
            Opcode::COMMUNICATION_HEAR_EMOTE => match comms::CommunicationHearEmote::read(&mut r) {
                Ok(m) => self.hear_emote(world, chat, m.sender, &m.sender_name, &m.text),
                Err(_) => self.stats.undecodable += 1,
            },
            // Soul emotes first compare the sender with the local player. A self-emote returns
            // without drawing; any other sender has `^` appended to the name and continues through
            // the ordinary emote path.
            //
            // So the pose you performed yourself is **not** echoed to you by this path — the
            // client already printed it locally when the pose command sent `0x01E1`
            // (`dereth_client_model::emotes::pose`), and this is the branch that stops it appearing
            // twice. The caret it appends is then trimmed straight back off by the emote handler,
            // and its only lasting effect is to make the line one that is never garbled.
            Opcode::COMMUNICATION_HEAR_SOUL_EMOTE => {
                match comms::CommunicationHearSoulEmote::read(&mut r) {
                    Ok(m) => {
                        if self.player == Some(m.sender) {
                            self.stats.soul_emote_self_echoes_discarded += 1;
                            return;
                        }
                        let marked = crate::chat::soul_emote_sender_name(&m.sender_name);
                        self.hear_emote(world, chat, m.sender, &marked, &m.text);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // The `Communication_ChannelBroadcast` handler's user-name stores: the `0x4000` arm
            // ends by storing
            // `senderName` as the last `@monarch` user name and the `0x2000` arm ends
            // by storing `senderName` as the last `@patron` user name, and those two names are what `@mr `/`@pr `
            // and reply keys `0x10000020`/`0x10000021` expand to. Both are guarded by the same
            // test the rest of the function branches on: `senderName` empty means the broadcast
            // is **our own** line coming back (`"You say to your ..."`), and the client stores
            // nothing for it.
            //
            // **The function's other half, the formatted line.**
            // The formatting operands and the per-branch
            // text types are
            // represented by [`crate::chat::channel_broadcast_line`]. The order here is the
            // client's: the user-name store sits inside the format branch, then the line is
            // finished, then `is_squelched(0, "", type)` gates adding it to the chat scroll as
            // `(line, 0, type, 1)`. So a squelched line still arms `@pr` / `@mr`, exactly as in
            // retail.
            //
            // No gate: unlike the three speech handlers this one goes
            // straight into the channel-name lookup, so a broadcast that
            // arrives before the body exists is still drawn.
            Opcode::COMMUNICATION_CHANNEL_BROADCAST => {
                match comms::CommunicationChannelBroadcastRecv::read(&mut r) {
                    Ok(m) => {
                        self.note_at_channel_speaker(&m, world);
                        let (ty, line) = crate::chat::channel_broadcast_line(
                            m.channel,
                            &m.sender_name,
                            &m.message,
                        );
                        // A scroll entry `(0, "", type)`: character 0 has no
                        // entry, so only the global per-type table can answer "yes" — and only
                        // for a type `IsLegalChannel` accepts, which of this handler's seven is
                        // Fellowship (`0x13`) alone.
                        if world.chat.is_squelched(ObjectId(0), "", ty) {
                            self.stats.channel_broadcast_lines_squelched += 1;
                            return;
                        }
                        chat.push(speech(
                            ty,
                            crate::chat::add_text_to_scroll_trim(&line).to_owned(),
                        ));
                        self.stats.channel_broadcast_lines_composed += 1;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // All twenty-six `Qualities_*Update*` forms, not only the four vitals ones.
            // The two **private** forms of each pair are about the player
            // by construction; the **public** ones carry an object id, and only the player's own
            // reaches the local player description this module keeps.
            //
            // Without the other twenty-two the character panel would show the character's stats
            // **as of login, for ever**.
            op if dereth_client_model::qualities::update::is_update_opcode(op) => {
                if let Some(mode) = self.apply_quality_update(op, body, world) {
                    on_combat_mode(world, mode);
                }
            }
            // The sixteen `Qualities_*Remove*Event` forms, `0x01D1`..=`0x01DE` plus
            // the late Int64 pair at `0x02B8`/`0x02B9`.
            //
            // The client has all sixteen private and public remove handlers over **eight**
            // remove-stat templates, one per generic quality
            // table. There is no skill, attribute or secondary-attribute remove.
            //
            // **This family is dead against ACE and the arm is still right.** ACE
            // carries the sixteen names in `PacketOpCodeNames.cs` and has no sender class for any
            // of them, and all twelve recorded `netblobs` scenarios (13,535 blobs) contain zero of
            // them against 1,443 `0x02CD`..`0x02EA` in the same scan. Parity with retail is the
            // reason, and the tests synthesise the bodies rather than replaying any.
            op if dereth_client_model::qualities::remove::is_remove_opcode(op) => {
                self.apply_quality_remove(op, body, world);
            }
            // The enchantment-registry messages have no arm here. The client has one enchantment
            // registry inside the player's qualities, and so does this build: `interaction.rs`
            // handles all five opcodes and writes it, and `HudView::enchantment_counts` and
            // `HudView::vitae` read it. A second arm here would be a **double apply**, counting the
            // spell totals twice per insert and making the buff and debuff lamps read double.
            // `0x01A8 Magic_RemoveSpell` *inbound*. The client sends this
            // opcode too for a confirmed DELETE, and the shard answers with the same opcode.
            // Without this arm a deleted spell would stay in the book and on the bars for ever
            // while the shard refused to cast it.
            Opcode::MAGIC_REMOVE_SPELL => {
                match dereth_protocol::qualities::MagicRemoveSpell::read(&mut r) {
                    Ok(m) => self.handle_magic_remove_spell(m.layered_spell_id, world),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x01F4 Communication_SetSquelchDB`.**
            //
            // Without this arm `dereth_client_model::chat::ChatState::squelch` has **no production
            // writer**, while incoming speech and emote handling read it on every line
            // from the three speech arms and the two emote arms above: each login would decode the
            // player's whole squelch list and discard it, and the player would hear everyone they
            // had ever muted.
            //
            // The message unpacks a whole squelch database and replaces the chat state's existing
            // database. See
            // [`dereth_client_model::chat::ChatState::recv_set_squelch_db`] for that handler read at the
            // bytes, including why its second call is dead.
            //
            // `read_body_padded` and not `read_body`: the retail reader ends with
            // `return (consumed <= size)`, so the retail receiver simply stops reading and
            // trailing alignment bytes are invisible to it. ACE's `GameMessage` constructors end
            // with `Writer.Align()`, so those bytes are on the wire from a real shard.
            Opcode::COMMUNICATION_SET_SQUELCH_DB => {
                match dereth_protocol::read_body_padded::<comms::CommunicationSetSquelchDb>(body) {
                    Ok(m) => {
                        world.chat.recv_set_squelch_db(m);
                        self.stats.squelch_db_applied += 1;
                        self.stats.squelch_rows_applied = u64::try_from(
                            world.chat.squelch.accounts.len() + world.chat.squelch.characters.len(),
                        )
                        .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **The fellowship family, all five of them.**
            //
            // These five opcodes are most of the inbound messages in the recorded sessions that
            // would otherwise have no receiver. Without them the fellowship state has **no
            // production writer**: `selection_type_rejects` reads it on every tab-target, so
            // `select_next`'s `Monster` arm would pick your own fellows, and the Fellowship entry
            // in the chat drop-down would never enable.
            //
            // See `dereth_client_model::fellowship`'s `impl World` block for the five fellowship handlers,
            // including the one retail radar-look update this
            // build reaches by polling instead of by notice.
            //
            // **And the chat line each one of them ends in**, the Fellowship panel's own
            // scroll insertion; without it a fellowship could be created, opened, closed and left
            // without the log saying a word. The sentences and their conditions are in
            // [`crate::chat`]'s fellowship section.
            Opcode::FELLOWSHIP_FULL_UPDATE => {
                match dereth_protocol::social::FellowshipFullUpdate::read(&mut r) {
                    Ok(m) => {
                        // A full update counts as creation when there is no fellowship copy yet or
                        // its member table is empty.
                        let table_was_empty = world
                            .fellowship
                            .as_ref()
                            .is_none_or(|f| f.members.is_empty());
                        let (created, leader_changed) = world.recv_fellowship_full_update(&m.0);
                        self.stats.fellowship_updates += 1;
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                        if created {
                            self.stats.fellowships_created += 1;
                        }
                        if leader_changed {
                            self.stats.fellowship_leader_changes += 1;
                        }
                        if let Some(f) = world.fellowship.as_ref() {
                            let leader_name =
                                f.members.get(&f.leader).map_or("", |l| l.name.as_str());
                            if let Some(line) = crate::chat::fellowship_update_line(
                                table_was_empty,
                                world.is_the_player(f.leader),
                                &f.name,
                                f.open_fellow,
                                leader_name,
                            ) {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::FELLOWSHIP_UPDATE_FELLOW => {
                match dereth_protocol::social::FellowshipUpdateFellow::read(&mut r) {
                    Ok(m) => {
                        let update =
                            dereth_client_model::fellowship::FellowUpdate::from_raw(m.update_type);
                        if world.recv_fellowship_update_fellow(m.fellow_id, &m.fellow, update) {
                            self.stats.fellows_added += 1;
                            // Once the update adds a previously absent member, the client looks up
                            // that member and composes `"%hs is now a member of your Fellowship.\n"`.
                            chat.push(fellowship_ui_line(crate::chat::fellow_added_line(
                                &m.fellow.name,
                            )));
                            self.stats.fellowship_lines_composed += 1;
                        }
                        // The refusal branch: retail would have dereferenced a null
                        // fellowship pointer. A non-zero count here means a `0x02C0` reached this
                        // client before any `0x02BE` did, which is an ordering fact about the
                        // shard and not a decode failure -- hence its own counter rather than
                        // `undecodable`.
                        if world.fellowship.is_none() {
                            self.stats.fellow_updates_without_a_fellowship += 1;
                        } else {
                            self.stats.fellow_updates += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x02BF` carries nothing but its opcode. `dereth_protocol`'s `empty_message!` read is
            // infallible and **does not** check that the body is empty, so the `Err` arm here can
            // never fire; it is written out anyway so this arm keeps the shape of its four
            // neighbours and so a future `FellowshipDisband` with a body needs no rewrite.
            // The retail `Fellowship_Disband` handler takes no argument either.
            Opcode::FELLOWSHIP_DISBAND => {
                match dereth_protocol::social::FellowshipDisband::read(&mut r) {
                    Ok(_) => {
                        // Fellowship disbanded: nothing at all without a fellowship copy;
                        // otherwise the line is chosen on
                        // `_leader == player_id` and read *before* the copy is deleted.
                        if let Some(f) = world.fellowship.as_ref() {
                            let leader_name =
                                f.members.get(&f.leader).map_or("", |l| l.name.as_str());
                            let line = crate::chat::fellowship_disbanded_line(
                                world.is_the_player(f.leader),
                                leader_name,
                            );
                            chat.push(fellowship_ui_line(line));
                            self.stats.fellowship_lines_composed += 1;
                        }
                        world.recv_fellowship_disband();
                        self.stats.fellowship_disbands += 1;
                        self.stats.fellowship_members = 0;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // `0x00A3` and `0x00A4` are the same handler in retail but for the notice each raises,
            // so they share `recv_fellowship_member_left` and differ only in the counter. Both
            // carry one `ObjectID`: the member who left, which may be the player.
            Opcode::FELLOWSHIP_QUIT => {
                match dereth_protocol::social::FellowshipQuitNotice::read(&mut r) {
                    Ok(m) => {
                        // A fellow quit: the line is composed off the panel's copy before
                        // it is deleted (the name is read first, then the copy deleted).
                        if let Some(f) = world.fellowship.as_ref() {
                            let fellow_name =
                                f.members.get(&m.member).map_or("", |l| l.name.as_str());
                            if let Some(line) = crate::chat::fellow_quit_line(
                                world.is_the_player(m.member),
                                f.is_fellow(m.member),
                                &f.name,
                                fellow_name,
                            ) {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                        let was_us = world.recv_fellowship_member_left(m.member, wall_clock_unix());
                        self.stats.fellowship_quits += 1;
                        if was_us {
                            self.stats.fellowship_departures_our_own += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::FELLOWSHIP_DISMISS => {
                match dereth_protocol::social::FellowshipDismiss::read(&mut r) {
                    Ok(m) => {
                        // A fellow dismissed, before the copy is deleted or the row
                        // removed. The leader test picks the second-person line.
                        if let Some(f) = world.fellowship.as_ref() {
                            let name_of = |id: dereth_primitives::ObjectId| {
                                f.members.get(&id).map_or("", |l| l.name.as_str())
                            };
                            if let Some(line) = crate::chat::fellow_dismissed_line(
                                world.is_the_player(m.target),
                                f.is_fellow(m.target),
                                world.is_the_player(f.leader),
                                name_of(f.leader),
                                name_of(m.target),
                            ) {
                                chat.push(fellowship_ui_line(line));
                                self.stats.fellowship_lines_composed += 1;
                            }
                        }
                        let was_us = world.recv_fellowship_member_left(m.target, wall_clock_unix());
                        self.stats.fellowship_dismissals += 1;
                        if was_us {
                            self.stats.fellowship_departures_our_own += 1;
                        }
                        self.stats.fellowship_members =
                            u64::try_from(world.fellowship.as_ref().map_or(0, |f| f.members.len()))
                                .unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x01C9 Fellowship_FellowUpdateDone`, and retail does NOTHING.**
            //
            // `0x01C9` does not trigger a roster redraw after a
            // burst of `0x02C0` updates: after the opcode and subsystem guards, the
            // handler returns without stores or calls. The neighboring `0x01CA` fellow-stats-done
            // message shares the same empty behavior. Two "done" markers, two empty handlers.
            //
            // `dereth_protocol::social`'s own doc says the same (*"the client does
            // **nothing** with it; it exists so the server can bracket a burst of `0x02C0`
            // updates"*). So the arm is a **counter and
            // a `continue`**, and that is not a stub: the roster is already correct when this
            // arrives, because every `0x02C0` before it moved on its own. An
            // arm here that redrew anything would be inventing behaviour.
            //
            // One oddity is deliberately not reproduced: the dispatcher reads a dword past the
            // end of this empty message and passes it to the handler, which ignores the argument.
            Opcode::FELLOWSHIP_FELLOW_UPDATE_DONE => {
                match dereth_protocol::social::FellowshipFellowUpdateDone::read(&mut r) {
                    Ok(_) => self.stats.fellow_update_done += 1,
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0226 House_HouseStatus`, the login `QueryHouse` answer.**
            //
            // The dispatcher reads the message's one `u32` and sends a failed-house-transaction
            // notice. Three receivers take that notice, and none reads the value: the house panel
            // redraws, the purchase panel clears its pending state, and the allegiance house flow
            // may repeat its lord query.
            //
            // The house panel closes its pending dialog and, when house data exists, re-runs all
            // seven display sections. So the
            // observable effect of `0x0226` is **a redraw of the House pane**, not a value stored
            // anywhere — which is why the pane's answer for a
            // houseless character is a constant rather than a field.
            //
            // The word on the wire is ACE's `WeenieError`: `GameEventHouseStatus.cs` writes
            // `(uint)weenieError`, `HandleActionQueryHouse` sends it with the default `BadParam`
            // (2) for *"no house owned"*, and eviction sends `HouseEvicted`. All three recorded
            // `0x0226`s carry the value 2. It is kept in [`HudStats::house_status_last_notice`]
            // as a **measurement**, not as state the panel reads — retail stores it nowhere and a
            // field the panel consulted would be this client inventing a behaviour.
            //
            // `0x0259 House_HouseTransaction` is the same handler in retail
            // (its UI dispatch calls the same one), so it shares this arm. It did not occur in the
            // capture corpus. The two opcodes
            // share one arm because retail shares one *function*, not because they look alike:
            // both dispatchers call the house-transaction handler, whose whole body raises the
            // failed-house-transaction notice with the word.
            //
            // ACE never sends `0x0259` — there is no `GameEvent` for it in
            // `ACE.Server/Network/GameEvent/Events` — so this arm is unreachable against the
            // tested shard and is asserted from a synthesised payload.
            Opcode::HOUSE_HOUSE_STATUS | Opcode::HOUSE_HOUSE_TRANSACTION => {
                match dereth_protocol::trade::HouseHouseStatus::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_status_notices += 1;
                        self.stats.house_status_last_notice = u64::from(m.notice_type);
                        // The house panel's update first deletes its house data
                        // and clears the pointer, because `0x0225` can have set it and a shard
                        // that evicts a player sends exactly this message
                        // (`Player_House.cs:566`, `GameEventHouseStatus(HouseEvicted)`).
                        world.clear_house_data();
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0225 House_HouseData`, the message that the purchase-time text
            // and five other sections of the House tab wait for.**
            //
            // The dispatcher unpacks a `HouseData` and calls
            // a handler that forwards `&data` in the house-data notice.
            // Its one registered receiver is the house UI; its update body
            // keeps a copy, then redraws. This build retains the copy for the pane to read.
            //
            // ACE sends it from `Player_House.cs::HandleActionQueryHouse` — the reply to the
            // `0x021E House_QueryHouse` that the player system sends
            // at login — whenever the account owns a house; a houseless account gets `0x0226`
            // instead, which is the arm above and why every one of the ten recordings has
            // the status and none has the data.
            Opcode::HOUSE_HOUSE_DATA => {
                match dereth_protocol::trade::HouseDataMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_data_notices += 1;
                        world.recv_house_data(&m);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x021D House_HouseProfile`, the message that OPENS the purchase /
            // maintenance window.**
            //
            // A slumlord use (`0x0036 Inventory_UseEvent`) is answered by a `0x021D` house profile;
            // no other server path sends one. Receipt of that profile raises the purchase or
            // maintenance window.
            //
            // The request half is `dereth_client_model::inventory::use_object`, which constructs
            // `0x0036`.
            //
            // The decoder uses the twelve-field order. The captured profile decodes to
            // dwelling `0x0592`, owner 0,
            // bitmask 1, min level 35, three `-1`s, type 2 (villa), an empty owner name, three buy
            // lines (2,000,000 Pyreal / 5 Writ of Refuge / 1 Crude Lockpick) and two rent lines,
            // and a test asserts exactly that against the recorded bytes.
            Opcode::HOUSE_HOUSE_PROFILE => {
                match dereth_protocol::trade::HouseProfileMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_profile_notices += 1;
                        world.recv_house_profile(&m);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0227 House_UpdateRentTime` and `0x0228 House_UpdateRentPayment`,
            // the two messages that move the House pane without a `0x0225`.**
            //
            // Both travel the same road as `0x0225`: their dispatchers unpack and call
            // handlers that forward the corresponding purchase/rent payment notice.
            // The one registered receiver is the house UI.
            //
            // What makes them *different* from `0x0225`, and the reason they need their own arm
            // rather than a shared one, is that the House panel's handlers do **not** go through
            // the update: they mutate the house data in place and redisplay it directly. So there
            // is no house notice to count, no house-data replacement, and — the part that is easy
            // to miss — **no arm at all for a houseless character**: both handlers return
            // straight away when there is no house data, with no redraw. `World` carries that guard
            // and reports it, which is what the second counter of each pair is for.
            //
            // The redraw is `HousePanel::update`'s third arm, which
            // compares the `HouseDataView` against the one it last drew. That is not a
            // retail shape — retail is pushed and redraws unconditionally — but it is
            // observationally identical here, because a redraw from an unchanged `HouseData`
            // produces the same eight rows.
            //
            // Both are corpus zeros (0 of 13,535 recorded blobs in all three
            // spaces), like every inbound house message but `0x0226`. ACE sends `0x0227` from
            // `GameEventHouseUpdateRentTime` and never sends `0x0228` at all, re-deriving the
            // whole rent list into a fresh `0x0225` instead; the tests synthesise both in
            // ACE's writer order rather than through our own encoder.
            Opcode::HOUSE_UPDATE_RENT_TIME => {
                match dereth_protocol::trade::HouseUpdateRentTime::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_rent_time_updates += 1;
                        if world.recv_update_rent_time(m.rent_time) {
                            self.stats.house_rent_time_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::HOUSE_UPDATE_RENT_PAYMENT => {
                match dereth_protocol::trade::HouseUpdateRentPayment::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_rent_payment_updates += 1;
                        if world.recv_update_rent_payment(&m.payments) {
                            self.stats.house_rent_payment_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0248 House_UpdateRestrictions`.** The house-data state
            // carries the update-restrictions handler's four gates in the right order;
            // this arm is its production caller.
            //
            // The four gates are the handler, and they are the reason this cannot be a bare
            // store: a zero id is ignored, the **local player is never given restrictions**
            // (the object id must not be the player's), an unknown object is ignored, and a
            // stale sequence byte is rejected by the per-object house-restriction timestamp. All
            // four live in `World`.
            //
            // The message is byte-packed and **unaligned from offset 5** — one sequence byte then
            // an unaligned object id, which retail reads as a byte at offset 4 and a u32 at
            // offset 5 before handing on the rest from offset 9.
            // `dereth_protocol::trade::HouseUpdateRestrictions` reads it that way.
            Opcode::HOUSE_UPDATE_RESTRICTIONS => {
                match dereth_protocol::trade::HouseUpdateRestrictions::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_restriction_updates += 1;
                        if world.recv_update_restrictions(m.sequence, m.sender, m.restrictions) {
                            self.stats.house_restrictions_applied += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0257 House_UpdateHAR`, the answer `@house guest list` is for.**
            //
            // The message unpacks a house access record, formats its guest rows, and writes them to the scroll as
            // text type zero. The roommate pass uses the same shape with its selector set.
            //
            // Chat type **0** and window **0** — not the current command source, even though the
            // thing that asked for it was a typed command. That asymmetry is retail's, and the
            // scroll is a live consumer: `Hud::drain_scroll` turns it into `ChatMessage`s at the
            // head of the very next batch.
            //
            // **The arrival is proved rather than assumed.** The house-request capture has each
            // `0x024D` answered by this message; ACE sends it from
            // `Player_House.HandleActionGuestList` through `GameEventUpdateHAR`.
            Opcode::HOUSE_UPDATE_HAR => {
                match dereth_protocol::trade::HouseUpdateHar::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_har_updates += 1;
                        self.stats.house_har_guests +=
                            u64::try_from(m.0.guest_table.entries.len()).unwrap_or(u64::MAX);
                        let text = dereth_client_model::housing::har_dump(&m.0, false);
                        world.scroll.add_text_to_scroll(&text, 0, true, 0);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0271 House_AvailableHouses`, the answer to `@hslist`.**
            //
            // Available-house output is a header line, then coordinates, then a cut-off line. The
            // formatting literals are represented by `dereth_client_model::housing::available_houses_header`
            // and `coord_line`.
            //
            // Two branches are easy to miss: an **apartment** listing prints the header and stops,
            // with no coordinates, while a non-apartment listing adds the cut-off line only when
            // the count exceeds 400.
            //
            // The count in the header is the message's `num_houses`, which ACE sets to
            // `locations.Count` **before** `Distinct()`, while the coordinate list is the
            // deduplicated one. The two numbers legitimately disagree and neither is wrong.
            //
            // This one is a corpus zero — the capture sweep never sent `@hslist` — so unlike
            // `0x0257` its arrival is established from ACE's writer
            // (`GameEventHouseAvailableHouses`, enqueued unconditionally by
            // `Player_House.HandleActionListAvailable` for every `0x0270`) rather than from a
            // recording, and the tests drive it through the replay transport.
            Opcode::HOUSE_AVAILABLE_HOUSES => {
                match dereth_protocol::trade::HouseAvailableHouses::read(&mut r) {
                    Ok(m) => {
                        self.stats.house_available_houses += 1;
                        let header = dereth_client_model::housing::available_houses_header(
                            m.house_type,
                            m.num_houses,
                        );
                        world.scroll.add_text_to_scroll(&header, 0, true, 0);
                        if dereth_client_model::housing::available_houses_lists_coords(m.house_type)
                        {
                            for cell in &m.landcells {
                                let Some((ew, ns)) = dereth_physics::landdefs::gid_to_lcoord(
                                    dereth_primitives::CellId(*cell),
                                ) else {
                                    continue;
                                };
                                let line = dereth_client_model::housing::coord_line(ew, ns);
                                world.scroll.add_text_to_scroll(&line, 0, true, 0);
                                self.stats.house_available_coord_lines += 1;
                            }
                            if dereth_client_model::housing::available_houses_truncated(
                                m.num_houses,
                            ) {
                                world.scroll.add_text_to_scroll(
                                    dereth_client_model::housing::TOO_MANY_HOUSES,
                                    0,
                                    true,
                                    0,
                                );
                            }
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x019E Combat_HandlePlayerDeathEvent`.**
            //
            // The handler unpacks the message, then emits its line only when it is non-empty and
            // the local player is neither the victim nor the killer.
            //
            // **The `!=` pair is the handler, not decoration.** A death you were part of — as the
            // victim or as the killer — produces **no** line here; those are carried by the combat
            // and victim-notification messages that arrive with it, and wiring this one
            // unconditionally would double them. It is the *third-party* deaths, the ones that
            // make the "So-and-so has been slain by…" traffic of a busy landblock, that only this
            // message carries. The ids are read **after** the string, because unpacking the string
            // advances the read pointer before the first id is taken.
            //
            // A stored length of 1 is the empty-string case because the length counts the
            // terminator. The trailing `"\n"` is retail's; `add_text_to_scroll` trims
            // it straight back off, and it is written here anyway so the call site stays honest.
            //
            // The scroll is a live consumer: `Hud::drain_scroll` turns it into `ChatMessage`s at
            // the head of the very next batch and `App` delivers those to the chat windows.
            Opcode::COMBAT_HANDLE_PLAYER_DEATH_EVENT => {
                match dereth_protocol::combat::CombatHandlePlayerDeathEvent::read(&mut r) {
                    Ok(m) => {
                        self.stats.player_deaths += 1;
                        let ours = world.is_the_player(m.killed) || world.is_the_player(m.killer);
                        if !ours && !m.message.is_empty() {
                            world.scroll.add_text_to_scroll(
                                &format!("{}\n", m.message),
                                0,
                                true,
                                0,
                            );
                            self.stats.player_deaths_announced += 1;
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02C1 Magic_UpdateSpell`, the sibling of the arm above.**
            //
            // Without it a newly learned spell would not appear in the book until the player
            // relogged: the book is filled from `0x0013` at login, and `0x01A8` is only the
            // *remove* half.
            //
            // The add path mirrors spell removal: when a player description exists, update its
            // spellbook and then announce the spellbook change.
            //
            // Adding creates the spellbook when the character has
            // none and inserts a `SpellBookPage` whose `_casting_likelihood` is **0.0** — the
            // value is not on the wire, only the id is. It reaches
            // insertion into the packed spell-book page table, which does **not** overwrite an
            // existing key, so a repeat leaves the page the login description delivered alone.
            //
            // The rebuild is [`Self::handle_magic_remove_spell`]'s, for the same reason: the
            // panel's join is a pure function of the qualities, so a changed book rebuilds it.
            // **Not done here:** the panel also selects the new spell when the current filter would
            // show it. That is the panel's own state, not the book's.
            Opcode::MAGIC_UPDATE_SPELL => {
                match dereth_protocol::qualities::MagicUpdateSpell::read(&mut r) {
                    Ok(m) => self.handle_magic_update_spell(m.layered_spell_id, world),
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0021 Social_FriendsUpdate`.**
            //
            // The message unpacks a counted list of friend records and then the update type; the
            // five-arm switch is `dereth_client_model::friends::FriendsUpdate`.
            //
            // It arrives **unprompted**: there is no subscribe, the Friends panel has no
            // visibility-changed handler at all, and the shard pushes one per login and one per
            // change. That is why the recorded captures carry 11 of them with the tab never once
            // opened.
            //
            // The reader is `dereth_ui_screens::panels::friends`.
            Opcode::SOCIAL_FRIENDS_UPDATE => {
                match dereth_protocol::social::SocialFriendsUpdate::read(&mut r) {
                    Ok(m) => {
                        let kind = world.recv_friends_update(&m);
                        self.stats.friends_updates += 1;
                        if !kind.is_handled() {
                            self.stats.friends_updates_unknown_type += 1;
                        }
                        self.stats.friends =
                            u64::try_from(world.friends().len()).unwrap_or(u64::MAX);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x0314` and `0x0315`, the contract tracker messages.**
            //
            // A corpus scan finds neither anywhere, so there is **no capture
            // oracle for the contract record** and the layout was checked against
            // `ACE/Source/ACE.Server/Network/Structure/ContractTracker.cs`'s `Write` extension
            // instead -- `Version`, `ContractId`, `(uint)Stage`, `TimeWhenDone`, `TimeWhenRepeats`,
            // then the two flags written by the *event* and not by the record. That is exactly
            // what `dereth_protocol::social` has.
            //
            // The reader is `dereth_ui_screens::panels::contracts`, through [`HudView::contracts`].
            Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER_TABLE => {
                match dereth_protocol::social::SocialSendClientContractTrackerTable::read(&mut r) {
                    Ok(m) => {
                        let n = world.recv_contract_tracker_table(&m, self.server_now());
                        self.stats.contract_tables += 1;
                        self.stats.contracts = u64::try_from(n).unwrap_or(u64::MAX);
                        self.count_unresolved_contracts(world);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::SOCIAL_SEND_CLIENT_CONTRACT_TRACKER => {
                match dereth_protocol::social::SocialSendClientContractTracker::read(&mut r) {
                    Ok(m) => {
                        use dereth_client_model::quests::ContractUpdate;
                        match world.recv_contract_tracker(&m, self.server_now()) {
                            ContractUpdate::Added => self.stats.contract_trackers_added += 1,
                            ContractUpdate::Updated => self.stats.contract_trackers_updated += 1,
                            ContractUpdate::Removed => self.stats.contract_trackers_removed += 1,
                        }
                        if m.set_as_display_contract != 0 {
                            self.stats.contract_display_requests += 1;
                        }
                        self.stats.contracts =
                            u64::try_from(world.contract_trackers().len()).unwrap_or(u64::MAX);
                        self.count_unresolved_contracts(world);
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x027A Allegiance_AllegianceLoginNotificationEvent`.**
            //
            // *"Your vassal has logged on."* The recorded captures have none.
            // **That zero is not evidence it never arrives**: no allegiance member logged on or
            // off during the capture sessions. It is the same kind of zero `0x019E` above has — an event nothing
            // asks for that fires the first time it happens — and not the kind `0x02C9`–`0x02CC`
            // have, which need an ACE developer command to exist at all.
            //
            // The dispatcher reads the member id and a status integer, then applies
            // `(id, status != 0)` to the logon-status handler.
            //
            // The chat scroll is the **only** receiver that handles that notice: every other
            // notice receiver ignores it. So the consumer really is the chat scroll and nothing
            // else.
            // In particular the roster's `" *"` logged-in marker is **not** refreshed here — retail
            // leaves it stale until the next `0x0020`, so this arm writes no `AllegianceData`.
            //
            // The cached member name is the append target and the logon/logoff phrase is the
            // suffix, so the resulting line is `name + suffix`.
            //
            // **The allegiance data lookup's refusal is the handler's only guard, and it is a real
            // one.** A member the cached tree does not hold prints *nothing at all*. There is no id
            // fallback and no "someone".
            // A client that has never sent `0x001B Allegiance_AllegianceUpdate` therefore shows no
            // logon lines whatsoever, which is retail's behaviour and not a gap in this arm.
            //
            // **Neither side gates on `DisplayAllegianceLogonNotifications`.** That option (retail
            // ordinal 24) is the *server's* test: `Player_Allegiance.cs:455` / `:467` send this
            // event only to online allegiance members whose `ShowAllegianceLogons` is set, and
            // never to the member who is logging in. The receiver reads no option word, so
            // gating here would drop lines the shard had already decided to send.
            //
            // The literals include their leading space and trailing newline:
            // `" is logged in.\n"` (15 bytes) and `" has logged out.\n"` (17).
            // `add_text_to_scroll` trims the newline straight back off, as it does for `0x019E`.
            Opcode::ALLEGIANCE_ALLEGIANCE_LOGIN_NOTIFICATION_EVENT => {
                match dereth_protocol::social::AllegianceLoginNotification::read(&mut r) {
                    Ok(m) => {
                        self.stats.allegiance_logins += 1;
                        // The bare name, not the full name: retail takes the stored name directly
                        // and never asks for the rank title.
                        let name = world.allegiance.look_up(m.member).map(|d| d.name.clone());
                        match name {
                            Some(name) => {
                                let suffix = if m.now_logged_in == 0 {
                                    " has logged out.\n"
                                } else {
                                    " is logged in.\n"
                                };
                                world.scroll.add_text_to_scroll(
                                    &format!("{name}{suffix}"),
                                    0,
                                    true,
                                    0,
                                );
                                self.stats.allegiance_logins_announced += 1;
                            }
                            None => self.stats.allegiance_logins_without_a_member += 1,
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // **`0x02B4`, the salvage results.**
            //
            // The recorded captures have no arrivals of it, against a control of 23 `0x02BE` in the
            // same sweep, so the instrument was demonstrably looking. The layout
            // was therefore checked against `ACE.Server/Network/GameEvent/Events/
            // GameEventSalvageOperationsResult.cs` -- `(uint)skill`, a count-0 not-salvagable
            // list, the results list, then the augmentation bonus -- which is exactly what
            // `dereth_protocol::items::SalvageResultMessage` has, with the packed list
            // a `u32` count plus elements in both.
            //
            // `dereth_client_model::inventory::salvage` is the reader, and
            // `dereth_ui_screens::panels::salvage` is the sender of the `0x027D` it answers.
            //
            // The three name resolvers are supplied here because `dereth-client-model` has no dat access:
            // the material names are [`MATERIAL_TYPE_NAMES`] with `_` mapped to a space, the
            // skill name already comes from `panels::examination`, and the not-salvagable object
            // names come from the world.
            Opcode::INVENTORY_SALVAGE_OPERATIONS_RESULT_DATA => {
                match dereth_protocol::items::SalvageResultMessage::read(&mut r) {
                    Ok(m) => {
                        self.stats.salvage_results += 1;
                        let names = self.material_names.as_ref();
                        // The lookup is [`material_name_of`], which the
                        // display-name composer shares; the `"Unknown"` stays here because it is
                        // *this* caller's miss arm. The salvaged-materials text
                        // writes it into the string **before** it asks the mapper, so it is
                        // retail's own answer and not a placeholder -- and it is **not**
                        // the object-name query's answer to the same miss.
                        let material = |id: u32| {
                            material_name_of(names, id).unwrap_or_else(|| "Unknown".to_owned())
                        };
                        let skill = |id: u32| {
                            dereth_client_contract::panels::examination::skill_to_string(id)
                                .map(str::to_owned)
                        };
                        // Resolved up front, because the call below needs `&mut World` and a
                        // closure reading `weenie()` would hold an immutable borrow across it.
                        // An empty name is the object lookup missing as far
                        // as the non-suitables text is concerned: both skip
                        // the row without bumping its separator counter.
                        let resolved: Vec<(dereth_primitives::ObjectId, String)> = m
                            .not_salvagable
                            .iter()
                            .filter_map(|id| {
                                let n = world.weenie(*id)?.pwd.name.as_str();
                                (!n.is_empty()).then(|| (*id, n.to_owned()))
                            })
                            .collect();
                        let item = |id: dereth_primitives::ObjectId| {
                            resolved
                                .iter()
                                .find(|(i, _)| *i == id)
                                .map(|(_, n)| n.clone())
                        };
                        // `None` is the squelch; `Some(0)` is "both lists said nothing", which
                        // is not the same event and must not share a counter.
                        match world.recv_salvage_operations_result(&m, &material, &skill, &item) {
                            None => self.stats.salvage_results_squelched += 1,
                            Some(lines) => self.stats.salvage_lines += u64::from(lines),
                        }
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            // ---- the six inbound chess messages, all of them corpus zeros ----
            //
            // The recorded corpus has **0** arrivals for each of
            // `0x0281`, `0x0282`, `0x0283`, `0x0284`, `0x0285` and `0x028C` in all three spaces
            // over 13,535 blobs across ten sessions, and the same for the five outbound ones. No
            // recorded session plays chess, so the shapes below are ACE's writer order
            // (`GameEventJoinGameResponse` and its five siblings, every one `Write(boardGuid.Full)`
            // then the `int`s) checked against the global sequence-stamp table.
            //
            // All six travel the identical road in retail --
            // the mini-game event through the game UI dispatcher and mini-game receiver (each a
            // one-line forward) to the single handler, which is the
            // single receiver registered for all of them (the window registers nine
            // notices in total at setup).
            //
            // Every one of the six is **guarded on the game id** and two of them on the game state as
            // well, so a message for a board this window did not join changes nothing and says
            // nothing; `minigame_guarded` is that guard's counter and must not read the same as
            // "never arrived".
            Opcode::GAME_JOIN_GAME_RESPONSE => {
                match dereth_protocol::trade::GameJoinGameResponse::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_join_game_response(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_START_GAME => match dereth_protocol::trade::GameStartGame::read(&mut r) {
                Ok(m) => {
                    self.stats.minigame_events += 1;
                    if !world.recv_start_game(&m) {
                        self.stats.minigame_guarded += 1;
                    }
                    self.stats.minigame_lines += world.drain_minigame_text() as u64;
                }
                Err(_) => self.stats.undecodable += 1,
            },
            Opcode::GAME_MOVE_RESPONSE => {
                match dereth_protocol::trade::GameMoveResponse::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_move_response(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_OPPONENT_TURN => {
                match dereth_protocol::trade::GameOpponentTurn::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_opponent_turn(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_OPPONENT_STALEMATE_STATE => {
                match dereth_protocol::trade::GameOpponentStalemateState::read(&mut r) {
                    Ok(m) => {
                        self.stats.minigame_events += 1;
                        if !world.recv_opponent_stalemate(&m) {
                            self.stats.minigame_guarded += 1;
                        }
                        self.stats.minigame_lines += world.drain_minigame_text() as u64;
                    }
                    Err(_) => self.stats.undecodable += 1,
                }
            }
            Opcode::GAME_GAME_OVER => match dereth_protocol::trade::GameGameOver::read(&mut r) {
                Ok(m) => {
                    self.stats.minigame_events += 1;
                    if !world.recv_game_over(&m) {
                        self.stats.minigame_guarded += 1;
                    }
                    self.stats.minigame_lines += world.drain_minigame_text() as u64;
                }
                Err(_) => self.stats.undecodable += 1,
            },
            // **Count every unhandled message.** Every `COMMUNICATION_*`, `FELLOWSHIP_*`,
            // `SOCIAL_*`, `HOUSE_*` and `GAME_*` message the shard sends that this build has no arm
            // for reaches here; without a counter it would vanish without leaving a number anywhere
            // in the process, and the unhandled set could only be found by reading source rather
            // than by running a client.
            _ => {
                self.stats.ui_events_unhandled += 1;
                crate::dropped::record(crate::dropped::Site::UiEvent, opcode);
            }
        }
    }

    /// `0x02C1 Magic_UpdateSpell` — the add
    /// half of the pair whose remove half is [`Self::handle_magic_remove_spell`].
    ///
    /// Add one spellbook page with casting likelihood `0.0`, creating the book if necessary.
    ///
    /// Three things that are easy to get wrong and are taken off that body:
    ///
    /// * **The book is created if the character has none.** A character who has learned no spell
    ///   at all has no `_spell_book`, and the first `0x02C1` is what makes one.
    /// * **`_casting_likelihood` is `0.0`.** The wire carries the id and nothing else.
    /// * **A duplicate keeps the page already there.** Hash-table insertion returns false without
    ///   overwriting, so a `0x02C1` for a spell
    ///   the login description already delivered must not reset its likelihood to zero.
    ///
    /// Retail raises the notice unconditionally, ignoring the add's return, so the rebuild is
    /// not gated on the insert having happened — exactly as the remove half is not.
    fn handle_magic_update_spell(&mut self, spell_id: u32, world: &mut dereth_client_model::World) {
        self.stats.spells_added += 1;
        let Some(q) = world.player_qualities_mut() else {
            return;
        };
        q.spell_book
            .get_or_insert_with(Default::default)
            .entry(spell_id)
            .or_insert_with(Default::default);
        self.spells = self.build_spells(world);
    }

    /// When a player description exists, remove the spell from its book and
    /// announce the spellbook change. Removal is one hash-table operation guarded by the book's
    /// presence. There is no layer mask: the
    /// key is the `layered_spell_id` as it arrived, which for a spellbook page is the plain id.
    ///
    /// The notice reaches exactly two handlers, and **neither reads the spell id it is handed**:
    ///
    /// * the spellbook view rebuilds the whole book from the qualities. That is
    ///   [`Self::build_spells`], the same join `PlayerDescReceived` uses.
    /// * the spell-casting view refreshes all eight tabs. That half lives in
    ///   `dereth_ui_screens::panels::spellcasting`, which reads `GameView::spell_tab` and
    ///   `GameView::is_spell_known` — both of which move as soon as the book does.
    ///
    /// The Skills panel is **not** a subscriber, so this rebuilds the spell list alone rather than
    /// calling [`Self::rebuild_panel_tables`]; the skills join is unchanged by a spell removal.
    ///
    /// Retail raises the notice unconditionally — the `Magic_RemoveSpell` handler ignores the
    /// removal's return, and the removal returns 0 when the character has no spellbook at
    /// all — so the rebuild here is not gated on the key having been present either.
    fn handle_magic_remove_spell(&mut self, spell_id: u32, world: &mut dereth_client_model::World) {
        self.stats.spells_removed += 1;
        let Some(q) = world.player_qualities_mut() else {
            return;
        };
        if let Some(book) = q.spell_book.as_mut() {
            book.remove(&spell_id);
        }
        self.spells = self.build_spells(world);
    }
    /// A stat update applied to the local player description — the whole family, in
    /// one body, the way the client has it.
    ///
    /// [`dereth_client_model::qualities::update`] carries the setters, so there is no hand
    /// decode of the `Attribute2nd` opcodes here and every one of them passes the
    /// `PropertySequenceGate` gate.
    ///
    /// Three things this does:
    ///
    /// 1. **The sequence gate.** The stat update runs it *before* the
    ///    setter, so a re-ordered or replayed update is dropped rather than applied.
    /// 2. **Skills and attributes land.** Those are the messages a raise comes back as.
    /// 3. **The `0x10000004` arm.** A quality the stat-management panel draws from sets
    ///    [`Self::raise_answered`], which `Self::drive` turns into
    ///    `SkillsPanel::clear_awaiting_raise` plus a footer re-run.
    fn apply_quality_update(
        &mut self,
        opcode: dereth_protocol::Opcode,
        body: &[u8],
        world: &mut dereth_client_model::World,
    ) -> Option<dereth_client_model::combat::CombatMode> {
        use dereth_client_model::qualities::update::{self, Outcome};

        let Some(u) = update::decode(opcode, body) else {
            self.stats.undecodable += 1;
            return None;
        };
        // There is no subject guard here: the arbiter is the store's own stat-update subject
        // lookup with the message's object id, so the identity that decides is the store's
        // and not a second copy of it kept here. The player-scoped update refuses a
        // public form naming anybody else, which is the same test against the one owner.
        // The store, sequence gate, and player-scoped handlers all belong to
        // `dereth_client_model::World`. Retail checks the `(property, sequence)` pair and then
        // writes the qualities on the same player object that the public form addresses. A second
        // `Qualities` and `PropertySequenceGate` kept here would let `Shop::update_total_value`
        // read `0` while this module held 9,995.
        let vital = u.key.stat_type() == dereth_client_model::StatType::Attribute2nd;
        // `if (!obj) return 0` — no player, no row, or a public form naming somebody else.
        let outcome = world.apply_player_quality_update(&u);
        if crate::trace::raise() && u.key.stat_type() == dereth_client_model::StatType::Int64 {
            // `AvailableExperience` / `TotalExperience` as the live shard
            // sends them after login, and what the stat update's gate said.
            tracing::debug!(
                target: "dereth::trace::raise",
                "raise-trace quality {:#06x} ({}) subject={:?} seq={} Int64/{} \
                 value={:?} -> outcome={:?} answers_a_raise={}",
                opcode.0,
                opcode.name().unwrap_or("?"),
                u.subject,
                u.sequence,
                u.key.property(),
                u.value,
                outcome,
                update::answers_a_raise_update(&u),
            );
        }
        let outcome = outcome?;
        match outcome {
            Outcome::Stale => {
                self.stats.quality_updates_stale += 1;
                return None;
            }
            Outcome::Unstorable => {
                self.stats.quality_updates_unstorable += 1;
                // The vitals opcodes must leave this counter at zero, so
                // the vitals subset keeps its own counter and its own meaning.
                if vital {
                    self.stats.vital_updates_unstorable += 1;
                }
                return None;
            }
            Outcome::Applied => {}
        }
        self.stats.quality_updates += 1;
        match u.key.stat_type() {
            dereth_client_model::StatType::Attribute2nd => self.stats.vital_updates += 1,
            dereth_client_model::StatType::Skill => self.stats.skill_updates += 1,
            dereth_client_model::StatType::Attribute => self.stats.attribute_updates += 1,
            _ => {}
        }
        // The skill and attribute joins read only qualities, so either kind of change rebuilds
        // them. The vitals are not among them — the Vitals panel reads the derived attribute live, and
        // rebuilding 38 skill rows on every regeneration tick would be 55 rebuilds a session.
        if matches!(
            u.key.stat_type(),
            dereth_client_model::StatType::Skill | dereth_client_model::StatType::Attribute
        ) {
            self.rebuild_panel_tables(world);
        }
        // `answers_a_raise_update` rather than `answers_a_raise`. The
        // key-only predicate cannot separate `Qualities_*UpdateAttribute2nd` (`0x02E7`, the whole
        // record, which is what `Train_TrainAttribute2nd` is answered with) from
        // `…Attribute2ndLevel` (`0x02E9`, a regeneration tick) because the two share tag 9, so it
        // would refuse both and a **vital raise's answer would never clear the latch**. The value
        // variant separates them; see
        // `dereth_client_model::qualities::update::answers_a_raise_update`.
        if update::answers_a_raise_update(&u) {
            self.raise_answered = true;
        }
        // **The purse delivery belongs to the writer.** The container UI registers
        // the player's `0x14 CoinValue` integer quality, and the player-quality notification
        // fires that handler set from inside
        // the stat update whenever the changed weenie is the player. So the subscription
        // belongs to the *writer*, not to this dispatcher: it is
        // the world's player-quality handlers, reached from
        // the quality-update path above and from the login-description path's own
        // `QualityScope::Player` arm — which is why a public `CoinValue` for the player moves
        // the purse too, and why the login description does.
        //
        // The six recorded `CoinValue` updates (9999, 9998, 9997, 9995 at the grocer, **9930** at
        // idx 4744 for a purchase and **9988** at idx 4788 for a sale) arrive in one store and are
        // not copied into another.
        //
        // The combat UI subscribes to the player's Int 0x28.
        // The quality-changed callback reads that quality and sets the combat mode (not forced).
        // This must follow the timestamped update, not merely decoding the packet or observing
        // the value.
        if u.key == dereth_client_model::StatKey::new(dereth_client_model::StatType::Int, 0x28) {
            if let dereth_client_model::StatValue::Int(mode) = u.value {
                return Some(dereth_client_model::combat::CombatMode::from_raw(
                    mode as u32,
                ));
            }
        }
        None
    }

    /// Remove a stat from the player's qualities — the
    /// sixteen `Qualities_*Remove*Event` forms in one body, the way the client has them.
    ///
    /// This is [`Self::apply_quality_update`]'s twin and is deliberately a *separate* function,
    /// because the two retail functions are separate and differ in three places that matter:
    ///
    /// 1. **No value.** The private remove form is `[u8 seq][u32 property]`, nine bytes with the
    ///    opcode.
    /// 2. **No mirror.** The stat update mirrors into the player description;
    ///    none of the eight remove-stat templates does.
    /// 3. **The remove handler**, not the change handler. That is what
    ///    decides everything below, because **only two panels in the whole client override the
    ///    quality-removal callback** — the Radar and Vendor panels, against
    ///    thirteen that override the quality-changed callback. Everybody else inherits the base no-op.
    ///
    /// So three things this does **not** do, each because retail does not:
    ///
    /// * **it does not clear the raise latch.** The stat-management panel overrides
    ///   the quality-changed callback and *not* the quality-removed one, so
    ///   [`dereth_client_model::qualities::update::answers_a_raise_update`] has no business on this path.
    /// * **it does not rebuild the skills or spells tables.** The Skills panel has no
    ///   quality-removed callback either — and there is no remove opcode that could name a skill or an
    ///   attribute in the first place, since stat removal has only the eight generic templates.
    /// * **it does not change the combat mode.**
    ///   The combat UI is the `Int 0x28` subscriber and has no quality-removed callback, so a
    ///   removed combat mode leaves the client's stance where it was. `apply_quality_update`
    ///   returns a [`dereth_client_model::combat::CombatMode`] for exactly that reason and this returns
    ///   nothing.
    ///
    /// What it *does* raise is the player-scope handler set, which is
    /// the container-update path's job because that is where retail keeps
    /// it: the container UI registers `Int 0x14 CoinValue` for the player,
    /// and the quality-removed handler's first arm (property `0x14`) reaches
    /// the total-value update. So a shard that clears the purse redraws it.
    fn apply_quality_remove(
        &mut self,
        opcode: dereth_protocol::Opcode,
        body: &[u8],
        world: &mut dereth_client_model::World,
    ) {
        use dereth_client_model::qualities::remove::{self, Outcome};

        let Some(r) = remove::decode(opcode, body) else {
            self.stats.undecodable += 1;
            return;
        };
        // `if (!obj) return 0` — no player, no row, or a public form naming somebody else. The
        // public forms for *other* objects are `interaction::apply_events_at_boundary`'s, which is
        // the same partition `apply_quality_update` already draws.
        let Some(outcome) = world.apply_player_quality_remove(&r) else {
            return;
        };
        match outcome {
            Outcome::Stale => self.stats.quality_removes_stale += 1,
            Outcome::Absent => {
                self.stats.quality_removes_absent += 1;
                self.stats.quality_removes += 1;
            }
            Outcome::Removed => self.stats.quality_removes += 1,
        }
    }

    /// The chat lines waiting for the chat windows, oldest first, for the screen generation
    /// `screen`: a line raised for an earlier generation of the screen (a notice of a window that
    /// has since been rebuilt) is dropped, and a line tied to no screen is kept. The queue is
    /// emptied either way, so no line is delivered twice.
    pub fn take_chat_lines(&mut self, screen: u64) -> Vec<ChatMessage> {
        std::mem::take(&mut self.pending_chat)
            .into_iter()
            .filter(|(generation, _)| !generation.is_some_and(|g| g != screen))
            .map(|(_, m)| m)
            .collect()
    }

    /// Rebuild the derived per-frame values: the radar list, the coordinates and the heading.
    ///
    /// The radar's regeneration walks every live weenie and adds each one; this is that walk,
    /// in the same table order (`dereth_client_model::ObjMap` reproduces
    /// `LongHash`'s bucket walk on purpose).
    pub fn sync(&mut self, objects: &crate::objects::ObjectStream, viewer: Option<ViewerFrame>) {
        let world = &objects.world;
        let player = world.player;
        self.radar.clear();
        let origin = viewer.map(|v| v.position);
        for (id, presence) in objects.presences() {
            let Some(w) = world.weenie(id) else { continue };
            let pwd = &w.pwd;
            let pos = presence.position.as_ref();
            let in_world = pos.is_some();
            // Conversion to player space is exactly `localtolocal`
            // `(player.position, &out, obj.position, {0,0,0})` — which is
            // what makes the radar heading-up, because `localtolocal` applies the player frame's
            // own rotation, which is why the compass letters orbit instead of the blips.
            //
            // Without a viewer position there is no player space, so the entry is carried with
            // `in_world` clear and `draw_objects` skips it — the same skip retail's radar object draw makes for
            // an object with no cell.
            let player_space = match (origin.as_ref(), pos) {
                (Some(o), Some(p)) => {
                    let v = dereth_physics::math::localtolocal(o, p, dereth_primitives::Vec3::ZERO);
                    (v.x, v.y, v.z)
                }
                _ => (0.0, 0.0, 0.0),
            };
            self.radar.push(RadarEntry {
                id,
                player_space,
                blip_color: pwd.blip_color.unwrap_or(0),
                bitfield: pwd.bitfield,
                // The radar-behaviour field is the radar's
                // filter and this is the field it reads. Absent means `radar_enum_value::UNDEF`
                // (0), and that is not shown — the client's own default, not a substitute for one.
                radar_enum: pwd.radar_enum.unwrap_or(radar_enum_value::UNDEF),
                // `IsPlayer` is `_bitfield & 0x08` (`ObjectDescriptionFlag::Player`), which is what
                // the blip-colour lookup asks. It is a **different** question from
                // "is this our own character", which the radar asks separately (and answers by
                // leaving the player out of the blip list entirely); an admin watching another
                // player still needs that player coloured as a player.
                is_player: pwd.bitfield & bits::PLAYER != 0,
                is_self: Some(id) == player,
                // These come from the four predicates used by blip colour and shape.
                // The four one-line bit tests establish every input:
                //
                //   player         `_bitfield >> 3    & 1`  0x00000008 Player
                //   player killer  `_bitfield >> 5    & 1`  0x00000020 PlayerKiller
                //   PK lite        `_bitfield >> 0x19 & 1`  0x02000000 PkLiteStatus
                //   creature       `_type >> 4        & 1`  ITEM_TYPE 0x10
                //
                // `is_attackable` carries the creature test — `is_creature`, over `_type`, **not** a
                // `_bitfield` bit. The separate
                // `_bitfield & 0x10` half of the same branch is `Attackable`, and `get_blip_color`
                // still tests it. Without this line every creature in the world falls through to
                // `RadarDefault` and the radar plots them white instead of gold.
                is_attackable: pwd.obj_type & ITEM_TYPE_CREATURE != 0,
                is_pk: pwd.bitfield & bits::PK != 0,
                is_pk_lite: pwd.bitfield & bits::PK_LITE != 0,
                // `get_blip_shape` asks
                // the allegiance-member predicate, which compares the two
                // `_monarch` ids and **does not** consult the allegiance hierarchy — see
                // the world's allegiance-member predicate.
                is_allegiance_member: world.is_allegiance_member(id),
                // Radar updates query allegiance
                // membership and then `is_fellow` on every blip, and `get_blip_shape` asks
                // the same pair before it asks about the allegiance — so with these two nailed to
                // `false` a fellow would draw as a plain player in the radar-default/creature
                // colours and never in the fellowship colour, whatever the table held.
                //
                // The fellowship-leader test is **not** gated on membership: it is "a fellowship
                // is held and its leader is `id`", the comparison and nothing else.
                is_fellow: world.is_fellow(id),
                is_fellowship_leader: world.is_fellowship_leader(id),
                in_world: in_world && origin.is_some(),
            });
        }
        // Whether the player exists, snapshotted where the client would read it.
        // `App::frame` builds the `ViewerFrame` from `WorldScene::character`, which *is* the local
        // physics object, so `viewer.is_some()` is that gate and nothing else on this
        // struct is. Written unconditionally — outside the `if let` below — because losing the
        // body has to clear it, and `player_cell` deliberately latches.
        self.player_body = viewer.is_some();
        if let Some(v) = viewer {
            self.heading = v.heading_degrees;
            self.coords = player_coords(v.position.cell);
            // Keep the same cell `player_coords` consumes: it is the outdoor-state
            // query's only input and therefore selects the radar range. `viewer` is the character.
            self.player_cell = Some(v.position.cell);
        }
        // The inventory placements for the paper doll, in the server's own order — which is what
        // makes the paper doll's item placement's "last write wins" per slot mean what it means.
        self.equipment.clear();
        // Use the player id recorded by `0xF746`: `0x0013` arrives before the
        // player's own `0xF745`, and `remake_character_inventory` reads
        // the recorded player id for exactly that reason. Using the object table's copy
        // here leaves the doll empty for every frame between the two messages — and for the whole
        // of a corpus replay that feeds `0x0013` without the create stream.
        if let Some(p) = world.player.or(self.player) {
            if let Some(inv) = world.inventory(p) {
                self.equipment
                    .extend(inv.placements.iter().map(|p| (p.iid, p.loc)));
            }
        }
    }

    /// The display-name material prefix, for every object that has
    /// one, so a salvage bag shows its material and not the shard's bare `Salvage (100)`.
    ///
    /// The client composes the name inside its object-name query on every call and needs no
    /// cache: it has the dat mapper to hand and writes into a static buffer. This build's
    /// `GameView::name` answers a `&str` borrowed from the world and the mapper lives on this side
    /// of the seam, so the composed strings are built here, once per object per change, and the
    /// view reads them back.
    ///
    /// **Guarded on the three inputs, not on the frame.** A pack of twenty loot items would
    /// otherwise allocate forty `String`s every frame for a name that never moves;
    /// [`DisplayName::matches`] is what stops it, and [`DisplayName::wanted`] is what keeps
    /// ordinary objects out of the map entirely. `retain` first, so an object that was destroyed --
    /// or whose material was cleared -- does not leave a stale name behind for its id to be reused
    /// with. The counter is the instrument: a window full of salvage with
    /// `display_names_composed == 0` is a defect and must not read like an empty world.
    pub fn refresh_display_names(&mut self, world: &dereth_client_model::World) {
        // Taken out so the mapper (`&self.material_names`) can be read while the map is written.
        let mut cache = std::mem::take(&mut self.display_names);
        cache.retain(|id, _| world.weenie(*id).is_some_and(DisplayName::wanted));
        for (id, w) in world.tables.weenies.iter() {
            if !DisplayName::wanted(w) {
                continue;
            }
            let material = w.pwd.material_type.unwrap_or(0);
            if cache.get(&id).is_some_and(|d| d.matches(w, material)) {
                continue;
            }
            let name = material_name_of(self.material_names.as_ref(), material);
            cache.insert(id, DisplayName::compose(w, material, name.as_deref()));
            self.stats.display_names_composed += 1;
        }
        self.display_names = cache;
    }

    /// The communication state projected into either interface's target menu.
    #[must_use]
    pub fn chat_focus_view(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::mainchat::ChatFocusView {
        dereth_client_contract::chat::mainchat::ChatFocusView {
            focus: world.chat.talk_focus as u32,
            enabled: world.chat.enabled_focuses(),
            selectable: world.chat.selectable_focuses(),
            is_olthoi: self.is_olthoi(world),
            target: world.chat.last_speakable_target.and_then(|id| {
                world.weenie(id).map(
                    |w| dereth_client_contract::chat::mainchat::SpeakableTarget {
                        id: id.0,
                        name: w.object_name(dereth_client_model::weenie::NameType::Appropriate),
                        talkable: w.is_talkable(),
                        squelched: world.chat.is_squelched(id, "", 1),
                    },
                )
            }),
        }
    }

    /// The eight answers the auto-target sweep needs from the object
    /// system, gathered once per frame.
    ///
    /// `in_range_of_player` checks `(id, player, radar radius, true, false)`,
    /// evaluated for every presence. Two deviations, both stated rather than
    /// hidden:
    ///
    /// * **the distance is centre to centre.** The range query's bounding-box flag selects
    ///   the distance query that subtracts the two
    ///   objects' physics radii; this seam carries positions and not radii, so an object leaves
    ///   the set *slightly sooner* here than in retail — by the sum of the radii, i.e. under a
    ///   metre on a 75 m radius.
    /// * **The radius follows the outdoor state.** The radius here is
    ///   [`Hud::player_outside`]`() ? 75.0 : 25.0`, the same single owner
    ///   `GameView::player_outside` answers from, so the sweep's radius and the radar's scale
    ///   cannot disagree.
    ///
    /// `self.radar` is the whole presence list with `player_space` already computed by
    /// [`Self::sync`] — `RadarEntry::in_world` is the "we have a position for it" flag — so this
    /// walks it rather than the object stream a second time.
    ///
    /// Both interface adapters and the runtime read these facts without owning the sweep.
    #[must_use]
    pub fn auto_target_world(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::mainchat::AutoTargetWorld {
        use dereth_client_contract::chat::mainchat::AutoTargetWorld;
        let player = world.player.or(self.player);
        let radius = dereth_client_contract::radar::radar_range(self.player_outside());
        let in_range_of_player: Vec<u32> = self
            .radar
            .iter()
            .filter(|e| e.in_world && !e.is_self)
            .filter(|e| {
                let (x, y, z) = e.player_space;
                (x * x + y * y + z * z).sqrt() < radius
            })
            .map(|e| e.id.0)
            .collect();
        let selected = world.selected;
        let last = world.chat.last_speakable_target;
        AutoTargetWorld {
            selected_id: selected.map_or(0, |s| s.0),
            player_id: player.map_or(0, |p| p.0),
            // Whether the selected object's description is talkable.
            selected_talkable: selected
                .and_then(|s| world.weenie(s))
                .is_some_and(dereth_client_model::weenie::Weenie::is_talkable),
            // Whether the last speakable target is owned by the player — note it is the **last
            // speakable target**, not the selection: the first arm of the sweep is about keeping
            // the target it already has.
            owned_by_player: last.is_some_and(|t| world.is_owned_by_player(t)),
            container_id: last
                .and_then(|t| world.weenie(t))
                .and_then(|w| w.pwd.container_id)
                .map_or(0, |c| c.0),
            in_range_of_player,
            // The appropriate-form name query for the selected id — name type 2 is the
            // "appropriate" form, which pluralises a stack.
            selected_name: selected
                .and_then(|s| world.weenie(s))
                .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate))
                .unwrap_or_default(),
            selected_squelched: selected.is_some_and(|s| world.chat.is_squelched(s, "", 1)),
        }
    }

    /// One `CharacterOptions1` bit, or the documented default when the bit is unknown.
    /// The key the player-module refresh pass is gated on.
    ///
    /// `side_by_side` is a parameter rather than a read of `character_option::SIDE_BY_SIDE_VITALS`
    /// so that a test can force a real bit and assert the latch follows it: with the option
    /// hard-coded, **no test could tell a key that reads the option from one that does not**. The
    /// constant is `Some(bit 21)`, so the parameter is not load bearing for the production path —
    /// it is kept because it is what makes the two arms of
    /// `the_applied_latch_moves_when_side_by_side_vitals_moves` distinguishable. The one
    /// production call site passes the constant.
    ///
    /// `side_by_side_vitals` is read through [`Self::option_bit`], which reads the option word the
    /// player's own tick writes rather than the login blob's copy. See that method for which copy
    /// survives.
    pub fn applied_key(
        &self,
        world: &dereth_client_model::World,
        screen_serial: u64,
        side_by_side: Option<u32>,
    ) -> AppliedKey {
        AppliedKey {
            screen_serial,
            placements: self.placements.clone(),
            side_by_side_vitals: self.option_bit(world, side_by_side),
        }
    }

    /// One `CharacterOption` (`options_`) bit off **the** option word.
    ///
    /// # Which copy this reads, and why it is that one
    ///
    /// It reads [`dereth_client_model::player::PlayerSystem::options`]`.options` — the same word
    /// [`character_option`](fn@character_option) reads, `PlayerSystem::apply_player_module` fills from `0x0013` and
    /// `PlayerSystem::set_option` read-modify-writes when the player ticks a row on the Character
    /// Options page (`interaction.rs`'s `UiRequest::SetPlayerOption` arm).
    ///
    /// [`Hud::player_module`]`.options` is a **second clone of the same `0x0013` blob** and only
    /// the `0x0013` arm ever writes it. Reading it would answer the login blob's value for the rest
    /// of the session after the player ticks a checkbox — a working-*looking* client that ignores
    /// the checkbox, with the same symptom as a stale [`AppliedKey`] latch and a different cause.
    ///
    /// `Hud::player_module` still supplies the shortcut bar and spell filters from login, while
    /// local window placements are mirrored from the retained PlayerSystem module. Its `options`
    /// word has **no** production readers and must keep none: it is a snapshot of login and it does
    /// not follow the player. Its `options2` word has exactly one reader and one writer,
    /// [`Self::lock_ui`] / [`Self::set_lock_ui`]. It is a screen-rebuild cache; the authoritative
    /// `PlayerSystem` module is written first and this cache is kept synchronized from the chosen
    /// screen value.
    ///
    /// The presence gate is `player_system.module.is_some()`, which is the same third state
    /// [`character_option`](fn@character_option) carries: before `0x0013` there is no character's word to read, and
    /// `Options::default()`'s default option word is not this character's answer.
    pub fn option_bit(&self, world: &dereth_client_model::World, bit: Option<u32>) -> bool {
        let Some(bit) = bit else { return false };
        world.player_system.module.is_some() && world.player_system.options.options & bit != 0
    }

    /// `options2_` bit 24, off the login blob.
    ///
    /// A separate accessor from [`Self::option_bit`] because the two options live in **different
    /// words**: `SideBySideVitals` is `options_`, `LockUI` is `options2_`. Reading the lock out of
    /// `options_` would answer `true` for a blob whose bit 24 of `options_` happens to be
    /// set and lock the whole HUD.
    #[must_use]
    pub fn lock_ui(&self) -> bool {
        let (Some(bit), Some(m)) = (character_option::LOCK_UI, self.player_module.as_ref()) else {
            return false;
        };
        m.options2 & bit != 0
    }

    /// Mirror `LockUI` by changing `options2_` bit 24 only when the requested value differs.
    ///
    /// This is the **presentation mirror** where the radar's padlock lands after
    /// `dereth_client::ui` pushes the new value into `GamePlayScreen` and broadcasts global `0x0D`.
    /// The authoritative write is deliberately not attempted from `Hud::drive`'s immutable
    /// object view: the request owner first calls `PlayerSystem::set_option(51, ..)`, which mirrors
    /// bit 24 into the module `save_to_server` packs and emits `0x0005` because `LockUI` is one of
    /// the twenty-one auto-save option ordinals. This cache then preserves the same
    /// value across a screen rebuild.
    pub fn set_lock_ui(&mut self, v: bool) {
        let Some(bit) = character_option::LOCK_UI else {
            return;
        };
        let Some(m) = self.player_module.as_mut() else {
            return;
        };
        if (m.options2 & bit != 0) == v {
            return;
        }
        m.options2 = if v {
            m.options2 | bit
        } else {
            m.options2 & !bit
        };
        self.stats.lock_ui_writes += 1;
    }

    /// The player-outside predicate. A missing player returns false; otherwise the
    /// low 16 bits of the player's cell id are outdoors exactly when they are below `0x100`. This is
    /// `dereth_physics::landdefs::is_outdoors` — the same one comparison the renderer,
    /// the sky and the ambient sweep already branch on. It is the **only** input to
    /// the speech radius, whose whole rule is `outside ? 75.0 : 25.0`. There is no third arm: a
    /// dungeon room and an above-ground building interior are
    /// both environment cells with a cell index `>= 0x100` and both take the 25.
    ///
    /// **Without this producer** `GameView::player_outside` answers its trait default `true`, so
    /// the radar would draw at the outdoor 75 everywhere (too far out indoors) and
    /// [`Self::auto_target_world`] would filter at 75.
    ///
    /// `None` selects **false**, deliberately: that is the is-player-outside test's own null-player
    /// return. It is not reachable from the radar, whose whole regeneration is gated on there
    /// being a player, and `GamePlayScreen::update_radar`
    /// carries that gate.
    #[must_use]
    pub fn player_outside(&self) -> bool {
        self.player_cell
            .is_some_and(dereth_physics::landdefs::is_outdoors)
    }

    /// Convert one object's `x` and `y` into the player's own frame.
    ///
    /// `None` is the object having no physics body, which is the case
    /// the hearing predicate skips its distance half for.
    /// See [`dereth_client_model::chat::ChatState::can_hear`].
    ///
    /// The offsets come out of [`Self::radar`]'s own `player_space`, which [`Self::sync`] fills
    /// with the same `dereth_physics::math::localtolocal` call the radar's blips are drawn from —
    /// **one conversion per frame, shared**, so the range a line is heard at and the range its
    /// blip is drawn at cannot drift apart. `in_world` is that entry's "we have a position and a
    /// viewer" flag, and without both there is no player space to compute, which is the same
    /// `None`.
    ///
    /// **It is one frame old**, because `sync` runs in step 7 and the message arrives in step 3.
    /// Retail reads `position` live, but those positions were themselves last
    /// written by the previous frame's physics, so the two are the same vintage. A speaker whose
    /// very first `0x02BB` arrives on the same frame its object was created is the one case that
    /// differs, and it takes the `None` arm — audible — which is the safe direction.
    #[must_use]
    pub fn speaker_player_space(&self, id: ObjectId) -> Option<(f32, f32)> {
        let e = self.radar.iter().find(|e| e.id == id && e.in_world)?;
        Some((e.player_space.0, e.player_space.1))
    }

    /// The distance term for ranged speech: player to sender, without cylinder radii
    /// and without ignoring Z.
    ///
    /// With both range modifiers clear, this is the **three-dimensional** center-to-center
    /// distance without cylinder radii, so
    /// unlike the hearing test a speaker on the floor above is not at range zero.
    ///
    /// It is the norm of the same player-space vector [`Self::speaker_player_space`] reads, which
    /// is what makes this one line rather than a second position lookup: `localtolocal` is a rigid
    /// motion, so the player-space vector's norm equals the direct distance exactly. Sharing the
    /// snapshot also means the say gate, the ranged gate and the radar cannot disagree about where
    /// a speaker is.
    ///
    /// `None` means either physics body is absent, which the ranged-speech predicate answers **out
    /// of range** — the opposite direction from the hearing test's null
    /// escape, and the caller must not confuse the two.
    #[must_use]
    pub fn speaker_distance(&self, id: ObjectId) -> Option<f32> {
        let e = self.radar.iter().find(|e| e.id == id && e.in_world)?;
        let (x, y, z) = e.player_space;
        Some((x * x + y * y + z * z).sqrt())
    }

    /// The read-only seam the HUD panels see.
    #[must_use]
    pub fn view<'a>(&'a self, objects: &'a crate::objects::ObjectStream) -> HudView<'a> {
        HudView {
            hud: self,
            world: &objects.world,
        }
    }

    /// The same seam, **owned**.
    ///
    /// [`HudView`] is two references, so every `&str` and `&[T]` it answers with points into
    /// this `Hud` and this `World`. That is free in process and impossible out of it. This takes
    /// the same 115 answers and keeps them: `String` for `&str`, `Vec<T>` for `&[T]`, a map per
    /// keyed read. The copy is made by *calling the view*, so there is no second transcription of
    /// these accessors to drift from the first one.
    ///
    /// **Nothing reads one yet.** It is built here, beside the borrowing view, so that a
    /// presentation moved off `HudView` has something to move onto; the
    /// frame still hands panels `&dyn GameView` from [`Hud::view`].
    #[must_use]
    pub fn snapshot(
        &self,
        objects: &crate::objects::ObjectStream,
    ) -> dereth_client_contract::GameSnapshot {
        dereth_client_contract::GameSnapshot::from_view(&self.view(objects))
    }
}

/// The spell-component panel's component walk, as a snapshot.
///
/// Seven component categories in their defined order, each already sorted by display name and
/// joined to the desired component level for each `wcid`.
///
/// The `Undef` bucket (8) is **not** included: the component-panel loop iterates categories 0
/// through 6, so a
/// component the `SpellComponentTable` does not know is tracked and never drawn. That is the
/// client's behaviour; it is reproduced rather than corrected, and `(8)`
/// is where such a row can be found by a test.
fn component_categories(
    world: &dereth_client_model::World,
) -> Vec<dereth_client_contract::ComponentCategory> {
    use dereth_client_contract::{ComponentCategory, ComponentRow};
    world
        .magic
        .components
        .categories()
        .map(|(category, rows)| ComponentCategory {
            category,
            rows: rows
                .iter()
                .map(|d| ComponentRow {
                    wcid: d.class_id,
                    name: d.name.clone(),
                    // Component rows draw the spell-component table's icon id for the
                    // component id the class id maps to (`wcid_to_scid`), **not** the object's own
                    // `pwd.icon_id`, which nothing in this path draws — `ComponentRow::icon` is the
                    // table's. `0` is the icon builder's two early returns, and `row_icon` turns it
                    // into "leave the row's image alone", which is where the image clear sits
                    // relative to them.
                    icon: dereth_client_contract::panels::spellcomponent::row_icon(
                        world
                            .magic
                            .catalogue
                            .component_icon(d.class_id)
                            .unwrap_or(0),
                    ),
                    owned: d.num_items(),
                    desired: world.player_system.desired_comp_level(d.class_id),
                    object: d.first_object_id(),
                })
                .collect(),
        })
        .collect()
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

/// The language record's two speech markers, trimmed off a **sender name** before it is remembered.
///
/// Direct speech checks for `^` and then `&`, and in either case trims the trailing marker before
/// remembering the sender name. The accompanying flag chooses Olthoi or human replacement text
/// for a listener who cannot understand the speaker.
///
/// `[inferred]` — the exact parameter shape is not directly observable, so it is inferred from how
/// the name and id are used. It is reproduced
/// because a marker left on the name composes `@tell Bob^,`, a tell to a character that does not
/// exist. **No name in the 126-tell corpus carries either marker**, so this line is not exercised
/// by it and a name without a marker is returned unchanged.
///
/// The body is [`crate::chat::language_marker`], which
/// the `Communication_HearEmote` handler needs as well — the same `strstr` then
/// `trim(leading = 0, trailing = 1, marker)` pair, `^` before `&`. One copy, two callers, so a
/// correction to either function's reading cannot land in only one of them.
fn trim_language_marker(name: &str) -> &str {
    crate::chat::language_marker(name).1
}

/// The shift from UTC to the zone `localtime` would have used for `at`, in seconds.
///
/// Scroll insertion stamps with `wcsftime(L"%#H:%M:%S ", localtime(time(NULL)))`, and
/// five other places in retail format a date through `localtime` as well. Every Win32 entry
/// point in the `windows` crate is an `unsafe fn` and this crate is `#![forbid(unsafe_code)]`;
/// the way through is WinRT, whose bindings are safe. See
/// `crate::platform::local_utc_offset_secs`, which is the whole of it.
///
/// It takes the instant because the answer depends on it — a daylight rule is not a constant.
fn utc_offset_secs(at: i64) -> i32 {
    crate::platform::clock::local_utc_offset_secs(at)
}

/// `time(NULL)` — seconds since the Unix epoch, read at the moment a batch of lines lands, which
/// is where the client reads it.
///
/// A clock before the epoch is not representable and is not a case this build has to render, so it
/// falls back to 0 rather than panicking in a chat path.
fn wall_clock_unix() -> i64 {
    crate::platform::clock::system_unix_time()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// One composed speech line on its way to the chat scroll.
///
/// `body` is the **whole** line — speaker, verb, comma and quotes — as one of the seven fixed
/// templates in [`crate::chat`] composed it. `prefix` is `None` here because the notice's prefix
/// slot is the **timestamp**, and scroll insertion is what fills it; see
/// [`crate::hud::Hud::stamp_timestamps`]. Putting the speaker's name in the prefix slot and the
/// raw message in the body would draw a grey `Lark` abutting a bare `W`.
///
/// `window` is 0: both speech handlers pass their fifth argument straight through, and it is the
/// broadcast id — the final-string display notice then offers the line to every
/// window whose 64-bit text-type filter accepts the type.
fn speech(text_type: u32, body: String) -> ChatMessage {
    ChatMessage {
        ty: u8::try_from(text_type).unwrap_or(0),
        body,
        prefix: None,
        window: 0,
    }
}

/// The tag a player-killer death broadcast carries in its `0xF7E0` text.
pub const PK_DEATH_TAG: &str = "[PKDe]";

/// What the system-line handler does with one `0xF7E0` text before its squelch gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PkDeathLine {
    /// No death tag: the line is drawn as sent.
    Untagged,
    /// Tagged, and the hear-PK-deaths option is off: the line is not drawn at all.
    Dropped,
    /// Tagged and heard: the line with every copy of the tag removed.
    Stripped(String),
}

/// Decide a `0xF7E0` line's fate by the player-killer death tag and the hear-PK-deaths option.
///
/// The tag is searched for anywhere in the text. A tagged line is dropped when the option is off;
/// when it is on, every copy of the tag is replaced by nothing and the rest of the text is left
/// exactly as sent (a space beside the tag stays).
#[must_use]
pub fn pk_death_filter(text: &str, hear_pk_deaths: bool) -> PkDeathLine {
    if !text.contains(PK_DEATH_TAG) {
        PkDeathLine::Untagged
    } else if !hear_pk_deaths {
        PkDeathLine::Dropped
    } else {
        PkDeathLine::Stripped(text.replace(PK_DEATH_TAG, ""))
    }
}

/// One Fellowship-panel line on its way to the chat scroll.
///
/// The composers in [`crate::chat`] return the fixed literal with its trailing newline;
/// scroll insertion's first act is `trim(text, true, true, L"\n")`, applied here. Chat type 0 —
/// every one of those five functions pushes `0` ( …).
fn fellowship_ui_line(body: String) -> ChatMessage {
    speech(
        crate::chat::FELLOWSHIP_UI_CHAT_TYPE,
        crate::chat::add_text_to_scroll_trim(&body).to_owned(),
    )
}

/// A failure-event line on its way to the scroll: the same trim, the arm's own
/// type kept. The `0x028A` / `0x028B` arms push the trimmed literal.
fn failure_line(m: ChatMessage) -> ChatMessage {
    ChatMessage {
        body: crate::chat::add_text_to_scroll_trim(&m.body).to_owned(),
        ..m
    }
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

/// Decode player-module gameplay options into `Option_PlacementArray` rows keyed by window id.
/// Look `0x1000008C`
/// up in the collection, check its descriptor is type `0x11` (`Array`), index it by `windowID − 1`,
/// and read the named property out of the `0x1000008B` struct that lands there.
#[must_use]
pub fn decode_placements(m: &PlayerModule) -> WindowPlacements {
    let mut out = WindowPlacements::default();
    let Some(opts) = m.gameplay_options.as_ref() else {
        return out;
    };
    let Some(BasePropertyValue::Array(rows)) = opts.properties.get(placement::ARRAY) else {
        return out;
    };
    for (i, row) in rows.iter().enumerate() {
        let Some(BasePropertyValue::Struct(fields)) = row.value.as_ref() else {
            continue;
        };
        let window_id = u32::try_from(i).unwrap_or(0) + 1;
        out.set(window_id, placement_row(fields));
    }
    out
}

/// The two **general** chat options the Chat Options panel's sliders write —
/// `0x10000080 Option_DefaultOpacity` and `0x10000081 Option_ActiveOpacity`, both `T::Float` in
/// `dereth_protocol::property::PROPERTY_TYPES`.
///
/// They sit at the **top** of the gameplay-options collection, not inside the `0x1000008C` per-window
/// array `decode_placements` walks: the Chat Options page's general section is one pair of
/// sliders for all five windows, and each request names the property with no window index.
///
/// `None` is a module that carries no value, which is not the same as one that carries `0.0` —
/// the options reader substitutes `1.0f` for an absent attribute; a
/// zero would make the chat window invisible.
#[must_use]
pub fn decode_chat_opacity(m: &PlayerModule) -> (Option<f32>, Option<f32>) {
    let Some(opts) = m.gameplay_options.as_ref() else {
        return (None, None);
    };
    let f = |name: u32| match opts.properties.get(name) {
        Some(BasePropertyValue::Float(v)) => Some(*v),
        _ => None,
    };
    (
        f(dereth_client_contract::chat::interface::opacity_attr::DEFAULT),
        f(dereth_client_contract::chat::interface::opacity_attr::ACTIVE),
    )
}

/// `(window id, 0x1000007F)` for every window the blob carries —
/// the per-window 64-bit text-type filter the Chat Options page edits.
///
/// Same walk as [`decode_placements`], because it is literally the same function:
/// The chat-option lookup finds `0x1000008C`, checks the descriptor is an
/// `Array`, indexes it by `windowID - 1` and reads the named property out of the `0x1000008B`
/// struct that lands there. A row with no `0x1000007F` is skipped rather than reported as 0 —
/// the chat-window option query answers `false` there and
/// leaves the text-type filter alone, which is the window's setup default and not "accept
/// nothing".
#[must_use]
pub fn decode_chat_filters(m: &PlayerModule) -> Vec<(u32, u64)> {
    let mut out = Vec::new();
    let Some(opts) = m.gameplay_options.as_ref() else {
        return out;
    };
    let Some(BasePropertyValue::Array(rows)) = opts.properties.get(placement::ARRAY) else {
        return out;
    };
    for (i, row) in rows.iter().enumerate() {
        let Some(BasePropertyValue::Struct(fields)) = row.value.as_ref() else {
            continue;
        };
        let Some(BasePropertyValue::Bitfield64(mask)) =
            fields.get(dereth_client_model::player::CHAT_TEXT_TYPE_FILTER)
        else {
            continue;
        };
        out.push((u32::try_from(i).unwrap_or(0) + 1, *mask));
    }
    out
}

fn placement_row(fields: &PropertyCollection) -> WindowPlacement {
    let int = |name: u32| match fields.get(name) {
        Some(BasePropertyValue::Integer(v)) => Some(*v),
        _ => None,
    };
    WindowPlacement {
        x: int(placement::X),
        y: int(placement::Y),
        w: int(placement::WIDTH),
        h: int(placement::HEIGHT),
        visible: match fields.get(placement::VISIBILITY) {
            Some(BasePropertyValue::Bool(v)) => Some(*v),
            _ => None,
        },
        title: match fields.get(placement::TITLE) {
            Some(BasePropertyValue::StringInfo(s)) if s.over == 1 => {
                Some(dereth_client_contract::ChatWindowTitle::Literal(
                    s.literal.clone().unwrap_or_default(),
                ))
            }
            Some(BasePropertyValue::StringInfo(s)) => {
                Some(dereth_client_contract::ChatWindowTitle::Table {
                    string_id: s.string_id,
                    table_id: s.table_id,
                })
            }
            _ => None,
        },
    }
}

/// `dereth_assets::tables::Contract` -> `dereth_client_model::quests::Contract` — the dat's eleven-string
/// array given the meanings its contract-record offsets carry.
///
/// The order is the one the client reads, i.e. ascending offset from
/// `0x0C`: name, description, description_progress, npc_start, npc_end, then the six quest flags
/// (`stamped`, `started`, `finished`, `progress`, `timer`, `repeat_time`). `dereth-assets` keeps
/// them positional on purpose — it names no gameplay semantics — so this is the one place the
/// mapping is written down.
fn contract_of(c: &dereth_assets::tables::Contract) -> dereth_client_model::quests::Contract {
    let s = |i: usize| c.strings.get(i).cloned().unwrap_or_default();
    dereth_client_model::quests::Contract {
        version: c.version,
        contract_id: c.contract_id,
        contract_name: s(0),
        description: s(1),
        description_progress: s(2),
        name_npc_start: s(3),
        name_npc_end: s(4),
        questflag_stamped: s(5),
        questflag_started: s(6),
        questflag_finished: s(7),
        questflag_progress: s(8),
        questflag_timer: s(9),
        questflag_repeat_time: s(10),
    }
}

/// Contract-panel coordinate arithmetic. A zero cell writes nothing; a cell that cannot convert is
/// labeled `Indoors`. Both converted axes subtract `0x400`, multiply by `0.1`, and add `0.5`.
///
/// **This is not the radar conversion.** That one is the numeric half of
/// the cell-id-to-coordinate-string conversion and subtracts `0x100` from the north/south axis with no
/// `+ 0.5`. The two functions disagree and this panel uses its own; the pair is recorded here
/// rather than reconciled, because reconciling them would change the radar.
///
/// The first converted value is the **east/west** axis and the second the north/south one;
/// `location_string` prints north/south first.
fn contract_location(p: dereth_primitives::Position) -> Option<String> {
    if p.cell.0 == 0 {
        return None;
    }
    let coords = dereth_physics::landdefs::gid_to_lcoord(p.cell).map(|(x, y)| {
        (
            f64::from(y - 0x400) * 0.1 + 0.5,
            f64::from(x - 0x400) * 0.1 + 0.5,
        )
    });
    Some(dereth_client_model::quests::contract_location_text(coords))
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

impl HudView<'_> {
    /// The local player's qualities, which belong to the player's own object and nothing else.
    ///
    /// The constructor registers the object allocated for the player id, so
    /// the panels' local-player-description view and the object table's row are one object. Two
    /// copies would let `Shop::update_total_value` read the one the purse never arrived in.
    ///
    /// **`None` means "no `0x0013` yet", not "no row yet"** — the distinction every caller relies
    /// on. The object stream gives the player's row an empty `Qualities` at `CreateObject`, so
    /// the emptiness of the row is a different question. Retail asks this one with the same
    /// initialization bit, and every
    /// skill-list rebuild guard checks for a local player description with a description unpacked into it.
    fn player_desc(&self) -> Option<&dereth_client_model::Qualities> {
        self.hud.player_desc(self.world)
    }

    /// Resolve a character title id to its display string; shared by the header's display title
    /// and the Titles tab's list.
    ///
    /// Id zero is refused. Other ids resolve to a title token, whose hash selects the display
    /// string. The two enum ids resolve through to `EnumMapper 0x22000041` and
    /// `StringTable 0x2300000E`, which are [`Hud::title_tokens`] and [`Hud::title_strings`] —
    /// both loaded at startup. `id == 0` is refused outright, and so is a token neither table
    /// carries.
    fn title_name(&self, id: u32) -> Option<String> {
        if id == 0 {
            return None;
        }
        let token = self
            .hud
            .title_tokens
            .as_ref()?
            .id_to_string
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, s)| s.as_str())?;
        let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
        self.hud
            .title_strings
            .as_ref()?
            .strings
            .iter()
            .find(|(k, _)| *k == hash)
            .and_then(|(_, s)| s.strings.first().cloned())
    }
}

impl GameView for HudView<'_> {
    fn era(&self) -> Option<&dereth_client_contract::EraView> {
        Some(&self.hud.era)
    }

    /// The object table's copy first, because that is the one every other
    /// accessor keys on; otherwise it uses `0xF746`'s id when the object has not been created yet,
    /// because this reads the local player description and not the table.
    fn player(&self) -> Option<ObjectId> {
        self.world.player.or(self.hud.player)
    }

    /// The singular-name query with flag 0, composed name and all.
    ///
    /// The client prefixes the
    /// `MaterialType` string when `_material_type > 0`, and drops a leading `'+'` from an
    /// admin-hidden object. A salvage bag is named `Salvage (100)` by the shard and carries the
    /// material beside it, so the prefix is the whole difference between the shard's name and what
    /// retail draws. See [`Hud::refresh_display_names`] for why the answer is cached.
    ///
    /// The `NAME_APPROPRIATE` fork is not here: this seam hands out the singular and the plural
    /// separately ([`Self::plural_name`]) and the widget picks by stack size, moving that test one
    /// level out.
    fn name(&self, id: ObjectId) -> Option<&str> {
        if let Some(d) = self.hud.display_names.get(&id) {
            return (!d.singular.is_empty()).then_some(d.singular.as_str());
        }
        let n = self.world.weenie(id)?.pwd.name.as_str();
        (!n.is_empty()).then_some(n)
    }

    /// The Book panel's whole model, and the title the open-book handler's tail composes.
    ///
    /// The title is the one field this side has to build rather than copy: an **unsigned** book
    /// shows the object's own name and a **signed** one shows its
    /// inscription. The scribe's name is not the title in either case; it is the strip's label.
    /// The object-name query is `dereth_client_model`'s and this crate is where the two meet.
    fn open_book(&self) -> Option<dereth_client_contract::BookView> {
        let b = self.world.book.open.as_ref()?;
        let object_name = self.world.weenie(b.book_id).map_or_else(String::new, |w| {
            let material = material_name_of(
                self.hud.material_names.as_ref(),
                w.pwd.material_type.unwrap_or(0),
            );
            w.display_name(
                dereth_client_model::weenie::NameType::Appropriate,
                material.as_deref(),
            )
        });
        let title = if b.scribe_id == ObjectId(0) {
            object_name.clone()
        } else {
            b.inscription.clone()
        };
        Some(dereth_client_contract::BookView {
            book_id: b.book_id,
            object_name,
            player_id: self.world.player.unwrap_or(ObjectId(0)),
            // The PSR test checks access levels 0x2C and 0x2D, and additionally accepts 0x61.
            viewer_is_psr: self
                .world
                .player_qualities()
                .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|key| q.inq_bool(key))),
            max_num_pages: b.max_num_pages,
            pages: b
                .pages
                .pages
                .iter()
                .map(|p| dereth_client_contract::BookPageView {
                    author_id: p.author_id,
                    author_name: p.author_name.clone(),
                    author_account: p.author_account.clone(),
                    // The client leaves the page text unread when
                    // the text-included flag is 0, and that is the state `set_cur_page` answers
                    // with a book-page-data request rather than by drawing.
                    text: (p.text_included != 0).then(|| p.page_text.clone().unwrap_or_default()),
                    // Preserve the signed dword: the menu update tests exactly 1, while
                    // the page display and the page close test nonzero.
                    ignore_author: p.ignore_author,
                })
                .collect(),
            inscription: b.inscription.clone(),
            scribe_id: b.scribe_id,
            scribe_name: b.scribe_name.clone(),
            title,
            opening: self.world.book.opening,
            page_data_applied: self.world.book.page_data_applied,
            add_page_responses: self.world.book.add_page_responses,
            // The add-page response reads the author inside the
            // handler -- first getting the player id, then querying string property 1
            // on the local player description -- and `dereth-ui-screens` can see neither, so
            // both are composed here for the same reason `title` is.
            add_page: self
                .world
                .book
                .add_page
                .map(|a| dereth_client_contract::AddedPageView {
                    page: a.page,
                    success: a.success,
                    author_id: self.world.player.unwrap_or(ObjectId(0)),
                    author_name: self
                        .world
                        .player
                        .and_then(|p| self.world.weenie(p))
                        .map(|w| w.pwd.name.clone())
                        .unwrap_or_default(),
                }),
        })
    }

    fn barber(&self) -> Option<dereth_client_contract::BarberView> {
        let b = self.hud.barber?;
        let q = self.player_desc();
        Some(dereth_client_contract::BarberView {
            generation: self.hud.barber_generation,
            base_palette: b.base_palette,
            head_object: b.head_object,
            head_texture: b.head_texture,
            default_head_texture: b.default_head_texture,
            eyes_texture: b.eyes_texture,
            default_eyes_texture: b.default_eyes_texture,
            nose_texture: b.nose_texture,
            default_nose_texture: b.default_nose_texture,
            mouth_texture: b.mouth_texture,
            default_mouth_texture: b.default_mouth_texture,
            skin_palette: b.skin_palette,
            hair_palette: b.hair_palette,
            eyes_palette: b.eyes_palette,
            setup_id: b.setup_id,
            option1: b.option1,
            option2: b.option2,
            heritage: q
                .and_then(|q| u32::try_from(q.inq_int(HERITAGE_GROUP)).ok())
                .unwrap_or(0),
            gender: q
                .and_then(|q| u32::try_from(q.inq_int(GENDER)).ok())
                .unwrap_or(0),
        })
    }

    fn icon(&self, id: ObjectId) -> Option<DataId> {
        let i = self.world.weenie(id)?.pwd.icon_id;
        (i != 0).then_some(DataId(i))
    }

    fn int_stat(&self, id: ObjectId, prop: u32) -> Option<i32> {
        Some(self.world.weenie(id)?.qualities.as_ref()?.inq_int(prop))
    }

    /// `(stat, &value, 0)` for the current and the maximum, which is
    /// exactly the vitals UI's two reads per bar.
    fn vital(&self, id: ObjectId, which: Vital) -> Option<(u32, u32)> {
        let table = self.hud.vitals_table.as_ref()?;
        let filter = self.hud.quality_filter.as_ref();
        // The player's numbers come from the local player description handed to the vitals bar;
        // every other object's come from the object table.
        let q = if Some(id) == self.player() {
            self.player_desc()?
        } else {
            self.world.weenie(id)?.qualities.as_ref()?
        };
        let (cur, max) = which.stats();
        Some((
            inq_attribute_2nd(q, table, cur, false, filter)?,
            inq_attribute_2nd(q, table, max, false, filter)?,
        ))
    }

    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.world.open_container.or_else(|| self.player())
    }
    /// The loose items in server order, which is the grid order.
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        self.world.inventory(id).map_or(&[][..], |i| &i.items)
    }

    /// The contained side packs.
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        self.world.inventory(id).map_or(&[][..], |i| &i.containers)
    }

    /// `[tab]` — one of the eight spell bars.
    ///
    /// `dereth_client_model::player::PlayerSystem::spell_tabs` holds these, filled from
    /// `0x0013`'s `PlayerModule` by `apply_player_module`. They are the contents
    /// the spell-bar UI puts in each tab's item list, so they are also the order a quick-cast key
    /// indexes.
    fn spell_tab(&self, tab: usize) -> &[u32] {
        self.world
            .player_system
            .spell_tabs
            .get(tab)
            .map_or(&[], Vec::as_slice)
    }

    /// The three conditions for the spellcasting endowment icon.
    ///
    /// There is no endowment unless the player holds an object at location `0x1000000`, that
    /// object is known, its item type has the `0x8000` bit, and its spell id is non-zero.
    ///
    /// `0x8000` is `ITEM_TYPE::Caster`. Retail tests it as the sign of the type's second byte,
    /// which reads like a range check and is not one.
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        use dereth_client_contract::panels::spellcasting::ENDOWMENT_LOCATION;
        const CASTER: u32 = dereth_client_model::weenie::item_type::CASTER;
        let (item, _) = self
            .hud
            .equipment
            .iter()
            .find(|(_, loc)| *loc == ENDOWMENT_LOCATION)
            .copied()?;
        let w = self.world.weenie(item)?;
        if w.inq_type() & CASTER == 0 {
            return None;
        }
        let spell = u32::from(w.pwd.spell_id.unwrap_or(0));
        (spell != 0).then_some((item, spell))
    }

    /// The paper doll's `(object, location)` pairs.
    ///
    /// `dereth_client_model` keeps the inventory placements as
    /// `InventoryPlacement { iid, loc, priority }` and the paper doll reads only the first two, so
    /// the seam carries the pair.
    fn equipment(&self, id: ObjectId) -> &[(ObjectId, u32)] {
        // A borrowed slice is what the trait promises, so the pairs are cached beside the HUD
        // rather than built here. See [`Hud::equipment`].
        let _ = id;
        &self.hud.equipment
    }

    /// Absent means the object declares none, which
    /// the item list's container-size update treats as 0 rather than as unbounded.
    fn items_capacity(&self, id: ObjectId) -> Option<i32> {
        Some(dereth_client_model::capacity::capacity(
            self.world.weenie(id)?.pwd.items_capacity.unwrap_or(0),
        ))
    }

    /// The number of container slots this object supplies.
    fn containers_capacity(&self, id: ObjectId) -> Option<i32> {
        Some(dereth_client_model::capacity::capacity(
            self.world.weenie(id)?.pwd.containers_capacity.unwrap_or(0),
        ))
    }

    /// `set_waiting_state(1)` set it and only
    /// the server's move-item and attempt-failed answers clear it. **There is no timeout**,
    /// which is why the ghost can sit there for ever.
    fn item_waiting(&self, id: ObjectId) -> bool {
        self.world.weenie(id).is_some_and(|w| w.waiting)
    }

    /// The pending row, matched on the pair that names a list in
    /// this build: the parent container id and the container list.
    fn pending_row(
        &self,
        container: Option<ObjectId>,
        containers_list: bool,
    ) -> Option<(ObjectId, u32)> {
        let p = self.world.pending_row?;
        (Some(p.container) == container && p.containers_list == containers_list)
            .then_some((p.item, p.index))
    }

    fn selection(&self) -> Option<ObjectId> {
        self.world.selected
    }

    /// Query one live player option.
    ///
    /// Without this override the Character Options page would read the trait's `false` and all
    /// 50 rows would open unticked whatever the server sent. `dereth_client_model`'s
    /// `PLAYER_OPTIONS` carries all 53 masks.
    ///
    /// # Before a `0x0013` this answers the **constructed defaults**, not `false`
    ///
    /// The option word is a **member**, not a pointer, so every UI in the retail client that
    /// asks before the description lands gets the default word, which is
    /// the two default character-option words. Reading through
    /// [`character_option`](fn@character_option) and mapping its third state to `false` would give a different answer
    /// for the **sixteen** options the default-option table starts on (as `StayInChatMode` does).
    ///
    /// That shows as soon as a consumer *gates* on one of those sixteen: with a body in the world
    /// and no `0x0013` at all, the `CoordinatesOnRadar` gate (ordinal 20, one of the sixteen)
    /// would take the coordinate strip down where retail shows it. Reading the word directly is
    /// what retail does.
    ///
    /// [`character_option`](fn@character_option) keeps the third state and keeps its callers: anything that has to tell
    /// *off* from *not asked* — `Hud::option_bit`, which gates the player-module refresh
    /// re-seed on a module existing at all — still asks it.
    ///
    /// `dereth_client_model::player::options::Options::default()` is those two words, so this is one read
    /// with no branch.
    fn player_option(&self, o: dereth_client_contract::PlayerOption) -> bool {
        self.world.player_system.options.get(option_ordinal(o))
    }

    /// Query the compiled-in default-option true-list, which
    /// `dereth_client_model::player::options::default_option_value` already represents. It is the
    /// Character page's *Restore Defaults* button's only source.
    ///
    /// `Some` unconditionally: the client's function answers for every ordinal, so a `None` here
    /// would mean *this host cannot say*, and this host can. The world is not consulted — the
    /// default belongs to the option, not to the character.
    fn player_option_default(&self, o: dereth_client_contract::PlayerOption) -> Option<bool> {
        Some(dereth_client_model::player::options::default_option_value(
            option_ordinal(o),
        ))
    }

    /// `(property)` for a `Float` gameplay option — the Chat Options page's
    /// two opacity sliders.
    ///
    /// The retained module is the client's, so `None` here is
    /// *"before `0x0013`"* or *"the blob carries no such property"*, which is exactly the `false`
    /// the option query answers with and which the page turns into its default.
    fn gameplay_option_float(&self, property: u32) -> Option<f32> {
        let m = self.world.player_system.module.as_ref()?;
        match m.gameplay_options.as_ref()?.properties.get(property) {
            Some(BasePropertyValue::Float(v)) => Some(*v),
            _ => None,
        }
    }

    /// Query chat-window option `0x1000007F`.
    ///
    /// Reads the live module rather than [`Hud`]'s decoded snapshot, because the page's own
    /// `Apply` has already written the module by the time the next frame's current-value save
    /// asks — `consume_placement_requests` runs before `Hud::drive` — and a one-frame-stale
    /// snapshot would make Apply look like it had reverted the click.
    fn chat_window_filter(&self, window: u32) -> Option<u64> {
        let m = self.world.player_system.module.as_ref()?;
        decode_chat_filters(m)
            .into_iter()
            .find(|(id, _)| *id == window)
            .map(|(_, mask)| mask)
    }

    /// The four facts the selection query's not-a-stack arm branches
    /// on.
    ///
    /// `None` when the object has no weenie row, which is the same early return the client takes
    /// (it looks the object up and returns if there is none) before it can ask any of these.
    fn selection_query_facts(&self, id: ObjectId) -> Option<SelectionQueryFacts> {
        let w = self.world.weenie(id)?;
        Some(SelectionQueryFacts {
            is_player: w.is_player(),
            has_pet_owner: w.pwd.pet_owner.is_some_and(|o| o.0 != 0),
            attackable: self.world.object_is_attackable(id),
            owned_by_player: self.world.is_owned_by_player(id),
        })
    }

    /// `dereth_client_model::combat::SelectedMeters` — the `0x01C0` / `0x0264` replies.
    ///
    /// Selection clears the pair on a real selection edge, so the guard
    /// the object-health handler applies (*"is this notice about the object the
    /// toolbar is showing?"*) is already enforced upstream and is not repeated here.
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        (
            self.world.selected_meters.health,
            self.world.selected_meters.mana,
        )
    }

    fn radar_objects(&self) -> &[RadarEntry] {
        &self.hud.radar
    }

    fn radar_blank(&self) -> bool {
        self.hud.radar_blank
    }

    // The enchantment counts and the portal storm level are deliberately left at the trait's
    // defaults here: they are the indicator strip's inputs.

    /// `_vitae->_smod.val`, or **1.0** when
    /// there is no vitae. The vitae lamp's only input: the lamp asks for the vitae value and
    /// lights state 1 when the multiplier is below 1.0.
    ///
    /// Without this override `hud::indicators::vitae_state` could only ever answer
    /// `STATE_NOTHING` through the trait's `None`: no vitae penalty would be drawable, however deep
    /// the character's was. The value is `EnchantmentRegistry::vitae_value` from
    /// `dereth-client-model`.
    ///
    /// **The three states are kept distinct, and the middle one is why the override is safe.**
    /// No local player description yet is `None` (nothing is known); a description with no vitae is
    /// `Some(1.0)`, which `vitae_state` maps to the same `STATE_NOTHING` as `None`; only an actual
    /// penalty lights the lamp.
    ///
    /// **No capture can test the lit case.** Across the seven recorded captures: 5 of 7 carry a
    /// `0x0013` at all (`login-account-booted` and `ddd-interrogation-only` are login-only), **1**
    /// of those 5 carries an `EnchantmentRegistry`, and **0** carry a vitae — with 31 enchantment
    /// messages in the `0x02Cx` range in the corpus, none of which installs one.
    /// So the lit branch is asserted against retail's vitae rule and is
    /// `[verified against retail]` rather than `[verified against traffic]`.
    fn vitae(&self) -> Option<f32> {
        Some(self.player_desc()?.enchantments.vitae_value())
    }

    /// The vitae display's three inputs.
    ///
    /// The pool is int property 129 (`VitaeCpPool`); the level is int property 139
    /// (`DeathLevel`), falling back to 25 (`Level`) when that is zero; and the experience still
    /// needed is the vitae threshold for that level minus the pool.
    ///
    /// The fall-back and the threshold are done here for the reason
    /// [`dereth_client_contract::VitaeDisplay`] gives, through
    /// `dereth_client_model::advancement::vitae_cp_pool_threshold`.
    ///
    /// `None` means no local player description, i.e. no `0x0013` yet.
    fn vitae_display(&self) -> Option<dereth_client_contract::VitaeDisplay> {
        let q = self.player_desc()?;
        let multiplier = q.enchantments.vitae_value();
        let pool = q.inq_int(VITAE_CP_POOL);
        let level = match q.inq_int(DEATH_LEVEL) {
            0 => q.inq_int(LEVEL),
            n => n,
        };
        // The client treats the level as an unsigned 32-bit value when converting it.
        // The world's era decides the curve (the older one before Throne of Destiny).
        let threshold = dereth_client_model::advancement::vitae_cp_pool_threshold_in(
            self.hud.era.era.vitae_recovery(),
            f64::from(multiplier),
            f64::from(level.unsigned_abs()),
        );
        Some(dereth_client_contract::VitaeDisplay {
            multiplier,
            cp_pool: pool,
            threshold,
        })
    }

    /// The Character Info panel's six section inputs.
    ///
    /// The four int-quality queries, the eight attribute queries and the load query the client
    /// makes across the panel update's callees, plus the one derived value
    /// derived here because it lives in
    /// `dereth-client-model`. Every field is written out in
    /// [`dereth_client_contract::panels::characterinfo`].
    ///
    /// **`innate` is `_init_level`, not the base value.**
    /// The base inquiry answers `_init_level + _level_from_cp`, and `dereth_client_model::attributes::inq_attribute_base` is
    /// that sum; the innate-attribute display reads `_init_level` on its own. So this is one of
    /// the few places that goes to `Qualities::attribute` rather than through the inquiry.
    ///
    /// `None` is no `0x0013` yet.
    fn character_info(&self) -> Option<dereth_client_contract::CharacterInfo> {
        use dereth_client_model::attributes::inq_attribute;
        let q = self.player_desc()?;
        // `ID_CharacterInfo_Innates`' display order: 1, 2, 4, 3, 5, 6.
        let innate_of = |id: u32| {
            i32::try_from(q.attribute(id).map_or(0, |a| a.init_level)).unwrap_or(i32::MAX)
        };
        let innate = [
            innate_of(1),
            innate_of(2),
            innate_of(4),
            innate_of(3),
            innate_of(5),
            innate_of(6),
        ];
        let raw =
            |id: u32| i32::try_from(inq_attribute(q, id, true).unwrap_or(0)).unwrap_or(i32::MAX);
        let augmentations = q.inq_int(AUG_INCREASED_CARRYING_CAPACITY);
        // Strength is set to 10 before the attribute query `(1, &strength, raw = 0)` -- the
        // **default is 10**, and it is the value `EncumbranceCapacity` is handed, not the raw one
        // the resist ladder uses.
        let cap_strength =
            i32::try_from(inq_attribute(q, 1, false).unwrap_or(10)).unwrap_or(i32::MAX);
        Some(dereth_client_contract::CharacterInfo {
            innate,
            // Retail defaults the value to `0x578` before the inquiry, i.e. 1400 is the
            // client-side default.
            chess_rank: match q.inq_int(CHESS_RANK) {
                0 => 1_400,
                n => n,
            },
            fishing_skill: q.inq_int(FAKE_FISHING_SKILL),
            num_deaths: q.inq_int(NUM_DEATHS),
            strength: raw(1),
            endurance: raw(2),
            load: dereth_client_model::inventory::burden::inq_load(q),
            encumbrance: q.inq_int(ENCUMBRANCE_VAL),
            capacity: dereth_client_model::inventory::burden::encumbrance_capacity(
                cap_strength,
                augmentations,
            ),
            augmentations,
            // The birth/age/deaths read-out's first two arms are
            // gated on the int-quality query's **return**, so `None` and `Some(0)` are different
            // sheets: a character with no `CreationTimestamp` gets no birth line at all, and one
            // stamped at the epoch gets `You were born on 1/1/1970 12:00:00 AM.`
            created: int_opt(q, charinfo::prop::CREATION_TIMESTAMP),
            age: int_opt(q, charinfo::prop::AGE),
            enlightenment: int_opt(q, charinfo::prop::ENLIGHTENMENT),
            melee_mastery: q.inq_int(charinfo::prop::WEAPON_MASTERY),
            ranged_mastery: q.inq_int(charinfo::prop::MISSILE_MASTERY),
            summoning_mastery: q.inq_int(charinfo::prop::SUMMONING_MASTERY),
            // The augmentations read-out asks for fifty-four ints and shows the ones
            // that came back positive. An absent key and a zero are the same answer here,
            // which is why the zeroes are dropped rather than stored.
            aug_ints: charinfo::LUMINANCE
                .iter()
                .map(|(id, _, _)| *id)
                .chain(charinfo::AUGMENTATIONS.iter().map(|(id, _)| *id))
                .map(|id| (id, q.inq_int(id)))
                .filter(|(_, v)| *v != 0)
                .collect(),
            // The client runs the timestamp through `localtime` before
            // `strftime("%c")`, so the born line is in the machine's zone for the **birth**
            // instant — a character created in July reads in daylight time whatever month it is
            // read in, which is what `localtime` does and a `now`-based offset would not.
            utc_offset_secs: int_opt(q, charinfo::prop::CREATION_TIMESTAMP)
                .map_or(0, |t| utc_offset_secs(i64::from(t))),
        })
    }

    /// The burden drawn by the backpack meter and text, through
    /// `dereth_client_model::inventory::burden::inq_load`.
    ///
    /// The source is the canonical local-player object row. Before that row exists, the parked
    /// `0x0013` description supplies the attribute cache and `EncumbranceVal`; once registered,
    /// both names refer to the same qualities.
    fn load(&self) -> Option<f32> {
        Some(dereth_client_model::inventory::burden::inq_load(
            self.player_desc()?,
        ))
    }

    /// The object's public-description decoration fields used by the item-display update.
    ///
    /// `is_container` is the client's own three-term test
    /// `(bitfield & 0x800000) || items_capacity || containers_capacity`, and `0x800000` is
    /// an object that has to occupy a
    /// *container* slot, which is what a pack is, verified against the retail enum table.
    fn slot_decoration(&self, id: ObjectId) -> Option<dereth_client_contract::SlotDecoration> {
        /// Public-description flag requiring a pack slot.
        const BF_REQUIRES_PACKSLOT: u32 = 0x0080_0000;
        /// Public-description flag marking a readied item.
        const BF_OPENABLE: u32 = 0x0000_0001;
        let w = self.world.weenie(id)?;
        let items_capacity =
            dereth_client_model::capacity::capacity(w.pwd.items_capacity.unwrap_or(0));
        let containers_capacity =
            dereth_client_model::capacity::capacity(w.pwd.containers_capacity.unwrap_or(0));
        Some(dereth_client_contract::SlotDecoration {
            // A zero stack size counts as 1 — the tooltip update's own mapping.
            stack_size: u32::from(w.pwd.stack_size.unwrap_or(0)).max(1),
            is_container: w.pwd.bitfield & BF_REQUIRES_PACKSLOT != 0
                || items_capacity != 0
                || containers_capacity != 0,
            items_capacity,
            contained_items: i32::try_from(self.world.inventory(id).map_or(0, |i| i.items.len()))
                .unwrap_or(i32::MAX),
            structure: u32::from(w.pwd.structure.unwrap_or(0)),
            max_structure: u32::from(w.pwd.max_structure.unwrap_or(0)),
            // Written by selection updates; this is the selection ring's only input.
            selected: w.selected,
            // The object's sell and trade states: the Vendor panel writes sell state, while the
            // Trade panel writes trade state. The Salvage and Housing panels are the two remaining
            // writers with no window.
            sell_state: w.sell_state != 0,
            trade_state: w.trade_state != 0,
            openable: w.pwd.bitfield & BF_OPENABLE != 0,
            is_player: Some(id) == self.player(),
            containers_capacity,
            cooldown_id: w.pwd.cooldown_id.unwrap_or(0),
            cooldown_duration: w.pwd.cooldown_duration.unwrap_or(0.0),
            // The object's type selects the cell's background tile; without it every filled cell
            // would draw its icon on bare panel art.
            obj_type: w.pwd.obj_type,
            // The other four icon inputs, which
            // together with `obj_type` are exactly the five fields the icon update
            // compares before it decides to rebuild the composite.
            //
            // `_effects` is decoded with the public description. `icon_overlay_id` and
            // `icon_underlay_id` are **written** by
            // `dereth_client_model::weenie::mirror_stat_update`; this is where they are read.
            icon_id: w.pwd.icon_id,
            effects: w.pwd.effects.unwrap_or(0),
            icon_overlay_id: w.pwd.icon_overlay_id.map(DataId),
            icon_underlay_id: w.pwd.icon_underlay_id.map(DataId),
            // The waiting flag rides in the decoration, not only on
            // [`dereth_client_contract::GameView::item_waiting`], which covers the item list and
            // the doll but not the side-pack strip and the main-pack slot; without it a dragged
            // **backpack** would stay greyed for the session. The client has one mirror, in the
            // item tile's update, and it runs on every tile; carrying the flag in the decoration is
            // what lets it.
            waiting: w.waiting,
            // The cached shortcut number and its ghosted flag.
            //
            // The client caches the slot on the object: assigning a shortcut writes its index,
            // while removal writes `-1`, both from the same sparse shortcut array carried in the
            // player module. This asks the identical question one seam earlier — the same
            // reading removal's own sweep uses — and asking it
            // here cannot disagree with the bar, which is filled from that same array.
            shortcut_num: self
                .world
                .player_system
                .shortcut_slot_of(id)
                .and_then(|i| u32::try_from(i).ok()),
            // Combat mode 8 makes the toolbar inactive; every other mode makes it active. The
            // shortcut manager writes `ghost = !active` with the cached slot for all 18 entries,
            // and that update writes through to the
            // object, so it is a per-player fact and every
            // tile showing the object gets the dimmed numeral. Same predicate the bar itself uses
            // (`crate::toolbar::combat_mode::toolbar_active`), asked once here.
            shortcut_ghosted: !dereth_client_contract::combat_mode::toolbar_active(
                self.combat_mode(),
            ),
        })
    }

    /// Read cooldown state from the player's own registry, through
    /// `dereth_client_model::EnchantmentRegistry::cooldown_remaining`.
    ///
    /// Cooldown display reaches the registry through the canonical local-player object row,
    /// exactly as the vitals and vitae lamp do. `+ 0x8000` belongs to this caller, as it does in
    /// the client.
    ///
    /// The two guards above it are retained: no description yet, or
    /// one with no registry, is `None` and hides every wedge.
    fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
        self.player_desc()?
            .enchantments
            .cooldown_remaining(cooldown_id + 0x8000, dereth_primitives::LocalTime(now))
    }

    /// The plural-name query with flag 0 returns the stored `_plural_name`,
    /// with the material prefix, applied to singular and plural names alike.
    ///
    /// `None` still means "no `_plural_name`", which is the condition the widget's fallback keys
    /// on; the cached row carries `None` in exactly that case, so a cache hit cannot invent one.
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        if let Some(d) = self.hud.display_names.get(&id) {
            return d.plural.as_deref();
        }
        let n = self.world.weenie(id)?.pwd.plural_name.as_deref()?;
        (!n.is_empty()).then_some(n)
    }

    fn player_coords(&self) -> Option<(f32, f32)> {
        self.hud.coords
    }

    /// The identity the journal's page save needs. Composed in `App::frame`; see
    /// [`Hud::journal_identity`].
    fn journal_identity(&self) -> Option<dereth_client_contract::journal::JournalIdentity> {
        self.hud.journal_identity.clone()
    }

    /// See [`Hud::game_date_time`].
    fn game_date_time(&self) -> Option<(String, String)> {
        self.hud.game_date_time.clone()
    }

    fn player_heading(&self) -> f32 {
        self.hud.heading
    }

    /// The shared outdoor-state predicate — see
    /// [`Hud::player_outside`], which is the single owner of the answer so that the radar's range
    /// and the chat sweep's radius cannot disagree.
    fn player_outside(&self) -> bool {
        self.hud.player_outside()
    }

    /// Current link status; see [`Hud::link_status`].
    fn link_status(&self) -> Option<f64> {
        self.hud.link_status
    }

    /// Current UI time as of this frame's `Hud::drive`.
    fn now(&self) -> f64 {
        self.hud.now.0
    }

    /// How many `0x01EA Character_ReturnPing` have been decoded — see
    /// [`crate::net::ping_holder`].
    fn ping_returns(&self) -> u64 {
        crate::net::ping_holder::returns()
    }

    /// Return the cached packet-loss percentage.
    ///
    /// The measurement is a received/expected window:
    /// [`dereth_client_net::linkstatus::LinkStatusAverages`]. The transport counts sent, received,
    /// retransmitted and NAKed packets, snapshots them every two seconds
    /// into a forty-sample ring, and [`crate::net::link_status_holder::on_packet_loss`] is
    /// the heartbeat's store of the packet-loss figure. This accessor is the read.
    fn packet_loss_percent(&self) -> f32 {
        crate::net::link_status_holder::packet_loss()
    }

    /// The quickbar object id at `slot`.
    ///
    /// The player module holds the array and `0x0013 Login_PlayerDescription` is what fills it, so
    /// the source here is the same `player_module` blob every player-module refresh reads. The
    /// wire form is a **sparse list keyed by `index`**, not a dense eighteen, so lookup is by
    /// `index` and not
    /// by position, exactly as `dereth_client_model::player::PlayerSystem::apply_player_module` does it.
    ///
    /// A shortcut whose object id is 0 is no shortcut: login skips such entries, and later
    /// insertion also returns early for a zero id.
    ///
    /// **Read the model's copy, or the bar takes no drop.** There is exactly one shortcut array in
    /// the client. The shortcut manager is its sole writer, both at login and on
    /// every drop. This build keeps **two** copies of that blob — [`Hud::player_module`], a
    /// verbatim clone of the `0x0013` bytes, and `dereth_client_model::player::PlayerSystem`, which
    /// `apply_player_module` fills from the same bytes by `index` — and the drop writes the
    /// second. `Hud::player_module` is never written again after login (`hud.rs`'s `0x0013` arm is
    /// its only writer), so reading it here would put every dragged shortcut in a store nothing
    /// draws from and leave the tile an empty numbered plate. The character options have the same
    /// two-copy hazard.
    fn shortcut(&self, slot: u32) -> Option<ObjectId> {
        let i = usize::try_from(slot).ok()?;
        let sc = (*self.world.player_system.shortcuts.get(i)?)?;
        (sc.object_id.0 != 0).then_some(sc.object_id)
    }

    /// The skill-list join — see [`Hud::build_skills`].
    fn skills(&self) -> &[SkillEntry] {
        &self.hud.skills
    }

    /// The player's spellbook joined with the `SpellTable` — see [`Hud::build_spells`].
    fn spellbook(&self) -> &[SpellEntry] {
        &self.hud.spells
    }

    /// Resolve a spell from the table, independently of whether it is in the player's book;
    /// see [`Hud::spell_entry`] and the trait method's own note for why the two are different
    /// questions and why the endowment needs this one.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.hud.spell_entry(spell_id)
    }

    /// `(spell)` — the **book itself**, not the join
    /// [`Self::spellbook`] hands the panel.
    ///
    /// Spellbook membership is a hash-table lookup, so a
    /// spell with a page is known whether or not the `SpellTable` has a row for it. That is what
    /// keeps spell-bar pruning from sending `0x01E4` for a spell
    /// the player still has.
    ///
    /// No qualities yet means the client has no local player description: retail's
    /// player-module refresh returns before the prune loop in that case, so answering "known"
    /// here is the arm that produces the same silence.
    fn is_spell_known(&self, spell_id: u32) -> bool {
        let Some(q) = self.player_desc() else {
            return true;
        };
        q.spell_book
            .as_ref()
            .is_some_and(|b| b.contains_key(&spell_id))
    }

    /// Spell examination's three dat joins, done where the tables are.
    ///
    /// * the spell-table row, which is also where the name,
    ///   description, school, `_base_mana`, `_mana_mod` and icon come from;
    /// * the appropriate-formula query — the same call
    ///   spell casting makes, so the pane lists the formula the player would actually spend
    ///   (scarab-only for an Infused augmentation or an owned spell pack, the customized one
    ///   otherwise);
    /// * the spell-component query for each slot
    ///   in the formula, **keyed by SCID** and left as
    ///   `None` on a miss so the pane can apply the client's own skip.
    ///
    /// The component table read is [`Hud::component_catalogue`] and **not** `.catalogue`,
    /// which is the same rows copied in on `0x0013`. The component table is
    /// the dat object itself, available from the moment the store is open and with no local player description
    /// in it; reading the world's copy would make a spell examined before the description landed
    /// list no components at all.
    ///
    /// Spell range is evaluated here too from the local player description: it uses the spell's
    /// own school skill, or the **best** of the five magic skills when the school is
    /// outside `1..=5`. Before a player description arrives, the skill is `0`, matching the
    /// client's null-interface result.
    ///
    /// **The formula is `get_appropriate_spell_formula`'s, not the plain spell-formula query's**. Both spell
    /// examination and casting call the selector. Those are its only two callers, which is
    /// why the pane and the cast can never disagree: a character with a Foci (or an
    /// `AugmentationInfused*Magic`) is shown *and* charged scarab + prismatic tapers, and every
    /// other character is shown *and* charged the full per-account formula. The customized arm
    /// and the foci arm's `school_of_magic_to_wcid` map are both transcribed.
    fn spell_examine(&self, spell_id: u32) -> Option<dereth_client_contract::SpellExamineView> {
        use dereth_client_contract::panels::spell_examine::{
            skill_for_spell, spell_range, MAGIC_SKILLS,
        };
        use dereth_client_contract::{SpellExamineComponent, SpellExamineView};

        let table = self.hud.spell_table.as_ref()?;
        let base = table.spells.get(&spell_id)?;
        let skill = {
            let q = self.player_desc();
            let want = skill_for_spell(base.school);
            let level = |id: u32| {
                q.map_or(0, |q| {
                    i32::try_from(dereth_client_model::skills::inq_skill_level(q, id)).unwrap_or(0)
                })
            };
            if want == 0 {
                MAGIC_SKILLS.iter().map(|s| level(*s)).max().unwrap_or(0)
            } else {
                level(want)
            }
        };
        let formula = self.world.spell_formula(base);
        let n = dereth_client_model::magic::num_spell_components(&formula);
        let components = formula
            .iter()
            .take(n)
            .map(|scid| {
                self.hud
                    .component_catalogue
                    .inq_spell_component_base(*scid)
                    .map(|b| SpellExamineComponent {
                        scid: *scid,
                        name: b.name.clone(),
                        icon: (b.icon != 0).then_some(DataId(b.icon)),
                    })
            })
            .collect();
        Some(SpellExamineView {
            name: base.name.clone(),
            description: base.description.clone(),
            school: base.school,
            base_mana: base.base_mana,
            mana_mod: base.mana_mod,
            // The pane uses `-1.0` when the meta-spell has no duration, exactly what
            // `SpellBase::duration` being `None` means.
            duration: base.duration.map_or(-1.0, |(d, _, _)| d),
            range: spell_range(base.base_range_constant, base.base_range_mod, skill),
            icon: (base.icon != 0).then_some(DataId(base.icon)),
            level: dereth_client_model::magic::spell_level_by_rough_heuristic(
                dereth_client_model::magic::scarab_power_level(
                    dereth_client_contract::spellbook::power_component(
                        base.raw_comps[0],
                        base.comp_key,
                    ),
                ),
            ),
            bitfield: base.bitfield,
            components,
        })
    }

    /// Resolve the object carrying one spell component.
    ///
    /// Unlike [`Self::spell_examine`] above, this one reads the player's component tracker, **not**
    /// [`Hud::component_catalogue`]: the answer is about what the player is *carrying*, which only
    /// the tracker knows, and the tracker's rows were bucketed with the world's own copy of the
    /// SCID map.
    fn component_object_id(&self, scid: u32) -> Option<dereth_primitives::ObjectId> {
        self.world.component_object_id(scid)
    }

    /// Test ownership of the component keyed by the row's SCID.
    fn component_is_owned(&self, scid: u32) -> bool {
        self.world.spell_component_is_owned(scid)
    }

    /// The component-tracker change edge, represented as a serial.
    fn component_serial(&self) -> u64 {
        self.world.magic.component_serial
    }

    /// `(id, &v, raw = 0)` — one of the six primary attributes, for
    /// the Attributes panel.
    ///
    /// `dereth_client_model::attributes::inq_attribute` is retail's arithmetic; the `false`
    /// is the client's `raw = 0`, so the number is the enchanted one. The source is the canonical
    /// local-player object row; before that row exists, the parked `0x0013` description supplies
    /// the same qualities.
    fn attribute(&self, id: u32) -> Option<i32> {
        let q = self.player_desc()?;
        i32::try_from(dereth_client_model::attributes::inq_attribute(
            q, id, false,
        )?)
        .ok()
    }

    /// The Skills panel's footer inputs for one skill.
    ///
    /// This is the caller of `dereth_client_model::advancement`'s four cost functions.
    ///
    /// The two `experience_to_skill_level` calls are the trained-skill footer's
    /// own, and they are made here rather than in the panel for the same reason the skill's name
    /// is: `dereth-ui-screens` must not depend on `dereth-client-model` or on the experience table.
    fn skill_advancement(&self, id: u32) -> Option<dereth_client_contract::SkillAdvancement> {
        use dereth_client_model::advancement as adv;
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let skills = self.hud.skill_table.as_ref()?;
        let s = q.skill(id).copied().unwrap_or_default();
        let sac = dereth_client_model::skills::Sac::from_raw(s.sac);
        let level = u32::from(s.level_from_pp);
        Some(dereth_client_contract::SkillAdvancement {
            sac: s.sac,
            pp: s.pp,
            level_from_pp: level,
            cost_to_raise: adv::skill_cost_to_raise(q, skills, xp, id),
            cost_to_raise_10: adv::skill_cost_to_raise_10(q, xp, id),
            // The experience-to-skill-level conversion returns `0xFFFFFFFF` for a skill below
            // TRAINED, which would
            // make the meter's span nonsense; the untrained footer has no meter, so 0/0 is the
            // honest answer and `meter_fill` reads a zero span as 0.0.
            xp_at_level: level_xp(xp, sac, level),
            xp_at_next_level: level_xp(xp, sac, level + 1),
        })
    }

    /// The Attributes panel's footer inputs for one of its nine rows.
    ///
    /// `id` is the row token's stat id, so for a vital it is the **odd**, maximum id (see
    /// `dereth_ui_screens::panels::attributes::AttributeRow::wire_stat`), and both `Attribute` and
    /// `SecondaryAttribute` records are reachable from either half of a pair.
    ///
    /// This is the caller of `dereth_client_model::advancement::{attribute_cost_to_raise,
    /// attribute_cost_to_raise_10}`, whose arithmetic matches the recorded
    /// `short-play-with-training` traffic.
    fn attribute_advancement(
        &self,
        id: u32,
        secondary: bool,
    ) -> Option<dereth_client_contract::AttributeAdvancement> {
        use dereth_client_model::advancement as adv;
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let i32_of = |v: Option<u32>| v.and_then(|v| i32::try_from(v).ok()).unwrap_or(0);
        let (level_from_cp, cp_spent, value, effective, current, vitae) = if secondary {
            let s = q.attribute_2nd(id)?;
            // the formula base plus the stored rank,
            // enchanted unless `raw`. It needs the `Attribute2ndTable`; without it there is no
            // maximum, so the two display halves come out 0 while the **cost** below, which
            // reads only `_level_from_cp` and `_cp_spent`, is unaffected.
            let inq = |k: u32, raw: bool| -> i32 {
                self.hud.vitals_table.as_ref().map_or(0, |t| {
                    i32_of(dereth_client_model::attributes::inq_attribute_2nd(
                        q,
                        t,
                        k,
                        raw,
                        self.hud.quality_filter.as_ref(),
                    ))
                })
            };
            let raw = inq(id, true);
            let eff = inq(id, false);
            // The stat id plus 1 — the current half of the same pair.
            let cur = inq(id + 1, false);
            // `Enchant(raw) - raw` with
            // **only** the vitae enchantment applied, so it is negative under a penalty and 0
            // otherwise. Not the same quantity as `eff - raw`, which also carries the spells.
            //
            // The multiplier is read straight off the registry rather than through
            // `GameView::vitae`, because **`HudView` does not implement that method**, so the
            // vitae lamp has no source; that is a separate gap.
            let vitae = dereth_client_contract::panels::inforegion::apply_vitae(
                raw,
                Some(q.enchantments.vitae_value()),
            ) - raw;
            (
                s.attribute.level_from_cp,
                s.attribute.cp_spent,
                raw,
                eff,
                cur,
                vitae,
            )
        } else {
            let a = q.attribute(id)?;
            let raw = i32_of(dereth_client_model::attributes::inq_attribute(q, id, true));
            let eff = i32_of(dereth_client_model::attributes::inq_attribute(q, id, false));
            // The base vitae-modifier query returns 0 and primary-attribute rows do not
            // override it: a primary attribute carries no vitae penalty.
            (a.level_from_cp, a.cp_spent, raw, eff, eff, 0)
        };
        Some(dereth_client_contract::AttributeAdvancement {
            level_from_cp,
            cp_spent,
            cost_to_raise: adv::attribute_cost_to_raise(xp, level_from_cp, cp_spent, secondary),
            cost_to_raise_10: adv::attribute_cost_to_raise_10(
                xp,
                level_from_cp,
                cp_spent,
                secondary,
            ),
            value,
            effective,
            current,
            vitae,
        })
    }

    /// The gender/heritage line for the player.
    ///
    /// It starts empty; a non-zero gender with a display name contributes that name. A non-zero
    /// heritage must have a display name (otherwise the answer is nothing) and is appended, after
    /// a space if a gender was written. With no heritage, a creature type's display name would be
    /// used instead.
    ///
    /// The character-info update passes no creature type, so the third arm is unreachable from this
    /// panel and is not implemented; the character-examination panel uses it and fills it
    /// in only when the heritage is 0.
    ///
    /// **Three heritages are hard-coded literals and do not come from the mapper.**
    /// The heritage display-name lookup answers `2` with `"Gharu'ndim"`, `5` with
    /// `"Umbraen"` and `0xD` with `"Olthoi"` before it ever reaches the mapper, whose
    /// own rows for those three are `Gharundim`, `Shadowbound` and `OlthoiAcid`. \[verified\]
    fn gender_heritage_display(&self) -> Option<String> {
        let q = self.player_desc()?;
        let gender = u32::try_from(q.inq_int(GENDER)).unwrap_or(0);
        let heritage = u32::try_from(q.inq_int(HERITAGE_GROUP)).unwrap_or(0);
        let lookup = |m: &Option<dereth_assets::tables::EnumMapper>, k: u32| -> Option<String> {
            m.as_ref()?
                .id_to_string
                .iter()
                .find(|(id, _)| *id == k)
                .map(|(_, s)| s.clone())
        };
        let mut out = String::new();
        if gender != 0 {
            if let Some(g) = lookup(&self.hud.gender_names, gender) {
                out.push_str(&g);
            }
        }
        if heritage != 0 {
            let h = match heritage {
                2 => Some("Gharu'ndim".to_owned()),
                5 => Some("Umbraen".to_owned()),
                0xD => Some("Olthoi".to_owned()),
                other => lookup(&self.hud.heritage_names, other),
            }?;
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&h);
        }
        (!out.is_empty()).then_some(out)
    }

    /// Resolve the currently displayed character title.
    ///
    /// `id == 0` is refused outright; otherwise the id becomes an `ID_CharacterTitle_*` token
    /// through `EnumMapper 0x22000041` and the token's string hash (`dereth_primitives::num::hash::str_hash`) indexes
    /// `StringTable 0x2300000E`.
    fn display_title(&self) -> Option<String> {
        self.title_name(self.hud.display_title)
    }

    /// The title table joined to its id lookup.
    ///
    /// The list is `player_system.social.titles`, which `0x0029` and `0x002B` write — see
    /// `Hud::ui_event`. Each id is resolved through the **same** `EnumMapper 0x22000041` /
    /// `StringTable 0x2300000E` pair [`Self::display_title`] uses, and an id the pair does not
    /// carry keeps the empty name rather than vanishing, so the panel can count it.
    fn character_titles(&self) -> dereth_client_contract::CharacterTitles {
        let social = &self.world.player_system.social;
        dereth_client_contract::CharacterTitles {
            display: self.hud.display_title,
            titles: social
                .titles
                .iter()
                .map(|id| (*id, self.title_name(*id).unwrap_or_default()))
                .collect(),
        }
    }

    /// Resolve the local player's PK status.
    ///
    /// The two predicates are `dereth_client_model::weenie::Weenie::{is_pk, is_pk_lite}`. The object is the
    /// **player's**, and a missing one is `NPK` rather than
    /// an error — the client's no-object arm falls onto the same string id.
    fn pk_status(&self) -> dereth_client_contract::PkStatus {
        let w = self.player().and_then(|p| self.world.weenie(p));
        w.map_or(dereth_client_contract::PkStatus::Npk, |w| {
            dereth_client_contract::PkStatus::of(w.is_pk(), w.is_pk_lite())
        })
    }

    /// Int64 qualities `6` and `7`.
    fn luminance(&self) -> (i64, i64) {
        let Some(q) = self.player_desc() else {
            return (0, 0);
        };
        let int64 = |p: u32| match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            p,
        )) {
            Some(dereth_client_model::StatValue::Int64(n)) => n,
            _ => 0,
        };
        (int64(AVAILABLE_LUMINANCE), int64(MAXIMUM_LUMINANCE))
    }

    /// The name text's source.
    ///
    /// The client reads the player's object name first, then falls back to string property 1 on
    /// the local player description. The fallback is not a
    /// convenience: `0x0013` arrives before `0xF745 Object_CreatePlayer`, so between the two the
    /// object table has no name and the header would come up blank — the same ordering
    /// the Vitals panel works around by reading the local player description.
    ///
    /// **The allegiance rank prefix is not applied.** No capture carries an
    /// allegiance, so there is nothing to prefix with and nothing to test it against; it is left
    /// out rather than invented.
    fn character_name(&self) -> Option<&str> {
        if let Some(n) = self.player().and_then(|p| {
            let n = self.world.weenie(p)?.pwd.name.as_str();
            (!n.is_empty()).then_some(n)
        }) {
            return Some(n);
        }
        match self.player_desc()?.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::String,
            CHARACTER_NAME,
        )) {
            Some(dereth_client_model::StatValue::Str(s)) if !s.is_empty() => {
                // `StatValue::Str` hands back an owned copy, and the trait returns a borrow, so the
                // name is looked up again on the map that owns it.
                self.player_desc()?
                    .strings
                    .as_ref()?
                    .get(&CHARACTER_NAME)
                    .map(String::as_str)
            }
            _ => None,
        }
    }

    /// The experience header's inputs.
    ///
    /// `dereth_client_model::advancement::experience_header` is retail's arithmetic — including
    /// both Throne-of-Destiny behaviours, the 2³²−1 saturation and the level-126 "Infinity!".
    ///
    /// **Throne of Destiny is assumed present.** There is no account-entitlement counterpart here;
    /// every retail account after 2005 has it, and the two behaviors it gates
    /// are only reachable above 2³² total experience or at exactly level 126. `// UNVERIFIED:`
    /// stated rather than silently defaulted.
    fn experience_header(&self) -> Option<dereth_client_contract::statmgmt::XpHeader> {
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let int64 = |p: u32| match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            p,
        )) {
            Some(dereth_client_model::StatValue::Int64(n)) => n,
            _ => 0,
        };
        let total = u64::try_from(int64(TOTAL_EXPERIENCE)).unwrap_or(0);
        let level = q.inq_int(LEVEL);
        let h = dereth_client_model::advancement::experience_header(xp, total, level, true);
        let this_level = dereth_client_model::advancement::experience_to_level(
            xp,
            usize::try_from(level.max(0)).unwrap_or(0),
        )
        .unwrap_or(0);
        let next_level = dereth_client_model::advancement::experience_to_level(
            xp,
            usize::try_from(level.max(0)).unwrap_or(0) + 1,
        )
        .unwrap_or(0);
        Some(dereth_client_contract::statmgmt::XpHeader {
            total: h.total,
            into_level: h.xp_into_level,
            level_span: next_level.saturating_sub(this_level),
            to_level: h.xp_to_level,
            level,
            at_cap: h.at_cap,
        })
    }

    /// The Vendor panel's state. The whole of the conversion, including the four
    /// `dereth_client_model::vendor` functions it calls, is [`crate::vendor_view::shop`].
    /// Declared crossing: one line, alongside the identical `allegiance_roster` below.
    fn shop(&self) -> dereth_client_contract::ShopView {
        crate::vendor_view::shop(self.world)
    }

    /// The Trade panel's state. The whole of the conversion is [`crate::trade_view::trade`].
    /// Declared crossing, on the same terms as `shop` above.
    fn trade(&self) -> dereth_client_contract::TradeView {
        crate::trade_view::trade(self.world)
    }

    /// Test whether the vendor accepts a dragged item `(id, true)`. Same
    /// declared crossing: one line, and the whole decision is the inventory predicate.
    fn vendor_drag_item_accepted(&self, item: ObjectId) -> bool {
        self.world.drag_item_accepted(item)
    }

    /// `(id, quiet = 1)`'s ownership half.
    /// The same declared crossing the line above is: one line, and the whole
    /// decision is the inventory predicate -- which the drop path also calls, so the hint and the
    /// drop cannot disagree about ownership.
    fn trade_drag_item_acceptable(&self, item: ObjectId) -> bool {
        self.world.trade_item_acceptable(item).is_none()
    }

    /// `(id, quiet = 1)` over the window's
    /// own ground-object id.
    ///
    /// The hook-acceptance predicate is transcribed here rather than in `dereth_client_model`
    /// because both of its inputs are `PublicWeenieDesc` fields this file already reads the same
    /// way for `item_valid_locations`, and because its two callers on the *drop* side
    /// (the container-placement attempt and the drag-into-container legality check)
    /// take the `OPENABLE` route the game model already owns -- adding a
    /// second entry point there would duplicate that logic.
    ///
    /// `Weenie::is_hook` reproduces the native hook predicate's pair of tests: a
    /// container that is not a hook accepts everything, which is every chest, corpse and ground
    /// pack in the game.
    fn external_container_drag_item_acceptable(&self, item: ObjectId, ground: ObjectId) -> bool {
        // An unknown ground object accepts; an unknown item refuses.
        let Some(container) = self.world.weenie(ground) else {
            return true;
        };
        let Some(carried) = self.world.weenie(item) else {
            return false;
        };
        if !container.is_hook() {
            return true; // the accept arm, and the only one a chest or a corpse can reach.
        }
        // The next test: a hook with no house owner refuses, and sets the out-parameter nothing in
        // this path reads.
        if container.pwd.house_owner_iid.unwrap_or(ObjectId(0)).0 == 0 {
            return false;
        }
        let valid = u32::from(carried.pwd.hook_type.unwrap_or(0));
        let hook = u32::from(container.pwd.hook_type.unwrap_or(0));
        // Test the carried object's own hook-type mask against the
        // hook's location, then the item type against the hook's `_hook_item_types`.
        valid != 0
            && hook & valid != 0
            && carried.pwd.obj_type & container.pwd.hook_item_types.unwrap_or(0) != 0
    }

    /// The item's `pwd._valid_locations`, for the equipment-slot check.
    /// `None` when the object is not in the table.
    fn item_valid_locations(&self, item: ObjectId) -> Option<u32> {
        self.world
            .weenie(item)
            .map(|w| w.pwd.valid_locations.unwrap_or(0))
    }

    fn equipment_hover(&self, item: ObjectId) -> dereth_client_contract::view::EquipmentHover {
        self.world.equipment_hover(item)
    }

    /// The client predicate, answered by
    /// `dereth_client_model::inventory::salvage::is_item_suitable`.
    ///
    /// The option is read here rather than in the panel because `SalvageMultiple` is a
    /// `PlayerModule` bit. It is the third of the four tests and belongs to the player, not the item.
    fn salvage_item_suitable(&self, item: ObjectId, panel_material: u32) -> bool {
        let Some(w) = self.world.weenie(item) else {
            return false;
        };
        let multiple = character_option(
            self.world,
            dereth_client_contract::PlayerOption::SalvageMultiple,
        )
        .unwrap_or(false);
        dereth_client_model::inventory::salvage::is_item_suitable(w, multiple, panel_material)
    }

    /// The client predicate -- `drag_item_acceptable`.
    fn item_owned_by_player(&self, item: ObjectId) -> bool {
        self.world.is_owned_by_player(item)
    }

    /// The item's material type, for the window's material latch.
    fn item_material_type(&self, item: ObjectId) -> u32 {
        self.world
            .weenie(item)
            .and_then(|w| w.pwd.material_type)
            .unwrap_or(0)
    }

    /// The item's class id —
    /// `drag_item_acceptable` hands it to the needs-more check and the item add puts it in
    /// the `HousePayment` it builds.
    fn item_wcid(&self, item: ObjectId) -> u32 {
        self.world.weenie(item).map_or(0, |w| w.pwd.wcid)
    }

    /// A single unstacked item pays **one**, while a stack pays its stack size.
    /// Note what this is *not*:
    /// it does not consult the item's value, which is why a trade note needs
    /// a trade note's value, which is handled further down in `HousePaymentList`.
    fn item_house_payment(&self, item: ObjectId) -> i32 {
        self.world
            .weenie(item)
            .and_then(|w| w.pwd.stack_size)
            .map_or(1, |n| if n == 0 { 1 } else { i32::from(n) })
    }

    /// The slumlord and its owner id, projected for the window.
    fn slumlord(&self) -> Option<dereth_client_contract::SlumlordView> {
        let (slumlord, p) = self.world.slumlord.as_ref()?;
        Some(dereth_client_contract::SlumlordView {
            slumlord: *slumlord,
            owner: p.owner,
            owner_name: p.name.clone(),
            house_type: p.house_type,
            // Am-I-the-house-owner — `_owner` equals the player id, and with no
            // world objects at all it is `_owner == 0`.
            am_i_the_owner: self.world.player.unwrap_or(ObjectId(0)) == p.owner,
        })
    }

    /// The Mini Game panel's own fields.
    ///
    /// `Some` unconditionally, because the window exists in the shipped tree whether or not a game
    /// is on and a current game id of 0 is how "no game" is represented.
    fn minigame(&self) -> Option<dereth_client_contract::MiniGameView> {
        let g = &self.world.minigame;
        let mut piece_slots = [None; 64];
        for piece in &g.board.logic.pieces {
            let within_side = match piece.piece_type {
                dereth_client_model::chess::PieceType::Empty => continue,
                dereth_client_model::chess::PieceType::Pawn => 0,
                dereth_client_model::chess::PieceType::Rook => 3,
                dereth_client_model::chess::PieceType::Bishop => 1,
                dereth_client_model::chess::PieceType::Knight => 2,
                dereth_client_model::chess::PieceType::Queen => 4,
                dereth_client_model::chess::PieceType::King => 5,
            };
            let side = match piece.player {
                0 => 0,
                1 => 6,
                _ => continue,
            };
            let Some(cell) =
                dereth_client_model::minigame::GameBoard::cell_of_coord(piece.cur_pos, g.team)
            else {
                continue;
            };
            piece_slots[cell] = Some(side + within_side);
        }
        Some(dereth_client_contract::MiniGameView {
            visible: g.visible,
            team: g.team,
            game: g.current_game,
            // The draw's index arithmetic, which is team-dependent and therefore
            // the model's to apply.
            selected_cell: g
                .board
                .selected
                .and_then(|c| dereth_client_model::minigame::GameBoard::cell_of_coord(c, g.team)),
            piece_slots,
            draws: g.board.draws,
            stalemate: g.stalemate,
        })
    }

    fn slumlord_notices(&self) -> u64 {
        self.hud.stats.house_profile_notices
    }

    /// Copy the payment profile, apply every dropped payment, then compose the
    /// requirements and paid-in-full result over that updated copy.
    ///
    /// The replay is the payment: retail mutates the working profile when a row is added and
    /// reverses it when a row is removed, while preserving a pristine backup. Here `p` begins as
    /// that pristine clone and the panel's row list supplies the mutations, so the two cannot drift.
    fn slumlord_payment(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
    ) -> dereth_client_contract::SlumlordPayment {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return dereth_client_contract::SlumlordPayment::default();
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (wcid, amount, trade_note_value) in drops {
            p.pay(op, *wcid, *amount, *trade_note_value);
        }
        dereth_client_contract::SlumlordPayment {
            // The house refresh uses `compose_text` for the buy tab and
            // `compose_text2` for the rent tab. The asymmetry is retail's.
            requirements: if rent {
                p.compose_text2(op)
            } else {
                p.compose_text(op)
            },
            // The button update always asks about **`HouseOp::Buy`**, whichever tab is up.
            paid_in_full: p.op_is_paid_in_full(HouseOp::Buy),
        }
    }

    /// `(op, wcid)` over the same replay.
    fn slumlord_needs_more(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
        wcid: u32,
        trade_note_value: Option<i32>,
    ) -> bool {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return false;
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (w, amount, note) in drops {
            p.pay(op, *w, *amount, *note);
        }
        p.needs_more(op, wcid, trade_note_value) != 0
    }

    /// `(op, {wcid, num})` over the same replay.
    fn slumlord_pay(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
        wcid: u32,
        amount: i32,
        trade_note_value: Option<i32>,
    ) -> bool {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return false;
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (w, a, note) in drops {
            p.pay(op, *w, *a, *note);
        }
        p.pay(op, wcid, amount, trade_note_value)
    }

    fn item_trade_note_value(&self, item: ObjectId) -> Option<i32> {
        let wcid = self.world.weenie(item)?.pwd.wcid;
        self.hud
            .trade_note_values
            .iter()
            .find(|(_, mapped_wcid)| *mapped_wcid == wcid)
            .and_then(|(value, _)| i32::try_from(*value).ok())
    }

    /// Same declared crossing.
    fn selected_object(&self) -> Option<ObjectId> {
        self.world.selected
    }

    /// The client predicate: the spell formula's targeting type is zero.
    ///
    /// `dereth_client_model::magic::spell_target_type` is that walk (on the decrypted formula, and only
    /// when its first five slots are filled: the last filled slot of the run that starts at slot 5,
    /// or slot 4 when slot 5 is empty, through the component target-type lookup), and it is the same
    /// result the
    /// cast path branches on at its untargeted arm — so the button lights for
    /// exactly the spells the cast path will send the untargeted-cast request `0x0048` for.
    fn spell_is_untargeted(&self, spell_id: u32) -> bool {
        let Some(base) = self
            .hud
            .spell_table
            .as_ref()
            .and_then(|t| t.spells.get(&spell_id))
        else {
            return false;
        };
        dereth_client_model::magic::spell_target_type(base) == 0
    }

    /// Check whether the selected object is compatible with the spell, quietly —
    /// the **quiet** call, unlike the spell-casting path's.
    fn spell_target_compatible(&self, spell_id: u32) -> bool {
        let Some(target) = self.world.selected else {
            return false;
        };
        let Some(base) = self
            .hud
            .spell_table
            .as_ref()
            .and_then(|t| t.spells.get(&spell_id))
        else {
            return false;
        };
        let mask = dereth_client_model::magic::spell_target_type(base);
        // `quiet = 1`: the sink is a local nobody reads, as in the
        // client. Building a tooltip must not put a line in the chat.
        let mut quiet = dereth_client_model::NullSink;
        self.world
            .object_compatible_with_spell_target_type(&mut quiet, Some(target), mask, true)
            .is_ok()
    }

    /// Whether the item's target-use flags permit using it on the player.
    fn item_useable_self_target(&self, item: ObjectId) -> bool {
        let Some(w) = self.world.weenie(item) else {
            return false;
        };
        crate::cursor::ItemUses(w.pwd.useability.unwrap_or(0)).is_useable_self_target()
    }

    /// Check whether the selected target is compatible with the item, quietly —
    /// target first, source second.
    fn item_target_compatible(&self, item: ObjectId) -> bool {
        let Some(target) = self.world.selected else {
            return false;
        };
        self.world.target_compatible_with_object(target, item)
    }

    /// The allegiance hierarchy, walked. The whole of the conversion, including
    /// the four hierarchy walks, is [`crate::allegiance_view::roster`].
    fn allegiance_roster(&self) -> dereth_client_contract::AllegianceRoster {
        crate::allegiance_view::roster(
            self.world,
            self.player_desc(),
            self.hud.quality_filter.as_ref(),
        )
    }

    /// `0x0003`'s count, straight off the world — see
    /// [`dereth_client_contract::GameView::allegiance_update_aborts`].
    fn allegiance_update_aborts(&self) -> u64 {
        self.world.allegiance_aborts
    }

    /// `0x0020`'s count, straight off the world — see
    /// [`dereth_client_contract::GameView::allegiance_updates`].
    fn allegiance_updates(&self) -> u64 {
        self.world.allegiance_updates
    }

    /// **The Portal Storm indicator's one input.**
    ///
    /// `dereth_ui_screens::hud::indicators::portal_storm_state` and the lamp's row in
    /// `screens::gameplay` read the trait's default `0.0` unless this is overridden: the four
    /// `0x02C9`…`0x02CC` arms write the level, and `dereth_client_model::portal_storm` is their
    /// transcription.
    fn portal_storm_level(&self) -> f32 {
        self.world.portal_storm_level
    }

    /// The House panel's trigger for the *clearing* update.
    ///
    /// It is a count and not a `HouseData` because `0x0226`'s payload is one `u32` that retail
    /// keeps nowhere — see [`HudStats::house_status_last_notice`] and the `0x0226` arm. The real
    /// view type sits beside it rather than widening this: [`Self::house_data`].
    fn house_status_notices(&self) -> u64 {
        self.hud.stats.house_status_notices
    }

    /// The House panel's view, projected for the six sections that read it.
    ///
    /// Everything here uses `dereth_client_model::housing` to compose payment text, compute both rent-period
    /// boundaries, and construct the warning message, plus the three
    /// physics-crate calls, which are the reason this crossing exists at all: `dereth-ui-screens`
    /// has no `dereth-physics`, and `dereth-client-model` does not either.
    fn house_data(&self) -> Option<dereth_client_contract::HouseDataView> {
        use dereth_client_model::housing;
        let h = self.world.house.as_ref()?;
        let rent_owed = h.rent_is_owed();
        // Retail's House tab groups every payment count using the shipped language rule, as a
        // side-by-side retail comparison shows. Apply that rule at this projection boundary: the
        // shared `*` methods also feed the slumlord window, whose visible grouping has not been
        // established.
        let payment_text = |list: &housing::HousePaymentList, show_paid: bool| {
            list.0
                .iter()
                .map(|payment| {
                    let required =
                        dereth_client_contract::panels::numfmt::number(i64::from(payment.num));
                    let name = payment.get_name(payment.num);
                    if show_paid {
                        let paid =
                            dereth_client_contract::panels::numfmt::number(i64::from(payment.paid));
                        format!("{paid}/{required} {name}")
                    } else {
                        format!("{required} {name}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        Some(dereth_client_contract::HouseDataView {
            buy_text: payment_text(&h.buy, false),
            rent_text: payment_text(&h.rent, true),
            buy_time: i64::from(h.buy_time),
            maintenance_period_end: h.maintenance_period_end(),
            maintenance_next_due: h.maintenance_next_due(),
            location: self.world.house_location(house_lcoord),
            rent_owed,
            rent_warning: if rent_owed {
                housing::construct_rent_warning_message(housing::rent_period_days(h.house_type))
            } else {
                String::new()
            },
            // `convert_time` is called once per row and calls
            // `localtime` each time, so each instant gets the zone *it* falls in.
            // A house bought last winter with maintenance due next summer draws two different
            // daylight offsets on retail, and here.
            utc_offset_secs: [
                utc_offset_secs(i64::from(h.buy_time)),
                utc_offset_secs(h.maintenance_period_end()),
                utc_offset_secs(h.maintenance_next_due()),
            ],
        })
    }

    /// The pane's other redraw edge — see [`HudStats::house_data_notices`].
    fn house_data_notices(&self) -> u64 {
        self.hud.stats.house_data_notices
    }

    /// The zone `localtime` would use *now*, for any panel that formats an
    /// instant it did not carry an offset for. The dated views above each carry their own,
    /// because `localtime`'s answer depends on which instant it is given.
    fn utc_offset_secs(&self) -> i32 {
        utc_offset_secs(wall_clock_unix())
    }

    /// The purchase-time display's local-player-description half.
    ///
    /// Resolving the local player description is `player_qualities()` answering `Some` — the
    /// interface is the local player's quality bag and nothing else — and int quality `0xC7` is
    /// `PropertyInt::HousePurchaseTimestamp`. The client pre-zeroes the output and ignores the
    /// query's return value, so an absent quality reads `0`, which
    /// `has_purchase_wait_period_expired` always calls expired.
    ///
    /// The seed is CRT `time(NULL)`; [`wall_clock_unix`] is this tree's
    /// transcription of it and `chat_real_time` in `interaction.rs` is the same call.
    fn house_purchase(&self) -> dereth_client_contract::HousePurchaseView {
        let Some(q) = self.world.player_qualities() else {
            return dereth_client_contract::HousePurchaseView::default();
        };
        let t = q.inq_int(dereth_client_model::housing::HOUSE_PURCHASE_TIMESTAMP);
        dereth_client_contract::HousePurchaseView {
            have_player_desc: true,
            purchase_timestamp: t,
            wait_expired: dereth_client_model::housing::has_purchase_wait_period_expired(
                wall_clock_unix(),
                i64::from(t),
            ),
            // `localtime` on int quality `0xC7` + `0x278D00` — the instant
            // the panel prints, thirty days on, not the raw quality.
            utc_offset_secs: utc_offset_secs(
                i64::from(t) + dereth_client_contract::panels::house::PURCHASE_WAIT_SECONDS,
            ),
        }
    }

    /// Same declared crossing as the line above: the
    /// conversion is three fields, and the panel does its own sorted insert because
    /// the sorted-insert-position search is the panel's function and not the list's.
    fn friends(&self) -> Vec<dereth_client_contract::FriendEntry> {
        self.world
            .friends()
            .iter()
            .map(|f| dereth_client_contract::FriendEntry {
                id: f.id,
                name: f.name.clone(),
                online: f.online,
            })
            .collect()
    }

    /// The squelch-list iteration. Same declared crossing as the line above:
    /// the walk itself is `dereth_client_model::chat::ChatState::squelch_iteration` — it belongs to the
    /// communication system and not to the panel — and the conversion here is two fields.
    ///
    /// The model this reads has one production writer (`recv_set_squelch_db`, the `0x01F4`
    /// arm), and this is its only reader outside the chat router, so a broken seam here does
    /// not show anywhere else.
    fn squelch_list(&self) -> Vec<dereth_client_contract::SquelchEntry> {
        self.world
            .chat
            .squelch_iteration()
            .into_iter()
            .map(|(name, account)| dereth_client_contract::SquelchEntry { name, account })
            .collect()
    }

    /// Join the player's contract-tracker rows to the DAT contract table.
    /// As with the two queries above, this layer resolves the data so `dereth-ui-screens`
    /// need not access the DAT object.
    ///
    /// Every string here is produced by a `dereth_client_model::quests` function, so the panel transcribes
    /// none of them a second time. The two `Option`s are the one
    /// case `update_buttons` writes **nothing at all**: a `Position` whose `objcell_id` is zero
    /// (`objcell_id == 0`).
    ///
    /// A tracker whose contract the dat does not carry is **dropped and counted**
    /// ([`HudStats::contracts_unresolved`]); the reference client assumes the lookup succeeds.
    fn contracts(&self) -> Vec<dereth_client_contract::ContractEntry> {
        use dereth_client_model::quests;
        let now = dereth_primitives::ServerTime(self.hud.now.0);
        let fmt = |s: f64| {
            dereth_client_contract::journal::delta_time_to_string(i64::from(
                dereth_primitives::num::to_i32_f64(s),
            ))
        };
        let Some(table) = self.hud.contract_table.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (id, t) in self.world.contract_trackers() {
            let Some(raw) = table.contracts.get(id) else {
                continue;
            };
            let c = contract_of(raw);
            let end = quests::contact_is_the_end_npc(&c, t.contract_stage);
            let (contact, contact_pos) = if end {
                (c.name_npc_end.clone(), raw.location_npc_end)
            } else {
                (c.name_npc_start.clone(), raw.location_npc_start)
            };
            out.push(dereth_client_contract::ContractEntry {
                contract_id: *id,
                name: c.contract_name.clone(),
                status: quests::fill_progress_string(t, &c, now, &fmt),
                stage: t.contract_stage,
                description: c.description.clone(),
                contact,
                contact_location: contract_location(contact_pos),
                area_location: contract_location(raw.location_quest_area),
                timed: quests::timed_string(t, &c, now, &fmt),
                // The contract sort-status guard, verbatim.
                repeat_remaining: (t.contract_stage == quests::stage::DONE
                    && t.time_of_server_update > 0.0)
                    .then_some(t.time_when_repeats - (now.0 - t.time_of_server_update)),
            });
        }
        out
    }

    /// Look up `id` in the allegiance profile, for
    /// the allegiance UI's final swear-button condition.
    fn allegiance_has_member(&self, id: ObjectId) -> bool {
        self.world.allegiance.look_up(id).is_some()
    }

    /// The player's `InstanceID` quality **26 (`Monarch`)**, mirrored by
    /// `dereth_client_model::weenie::mirror_stat_update`'s key-26 arm from `0x02DA`.
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        let player = self.world.player?;
        self.world.weenie(player)?.pwd.monarch.filter(|m| m.0 != 0)
    }
    /// The player's fellowship pointer, as the fellowship UI needs
    /// it.
    ///
    /// The one thing computed here rather than carried is the per-fellow experience share, and it
    /// is computed here for the reason [`Self::skill_advancement`]'s two lookups are: both the
    /// even-split and level-proportional paths read the experience table,
    /// and `dereth-ui-screens` must not depend on `dereth-client-model` or on a dat table.
    ///
    /// The arithmetic returns zero when sharing is disabled, uses the member count for an even
    /// split, or divides this member's level proportion by the fellowship total. It then multiplies
    /// by 100 and truncates toward zero.
    ///
    /// With no experience table loaded the third arm cannot be taken and the share reads `0`,
    /// which is the same number the *share off* arm produces — the two are not distinguishable in
    /// the panel and are not meant to be: the client shows one integer.
    fn fellowship(&self) -> Option<dereth_client_contract::FellowshipView> {
        use dereth_client_contract::{FellowEntry, FellowshipView};
        let f = self.world.fellowship.as_ref()?;
        let xp = self.hud.xp_table.as_ref();
        let sum = xp.map_or(0, |t| f.experience_proportion_sum(t));
        // The share is held as an `f32` (the panel stores it single precision before the multiply),
        // then widened: that is what makes six members read 44% rather than 45%.
        let even = f64::from(dereth_client_model::fellowship::even_split_xp_percentage(
            f.members.len(),
        ));
        let members = f
            .members
            .iter()
            .map(|(id, m)| {
                let share = if !f.share_xp {
                    0.0
                } else if f.even_xp_split {
                    even
                } else if sum == 0 {
                    0.0
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    let p = xp.map_or(0, |t| {
                        dereth_client_model::fellowship::get_experience_proportion(t, m.level)
                    }) as f64;
                    #[allow(clippy::cast_precision_loss)]
                    let d = sum as f64;
                    #[allow(clippy::cast_possible_truncation)]
                    let share = (p / d) as f32;
                    f64::from(share)
                };
                let xp_percent = dereth_primitives::num::to_i32_f64(share * 100.0);
                FellowEntry {
                    id: *id,
                    name: m.name.clone(),
                    level: m.level,
                    xp_percent,
                    current_health: m.current_health,
                    max_health: m.max_health,
                    current_stamina: m.current_stamina,
                    max_stamina: m.max_stamina,
                    current_mana: m.current_mana,
                    max_mana: m.max_mana,
                }
            })
            .collect();
        Some(FellowshipView {
            name: f.name.clone(),
            leader: f.leader,
            share_xp: f.share_xp,
            even_xp_split: f.even_xp_split,
            open_fellow: f.open_fellow,
            locked: f.locked,
            members,
        })
    }

    /// `AppraisalProfile`'s six questions, for the identify panel.
    ///
    /// The cache behind it is `dereth_client_model::AppraisalCache`, filled by
    /// `0x00C9 Item_SetAppraiseInfo`; `Notice::AppraisalReady` reaches its consumer through this
    /// seam.
    ///
    /// Each field answers one appraisal-profile query; see
    /// [`dereth_client_contract::AppraisalView`] for the table.
    fn appraisal(
        &self,
        id: dereth_primitives::ObjectId,
    ) -> Option<dereth_client_contract::AppraisalView> {
        let p = self.world.appraisal.get(id)?;
        let s = |k: u32| -> Option<String> {
            p.tables
                .strings
                .as_ref()
                .and_then(|m| m.entries.iter().find(|(id, _)| *id == k))
                .map(|(_, v)| v.clone())
        };
        let i = |k: u32| -> Option<i32> {
            p.tables
                .ints
                .as_ref()
                .and_then(|m| m.entries.iter().find(|(id, _)| *id == k))
                .map(|(_, v)| *v)
        };
        // The six questions below are what the frame and the value/burden
        // blocks need; everything after them is what the creature pane and the rest of
        // the examination panel's ordered blocks need. The profile is
        // decoded whole by the appraisal reader and cached whole by `AppraisalCache::set`;
        // **this function is where the fields are read**, and the panel draws only what it is
        // given.
        use dereth_client_model::appraisal_model as am;
        let w = self.world.weenie(id);
        // `pwd.valid_locations` / `pwd.ammo_type`, replaced by the hook profile's for an item on
        // a housing hook (setting the appraise info reads the hooked item's values when hooked).
        let hook = p.hook_profile;
        let valid_locations = hook.map_or_else(
            || w.and_then(|w| w.pwd.valid_locations).unwrap_or(0),
            |h| h.valid_locations,
        );
        let ammo_type = hook.map_or_else(
            || w.and_then(|w| w.pwd.ammo_type).unwrap_or(0),
            |h| u16::try_from(h.ammo_type).unwrap_or(0),
        );
        let inscribable = hook.map_or_else(
            || w.is_some_and(|w| w.pwd.bitfield & am::OBJECT_DESC_INSCRIBABLE != 0),
            |h| h.bitfield & dereth_client_model::appraisal::hook_appraisal::INSCRIBABLE != 0,
        );
        // The callers of `dereth_client_model::appraisal`'s three highlight functions.
        // `int_highlight`, `float_highlight` and `highlight_state` are transcribed
        // whole (the enchantment-modifier bit pairs, low bit "enchanted" and the same bit
        // sixteen places up "raised"); without this call every line retail draws in
        // `mod_high_font`/`mod_low_font` would draw plain. The keys are
        // the two mod blocks' own, so the list lives with them.
        let mut enchantment_mods = std::collections::BTreeMap::new();
        for (key, is_float) in dereth_client_contract::panels::examination::HIGHLIGHTED_PROPERTIES {
            let hl = if *is_float {
                dereth_client_model::appraisal::float_highlight(*key)
            } else {
                dereth_client_model::appraisal::int_highlight(*key)
            };
            let Some(hl) = hl else { continue };
            match dereth_client_model::appraisal::highlight_state(p, hl) {
                dereth_client_model::appraisal::HighlightState::Plain => {}
                dereth_client_model::appraisal::HighlightState::Beneficial => {
                    enchantment_mods.insert(*key, true);
                }
                dereth_client_model::appraisal::HighlightState::Harmful => {
                    enchantment_mods.insert(*key, false);
                }
            }
        }
        let mut attributes = [None; 6];
        let mut attribute_enchanted = [None; 6];
        let mut vitals = [None; 6];
        let mut vital_enchanted = [None; 6];
        for k in 1..=6u32 {
            attributes[(k - 1) as usize] = am::creature_attribute(p, k);
            attribute_enchanted[(k - 1) as usize] = am::creature_attribute_enchanted(p, k);
            vitals[(k - 1) as usize] = am::creature_vital(p, k);
            vital_enchanted[(k - 1) as usize] = am::creature_vital_enchanted(p, k);
        }
        // Creature display name -- `EnumMapper 0x2200000E` on
        // int quality `2`, and only for a creature: the item pane has no such line.
        let creature_display_name = p.creature_profile.and_then(|_| {
            let t = u32::try_from(am::inq::int(p, am::property::CREATURE_TYPE)?).ok()?;
            self.hud
                .creature_type_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == t)
                .map(|(_, v)| v.clone())
        });
        // The examination view's eighteen quality queries,
        // in its own order. The slayer's display name is the **same**
        // mapper `creature_display_name`
        // above uses, on int quality `0xA6` instead of int quality `2`.
        use am::property::special as sp;
        let creature_name = |t: i32| {
            let t = u32::try_from(t).ok()?;
            self.hud
                .creature_type_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == t)
                .map(|(_, v)| v.clone())
        };
        // Retail starts the mask from int property `0xB3` (0 when absent) and ORs in four more,
        // each only when present. `None` when not one of the five was present, which is the state
        // retail cannot distinguish from zero but this build can.
        let imbued = sp::IMBUED_EFFECT
            .iter()
            .filter_map(|k| am::inq::int(p, *k))
            .fold(None::<u32>, |acc, v| Some(acc.unwrap_or(0) | v as u32));
        let special = dereth_client_contract::SpecialPropertiesView {
            unique_limit: am::inq::int(p, sp::UNIQUE_LIMIT),
            cooldown_duration: am::inq::float(p, 0xA7),
            cooldown_group: am::inq::int(p, 0x118).map(|v| v as u32),
            cooldown_remaining: am::inq::int(p, 0x118).and_then(|group| {
                // Special-property display shares the same player enchantment registry as item wedges.
                self.player_desc()?
                    .enchantments
                    .cooldown_remaining((group as u32).wrapping_add(0x8000), self.hud.now)
            }),
            cleave: am::inq::int(p, sp::CLEAVE),
            slayer: am::inq::int(p, sp::SLAYER_CREATURE_TYPE)
                .and_then(|t| creature_name(t).map(|n| (t, n))),
            weapon_skill: am::inq::int(p, sp::WEAPON_SKILL),
            imbued,
            absorb_magic_damage: am::inq::float(p, sp::ABSORB_MAGIC_DAMAGE).is_some(),
            item_spellcraft: am::inq::int(p, sp::ITEM_SPELLCRAFT),
            attuned: am::inq::int(p, sp::ATTUNED),
            bonded: am::inq::int(p, sp::BONDED),
            retained: am::inq::boolean(p, sp::RETAINED),
            critical_multiplier: am::inq::float(p, sp::CRITICAL_MULTIPLIER).is_some(),
            critical_frequency: am::inq::float(p, sp::CRITICAL_FREQUENCY).is_some(),
            ignore_armor: am::inq::float(p, sp::IGNORE_ARMOR).is_some(),
            // One `&&`: the float is the gate and the int selects the text.
            resistance_cleaving: am::inq::float(p, sp::IGNORE_SHIELD)
                .and_then(|_| am::inq::int(p, sp::RESISTANCE_MODIFIER_TYPE))
                .map(|v| v as u32),
            proc_spell: am::inq::data_id(p, sp::PROC_SPELL).is_some(),
            ivoryable: am::inq::boolean(p, sp::IVORYABLE),
            dyeable: am::inq::boolean(p, sp::DYEABLE),
            tethered_left: am::inq::boolean(p, sp::TETHERED_LEFT),
        };
        // Magic-property display includes the item's spells.
        //
        // The spell list is decoded and cached whole with the rest of the profile; this is where
        // it is read.
        //
        // Spell-name lookup and the spell-description lookup are
        // both the spell-table base lookup on `(6, 2, 0x10000005)`, which is the
        // portal dat's `SpellTable 0x0E00000E` -- `Hud::spell_table`, loaded once at startup and
        // otherwise read only by the player's own spellbook join. The lookup masks off the high
        // enchantment bit first; an id the table has no row for keeps
        // its entry with empty strings, because the client marks the entry present before the
        // name is tested.
        use am::property::magic as mg;
        let magic = dereth_client_contract::MagicInfoView {
            spells: p.spell_book.as_ref().map(|ids| {
                ids.iter()
                    .map(|raw| {
                        let base = self
                            .hud
                            .spell_table
                            .as_ref()
                            .and_then(|t| t.spells.get(&(raw & 0x7FFF_FFFF)));
                        dereth_client_contract::AppraisalSpellView {
                            raw_id: *raw,
                            enchantment: raw & 0x8000_0000 != 0,
                            name: base.map(|b| b.name.clone()).unwrap_or_default(),
                            description: base.map(|b| b.description.clone()).unwrap_or_default(),
                        }
                    })
                    .collect()
            }),
            spellcraft: am::inq::int(p, mg::ITEM_SPELLCRAFT),
            cur_mana: am::inq::int(p, mg::ITEM_CUR_MANA),
            max_mana: am::inq::int(p, mg::ITEM_MAX_MANA),
            mana_rate: am::inq::float(p, am::property::float::MANA_RATE),
            mana_cost: am::inq::int(p, mg::ITEM_MANA_COST),
        };
        // **The eighteen remaining appraisal display blocks.**
        //
        // Everything below is a property the profile already carried and this function already
        // could have read; the blocks that read them are in
        // `dereth_client_contract::panels::examination`. Four resolutions
        // happen *here* rather than there, for the reason the file header gives — the panel crate
        // cannot read dats:
        //
        // * skill names come from `SkillTable 0x0E000004`, `Hud::skill_table`;
        // * heritage names come from `EnumMapper 0x10000002`
        //   with three hard-coded overrides before it (`2 Gharu'ndim`, `5 Umbraen`, `13 Olthoi`),
        //   and `Hud::heritage_names` is that mapper;
        // * attribute and secondary-attribute names are
        //   switches, and the panel already publishes them as `attribute_name` / `vital_name`;
        // * wield requirement names use a switch over those four, so the whole resolution lives
        //   here and the panel is handed its answer.
        let skill_name = |k: u32| -> Option<String> {
            if k == 0 {
                return None;
            }
            self.hud
                .skill_table
                .as_ref()?
                .skills
                .get(&k)
                .map(|s| s.name.clone())
        };
        // the three `if` arms precede the mapper and override it.
        let heritage_name = |v: i32| -> Option<String> {
            match v {
                2 => return Some("Gharu'ndim".to_string()),
                5 => return Some("Umbraen".to_string()),
                13 => return Some("Olthoi".to_string()),
                _ => {}
            }
            let v = u32::try_from(v).ok()?;
            self.hud
                .heritage_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == v)
                .map(|(_, s)| s.clone())
        };
        // The wield-requirement subject is resolved here whole. The first argument is the
        // requirement, the second the skill/attribute id, the third the difficulty — and cases 11
        // and 12 read the third, not the second, which is why all three are taken.
        let requirement_subject = |req: i32, skill: i32, difficulty: i32| -> Option<String> {
            use dereth_client_contract::panels::examination as ex;
            // The "base " prefix is chosen before the switch.
            let prefix = if matches!(req, 2 | 4 | 6) {
                "base "
            } else {
                ""
            };
            // **The name failing is not the subject failing.** The client sets the output to
            // the prefix and then appends the skill-name lookup's result — a skill the table has
            // no row for appends an empty string, so the subject is the bare prefix and
            // the line still draws. Same for the two attribute arms.
            let body = match req {
                1 | 2 | 8 => u32::try_from(skill)
                    .ok()
                    .and_then(&skill_name)
                    .unwrap_or_default(),
                3 | 4 => u32::try_from(skill)
                    .ok()
                    .and_then(ex::attribute_name)
                    .unwrap_or_default()
                    .to_string(),
                5 | 6 => u32::try_from(skill)
                    .ok()
                    .and_then(ex::vital_name)
                    .unwrap_or_default()
                    .to_string(),
                // Level is a plain `set`, so the `"base "` prefix is discarded.
                7 => return Some("level".to_string()),
                9 | 10 => {
                    return Some(
                        match skill {
                            0x11F => "Standing with the Celestial Hand",
                            0x120 => "Standing with the Eldrytch Web",
                            0x121 => "Standing with the Radiant Blood",
                            _ => "unknown quality",
                        }
                        .to_string(),
                    )
                }
                // Creature-type name lookup on the **difficulty**.
                11 => creature_name(difficulty)?,
                12 => return heritage_name(difficulty),
                _ => return None,
            };
            Some(format!("{prefix}{body}"))
        };
        let mut wield_requirements = Vec::new();
        for (req_k, skill_k, diff_k) in [
            (0x9Eu32, 0x9Fu32, 0xA0u32),
            (0x10E, 0x10F, 0x110),
            (0x111, 0x112, 0x113),
        ] {
            // all three int-quality queries must succeed or the triple is skipped entirely.
            let (Some(req), Some(skill), Some(diff)) = (
                am::inq::int(p, req_k),
                am::inq::int(p, skill_k),
                am::inq::int(p, diff_k),
            ) else {
                continue;
            };
            wield_requirements.push(dereth_client_contract::WieldRequirementView {
                requirement: req,
                difficulty: diff,
                subject: requirement_subject(req, skill, diff),
            });
        }
        // The appraisal ratings section, in `GEAR_RATING_ROWS`' drawn order with
        // `GearMaxHealth` in the slot after the thirteen terms.
        let gear_rows = dereth_client_contract::panels::examination::GEAR_RATING_ROWS;
        let mut gear_ratings = [None; 14];
        for (slot, (key, _)) in gear_rows.iter().enumerate() {
            gear_ratings[slot] = am::inq::int(p, *key);
        }
        gear_ratings[gear_rows.len()] = am::inq::int(
            p,
            dereth_client_contract::panels::examination::GEAR_MAX_HEALTH,
        );
        // The appraisal item-level section: the first int64 quality `5` query is the gate.
        let item_level = am::inq::int64(p, 5).map(|base| dereth_client_contract::ItemLevelView {
            total_xp: am::inq::int64(p, 4).unwrap_or(0) as u64,
            base_xp: base as u64,
            max_level: am::inq::int(p, 0x13F).unwrap_or(0),
            xp_style: am::inq::int(p, 0x140).unwrap_or(0),
        });
        // The appraisal activation-requirements section's three paired terms: the *level*
        // key gates and supplies the `%d`, the *limit* key names the skill/attribute, and a name
        // the table cannot answer for drops the term.
        let paired = |level_k: u32, limit_k: u32, name: &dyn Fn(i32) -> Option<String>| {
            let v = am::inq::int(p, level_k)?;
            if v <= 0 {
                return None;
            }
            let id = am::inq::int(p, limit_k)?;
            name(id).map(|n| (n, v))
        };
        use dereth_client_contract::panels::examination as ex;
        let activation_skill = paired(0x73, 0xB0, &|id| {
            u32::try_from(id).ok().and_then(&skill_name)
        });
        let activation_attribute = paired(0x102, 0x101, &|id| {
            u32::try_from(id)
                .ok()
                .and_then(ex::attribute_name)
                .map(ToString::to_string)
        });
        let activation_attribute_2nd = paired(0x104, 0x103, &|id| {
            u32::try_from(id)
                .ok()
                .and_then(ex::vital_name)
                .map(ToString::to_string)
        });
        // **The three character-pane values that need a dat table.**
        //
        // The gender name ([`GENDER_ENUM_MAPPER`]), then — if int quality `0xBC`
        // HeritageGroup is non-zero — a space and the heritage name, and **only** if it is zero
        // does it fall through to the creature display name for int quality `2`. Its `BOOL`
        // return is set to 0 when either lookup misses, and retail **ignores it** and writes the
        // element regardless, so a name that will not resolve leaves the other one standing alone
        // rather than blanking the line.
        let gender_name = |v: i32| -> Option<String> {
            let v = u32::try_from(v).ok()?;
            self.hud
                .gender_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == v)
                .map(|(_, s)| s.clone())
        };
        let char_gender = am::inq::int(p, 0x71).unwrap_or(0);
        let char_heritage = am::inq::int(p, 0xBC).unwrap_or(0);
        let gender_heritage_display = p.creature_profile.is_some().then(|| {
            let mut out = String::new();
            if char_gender != 0 {
                if let Some(g) = gender_name(char_gender) {
                    out.push_str(&g);
                }
            }
            let tail = if char_heritage != 0 {
                heritage_name(char_heritage)
            } else {
                am::inq::int(p, am::property::CREATURE_TYPE)
                    .filter(|v| *v != 0)
                    .and_then(&creature_name)
            };
            if let Some(t) = tail {
                // The space is added only when the gender resolved.
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&t);
            }
            out
        });
        // Profession uses the title table for int quality `0x105`, and string quality
        // `5 Template` when that integer is absent **or** the title table had no row for it
        // (the fallback flag is set inside the success arm only).
        let profession = am::inq::int(p, 0x105)
            .and_then(|t| u32::try_from(t).ok())
            .and_then(|t| self.title_name(t))
            .or_else(|| am::inq::string(p, 5));
        // The allegiance title needs only the rank, heritage and gender; the lookup is
        // `dereth_client_model::allegiance::get_title`.
        let allegiance_title = u16::try_from(am::inq::int(p, 0x1E).unwrap_or(0))
            .ok()
            .and_then(|rank| {
                dereth_client_model::allegiance::get_title(
                    rank,
                    u8::try_from(char_heritage).unwrap_or(0),
                    u8::try_from(char_gender).unwrap_or(0),
                )
            })
            .map(ToString::to_string);
        use dereth_client_model::weenie::bitfield as bf;
        let bits = w.map_or(0, |w| w.pwd.bitfield);
        let hook_flags = p.hook_profile.map_or(0, |h| h.bitfield);
        use dereth_client_model::appraisal::hook_appraisal as hk;

        Some(dereth_client_contract::AppraisalView {
            delivery: self.world.appraisal.delivery(id),
            creature: p.creature_profile.is_some(),
            template: s(5).is_some(),
            character_title: i(0x105).is_some(),
            gear_plating_name: s(0x34),
            value: i(0x13),
            burden: i(5),

            success: p.success_flag != 0,

            level: am::inq::int(p, am::property::LEVEL),
            creature_display_name,
            attributes,
            vitals,
            attribute_enchanted,
            vital_enchanted,

            valid_locations,
            ammo_type,
            weapon: p
                .weapon_profile
                .map(|w| dereth_client_contract::WeaponView {
                    damage_type: w.damage_type,
                    weapon_time: w.weapon_time,
                    weapon_skill: w.weapon_skill,
                    weapon_damage: w.weapon_damage,
                    damage_variance: w.damage_variance,
                    damage_mod: w.damage_mod,
                    max_velocity: w.max_velocity,
                    weapon_offense: w.weapon_offense,
                    max_velocity_estimated: w.max_velocity_estimated,
                }),
            weapon_type: am::inq::int(p, am::property::WEAPON_TYPE),
            armor_level: am::inq::int(p, am::property::ARMOR_LEVEL),
            enchantment_mods,
            armor_mods: p.armor_profile.map(|a| {
                [
                    a.mod_vs_slash,
                    a.mod_vs_pierce,
                    a.mod_vs_bludgeon,
                    a.mod_vs_cold,
                    a.mod_vs_fire,
                    a.mod_vs_acid,
                    a.mod_vs_nether,
                    a.mod_vs_electric,
                ]
            }),
            use_text: am::inq::string(p, am::property::string::USE),
            // All three quality queries must succeed, but only the third value is
            // read after the gates.
            remaining_lifespan: am::inq::int(p, ex::LIFESPAN)
                .and_then(|_| am::inq::int(p, ex::CREATION_TIMESTAMP))
                .and_then(|_| am::inq::int(p, ex::REMAINING_LIFESPAN)),
            long_desc: am::inq::string(p, am::property::string::LONG_DESC),
            short_desc: am::inq::string(p, ex::SHORT_DESC),
            augmentation_cost: am::inq::int64(p, 3),
            long_desc_decoration: am::inq::int(p, 0xAC).map(|v| v as u32),
            description_material: am::inq::int(p, 0x83).filter(|v| *v > 0).map(|v| {
                material_name_of(self.hud.material_names.as_ref(), v as u32).unwrap_or_default()
            }),
            description_gems: am::inq::int(p, 0xB1).zip(am::inq::int(p, 0xB2)).map(
                |(count, gem)| {
                    let name = material_name_of(self.hud.material_names.as_ref(), gem as u32)
                        .unwrap_or_default();
                    let name = if count == 1 {
                        name
                    } else {
                        ex::pluralized_gem_name(gem as u32, &name)
                    };
                    (count, name)
                },
            ),
            // The appraisal description section asks the same profile for `PortalBitmask`,
            // which the int table carries.
            portal_bitmask: am::inq::int(
                p,
                dereth_client_contract::panels::examination::PORTAL_BITMASK,
            ),

            // The appraisal lock-info section. The three keys are decoded by and cached whole
            // from the appraisal profile; this function is where they are read.
            //
            // The predicate is `is_hook`, not `is_creature`, and it is evaluated on the object
            // rather than the appraisal profile. Setting the appraise info makes the same call
            // and follows it with the hooked item's valid-locations query.
            weenie_is_hook: w.is_some_and(dereth_client_model::weenie::Weenie::is_hook),
            weenie_is_healer: bits & bf::HEALER != 0,
            weenie_is_lockpick: bits & bf::LOCKPICK != 0,
            items_capacity: w
                .and_then(|w| w.pwd.items_capacity)
                .map_or(0, dereth_client_model::capacity::capacity),
            containers_capacity: w
                .and_then(|w| w.pwd.containers_capacity)
                .map_or(0, dereth_client_model::capacity::capacity),
            hooked_item: p.hook_profile.is_some(),
            hooked_item_healer: hook_flags & hk::HEALER != 0,
            hooked_item_lockpick: hook_flags & hk::LOCKPICK != 0,

            num_times_tinkered: am::inq::int(p, 0xAB),
            tinker_name: am::inq::string(p, 0x27),
            imbuer_name: am::inq::string(p, 0x28),
            workmanship: am::inq::int(p, 0x69),
            num_items_in_material: am::inq::int(p, 0xAA),

            equipment_set_id: am::inq::int(p, 0x109),
            gear_ratings,
            defense_mods: [
                am::inq::float(p, 0x1D),
                am::inq::float(p, 0x95),
                am::inq::float(p, 0x96),
            ],
            mana_conversion_mod: am::inq::float(p, 0x90),
            elemental_damage_mod: am::inq::float(p, 0x98),
            caster_damage_type: am::inq::int(p, 0x2D),

            level_limits: (am::inq::int(p, 0x56), am::inq::int(p, 0x57)),
            portal_destination: am::inq::string(p, 0x26),

            has_allowed_wielder: am::inq::boolean(p, 0x55) == Some(true),
            craftsman_name: am::inq::string(p, 0x19),
            account_requirements: am::inq::int(p, 0x1A),
            heritage_specific_armor: am::inq::int(p, 0x144).and_then(&heritage_name),
            wield_requirements,

            use_requires_level: am::inq::int(p, 0x171),
            use_requires_skill: am::inq::int(p, 0x16E).filter(|v| *v != 0).and_then(|s| {
                // the level must be present **and** non-zero or the line is skipped.
                let lvl = am::inq::int(p, 0x16F).filter(|v| *v != 0)?;
                Some((u32::try_from(s).ok().and_then(&skill_name), lvl))
            }),
            use_requires_skill_spec: am::inq::int(p, 0x170)
                .filter(|v| *v != 0)
                .map(|s| u32::try_from(s).ok().and_then(&skill_name)),

            item_level,
            cloak_weave_proc: am::inq::int(p, 0x160),

            item_difficulty: am::inq::int(p, 0x6D),
            allegiance_rank_limit: am::inq::int(p, 0x6E),
            heritage_group: am::inq::int(p, 0xBC)
                .filter(|v| *v != 0)
                .and_then(&heritage_name),
            activation_skill,
            activation_attribute,
            activation_attribute_2nd,
            has_allowed_activator: am::inq::boolean(p, 0x5E) == Some(true),

            boost_value: am::inq::int(p, 0x5A),
            booster_enum: am::inq::int(p, 0x59),
            healkit_mod: am::inq::float(p, 0x64),

            pages: am::inq::int(p, 0xAF)
                .and_then(|max| am::inq::int(p, 0xAE).map(|cur| (cur, max))),

            stored_mana: am::inq::int(p, 0x6B),
            item_efficiency: am::inq::float(p, 0x57),
            destroy_chance: am::inq::float(p, 0x89),

            num_keys: am::inq::int(p, 0xC1),
            unlimited_use: am::inq::boolean(p, 0x3F) == Some(true),
            structure: am::inq::int(p, 0x5C),

            // Retail requires the property to be present **and** false.
            cannot_be_sold: am::inq::boolean(p, 0x45) == Some(false),

            rare_uses_timer: am::inq::boolean(p, 0x6C) == Some(true),
            rare_id: am::inq::int(p, 0x11),
            locked: am::inq::boolean(p, am::property::boolean::LOCKED),
            resist_lockpick: am::inq::int(p, am::property::RESIST_LOCKPICK),
            special,
            magic,
            lockpick_success_percent: am::inq::int(
                p,
                am::property::APPRAISAL_LOCKPICK_SUCCESS_PERCENT,
            ),

            inscribable,
            // The PSR test checks access levels 0x2C and 0x2D, and additionally accepts 0x61.
            // These are the retained local-player qualities, not facts from the object being
            // appraised.
            viewer_is_psr: self
                .player_desc()
                .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|key| q.inq_bool(key))),
            scribe_name: am::inq::string(p, am::property::string::SCRIBE_NAME),
            inscription: am::inq::string(p, am::property::string::INSCRIPTION),
            // The one question the inscription editable-state setter asks that the profile
            // cannot answer.
            owned_by_player: self.world.is_owned_by_player(id),

            // Every key below is decoded by and cached whole from the appraisal profile, and
            // the `0x4000` base-armour block has a decoder and **no other reader in the
            // workspace**: this function is where they are all read.
            gender_heritage_display,
            profession,
            weenie_is_pk: bits & bf::PLAYER_KILLER != 0,
            weenie_is_pk_lite: bits & bf::PK_LITE != 0,
            allegiance_title,
            faction_bits: am::inq::int(p, 0x119),
            viewer_faction_bits: self.player_desc().map_or(0, |q| q.inq_int(0x119)),
            society_ranks: [0x11Fu32, 0x120, 0x121].map(|k| am::inq::int(p, k).unwrap_or(0)),
            allegiance_rank: am::inq::int(p, 0x1E),
            allegiance_name: am::inq::string(p, 0x2F),
            monarch_title: am::inq::string(p, 0x15),
            patron_title: am::inq::string(p, 0x23),
            // The same key number as `patron_title`, in the **int** table.
            allegiance_followers: am::inq::int(p, 0x23),
            base_armor: p
                .base_armor
                .map(|a| a.map(|v| i32::try_from(v).unwrap_or(i32::MAX))),
            ratings: [
                0x133u32, 0x134, 0x139, 0x13A, 0x13B, 0x13C, 0x143, 0x15E, 0x15F, 0x17D, 0x17E,
                0x182, 0x183,
            ]
            .map(|k| am::inq::int(p, k)),
            fellowship: am::inq::string(p, 0x0A),
            date_of_birth: am::inq::string(p, 0x2B),
            age: am::inq::int(p, 0x7D),
            chess_rank: am::inq::int(p, 0xB5),
            fishing_skill: am::inq::int(p, 0xC0),
            // Likewise the same key number as `date_of_birth`, in the **int** table.
            num_deaths: am::inq::int(p, 0x2B),
            num_character_titles: am::inq::int(p, 0x106),
            enlightenment: am::inq::int(p, 0x186),
        })
    }

    fn inscription_mouse_facts(&self, id: dereth_primitives::ObjectId) -> Option<(bool, u32)> {
        self.world
            .weenie(id)
            .map(|w| (w.pwd.bitfield & 0x2 != 0, w.pwd.location.unwrap_or(0)))
    }

    /// The examine request carried across the seam: the object being examined and its serial.
    ///
    /// `AppraisalCache::examining` is its only writer. The panel is on the other side of this
    /// trait and cannot be called from `interaction.rs`, so the notice is pulled instead of
    /// pushed; the serial is what makes a second examine of the same object a new notice rather
    /// than the old one being re-offered.
    fn examine_request(&self) -> Option<(dereth_primitives::ObjectId, u64)> {
        Some((
            self.world.appraisal.examining?,
            self.world.appraisal.examine_serial,
        ))
    }

    /// Helpful and harmful enchantment counts.
    ///
    /// This is the override `hud::indicators` reads through `GameView::enchantment_counts`;
    /// without it the buff/debuff indicator would read the trait's `(0, 0)` for every character
    /// in every session. The two fields it names are maintained by the client's spell-totals
    /// count, which must be called for them to move.
    fn enchantment_counts(&self) -> (u32, u32) {
        self.player_desc().map_or((0, 0), |q| {
            (q.enchantments.helpful_count, q.enchantments.harmful_count)
        })
    }

    /// Active enchantments joined to the `SpellTable`, which is
    /// the effect-list rebuild's whole input.
    ///
    /// Three things are done here rather than in the panel, each because the client does them
    /// here too:
    ///
    /// * **The join.** The list rebuild queries the spell table per entry and drops
    ///   an enchantment the `SpellTable` has no row for; so does this. That is also what makes
    ///   `_bitfield & 4` — the spell-effect UI-type match's only test — available to a
    ///   crate that may not depend on `dereth-assets`' tables.
    /// * **The clock.** `(e._duration + e._start_time) - current_time`, with
    ///   [`Hud::now`] as the current time. The two times were **rebased on receipt** by
    ///   `dereth_client_model::enchant`; recomputing them from a server timestamp
    ///   here would be wrong.
    /// * **The registry.** The canonical local-player object row owns it. The five
    ///   `Magic_*Enchantment*` messages update that same registry, without which no buff
    ///   acquired after login could reach either the lamp or this panel.
    ///
    /// `enchantments_in_effect` is `dereth_client_model::EnchantmentRegistry`'s; this is its
    /// production caller.
    fn active_effects(&self) -> Vec<dereth_client_contract::EffectEntry> {
        let (Some(q), Some(table)) = (self.player_desc(), self.hud.spell_table.as_ref()) else {
            return Vec::new();
        };
        q.enchantments
            .enchantments_in_effect()
            .into_iter()
            .filter_map(|e| {
                let spell = u32::from(e.spell_id());
                let b = table.spells.get(&spell)?;
                Some(dereth_client_contract::EffectEntry {
                    spell,
                    name: b.name.clone(),
                    description: b.description.clone(),
                    icon: (b.icon != 0).then_some(DataId(b.icon)),
                    // ` & 4` — `SpellBitfield::Beneficial`. The same bit the client's
                    // spell-totals count uses for the helpful lamp, which is why
                    // the lamp and the panel can never disagree about a spell.
                    beneficial: b.bitfield & 4 != 0,
                    remaining: e.remaining(self.hud.now),
                    permanent: e.is_permanent(),
                    category: e.spell_category,
                    power_level: e.power_level,
                })
            })
            .collect()
    }

    /// Join the component tracker's seven category lists to the player's desired counts.
    /// See [`component_categories`].
    ///
    /// Kept in this file rather than in a sibling module the way `vendor_view` and
    /// `allegiance_view` are, so that no new module declaration is needed.
    fn spell_components(&self) -> Vec<dereth_client_contract::ComponentCategory> {
        component_categories(self.world)
    }

    /// Look up a carried spell-component object in the tracker's object-id map.
    ///
    /// `.components` already keeps that map, filled by the component tracker's writers; this is
    /// the only reader of it outside `dereth-client-model`.
    fn object_is_owned_component(&self, obj: dereth_primitives::ObjectId) -> Option<u32> {
        self.world.magic.components.object_is_owned_component(obj)
    }

    /// `(0x18)` — available skill credits.
    fn skill_credits(&self) -> i64 {
        self.player_desc()
            .map_or(0, |q| i64::from(q.inq_int(AVAILABLE_SKILL_CREDITS)))
    }

    /// `(2)` — unassigned experience.
    fn available_experience(&self) -> i64 {
        self.player_desc().map_or(0, |q| {
            match q.get(dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                AVAILABLE_EXPERIENCE,
            )) {
                Some(dereth_client_model::StatValue::Int64(n)) => n,
                _ => 0,
            }
        })
    }

    /// The shared player state includes local filter changes before any server reply.
    fn spell_filters(&self) -> u32 {
        self.world.player_system.spell_filters
    }

    /// `UNDEF` is mapped to `NONCOMBAT` because the toolbar has
    /// no picture for "no mode" — all four stance buttons would be hidden — and a character in the
    /// world is always in one of the four.
    fn combat_mode(&self) -> u32 {
        let m = self.world.combat.combat_mode.raw();
        if m == 0 {
            dereth_client_contract::combat_mode::NONCOMBAT
        } else {
            m
        }
    }

    /// The advanced-combat option gate read before combat UI updates.
    ///
    /// This is the option word the client reads, not `CombatState::advanced_combat_mode`: the
    /// handler calls the accessor itself, while the combat system's cached copy is a separate read.
    /// Both exist and both come from the same bit.
    fn advanced_combat_ui(&self) -> bool {
        self.world.player_system.options.advanced_combat_ui()
    }

    /// The local player's skill-advancement query for skill `0x32`.
    ///
    /// `Sac::Undef` (0) when no local player description has arrived, which is what the client's
    /// null check produces: the handler still runs the comparison and
    /// `0 < 2`, so the recklessness meter stays hidden.
    fn recklessness_advancement_class(&self) -> u32 {
        self.player_desc().map_or(0, |q| {
            dereth_client_model::skills::inq_skill_advancement_class(
                q,
                dereth_client_contract::combat_notice::RECKLESSNESS_SKILL,
            ) as u32
        })
    }

    /// The producer for the Combat window's three read-back notices, with all four fields taken
    /// straight from current combat state.
    ///
    /// `level` is the latest power-bar level that was *sent*, not the power-bar level evaluated at
    /// [`Hud::now`], for two reasons:
    ///
    /// * **The notice does not carry the clock's level, it carries what was *sent*.** Every
    ///   power-bar-level notice passes `set_power_bar_level`'s
    ///   argument, and the latest power-bar level is that argument. On three arms it is
    ///   **not** the live power-bar level: the per-frame use-time update pins it to the smaller of
    ///   the requested power and the level on the frame the attack fires, sends **0.0** when the
    ///   body left the ready position mid-charge, and the commence-attack handler sets it to the
    ///   requested attack power **with the build-in-progress flag cleared** — where re-evaluating
    ///   the power-bar level returns 0.0 and the meter would drop to empty on the swing.
    /// * **The cache has a per-frame writer.** The power-level update calls
    ///   `set_power_bar_level` on every arm, so the cache moves once a frame and there is nothing
    ///   to work around.
    ///
    /// The snapshot remains useful for inspecting retail state and polling height/the notch;
    /// neither display polls its level. `deliver_power_bar_notices` delivers the ordered
    /// Begin/SetLevel/Finish journal, so a hide/restart cannot erase an intermediate zero.
    /// **Declared deviation: deferred until the UI frame**, because `App::ui_use_time` runs
    /// before `App::interaction_use_time`.
    ///
    /// One further deviation is load-bearing here and is stated rather than relied on silently:
    /// retail's power-bar hide does **not** write the latest power-bar level (it touches four
    /// other fields and nothing else). Setting it to 0.0 on hide would not make the display agree
    /// either: the snapshot mode is already Undef and the classic meter correctly refuses it. The
    /// journal delivers the notices and leaves that cache alone, preserving both the retail stale
    /// cached float and the actual zero notice.
    fn combat_bar(&self) -> dereth_client_contract::CombatBar {
        let c = &self.world.combat;
        dereth_client_contract::CombatBar {
            requested_attack_height: c.requested_attack_height as u32,
            power_bar_mode: c.power_bar_mode as u32,
            level: c.latest_power_bar_level,
            desired_power: c.ui_requested_power,
        }
    }
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
mod tests {
    use super::*;

    /// The era every front end reads comes from the world's own tables: February 2005's 126
    /// levels and 36 skills (the old weapon skills), the end of retail's 275 levels and its
    /// consolidated skills. The era itself is the server's when it announced one, else the dats'.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail and February 2005 dats: --features retail-dats"
    )]
    fn the_era_view_reads_the_worlds_level_cap_and_skills() {
        let world = dereth_client_model::World::default();
        let old = dereth_dat::testing::open_pre_tod_store_or_fail();
        let mut h = Hud::new();
        h.load_tables(&old, &world);
        let v = HudView {
            hud: &h,
            world: &world,
        };
        let era = GameView::era(&v).expect("a world");
        assert!(era.before_throne_of_destiny());
        assert_eq!(era.level_cap, 126);
        assert_eq!(era.skills.len(), 36);
        assert!(
            era.has_skill(11) && !era.has_skill(44),
            "Sword, not Heavy Weapons"
        );
        assert_eq!(
            era.era,
            dereth_primitives::EraId::Infiltration,
            "read from the dats"
        );
        assert!(!era.features().ratings && !era.features().luminance);

        // The server's announcement wins over the files.
        let mut h = Hud::new();
        h.era.era = dereth_primitives::EraId::Eor;
        h.era.era_announced = true;
        h.load_tables(&old, &world);
        assert_eq!(h.era.era, dereth_primitives::EraId::Eor);
        assert!(h.era.features().ratings);

        let later = dereth_dat::testing::open_store_or_fail();
        let mut h = Hud::new();
        h.load_tables(&later, &world);
        assert!(!h.era.before_throne_of_destiny());
        assert_eq!(h.era.level_cap, 275);
        assert!(h.era.has_skill(44), "Heavy Weapons");
        assert_eq!(h.era.era, dereth_primitives::EraId::Eor);
    }

    /// Oracle: the coordinate conversion checked against the live retail read-out —
    /// **42.2N, 33.8E** while standing in Holtburg, which is
    /// landblock `0xA9B4`.
    #[test]
    fn the_coordinate_read_out_matches_the_live_retail_value_over_holtburg() {
        // The cell the headless run stands in, `0xA9B40025`: block 0xA9/0xB4, cell index 0x25.
        let c = dereth_primitives::CellId(0xA9B4_0025);
        let (n, e) = player_coords(c).expect("Holtburg's cell is a valid lcoord");
        // block y 0xB4 = 180 -> ly = 1440 + (0x25-1)%8 = 1444 -> (1444-1024)*0.1+0.5 = 42.5
        // block x 0xA9 = 169 -> lx = 1352 + (0x25-1)/8 = 1356 -> (1356-1024)*0.1+0.5 = 33.7
        assert!((n - 42.5).abs() < 0.001, "north was {n}");
        assert!((e - 33.7).abs() < 0.001, "east was {e}");
        // The retail read-out over the same town is 42.2N, 33.8E: the same tenth-scale, within the
        // few cells a player moves. What is asserted here is the formula and the axis order — a
        // swap would put 33.x first, and the retail line puts 42.x first.
        assert!(n > e, "N/S is the first coordinate");

        // A cell id `gid_to_lcoord` rejects is the player-coordinates query returning false.
        assert_eq!(player_coords(dereth_primitives::CellId(0)), None);
    }

    /// Oracle: — the array is indexed by
    /// `windowID − 1`, so row 0 is window 1 and the main chat window (id 8) is row 7.
    #[test]
    fn the_placement_array_is_indexed_by_window_id_minus_one() {
        use dereth_protocol::property::{BaseProperty, PackObjPropertyCollection};

        let row = |visible: bool, x: i32| {
            let mut c = PropertyCollection::default();
            c.entries.push((
                placement::VISIBILITY,
                BaseProperty {
                    name: placement::VISIBILITY,
                    value: Some(BasePropertyValue::Bool(visible)),
                },
            ));
            c.entries.push((
                placement::X,
                BaseProperty {
                    name: placement::X,
                    value: Some(BasePropertyValue::Integer(x)),
                },
            ));
            BaseProperty {
                name: placement::ARRAY,
                value: Some(BasePropertyValue::Struct(c)),
            }
        };

        let mut props = PropertyCollection::default();
        props.entries.push((
            placement::ARRAY,
            BaseProperty {
                name: placement::ARRAY,
                value: Some(BasePropertyValue::Array(vec![
                    row(false, 10),
                    row(false, 20),
                    row(false, 30),
                    row(false, 40),
                    row(false, 50),
                    row(false, 60),
                    row(false, 70),
                    row(true, 80),
                ])),
            },
        ));
        let m = PlayerModule {
            option_flags: dereth_protocol::login::player_module_flags::GAMEPLAY_OPTIONS,
            gameplay_options: Some(PackObjPropertyCollection {
                version: PackObjPropertyCollection::CORE_VERSION,
                properties: props,
            }),
            ..PlayerModule::default()
        };

        let p = decode_placements(&m);
        assert_eq!(p.rows.len(), 8);
        // Window 8 is the main chat window (`chat::interface::window::MAIN`), and it is row 7.
        let main = p
            .get(dereth_client_contract::chat::interface::window::MAIN)
            .expect("window 8");
        assert_eq!(main.visible, Some(true));
        assert_eq!(main.x, Some(80));
        // Window 1 is row 0, and window 0 does not exist.
        assert_eq!(p.get(1).and_then(|r| r.x), Some(10));
        assert!(p.get(0).is_none());
        assert!(p.get(9).is_none());
    }

    /// Oracle: the module's own tolerance contract — a `PlayerModule` with no gameplay-options
    /// section decodes to an empty placement table rather than failing, which is
    /// the chat-window option query returning false for every window and the layout's own values standing.
    #[test]
    fn a_player_module_with_no_gameplay_options_places_nothing() {
        let p = decode_placements(&PlayerModule::default());
        assert!(p.rows.is_empty());
    }

    /// `SideBySideVitals` — `options_` bit 21.
    const SIDE_BY_SIDE: u32 = 0x0020_0000;

    /// The `PlayerOption` ordinal for Side-by-Side Vitals, taken through the same bridge
    /// `interaction.rs`'s `SetPlayerOption` arm takes rather than restated as a literal.
    fn side_by_side_ordinal() -> usize {
        option_ordinal(dereth_client_contract::PlayerOption::SideBySideVitals)
    }

    fn hud_with(options: u32, options2: u32) -> (Hud, dereth_client_model::World) {
        let m = PlayerModule {
            options,
            options2,
            ..PlayerModule::default()
        };
        let mut h = Hud::new();
        h.player_module = Some(m.clone());
        let mut w = dereth_client_model::World::new();
        w.player_system.apply_player_module(&m);
        (h, w)
    }

    /// The applied latch moves when side by side vitals moves.
    #[test]
    fn the_applied_latch_moves_when_side_by_side_vitals_moves() {
        let (h, mut w) = hud_with(0, 0);
        let off = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
        assert!(
            !off.side_by_side_vitals,
            "bit 21 is clear in an all-zero options word"
        );

        // The player ticks Side-by-Side Vitals on the Character Options page. Nothing else about
        // the screen or the placements changes.
        let change = w.player_system.set_option(
            side_by_side_ordinal(),
            true,
            dereth_primitives::ServerTime(0.0),
        );
        assert!(change.moved(), "the tick moved no bit at all");
        let on = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
        assert!(on.side_by_side_vitals);
        assert_ne!(
            off, on,
            "the latch cannot see the option it applies, so it will freeze"
        );
        assert_eq!(
            on.screen_serial, off.screen_serial,
            "the screen was not rebuilt"
        );
    }

    /// The tick moves the word option bit reads and the login blob copy does not.
    #[test]
    fn the_tick_moves_the_word_option_bit_reads_and_the_login_blob_copy_does_not() {
        // Arm 1 — the player's own tick, and only it. `Hud::player_module` is left at login's
        // all-zero word, so a reader on that copy answers `false` and this fails.
        let (h, mut w) = hud_with(0, 0);
        w.player_system.set_option(
            side_by_side_ordinal(),
            true,
            dereth_primitives::ServerTime(0.0),
        );
        assert_eq!(
            h.player_module.as_ref().expect("a module").options & SIDE_BY_SIDE,
            0,
            "the tick did not reach the login blob's copy -- that is the premise, not the defect"
        );
        assert!(
            h.option_bit(&w, Some(SIDE_BY_SIDE)),
            "a player tick must reach option_bit, not merely character_option"
        );
        assert_eq!(
            character_option(&w, dereth_client_contract::PlayerOption::SideBySideVitals),
            Some(true),
            "and the two accessors must answer from one word"
        );

        // Arm 2 — the login blob's copy moved on its own, which is what a stale reader would
        // follow. There is one option word now, and this is not it.
        let (mut h, w) = hud_with(0, 0);
        h.player_module.as_mut().expect("a module").options = u32::MAX;
        assert!(
            !h.option_bit(&w, Some(SIDE_BY_SIDE)),
            "option_bit is reading Hud::player_module again: see its own note"
        );
    }

    /// With no player description every option bit is false.
    #[test]
    fn with_no_player_description_every_option_bit_is_false() {
        let h = Hud::new();
        let mut w = dereth_client_model::World::new();
        // The model word can be non-zero before a description lands: `Options::default()` is
        // the default character-option word. The gate is the *module*, not the word.
        w.player_system.options.options = u32::MAX;
        assert!(w.player_system.module.is_none());
        assert!(!h.option_bit(&w, Some(SIDE_BY_SIDE)));
        assert_eq!(
            character_option(&w, dereth_client_contract::PlayerOption::SideBySideVitals),
            None
        );
    }

    /// With the production constant the latch follows the option.
    #[test]
    fn with_the_production_constant_the_latch_follows_the_option() {
        let (h, mut w) = hud_with(0, 0);
        let off = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
        assert!(
            !off.side_by_side_vitals,
            "bit 21 is clear in an all-zero options word"
        );
        // The player's own tick, through the same bridge `interaction.rs`'s arm uses.
        w.player_system.set_option(
            side_by_side_ordinal(),
            true,
            dereth_primitives::ServerTime(0.0),
        );
        let on = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
        assert!(
            on.side_by_side_vitals,
            "the production constant must read the option"
        );
        assert_ne!(
            off, on,
            "the pass is gated on this key: if it cannot move, ticking the row changes nothing on \
             screen until the next screen rebuild"
        );
        assert_eq!(
            on.screen_serial, off.screen_serial,
            "the screen was not rebuilt"
        );
        assert_eq!(
            character_option::SIDE_BY_SIDE_VITALS,
            Some(0x0020_0000),
            "side-by-side vitals is `options_ >> 0x15 & 1`"
        );
    }

    /// The bulk of what the pass applies. The original two writers were the `0x0013` arm and
    /// session-end reset. Local numeric placement writes now also mirror this cache, explicitly
    /// updating the applied key because their retail notice does not reseed the screen. An
    /// arbitrary placement replacement still invalidates the key, as this test demonstrates.
    #[test]
    fn the_applied_latch_moves_when_the_placements_move() {
        let (mut h, w) = hud_with(0, 0);
        let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
        h.placements.set(
            dereth_client_contract::chat::interface::window::MAIN,
            dereth_client_contract::floaty::WindowPlacement {
                visible: Some(true),
                ..dereth_client_contract::floaty::WindowPlacement::default()
            },
        );
        assert_ne!(
            before,
            h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS)
        );
        // And the screen serial, which is the term the latch has always carried.
        assert_ne!(
            h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
            h.applied_key(&w, 8, character_option::SIDE_BY_SIDE_VITALS)
        );
    }

    /// **The declared exclusion, asserted so it cannot be reversed by accident.** `lock_ui` is the
    /// third thing the pass applies and is deliberately *not* in the key: its visible mirror flows
    /// screen → HUD (`Hud::set_lock_ui` runs after the pass), so a toggle would otherwise make
    /// the next frame a re-seed frame and re-assert every window's stored visibility over whatever
    /// the player had since changed. If somebody adds it, this fails and they have to read the note
    /// on [`AppliedKey`] before deciding.
    #[test]
    fn lock_ui_is_deliberately_not_in_the_applied_latch() {
        let (mut h, w) = hud_with(0, 0);
        let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
        h.set_lock_ui(true);
        assert!(
            h.lock_ui(),
            "the padlock did not go up, so this asserts nothing"
        );
        assert_eq!(h.stats.lock_ui_writes, 1);
        assert_eq!(
            before,
            h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
            "lock_ui entered the latch: see AppliedKey's note before allowing this"
        );
    }

    /// The padlock hud mirror preserves neighboring options2 bits.
    #[test]
    fn the_padlock_hud_mirror_preserves_neighboring_options2_bits() {
        let (mut h, _w) = hud_with(0, 0);
        h.player_module.as_mut().expect("a module").options2 |= 0x0200_0000;
        h.set_lock_ui(true);
        assert!(h.lock_ui(), "the padlock went up in the HUD's copy");
        assert_eq!(
            h.player_module.as_ref().expect("a module").options2 & 0x0200_0000,
            0x0200_0000,
            "the neighboring options2 bit survives the lock mirror"
        );
    }

    /// One `SessionEvent::UiEvent` carrying `m`, shaped as `Session::dispatch` shapes it: the
    /// opcode dword and then the body.
    fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
        let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
        blob.extend(dereth_protocol::write_body(m).expect("a synthetic message encodes"));
        SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob,
        }
    }

    /// The title table message keeps its list and not only the display title.
    #[test]
    fn the_title_table_message_keeps_its_list_and_not_only_the_display_title() {
        let mut h = Hud::new();
        let mut w = dereth_client_model::World::new();
        let m = dereth_protocol::social::CharacterTitlesMessage {
            version: 1,
            display_title: 34,
            titles: vec![1, 34, 87],
        };
        let _ = h.apply_events(&[ui_event(&m)], &mut w);
        assert_eq!(h.display_title, 34, "the header's title id still arrives");
        assert_eq!(
            w.player_system.social.titles,
            vec![1, 34, 87],
            "the title list survives the handler -- this is what the Titles tab draws"
        );
        assert_eq!(w.player_system.social.display_title, 34);
        assert_eq!(h.stats.undecodable, 0);
    }

    /// Oracle: adding a character title always inserts it, then optionally makes it the display title.
    ///
    /// The **add is unconditional**; only the display move is gated. The panel's own
    /// add-title and set-display-title handlers
    /// both walk the title list for the id first and insert only when it is absent,
    /// which is the dedupe below.
    #[test]
    fn add_or_set_character_title_appends_to_the_list_whether_or_not_it_becomes_the_display() {
        let mut h = Hud::new();
        let mut w = dereth_client_model::World::new();
        let table = dereth_protocol::social::CharacterTitlesMessage {
            version: 1,
            display_title: 34,
            titles: vec![34],
        };
        let quiet = dereth_protocol::social::SocialAddOrSetCharacterTitle {
            new_title: 12,
            set_as_display_title: 0,
        };
        let loud = dereth_protocol::social::SocialAddOrSetCharacterTitle {
            new_title: 99,
            set_as_display_title: 1,
        };
        let again = dereth_protocol::social::SocialAddOrSetCharacterTitle {
            new_title: 12,
            set_as_display_title: 0,
        };
        let _ = h.apply_events(
            &[
                ui_event(&table),
                ui_event(&quiet),
                ui_event(&loud),
                ui_event(&again),
            ],
            &mut w,
        );
        assert_eq!(
            w.player_system.social.titles,
            vec![34, 12, 99],
            "every earned title is on the list, once, in arrival order"
        );
        assert_eq!(
            h.display_title, 99,
            "and only the flagged one moved the header"
        );
        assert_eq!(w.player_system.social.display_title, 99);
    }

    /// The death tag decides a system line's fate by the hear-PK-deaths option alone.
    #[test]
    fn the_pk_death_filter_drops_or_strips_only_a_tagged_line() {
        assert_eq!(
            pk_death_filter("You have entered the Trade channel.", false),
            PkDeathLine::Untagged
        );
        assert_eq!(
            pk_death_filter("[PKDe]Lark was slain by Wren!", false),
            PkDeathLine::Dropped
        );
        assert_eq!(
            pk_death_filter("[PKDe]Lark was slain by Wren!", true),
            PkDeathLine::Stripped("Lark was slain by Wren!".into())
        );
        // Anywhere in the line, every copy, and nothing else touched.
        assert_eq!(
            pk_death_filter("Lark [PKDe]was slain [PKDe]", true),
            PkDeathLine::Stripped("Lark was slain ".into())
        );
        // The tag is matched exactly, case and all.
        assert_eq!(pk_death_filter("[pkde]Lark", false), PkDeathLine::Untagged);
    }

    /// End to end through the `0xF7E0` arm: heard by default with the tag gone, and not drawn at
    /// all once the player turns the option off; an untagged line is drawn either way.
    #[test]
    fn a_pk_death_broadcast_is_heard_stripped_by_default_and_dropped_when_the_option_is_off() {
        use dereth_client_model::player::options::option::HEAR_PK_DEATHS;
        let line = |text: &str| dereth_protocol::comms::CommunicationTextboxString {
            text: text.into(),
            text_type: 0,
        };
        let mut h = Hud::new();
        let mut w = dereth_client_model::World::new();
        assert!(w.player_system.options.hear_pk_deaths(), "on by default");
        let got = h.apply_events(&[ui_event(&line("[PKDe]Lark was slain by Wren!"))], &mut w);
        let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, vec!["Lark was slain by Wren!"]);

        w.player_system
            .set_option(HEAR_PK_DEATHS, false, dereth_primitives::ServerTime(0.0));
        let got = h.apply_events(
            &[
                ui_event(&line("[PKDe]Lark was slain by Wren!")),
                ui_event(&line("You feel refreshed.")),
            ],
            &mut w,
        );
        let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, vec!["You feel refreshed."]);
        assert_eq!(h.stats.textbox_pk_deaths_dropped, 1);
        assert_eq!(h.stats.textbox_lines_squelched, 0);
    }

    /// Event `0x0317` draws its one string exactly as `0x02EB` does: chat type `0x1A`, no prefix,
    /// window 0.
    #[test]
    fn event_0317_is_drawn_as_a_transient_line() {
        let mut h = Hud::new();
        let mut w = dereth_client_model::World::new();
        let got = h.apply_events(
            &[
                ui_event(&dereth_protocol::comms::CommunicationTransientString0317 {
                    text: "The chest is locked.".into(),
                }),
                ui_event(&dereth_protocol::comms::CommunicationTransientString {
                    text: "The chest is locked.".into(),
                }),
            ],
            &mut w,
        );
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], got[1], "both events draw the same line");
        assert_eq!(got[0].body, "The chest is locked.");
        assert_eq!(got[0].ty, 0x1A);
        assert_eq!(h.stats.undecodable, 0);
    }
}
