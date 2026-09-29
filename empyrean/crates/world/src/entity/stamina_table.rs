// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/StaminaTable.cs
//! Port of `Source/ACE.Server/Entity/StaminaTable.cs`.
//!
//! ACE's static `StaminaTable.Costs` is built once by the static constructor; here it is a
//! [`LazyLock`] over [`build_table`]. Only lookups are made, so its iteration order is not
//! observable and a `HashMap` stands in for the `Dictionary`.

use std::collections::HashMap;
use std::sync::LazyLock;

use empyrean_entity::enums::PowerAccuracy;

/// One step of an attack's stamina cost: every `burden` units cost `stamina`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaminaCost {
    pub burden: i32,
    pub stamina: f32,
}

impl StaminaCost {
    // ACE: StaminaCost.StaminaCost
    #[must_use]
    pub fn new(burden: i32, stamina: f32) -> Self {
        Self { burden, stamina }
    }
}

// ACE: StaminaTable.StaminaTable
/// `StaminaTable.Costs`, built by the static constructor.
pub static COSTS: LazyLock<HashMap<PowerAccuracy, Vec<StaminaCost>>> = LazyLock::new(build_table);

// ACE: StaminaTable.BuildTable
/// The cost steps per power/accuracy bar. Each list must be in descending order of burden.
#[must_use]
pub fn build_table() -> HashMap<PowerAccuracy, Vec<StaminaCost>> {
    let mut costs = HashMap::new();

    // must be in descending order
    let low_costs = vec![
        StaminaCost::new(1600, 1.5),
        StaminaCost::new(1200, 1.0),
        StaminaCost::new(700, 1.0),
    ];

    let mid_costs = vec![
        StaminaCost::new(1600, 3.0),
        StaminaCost::new(1200, 2.0),
        StaminaCost::new(700, 1.0),
    ];

    let high_costs = vec![
        StaminaCost::new(1600, 6.0),
        StaminaCost::new(1200, 4.0),
        StaminaCost::new(700, 2.0),
    ];

    costs.insert(PowerAccuracy::Low, low_costs);
    costs.insert(PowerAccuracy::Medium, mid_costs);
    costs.insert(PowerAccuracy::High, high_costs);
    costs
}

// ACE: StaminaTable.GetStaminaCost
/// The stamina an attack costs at `power_accuracy` with `burden`.
///
/// # Panics
/// For a `power_accuracy` outside `Low`/`Medium`/`High`, as ACE's `Costs[powerAccuracy]` throws
/// `KeyNotFoundException`.
#[must_use]
pub fn get_stamina_cost(power_accuracy: PowerAccuracy, burden: i32) -> f32 {
    let mut burden = burden;
    let mut base_cost = 0.0f32;
    let attack_costs = COSTS
        .get(&power_accuracy)
        .unwrap_or_else(|| panic!("KeyNotFoundException: StaminaTable.Costs[{power_accuracy:?}]"));
    for attack_cost in attack_costs {
        if burden >= attack_cost.burden {
            let num_times = burden / attack_cost.burden;
            base_cost += attack_cost.stamina * num_times as f32;
            burden -= attack_cost.burden * num_times;
        }
    }
    base_cost
}
