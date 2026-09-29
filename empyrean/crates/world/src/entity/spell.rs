// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Spell.cs
//! Port of `Source/ACE.Server/Entity/Spell.cs`.
//!
//! The whole class: construction, the members the enchantment managers read, the `Formula` field,
//! the level and category predicates, component burning and the component lookups. The `SpellProperties.cs` accessors are in
//! [`crate::entity::spell_properties`].
//!
//! `Console.WriteLine` diagnostics become `log::info!`. Members that read a creature's skills take
//! the world and the creature's guid.

use std::fmt;
use std::sync::Arc;

use dereth_assets::tables::SpellBase;
use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_dat::file_types::DualDidMapper;
use empyrean_entity::enums::properties::{PropertyAttribute, PropertyAttribute2nd, PropertyInt};
use empyrean_entity::enums::{
    EnchantmentTypeFlags, MagicSchool, Skill, SpellCategory, SpellFlags, SpellId, SpellType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::spell_formula::{self, SpellFormula};
use crate::world_objects::world_object_magic::creature_get_creature_skill_current;
use crate::World;

/// The Spell class for game code: a wrapper around the client dat's `SpellBase` and the world
/// database's `Spell` row. Both halves are copies of the immutable data ACE shares by reference.
// ACE: Spell
#[derive(Debug, Clone, Default)]
pub struct Spell {
    /// The spell information from the client DAT.
    // ACE: Spell._spellBase
    pub spell_base: Option<SpellBase>,
    /// The spell information from the server DB.
    // ACE: Spell._spell
    pub spell: Option<Arc<DbSpell>>,
    /// The components required to cast the spell (`null` when the dat has no such spell).
    // ACE: Spell.Formula
    pub formula: Option<SpellFormula>,
}

impl Spell {
    /// `new Spell(uint spellID, bool loadDB = true)`. The `int` and `SpellId` overloads are
    /// [`Spell::from_int`] and [`Spell::from_spell_id`]; all three cast to `uint` and call `Init`.
    // ACE: Spell.Spell
    #[must_use]
    pub fn new(w: &World, spell_id: u32, load_db: bool) -> Spell {
        let mut spell = Spell::default();
        spell.init(w, spell_id, load_db);
        spell
    }

    /// `new Spell(int spellID, bool loadDB = true)`: `Init((uint)spellID, loadDB)`.
    #[must_use]
    pub fn from_int(w: &World, spell_id: i32, load_db: bool) -> Spell {
        Spell::new(w, spell_id.cast_unsigned(), load_db)
    }

    /// `new Spell(SpellId spell, bool loadDB = true)`.
    #[must_use]
    pub fn from_spell_id(w: &World, spell: SpellId, load_db: bool) -> Spell {
        Spell::new(w, spell.0, load_db)
    }

    /// Default initializer.
    // ACE: Spell.Init
    pub fn init(&mut self, w: &World, spell_id: u32, load_db: bool) {
        self.spell_base = w
            .dats
            .portal_dat()
            .spell_table()
            .spells
            .get(&spell_id)
            .cloned();

        if load_db {
            self.spell = w.content.get_cached_spell(spell_id);
        }

        if self.spell_base.is_some() {
            self.formula = Some(SpellFormula::new(self, self.formula_list()));
        }

        if load_db && (self.spell.is_none() || self.spell_base.is_none()) {
            log::debug!(
                "Spell.Init(spellID = {spell_id}, loadDB = {load_db}) failed! {} {}",
                if self.spell.is_none() {
                    "_spell was null"
                } else {
                    ""
                },
                if self.spell_base.is_none() {
                    "_spellBase was null"
                } else {
                    ""
                }
            );
        }
    }

    /// Returns TRUE if spell is missing from either the client DAT or the server spell db.
    // ACE: Spell.NotFound
    #[must_use]
    pub fn not_found(&self) -> bool {
        self.spell_base.is_none() || self.spell.is_none()
    }

    /// `_spellBase.<member>`: ACE throws `NullReferenceException` when the dat has no such spell.
    pub(crate) fn base(&self) -> &SpellBase {
        self.spell_base
            .as_ref()
            .expect("ACE: Spell._spellBase is null (NullReferenceException)")
    }

    /// `_spell.<member>`: ACE throws `NullReferenceException` when the world database has no row,
    /// or the spell was built with `loadDB: false`.
    pub(crate) fn db(&self) -> &DbSpell {
        self.spell
            .as_deref()
            .expect("ACE: Spell._spell is null (NullReferenceException)")
    }

    /// `Formula.<member>`: ACE throws `NullReferenceException` when the dat has no such spell.
    pub(crate) fn formula_ref(&self) -> &SpellFormula {
        self.formula
            .as_ref()
            .expect("ACE: Spell.Formula is null (NullReferenceException)")
    }

    /// Uses the server spell level formula, which checks the power level of the spell, and
    /// compares to the minimum power for each spell level. (For the client / scarab-based
    /// formula, use `Formula.Level`.)
    // ACE: Spell.Level
    #[must_use]
    pub fn level(&self) -> u32 {
        let mut spell_level = spell_formula::MAX_SPELL_LEVEL;
        while spell_level > 0 {
            let min_power = spell_formula::min_power(spell_level);
            if self.power() >= min_power {
                return spell_level;
            }
            spell_level -= 1;
        }
        0
    }

    /// Returns TRUE if the spell levels are the same between the client and server formulas.
    // ACE: Spell.LevelMatch
    #[must_use]
    pub fn level_match(&self) -> bool {
        self.formula_ref().level() == self.level()
    }

    fn has_flag(&self, flag: SpellFlags) -> bool {
        (self.flags() & flag) == flag
    }

    /// Returns TRUE if this is a beneficial spell.
    // ACE: Spell.IsBeneficial
    #[must_use]
    pub fn is_beneficial(&self) -> bool {
        self.has_flag(SpellFlags::Beneficial)
    }

    /// Returns TRUE if this is a harmful spell.
    // ACE: Spell.IsHarmful
    #[must_use]
    pub fn is_harmful(&self) -> bool {
        !self.is_beneficial()
    }

    /// Returns TRUE if this spell is resistable.
    // ACE: Spell.IsResistable
    #[must_use]
    pub fn is_resistable(&self) -> bool {
        self.has_flag(SpellFlags::Resistable)
    }

    // ACE: Spell.IsProjectile
    #[must_use]
    pub fn is_projectile(&self) -> bool {
        self.num_projectiles() > 0
    }

    // ACE: Spell.IsSelfTargeted
    #[must_use]
    pub fn is_self_targeted(&self) -> bool {
        self.has_flag(SpellFlags::SelfTargeted)
    }

    // ACE: Spell.IsTracking
    #[must_use]
    pub fn is_tracking(&self) -> bool {
        !self.has_flag(SpellFlags::NonTrackingProjectile)
    }

    // ACE: Spell.IsFellowshipSpell
    #[must_use]
    pub fn is_fellowship_spell(&self) -> bool {
        // some spells are missing SpellFlags.FellowshipSpell:
        // 3043 - Kiss of the Grave
        // 3320 - Lesser Corrosive Ward
        // 3375 - Fungal Bloom
        // 3470 - Lesser Endless Well
        // 3474 - Lesser Soothing Wind
        // 3478 - Lesser Golden Wind

        // some spells have SpellFlags.FellowshipSpell, but aren't an actual Fellow* MetaSpellType:
        // 3337 - Inferno Ward (Enchantment)
        // 3381 - Debilitating Spore (Boost)
        // 3382 - Diseased Air (Boost)
        // 3406 - Kivik Lir's Boon (Enchantment)

        self.has_flag(SpellFlags::FellowshipSpell)
            || self.meta_spell_type() >= SpellType::FellowBoost
                && self.meta_spell_type() <= SpellType::FellowDispel
    }

    /// Rolls each component of `Formula.CurrentFormula` for burning; `player` is the caster.
    ///
    /// # Panics
    /// When `Formula` or `Formula.CurrentFormula` is null (`NullReferenceException`).
    // ACE: Spell.TryBurnComponents
    pub fn try_burn_components(&self, w: &mut World, player: ObjectGuid) -> Vec<u32> {
        let mut consumed = Vec::new();

        // the base rate for each component is defined per-spell
        let base_rate = self.component_loss();

        // get magic skill mod
        let magic_skill = self.get_magic_skill();
        let player_skill_current = creature_get_creature_skill_current(w, player, magic_skill);
        let power: f32 = self.power().cs_cast();
        let current: f32 = player_skill_current.cs_cast();
        let skill_mod = math::min_f32(1.0, power / current);
        //Console.WriteLine($"TryBurnComponents.SkillMod: {skillMod}");

        //DebugComponents();

        let current_formula = self
            .formula_ref()
            .current_formula
            .clone()
            .expect("ACE: SpellFormula.CurrentFormula is null (NullReferenceException)");
        for component in current_formula {
            let Some(spell_component) = spell_formula::spell_components_table(w)
                .components
                .get(&component)
            else {
                log::info!("Spell.TryBurnComponents(): Couldn't find SpellComponent {component}");
                continue;
            };

            // component burn rate = spell base rate * component destruction modifier * skillMod?
            let burn_rate = base_rate * spell_component.cdm * skill_mod;

            // TODO: curve?
            let rng = ThreadSafeRandom::next_float(0.0, 1.0);
            if rng < f64::from(burn_rate) {
                consumed.push(component);
            }
        }
        consumed
    }

    /// Logs the base and current formulas' component names.
    // ACE: Spell.DebugComponents
    pub fn debug_components(&self, w: &World) {
        let base_components = &self.formula_ref().components;
        let curr_components = self
            .formula_ref()
            .current_formula
            .as_deref()
            .unwrap_or_default();

        log::info!("{}:", self.name());
        log::info!(
            "Base formula: {}",
            Spell::get_component_names(w, base_components).join(", ")
        );
        log::info!(
            "Current formula: {}",
            Spell::get_component_names(w, curr_components).join(", ")
        );
    }

    // ACE: Spell.GetConsumeString
    #[must_use]
    pub fn get_consume_string(w: &World, components: &[u32]) -> String {
        let comp_names = Spell::get_component_names(w, components);
        format!(
            "The spell consumed the following components: {}",
            comp_names.join(", ")
        )
    }

    // ACE: Spell.GetComponentNames
    #[must_use]
    pub fn get_component_names(w: &World, components: &[u32]) -> Vec<String> {
        let mut comp_names = Vec::new();

        for &component in components {
            let Some(spell_component) = spell_formula::spell_components_table(w)
                .components
                .get(&component)
            else {
                log::info!("Spell.GetComponentNames(): Couldn't find SpellComponent {component}");
                continue;
            };

            comp_names.push(spell_component.name.clone());
        }
        comp_names
    }

    // ACE: Spell.SpellComponentDIDs
    /// The `DualDidMapper` of spell component ids to weenie class ids (a mutable static that
    /// nothing assigns).
    pub const SPELL_COMPONENT_DIDS: u32 = 0x2700_0002;

    /// The weenie class id of a spell component, or 0.
    // ACE: Spell.GetComponentWCID
    #[must_use]
    pub fn get_component_wcid(w: &World, comp_id: u32) -> u32 {
        // `ReadFromDat` answers an empty mapper for a missing file (here `None`, V11).
        let dual_dids = w
            .dats
            .portal_dat()
            .read_from_dat::<DualDidMapper>(Spell::SPELL_COMPONENT_DIDS);

        // ClientEnumToID is the mapper's first table.
        let wcid = dual_dids.as_ref().and_then(|m| {
            m.0.enum_to_id
                .iter()
                .find(|(k, _)| *k == comp_id)
                .map(|(_, v)| *v)
        });
        let Some(wcid) = wcid else {
            log::info!("GetComponentWCID({comp_id}): couldn't find component ID");
            return 0;
        };
        wcid
    }

    // ACE: Spell.GetMagicSkill
    #[must_use]
    pub fn get_magic_skill(&self) -> Skill {
        match self.school() {
            MagicSchool::CreatureEnchantment => Skill::CreatureEnchantment,
            MagicSchool::ItemEnchantment => Skill::ItemEnchantment,
            MagicSchool::LifeMagic => Skill::LifeMagic,
            MagicSchool::WarMagic => Skill::WarMagic,
            MagicSchool::VoidMagic => Skill::VoidMagic,
            _ => Skill::None,
        }
    }

    /// Returns TRUE if spell category matches impen / bane / brittlemail / lure.
    // ACE: Spell.IsImpenBaneType
    #[must_use]
    pub fn is_impen_bane_type(&self) -> bool {
        match self.category() {
            n if n >= SpellCategory::ArmorValueRaising
                && n <= SpellCategory::AcidicResistanceLowering =>
            {
                true
            }
            SpellCategory::ArmorValueRaisingRare
            | SpellCategory::AcidResistanceRaisingRare
            | SpellCategory::BludgeonResistanceRaisingRare
            | SpellCategory::ColdResistanceRaisingRare
            | SpellCategory::ElectricResistanceRaisingRare
            | SpellCategory::FireResistanceRaisingRare
            | SpellCategory::PierceResistanceRaisingRare
            | SpellCategory::SlashResistanceRaisingRare => true,
            _ => false,
        }
    }

    /// Returns TRUE if spell category matches spells that should redirect to items player is
    /// holding.
    // ACE: Spell.IsItemRedirectableType
    #[must_use]
    pub fn is_item_redirectable_type(&self) -> bool {
        matches!(
            self.category(),
            SpellCategory::DamageRaisingRare
                | SpellCategory::AttackModRaisingRare
                | SpellCategory::DefenseModRaisingRare
                | SpellCategory::WeaponTimeRaisingRare
                | SpellCategory::AppraisalResistanceLoweringRare
                | SpellCategory::MaxDamageRaising
        )
    }

    // ACE: Spell.IsNegativeRedirectable
    #[must_use]
    pub fn is_negative_redirectable(&self) -> bool {
        self.is_harmful() && (self.is_impen_bane_type() || self.is_other_negative_redirectable())
    }

    // ACE: Spell.IsOtherNegativeRedirectable
    #[must_use]
    pub fn is_other_negative_redirectable(&self) -> bool {
        matches!(
            self.category(),
            SpellCategory::DamageLowering            // encompasses both blood and spirit loather, inconsistent with spirit drinker in dat
                | SpellCategory::DefenseModLowering
                | SpellCategory::AttackModLowering
                | SpellCategory::WeaponTimeLowering  // verified
                | SpellCategory::ManaConversionModLowering // hermetic void, replaced hide value, unchanged category in dat
        )
    }

    // ACE: Spell.IsPortalSpell
    #[must_use]
    pub fn is_portal_spell(&self) -> bool {
        matches!(
            self.meta_spell_type(),
            SpellType::PortalLink
                | SpellType::PortalRecall
                | SpellType::PortalSending
                | SpellType::PortalSummon
                | SpellType::FellowPortalSending
        )
    }

    /// Handles forward compatibility for old item spells which should be auras.
    // ACE: Spell.HasItemCategory
    #[must_use]
    pub fn has_item_category(&self) -> bool {
        matches!(
            self.category(),
            SpellCategory::AttackModRaising
                | SpellCategory::DamageRaising
                | SpellCategory::DefenseModRaising
                | SpellCategory::WeaponTimeRaising // verified
                | SpellCategory::ManaConversionModRaising
                | SpellCategory::SpellDamageRaising
        )
    }

    /// Returns TRUE for any spells which could potentially affect the run rate, such as spells
    /// which alter run / quickness / strength.
    // ACE: Spell.UpdatesRunRate
    #[must_use]
    pub fn updates_run_rate(&self) -> bool {
        if self.spell.is_none() {
            return false;
        }

        // this is commented out as below in UpdatesMaxVitals
        // i forget the exact reasoning, are all the proper hooks in places for each vitae %,
        // and not just add/remove?
        /*if (_spell.Id == 666)   // vitae
        return true;*/

        let stat_mod_type = self.stat_mod_type();
        if (stat_mod_type & EnchantmentTypeFlags::Attribute) == EnchantmentTypeFlags::Attribute
            && (self.stat_mod_key() == u32::from(PropertyAttribute::Strength.0)
                || self.stat_mod_key() == u32::from(PropertyAttribute::Quickness.0))
        {
            return true;
        }

        if (stat_mod_type & EnchantmentTypeFlags::Skill) == EnchantmentTypeFlags::Skill
            && self.stat_mod_key() == Skill::Run.0.cast_unsigned()
        {
            return true;
        }

        false
    }

    /// Returns a list of MaxVitals affected by this spell.
    // ACE: Spell.UpdatesMaxVitals
    #[must_use]
    pub fn updates_max_vitals(&self) -> Vec<PropertyAttribute2nd> {
        let mut max_vitals = Vec::new();

        if self.spell.is_none() {
            return max_vitals;
        }

        let stat_mod_type = self.stat_mod_type();
        #[allow(clippy::cast_possible_truncation)]
        // C#'s `(PropertyAttribute2nd)uint`: the enum is `ushort`
        if (stat_mod_type & EnchantmentTypeFlags::SecondAtt) == EnchantmentTypeFlags::SecondAtt
            && self.stat_mod_key() != 0
        {
            max_vitals.push(PropertyAttribute2nd(self.stat_mod_key() as u16));
        } else if (stat_mod_type & EnchantmentTypeFlags::Attribute)
            == EnchantmentTypeFlags::Attribute
        {
            match PropertyAttribute(self.stat_mod_key() as u16) {
                PropertyAttribute::Endurance => {
                    max_vitals.push(PropertyAttribute2nd::MaxHealth);
                    max_vitals.push(PropertyAttribute2nd::MaxStamina);
                }
                PropertyAttribute::Self_ => {
                    max_vitals.push(PropertyAttribute2nd::MaxMana);
                }
                _ => {}
            }
        }
        max_vitals
    }

    /// Returns TRUE if this spell is a DamageOverTime or HealingOverTime spell.
    // ACE: Spell.IsDamageOverTime
    #[must_use]
    pub fn is_damage_over_time(&self) -> bool {
        if (self.flags() & SpellFlags::DamageOverTime) == SpellFlags::DamageOverTime {
            return true;
        }

        match self.category() {
            SpellCategory::HealOverTimeRaising
            | SpellCategory::DamageOverTimeRaising
            | SpellCategory::AetheriaProcHealthOverTimeRaising
            | SpellCategory::AetheriaProcDamageOverTimeRaising
            | SpellCategory::NetherDamageOverTimeRaising
            | SpellCategory::NetherDamageOverTimeRaising2
            | SpellCategory::NetherDamageOverTimeRaising3 => return true,
            _ => {}
        }

        // `(PropertyInt)StatModKey`: PropertyInt is a `ushort` enum, so the key truncates.
        #[allow(clippy::cast_possible_truncation)]
        match PropertyInt(self.stat_mod_key() as u16) {
            PropertyInt::HealOverTime | PropertyInt::DamageOverTime => return true,
            _ => {}
        }

        false
    }

    // ACE: Spell.HasExtraTick
    #[must_use]
    pub fn has_extra_tick(&self) -> bool {
        self.is_damage_over_time()
    }

    /// `IEquatable<Spell>.Equals`: same id.
    // ACE: Spell.Equals
    #[must_use]
    pub fn equals(&self, spell: Option<&Spell>) -> bool {
        spell.is_some_and(|s| self.id() == s.id())
    }

    /// `Id.GetHashCode()`: a `uint`'s hash is its bits as an `int`.
    // ACE: Spell.GetHashCode
    #[must_use]
    pub fn get_hash_code(&self) -> i32 {
        self.id().cast_signed()
    }
}

/// `Spell.ToString()`: the spell's name.
// ACE: Spell.ToString
impl fmt::Display for Spell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
