// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Crafting.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Crafting.cs`.
//!
//! Salvaging with the Ust: the amount of salvage an item yields (`GetStructure`, `CalcNumUnits`),
//! the salvage bags it fills (`GetSalvageBag`, `TryAddSalvage`, `AddSalvage`) and the result
//! messages (`SalvageResults`). New salvage bags are objects in `World.objects` from the moment the
//! factory builds them, as `player_inventory.rs` does for new objects.

use std::sync::LazyLock;

use empyrean_common::dotnet::{math, CsCast, DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    ChatMessageType, ItemType, MaterialType, PropertyInt, Skill, SkillAdvancementClass,
    WeenieClassName, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::salvage_results::SalvageResults;
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_salvage_operations_result::game_event_salvage_operations_result;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::{container, player_networking};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_Crafting.cs`.
#[derive(Debug, Default)]
pub struct PlayerCraftingFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

// ACE: Player.MaterialSalvage
/// A lookup table for MaterialType => Salvage Bag WCIDs (a `public static Dictionary` nothing
/// modifies).
pub static MATERIAL_SALVAGE: LazyLock<DotNetDict<i32, i32>> = LazyLock::new(|| {
    let rows: [(i32, i32); 77] = [
        (1, 20983),  // Ceramic
        (2, 21067),  // Porcelain
        (3, 0),      // ======= Cloth =======
        (4, 20987),  // Linen
        (5, 20992),  // Satin
        (6, 21076),  // Silk
        (7, 20994),  // Velvet
        (8, 20995),  // Wool
        (9, 0),      // ======= Gems =======
        (10, 21034), // Agate
        (11, 21035), // Amber
        (12, 21036), // Amethyst
        (13, 21037), // Aquamarine
        (14, 21038), // Azurite
        (15, 21039), // Black Garnet
        (16, 21040), // Black Opal
        (17, 21041), // Bloodstone
        (18, 21043), // Carnelian
        (19, 21044), // Citrine
        (20, 21046), // Diamond
        (21, 21048), // Emerald
        (22, 21049), // Fire Opal
        (23, 21050), // Green Garnet
        (24, 21051), // Green Jade
        (25, 21053), // Hematite
        (26, 21054), // Imperial Topaz
        (27, 21056), // Jet
        (28, 21057), // Lapis Lazuli
        (29, 21058), // Lavender Jade
        (30, 21060), // Malachite
        (31, 21062), // Moonstone
        (32, 21064), // Onyx
        (33, 21065), // Opal
        (34, 21066), // Peridot
        (35, 21069), // Red Garnet
        (36, 21070), // Red Jade
        (37, 21071), // Rose Quartz
        (38, 21072), // Ruby
        (39, 21074), // Sapphire
        (40, 21078), // Smokey Quartz
        (41, 21079), // Sunstone
        (42, 21081), // Tiger Eye
        (43, 21082), // Tourmaline
        (44, 21083), // Turquoise
        (45, 21084), // White Jade
        (46, 21085), // White Quartz
        (47, 21086), // White Sapphire
        (48, 21087), // Yellow Garnet
        (49, 21088), // Yellow Topaz
        (50, 21089), // Zircon
        (51, 21055), // Ivory
        (52, 21059), // Leather
        (53, 20981), // Armoredillo Hide
        (54, 21052), // Gromnie Hide
        (55, 20991), // Reedshark Hide
        (56, 0),     // ======= Metal =======
        (57, 21042), // Brass
        (58, 20982), // Bronze
        (59, 21045), // Copper
        (60, 20984), // Gold
        (61, 20986), // Iron
        (62, 21068), // Pyreal
        (63, 21077), // Silver
        (64, 20993), // Steel
        (65, 0),     // ======= Stone =======
        (66, 20980), // Alabaster
        (67, 20985), // Granite
        (68, 21061), // Marble
        (69, 21063), // Obsidian
        (70, 21073), // Sandstone
        (71, 21075), // Serpentine
        (72, 0),     // ======= Wood =======
        (73, 21047), // Ebony
        (74, 20988), // Mahogany
        (75, 20989), // Oak
        (76, 20990), // Pine
        (77, 21080), // Teak
    ];
    let mut d = DotNetDict::new();
    for (k, v) in rows {
        d.add(k, v);
    }
    d
});

// ACE: Player.GetMaxSkill
/// Returns the skill with the largest current value (buffed); the first of equals.
pub fn get_max_skill(w: &mut World, this: ObjectGuid, skills: &[Skill]) -> Option<CreatureSkill> {
    let mut max_skill: Option<(CreatureSkill, u32)> = None;
    for &skill in skills {
        let creature_skill = obj_mut(w, this)
            .get_creature_skill(skill, true)
            .expect("ACE: GetCreatureSkill(skill, add: true)");
        let current = creature_skill.current(w, this);
        if max_skill.is_none_or(|(_, max)| current > max) {
            max_skill = Some((creature_skill, current));
        }
    }
    max_skill.map(|(s, _)| s)
}

// ACE: Player.TinkeringSkills
/// A `public static List<Skill>` nothing modifies.
pub const TINKERING_SKILLS: [Skill; 4] = [
    Skill::ArmorTinkering,
    Skill::WeaponTinkering,
    Skill::ItemTinkering,
    Skill::MagicItemTinkering,
];

// ACE: Player.ToolIsValidUst
/// The tool must be a Ust that the player owns.
fn tool_is_valid_ust(w: &mut World, this: ObjectGuid, tool: u32) -> bool {
    let found =
        player_inventory::find_object(w, this, ObjectGuid::new(tool), SearchLocations::Everywhere);
    let tool_object = found.result;
    let root_owner = found.root_owner;

    if tool_object.is_none_or(|t| {
        obj(w, t).biota.weenie_class_id != u32::from(WeenieClassName::W_TINKERINGTOOL_CLASS.0)
    }) {
        player_networking::send_weenie_error(w, this, WeenieError::NotASalvageTool);
        false
    } else if root_owner != Some(this) {
        player_networking::send_weenie_error(w, this, WeenieError::YouDoNotOwnThatSalvageTool);
        false
    } else {
        true
    }
}

// ACE: Player.HandleSalvaging
/// Salvages each listed inventory item with the Ust `tool`: the salvage goes into bags by material,
/// the items are consumed, the bags are added to the inventory, then one result message per skill
/// (unless the player squelches Salvaging).
pub fn handle_salvaging(w: &mut World, this: ObjectGuid, tool: u32, salvage_items: Vec<u32>) {
    // DIVERGE: a world without tinkering (`EraFeatures::tinkering`) refuses salvaging: no item
    // is consumed and no salvage bag made (V421).
    if !crate::world_objects::era_gates::has(w, this, w.era.features.tinkering, "tinkering") {
        return;
    }

    if !tool_is_valid_ust(w, this, tool) {
        return;
    }

    let mut salvage_bags: Vec<ObjectGuid> = Vec::new();
    let mut salvage_results = SalvageResults::new();

    for item_guid in salvage_items {
        let item = container::get_inventory_item(w, this, ObjectGuid::new(item_guid));
        let Some(item) = item else {
            //log.DebugFormat("[CRAFTING] {0}.HandleSalvaging({1:X8}): couldn't find inventory item", Name, itemGuid);
            continue;
        };

        if obj(w, item).material_type().is_none() {
            log::warn!(
                "[CRAFTING] {}.HandleSalvaging({}): no material type",
                name(w, this),
                name(w, item)
            );
            continue;
        }

        if shims::is_trading(w, this)
            && dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded(w, item, &shims::items_in_trade_window(w, this))
        {
            player_networking::send_weenie_error(w, this, WeenieError::YouCannotSalvageItemsInTrading);
            continue;
        }

        if obj_mut(w, item).workmanship().is_none() || obj(w, item).retained() {
            continue;
        }

        add_salvage(w, this, &mut salvage_bags, item, &mut salvage_results);

        // can any salvagable items be stacked?
        player_inventory::try_consume_from_inventory_with_networking(w, this, item, i32::MAX);
    }

    // add salvage bags
    for salvage_bag in salvage_bags {
        if player_inventory::try_create_in_inventory_with_networking(w, this, salvage_bag).is_none()
        {
            // DIVERGE: a bag that does not fit is dropped unreferenced in ACE (the garbage
            // collector's); it leaves World.objects here (4.5a's rule for unclaimed objects).
            w.objects.remove(salvage_bag);
        }
    }

    // send network messages
    if !crate::world_objects::managers::squelch_manager::squelches_contains(
        w,
        this,
        Some(this),
        ChatMessageType::Salvaging,
    ) {
        let Some(session) = player_session(w, this) else {
            return;
        };
        for (skill, messages) in salvage_results.get_messages().iter() {
            let m = game_event_salvage_operations_result(w, session, *skill, messages);
            enqueue_send(w, session, m);
        }
    }
}

// ACE: Player.AddSalvage
/// Adds the salvage `item` yields to the bags of its material, filling each before making another,
/// and raises each bag's value.
pub fn add_salvage(
    w: &mut World,
    this: ObjectGuid,
    salvage_bags: &mut Vec<ObjectGuid>,
    item: ObjectGuid,
    salvage_results: &mut SalvageResults,
) {
    // `(MaterialType)item.MaterialType`: a null throws (InvalidOperationException).
    let material_type = obj(w, item)
        .material_type()
        .expect("System.InvalidOperationException: Nullable object must have a value.");

    // determine the amount of salvage produced (structure)
    let mut message: Option<u32> = None;
    let amount_produced = get_structure(w, this, item, salvage_results, &mut message);

    let mut remaining = amount_produced;

    while remaining > 0 {
        // get the destination salvage bag

        // if there are no existing salvage bags for this material type,
        // or all of the salvage bags for this material type are full,
        // this will create a new salvage bag, and adds it to salvageBags

        let salvage_bag = get_salvage_bag(w, this, material_type, salvage_bags);

        let added = try_add_salvage(w, this, salvage_bag, item, remaining);
        remaining = remaining.wrapping_sub(added);

        // https://asheron.fandom.com/wiki/Salvaging/Value_Pre2013

        // increase value of salvage bag - salvage skill is a factor,
        // if bags aren't being combined here
        #[allow(clippy::cast_precision_loss)] // int to float, as C# promotes it
        let mut value_factor = added as f32 / amount_produced as f32;
        if obj(w, item).item_type() != ItemType::TinkeringMaterial {
            let salvaging = obj_mut(w, this)
                .get_creature_skill(Skill::Salvaging, true)
                .expect("ACE: GetCreatureSkill(skill, add: true)");
            #[allow(clippy::cast_precision_loss)]
            let current = salvaging.current(w, this) as f32;
            #[allow(clippy::cast_precision_loss)]
            let augs = obj(w, this).augmentation_bonus_salvage() as f32;
            value_factor *= current / 387.0f32 * (1.0f32 + 0.25f32 * augs);
        }

        #[allow(clippy::cast_precision_loss)]
        let item_value = obj(w, item).value().unwrap_or(0) as f32;
        let added_value: i32 = math::round(f64::from(item_value * value_factor)).cs_cast();

        let bag = obj_mut(w, salvage_bag);
        bag.set_value(Some(
            bag.value()
                .unwrap_or(0)
                .wrapping_add(added_value)
                .min(75000),
        ));

        // a bit different here, since ACE handles overages
        if let Some(key) = message {
            #[allow(clippy::cast_precision_loss)]
            let item_workmanship = obj(w, item).item_workmanship().map_or(0.0f32, |x| x as f32);
            let m = salvage_results.message_mut(key);
            m.workmanship += item_workmanship;
            m.num_items_in_material = m.num_items_in_material.wrapping_add(1);
        }
    }
}

// ACE: Player.TryAddSalvage
/// Adds up to `try_amount` units of `item` to the bag; returns the units taken (a bag of salvage
/// being combined reports all of it unless `salvage_handle_overages`, so retail's overage is lost).
pub fn try_add_salvage(
    w: &mut World,
    _this: ObjectGuid,
    salvage_bag: ObjectGuid,
    item: ObjectGuid,
    try_amount: i32,
) -> i32 {
    let (max_structure, structure) = {
        let b = obj(w, salvage_bag);
        (
            b.max_structure().map_or(100, i32::from),
            b.structure().map_or(0, i32::from),
        )
    };

    let space = max_structure.wrapping_sub(structure);

    let amount = try_amount.min(space);

    obj_mut(w, salvage_bag).set_structure(Some(structure.wrapping_add(amount).cs_cast()));

    // add workmanship
    let mut item_num_items = obj(w, item).stack_size().unwrap_or(1);
    let workmanship_bag = obj(w, salvage_bag).item_workmanship().unwrap_or(0);
    let workmanship_item = obj(w, item).item_workmanship().unwrap_or(0);

    obj_mut(w, salvage_bag).set_item_workmanship(Some(
        workmanship_bag.wrapping_add(workmanship_item.wrapping_mul(item_num_items)),
    ));

    // increment # of items that went into this salvage bag
    let is_material = obj(w, item).item_type() == ItemType::TinkeringMaterial;
    if is_material {
        item_num_items = obj(w, item).num_items_in_material().unwrap_or(1);

        // handle overflows when combining bags
        if try_amount > space {
            #[allow(clippy::cast_precision_loss)] // int to float, as C# promotes it
            let mut scalar = space as f32 / try_amount as f32;
            #[allow(clippy::cast_precision_loss)]
            let mut new_items: i32 = f64::from(item_num_items as f32 * scalar).ceil().cs_cast();
            #[allow(clippy::cast_precision_loss)]
            {
                scalar = new_items as f32 / item_num_items as f32;
            }
            let prev_num_items = item_num_items;
            item_num_items = new_items;

            let reduce: i32 =
                math::round(f64::from(workmanship_item) * (1.0 - f64::from(scalar))).cs_cast();
            let b = obj_mut(w, salvage_bag);
            b.set_item_workmanship(b.item_workmanship().map(|x| x.wrapping_sub(reduce)));

            // and for the next bag...
            if prev_num_items == new_items {
                new_items -= 1;
            }

            let item_workmanship = obj_mut(w, item).workmanship();
            let i = obj_mut(w, item);
            i.set_num_items_in_material(
                i.num_items_in_material().map(|x| x.wrapping_sub(new_items)),
            );
            //item.ItemWorkmanship -= (int)Math.Round(workmanship_item * scalar);
            let n = i
                .num_items_in_material()
                .expect("System.InvalidOperationException: Nullable object must have a value.");
            let iw = item_workmanship
                .expect("System.InvalidOperationException: Nullable object must have a value.");
            #[allow(clippy::cast_precision_loss)]
            let v: i32 = math::round(f64::from(n as f32 * iw)).cs_cast();
            i.set_item_workmanship(Some(v));
        }
    }
    let b = obj_mut(w, salvage_bag);
    b.set_num_items_in_material(Some(
        b.num_items_in_material()
            .unwrap_or(0)
            .wrapping_add(item_num_items),
    ));

    let structure_now = obj(w, salvage_bag)
        .structure()
        .map(|s| s.to_string())
        .unwrap_or_default();
    world_object::world_object_set_name(w, salvage_bag, format!("Salvage ({structure_now})"));

    if is_material {
        if property_manager::get_bool(w, "salvage_handle_overages", false, true).item {
            amount
        } else {
            try_amount
        }
    } else {
        amount
    }
}

// ACE: Player.GetStructure
/// The units of salvage `salvage_item` yields: the better of the Salvaging skill (with the
/// Ciandra's Fortune augmentations) and the highest trained tinkering skill (capped at the item's
/// workmanship), times the stack size; a bag of salvage keeps its own. `message` is set to the
/// key of the result message it is counted in.
pub fn get_structure(
    w: &mut World,
    this: ObjectGuid,
    salvage_item: ObjectGuid,
    salvage_results: &mut SalvageResults,
    message: &mut Option<u32>,
) -> i32 {
    // By default, salvaging uses either a tinkering skill, or your salvaging skill that would yield the greatest amount of material.
    // Tinkering skills can only yield at most the workmanship number in units of salvage.
    // The salvaging skill can produce more units than workmanship.

    // You can also significantly increase the amount of material returned by training the Ciandra's Fortune augmentation.
    // This augmentation can be trained 4 times, each time providing an additional 25% bonus to the amount of material returned.

    // is this a bag of salvage?
    // if so, return its existing structure
    if obj(w, salvage_item).item_type() == ItemType::TinkeringMaterial {
        return i32::from(
            obj(w, salvage_item)
                .structure()
                .expect("System.InvalidOperationException: Nullable object must have a value."),
        );
    }

    let workmanship = obj_mut(w, salvage_item).workmanship().unwrap_or(1.0);
    let stack_size = obj(w, salvage_item).stack_size().unwrap_or(1);

    // should this be getting the highest tinkering skill,
    // or the tinkering skill for the material?
    let salvaging = obj_mut(w, this)
        .get_creature_skill(Skill::Salvaging, true)
        .expect("ACE: GetCreatureSkill(skill, add: true)");
    let salvage_skill = salvaging.current(w, this);
    let highest_tinkering_skill =
        get_max_skill(w, this, &TINKERING_SKILLS).expect("ACE: GetMaxSkill of four skills");
    let highest_trained_tinkering_skill = if highest_tinkering_skill.advancement_class(obj(w, this))
        >= SkillAdvancementClass::Trained
    {
        highest_tinkering_skill.current(w, this)
    } else {
        0
    };

    // take augs into account for salvaging only
    let augs = obj(w, this).augmentation_bonus_salvage();
    let salvage_amount =
        calc_num_units(salvage_skill.cs_cast(), workmanship, augs).wrapping_mul(stack_size);
    let mut tinkering_amount =
        calc_num_units(highest_trained_tinkering_skill.cs_cast(), workmanship, 0);

    // cap tinkeringAmount to item workmanship
    let cap: i32 = math::round(f64::from(
        obj_mut(w, salvage_item).workmanship().unwrap_or(1.0),
    ))
    .cs_cast();
    tinkering_amount = tinkering_amount.min(cap).wrapping_mul(stack_size);

    // choose the best one
    let add_structure = salvage_amount.max(tinkering_amount);

    let skill = if salvage_amount > tinkering_amount {
        Skill::Salvaging
    } else {
        get_max_skill(w, this, &TINKERING_SKILLS)
            .expect("ACE: GetMaxSkill of four skills")
            .skill
    };

    let material_type = obj(w, salvage_item)
        .material_type()
        .unwrap_or(MaterialType::Unknown);
    let key = salvage_results.get_message(material_type, skill);
    *message = Some(key);
    let m = salvage_results.message_mut(key);
    m.amount = m.amount.wrapping_add(add_structure.cast_unsigned());

    add_structure
}

// ACE: Player.CalcNumUnits
/// Calculates the number of units returned from a salvaging operation.
///
/// `skill` is the current salvaging or highest trained tinkering skill for the player,
/// `workmanship` that of the item being salvaged, `num_augs` the player's
/// `AugmentationBonusSalvage`.
#[must_use]
pub fn calc_num_units(skill: i32, workmanship: f32, num_augs: i32) -> i32 {
    // https://web.archive.org/web/20170130213649/http://www.thejackcat.com/AC/Shopping/Crafts/Salvage_old.htm
    // https://web.archive.org/web/20170130194012/http://www.thejackcat.com/AC/Shopping/Crafts/Salvage.htm

    #[allow(clippy::cast_precision_loss)] // int to float, as C# promotes it
    let units: i32 =
        f64::from(skill as f32 / 194.0f32 * workmanship * (1.0f32 + 0.25f32 * num_augs as f32))
            .floor()
            .cs_cast();
    1i32.wrapping_add(units)
}

// ACE: Player.GetSalvageBag
/// The first bag of `material_type` in `salvage_bags` that is not full, or a new one (added to the
/// list).
///
/// # Panics
/// For a material with no bag (`KeyNotFoundException`), or one whose bag weenie is missing (a null
/// object, `NullReferenceException`).
pub fn get_salvage_bag(
    w: &mut World,
    _this: ObjectGuid,
    material_type: MaterialType,
    salvage_bags: &mut Vec<ObjectGuid>,
) -> ObjectGuid {
    // first try finding the first non-filled salvage bag, for this material type
    let existing = salvage_bags.iter().copied().find(|&i| {
        let o = obj(w, i);
        i64::from(o.get_property(PropertyInt::MaterialType).unwrap_or(0))
            == i64::from(material_type.0)
            && o.structure().map_or(0, i32::from) < o.max_structure().map_or(0, i32::from)
    });

    if let Some(existing) = existing {
        return existing;
    }

    // not found - create a new salvage bag
    let key: i32 = material_type.0.cs_cast();
    let wcid: u32 = MATERIAL_SALVAGE
        .get(&key)
        .copied()
        .unwrap_or_else(|| panic!("System.Collections.Generic.KeyNotFoundException: {key}"))
        .cast_unsigned();
    let salvage_bag = shims::create_new_world_object(w, wcid)
        .expect("System.NullReferenceException: CreateNewWorldObject returned null");

    let b = obj_mut(w, salvage_bag);
    b.set_structure(None); // TODO: fix bugged TOD data for mahogany 20988 / green garnet 21050
    b.set_item_workmanship(None);
    b.set_num_items_in_material(None);

    salvage_bags.push(salvage_bag);

    salvage_bag
}

// ---- shims

mod shims {
    use super::{DotNetHashSet, ObjectGuid, World};

    /// `Player.IsTrading` (`Player_Trade.cs`).
    pub(super) fn is_trading(w: &World, this: ObjectGuid) -> bool {
        crate::world_objects::player_trade::is_trading(w, this)
    }

    /// `Player.ItemsInTradeWindow` (`Player_Trade.cs`).
    pub(super) fn items_in_trade_window(w: &World, this: ObjectGuid) -> DotNetHashSet<ObjectGuid> {
        crate::world_objects::player_trade::items_in_trade_window(w, this)
    }

    /// `WorldObjectFactory.CreateNewWorldObject(wcid)`, put in `World.objects`.
    pub(super) fn create_new_world_object(
        w: &mut World,
        weenie_class_id: u32,
    ) -> Option<ObjectGuid> {
        crate::managers::recipe_manager::shims::create_new_world_object(w, weenie_class_id)
    }
}
