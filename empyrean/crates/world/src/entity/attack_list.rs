// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AttackList.cs
//! Port of `Source/ACE.Server/Entity/AttackList.cs`.

use empyrean_entity::ObjectGuid;

use crate::entity::attack_damage::AttackDamage;

// ACE: AttackList
/// Tracks top damager.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttackList {
    /// `Dictionary<WorldObject, uint>`, in insertion order (a null source is a key too).
    // ACE: AttackList.Damagers
    pub damagers: Vec<(Option<ObjectGuid>, u32)>,
}

impl AttackList {
    // ACE: AttackList.AttackList
    #[must_use]
    pub fn new() -> Self {
        let mut this = Self::default();
        this.init();
        this
    }

    // ACE: AttackList.AttackList
    #[must_use]
    pub fn from_attacks(attack_damages: &[AttackDamage]) -> Self {
        let mut this = Self::new();

        for attack_damage in attack_damages {
            this.add(attack_damage.source, attack_damage.amount);
        }
        this
    }

    // ACE: AttackList.Init
    pub fn init(&mut self) {
        self.damagers = Vec::new();
    }

    // ACE: AttackList.Add
    pub fn add(&mut self, damager: Option<ObjectGuid>, amount: u32) {
        // ACE-BUG: `Dictionary.Add(null, ...)` throws for a null source; the port keys it like any other
        match self.damagers.iter_mut().find(|(d, _)| *d == damager) {
            Some((_, v)) => *v = v.wrapping_add(amount),
            None => self.damagers.push((damager, amount)),
        }
    }

    // ACE: AttackList.TopDamager
    /// The damager with the most damage (the first of a tie: `OrderByDescending` is stable).
    #[must_use]
    pub fn top_damager(&self) -> Option<ObjectGuid> {
        let mut best: Option<(Option<ObjectGuid>, u32)> = None;
        for &(d, v) in &self.damagers {
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((d, v));
            }
        }
        best.and_then(|(d, _)| d)
    }

    // ACE: AttackList.OnHeal
    /// Called when an AttackTarget regains health: the amount of health restored and the amount
    /// that was missing before healing.
    ///
    /// # Panics
    /// With no missing health (ACE's `DivideByZeroException`).
    pub fn on_heal(&mut self, heal_amount: i32, missing_health: i32) {
        // on heal, scale the damage from each source by 1 - healAmount / missingHealth
        // ACE-BUG: `healAmount / missingHealth` is an integer division, and the scalar is rounded before it multiplies
        assert!(missing_health != 0, "ACE: DivideByZeroException");
        #[allow(clippy::cast_precision_loss)]
        let scalar = 1.0f32 - heal_amount.wrapping_div(missing_health) as f32;

        let factor: u32 = empyrean_common::dotnet::CsCast::cs_cast(
            empyrean_common::dotnet::math::round(f64::from(scalar)),
        );
        for (_, v) in &mut self.damagers {
            *v = v.wrapping_mul(factor);
        }
    }
}
