// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AddEnchantmentResult.cs
//! Port of `Source/ACE.Server/Entity/AddEnchantmentResult.cs`.
//!
//! ACE's result holds references to the registry's own entry objects. Here the lists and
//! [`AddEnchantmentResult::enchantment`] are copies taken when the result is built; ACE's callers
//! only read them right after `EnchantmentManager.Add` (to name the spells in the cast message and
//! to send `MagicUpdateEnchantment`). The one reference ACE writes through, `RefreshCaster`, is
//! also kept as its position in the `entries` given to [`AddEnchantmentResult::build_stack`], so
//! `Add` can update the live entry.

use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::entity::spell_set::set_spells;
use crate::world_objects::managers::enchantment_manager::StackType;
use crate::World;

/// The result of adding an enchantment, and where it fits into the current stack.
// ACE: AddEnchantmentResult
#[derive(Debug, Clone, Default)]
pub struct AddEnchantmentResult {
    /// The resulting enchantment that was added or refreshed (set in `EnchantmentManager.Add`).
    // ACE: AddEnchantmentResult.Enchantment
    pub enchantment: Option<PropertiesEnchantmentRegistry>,

    /// How this enchantment relates to the most powerful spell in this category, for the
    /// surpassing / refreshing / surpassed by message.
    // ACE: AddEnchantmentResult.StackType
    pub stack_type: StackType,

    /// The existing enchantments in this stack. ACE leaves them `null` until `BuildStack`; they
    /// are empty here.
    // ACE: AddEnchantmentResult.Surpass
    pub surpass: Vec<PropertiesEnchantmentRegistry>,
    // ACE: AddEnchantmentResult.Refresh
    pub refresh: Vec<PropertiesEnchantmentRegistry>,
    // ACE: AddEnchantmentResult.Surpassed
    pub surpassed: Vec<PropertiesEnchantmentRegistry>,

    /// The most powerful spells in this stack, for each stack type.
    // ACE: AddEnchantmentResult.SurpassSpell
    pub surpass_spell: Option<Spell>,
    // ACE: AddEnchantmentResult.RefreshSpell
    pub refresh_spell: Option<Spell>,
    // ACE: AddEnchantmentResult.SurpassedSpell
    pub surpassed_spell: Option<Spell>,

    /// This handles situations where the same spell can come from both a creature and item source.
    // ACE: AddEnchantmentResult.RefreshCaster
    pub refresh_caster: Option<PropertiesEnchantmentRegistry>,
    /// Not ACE: the position of [`Self::refresh_caster`] in the `entries` passed to
    /// [`Self::build_stack`] (ACE keeps the object reference itself).
    pub refresh_caster_entry: Option<usize>,

    // ACE: AddEnchantmentResult.TopLayerId
    pub top_layer_id: u16,
}

impl AddEnchantmentResult {
    // ACE: AddEnchantmentResult.NextLayerId
    #[must_use]
    pub fn next_layer_id(&self) -> u16 {
        self.top_layer_id.wrapping_add(1)
    }

    /// `new AddEnchantmentResult()`.
    // ACE: AddEnchantmentResult.AddEnchantmentResult
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `new AddEnchantmentResult(StackType stackType)`.
    #[must_use]
    pub fn with_stack_type(stack_type: StackType) -> Self {
        Self {
            stack_type,
            ..Self::default()
        }
    }

    /// Sorts the existing entries of the spell's category into surpass / refresh / surpassed,
    /// finds the top layer, and picks the spells and the refresh caster. `caster` is ACE's
    /// `WorldObject caster` (`None` for `null`); `equip` and `is_weapon_spell` default to false.
    ///
    /// # Panics
    /// When some entry is refreshed and `caster` is `None` (ACE: `NullReferenceException` in
    /// `SetRefreshCaster`).
    // ACE: AddEnchantmentResult.BuildStack
    #[allow(clippy::float_cmp)]
    pub fn build_stack(
        &mut self,
        w: &World,
        entries: &[PropertiesEnchantmentRegistry],
        spell: &Spell,
        caster: Option<ObjectGuid>,
        equip: bool,
        is_weapon_spell: bool,
    ) {
        self.surpass = Vec::new();
        self.refresh = Vec::new();
        self.surpassed = Vec::new();
        let mut refresh_entries = Vec::new();

        let power_level = spell.power();

        // `OrderByDescending(i => i.PowerLevel)` is a stable sort.
        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by(|&a, &b| entries[b].power_level.cmp(&entries[a].power_level));

        for i in order {
            let entry = &entries[i];
            if power_level > entry.power_level {
                // surpassing existing spell
                self.surpass.push(entry.clone());
            } else if power_level == entry.power_level {
                // refreshing existing spell
                if i64::from(spell.id()) == i64::from(entry.spell_id) {
                    self.refresh.push(entry.clone());
                    refresh_entries.push(i);
                } else {
                    // handle special case to prevent message: Pumpkin Shield casts Web of Defense on you, refreshing Aura of Defense
                    let mut spell_duration = if equip {
                        f64::INFINITY
                    } else {
                        spell.duration()
                    };

                    if !equip && !is_weapon_spell {
                        if let Some(aug) =
                            caster_player_augmentation_increased_spell_duration(w, caster)
                        {
                            spell_duration *= f64::from(1.0f32 + aug as f32 * 0.2f32);
                        }
                    }

                    let entry_duration = if entry.duration == -1.0 {
                        f64::INFINITY
                    } else {
                        entry.duration
                    };

                    if spell_duration > entry_duration
                        || spell_duration == entry_duration
                            && !set_spells(&w.dats).contains(&entry.spell_id)
                    {
                        self.surpass.push(entry.clone());
                    } else if spell_duration < entry_duration {
                        self.surpassed.push(entry.clone());
                    } else {
                        // fallback on spell id, for overlapping set spells in multiple sets, where the different 'level' names each have the same spellLevel and powerLevel?
                        // ie. for Gauntlet Damage Boost I and II
                        // this bug still exists in acclient visual enchantment display, unknown whether this bug existed on retail server
                        if i64::from(spell.id()) > i64::from(entry.spell_id) {
                            self.surpass.push(entry.clone());
                        } else {
                            self.surpassed.push(entry.clone());
                        }
                    }
                }
            } else if power_level < entry.power_level {
                // surpassed by existing spell
                self.surpassed.push(entry.clone());
            }

            if entry.layer_id > self.top_layer_id {
                self.top_layer_id = entry.layer_id;
            }
        }

        self.set_stack_type();
        self.set_spell(w);

        if !self.refresh.is_empty() {
            let caster = caster.expect("ACE: AddEnchantmentResult.SetRefreshCaster: caster is null (NullReferenceException)");
            self.set_refresh_caster(caster);
            self.refresh_caster_entry = self
                .refresh
                .iter()
                .zip(&refresh_entries)
                .rfind(|(r, _)| r.caster_object_id == caster.full())
                .map(|(_, &i)| i);
        }
    }

    // ACE: AddEnchantmentResult.SetStackType
    pub fn set_stack_type(&mut self) {
        if !self.surpassed.is_empty() {
            self.stack_type = StackType::Surpassed;
        } else if !self.refresh.is_empty() {
            self.stack_type = StackType::Refresh;
        } else if !self.surpass.is_empty() {
            self.stack_type = StackType::Surpass;
        }
    }

    // ACE: AddEnchantmentResult.SetSpell
    pub fn set_spell(&mut self, w: &World) {
        if let Some(first) = self.surpass.first() {
            self.surpass_spell = Some(Spell::from_int(w, first.spell_id, false));
        }

        if let Some(first) = self.refresh.first() {
            self.refresh_spell = Some(Spell::from_int(w, first.spell_id, false));
        }

        if let Some(first) = self.surpassed.first() {
            self.surpassed_spell = Some(Spell::from_int(w, first.spell_id, false));
        }
    }

    /// The same spells from different casters are written to separate layers; the last refresh
    /// entry from `caster` wins.
    // ACE: AddEnchantmentResult.SetRefreshCaster
    pub fn set_refresh_caster(&mut self, caster: ObjectGuid) {
        // the same spells from different casters should definitely be written to separate layers,
        // but it's questionable if retail sent 'refreshing' or 'surpassing' here
        for refresh in &self.refresh {
            if refresh.caster_object_id == caster.full() {
                self.refresh_caster = Some(refresh.clone());
            }
        }
    }
}

/// `caster is Player player && player.AugmentationIncreasedSpellDuration > 0`: the augmentation
/// when both hold.
pub(crate) fn caster_player_augmentation_increased_spell_duration(
    w: &World,
    caster: Option<ObjectGuid>,
) -> Option<i32> {
    let o = w.objects.get(caster?)?;
    if !o.is_player() {
        return None;
    }
    let aug = o.augmentation_increased_spell_duration();
    (aug > 0).then_some(aug)
}
