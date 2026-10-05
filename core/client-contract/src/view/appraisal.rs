//! The appraisal read-out: an examined object's properties, requirements and magic.

/// The assessed-object fields the examination window needs.
///
/// The initial six-field projection of `AppraisalProfile` was:
///
/// | field | property |
/// |---|---|
/// | `creature` | the `0x0100` creature block |
/// | `template` | string 5, `Template` |
/// | `character_title` | integer `0x105` (261), `CharacterTitleId` |
/// | `gear_plating_name` | string `0x34` (52), `GearPlatingName` |
/// | `value` | integer `0x13` (19), `Value` |
/// | `burden` | integer 5, `EncumbranceVal` |
///
/// `template` and `character_title` record presence, not value: either present
/// property sends a creature to the character pane. **This now carries the
/// whole readable profile.** The initial six served the frame and value/burden
/// blocks; the rest serve the creature pane and ordered item-examine blocks.
/// Skill names, creature-type names and attribute labels cross as resolved strings
/// because their dat sources (`SkillTable 0x0E000004`, `EnumMapper 0x2200000E`)
/// are unavailable to this crate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppraisalView {
    /// `dereth_client_model::AppraisalCache::delivery` — which `0x00C9` this profile came in on.
    ///
    /// It has no counterpart in the client and is not read from the profile: it is what lets a
    /// per-frame **pull** tell a new reply from the same one being offered again, which a client
    /// called *by* the reply never has to ask. See the field's own note in `dereth_client_model::appraisal`.
    pub delivery: u64,
    pub creature: bool,
    pub template: bool,
    pub character_title: bool,
    pub gear_plating_name: Option<String>,
    pub value: Option<i32>,
    pub burden: Option<i32>,

    // ---- the frame's own flag ------------------------------------------------------------------
    /// Whether appraisal succeeded. Every update and every modifier line branches on it:
    /// a failed assess draws its numbers in the "unknown" font and the vitals as a bare percentage.
    pub success: bool,

    // ---- the creature and character panes ------------------------------------------------------
    /// `InqInt(0x19)` — the creature pane's level text. `< 1` is `"???"`.
    pub level: Option<i32>,
    /// The creature-type label, resolved from integer quality 2 through enumeration table
    /// `0x2200000E` and displayed in the creature pane's first line (`0x1000014E`).
    pub creature_display_name: Option<String>,
    /// The six primary attributes for ids `1..=6`, in **attribute-id** order
    /// (`Strength Endurance Quickness Coordination Focus Self`); the pane's *drawn* order swaps
    /// the middle pair. `None` is the client's zero-is-absent.
    pub attributes: [Option<u32>; 6],
    /// The vital lookup for `1..=6`, in vital-id order
    /// (`MaxHealth Health MaxStamina Stamina MaxMana Mana`).
    pub vitals: [Option<u32>; 6],
    /// The attribute enchantment-mod query for `1..=6`; `Some(true)` beneficial, `Some(false)` harmful.
    pub attribute_enchanted: [Option<bool>; 6],
    /// The vital enchantment-mod query for the three **maxima**, indexed the same as `vitals`;
    /// the current vitals are always `None` because the client has no bit for them.
    pub vital_enchanted: [Option<bool>; 6],

    // ---- the item pane's blocks ----------------------------------------------------------------
    /// The object's valid locations, or the hooked-item override for a hooked item —
    /// the value every arm of the weapon-and-armour appraisal display branches on.
    pub valid_locations: u32,
    /// The object's ammunition type, or the hooked-item override for a hooked item.
    pub ammo_type: u16,
    /// The unpacked weapon block (`0x0020`).
    pub weapon: Option<WeaponView>,
    /// `InqInt(0x161)` `WeaponType`, the parenthesised family after the skill name.
    pub weapon_type: Option<i32>,
    /// Attack-form bits used by the special-properties display.
    pub attack_type: Option<i32>,
    /// Additional elemental damage reported separately from the weapon's base damage.
    pub elemental_damage_bonus: Option<i32>,
    /// The literal heritage restriction attached to item activation.
    pub activation_heritage: Option<String>,
    /// `InqInt(0x1C)` `ArmorLevel`.
    pub armor_level: Option<i32>,
    /// The armor query — the `0x0080` block's eight floats, in **wire** order
    /// (`slash pierce bludgeon cold fire acid nether electric`).
    pub armor_mods: Option<[f32; 8]>,
    /// The profile's integer and float enchantment-mod queries, **already resolved**, for every
    /// property
    /// the presentation's highlighted-property table names.
    ///
    /// Same encoding as [`Self::attribute_enchanted`]: a key is present only when the
    /// property is enchanted at all (the profile's low bit), and the value is whether the
    /// enchantment **raised** it (the same bit sixteen places up). Absent is plain.
    ///
    /// The two bitfield tables live in `dereth_client_model::appraisal` -- `int_highlight`,
    /// `float_highlight` and `highlight_state` -- and the join is made in `Hud::appraisal`, so the
    /// bit arithmetic has exactly one copy and this crate never sees a raw profile.
    pub enchantment_mods: std::collections::BTreeMap<u32, bool>,
    /// String property `0x0E` `Use` — the usage block.
    pub use_text: Option<String>,
    /// `InqInt(0x10C)` `RemainingLifespan`, present only when the preceding
    /// `InqInt(0x10B) Lifespan` and `InqInt(0x62) CreationTimestamp` calls also succeed.
    ///
    /// The other two values are not carried: the original uses them only as presence
    /// gates, then format this value directly without consulting a clock or computing a remainder.
    pub remaining_lifespan: Option<i32>,
    /// String property `0x10` `LongDesc` — the description block.
    pub long_desc: Option<String>,
    /// String property `0x0F` `ShortDesc` — the fallback read only when LongDesc is absent.
    /// Presence is retained independently because a present empty LongDesc suppresses this arm.
    pub short_desc: Option<String>,
    /// The augmentation cost from property 3. Presence alone gates the
    /// localized cost row, including zero and negative values.
    pub augmentation_cost: Option<i64>,
    /// `InqInt(0xAC)` gates the present-LongDesc decoration branch, including a zero mask.
    pub long_desc_decoration: Option<u32>,
    /// Positive `InqInt(0x83)` resolved through the material mapper; a lookup miss is `Some("")`.
    pub description_material: Option<String>,
    /// Present GemCount/GemType pair, with the singular/plural name resolved for that count.
    pub description_gems: Option<(i32, String)>,
    /// `InqInt(0x6F)` `PortalBitmask` — the guard *and* the operand of the description block's
    /// portal section.
    ///
    /// `None` is the client's `InqInt` answering zero, which skips the whole block — including
    /// its two unconditional info lines. ACE's `PropertyInt.PortalBitmask = 111` is an
    /// `[AssessmentProperty]`, so any portal the shard describes carries it;
    /// `ACE.Entity.Enum.PortalBitmask` names the bits
    /// (`Unrestricted 0x01, NoPk 0x02, NoPKLite 0x04, NoNPK 0x08, NoSummon 0x10, NoRecall 0x20`).
    pub portal_bitmask: Option<i32>,

    // ---- the lock-appraisal block -------------------------------------------------------------
    /// Whether the examined object is a hook. A hook skips the whole lock block.
    ///
    /// **The predicate is the hook test, not creature detection.** The independent
    /// appraise-info path confirms hook detection: a true result leads to a query
    /// for valid hook locations.
    ///
    /// Five blocks use the predicate: lock appraisal, item appraise-info, boost value,
    /// heal kit and remaining uses.
    pub weenie_is_hook: bool,
    /// Bit `0x10000` of the examined object's public-description flags — the first thing
    /// the boost-value block and the heal-kit block test.
    /// Set means "this is a healing kit", and it forks the two
    /// blocks apart: the boost-value block returns outright, the heal-kit block is the only arm
    /// that draws.
    pub weenie_is_healer: bool,
    /// The object's `0x20000` description bit, which means "lockpick". Together with the healer
    /// bit it decides whether an unassessed object gets
    /// *"Number of uses remaining:  Unknown"* at all.
    pub weenie_is_lockpick: bool,
    /// The item capacity is read off the **object**, not the profile. No `0x00C9` carries it.
    pub items_capacity: i32,
    /// The object's container capacity.
    pub containers_capacity: i32,
    /// Whether the appraisal profile describes a hooked item — a null test on its hook profile,
    /// so this
    /// is simply "the profile carries a `0x0040` hook block".
    /// The lock-appraisal block is its only caller.
    pub hooked_item: bool,
    /// Whether the hook profile's is-healer bit is set,
    /// which is the hook profile's bitfield `& 2`. The boost-value block
    /// and the heal-kit block ask it, and between them it decides which of
    /// the two blocks draws `InqInt(0x5A)`.
    pub hooked_item_healer: bool,
    /// Whether the hook profile's is-lockpick bit is set; it is
    /// `bitfield & 8`. The lock-appraisal block is its only caller here.
    pub hooked_item_lockpick: bool,

    // ---- the tinkering block ------------------------------------------------------------------
    /// `InqInt(0xAB)` `NumTimesTinkered` — *"This item has been tinkered %d time%s."*
    pub num_times_tinkered: Option<i32>,
    /// String property `0x27` `TinkerName` — *"Last tinkered by %s."*
    pub tinker_name: Option<String>,
    /// String property `0x28` `ImbuerName` — *"Imbued by %s."*
    pub imbuer_name: Option<String>,
    /// `InqInt(0x69)` `ItemWorkmanship` — the gate of the workmanship line.
    pub workmanship: Option<i32>,
    /// `InqInt(0xAA)` `NumItemsInMaterial` — present takes the salvage arm, absent the plain one.
    pub num_items_in_material: Option<i32>,

    // ---- the equipment-set block ---------------------------------------------------------------
    /// `InqInt(0x109)` `EquipmentSetId`, fed to the block's own 88-arm switch.
    pub equipment_set_id: Option<i32>,

    // ---- the gear-ratings block ----------------------------------------------------------------
    /// The thirteen `Gear*` rating terms in the block's **drawn** order — see
    /// the presentation's gear-rating rows — plus `GearMaxHealth` last.
    pub gear_ratings: [Option<i32>; 14],

    // ---- the defence-mod block -----------------------------------------------------------------
    /// Float property `0x1D` `WeaponDefense`, float property `0x95` `WeaponMissileDefense`,
    /// float property `0x96` `WeaponMagicDefense`. Each is drawn only when present **and not 1.0**.
    pub defense_mods: [Option<f64>; 3],

    // ---- the caster block ----------------------------------------------------------------------
    /// Float property `0x90` `ManaConversionMod`. The line prints `v + 1.0` as a modifier.
    pub mana_conversion_mod: Option<f64>,
    /// Float property `0x98` `ElementalDamageMod`, paired with [`Self::caster_damage_type`]; both
    /// are needed.
    pub elemental_damage_mod: Option<f64>,
    /// `InqInt(0x2D)` `DamageType` — the `%s` of *"Damage bonus for %s spells:"*.
    pub caster_damage_type: Option<i32>,

    // ---- the level-limit block -----------------------------------------------------------------
    /// `InqInt(0x56)` `MinLevel` and `InqInt(0x57)` `MaxLevel`, both defaulted to `-1` by the
    /// block itself so an absent key and a non-positive one take the same arm.
    pub level_limits: (Option<i32>, Option<i32>),
    /// String property `0x26` `AppraisalPortalDestination` — *"Destination: "* plus the text.
    pub portal_destination: Option<String>,

    // ---- the wield-requirements block ----------------------------------------------------------
    /// `InqBool(0x55)` `AppraisalHasAllowedWielder`, tested `== 1`.
    pub has_allowed_wielder: bool,
    /// String property `0x19` `CraftsmanName` — the `%s` of the allowed-wielder, allowed-activator
    /// and *"Created by %s."* lines alike. Its default in the first two is *"the original owner"*.
    pub craftsman_name: Option<String>,
    /// `InqInt(0x1A)` `AccountRequirements`, `== 1` being *"Use requires Throne of Destiny."*
    pub account_requirements: Option<i32>,
    /// `InqInt(0x144)` `HeritageSpecificArmor`, resolved through the appraisal system's
    /// heritage-group display name.
    pub heritage_specific_armor: Option<String>,
    /// The three `(requirement, skill-or-attribute id, difficulty)` triples —
    /// `0x9E/0x9F/0xA0`, `0x10E/0x10F/0x110`, `0x111/0x112/0x113`. All three of a triple must be
    /// present or the triple draws nothing.
    pub wield_requirements: Vec<WieldRequirementView>,

    // ---- the usage-limit block -----------------------------------------------------------------
    /// `InqInt(0x171)` `UseRequiresLevel`, above 0.
    pub use_requires_level: Option<i32>,
    /// `InqInt(0x16E)` `UseRequiresSkill` resolved to a name, with `InqInt(0x16F)`
    /// `UseRequiresSkillLevel`. `None` for the name is the block's *"Unknown Skill"*.
    pub use_requires_skill: Option<(Option<String>, i32)>,
    /// `InqInt(0x170)` `UseRequiresSkillSpec` resolved to a name — *"Use requires specialized %s."*
    pub use_requires_skill_spec: Option<Option<String>>,

    // ---- the item-level block ------------------------------------------------------------------
    /// Int64 property `5` `ItemBaseXp`, `InqInt(0x13F)` `ItemMaxLevel`, `InqInt(0x140)`
    /// `ItemXpStyle` and int64 property `4` `ItemTotalXp` — the four the level line needs.
    pub item_level: Option<ItemLevelView>,
    /// `InqInt(0x160)` `CloakWeaveProc`, `== 2` being the damage-reduction sentence.
    pub cloak_weave_proc: Option<i32>,

    // ---- the activation-requirements block -----------------------------------------------------
    /// `InqInt(0x6D)` `ItemDifficulty`, above 0 — *"Arcane Lore: %d"*.
    pub item_difficulty: Option<i32>,
    /// `InqInt(0x6E)` `ItemAllegianceRankLimit`, above 0 — *"Allegiance Rank: %d"*.
    pub allegiance_rank_limit: Option<i32>,
    /// `InqInt(0xBC)` `HeritageGroup` through.
    pub heritage_group: Option<String>,
    /// `InqInt(0x73)` `ItemSkillLevelLimit` above 0 with `InqInt(0xB0)` `AppraisalItemSkill`
    /// resolved to a name — *"%s: %d"*. A skill the table has no row for drops the whole term.
    pub activation_skill: Option<(String, i32)>,
    /// `InqInt(0x102)` `ItemAttributeLevelLimit` above 0 with `InqInt(0x101)`
    /// `ItemAttributeLimit` through the attribute-name lookup.
    pub activation_attribute: Option<(String, i32)>,
    /// `InqInt(0x104)` `ItemAttribute2ndLevelLimit` above 0 with `InqInt(0x103)`
    /// `ItemAttribute2ndLimit` through the secondary-attribute-name lookup.
    pub activation_attribute_2nd: Option<(String, i32)>,
    /// `InqBool(0x5E)` `AppraisalHasAllowedActivator`, tested `== 1`.
    pub has_allowed_activator: bool,

    // ---- the boost-value block / the heal-kit block --------------------------------------------
    /// `InqInt(0x5A)` `BoostValue` — the `%d` of both blocks' first line.
    pub boost_value: Option<i32>,
    /// `InqInt(0x59)` `BoosterEnum` — `2` Health, `4` Stamina, `6` Mana, and nothing else draws.
    pub booster_enum: Option<i32>,
    /// Float property `0x64` `HealkitMod` — *"Restoration Bonus: %d%%"*, the float times **100**.
    ///
    pub healkit_mod: Option<f64>,

    // ---- the capacity block --------------------------------------------------------------------
    /// `InqInt(0xAF)` `AppraisalMaxPages` and `InqInt(0xAE)` `AppraisalPages` — the
    /// *"%d of %d pages full."* pair, and both are needed.
    pub pages: Option<(i32, i32)>,

    // ---- the mana-stone block ------------------------------------------------------------------
    /// `InqInt(0x6B)` `ItemCurMana` — *"Stored Mana: %d"*. The whole block is gated on the
    /// profile carrying **no** spell book.
    pub stored_mana: Option<i32>,
    /// Float property `0x57` `ItemEfficiency` — *"Efficiency: %d%%"*, truncated, **not** scaled.
    pub item_efficiency: Option<f64>,
    /// Float property `0x89` `ManaStoneDestroyChance` — *"Chance of Destruction: %d%%"*, likewise.
    pub destroy_chance: Option<f64>,

    // ---- the remaining-uses block --------------------------------------------------------------
    /// `InqInt(0xC1)` `NumKeys` — *"Contains %d key."* / *"…keys."*
    pub num_keys: Option<i32>,
    /// `InqBool(0x3F)` `UnlimitedUse` — *"Number of uses remaining:  Unlimited"* (two spaces).
    pub unlimited_use: bool,
    /// `InqInt(0x5C)` `Structure` — *"Number of uses remaining: %d"*.
    pub structure: Option<i32>,

    // ---- the is-sellable line ------------------------------------------------------------------
    /// `IsSellable`, and the test is **present and false** — `InqBool` answering true with a
    /// zero value. An absent key draws nothing.
    pub cannot_be_sold: bool,

    // ---- the rare-info block -------------------------------------------------------------------
    /// `InqBool(0x6C)` `RareUsesTimer`, tested `== 1`. Both `%d` of its sentence are the literal
    /// `3` the block pushes, not a property.
    pub rare_uses_timer: bool,
    /// `InqInt(0x11)` `RareId` — *"Rare #%d"*.
    pub rare_id: Option<i32>,
    /// `InqBool(3)` `Locked`. `None` is the client's `InqBool` answering false, which takes the
    /// block's **other** arm — the `Bonus to Lockpick Skill` line — not a silent skip.
    pub locked: Option<bool>,
    /// `InqInt(0x26)` `ResistLockpick` — the `%d` of the last line, and the gate that decides
    /// between it and *"You can't tell how hard the lock is to pick."*
    pub resist_lockpick: Option<i32>,
    /// `InqInt(0xAD)` `AppraisalLockpickSuccessPercent`, fed to the appraisal system's
    /// lockpick-percent formatter.
    pub lockpick_success_percent: Option<i32>,

    // ---- the special-properties block ----------------------------------------------------------
    /// The block's own inputs; see [`SpecialPropertiesView`].
    pub special: SpecialPropertiesView,

    // ---- the short and long magic blocks -------------------------------------------------------
    /// The two spell blocks' inputs; see [`MagicInfoView`]. The spell book behind it is
    /// decoded by the appraisal profile and cached whole.
    pub magic: MagicInfoView,

    // ---- the inscription box -------------------------------------------------------------------
    /// Bit `0x2` of the public-description flags, or the hooked-item inscription rule — the
    /// one test the inscription check makes before it looks at the profile.
    pub inscribable: bool,
    /// The viewer's PSR flag: Boolean quality `0x2C`, `0x2D`, or
    /// `0x61`. The inscription's editable-state setter consults it only after the displayed
    /// object's live PWD inscribable-bit gate and an ordinary scribe/ownership check fail.
    pub viewer_is_psr: bool,
    /// String property `8` `ScribeName`.
    pub scribe_name: Option<String>,
    /// String property `7` `Inscription`.
    pub inscription: Option<String>,
    /// The is-owned-by-the-player walk — owned-by-object against the player id, which is
    /// "this object *is* the player, or its container or wielder is, or it is inside
    /// something that is".
    ///
    /// The one question the inscription's editable-state setter asks that is not on the profile:
    /// is this object the player's? The game world's ownership query performs the same walk and
    /// answers it at the seam.
    pub owned_by_player: bool,

    // ---- the character pane's own appraise-info setter ----------------------------------------
    //
    // The character pane's own half: the society, allegiance and armour blocks. Every key below is an
    // `[AssessmentProperty]` in `ACE.Entity/Enum/Properties`, so the shard sends all of them.
    /// The appraisal system's gender-and-heritage display of `InqInt(0x71)` `Gender`,
    /// `InqInt(0xBC)` `HeritageGroup` and — only when the heritage is **0** — `InqInt(2)`
    /// `CreatureType`. Written to the heritage text `0x10000150`.
    ///
    /// Resolved at the seam because two of its three terms are dat lookups (`EnumMapper`s
    /// `0x1000000C` and `0x10000002`), which this crate cannot read.
    pub gender_heritage_display: Option<String>,
    /// The profession text `0x10000151`, and it is a **two-source** line:
    /// the character-title table's lookup of
    /// `InqInt(0x105)` `CharacterTitleId` first, and string property `5` `Template` only if that
    /// `InqInt` missed **or** the title table had no row (a flag is set only inside the
    /// found arm, and testing it is what skips the fallback).
    pub profession: Option<String>,
    /// The is-PK predicate — `_bitfield & 0x20` — on the **examined**
    /// object, for the PK-status text `0x10000152`.
    pub weenie_is_pk: bool,
    /// The is-PK-lite predicate — `_bitfield & 0x2000000`, asked only when
    /// is-PK said no.
    pub weenie_is_pk_lite: bool,
    /// Resolve this object's allegiance title from rank, heritage, and gender —
    /// `dereth_client_model::allegiance::get_title`.
    ///
    /// The allegiance full name is `title + " " + name` when this is `Some` and
    /// the bare name when it is `None`, and that is written over the title bar the
    /// enclosing pane had already filled.
    pub allegiance_title: Option<String>,
    /// `InqInt(0x119)` `Faction1Bits` on the examined object — the society selector, tested
    /// `& 1` Celestial Hand, `& 2` Eldrytch Web, `& 4` Radiant Blood.
    /// Present-but-none-of-the-three draws *"???"*.
    pub faction_bits: Option<i32>,
    /// The **viewer's** own `Faction1Bits`, read off `PlayerDesc` through
    /// integer quality `0x119`, and used for nothing but the
    /// row's colour: same society `1`, a different one `2`, none `0`.
    pub viewer_faction_bits: i32,
    /// `InqInt(0x11F/0x120/0x121)` `SocietyRankCelhan/Eldweb/Radblo`, in that order. The client
    /// reads only the one its `Faction1Bits` selected and leaves the local at **0** when the key
    /// is absent, so an absent rank and a zero rank take the same arm — no suffix.
    pub society_ranks: [i32; 3],
    /// `InqInt(0x1E)` `AllegianceRank`. `< 1` skips the whole allegiance block.
    pub allegiance_rank: Option<i32>,
    /// String property `0x2F` `AllegianceName` — the allegiance-name text `0x1000053A`, which is
    /// cleared unconditionally first.
    pub allegiance_name: Option<String>,
    /// String property `0x15` `MonarchsTitle`. Absent takes the *"Alleg. Monarch:"* follower arm.
    pub monarch_title: Option<String>,
    /// String property `0x23` `PatronsTitle`. Equal to the monarch's is one *"Monarch/Patron:"*
    /// row; different is two rows; absent is *"Monarch:"* alone.
    pub patron_title: Option<String>,
    /// `InqInt(0x23)` `AllegianceFollowers` — the same key number in the **int** table, which is
    /// how one property id serves two rows. Absent or negative is 0.
    pub allegiance_followers: Option<i32>,
    /// The `0x4000` base-armour block: head, chest, groin, bicep, wrist, hand, thigh,
    /// shin and foot. A value `>= 9999` marks an unenchantable piece and draws `*`
    /// plus `v - 9999`. Public ACE's `ArmorLevel.GetArmorLevel` adds the same 9999
    /// for that purpose.
    pub base_armor: Option<[i32; 9]>,
    /// The thirteen rating properties both panes read:
    /// `0x133 DamageRating`, `0x134 DamageResistRating`, `0x139 CritRating`,
    /// `0x13A CritDamageRating`, `0x13B CritResistRating`, `0x13C CritDamageResistRating`,
    /// `0x143 HealingBoostRating`, `0x15E DotResistRating`, `0x15F LifeResistRating`, then
    /// `0x17D PKDamageRating`, `0x17E PKDamageResistRating`, `0x182 Overpower` and
    /// `0x183 OverpowerResist`.
    ///
    /// See `dereth_presentation::appraisal::CHARACTER_RATING_ROWS`; `0x143` is read and **never used**.
    pub ratings: [Option<i32>; 13],
    /// String property `0x0A` `Fellowship` — *"Fellowship:"*.
    pub fellowship: Option<String>,
    /// String property `0x2B` `DateOfBirth` — *"Arrived in Dereth:"*.
    pub date_of_birth: Option<String>,
    /// `InqInt(0x7D)` `Age`, through the client's elapsed-time formatter —
    /// *"Time in Dereth:"*. That formatter already exists as
    /// `crate::journal::delta_time_to_string`.
    pub age: Option<i32>,
    /// `InqInt(0xB5)` `ChessRank` — *"Chess Rank:"*.
    pub chess_rank: Option<i32>,
    /// `InqInt(0xC0)` `FakeFishingSkill` — *"Fishing Skill:"*.
    pub fishing_skill: Option<i32>,
    /// `InqInt(0x2B)` `NumDeaths` — *"Deaths:"*, and `<= 0` is *"Has never died"*.
    /// The **same** key number as [`Self::date_of_birth`] in the string table.
    pub num_deaths: Option<i32>,
    /// `InqInt(0x106)` `NumCharacterTitles` — *"Titles Earned:"*.
    pub num_character_titles: Option<i32>,
    /// `InqInt(0x186)` `Enlightenment` — the character pane's *"Enlightenment:"* line, drawn
    /// whenever the property is present, zero included.
    pub enlightenment: Option<i32>,
    /// The rules the world plays by: what its own client showed in a shield's slot, how it
    /// worded the texts a world may word differently.
    pub world_rules: dereth_primitives::WorldRules,
}

/// One `(requirement, skill/attribute, difficulty)` triple of the wield-requirements block.
///
/// The block reads three of these — `0x9E/0x9F/0xA0`, `0x10E/0x10F/0x110`, `0x111/0x112/0x113` —
/// and runs the identical switch on each. `subject` is what the item-examine window's
/// requirement-string helper makes of `(requirement, skill)`,
/// resolved at the seam because two of its arms are dat lookups (`SkillTable 0x0E000004`,
/// `EnumMapper 0x10000002`); `None` is that function leaving the string as the `"base "`/`""`
/// prefix it starts with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WieldRequirementView {
    /// `InqInt(0x9E)` `WieldRequirements` and its two continuations — the switch selector.
    pub requirement: i32,
    /// `InqInt(0xA0)` `WieldDifficulty` and its two continuations — the `%d`, and for
    /// `requirement == 8` the trained/specialized fork (`!= 3` is *"trained"*).
    pub difficulty: i32,
    /// What the appraisal requirement-string lookup answers for `(requirement, skill)`.
    pub subject: Option<String>,
}

/// The four values the item-level block needs before it can draw a level.
///
/// All four must be present and the last two positive, which is the block's own four-term
/// guard.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ItemLevelView {
    /// Int64 property `4` `ItemTotalXp`.
    pub total_xp: u64,
    /// Int64 property `5` `ItemBaseXp` — and the guard is that it is **non-zero**, not positive.
    pub base_xp: u64,
    /// `InqInt(0x13F)` `ItemMaxLevel`, above 0.
    pub max_level: i32,
    /// `InqInt(0x140)` `ItemXpStyle`, above 0; `1`, `2` and `3` are the three curves the
    /// item-level inverse knows.
    pub xp_style: i32,
}

/// The `WeaponProfile` block (`0x0020`) of an `AppraisalProfile`, carried whole
/// because the weapon-and-armour display reads nine of its ten fields.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponView {
    pub damage_type: u32,
    pub weapon_time: i32,
    pub weapon_skill: u32,
    pub weapon_damage: i32,
    pub damage_variance: f64,
    pub damage_mod: f64,
    pub max_velocity: f64,
    pub weapon_offense: f64,
    /// `max_velocity_estimated` — non-zero appends `" (based on STRENGTH 100)"` to the range line.
    pub max_velocity_estimated: i32,
}

/// The special-properties block's inputs.
///
/// Each field is one `Inq*` in the client's own order; the key of every one is named and addressed
/// in `dereth_client_model::appraisal_model::property::special`. The whole block is a **list**, joined
/// with `", "` and drawn as one `"Properties: …"` line, plus three
/// stand-alone sentences around it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpecialPropertiesView {
    /// `InqInt(0x117)` — *"You can only carry N of these items."*
    pub unique_limit: Option<i32>,
    /// Present Float0xA7 gates both cooldown lines, including zero duration.
    pub cooldown_duration: Option<f64>,
    /// Int0x118 presence also owns the trailing blank, even when the group is not active.
    pub cooldown_group: Option<u32>,
    /// PlayerDesc registry query for group+0x8000 at the current display time, not item qualities.
    pub cooldown_remaining: Option<f64>,
    /// `InqInt(0x124)` — the `%d` of *"Cleave: %d enemies in front arc."*, drawn only above 1.
    pub cleave: Option<i32>,
    /// `InqInt(0xA6)` and its resolved slayer name. Both are carried because the **id** decides
    /// the `Bael'Zharon's Hate` special case
    /// (`0x1F`) and the *name* is what gets `" slayer"` appended.
    pub slayer: Option<(i32, String)>,
    /// `InqInt(0x2F)` `WeaponSkill`.
    pub weapon_skill: Option<i32>,
    /// The five `ImbuedEffect` ints `or`-ed together, and `None` when not one of them was present
    /// — which is not the same as `Some(0)`: the *"cannot be further imbued"* sentence is drawn on
    /// a non-zero mask.
    pub imbued: Option<u32>,
    /// Float property `0x9F`.
    pub absorb_magic_damage: bool,
    /// `InqInt(0x24)`.
    pub item_spellcraft: Option<i32>,
    /// `InqInt(0x72)` `Attuned`, as the raw attuned-status value.
    pub attuned: Option<i32>,
    /// `InqInt(0x21)` `Bonded`, as the raw bonded-status value — **signed**, because
    /// destroy-on-death is `-2` and slippery is `-1`.
    pub bonded: Option<i32>,
    /// `InqBool(0x5B)`.
    pub retained: Option<bool>,
    /// Float property `0x88`.
    pub critical_multiplier: bool,
    /// Float property `0x93`.
    pub critical_frequency: bool,
    /// Float property `0x9B`.
    pub ignore_armor: bool,
    /// `InqInt(0x107)`, and only when float property `0x9D` was present too — the pair is one `&&`.
    pub resistance_cleaving: Option<u32>,
    /// DataID property `0x37`.
    pub proc_spell: bool,
    /// `InqBool(0x63)`.
    pub ivoryable: Option<bool>,
    /// `InqBool(0x64)`.
    pub dyeable: Option<bool>,
    /// `InqBool(0x82)`.
    pub tethered_left: Option<bool>,
}

/// One entry in the appraisal spell book's `0x0010` block, with its two dat strings
/// already resolved.
///
/// The client asks the magic system for the spell's name and its description,
/// both of which come from the spell table selected by group 6, type 2, and enum `0x10000005` —
/// the portal dat's `SpellTable 0x0E00000E`. This crate does not read dats, so the
/// strings cross the seam already looked up, the same way `creature_display_name` does.
///
/// **A spell id the table does not know still produces an entry**, with empty strings: the short
/// list sets its flag before the name is tested, so an unknown id still contributes its `", "`
/// separator and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppraisalSpellView {
    /// Whether the spell table contains this ID, independent of empty authored strings.
    pub resolved: bool,
    /// The id **as it arrived**, high bit and all — kept because it is the only thing that
    /// distinguishes two entries with the same resolved name.
    pub raw_id: u32,
    /// `raw_id & 0x80000000`. The bit is masked off before the lookup,
    /// and it is the **only** thing that decides where the entry goes: set sends it
    /// to the *"Enchantments:"* paragraph, clear to *"Spell Descriptions:"* and to the
    /// `Spells: ` short list, which skips set ids outright.
    pub enchantment: bool,
    /// The resolved spell name, or `""` for an id the `SpellTable` has no row for.
    pub name: String,
    /// The resolved spell description, likewise empty for an unknown id.
    pub description: String,
}

/// The inputs of the short and long magic-info blocks that put an item's spells in the appraisal
/// pane.
///
/// Both blocks first test the spell book and return when it is null, so the `Option` is not
/// decoration: a profile with **no** spell block draws
/// nothing at all, while one carrying an empty list on a failed assess still draws
/// *"Spells: unknown."* — twice, once from each block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MagicInfoView {
    /// The resolved spell-book rows. `None` is the client's null spell book.
    pub spells: Option<Vec<AppraisalSpellView>>,
    /// `InqInt(0x6A)` — *"Spellcraft: %d."*
    pub spellcraft: Option<i32>,
    /// `InqInt(0x6B)` — the first `%d` of *"Mana: %d / %d."*
    pub cur_mana: Option<i32>,
    /// `InqInt(0x6C)` — the second.
    pub max_mana: Option<i32>,
    /// Float property `5` `ManaRate`. Present takes the *"1 point per %d seconds"* arm and
    /// [`Self::mana_cost`] is never asked for.
    pub mana_rate: Option<f64>,
    /// `InqInt(0x75)` — *"Mana Cost: %d."*, and above zero the Mana Conversion footnote.
    pub mana_cost: Option<i32>,
}
