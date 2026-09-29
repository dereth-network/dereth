// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DeathItem.cs
//! Port of `Source/ACE.Server/Entity/DeathItem.cs`.
//!
//! `DeathItems` (with `GetValueWithVariance`) and the nested `DeathItem`. A `DeathItem` holds its world object as a guid.

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::ItemType;
use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: DeathItemCategory
/// The different categories death items are sorted into. This is used to determine the adjusted
/// value. The declaration order is the sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeathItemCategory {
    None,
    MeleeWeapon,
    MissileWeapon,
    MagicCaster,
    Armor,
    Clothing,
    Jewelry,
    Food,
    Gem,
    SpellComponent,
    ManaStone,
    CraftingIngredient,
    ParchmentBook,
    Key,
    TradeNote,
    Misc,
}

// ACE: DeathItems
/// An inventory sorted by which items drop first during death.
#[derive(Debug, Clone, Default)]
pub struct DeathItems {
    // ACE: DeathItems.Inventory
    pub inventory: Vec<DeathItem>,

    // ACE: DeathItems.InventoryGroups
    /// for debugging
    pub inventory_groups: DotNetDict<DeathItemCategory, Vec<DeathItem>>,
}

impl DeathItems {
    // ACE: DeathItems.DeathItems
    /// Constructs a list of death items from the possible inventory items that can be dropped on
    /// death. Draws one `ThreadSafeRandom.Next(float, float)` per item with a known category, in
    /// the order sorted by category, then by value.
    #[must_use]
    pub fn new(w: &World, inventory: &[ObjectGuid]) -> Self {
        let mut this = Self {
            inventory: Vec::new(),
            inventory_groups: DotNetDict::new(),
        };

        for item in inventory {
            this.inventory.push(DeathItem::new(w, *item));
        }

        this.build_groups();

        // Here's the system, taken straight from the developer spec, that Asheron's Call uses to determine the way items are lost:

        // First, we sort the inventory by order of value.
        // Second, we go back through the inventory starting at the most expensive item and look at each item's category.
        // If we've seen the item category before, we divide the value of the item in half.
        // At this point, we add a random 0 - 10 % variance to each item's value.

        // Reference: http://acpedia.org/wiki/Recovering_from_Death

        // exclude unknown types
        this.inventory
            .retain(|i| i.category != DeathItemCategory::None);

        // sort by original value and category
        // (`OrderByDescending(AdjustedValue).OrderBy(Category)`: both stable, so the result is by
        // category, then by value descending, then in inventory order)
        this.inventory
            .sort_by_key(|i| std::cmp::Reverse(i.adjusted_value));
        this.inventory.sort_by_key(|i| i.category);

        // halve the values of every item, except the most valued item in each category
        Self::half_values(&mut this.inventory);

        // adjusted value by randomized variance
        for item in &mut this.inventory {
            item.adjusted_value = get_value_with_variance(item.adjusted_value);
        }

        // re-sort by final adjusted value
        this.inventory
            .sort_by_key(|i| std::cmp::Reverse(i.adjusted_value));

        this
    }

    // ACE: DeathItems.BuildGroups
    /// Builds the list of death items grouped by category.
    pub fn build_groups(&mut self) {
        self.inventory_groups = DotNetDict::new();

        for item in &self.inventory {
            self.inventory_groups
                .get_or_insert_with(item.category, Vec::new)
                .push(item.clone());
        }
    }

    // ACE: DeathItems.HalfValues
    /// Halves the values of every item, except the most valued item in each category. Assumes the
    /// list has been pre-sorted by category and item value.
    pub fn half_values(inventory: &mut [DeathItem]) {
        let mut prev_category = DeathItemCategory::None;

        for item in inventory {
            if item.category == prev_category {
                item.adjusted_value /= 2;
            } else {
                prev_category = item.category;
            }
        }
    }
}

// ACE: DeathItems.DeathItem
/// An inventory item that can be possibly dropped on death.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeathItem {
    // ACE: DeathItems.DeathItem.WorldObject
    pub world_object: ObjectGuid,
    // ACE: DeathItems.DeathItem.Category
    pub category: DeathItemCategory,
    // ACE: DeathItems.DeathItem.Name
    pub name: Option<String>,

    // ACE: DeathItems.DeathItem.AdjustedValue
    /// The original value is randomized slightly, and the non-most expensive items in each
    /// category are halved.
    pub adjusted_value: i32,
}

impl DeathItem {
    // ACE: DeathItems.DeathItem.DeathItem
    ///
    /// # Panics
    /// When `wo` is not in the store (ACE: `NullReferenceException`).
    #[must_use]
    pub fn new(w: &World, wo: ObjectGuid) -> Self {
        let name = crate::dispatch::name::name(w, wo);
        let o = w
            .objects
            .get(wo)
            .expect("NullReferenceException: DeathItem(wo)");
        let category = Self::get_category(o.item_type());
        let mut adjusted_value = o.value().unwrap_or(0); // stack size?
        if o.stack_size().unwrap_or(1) > 1 {
            adjusted_value /= o.stack_size().unwrap_or(1);
        }
        Self {
            world_object: wo,
            category,
            name,
            adjusted_value,
        }
    }

    // ACE: DeathItems.DeathItem.GetCategory
    /// The category of an item by its (exact) `ItemType`. ACE takes the world object and reads
    /// only its `ItemType`.
    #[must_use]
    pub fn get_category(item_type: ItemType) -> DeathItemCategory {
        match item_type {
            ItemType::MeleeWeapon => DeathItemCategory::MeleeWeapon,
            ItemType::MissileWeapon => DeathItemCategory::MissileWeapon,
            ItemType::Caster | ItemType::MagicWieldable => DeathItemCategory::MagicCaster,
            ItemType::Armor => DeathItemCategory::Armor,
            ItemType::Clothing => DeathItemCategory::Clothing,
            ItemType::Jewelry => DeathItemCategory::Jewelry,
            ItemType::Food => DeathItemCategory::Food,
            ItemType::Gem => DeathItemCategory::Gem,
            ItemType::SpellComponents => DeathItemCategory::SpellComponent,
            ItemType::ManaStone => DeathItemCategory::ManaStone,
            ItemType::CraftAlchemyBase
            | ItemType::CraftAlchemyIntermediate
            | ItemType::CraftCookingBase
            | ItemType::CraftFletchingBase
            | ItemType::CraftFletchingIntermediate => DeathItemCategory::CraftingIngredient,
            ItemType::Writable => DeathItemCategory::ParchmentBook,
            ItemType::Key => DeathItemCategory::Key,
            ItemType::PromissoryNote => DeathItemCategory::TradeNote, // should not drop?
            ItemType::Misc => DeathItemCategory::Misc,
            // containers don't drop? (`ItemType.Container`), and unknown types
            _ => DeathItemCategory::None,
        }
    }
}

// ACE: DeathItems.MaxVariance
/// Randomize the original item values by a variance %. A mutable `public static` in ACE that
/// nothing assigns, so a constant here.
pub const MAX_VARIANCE: f32 = 0.10;

// ACE: DeathItems.GetValueWithVariance
/// Returns the input value adjusted between -variance and +variance. Draws one
/// `ThreadSafeRandom.Next(float, float)`.
#[must_use]
pub fn get_value_with_variance(value: i32) -> i32 {
    // At this point, we add a random 0 - 10 % variance to each item's value.
    // should this be +/- 10%, or even +/- 5%?

    // http://www.postcount.net/forum/showthread.php?79784-death-item-formula
    // according to this post, it could be +/- 10%

    let variance = ThreadSafeRandom::next_float(-MAX_VARIANCE, MAX_VARIANCE);

    (f64::from(value) + f64::from(value) * variance).cs_cast()
}
