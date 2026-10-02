// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Entity/ChanceTable.cs, Source/ACE.Server/Factories/Entity/GemResult.cs

//! The two `Factories/Entity` types the generated tables are made of. They live here, not in
//! empyrean-world, because a `static` table needs its element type in the same crate or below.

use std::borrow::Cow;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use empyrean_common::dotnet::decimal::Decimal;
use empyrean_entity::enums::{MaterialType, SpellId};

use crate::enums::{
    TreasureArmorType, TreasureHeritageGroup, TreasureItemType, TreasureWeaponType, WeenieClassName,
};
use crate::rng;

/// ACE `ChanceTable<T>`: a `List<(T result, float chance)>` rolled against one uniform draw.
///
/// The generated tables are `static` and borrow their entries; tables ACE builds at runtime
/// (`CantripChance`'s rescaled tables) own theirs.
// ACE: ChanceTable
pub struct ChanceTable<T: Clone + 'static> {
    entries: Cow<'static, [(T, f32)]>,
    verified: AtomicBool,
    /// Not ACE: the chances are weights, divided by their sum when rolled (ClassicACE's
    /// `ChanceTableType.Weight`, the earlier eras' tables).
    weighted: bool,
}

impl<T: Clone + 'static> ChanceTable<T> {
    /// A table over literal entries (the generated tables).
    pub const fn new(entries: &'static [(T, f32)]) -> Self {
        Self {
            entries: Cow::Borrowed(entries),
            verified: AtomicBool::new(false),
            weighted: false,
        }
    }

    /// Not ACE: a table of weights (ClassicACE's `ChanceTableType.Weight`): each entry's chance is
    /// its weight over the sum of the weights, and the sum need not be 1.
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Entity/ChanceTable.cs
    pub const fn new_weighted(entries: &'static [(T, f32)]) -> Self {
        Self {
            entries: Cow::Borrowed(entries),
            verified: AtomicBool::new(false),
            weighted: true,
        }
    }

    /// A table built at runtime (`new ChanceTable<T>()` followed by `Add`s).
    pub fn from_vec(entries: Vec<(T, f32)>) -> Self {
        Self {
            entries: Cow::Owned(entries),
            verified: AtomicBool::new(false),
            weighted: false,
        }
    }

    /// Whether the chances are weights ([`Self::new_weighted`]).
    pub fn is_weighted(&self) -> bool {
        self.weighted
    }

    /// The `(result, chance)` entries in order.
    pub fn entries(&self) -> &[(T, f32)] {
        &self.entries
    }

    /// `List.Count`.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T: Clone + DotNetString + 'static> ChanceTable<T> {
    /// Logs an error if the chances do not add up to 1 within `0.0000001`, summing in `decimal`
    /// as ACE does. Runs once per table, on its first roll.
    // ACE: ChanceTable.VerifyTable
    fn verify_table(&self) {
        if self.weighted {
            // weights need not add up to anything
            self.verified.store(true, Ordering::Relaxed);
            return;
        }
        let mut total = Decimal::ZERO;

        for entry in self.entries.iter() {
            total = total + Decimal::from_f32(entry.1);
        }

        let threshold = Decimal::new(1, 7);
        if (Decimal::ONE - total).abs().gt(threshold) {
            let entries: Vec<String> = self
                .entries
                .iter()
                .map(|(result, chance)| format!("({}, {})", result.dotnet_string(), chance))
                .collect();
            log::error!(
                "Chance table adds up to {}, expected 1.0: {}",
                total.dotnet_string(),
                entries.join(", ")
            );
        }

        self.verified.store(true, Ordering::Relaxed);
    }

    /// Draws once in `[0, 1)` and returns the first entry whose running total exceeds the draw
    /// and is at least `quality_mod`; if none does, the last entry with a positive chance.
    /// ACE's `qualityMod` defaults to `0.0f`.
    // ACE: ChanceTable.Roll
    pub fn roll(&self, quality_mod: f32) -> T {
        if !self.verified.load(Ordering::Relaxed) {
            self.verify_table();
        }

        let mut total = 0.0f32;

        // ThreadSafeRandom.Next(float, float) returns a double; the comparison below is in double.
        let rng = rng::next_double(0.0, 1.0);

        // Not ACE: a weight table's running total is of each weight over their sum (ClassicACE).
        let total_weight = if self.weighted {
            self.entries.iter().map(|e| e.1).sum::<f32>()
        } else {
            1.0
        };

        for entry in self.entries.iter() {
            total += if self.weighted {
                entry.1 / total_weight
            } else {
                entry.1
            };

            if rng < f64::from(total) && total >= quality_mod {
                return entry.0.clone();
            }
        }

        // Enumerable.Last(predicate) throws when nothing matches.
        self.entries
            .iter()
            .rev()
            .find(|i| i.1 > 0.0)
            .expect("Sequence contains no matching element")
            .0
            .clone()
    }
}

impl<T: Clone + fmt::Debug + 'static> fmt::Debug for ChanceTable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.entries.iter()).finish()
    }
}

/// ACE `GemResult`: a gem's weenie class and material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GemResult {
    pub class_name: WeenieClassName,
    pub material_type: MaterialType,
}

impl GemResult {
    // ACE: GemResult.GemResult
    pub const fn new(class_name: WeenieClassName, material_type: MaterialType) -> Self {
        Self {
            class_name,
            material_type,
        }
    }
}

/// .NET `ToString()` of a chance-table result, for `VerifyTable`'s log line.
pub trait DotNetString {
    /// The value as .NET would print it.
    fn dotnet_string(&self) -> String;
}

impl DotNetString for i32 {
    fn dotnet_string(&self) -> String {
        self.to_string()
    }
}

impl DotNetString for bool {
    fn dotnet_string(&self) -> String {
        if *self { "True" } else { "False" }.to_owned()
    }
}

impl DotNetString for GemResult {
    fn dotnet_string(&self) -> String {
        // A class without a ToString override prints its full type name.
        "ACE.Server.Factories.Entity.GemResult".to_owned()
    }
}

macro_rules! dotnet_string_enum {
    ($($ty:ty),*) => {$(
        impl DotNetString for $ty {
            fn dotnet_string(&self) -> String {
                self.to_dotnet_string()
            }
        }
    )*};
}

dotnet_string_enum!(
    WeenieClassName,
    TreasureItemType,
    TreasureArmorType,
    TreasureWeaponType,
    TreasureHeritageGroup,
    SpellId
);
