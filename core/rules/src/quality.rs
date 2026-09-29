//! What the quality inquiries read from an object: the seam between the rules and whichever side
//! holds the object's qualities.
//!
//! The inquiries in [`attributes`](crate::attributes), [`skills`](crate::skills),
//! [`burden`](crate::burden) and [`advancement`](crate::advancement) (`inq_attribute`,
//! `inq_attribute_2nd`, `inq_skill`, `inq_load`, the raise costs) are written against this trait,
//! not against a storage type. The client's player qualities implement it; so can anything else
//! that holds the same facts, for example a server's own object, which then gets the client's
//! answers over its own storage.
//!
//! The required reads are **raw**: stored values, no enchantment applied. The inquiries apply the
//! enchantments themselves, through [`QualityRead::enchantments`]; the enchanted int and float
//! reads are [`QualityRead::inq_int_enchanted`] and [`QualityRead::inq_float_enchanted`].

use std::borrow::Cow;

use dereth_assets::tables::QualityFilter;
use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute, Skill};

use crate::enchant::EnchantmentRegistry;

/// The stored qualities an inquiry reads.
pub trait QualityRead {
    /// Int property `id` as stored, or `None` when the object has none.
    fn int(&self, id: u32) -> Option<i32>;

    /// Float property `id` as stored, or `None` when the object has none.
    fn float(&self, id: u32) -> Option<f64>;

    /// The primary attribute record for `PropertyAttribute` id `id` (1..=6), or `None`.
    fn attribute(&self, id: u32) -> Option<Attribute>;

    /// The secondary attribute record for `PropertyAttribute2nd` id `id` (1..=6). Odd ids are
    /// maxima and even ids current values, and both name the **same** record: 1 and 2 are the
    /// health record, 3 and 4 stamina, 5 and 6 mana. `None` for any other id or a missing record.
    fn attribute_2nd(&self, id: u32) -> Option<SecondaryAttribute>;

    /// The skill record for skill id `id`, or `None`.
    fn skill(&self, id: u32) -> Option<Skill>;

    /// The object's enchantments, as the stacking rules take them. Borrowed where the object keeps
    /// them in this form, built on the spot where it does not.
    fn enchantments(&self) -> Cow<'_, EnchantmentRegistry>;

    /// Int property `id`, defaulting to 0: the shape most inquiries read.
    fn inq_int(&self, id: u32) -> i32 {
        self.int(id).unwrap_or(0)
    }

    /// Int property `id` as the client's ordinary (not raw) read answers it: the stored value passed
    /// through the int enchantments, or `None` when the object has no such property, in which case
    /// nothing is enchanted.
    ///
    /// The enchantments apply only to a property `filter` lists. With no filter loaded the stored
    /// value is returned unchanged, as the client does when the filter cannot be fetched.
    /// `allow_negative` selects truncation over the zero clamp and round half up
    /// ([`EnchantmentRegistry::enchant_int`]).
    fn inq_int_enchanted(
        &self,
        id: u32,
        allow_negative: bool,
        filter: Option<&QualityFilter>,
    ) -> Option<i32> {
        let raw = self.int(id)?;
        let allowed = filter.is_some_and(|f| f.allows_int(id));
        Some(
            self.enchantments()
                .enchant_int(id, raw, allow_negative, allowed),
        )
    }

    /// Float property `id` as the client's ordinary (not raw) read answers it: the stored value
    /// passed through the float enchantments when `filter` lists the property, or `None` when the
    /// object has none. The arithmetic is `f32`, so an enchanted value carries `f32` precision.
    fn inq_float_enchanted(&self, id: u32, filter: Option<&QualityFilter>) -> Option<f64> {
        let raw = self.float(id)?;
        let allowed = filter.is_some_and(|f| f.allows_float(id));
        Some(self.enchantments().enchant_float(id, raw, allowed))
    }
}
