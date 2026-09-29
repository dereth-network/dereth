// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HashComparer.cs
//! Port of `Source/ACE.Server/Network/Structure/HashComparer.cs`.
//!
//! The retail client reads a packable hash table into buckets (`key % numBuckets`), so ACE writes
//! each table through a `SortedDictionary` with one of these comparers: bucket first, then key.
//! Keys are unique in every table, so [`sorted`] (an ordinary sort by the comparer) enumerates
//! exactly as the `SortedDictionary` does.

use std::cmp::Ordering;

use empyrean_entity::enums::{
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, Skill,
};
use empyrean_entity::ObjectGuid;

/// A C# `IComparer<T>` over hash-table keys.
pub trait KeyComparer<T> {
    /// `Compare(a, b)`.
    fn compare(&self, a: &T, b: &T) -> Ordering;
}

/// `new SortedDictionary<K, V>(dictionary, comparer)` enumerated: the entries in the comparer's
/// order.
pub fn sorted<K: Clone, V: Clone>(
    entries: impl IntoIterator<Item = (K, V)>,
    comparer: &impl KeyComparer<K>,
) -> Vec<(K, V)> {
    let mut v: Vec<(K, V)> = entries.into_iter().collect();
    v.sort_by(|a, b| comparer.compare(&a.0, &b.0));
    v
}

// ACE: HashComparer
/// Over `uint` keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashComparer {
    // ACE: HashComparer.NumBuckets
    pub num_buckets: u16,
}

impl HashComparer {
    // ACE: HashComparer.HashComparer
    #[must_use]
    pub const fn new(num_buckets: u16) -> Self {
        Self { num_buckets }
    }
}

impl KeyComparer<u32> for HashComparer {
    // ACE: HashComparer.Compare
    fn compare(&self, a: &u32, b: &u32) -> Ordering {
        let key_a = a % u32::from(self.num_buckets);
        let key_b = b % u32::from(self.num_buckets);

        let mut result = key_a.cmp(&key_b);

        if result == Ordering::Equal {
            result = a.cmp(b);
        }

        result
    }
}

/// The comparers over a property enum: `(int)a % NumBuckets`, then the enum's own order (its
/// numeric value). ACE declares one class per enum; they differ only in the key type.
macro_rules! property_comparer {
    ($(#[$doc:meta])* $name:ident, $key:ty, $anchor_ctor:literal, $anchor_cmp:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name {
            pub num_buckets: u16,
        }

        impl $name {
            #[doc = $anchor_ctor]
            #[must_use]
            pub const fn new(num_buckets: u16) -> Self {
                Self { num_buckets }
            }
        }

        impl KeyComparer<$key> for $name {
            #[doc = $anchor_cmp]
            fn compare(&self, a: &$key, b: &$key) -> Ordering {
                let key_a = i32::from(a.0) % i32::from(self.num_buckets);
                let key_b = i32::from(b.0) % i32::from(self.num_buckets);

                let mut result = key_a.cmp(&key_b);

                if result == Ordering::Equal {
                    result = a.0.cmp(&b.0);
                }

                result
            }
        }
    };
}

// ACE: PropertyIntComparer, PropertyIntComparer.NumBuckets, PropertyIntComparer.PropertyIntComparer, PropertyIntComparer.Compare
property_comparer!(
    PropertyIntComparer,
    PropertyInt,
    "`new PropertyIntComparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyInt64Comparer, PropertyInt64Comparer.NumBuckets, PropertyInt64Comparer.PropertyInt64Comparer, PropertyInt64Comparer.Compare
property_comparer!(
    PropertyInt64Comparer,
    PropertyInt64,
    "`new PropertyInt64Comparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyBoolComparer, PropertyBoolComparer.NumBuckets, PropertyBoolComparer.PropertyBoolComparer, PropertyBoolComparer.Compare
property_comparer!(
    PropertyBoolComparer,
    PropertyBool,
    "`new PropertyBoolComparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyFloatComparer, PropertyFloatComparer.NumBuckets, PropertyFloatComparer.PropertyFloatComparer, PropertyFloatComparer.Compare
property_comparer!(
    PropertyFloatComparer,
    PropertyFloat,
    "`new PropertyFloatComparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyStringComparer, PropertyStringComparer.NumBuckets, PropertyStringComparer.PropertyStringComparer, PropertyStringComparer.Compare
property_comparer!(
    PropertyStringComparer,
    PropertyString,
    "`new PropertyStringComparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyDataIdComparer, PropertyDataIdComparer.NumBuckets, PropertyDataIdComparer.PropertyDataIdComparer, PropertyDataIdComparer.Compare
property_comparer!(
    PropertyDataIdComparer,
    PropertyDataId,
    "`new PropertyDataIdComparer(numBuckets)`.",
    "`Compare(a, b)`."
);
// ACE: PropertyInstanceIdComparer, PropertyInstanceIdComparer.NumBuckets, PropertyInstanceIdComparer.PropertyInstanceIdComparer, PropertyInstanceIdComparer.Compare
property_comparer!(
    PropertyInstanceIdComparer,
    PropertyInstanceId,
    "`new PropertyInstanceIdComparer(numBuckets)`.",
    "`Compare(a, b)`."
);

// ACE: SkillComparer
/// Over `Skill` (an `int` enum): `(int)a % NumBuckets` keeps C#'s sign for a negative skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillComparer {
    // ACE: SkillComparer.NumBuckets
    pub num_buckets: u16,
}

impl SkillComparer {
    // ACE: SkillComparer.SkillComparer
    #[must_use]
    pub const fn new(num_buckets: u16) -> Self {
        Self { num_buckets }
    }
}

impl KeyComparer<Skill> for SkillComparer {
    // ACE: SkillComparer.Compare
    fn compare(&self, a: &Skill, b: &Skill) -> Ordering {
        let key_a = a.0 % i32::from(self.num_buckets);
        let key_b = b.0 % i32::from(self.num_buckets);

        let mut result = key_a.cmp(&key_b);

        if result == Ordering::Equal {
            result = a.0.cmp(&b.0);
        }

        result
    }
}

// ACE: SpellComparer
/// Over `int` spell ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellComparer {
    // ACE: SpellComparer.NumBuckets
    pub num_buckets: u16,
}

impl SpellComparer {
    // ACE: SpellComparer.SpellComparer
    #[must_use]
    pub const fn new(num_buckets: u16) -> Self {
        Self { num_buckets }
    }
}

impl KeyComparer<i32> for SpellComparer {
    // ACE: SpellComparer.Compare
    fn compare(&self, a: &i32, b: &i32) -> Ordering {
        let key_a = a % i32::from(self.num_buckets);
        let key_b = b % i32::from(self.num_buckets);

        let mut result = key_a.cmp(&key_b);

        if result == Ordering::Equal {
            result = a.cmp(b);
        }

        result
    }
}

// ACE: GuidComparer
/// Over `ObjectGuid`, by `Full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidComparer {
    // ACE: GuidComparer.NumBuckets
    pub num_buckets: u16,
}

impl GuidComparer {
    // ACE: GuidComparer.GuidComparer
    #[must_use]
    pub const fn new(num_buckets: u16) -> Self {
        Self { num_buckets }
    }
}

impl KeyComparer<ObjectGuid> for GuidComparer {
    // ACE: GuidComparer.Compare
    fn compare(&self, a: &ObjectGuid, b: &ObjectGuid) -> Ordering {
        let key_a = a.full() % u32::from(self.num_buckets);
        let key_b = b.full() % u32::from(self.num_buckets);

        let mut result = key_a.cmp(&key_b);

        if result == Ordering::Equal {
            result = a.full().cmp(&b.full());
        }

        result
    }
}
