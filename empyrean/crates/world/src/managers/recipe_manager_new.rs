// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/RecipeManager_New.cs
//! Port of `Source/ACE.Server/Managers/RecipeManager_New.cs`.
//!
//! The recipes found by the source's weenie class rather than the cook book (dyes, the
//! workmanship-based tinkers, imbues, shield covers, slayer stones, Paragon ambers, the
//! uninscription stone), and `ReadJSON` (which nothing in ACE calls).
//!
//! `(WeenieClassName)source.WeenieClassId` casts to a `ushort` enum, so a wcid above 65535 matches
//! the class 65536 below it. No weenie in ACE's world database aliases a class named here that way
//! (checked against it), so the truncation is ported and unmarked.

use std::sync::{Arc, LazyLock};

use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_content::models::world::Recipe;
use empyrean_entity::enums::{
    EquipMask, ItemType, MaterialType, PropertyBool, PropertyString, WeenieClassName as C,
    WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::recipe_precursor::RecipePrecursor;
use crate::managers::recipe_manager::wcid;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: RecipeManager.ReadJSON
/// Reads `json\recipeprecursors.json` (relative to the working directory) into
/// `RecipeManagerState::precursors`.
///
/// # Panics
/// When the file cannot be read (`FileNotFoundException` and the like) or is not a JSON array.
pub fn read_json(w: &mut World) {
    // read recipeprecursors.json
    // tool -> target -> recipe
    let json = std::fs::read_to_string(r"json\recipeprecursors.json")
        .unwrap_or_else(|e| panic!("System.IO.IOException: {e}"));
    read_json_text(w, &json);
}

/// [`read_json`] over the file's text.
// ACE-BUG: `RecipePrecursor` declares public fields and `JsonSerializer.Deserialize` binds only properties by default (IncludeFields is false), so every precursor reads as tool 0, target 0, recipe 0 and `Precursors` ends up {0: {0: 0}}; harmless, since nothing calls ReadJSON or reads Precursors.
///
/// # Panics
/// When the text is not a JSON array (`JsonException`).
pub fn read_json_text(w: &mut World, json: &str) {
    let precursors: Vec<RecipePrecursor> = (0..json_array_len(json))
        .map(|_| RecipePrecursor::default())
        .collect();
    let mut all: DotNetDict<u32, DotNetDict<u32, u32>> = DotNetDict::new();

    for precursor in precursors {
        let tool = all.get_or_insert_with(precursor.tool, DotNetDict::new);
        tool.insert(precursor.target, precursor.recipe_id);
    }
    w.recipe_manager.precursors = Some(all);
}

/// The number of elements of a top-level JSON array (strings and nesting skipped).
fn json_array_len(json: &str) -> usize {
    let s = json.trim_start_matches('\u{feff}').trim();
    assert!(
        s.starts_with('[') && s.ends_with(']'),
        "System.Text.Json.JsonException: not a JSON array"
    );
    let inner = s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return 0;
    }
    let (mut depth, mut in_str, mut esc, mut n) = (0i32, false, false, 1usize);
    for c in inner.chars() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => n += 1,
            _ => {}
        }
    }
    n
}

/// `DatabaseManager.World.GetCachedRecipe(id)`.
fn cached_recipe(w: &World, recipe_id: u32) -> Option<Arc<Recipe>> {
    w.content.get_cached_recipe(recipe_id)
}

/// `target.Workmanship != null`: the getter is null exactly when `ItemWorkmanship` is (its
/// old-encoding repair, which writes back, only runs on a non-null value and does not change
/// nullness; read-only here, as ACE's lookup never saves the object).
// DIVERGE: the getter's old-encoding repair is not written back during the lookup (it is on the next Workmanship read).
fn has_workmanship(t: &WorldObject) -> bool {
    t.item_workmanship().is_some()
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `SourceToRecipe[(WeenieClassName)source.WeenieClassId]`.
///
/// # Panics
/// On a class with no entry (`KeyNotFoundException`).
fn source_to_recipe(source_class: C) -> u32 {
    *SOURCE_TO_RECIPE.get(&source_class).unwrap_or_else(|| {
        panic!("System.Collections.Generic.KeyNotFoundException: {source_class:?}")
    })
}

// ACE: RecipeManager.GetNewRecipe
/// The recipe for `source` on `target` by the source's weenie class, or `None`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn get_new_recipe(
    w: &World,
    _player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> Option<Arc<Recipe>> {
    let recipe: Option<Arc<Recipe>>;

    let s = obj(w, source);
    let t = obj(w, target);
    let source_wcid = s.biota.weenie_class_id;
    // `(WeenieClassName)source.WeenieClassId`: a ushort enum.
    let source_class = C(source_wcid.cs_cast());

    match source_class {
        C::W_POTDYEDARKGREEN_CLASS
        | C::W_POTDYEDARKRED_CLASS
        | C::W_POTDYEDARKYELLOW_CLASS
        | C::W_POTDYEWINTERBLUE_CLASS
        | C::W_POTDYEWINTERGREEN_CLASS
        | C::W_POTDYEWINTERSILVER_CLASS
        | C::W_POTDYESPRINGBLACK_CLASS
        | C::W_POTDYESPRINGBLUE_CLASS
        | C::W_POTDYESPRINGPURPLE_CLASS => {
            // ensure item is armor/clothing and dyeable
            if t.biota.weenie_type != WeenieType::Clothing
                || !t.get_property(PropertyBool::Dyable).unwrap_or(false)
            {
                return None;
            }

            recipe = cached_recipe(w, 3844); // base dye recipe
        }

        C::W_DYERAREETERNALFOOLPROOFBLUE_CLASS
        | C::W_DYERAREETERNALFOOLPROOFBLACK_CLASS
        | C::W_DYERAREETERNALFOOLPROOFBOTCHED_CLASS
        | C::W_DYERAREETERNALFOOLPROOFDARKGREEN_CLASS
        | C::W_DYERAREETERNALFOOLPROOFDARKRED_CLASS
        | C::W_DYERAREETERNALFOOLPROOFDARKYELLOW_CLASS
        | C::W_DYERAREETERNALFOOLPROOFLIGHTBLUE_CLASS
        | C::W_DYERAREETERNALFOOLPROOFLIGHTGREEN_CLASS
        | C::W_DYERAREETERNALFOOLPROOFPURPLE_CLASS
        | C::W_DYERAREETERNALFOOLPROOFSILVER_CLASS => {
            // ensure item is armor/clothing and dyeable
            if t.biota.weenie_type != WeenieType::Clothing
                || !t.get_property(PropertyBool::Dyable).unwrap_or(false)
            {
                return None;
            }

            recipe = cached_recipe(w, 9068); // rare eternal dye recipe
        }

        C::W_MATERIALIVORY_CLASS | C::W_MATERIALRAREETERNALIVORY_CLASS => {
            // ensure item is ivoryable
            if !t.get_property(PropertyBool::Ivoryable).unwrap_or(false) {
                return None;
            }

            let recipe_id = if source_wcid == wcid(C::W_MATERIALRAREETERNALIVORY_CLASS) {
                9069
            } else {
                3977
            };

            recipe = cached_recipe(w, recipe_id);
        }

        C::W_MATERIALLEATHER_CLASS | C::W_MATERIALRAREETERNALLEATHER_CLASS => {
            // ensure item is not already retained, and is not stackable
            // and can either be salvaged, sold, or consumed with a mana stone
            if t.retained() || t.is_stackable() {
                return None;
            }

            if t.material_type().is_none() && !t.is_sellable() && t.item_max_mana().is_none() {
                return None;
            }

            let recipe_id = if source_wcid == wcid(C::W_MATERIALRAREETERNALLEATHER_CLASS) {
                9070
            } else {
                4426
            };

            recipe = cached_recipe(w, recipe_id);
        }

        C::W_MATERIALSANDSTONE_CLASS | C::W_MATERIALSANDSTONE100_CLASS => {
            // ensure item is retained and not stackable
            // and can either be salvaged, sold, or consumed with a mana stone
            if !t.retained() || t.is_stackable() {
                return None;
            }

            if t.material_type().is_none() && !t.is_sellable() && t.item_max_mana().is_none() {
                return None;
            }

            // use sandstone recipe as base
            recipe = cached_recipe(w, 8003);
        }

        C::W_MATERIALGOLD_CLASS => {
            // ensure item has value and workmanship
            if t.value().unwrap_or(0) == 0 || !has_workmanship(t) {
                return None;
            }

            // use gold recipe as base
            recipe = cached_recipe(w, 3851);
        }

        C::W_MATERIALLINEN_CLASS => {
            // ensure item has burden and workmanship
            if t.encumbrance_val().unwrap_or(0) == 0 || !has_workmanship(t) {
                return None;
            }

            // use linen recipe as base
            recipe = cached_recipe(w, 3854);
        }

        C::W_MATERIALMOONSTONE_CLASS => {
            // ensure item has mana and workmanship
            if t.item_max_mana().unwrap_or(0) == 0 || !has_workmanship(t) {
                return None;
            }

            // use moonstone recipe as base
            recipe = cached_recipe(w, 3978);
        }

        C::W_MATERIALPINE_CLASS => {
            // ensure item has value and workmanship
            if t.value().unwrap_or(0) == 0 || !has_workmanship(t) {
                return None;
            }

            // use pine recipe as base
            recipe = cached_recipe(w, 3858);
        }

        C::W_MATERIALIRON100_CLASS
        | C::W_MATERIALIRON_CLASS
        | C::W_MATERIALGRANITE100_CLASS
        | C::W_MATERIALGRANITE_CLASS
        | C::W_MATERIALGRANITEPATHWARDEN_CLASS
        | C::W_MATERIALVELVET100_CLASS
        | C::W_MATERIALVELVET_CLASS
        | C::W_LUCKYRABBITSFOOT_CLASS => {
            // ensure melee weapon and workmanship
            if t.biota.weenie_type != WeenieType::MeleeWeapon || !has_workmanship(t) {
                return None;
            }

            // grab correct recipe to use as base
            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_MATERIALMAHOGANY100_CLASS | C::W_MATERIALMAHOGANY_CLASS => {
            // ensure missile weapon and workmanship
            if t.biota.weenie_type != WeenieType::MissileLauncher || !has_workmanship(t) {
                return None;
            }

            // use mahogany recipe as base
            recipe = cached_recipe(w, 3855);
        }

        C::W_MATERIALOAK_CLASS => {
            // ensure melee or missile weapon, and workmanship
            if t.biota.weenie_type != WeenieType::MeleeWeapon
                && t.biota.weenie_type != WeenieType::MissileLauncher
                || !has_workmanship(t)
            {
                return None;
            }

            // use oak recipe as base
            recipe = cached_recipe(w, 3857);
        }

        C::W_MATERIALOPAL100_CLASS | C::W_MATERIALOPAL_CLASS => {
            // ensure item is caster and has workmanship
            if t.biota.weenie_type != WeenieType::Caster || !has_workmanship(t) {
                return None;
            }

            // use opal recipe as base
            recipe = cached_recipe(w, 3979);
        }

        C::W_MATERIALGREENGARNET100_CLASS | C::W_MATERIALGREENGARNET_CLASS => {
            // ensure item is caster and has workmanship
            if t.biota.weenie_type != WeenieType::Caster || !has_workmanship(t) {
                return None;
            }

            // use green garnet recipe as base
            recipe = cached_recipe(w, 5202);
        }

        C::W_MATERIALBRASS100_CLASS | C::W_MATERIALBRASS_CLASS => {
            // ensure item has workmanship
            if !has_workmanship(t) {
                return None;
            }

            // use brass recipe as base
            recipe = cached_recipe(w, 3848);
        }

        C::W_MATERIALROSEQUARTZ_CLASS
        | C::W_MATERIALREDJADE_CLASS
        | C::W_MATERIALMALACHITE_CLASS
        | C::W_MATERIALLAVENDERJADE_CLASS
        | C::W_MATERIALHEMATITE_CLASS
        | C::W_MATERIALBLOODSTONE_CLASS
        | C::W_MATERIALAZURITE_CLASS
        | C::W_MATERIALAGATE_CLASS
        | C::W_MATERIALSMOKYQUARTZ_CLASS
        | C::W_MATERIALCITRINE_CLASS
        | C::W_MATERIALCARNELIAN_CLASS => {
            // ensure item is generic (jewelry), and has workmanship
            if t.biota.weenie_type != WeenieType::Generic
                || !has_workmanship(t)
                || t.valid_locations() == Some(EquipMask::TrinketOne)
            {
                return None;
            }

            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_MATERIALSTEEL100_CLASS
        | C::W_MATERIALSTEEL_CLASS
        | C::W_MATERIALSTEELPATHWARDEN_CLASS
        | C::W_MATERIALALABASTER_CLASS
        | C::W_MATERIALBRONZE_CLASS
        | C::W_MATERIALMARBLE_CLASS
        | C::W_MATERIALARMOREDILLOHIDE_CLASS
        | C::W_MATERIALCERAMIC_CLASS
        | C::W_MATERIALWOOL_CLASS
        | C::W_MATERIALREEDSHARKHIDE_CLASS
        | C::W_MATERIALSILVER_CLASS
        | C::W_MATERIALCOPPER_CLASS => {
            // ensure loot-generated item w/ armor level
            if !has_workmanship(t) || !t.has_armor_level() {
                return None;
            }

            let allow_armor = t.item_type() == ItemType::Armor;

            // allow clothing that only covers an extremity
            // this excludes some clothing like boots and robes that cover extremities + non-extremities
            let vl = t.valid_locations();
            let allow_clothing = t.item_type() == ItemType::Clothing
                && (vl == Some(EquipMask::HeadWear)
                    || vl == Some(EquipMask::HandWear)
                    || vl == Some(EquipMask::FootWear));

            if !allow_armor && !allow_clothing {
                return None;
            }

            // TODO: replace with PropertyInt.MeleeDefenseImbuedEffectTypeCache == 1 when data is updated
            if s.material_type() == Some(MaterialType::Steel)
                && !crate::world_objects::world_object_weapon::is_enchantable(t)
            {
                return None;
            }

            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_MATERIALPERIDOT_CLASS
        | C::W_MATERIALYELLOWTOPAZ_CLASS
        | C::W_MATERIALZIRCON_CLASS
        | C::W_MATERIALRAREFOOLPROOFPERIDOT_CLASS
        | C::W_MATERIALRAREFOOLPROOFYELLOWTOPAZ_CLASS
        | C::W_MATERIALRAREFOOLPROOFZIRCON_CLASS
        | C::W_MATERIALACE36634FOOLPROOFPERIDOT
        | C::W_MATERIALACE36635FOOLPROOFYELLOWTOPAZ
        | C::W_MATERIALACE36636FOOLPROOFZIRCON => {
            // can be applied to anything with AL, including shields (according to base recipe)
            if !t.has_armor_level() || !has_workmanship(t) {
                return None;
            }

            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_MATERIALAQUAMARINE100_CLASS
        | C::W_MATERIALAQUAMARINE_CLASS
        | C::W_MATERIALBLACKGARNET100_CLASS
        | C::W_MATERIALBLACKGARNET_CLASS
        | C::W_MATERIALBLACKOPAL100_CLASS
        | C::W_MATERIALBLACKOPAL_CLASS
        | C::W_MATERIALEMERALD100_CLASS
        | C::W_MATERIALEMERALD_CLASS
        | C::W_MATERIALFIREOPAL100_CLASS
        | C::W_MATERIALFIREOPAL_CLASS
        | C::W_MATERIALIMPERIALTOPAZ100_CLASS
        | C::W_MATERIALIMPERIALTOPAZ_CLASS
        | C::W_MATERIALJET100_CLASS
        | C::W_MATERIALJET_CLASS
        | C::W_MATERIALREDGARNET100_CLASS
        | C::W_MATERIALREDGARNET_CLASS
        | C::W_MATERIALSUNSTONE100_CLASS
        | C::W_MATERIALSUNSTONE_CLASS
        | C::W_MATERIALWHITESAPPHIRE100_CLASS
        | C::W_MATERIALWHITESAPPHIRE_CLASS
        | C::W_LEFTHANDTETHER_CLASS
        | C::W_LEFTHANDTETHERREMOVER_CLASS
        | C::W_COREPLATINGINTEGRATOR_CLASS
        | C::W_COREPLATINGDISINTEGRATOR_CLASS
        | C::W_MATERIALRAREFOOLPROOFAQUAMARINE_CLASS
        | C::W_MATERIALRAREFOOLPROOFBLACKGARNET_CLASS
        | C::W_MATERIALRAREFOOLPROOFBLACKOPAL_CLASS
        | C::W_MATERIALRAREFOOLPROOFEMERALD_CLASS
        | C::W_MATERIALRAREFOOLPROOFFIREOPAL_CLASS
        | C::W_MATERIALRAREFOOLPROOFIMPERIALTOPAZ_CLASS
        | C::W_MATERIALRAREFOOLPROOFJET_CLASS
        | C::W_MATERIALRAREFOOLPROOFREDGARNET_CLASS
        | C::W_MATERIALRAREFOOLPROOFSUNSTONE_CLASS
        | C::W_MATERIALRAREFOOLPROOFWHITESAPPHIRE_CLASS
        | C::W_MATERIALACE36619FOOLPROOFAQUAMARINE
        | C::W_MATERIALACE36620FOOLPROOFBLACKGARNET
        | C::W_MATERIALACE36621FOOLPROOFBLACKOPAL
        | C::W_MATERIALACE36622FOOLPROOFEMERALD
        | C::W_MATERIALACE36623FOOLPROOFFIREOPAL
        | C::W_MATERIALACE36624FOOLPROOFIMPERIALTOPAZ
        | C::W_MATERIALACE36625FOOLPROOFJET
        | C::W_MATERIALACE36626FOOLPROOFREDGARNET
        | C::W_MATERIALACE36627FOOLPROOFSUNSTONE
        | C::W_MATERIALACE36628FOOLPROOFWHITESAPPHIRE => {
            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_CELESTIALHANDSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODSHIELDCOVER_CLASS
        | C::W_CELESTIALHANDBUCKLERSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBBUCKLERSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODBUCKLERSHIELDCOVER_CLASS
        | C::W_CELESTIALHANDCOVENANTSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBCOVENANTSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODCOVENANTSHIELDCOVER_CLASS
        | C::W_CELESTIALHANDKITESHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBKITESHIELDCOVER_CLASS
        | C::W_CELESTIALHANDLARGEKITESHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBLARGEKITESHIELDCOVER_CLASS
        | C::W_RADIANTBLOODLARGEKITESHIELDCOVER_CLASS
        | C::W_RADIANTBLOODKITESHIELDCOVER_CLASS
        | C::W_CELESTIALHANDOLTHOISHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBOLTHOISHIELDCOVER_CLASS
        | C::W_RADIANTBLOODOLTHOISHIELDCOVER_CLASS
        | C::W_CELESTIALHANDROUNDSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBROUNDSHIELDCOVER_CLASS
        | C::W_CELESTIALHANDLARGEROUNDSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBLARGEROUNDSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODLARGEROUNDSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODROUNDSHIELDCOVER_CLASS
        | C::W_CELESTIALHANDTOWERSHIELDCOVER_CLASS
        | C::W_ELDRYTCHWEBTOWERSHIELDCOVER_CLASS
        | C::W_RADIANTBLOODTOWERSHIELDCOVER_CLASS => {
            // ensure target is a shield
            if t.biota.weenie_type != WeenieType::Generic
                || t.item_type() != ItemType::Armor
                || !t.is_shield()
            {
                return None;
            }

            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_GREATERMUKKIRSLAYERSTONE_CLASS
        | C::W_BLACKSKULLOFXIKMA_CLASS
        | C::W_SPECTRALSKULL_CLASS
        | C::W_ANEKSHAYSLAYERSTONE_CLASS => {
            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_LUMINOUSAMBEROFTHE1STTIERPARAGON_CLASS => match t.biota.weenie_type {
            WeenieType::Caster => recipe = cached_recipe(w, 8700),

            WeenieType::MeleeWeapon => recipe = cached_recipe(w, 8701),

            WeenieType::MissileLauncher => recipe = cached_recipe(w, 8699),

            _ => return None,
        },

        C::W_LUMINOUSAMBEROFTHE2NDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE3RDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE4THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE5THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE6THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE7THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE8THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE9THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE10THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE11THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE12THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE13THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE14THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE15THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE16THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE17THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE18THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE19THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE20THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE21STTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE22NDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE23RDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE24THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE25THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE26THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE27THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE28THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE29THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE30THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE31STTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE32NDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE33RDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE34THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE35THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE36THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE37THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE38THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE39THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE40THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE41STTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE42NDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE43RDTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE44THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE45THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE46THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE47THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE48THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE49THTIERPARAGON_CLASS
        | C::W_LUMINOUSAMBEROFTHE50THTIERPARAGON_CLASS => {
            recipe = cached_recipe(w, source_to_recipe(source_class));
        }

        C::W_UNINSCRIPTIONSTONE_CLASS => {
            /* Skip this for check in favor of second version which is closer to intent of stone i think.
            // ensure workmanship and weenie type
            if (target.Workmanship == null
                || target.WeenieType != WeenieType.MeleeWeapon || target.WeenieType != WeenieType.MissileLauncher || target.WeenieType != WeenieType.Caster
                || target.WeenieType != WeenieType.Clothing)
            */

            // check for base weenie for an inscription, if it exists, you cannot uninscribe the item.
            // `GetCachedWeenie(wcid)?.PropertiesString?.TryGetValue(Inscription, out inscription)`: null
            // without a weenie or string properties, else whether it has an inscription.
            let base_weenie = w.content.get_cached_weenie(t.biota.weenie_class_id);
            let strings = base_weenie
                .as_ref()
                .and_then(|b| b.properties_string.as_ref());
            let inscription = strings
                .and_then(|p| p.get(&PropertyString::Inscription))
                .cloned()
                .unwrap_or_default();

            if strings.is_none() || !inscription.trim_matches(char::is_whitespace).is_empty() {
                return None;
            }

            recipe = cached_recipe(w, 9133);
        }

        _ => recipe = None,
    }

    recipe
}

// ACE: RecipeManager.SourceToRecipe
/// The base recipe of each source class that uses one (a `public static Dictionary` nothing
/// modifies).
pub static SOURCE_TO_RECIPE: LazyLock<DotNetDict<C, u32>> = LazyLock::new(|| {
    let mut d = DotNetDict::new();

    d.add(C::W_MATERIALIRON100_CLASS, 3853);
    d.add(C::W_MATERIALIRON_CLASS, 3853);
    d.add(C::W_MATERIALGRANITE100_CLASS, 3852);
    d.add(C::W_MATERIALGRANITE_CLASS, 3852);
    d.add(C::W_MATERIALGRANITEPATHWARDEN_CLASS, 3852);
    d.add(C::W_LUCKYRABBITSFOOT_CLASS, 8751);

    d.add(C::W_MATERIALVELVET100_CLASS, 3861);
    d.add(C::W_MATERIALVELVET_CLASS, 3861);

    d.add(C::W_MATERIALROSEQUARTZ_CLASS, 4446);
    d.add(C::W_MATERIALREDJADE_CLASS, 4442);
    d.add(C::W_MATERIALMALACHITE_CLASS, 4438);
    d.add(C::W_MATERIALLAVENDERJADE_CLASS, 4441);
    d.add(C::W_MATERIALHEMATITE_CLASS, 4440);
    d.add(C::W_MATERIALBLOODSTONE_CLASS, 4448);
    d.add(C::W_MATERIALAZURITE_CLASS, 4437);
    d.add(C::W_MATERIALAGATE_CLASS, 4445);
    d.add(C::W_MATERIALSMOKYQUARTZ_CLASS, 4447);
    d.add(C::W_MATERIALCITRINE_CLASS, 4439);
    d.add(C::W_MATERIALCARNELIAN_CLASS, 4443);

    // d.add(C::W_MATERIALSTEEL50_CLASS, 3860);
    d.add(C::W_MATERIALSTEEL100_CLASS, 3860);
    d.add(C::W_MATERIALSTEEL_CLASS, 3860);
    d.add(C::W_MATERIALSTEELPATHWARDEN_CLASS, 3860);

    d.add(C::W_MATERIALALABASTER_CLASS, 3846);
    d.add(C::W_MATERIALBRONZE_CLASS, 3849);
    d.add(C::W_MATERIALMARBLE_CLASS, 3856);
    d.add(C::W_MATERIALARMOREDILLOHIDE_CLASS, 3847);
    d.add(C::W_MATERIALCERAMIC_CLASS, 3850);
    d.add(C::W_MATERIALWOOL_CLASS, 3862);
    d.add(C::W_MATERIALREEDSHARKHIDE_CLASS, 3859);
    d.add(C::W_MATERIALSILVER_CLASS, 4427);
    d.add(C::W_MATERIALCOPPER_CLASS, 4428);

    d.add(C::W_MATERIALPERIDOT_CLASS, 4435);
    d.add(C::W_MATERIALYELLOWTOPAZ_CLASS, 4434);
    d.add(C::W_MATERIALZIRCON_CLASS, 4433);

    d.add(C::W_MATERIALRAREFOOLPROOFPERIDOT_CLASS, 8016);
    d.add(C::W_MATERIALACE36634FOOLPROOFPERIDOT, 8016);
    d.add(C::W_MATERIALRAREFOOLPROOFYELLOWTOPAZ_CLASS, 8015);
    d.add(C::W_MATERIALACE36635FOOLPROOFYELLOWTOPAZ, 8015);
    d.add(C::W_MATERIALRAREFOOLPROOFZIRCON_CLASS, 8014);
    d.add(C::W_MATERIALACE36636FOOLPROOFZIRCON, 8014);

    d.add(C::W_MATERIALAQUAMARINE100_CLASS, 4436);
    d.add(C::W_MATERIALAQUAMARINE_CLASS, 4436);
    d.add(C::W_MATERIALBLACKGARNET100_CLASS, 4449);
    d.add(C::W_MATERIALBLACKGARNET_CLASS, 4449);
    d.add(C::W_MATERIALBLACKOPAL100_CLASS, 3863);
    d.add(C::W_MATERIALBLACKOPAL_CLASS, 3863);
    d.add(C::W_MATERIALEMERALD100_CLASS, 4450);
    d.add(C::W_MATERIALEMERALD_CLASS, 4450);
    d.add(C::W_MATERIALFIREOPAL100_CLASS, 3864);
    d.add(C::W_MATERIALFIREOPAL_CLASS, 3864);
    d.add(C::W_MATERIALIMPERIALTOPAZ100_CLASS, 4454);
    d.add(C::W_MATERIALIMPERIALTOPAZ_CLASS, 4454);
    d.add(C::W_MATERIALJET100_CLASS, 4451);
    d.add(C::W_MATERIALJET_CLASS, 4451);
    d.add(C::W_MATERIALREDGARNET100_CLASS, 4452);
    d.add(C::W_MATERIALREDGARNET_CLASS, 4452);
    d.add(C::W_MATERIALSUNSTONE100_CLASS, 3865);
    d.add(C::W_MATERIALSUNSTONE_CLASS, 3865);
    d.add(C::W_MATERIALWHITESAPPHIRE100_CLASS, 4453);
    d.add(C::W_MATERIALWHITESAPPHIRE_CLASS, 4453);

    d.add(C::W_MATERIALRAREFOOLPROOFAQUAMARINE_CLASS, 8004);
    d.add(C::W_MATERIALRAREFOOLPROOFBLACKGARNET_CLASS, 8005);
    d.add(C::W_MATERIALRAREFOOLPROOFBLACKOPAL_CLASS, 8011);
    d.add(C::W_MATERIALRAREFOOLPROOFEMERALD_CLASS, 8006);
    d.add(C::W_MATERIALRAREFOOLPROOFFIREOPAL_CLASS, 8012);
    d.add(C::W_MATERIALRAREFOOLPROOFIMPERIALTOPAZ_CLASS, 8010);
    d.add(C::W_MATERIALRAREFOOLPROOFJET_CLASS, 8007);
    d.add(C::W_MATERIALRAREFOOLPROOFREDGARNET_CLASS, 8008);
    d.add(C::W_MATERIALRAREFOOLPROOFSUNSTONE_CLASS, 8013);
    d.add(C::W_MATERIALRAREFOOLPROOFWHITESAPPHIRE_CLASS, 8009);

    d.add(C::W_MATERIALACE36619FOOLPROOFAQUAMARINE, 8004);
    d.add(C::W_MATERIALACE36620FOOLPROOFBLACKGARNET, 8005);
    d.add(C::W_MATERIALACE36621FOOLPROOFBLACKOPAL, 8011);
    d.add(C::W_MATERIALACE36622FOOLPROOFEMERALD, 8006);
    d.add(C::W_MATERIALACE36623FOOLPROOFFIREOPAL, 8012);
    d.add(C::W_MATERIALACE36624FOOLPROOFIMPERIALTOPAZ, 8010);
    d.add(C::W_MATERIALACE36625FOOLPROOFJET, 8007);
    d.add(C::W_MATERIALACE36626FOOLPROOFREDGARNET, 8008);
    d.add(C::W_MATERIALACE36627FOOLPROOFSUNSTONE, 8013);
    d.add(C::W_MATERIALACE36628FOOLPROOFWHITESAPPHIRE, 8009);

    d.add(C::W_LEFTHANDTETHER_CLASS, 6798);
    d.add(C::W_LEFTHANDTETHERREMOVER_CLASS, 6799);

    d.add(C::W_COREPLATINGINTEGRATOR_CLASS, 6800);
    d.add(C::W_COREPLATINGDISINTEGRATOR_CLASS, 6801);

    d.add(C::W_CELESTIALHANDSHIELDCOVER_CLASS, 8337);
    d.add(C::W_ELDRYTCHWEBSHIELDCOVER_CLASS, 8338);
    d.add(C::W_RADIANTBLOODSHIELDCOVER_CLASS, 8339);
    d.add(C::W_CELESTIALHANDBUCKLERSHIELDCOVER_CLASS, 8313);
    d.add(C::W_ELDRYTCHWEBBUCKLERSHIELDCOVER_CLASS, 8314);
    d.add(C::W_RADIANTBLOODBUCKLERSHIELDCOVER_CLASS, 8315);
    d.add(C::W_CELESTIALHANDCOVENANTSHIELDCOVER_CLASS, 8316);
    d.add(C::W_ELDRYTCHWEBCOVENANTSHIELDCOVER_CLASS, 8317);
    d.add(C::W_RADIANTBLOODCOVENANTSHIELDCOVER_CLASS, 8318);
    d.add(C::W_CELESTIALHANDKITESHIELDCOVER_CLASS, 8319);
    d.add(C::W_ELDRYTCHWEBKITESHIELDCOVER_CLASS, 8320);
    d.add(C::W_CELESTIALHANDLARGEKITESHIELDCOVER_CLASS, 8322);
    d.add(C::W_ELDRYTCHWEBLARGEKITESHIELDCOVER_CLASS, 8323);
    d.add(C::W_RADIANTBLOODLARGEKITESHIELDCOVER_CLASS, 8324);
    d.add(C::W_RADIANTBLOODKITESHIELDCOVER_CLASS, 8321);
    d.add(C::W_CELESTIALHANDOLTHOISHIELDCOVER_CLASS, 8325);
    d.add(C::W_ELDRYTCHWEBOLTHOISHIELDCOVER_CLASS, 8326);
    d.add(C::W_RADIANTBLOODOLTHOISHIELDCOVER_CLASS, 8327);
    d.add(C::W_CELESTIALHANDROUNDSHIELDCOVER_CLASS, 8328);
    d.add(C::W_ELDRYTCHWEBROUNDSHIELDCOVER_CLASS, 8329);
    d.add(C::W_CELESTIALHANDLARGEROUNDSHIELDCOVER_CLASS, 8331);
    d.add(C::W_ELDRYTCHWEBLARGEROUNDSHIELDCOVER_CLASS, 8332);
    d.add(C::W_RADIANTBLOODLARGEROUNDSHIELDCOVER_CLASS, 8333);
    d.add(C::W_RADIANTBLOODROUNDSHIELDCOVER_CLASS, 8330);
    d.add(C::W_CELESTIALHANDTOWERSHIELDCOVER_CLASS, 8334);
    d.add(C::W_ELDRYTCHWEBTOWERSHIELDCOVER_CLASS, 8335);
    d.add(C::W_RADIANTBLOODTOWERSHIELDCOVER_CLASS, 8336);

    d.add(C::W_GREATERMUKKIRSLAYERSTONE_CLASS, 8752);
    d.add(C::W_BLACKSKULLOFXIKMA_CLASS, 8753);
    d.add(C::W_SPECTRALSKULL_CLASS, 8754);
    d.add(C::W_ANEKSHAYSLAYERSTONE_CLASS, 8755);

    d.add(C::W_LUMINOUSAMBEROFTHE2NDTIERPARAGON_CLASS, 8702);
    d.add(C::W_LUMINOUSAMBEROFTHE3RDTIERPARAGON_CLASS, 8703);
    d.add(C::W_LUMINOUSAMBEROFTHE4THTIERPARAGON_CLASS, 8704);
    d.add(C::W_LUMINOUSAMBEROFTHE5THTIERPARAGON_CLASS, 8705);
    d.add(C::W_LUMINOUSAMBEROFTHE6THTIERPARAGON_CLASS, 8706);
    d.add(C::W_LUMINOUSAMBEROFTHE7THTIERPARAGON_CLASS, 8707);
    d.add(C::W_LUMINOUSAMBEROFTHE8THTIERPARAGON_CLASS, 8708);
    d.add(C::W_LUMINOUSAMBEROFTHE9THTIERPARAGON_CLASS, 8709);
    d.add(C::W_LUMINOUSAMBEROFTHE10THTIERPARAGON_CLASS, 8710);
    d.add(C::W_LUMINOUSAMBEROFTHE11THTIERPARAGON_CLASS, 8711);
    d.add(C::W_LUMINOUSAMBEROFTHE12THTIERPARAGON_CLASS, 8712);
    d.add(C::W_LUMINOUSAMBEROFTHE13THTIERPARAGON_CLASS, 8713);
    d.add(C::W_LUMINOUSAMBEROFTHE14THTIERPARAGON_CLASS, 8714);
    d.add(C::W_LUMINOUSAMBEROFTHE15THTIERPARAGON_CLASS, 8715);
    d.add(C::W_LUMINOUSAMBEROFTHE16THTIERPARAGON_CLASS, 8716);
    d.add(C::W_LUMINOUSAMBEROFTHE17THTIERPARAGON_CLASS, 8717);
    d.add(C::W_LUMINOUSAMBEROFTHE18THTIERPARAGON_CLASS, 8718);
    d.add(C::W_LUMINOUSAMBEROFTHE19THTIERPARAGON_CLASS, 8719);
    d.add(C::W_LUMINOUSAMBEROFTHE20THTIERPARAGON_CLASS, 8720);
    d.add(C::W_LUMINOUSAMBEROFTHE21STTIERPARAGON_CLASS, 8721);
    d.add(C::W_LUMINOUSAMBEROFTHE22NDTIERPARAGON_CLASS, 8722);
    d.add(C::W_LUMINOUSAMBEROFTHE23RDTIERPARAGON_CLASS, 8723);
    d.add(C::W_LUMINOUSAMBEROFTHE24THTIERPARAGON_CLASS, 8724);
    d.add(C::W_LUMINOUSAMBEROFTHE25THTIERPARAGON_CLASS, 8725);
    d.add(C::W_LUMINOUSAMBEROFTHE26THTIERPARAGON_CLASS, 8726);
    d.add(C::W_LUMINOUSAMBEROFTHE27THTIERPARAGON_CLASS, 8727);
    d.add(C::W_LUMINOUSAMBEROFTHE28THTIERPARAGON_CLASS, 8728);
    d.add(C::W_LUMINOUSAMBEROFTHE29THTIERPARAGON_CLASS, 8729);
    d.add(C::W_LUMINOUSAMBEROFTHE30THTIERPARAGON_CLASS, 8730);
    d.add(C::W_LUMINOUSAMBEROFTHE31STTIERPARAGON_CLASS, 8731);
    d.add(C::W_LUMINOUSAMBEROFTHE32NDTIERPARAGON_CLASS, 8732);
    d.add(C::W_LUMINOUSAMBEROFTHE33RDTIERPARAGON_CLASS, 8733);
    d.add(C::W_LUMINOUSAMBEROFTHE34THTIERPARAGON_CLASS, 8734);
    d.add(C::W_LUMINOUSAMBEROFTHE35THTIERPARAGON_CLASS, 8735);
    d.add(C::W_LUMINOUSAMBEROFTHE36THTIERPARAGON_CLASS, 8736);
    d.add(C::W_LUMINOUSAMBEROFTHE37THTIERPARAGON_CLASS, 8737);
    d.add(C::W_LUMINOUSAMBEROFTHE38THTIERPARAGON_CLASS, 8738);
    d.add(C::W_LUMINOUSAMBEROFTHE39THTIERPARAGON_CLASS, 8739);
    d.add(C::W_LUMINOUSAMBEROFTHE40THTIERPARAGON_CLASS, 8740);
    d.add(C::W_LUMINOUSAMBEROFTHE41STTIERPARAGON_CLASS, 8741);
    d.add(C::W_LUMINOUSAMBEROFTHE42NDTIERPARAGON_CLASS, 8742);
    d.add(C::W_LUMINOUSAMBEROFTHE43RDTIERPARAGON_CLASS, 8743);
    d.add(C::W_LUMINOUSAMBEROFTHE44THTIERPARAGON_CLASS, 8744);
    d.add(C::W_LUMINOUSAMBEROFTHE45THTIERPARAGON_CLASS, 8745);
    d.add(C::W_LUMINOUSAMBEROFTHE46THTIERPARAGON_CLASS, 8746);
    d.add(C::W_LUMINOUSAMBEROFTHE47THTIERPARAGON_CLASS, 8747);
    d.add(C::W_LUMINOUSAMBEROFTHE48THTIERPARAGON_CLASS, 8748);
    d.add(C::W_LUMINOUSAMBEROFTHE49THTIERPARAGON_CLASS, 8749);
    d.add(C::W_LUMINOUSAMBEROFTHE50THTIERPARAGON_CLASS, 8750);
    d
});
