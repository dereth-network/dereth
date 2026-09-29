// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/AttackDamage.cs
//! Port of `Source/ACE.Server/Entity/AttackDamage.cs`.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: AttackDamage
/// An attackable objects keeps track of its damage sources.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackDamage {
    // ACE: AttackDamage.Source
    pub source: Option<ObjectGuid>,
    // ACE: AttackDamage.Amount
    pub amount: u32,
    // ACE: AttackDamage.Time
    pub time: DotNetDateTime,
    // ACE: AttackDamage.IsCritical
    pub is_critical: bool,
}

impl AttackDamage {
    // ACE: AttackDamage.AttackDamage
    /// Constructs a new attack damage: the attacker or source of damage, the amount of hit
    /// damage, and whether it was a critical hit (stamped now).
    #[must_use]
    pub fn new(w: &World, source: Option<ObjectGuid>, amount: u32, critical_hit: bool) -> Self {
        Self {
            source,
            amount,
            time: w.now.utc,
            is_critical: critical_hit,
        }
    }

    // ACE: AttackDamage.GetTotalDamage
    /// Returns the total damage from the source attacker.
    #[must_use]
    pub fn get_total_damage(attacks: &[AttackDamage], source: Option<ObjectGuid>) -> u64 {
        // `(ulong)attacks.Sum(a => a.Amount)`: a uint sum, which C# wraps unchecked
        u64::from(
            attacks
                .iter()
                .filter(|a| a.source == source)
                .fold(0u32, |sum, a| sum.wrapping_add(a.amount)),
        )
    }

    // ACE: AttackDamage.LastHitCritical
    /// Returns TRUE if last attack was critical hit.
    #[must_use]
    pub fn last_hit_critical(attacks: &[AttackDamage]) -> bool {
        attacks.last().is_some_and(|last_hit| last_hit.is_critical)
    }

    // ACE: AttackDamage.GetTopDamager
    /// Returns the top damager on creature death.
    #[must_use]
    pub fn get_top_damager(attacks: &[AttackDamage]) -> Option<ObjectGuid> {
        // build the attack list
        crate::entity::attack_list::AttackList::from_attacks(attacks).top_damager()
    }
}
