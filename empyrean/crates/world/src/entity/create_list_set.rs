// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CreateListSet.cs
//! Port of `Source/ACE.Server/Entity/CreateListSet.cs`.

use empyrean_entity::models::PropertiesCreateList;

/// LINQ's `Enumerable.Sum(IEnumerable<float>)`: .NET accumulates in `double` and narrows once.
fn sum_shade<'a>(items: impl Iterator<Item = &'a PropertiesCreateList>) -> f32 {
    let mut sum = 0.0f64;
    for i in items {
        sum += f64::from(i.shade);
    }
    #[allow(clippy::cast_possible_truncation)]
    let sum = sum as f32;
    sum
}

// ACE: CreateListSet
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateListSet {
    pub items: Vec<PropertiesCreateList>,
}

impl CreateListSet {
    // ACE: CreateListSet.CreateListSet
    #[must_use]
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    // ACE: CreateListSet.Trophies
    #[must_use]
    pub fn trophies(&self) -> Vec<PropertiesCreateList> {
        self.items
            .iter()
            .filter(|i| i.weenie_class_id != 0)
            .cloned()
            .collect()
    }

    // ACE: CreateListSet.None
    #[must_use]
    pub fn none(&self) -> Vec<PropertiesCreateList> {
        self.items
            .iter()
            .filter(|i| i.weenie_class_id == 0)
            .cloned()
            .collect()
    }

    // ACE: CreateListSet.TotalProbability
    #[must_use]
    pub fn total_probability(&self) -> f32 {
        sum_shade(self.items.iter())
    }

    // ACE: CreateListSet.TrophyProbability
    #[must_use]
    pub fn trophy_probability(&self) -> f32 {
        sum_shade(self.items.iter().filter(|i| i.weenie_class_id != 0))
    }

    // ACE: CreateListSet.NoneProbability
    #[must_use]
    pub fn none_probability(&self) -> f32 {
        sum_shade(self.items.iter().filter(|i| i.weenie_class_id == 0))
    }

    // ACE: CreateListSet.Add
    pub fn add(&mut self, item: PropertiesCreateList) {
        self.items.push(item);
    }
}
