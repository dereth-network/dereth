// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellFormula.cs
//! Port of `Source/ACE.Server/Entity/SpellFormula.cs`.
//!
//! The scarab statics and the `SpellFormula` instance (a spell's component list, the player and
//! foci formulas, the windup and cast gestures).
//!
//! **Shape.** ACE's `SpellFormula.Spell` is a back-reference to the owning `Spell`, which holds
//! the formula in its `Formula` field. Rust has no cycle here: the formula keeps copies of the
//! three spell members it reads (`Id`, `Flags`, `School`), all immutable dat values
//! (`DIVERGENCES.md`, arch). The static `SpellTable` / `SpellComponentsTable` properties take the
//! world, whose `dats` is ACE's `DatManager.PortalDat`.

use std::collections::HashMap;
use std::sync::LazyLock;

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_dat::file_types::spell_table::{self as dat_spell_table, component_type};
use empyrean_dat::file_types::{MotionTable, SpellComponentsTable, SpellTable};
use empyrean_entity::enums::{MagicSchool, MotionCommand, MotionStance, SpellFlags};
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::physics::motion_table;
use crate::World;

/// ACE enum `Scarab` (declared in ACE.Server), underlying `int`: the scarab spell components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Scarab(pub i32);

#[allow(non_upper_case_globals)]
impl Scarab {
    pub const Lead: Self = Self(1);
    pub const Iron: Self = Self(2);
    pub const Copper: Self = Self(3);
    pub const Silver: Self = Self(4);
    pub const Gold: Self = Self(5);
    pub const Pyreal: Self = Self(6);
    pub const Diamond: Self = Self(110);
    pub const Platinum: Self = Self(112);
    pub const Dark: Self = Self(192);
    pub const Mana: Self = Self(193);

    /// Every declared member, in `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[
        Self::Lead,
        Self::Iron,
        Self::Copper,
        Self::Silver,
        Self::Gold,
        Self::Pyreal,
        Self::Diamond,
        Self::Platinum,
        Self::Dark,
        Self::Mana,
    ];
}

// ACE: SpellFormula.ScarabLevel
/// A mapping of scarabs => their spell levels. If the first component in a spell is a scarab, the
/// client uses this to determine the spell level, for things like the spellbook filters.
pub static SCARAB_LEVEL: LazyLock<HashMap<Scarab, u32>> = LazyLock::new(|| {
    HashMap::from([
        (Scarab::Lead, 1),
        (Scarab::Iron, 2),
        (Scarab::Copper, 3),
        (Scarab::Silver, 4),
        (Scarab::Gold, 5),
        (Scarab::Pyreal, 6),
        (Scarab::Diamond, 6),
        (Scarab::Platinum, 7),
        (Scarab::Dark, 7),
        (Scarab::Mana, 8),
    ])
});

// ACE: SpellFormula.ScarabPower
/// A mapping of scarabs => their power levels.
pub static SCARAB_POWER: LazyLock<HashMap<Scarab, u32>> = LazyLock::new(|| {
    HashMap::from([
        (Scarab::Lead, 1),
        (Scarab::Iron, 2),
        (Scarab::Copper, 3),
        (Scarab::Silver, 4),
        (Scarab::Gold, 5),
        (Scarab::Pyreal, 6),
        (Scarab::Diamond, 7),
        (Scarab::Platinum, 8),
        (Scarab::Dark, 9),
        (Scarab::Mana, 10),
    ])
});

// ACE: SpellFormula.GetLevel
/// Returns the spell level for a scarab.
///
/// # Panics
/// For a value outside the enum, as ACE's dictionary indexer throws `KeyNotFoundException`.
#[must_use]
pub fn get_level(scarab: Scarab) -> u32 {
    *SCARAB_LEVEL
        .get(&scarab)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SpellFormula.ScarabLevel[{scarab:?}]"))
}

// ACE: SpellFormula.GetPower
/// Returns the power level for a scarab.
///
/// # Panics
/// For a value outside the enum, as ACE's dictionary indexer throws `KeyNotFoundException`.
#[must_use]
pub fn get_power(scarab: Scarab) -> u32 {
    *SCARAB_POWER
        .get(&scarab)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SpellFormula.ScarabPower[{scarab:?}]"))
}

// ACE: SpellFormula.MaxSpellLevel
/// The maximum spell level in retail (a mutable static that nothing assigns).
pub const MAX_SPELL_LEVEL: u32 = 8;

// ACE: SpellFormula.MinPower
/// A mapping of spell levels => minimum power (`MinPower[level]`; levels 1 to 8).
///
/// # Panics
/// For a level outside 1..=8, as ACE's dictionary indexer throws `KeyNotFoundException`.
#[must_use]
pub fn min_power(spell_level: u32) -> u32 {
    match spell_level {
        1 => 1,
        2 => 50,
        3 => 100,
        4 => 150,
        5 => 200,
        6 => 250,
        7 => 300,
        8 => 400,
        _ => panic!("KeyNotFoundException: SpellFormula.MinPower[{spell_level}]"),
    }
}

// ACE: SpellFormula.IsScarab
/// Returns TRUE if this spell component (an ID from the spell components table) is a scarab:
/// `Enum.IsDefined(typeof(Scarab), (int)componentID)`.
#[must_use]
pub fn is_scarab(component_id: u32) -> bool {
    let value: i32 = component_id.cs_cast();
    Scarab::ALL.contains(&Scarab(value))
}

// ACE: SpellFormula.ScarabScale
/// A mapping of scarabs => PlayScript scales. This determines the scale of the glowing blue/purple
/// ball of energy during the windup motion.
pub static SCARAB_SCALE: LazyLock<HashMap<Scarab, f32>> = LazyLock::new(|| {
    HashMap::from([
        (Scarab::Lead, 0.05),
        (Scarab::Iron, 0.2),
        (Scarab::Copper, 0.4),
        (Scarab::Silver, 0.5),
        (Scarab::Gold, 0.6),
        (Scarab::Pyreal, 1.0),
        (Scarab::Diamond, 1.0), // verify onward
        (Scarab::Platinum, 1.0),
        (Scarab::Dark, 1.0),
        (Scarab::Mana, 1.0),
    ])
});

/// `(Scarab)componentID`: the unchecked enum cast.
fn as_scarab(component_id: u32) -> Scarab {
    Scarab(component_id.cs_cast())
}

// ACE: SpellFormula.SpellTable
/// The spell table from the portal.dat.
#[must_use]
pub fn spell_table(w: &World) -> &SpellTable {
    w.dats.portal_dat().spell_table()
}

// ACE: SpellFormula.SpellComponentsTable
/// The spell components table from the portal.dat.
#[must_use]
pub fn spell_components_table(w: &World) -> &SpellComponentsTable {
    w.dats.portal_dat().spell_components_table()
}

/// The components required to cast a spell.
// ACE: SpellFormula
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpellFormula {
    /// `Spell.Id` of the spell for this formula (ACE keeps the `Spell` itself).
    pub spell_id: u32,
    /// `Spell.Flags` of the spell for this formula.
    pub spell_flags: SpellFlags,
    /// `Spell.School` of the spell for this formula.
    pub spell_school: MagicSchool,
    /// The spell component IDs from the spell components table in portal.dat (0x0E00000F).
    // ACE: SpellFormula.Components
    pub components: Vec<u32>,
    /// The spell components for the individual player; uses a hashing algorithm based on the
    /// account name. `None` for ACE's `null` (not built yet).
    // ACE: SpellFormula.PlayerFormula
    pub player_formula: Option<Vec<u32>>,
    /// The scarab + prismatic taper formula; applies if the player has a foci for the current
    /// magic school.
    // ACE: SpellFormula.FociFormula
    pub foci_formula: Option<Vec<u32>>,
    /// The current spell formula for the player: either `PlayerFormula` or `FociFormula`.
    // ACE: SpellFormula.CurrentFormula
    pub current_formula: Option<Vec<u32>>,
}

impl SpellFormula {
    /// Constructs a SpellFormula from a list of components. `spell` is the spell for this formula;
    /// `components` is the list of components required to cast it.
    // ACE: SpellFormula.SpellFormula
    #[must_use]
    pub fn new(spell: &Spell, components: Vec<u32>) -> SpellFormula {
        SpellFormula {
            spell_id: spell.id(),
            spell_flags: spell.flags(),
            spell_school: spell.school(),
            components,
            player_formula: None,
            foci_formula: None,
            current_formula: None,
        }
    }

    /// Returns a list of scarabs in the spell formula.
    // ACE: SpellFormula.Scarabs
    #[must_use]
    pub fn scarabs(&self) -> Vec<Scarab> {
        let mut scarabs = Vec::new();

        for &component in &self.components {
            if is_scarab(component) {
                scarabs.push(as_scarab(component));
            }
        }

        scarabs
    }

    /// Uses the client spell level formula, which is used for things like spell filtering: a
    /// 'rough heuristic' based on the first component of the spell, which is expected to be a
    /// scarab.
    // ACE: SpellFormula.Level
    #[must_use]
    pub fn level(&self) -> u32 {
        let Some(&first_comp) = self.components.first() else {
            return 0;
        };
        if !is_scarab(first_comp) {
            return 0;
        }

        get_level(as_scarab(first_comp))
    }

    /// Power is used to determine, among possibly other things, the number of Prismatic Tapers in
    /// a "Scarab Only Formula" (foci).
    // ACE: SpellFormula.Power
    #[must_use]
    pub fn power(&self) -> u32 {
        let Some(&first_comp) = self.components.first() else {
            return 0;
        };
        if !is_scarab(first_comp) {
            return 0;
        }

        get_power(as_scarab(first_comp))
    }

    /// Builds the pseudo-randomized spell formula based on account name.
    ///
    /// # Panics
    /// Where ACE throws: the player has no session (`NullReferenceException`), or
    /// `SpellTable.GetSpellFormula` throws for this spell.
    // ACE: SpellFormula.GetPlayerFormula
    pub fn get_player_formula(&mut self, w: &World, player: ObjectGuid) -> Vec<u32> {
        let session = crate::managers::player_manager::player_session(w, player)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        let account = w
            .sessions
            .get(session)
            .and_then(|s| s.account.clone())
            .unwrap_or_default();

        let player_formula =
            dat_spell_table::get_spell_formula(spell_table(w), self.spell_id, &account)
                .unwrap_or_else(|e| {
                    panic!(
                        "ACE: SpellTable.GetSpellFormula({}) threw {e}",
                        self.spell_id
                    )
                });
        self.player_formula = Some(player_formula);
        self.foci_formula = Some(self.get_foci_formula());

        self.get_current_formula(w, player);

        self.player_formula.clone().unwrap_or_default()
    }

    /// For monsters with PropertyBool.AiUseHumanMagicAnimations.
    ///
    /// # Panics
    /// Where `SpellTable.GetSpellFormula` throws for this spell.
    // ACE: SpellFormula.GetMonsterFormula
    pub fn get_monster_formula(&mut self, w: &World) -> Vec<u32> {
        let formula = dat_spell_table::get_spell_formula(spell_table(w), self.spell_id, "")
            .unwrap_or_else(|e| {
                panic!(
                    "ACE: SpellTable.GetSpellFormula({}) threw {e}",
                    self.spell_id
                )
            });
        self.player_formula = Some(formula.clone());
        formula
    }

    /// Returns the windup gesture from all the scarabs.
    // ACE: SpellFormula.WindupGestures
    #[must_use]
    pub fn windup_gestures(&self, w: &World) -> Vec<MotionCommand> {
        let mut windup_gestures = Vec::new();

        for scarab in self.scarabs() {
            let component = spell_components_table(w)
                .components
                .get(&scarab.0.cast_unsigned());
            let Some(component) = component else {
                log::info!(
                    "SpellFormula.WindupGestures error: spell ID {} contains scarab {:?} not found in components table, skipping",
                    self.spell_id,
                    scarab
                );
                continue;
            };
            windup_gestures.push(MotionCommand(component.gesture));
        }
        windup_gestures
    }

    // ACE: SpellFormula.HasWindupGestures
    #[must_use]
    pub fn has_windup_gestures(&self) -> bool {
        self.scarabs().iter().any(|&i| i != Scarab::Lead)
    }

    /// Returns the spell casting gesture, after the initial windup(s) are completed. Based on the
    /// talisman (assumed to be the last spell component).
    // ACE: SpellFormula.CastGesture
    #[must_use]
    pub fn cast_gesture(&self, w: &World) -> MotionCommand {
        let Some(player_formula) = self.player_formula.as_ref() else {
            return MotionCommand::Invalid;
        };
        let Some(&last) = player_formula.last() else {
            return MotionCommand::Invalid;
        };

        // ensure talisman
        let talisman = spell_components_table(w).components.get(&last);
        match talisman {
            Some(talisman) if talisman.component_type == component_type::TALISMAN => {
                MotionCommand(talisman.gesture)
            }
            _ => {
                log::info!(
                    "SpellFormula.CastGesture error: spell ID {} last component not talisman!",
                    self.spell_id
                );
                MotionCommand::Invalid
            }
        }
    }

    /// `Scarabs.First()`.
    ///
    /// # Panics
    /// When the formula has no scarab (`InvalidOperationException`).
    // ACE: SpellFormula.FirstScarab
    #[must_use]
    pub fn first_scarab(&self) -> Scarab {
        *self
            .scarabs()
            .first()
            .expect("ACE: SpellFormula.FirstScarab: Sequence contains no elements (InvalidOperationException)")
    }

    /// Returns a simple scale for the spell formula, based on the first scarab.
    ///
    /// # Panics
    /// When the formula has no scarab (`InvalidOperationException`).
    // ACE: SpellFormula.Scale
    #[must_use]
    pub fn scale(&self) -> f32 {
        let first = self.first_scarab();
        *SCARAB_SCALE
            .get(&first)
            .unwrap_or_else(|| panic!("KeyNotFoundException: SpellFormula.ScarabScale[{first:?}]"))
    }

    /// Returns the total casting time, based on windup + cast gestures. `weapon_cast_gesture`
    /// defaults to `null`.
    ///
    /// # Panics
    /// When the formula has no scarab: ACE's `WindupGestures.First()` throws
    /// `InvalidOperationException` before anything else.
    // ACE: SpellFormula.GetCastTime
    #[must_use]
    pub fn get_cast_time(
        &self,
        w: &World,
        motion_table_id: u32,
        speed: f32,
        weapon_cast_gesture: Option<MotionCommand>,
    ) -> f32 {
        let _windup_motion = *self
            .windup_gestures(w)
            .first()
            .expect("ACE: SpellFormula.GetCastTime: Sequence contains no elements (InvalidOperationException)");
        let cast_motion = weapon_cast_gesture.unwrap_or_else(|| self.cast_gesture(w));

        let motion_table = w
            .dats
            .portal_dat()
            .read_from_dat::<MotionTable>(motion_table_id);
        let mt = motion_table.as_deref();

        let mut windup_time = 0.0f32;
        //var windupTime = motionTable.GetAnimationLength(MotionStance.Magic, windupMotion) / speed;
        for motion in self.windup_gestures(w) {
            windup_time +=
                motion_table::get_animation_length_in(w, mt, MotionStance::Magic, motion, None)
                    / speed;
        }

        let cast_time =
            motion_table::get_animation_length_in(w, mt, MotionStance::Magic, cast_motion, None)
                / speed;

        // FastCast = no windup motion
        if (self.spell_flags & SpellFlags::FastCast) == SpellFlags::FastCast
            || weapon_cast_gesture.is_some()
        {
            return cast_time;
        }

        windup_time + cast_time
    }

    /// The scarab + prismatic taper formula (sets and returns `FociFormula`).
    // ACE: SpellFormula.GetFociFormula
    ///
    /// **Retail's rule, not ACE's (V380).** ACE took the number of Prismatic
    /// Tapers from the power of the formula's *first* component (none when that is not a scarab);
    /// the client takes it from the *most powerful* power component, and computes the requirement
    /// it shows the caster that way. The formula is now the one shared implementation,
    /// `dereth_rules::magic::scarab_only_formula`, over the client's eight slots (no dat formula has
    /// more), so the requirement the client shows is the one the server consumes.
    pub fn get_foci_formula(&mut self) -> Vec<u32> {
        let mut comps = [0u32; 8];
        for (slot, &c) in comps.iter_mut().zip(&self.components) {
            *slot = c;
        }
        let foci_formula: Vec<u32> = dereth_rules::magic::scarab_only_formula(&comps)
            .into_iter()
            .take_while(|&c| c != 0)
            .collect();

        self.foci_formula = Some(foci_formula.clone());
        foci_formula
    }

    /// `CurrentFormula = player.HasFoci(Spell.School) ? FociFormula : PlayerFormula`.
    // ACE: SpellFormula.GetCurrentFormula
    pub fn get_current_formula(&mut self, w: &World, player: ObjectGuid) {
        self.current_formula = if crate::world_objects::world_object_magic::player_has_foci(
            w,
            player,
            self.spell_school,
        ) {
            self.foci_formula.clone()
        } else {
            self.player_formula.clone()
        };
    }

    /// Returns a mapping of component wcid => number required, in first-seen order.
    ///
    /// # Panics
    /// When `CurrentFormula` has not been built (`NullReferenceException`).
    // ACE: SpellFormula.GetRequiredComps
    #[must_use]
    pub fn get_required_comps(&self, w: &World) -> DotNetDict<u32, i32> {
        let mut comps_required = DotNetDict::new();

        let current = self
            .current_formula
            .as_ref()
            .expect("ACE: SpellFormula.CurrentFormula is null (NullReferenceException)");
        for &component in current {
            let wcid = Spell::get_component_wcid(w, component);

            if let Some(count) = comps_required.get_mut(&wcid) {
                *count += 1;
            } else {
                comps_required.insert(wcid, 1);
            }
        }
        comps_required
    }
}
