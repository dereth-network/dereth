// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/BodyPartTable.cs
//! Port of `Source/ACE.Server/Entity/BodyPartTable.cs`.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{CombatBodyPart, Quadrant};
use empyrean_entity::models::Weenie;

use crate::entity::body_part_probability::BodyPartProbability;

/// A creature weenie's body parts by hit quadrant (12 quadrants: high/medium/low, left/right,
/// front/back), each with its hit weight.
// ACE: BodyPartTable
#[derive(Debug, Clone, Default)]
pub struct BodyPartTable {
    /// `Weenie.WeenieClassId` (ACE keeps the weenie itself).
    // ACE: BodyPartTable.Weenie
    pub weenie_class_id: u32,

    // ACE: BodyPartTable.Quadrants
    pub quadrants: [Vec<BodyPartProbability>; 12],
}

impl BodyPartTable {
    // ACE: BodyPartTable.BodyPartTable
    #[must_use]
    pub fn new(weenie: &Weenie) -> Self {
        let mut this = Self {
            weenie_class_id: weenie.weenie_class_id,
            quadrants: Default::default(),
        };

        let Some(parts) = weenie.properties_body_part.as_ref() else {
            log::error!(
                "BodyPartTable is null for {} - {}!",
                weenie.weenie_class_id,
                weenie.class_name.as_deref().unwrap_or_default()
            );
            return this;
        };

        for (&key, body_part) in parts.iter() {
            let q = &mut this.quadrants;
            let mut add = |i: usize, p: f32| {
                if p > 0.0 {
                    q[i].push(BodyPartProbability::new(key, p));
                }
            };

            add(0, body_part.hlf);
            add(1, body_part.mlf);
            add(2, body_part.llf);

            add(3, body_part.hrf);
            add(4, body_part.mrf);
            add(5, body_part.lrf);

            add(6, body_part.hlb);
            add(7, body_part.mlb);
            add(8, body_part.llb);

            add(9, body_part.hrb);
            add(10, body_part.mrb);
            add(11, body_part.lrb);
        }

        this
    }

    // ACE: BodyPartTable.RollBodyPart
    /// A weighted random body part of the quadrant (`Undefined` when it has none).
    ///
    /// # Panics
    /// For a quadrant index outside the table (ACE: `IndexOutOfRangeException`).
    #[must_use]
    pub fn roll_body_part(&self, quadrant: Quadrant) -> CombatBodyPart {
        let idx = usize::try_from(quadrant.get_index().0).expect("System.IndexOutOfRangeException");

        let body_parts = &self.quadrants[idx];

        // `bodyParts.Sum(i => i.Probability)`: LINQ's float Sum accumulates in double
        let total = body_parts
            .iter()
            .fold(0.0f64, |acc, i| acc + f64::from(i.probability)) as f32;

        if total == 0.0 {
            return CombatBodyPart::Undefined;
        }

        let rng = ThreadSafeRandom::next_float(0.0, total);

        let mut total_probability = 0.0f32;
        for body_part in body_parts {
            total_probability += body_part.probability;
            if rng < f64::from(total_probability) {
                return body_part.body_part;
            }
        }
        CombatBodyPart::Undefined
    }
}
