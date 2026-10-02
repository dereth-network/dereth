// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/PlayerFactory.cs
//! Port of `Source/ACE.Server/Factories/PlayerFactory.cs`.
//!
//! `PlayerFactory.Create` builds a new character's `Player` from the client's
//! `CharacterCreateInfo`, the portal dat's `CharGen` and `SkillTable`, and the starter gear
//! configuration. ACE's result is a `Player` object that never enters the world: the character
//! handler saves its biota, its possessions and its `Character` row, then drops it.
//!
//! # The new player outside the world
//!
//! Every member `Create` calls on the new player is the real port (`Player.TrainSkill`,
//! `SpecializeSkill`, `UntrainSkill`, `AddKnownSpell`, `AddTitle`, `SetCharacterOption`,
//! `HandleActionAddSpellFavorite`, `Name`, `CreatureVital.Base`, `Container.TryAddToInventory`,
//! `Creature.TryEquipObject`, `Player.GetAllPossessions`, ...). Those members resolve the player
//! and its possessions through `World.objects`, while ACE's new player is in no landblock, so each
//! call stages the player and its possessions in the store for its duration
//! ([`with_player_in_world`]); between calls they live in [`CreatedPlayer`]. The constructor
//! (`Player(Weenie, ObjectGuid, uint accountId)`) makes the `Character`, the vitals and attributes
//! (starting at max) and `IsOlthoiPlayer`.

use std::sync::Arc;

use empyrean_common::dotnet::CsCast;
use empyrean_common::era::{StartPositions, TownStart};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_dat::file_types::palette_set::PaletteSetExt;
use empyrean_dat::file_types::sex_cg::SexCgExt;
use empyrean_dat::file_types::PaletteSet;
use empyrean_dat::AceThrow;
use empyrean_entity::enums::{
    AttunedStatus, BondedStatus, CharacterOption, CloakStatus, EquipMask, HeritageGroup,
    PlayerKillerStatus, PositionType, PropertyAttribute, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass, WeaponType,
    WeenieType,
};
use empyrean_entity::{CharacterCreateInfo, ObjectGuid, Position, Weenie};
use empyrean_store::models::shard::Character;

use crate::dispatch::Class;
use crate::factories::starter_gear_factory;
use crate::factories::world_object_factory;
use crate::managers::guid_manager;
use crate::world_objects::player;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::{container, creature_equipment, player_skills, player_spells};
use crate::World;

/// ACE's `PlayerFactory.CreateResult`.
// ACE: PlayerFactory.CreateResult
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateResult {
    Success,
    TooManySkillCreditsUsed,
    InvalidSkillRequested,
    FailedToTrainSkill,
    FailedToSpecializeSkill,
    ClientServerSkillsMismatch,
    /// Not ACE's: a template number outside the heritage's list (a fix, V243). The handler answers it
    /// `Corrupt`, as every other failure but the skills mismatch.
    InvalidTemplate,
}

impl CreateResult {
    /// The C# enum member name (`ToString()`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            CreateResult::Success => "Success",
            CreateResult::TooManySkillCreditsUsed => "TooManySkillCreditsUsed",
            CreateResult::InvalidSkillRequested => "InvalidSkillRequested",
            CreateResult::FailedToTrainSkill => "FailedToTrainSkill",
            CreateResult::FailedToSpecializeSkill => "FailedToSpecializeSkill",
            CreateResult::ClientServerSkillsMismatch => "ClientServerSkillsMismatch",
            CreateResult::InvalidTemplate => "InvalidTemplate",
        }
    }
}

/// The `Player` object `PlayerFactory.Create` hands back (`out Player player`), with the members
/// of ACE's `Player` that `WorldObject` does not carry yet (SHIM, see the module docs).
///
/// It never joins `World.objects`: ACE's new player is not in any landblock either, only referenced
/// by the save callback, and dropped after it.
#[derive(Debug)]
pub struct CreatedPlayer {
    /// The `Player` (or `Sentinel`/`Admin`) world object.
    pub player: WorldObject,
    /// The objects of the player's `Container.Inventory`, in its enumeration order (the player's
    /// `ContainerFields::inventory` holds their guids).
    pub inventory: Vec<WorldObject>,
    /// The objects of the player's `Creature.EquippedObjects`, in its enumeration order.
    pub equipped_objects: Vec<WorldObject>,
}

impl CreatedPlayer {
    /// The new player's `Character` (`Player.Character`, made by the weenie constructor).
    ///
    /// # Panics
    /// When the object is not a player with a `Character`.
    #[must_use]
    pub fn character(&self) -> &Character {
        self.player
            .player
            .as_ref()
            .and_then(|p| p.player.character.as_ref())
            .expect("ACE: Player.Character is null (NullReferenceException)")
    }

    /// The new player's `Character`, to change.
    ///
    /// # Panics
    /// When the object is not a player with a `Character`.
    pub fn character_mut(&mut self) -> &mut Character {
        self.player
            .player
            .as_mut()
            .and_then(|p| p.player.character.as_mut())
            .expect("ACE: Player.Character is null (NullReferenceException)")
    }

    /// `Player.IsOlthoiPlayer`, as `Player.SetEphemeralValues` set it during construction (from
    /// the weenie's heritage, before `Create` sets the requested one).
    #[must_use]
    pub fn is_olthoi_player(&self) -> bool {
        self.player
            .player
            .as_ref()
            .is_some_and(|p| p.player_properties.is_olthoi_player)
    }

    /// `Player.GetAllPossessions()` (Player_Inventory.cs) over the staged player: inventory, then
    /// the contents of each side container in inventory order, then the equipped items.
    pub fn get_all_possessions(&mut self, w: &mut World) -> Vec<&WorldObject> {
        let guids = with_player_in_world(w, self, None, |w, player| {
            crate::world_objects::player_inventory::get_all_possessions(w, player)
        });
        guids
            .iter()
            .filter_map(|g| {
                self.inventory
                    .iter()
                    .chain(self.equipped_objects.iter())
                    .find(|o| o.guid == *g)
            })
            .collect()
    }
}

/// A C# exception thrown inside `Create`: ACE's inbound handler catches and logs it, and no
/// response is sent. Here it unwinds to the handler's `catch_unwind`.
fn throw(e: &AceThrow) -> ! {
    panic!("{e}")
}

/// `list[(int)index]`: .NET's `ArgumentOutOfRangeException` past the end (and for an index above
/// `int.MaxValue`, which the `(int)` cast makes negative).
fn index<T>(list: &[T], i: u32) -> &T {
    let Ok(i) = i32::try_from(i) else {
        throw(&AceThrow::IndexOutOfRange(u64::from(i)))
    };
    usize::try_from(i)
        .ok()
        .and_then(|i| list.get(i))
        .unwrap_or_else(|| throw(&AceThrow::IndexOutOfRange(i.cs_cast())))
}

/// `PropertyManager.GetBool(key).Item`. Shared with the character handler's `CharacterCreateEx`.
pub(crate) fn property_manager_get_bool(w: &World, key: &str) -> bool {
    crate::managers::property_manager::get_bool(w, key, false, true).item
}

/// `PropertyManager.GetDouble(key).Item`.
fn property_manager_get_double(w: &World, key: &str) -> f64 {
    crate::managers::property_manager::get_double(w, key, 0.0, true).item
}

/// `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)`: the cached weenie, a new dynamic
/// guid, construction; the guid is recycled when construction fails.
pub(crate) fn create_new_world_object_by_wcid(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<WorldObject> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;

    create_new_world_object(w, weenie)
}

/// `WorldObjectFactory.CreateNewWorldObject(Weenie weenie)`.
pub(crate) fn create_new_world_object(w: &mut World, weenie: Arc<Weenie>) -> Option<WorldObject> {
    let guid = guid_manager::new_dynamic_guid(w);

    let world_object = CtorEnv::with_world(w, |env| {
        world_object_factory::create_world_object(env, Some(weenie), guid)
    });

    if world_object.is_none() {
        guid_manager::recycle_dynamic_guid(w, guid);
    }

    world_object
}

// ACE: PlayerFactory.Create
/// Creates the character. `player` is always returned, as ACE's `out Player player` is assigned
/// before the first check; the result says whether it is complete.
///
/// # Panics
/// Where ACE throws: an unknown heritage or gender, a style or colour index past its `CharGen`
/// list, an unknown template or start area (the handler's catch-log-continue, no response).
#[allow(clippy::too_many_lines, clippy::if_same_then_else)] // the two spell arms are ACE's
pub fn create(
    w: &mut World,
    character_create_info: &CharacterCreateInfo,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
    account_id: u32,
    weenie_type: WeenieType,
) -> (CreateResult, CreatedPlayer) {
    let dats = Arc::clone(&w.dats);
    let char_gen = dats.portal_dat().char_gen();
    let heritage_key: u32 = character_create_info.heritage.0.cs_cast();
    let heritage_group = char_gen
        .heritage_groups
        .get(&heritage_key)
        .unwrap_or_else(|| throw(&AceThrow::KeyNotFound(heritage_key)));

    let class = if weenie_type == WeenieType::Admin {
        Class::Admin
    } else if weenie_type == WeenieType::Sentinel {
        Class::Sentinel
    } else {
        Class::Player
    };
    let player_object = CtorEnv::with_world(w, |env| {
        player::player_from_weenie(env, class, weenie, guid, account_id)
    });
    let mut p = CreatedPlayer {
        player: player_object,
        inventory: Vec::new(),
        equipped_objects: Vec::new(),
    };
    // the constructor's `GenerateContainList()` for an Olthoi player (it needs the world)
    if p.player
        .container
        .as_ref()
        .is_some_and(|c| c.container.generate_contain_list_pending)
    {
        with_player_in_world(
            w,
            &mut p,
            None,
            container::post_insert_generate_contain_list,
        );
    }

    p.player
        .set_property(PropertyInt::HeritageGroup, character_create_info.heritage.0);
    p.player
        .set_property(PropertyString::HeritageGroup, heritage_group.name.clone());
    p.player
        .set_property(PropertyInt::Gender, character_create_info.gender.cs_cast());
    p.player.set_property(
        PropertyString::Sex,
        if character_create_info.gender == 1 {
            "Male"
        } else {
            "Female"
        }
        .to_owned(),
    );

    //player.SetProperty(PropertyDataId.Icon, cgh.IconImage); // I don't believe this is used anywhere in the client, but it might be used by a future custom launcher

    // pull character data from the dat file
    let gender_key = character_create_info.gender;
    let sex = heritage_group
        .sexes
        .get(&gender_key)
        .unwrap_or_else(|| throw(&AceThrow::KeyNotFound(gender_key)));
    let appearance = &character_create_info.appearance;

    p.player
        .set_property(PropertyDataId::MotionTable, sex.motion_table.0);
    p.player
        .set_property(PropertyDataId::SoundTable, sex.sound_table.0);
    p.player
        .set_property(PropertyDataId::PhysicsEffectTable, sex.physics_table.0);
    p.player.set_property(PropertyDataId::Setup, sex.setup.0);
    p.player
        .set_property(PropertyDataId::PaletteBase, sex.base_palette.0);
    p.player
        .set_property(PropertyDataId::CombatTable, sex.combat_table.0);

    // Check the character scale
    if sex.scale != 100 {
        #[allow(clippy::cast_precision_loss)] // C#'s uint / float
        let scale = sex.scale as f32 / 100.0_f32;
        p.player
            .set_property(PropertyFloat::DefaultScale, f64::from(scale)); // Scale is stored as a percentage
    }

    // Get the hair first, because we need to know if you're bald, and that's the name of that tune!
    let hairstyle = index(&sex.hair_styles, appearance.hair_style);

    // Olthoi and Gear Knights have a "Body Style" instead of a hair style. These styles have multiple model/texture changes, instead of a single head/hairstyle.
    // Storing this value allows us to send the proper appearance ObjDesc
    if hairstyle.objdesc.anim_part_changes.len() > 1 {
        p.player
            .set_property(PropertyInt::Hairstyle, appearance.hair_style.cs_cast());
    }

    // Certain races (Undead, Tumeroks, Others?) have multiple body styles available. This is controlled via the "hair style".
    if hairstyle.alternate_setup.0 > 0 {
        p.player
            .set_property(PropertyDataId::Setup, hairstyle.alternate_setup.0);
    }

    let bald = hairstyle.bald != 0;
    let or_throw = |r: Result<u32, AceThrow>| r.unwrap_or_else(|e| throw(&e));
    p.player.set_property(
        PropertyDataId::EyesTexture,
        or_throw(sex.get_eye_texture(appearance.eyes, bald)),
    );
    p.player.set_property(
        PropertyDataId::DefaultEyesTexture,
        or_throw(sex.get_default_eye_texture(appearance.eyes, bald)),
    );
    p.player.set_property(
        PropertyDataId::NoseTexture,
        or_throw(sex.get_nose_texture(appearance.nose)),
    );
    p.player.set_property(
        PropertyDataId::DefaultNoseTexture,
        or_throw(sex.get_default_nose_texture(appearance.nose)),
    );
    p.player.set_property(
        PropertyDataId::MouthTexture,
        or_throw(sex.get_mouth_texture(appearance.mouth)),
    );
    p.player.set_property(
        PropertyDataId::DefaultMouthTexture,
        or_throw(sex.get_default_mouth_texture(appearance.mouth)),
    );
    p.character_mut().hair_texture = sex
        .get_hair_texture(appearance.hair_style)
        .unwrap_or_else(|e| throw(&e))
        .unwrap_or(0);
    p.character_mut().default_hair_texture = sex
        .get_default_hair_texture(appearance.hair_style)
        .unwrap_or_else(|e| throw(&e))
        .unwrap_or(0);
    // HeadObject can be null if we're dealing with GearKnight or Olthoi
    let head_object = sex
        .get_head_object(appearance.hair_style)
        .unwrap_or_else(|e| throw(&e));
    if let Some(head_object) = head_object {
        p.player
            .set_property(PropertyDataId::HeadObject, head_object);
    }

    // Skin is stored as PaletteSet (list of Palettes), so we need to read in the set to get the specific palette
    // (ReadFromDat answers an empty PaletteSet for a missing file, whose GetPaletteID is 0.)
    let skin_pal_set = dats
        .portal_dat()
        .read_from_dat::<PaletteSet>(sex.skin_palset.0);
    p.player.set_property(
        PropertyDataId::SkinPalette,
        skin_pal_set.map_or(0, |s| s.get_palette_id(appearance.skin_hue)),
    );
    p.player
        .set_property(PropertyFloat::Shade, appearance.skin_hue);

    // Hair is stored as PaletteSet (list of Palettes), so we need to read in the set to get the specific palette
    let hair_pal_set = dats
        .portal_dat()
        .read_from_dat::<PaletteSet>(*index(&sex.hair_colors, appearance.hair_color));
    p.player.set_property(
        PropertyDataId::HairPalette,
        hair_pal_set.map_or(0, |s| s.get_palette_id(appearance.hair_hue)),
    );

    // Eye Color
    p.player.set_property(
        PropertyDataId::EyesPalette,
        *index(&sex.eye_colors, appearance.eye_color),
    );

    // skip over this for olthoi, use the weenie defaults
    if !p.is_olthoi_player() {
        if p.player.get_property(PropertyInt::HeritageGroup) != Some(HeritageGroup::Gearknight.0) {
            // Gear Knights do not get clothing (pcap verified)
            if appearance.headgear_style < u32::MAX {
                // No headgear is max UINT
                let hat_weenie = or_throw(sex.get_headgear_weenie(appearance.headgear_style));
                let hat = get_clothing_object(
                    w,
                    hat_weenie,
                    appearance.headgear_color,
                    appearance.headgear_hue,
                );
                if let Some(hat) = hat {
                    let valid_locations = hat.valid_locations().unwrap_or_default();
                    try_equip_object_created(w, &mut p, hat, valid_locations);
                } else {
                    let iou = create_iou(
                        w,
                        or_throw(sex.get_headgear_weenie(appearance.headgear_style)),
                    );
                    try_add_to_inventory_created(w, &mut p, iou);
                }
            }

            let shirt_weenie = or_throw(sex.get_shirt_weenie(appearance.shirt_style));
            let shirt = get_clothing_object(
                w,
                shirt_weenie,
                appearance.shirt_color,
                appearance.shirt_hue,
            );
            if let Some(shirt) = shirt {
                let valid_locations = shirt.valid_locations().unwrap_or_default();
                try_equip_object_created(w, &mut p, shirt, valid_locations);
            } else {
                let iou = create_iou(w, or_throw(sex.get_shirt_weenie(appearance.shirt_style)));
                try_add_to_inventory_created(w, &mut p, iou);
            }

            let pants_weenie = or_throw(sex.get_pants_weenie(appearance.pants_style));
            let pants = get_clothing_object(
                w,
                pants_weenie,
                appearance.pants_color,
                appearance.pants_hue,
            );
            if let Some(pants) = pants {
                let valid_locations = pants.valid_locations().unwrap_or_default();
                try_equip_object_created(w, &mut p, pants, valid_locations);
            } else {
                let iou = create_iou(w, or_throw(sex.get_pants_weenie(appearance.pants_style)));
                try_add_to_inventory_created(w, &mut p, iou);
            }

            let shoes_weenie = or_throw(sex.get_footwear_weenie(appearance.footwear_style));
            let shoes = get_clothing_object(
                w,
                shoes_weenie,
                appearance.footwear_color,
                appearance.footwear_hue,
            );
            if let Some(shoes) = shoes {
                let valid_locations = shoes.valid_locations().unwrap_or_default();
                try_equip_object_created(w, &mut p, shoes, valid_locations);
            } else {
                let iou = create_iou(
                    w,
                    or_throw(sex.get_footwear_weenie(appearance.footwear_style)),
                );
                try_add_to_inventory_created(w, &mut p, iou);
            }
        }

        // **Not ACE's (a fix, V243):** ACE indexed the template list with no range check, so
        // a number outside it (the client's template starts at -1) threw, and the client was
        // answered with silence until its 110-second timeout. It is now answered `Corrupt`.
        let template_index: u32 = character_create_info.template_option.cs_cast();
        if usize::try_from(template_index).map_or(true, |i| i >= heritage_group.templates.len()) {
            return (CreateResult::InvalidTemplate, p);
        }
        let template = index(&heritage_group.templates, template_index);
        let template_name = template.name.clone();
        p.player
            .set_property(PropertyString::Template, template_name);

        let title = template.title;
        with_player_in_world(w, &mut p, None, |w, player| {
            crate::world_objects::player_character::add_title(w, player, title, true)
        });

        // attributes
        let result =
            validate_attribute_credits(character_create_info, heritage_group.attribute_credits);

        if result != CreateResult::Success {
            return (result, p);
        }

        for (attribute, value) in [
            (
                PropertyAttribute::Strength,
                character_create_info.strength_ability,
            ),
            (
                PropertyAttribute::Endurance,
                character_create_info.endurance_ability,
            ),
            (
                PropertyAttribute::Coordination,
                character_create_info.coordination_ability,
            ),
            (
                PropertyAttribute::Quickness,
                character_create_info.quickness_ability,
            ),
            (
                PropertyAttribute::Focus,
                character_create_info.focus_ability,
            ),
            (PropertyAttribute::Self_, character_create_info.self_ability),
        ] {
            // `player.Strength.StartingValue = characterCreateInfo.StrengthAbility;` and so on
            let creature_attribute = *p
                .player
                .attributes()
                .get(&attribute)
                .unwrap_or_else(|| throw(&AceThrow::KeyNotFound(u32::from(attribute.0))));
            creature_attribute.set_starting_value(&mut p.player, value);
        }

        // data we don't care about
        //characterCreateInfo.CharacterSlot;
        //characterCreateInfo.ClassId;

        // characters start with max vitals
        // (`player.Health.Current = player.Health.Base;` and the same for Stamina and Mana: `Base`
        // reads the equipped items' GearMaxHealth, so the player is staged with its possessions.)
        with_player_in_world(w, &mut p, None, |w, player| {
            let o = w.objects.get(player).expect("the staged player");
            for vital in [o.health(), o.stamina(), o.mana()] {
                let base = vital.base(w, w.objects.get(player).expect("the staged player"));
                vital.set_current(w.objects.get_mut(player).expect("the staged player"), base);
            }
        });

        // set initial skill credit amount. 52 for all but "Olthoi", which have 68
        p.player.set_property(
            PropertyInt::AvailableSkillCredits,
            heritage_group.skill_credits.cs_cast(),
        );
        p.player.set_property(
            PropertyInt::TotalSkillCredits,
            heritage_group.skill_credits.cs_cast(),
        );

        if character_create_info.skill_advancement_classes.len() != 55 {
            return (CreateResult::ClientServerSkillsMismatch, p);
        }

        let skill_table = dats.portal_dat().skill_table();

        // Retail's costing (V249): the credits are the wizard's own sum,
        // `dereth_rules::chargen::skill_credits_used`, and a skill specialises for its specialised cost
        // in total. The sum charges a skill the table does not name -1, so such a skill (skill 0
        // included) is refused before it is summed, as ACE refuses it; so is a negative cost, which
        // the wizard never offers.
        let mut levels = Vec::with_capacity(character_create_info.skill_advancement_classes.len());
        for (i, &sac) in character_create_info
            .skill_advancement_classes
            .iter()
            .enumerate()
        {
            if sac != SkillAdvancementClass::Inactive {
                let i_u32 = u32::try_from(i).unwrap_or(u32::MAX);
                let (trained, specialized) =
                    dereth_rules::chargen::skill_costs(char_gen, skill_table, heritage_key, i_u32);
                if !skill_table.skills.contains_key(&i_u32) {
                    log::error!(
                        "Character {} tried to create with skill {} that was not found in Portal dat.",
                        character_create_info.name.as_deref().unwrap_or(""),
                        i
                    );
                    return (CreateResult::InvalidSkillRequested, p);
                }
                if trained < 0 || specialized < 0 {
                    return (CreateResult::InvalidSkillRequested, p);
                }
            }
            levels.push(shared_class(sac));
        }
        if dereth_rules::chargen::skill_credits_used(char_gen, skill_table, heritage_key, &levels)
            > heritage_group.skill_credits.cs_cast()
        {
            return (CreateResult::TooManySkillCreditsUsed, p);
        }

        for (i, &sac) in character_create_info
            .skill_advancement_classes
            .iter()
            .enumerate()
        {
            if sac == SkillAdvancementClass::Inactive {
                continue;
            }

            let i_u32 = u32::try_from(i).unwrap_or(u32::MAX);
            // The shared costs; specialising charges the rest of the specialised cost.
            let (trained_cost, specialized_total) =
                dereth_rules::chargen::skill_costs(char_gen, skill_table, heritage_key, i_u32);
            let specialized_cost = specialized_total - trained_cost;

            let skill_id = Skill(i_u32.cs_cast());
            if sac == SkillAdvancementClass::Specialized {
                if !with_player_in_world(w, &mut p, None, |w, player| {
                    player_skills::train_skill_with(w, player, skill_id, trained_cost, false)
                }) {
                    return (CreateResult::FailedToTrainSkill, p);
                }
                if !with_player_in_world(w, &mut p, None, |w, player| {
                    player_skills::specialize_skill_with(
                        w,
                        player,
                        skill_id,
                        specialized_cost,
                        true,
                    )
                }) {
                    return (CreateResult::FailedToSpecializeSkill, p);
                }
            } else if sac == SkillAdvancementClass::Trained {
                if !with_player_in_world(w, &mut p, None, |w, player| {
                    player_skills::train_skill_with(w, player, skill_id, trained_cost, true)
                }) {
                    return (CreateResult::FailedToTrainSkill, p);
                }
            } else if sac == SkillAdvancementClass::Untrained {
                with_player_in_world(w, &mut p, None, |w, player| {
                    player_skills::untrain_skill(w, player, skill_id, 0)
                });
            }
        }

        // Set Heritage based Melee and Ranged Masteries
        let (melee_mastery, ranged_mastery) = get_masteries(p.player.heritage_group());

        p.player
            .set_property(PropertyInt::MeleeMastery, melee_mastery.0);
        p.player
            .set_property(PropertyInt::RangedMastery, ranged_mastery.0);

        // Set innate augs
        set_innate_augmentations(&mut p.player);

        let is_dual_wield_trained_or_specialized = p
            .player
            .biota
            .get_skill(Skill::DualWield)
            .is_some_and(|s| s.sac > SkillAdvancementClass::Untrained);

        // grant starter items based on skills
        // (`GetStarterGearConfiguration()` is null when the file failed to load: NullReferenceException.)
        let starter_gear_config = starter_gear_factory::get_starter_gear_configuration()
            .expect("NullReferenceException: starterGearConfig");
        let mut granted_weenies: Vec<u32> = Vec::new();

        for skill_gear in starter_gear_config.skills {
            //var charSkill = player.Skills[(Skill)skillGear.SkillId];
            let Some(char_skill_sac) = p
                .player
                .biota
                .get_skill(Skill(i32::from(skill_gear.skill_id)))
                .map(|s| s.sac)
            else {
                continue;
            };

            if char_skill_sac == SkillAdvancementClass::Trained
                || char_skill_sac == SkillAdvancementClass::Specialized
            {
                grant_starter_items(
                    w,
                    &mut p,
                    skill_gear.gear,
                    &mut granted_weenies,
                    is_dual_wield_trained_or_specialized,
                );

                let heritage_loot = skill_gear.heritage.iter().find(|h| {
                    h.heritage_id == CsCast::<u16>::cs_cast(character_create_info.heritage.0)
                });
                if let Some(heritage_loot) = heritage_loot {
                    grant_starter_items(
                        w,
                        &mut p,
                        heritage_loot.gear,
                        &mut granted_weenies,
                        is_dual_wield_trained_or_specialized,
                    );
                }

                for spell in skill_gear.spells {
                    let spell_id = spell.spell_id;
                    if char_skill_sac == SkillAdvancementClass::Trained && !spell.specialized_only {
                        with_player_in_world(w, &mut p, None, |w, player| {
                            player_spells::add_known_spell(w, player, spell_id)
                        });
                    } else if char_skill_sac == SkillAdvancementClass::Specialized {
                        with_player_in_world(w, &mut p, None, |w, player| {
                            player_spells::add_known_spell(w, player, spell_id)
                        });
                    }
                }
            }
        }
    } else {
        if let Some(title) = p.player.character_title_id().filter(|t| *t > 0) {
            with_player_in_world(w, &mut p, None, |w, player| {
                crate::world_objects::player_character::add_title(w, player, title.cs_cast(), true)
            });
        }

        let known: Vec<i32> = p
            .player
            .biota
            .properties_spell_book
            .as_ref()
            .map(|b| b.keys().copied().collect())
            .unwrap_or_default();
        if !known.is_empty() {
            // `var i = 0u; foreach (...) HandleActionAddSpellFavorite((uint)spell.Key, i++, 0);`
            for (i, spell) in (0u32..).zip(known) {
                with_player_in_world(w, &mut p, None, |w, player| {
                    crate::world_objects::player_character::handle_action_add_spell_favorite(
                        w,
                        player,
                        spell.cs_cast(),
                        i,
                        0,
                    )
                });
            }
        }
    }

    let name = character_create_info.name.clone().unwrap_or_default();
    with_player_in_world(w, &mut p, None, |w, player| {
        player::player_set_name(w, player, name.clone())
    });
    p.character_mut().name.clone_from(&name);

    // Index used to determine the starting location
    let start_area = character_create_info.start_area;

    let starter_area = index(&char_gen.starter_areas, start_area);

    // DIVERGE: the era's rule (`World.era`) chooses the start position; end of retail is ACE's.
    let (location, instantiation, recalls_disabled) = match w.era.start_positions {
        StartPositions::FromCharGen => {
            let (location, instantiation) = char_gen_start(w, starter_area);
            (location, instantiation, true)
        }
        StartPositions::Towns(towns) => {
            let location = town_start(towns, &starter_area.name);
            (location, location, false)
        }
    };
    p.player
        .set_position(PositionType::Location, Some(location));

    p.player
        .set_position(PositionType::Instantiation, Some(instantiation));

    if !p.is_olthoi_player() {
        p.player
            .set_position(PositionType::Sanctuary, Some(location));
        if recalls_disabled {
            p.player.set_property(PropertyBool::RecallsDisabled, true);
        }

        if property_manager_get_bool(w, "pk_server") {
            p.player.set_property(
                PropertyInt::PlayerKillerStatus,
                CsCast::<i32>::cs_cast(PlayerKillerStatus::PK.0),
            );
        } else if property_manager_get_bool(w, "pkl_server") {
            p.player.set_property(
                PropertyInt::PlayerKillerStatus,
                CsCast::<i32>::cs_cast(PlayerKillerStatus::NPK.0),
            );
        }

        if (property_manager_get_bool(w, "pk_server") || property_manager_get_bool(w, "pkl_server"))
            && property_manager_get_bool(w, "pk_server_safe_training_academy")
        {
            p.player.set_property(
                PropertyFloat::MinimumTimeSincePk,
                -property_manager_get_double(w, "pk_new_character_grace_period"),
            );
            p.player.set_property(
                PropertyInt::PlayerKillerStatus,
                CsCast::<i32>::cs_cast(PlayerKillerStatus::NPK.0),
            );
        }
    }

    if p.player.is_sentinel() || p.player.is_admin() {
        p.character_mut().is_plussed = true;
        p.player.set_cloak_status(CloakStatus::Off);
        let channels_active = p.player.channels_active();
        p.player.set_channels_allowed(channels_active);
    }

    character_create_set_default_character_options(w, &mut p);

    (CreateResult::Success, p)
}

/// ACE's start: the starter area's first location, and the town's "Free Ride" spell (or, for the
/// Olthoi lair, the location itself) as the instantiation point.
fn char_gen_start(
    w: &World,
    starter_area: &dereth_assets::tables::StarterArea,
) -> (Position, Position) {
    let loc = starter_area
        .locations
        .first()
        .unwrap_or_else(|| throw(&AceThrow::IndexOutOfRange(0)));
    let location = Position::from_components(
        loc.cell.0,
        loc.frame.origin.x,
        loc.frame.origin.y,
        loc.frame.origin.z,
        loc.frame.rotation.x,
        loc.frame.rotation.y,
        loc.frame.rotation.z,
        loc.frame.rotation.w,
        false,
    );

    let mut instantiation = Position::from_components(
        0xA9B4_0019,
        84.0,
        7.1,
        94.0,
        0.0,
        0.0,
        -0.078_459_1,
        0.996_917,
        false,
    ); // ultimate fallback.
    let spell_free_ride = match starter_area.name.as_str() {
        "OlthoiLair" => {
            //todo: check this when olthoi play is allowed in ace
            instantiation = location; // no training area for olthoi, so they start and fall back to same place.
            None
        }
        "Shoushi" => w.content.get_cached_spell(3813), // Free Ride to Shoushi
        "Yaraq" => w.content.get_cached_spell(3814),   // Free Ride to Yaraq
        "Sanamar" => w.content.get_cached_spell(3535), // Free Ride to Sanamar
        _ => w.content.get_cached_spell(3815), // Free Ride to Holtburg ("Holtburg" and default)
    };
    if let Some(spell) = spell_free_ride.filter(|s| !s.name.is_empty()) {
        let v = |f: Option<f32>| {
            f.expect("InvalidOperationException: Nullable object must have a value.")
        };
        instantiation = Position::from_components(
            spell
                .position_obj_cell_id
                .expect("InvalidOperationException: Nullable object must have a value."),
            v(spell.position_origin_x),
            v(spell.position_origin_y),
            v(spell.position_origin_z),
            v(spell.position_angles_x),
            v(spell.position_angles_y),
            v(spell.position_angles_z),
            v(spell.position_angles_w),
            false,
        );
    }

    (location, instantiation)
}

/// Not ACE: an era's listed start (ClassicACE's rule): the town the chosen starter area names (the
/// first town for any other area), then one of its areas, chosen evenly.
fn town_start(towns: &'static [TownStart], starter_area_name: &str) -> Position {
    let town = StartPositions::town(towns, starter_area_name);
    let area = &town.areas[usize::from(ThreadSafeRandom::next(0, 1) == 1)];
    let [x, y, z] = area.origin;
    let [rx, ry, rz, rw] = area.rotation;
    Position::from_components(area.cell, x, y, z, rx, ry, rz, rw, false)
}

/// The starter items of one gear list (a skill's, then its heritage entry's): the two identical
/// loops of `Create`.
fn grant_starter_items(
    w: &mut World,
    p: &mut CreatedPlayer,
    gear: &[starter_gear_factory::StarterItem],
    granted_weenies: &mut Vec<u32>,
    is_dual_wield_trained_or_specialized: bool,
) {
    for item in gear {
        if granted_weenies.contains(&item.weenie_id) {
            let Some(existing_item) = p
                .inventory
                .iter_mut()
                .find(|i| i.biota.weenie_class_id == item.weenie_id)
            else {
                continue;
            };
            if existing_item.max_stack_size().map_or(1, i32::from) <= 1 {
                continue;
            }

            let stack_size = existing_item
                .stack_size()
                .map(|s| s.wrapping_add(i32::from(item.stack_size)));
            existing_item.set_stack_size(stack_size);
            continue;
        }

        let mut loot = create_new_world_object_by_wcid(w, item.weenie_id);
        if let Some(loot) = loot.as_mut() {
            if let (Some(_), Some(max_stack_size)) = (loot.stack_size(), loot.max_stack_size()) {
                let max_stack_size = i32::from(max_stack_size);
                let value = if i32::from(item.stack_size) <= max_stack_size {
                    i32::from(item.stack_size)
                } else {
                    max_stack_size
                };
                loot.set_stack_size(Some(value));
            }
        } else {
            let iou = create_iou(w, item.weenie_id);
            try_add_to_inventory_created(w, p, iou);
        }

        // `loot.WeenieType` is read after `TryAddToInventory`, which keeps the object.
        let loot_weenie_type = loot.as_ref().map(|l| l.biota.weenie_type);
        let loot_exists = loot.is_some();
        if loot_exists && try_add_to_inventory_created(w, p, loot) {
            granted_weenies.push(item.weenie_id);
        }

        if is_dual_wield_trained_or_specialized
            && loot_exists
            && loot_weenie_type == Some(WeenieType::MeleeWeapon)
        {
            let dualloot = create_new_world_object_by_wcid(w, item.weenie_id);
            if dualloot.is_some() {
                try_add_to_inventory_created(w, p, dualloot);
            } else {
                let iou = create_iou(w, item.weenie_id);
                try_add_to_inventory_created(w, p, iou);
            }
        }
    }
}

// ACE: PlayerFactory.GetClothingObject
fn get_clothing_object(
    w: &mut World,
    weenie_class_id: u32,
    palette: u32,
    shade: f64,
) -> Option<WorldObject> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;

    // `(Clothing)WorldObjectFactory.CreateNewWorldObject(weenie)`: an InvalidCastException for another
    // class, then a NullReferenceException on `SetProperties` when construction returned null.
    let mut world_object =
        create_new_world_object(w, weenie).expect("NullReferenceException: worldObject");
    assert!(
        world_object.is_clothing(),
        "InvalidCastException: the weenie is not Clothing"
    );

    crate::world_objects::clothing::set_properties(w, &mut world_object, palette.cs_cast(), shade);

    // (ACE keeps the old ClothingBaseEffects / sub-palette code commented out here.)

    Some(world_object)
}

// ACE: PlayerFactory.GetMasteries
/// Set Heritage based Melee and Ranged Masteries.
#[must_use]
pub fn get_masteries(heritage_group: HeritageGroup) -> (WeaponType, WeaponType) {
    match heritage_group {
        HeritageGroup::Aluvian => (WeaponType::Dagger, WeaponType::Bow),
        HeritageGroup::Gharundim => (WeaponType::Staff, WeaponType::Magic),
        HeritageGroup::Sho => (WeaponType::Unarmed, WeaponType::Bow),
        HeritageGroup::Viamontian => (WeaponType::Sword, WeaponType::Crossbow),
        HeritageGroup::Penumbraen | HeritageGroup::Shadowbound => {
            (WeaponType::Unarmed, WeaponType::Crossbow)
        }
        HeritageGroup::Gearknight => (WeaponType::Mace, WeaponType::Crossbow),
        HeritageGroup::Tumerok => (WeaponType::Spear, WeaponType::Thrown),
        HeritageGroup::Undead | HeritageGroup::Lugian => (WeaponType::Axe, WeaponType::Thrown),
        HeritageGroup::Empyrean => (WeaponType::Sword, WeaponType::Magic),
        _ => (WeaponType::Undef, WeaponType::Undef),
    }
}

// ACE: PlayerFactory.SetInnateAugmentations
fn set_innate_augmentations(player: &mut WorldObject) {
    match player.heritage_group() {
        HeritageGroup::Aluvian
        | HeritageGroup::Gharundim
        | HeritageGroup::Sho
        | HeritageGroup::Viamontian => {
            player.set_augmentation_jack_of_all_trades(1);
        }
        HeritageGroup::Shadowbound | HeritageGroup::Penumbraen => {
            player.set_augmentation_critical_expertise(1)
        }
        HeritageGroup::Gearknight => player.set_augmentation_damage_reduction(1),
        HeritageGroup::Undead => player.set_augmentation_critical_defense(1),
        HeritageGroup::Empyrean => player.set_augmentation_infused_life_magic(1),
        HeritageGroup::Tumerok => player.set_augmentation_critical_power(1),
        HeritageGroup::Lugian => player.set_augmentation_increased_carrying_capacity(1),
        // Olthoi, OlthoiAcid and anything else: nothing.
        _ => {}
    }
}

// ACE: PlayerFactory.CreateIOU
/// An IOU book for a weenie the world database does not have, or `None` while `iou_trades` is off
/// (ACE's default).
pub fn create_iou(w: &mut World, missing_weenie_id: u32) -> Option<WorldObject> {
    if !property_manager_get_bool(w, "iou_trades") {
        log::warn!(
            "CreateIOU: Skipping creation of IOU for missing weenie {missing_weenie_id} because IOU system is disabled."
        );

        return None;
    }

    // `(Book)WorldObjectFactory.CreateNewWorldObject("parchment")`
    let weenie = w.content.get_cached_weenie_by_class_name("parchment");
    let mut iou = weenie
        .and_then(|weenie| create_new_world_object(w, weenie))
        .expect("NullReferenceException: iou");

    // DIVERGE: ACE signs the IOU "ACEmulator"; ours signs it with our name (brand). Turn-in accepts both.
    crate::world_objects::book::set_properties(
        &mut iou,
        "IOU",
        "An IOU for a missing database object.",
        "Sorry about that chief...",
        empyrean_common::brand::PRODUCT,
        "prewritten",
    );
    let _ = crate::world_objects::book::add_page(
        &mut iou,
        u32::MAX,
        empyrean_common::brand::PRODUCT,
        Some("prewritten"),
        false,
        &format!("{missing_weenie_id}\n\nSorry but the database does not have a weenie for weenieClassId #{missing_weenie_id} so in lieu of that here is an IOU for that item."),
    );
    iou.set_bonded(Some(BondedStatus::Bonded));
    iou.set_attuned(Some(AttunedStatus::Attuned));
    iou.set_is_sellable(false);
    iou.set_value(Some(0));
    iou.set_encumbrance_val(Some(0));

    Some(iou)
}

// ACE: PlayerFactory.ValidateAttributeCredits
/// Validates character creation attribute info.
#[must_use]
pub fn validate_attribute_credits(info: &CharacterCreateInfo, max_attributes: u32) -> CreateResult {
    let attribute_values = [
        info.strength_ability,
        info.endurance_ability,
        info.coordination_ability,
        info.quickness_ability,
        info.focus_ability,
        info.self_ability,
    ];

    let mut total: u32 = 0;

    for attribute_value in attribute_values {
        if !(10..=100).contains(&attribute_value) {
            return CreateResult::InvalidSkillRequested;
        }

        total = total.wrapping_add(attribute_value);
    }

    if total > max_attributes {
        return CreateResult::TooManySkillCreditsUsed;
    }

    CreateResult::Success
}

// ACE: PlayerFactory.CharacterCreateSetDefaultCharacterOptions
fn character_create_set_default_character_options(w: &mut World, p: &mut CreatedPlayer) {
    use crate::world_objects::player_character::set_character_option;
    with_player_in_world(w, p, None, |w, player| {
        // (ACE lists 19 individual options here, commented out.)
        set_character_option(w, player, CharacterOption::CharacterOptions1Default, true);
        set_character_option(w, player, CharacterOption::CharacterOptions2Default, true);

        // This option was seen in PCAPs on new characters, and possibly was added to Defaults post PDB we have
        set_character_option(w, player, CharacterOption::ListenToPKDeathMessages, true);
    });
}

/// Runs `f` with the new player, its possessions and `extra` in `World.objects`, then takes them
/// out again. Not ACE: ACE's new player is in no landblock, but the ported container and
/// equipment members resolve every object through the store, so each call moves the
/// objects in for its duration. Afterwards `p.inventory` and `p.equipped_objects` are rebuilt in
/// the order of the player's `Inventory` and `EquippedObjects`; an `extra` object neither of them
/// took is dropped, as ACE's garbage collector drops it.
pub(crate) fn with_player_in_world<R>(
    w: &mut World,
    p: &mut CreatedPlayer,
    extra: Option<WorldObject>,
    f: impl FnOnce(&mut World, ObjectGuid) -> R,
) -> R {
    let player_guid = p.player.guid;
    let mut staged = vec![player_guid];
    let insert = |w: &mut World, o: WorldObject| {
        let guid = o.guid;
        assert!(
            w.objects.insert(o).is_ok(),
            "character creation object 0x{:08X} is already live",
            guid.full()
        );
        guid
    };
    insert(w, std::mem::take(&mut p.player));
    for o in p
        .inventory
        .drain(..)
        .chain(p.equipped_objects.drain(..))
        .chain(extra)
    {
        staged.push(insert(w, o));
    }

    let result = f(w, player_guid);

    let player = w.objects.remove(player_guid).expect("the staged player");
    let take = |w: &mut World, guids: Vec<ObjectGuid>| -> Vec<WorldObject> {
        guids
            .into_iter()
            .map(|g| *w.objects.remove(g).expect("a staged possession"))
            .collect()
    };
    let inventory: Vec<ObjectGuid> = container::inventory(&player).keys().copied().collect();
    let equipped: Vec<ObjectGuid> = creature_equipment_values(&player);
    p.inventory = take(w, inventory);
    p.equipped_objects = take(w, equipped);
    p.player = *player;
    for guid in staged {
        w.objects.remove(guid);
    }

    result
}

/// `Creature.EquippedObjects` keys of an object outside the store.
fn creature_equipment_values(o: &WorldObject) -> Vec<ObjectGuid> {
    o.creature
        .as_ref()
        .map(|c| {
            c.creature_equipment
                .equipped_objects
                .keys()
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

/// `player.TryAddToInventory(worldObject)` (`Container.TryAddToInventory`) on the new
/// player: `placementPosition = 0`, `limitToMainPackOnly = false`, `burdenCheck = true`. `None` is
/// C#'s `null` (false).
pub(crate) fn try_add_to_inventory_created(
    w: &mut World,
    p: &mut CreatedPlayer,
    world_object: Option<WorldObject>,
) -> bool {
    let Some(world_object) = world_object else {
        return false;
    };
    let item = world_object.guid;

    with_player_in_world(w, p, Some(world_object), |w, player| {
        container::try_add_to_inventory(w, player, item, 0, false, true)
    })
}

/// `player.TryEquipObject(worldObject, wieldedLocation)` (`Creature.TryEquipObject`) on
/// the new player.
fn try_equip_object_created(
    w: &mut World,
    p: &mut CreatedPlayer,
    world_object: WorldObject,
    wielded_location: EquipMask,
) -> bool {
    let item = world_object.guid;

    with_player_in_world(w, p, Some(world_object), |w, player| {
        creature_equipment::try_equip_object(w, player, item, wielded_location)
    })
}

/// Not ACE's: a requested class as the shared chargen rules name it. A value outside the four
/// classes is none of them to ACE's loop (it trains nothing), so it counts as `Inactive`.
fn shared_class(sac: SkillAdvancementClass) -> dereth_rules::chargen::SkillAdvancementClass {
    use dereth_rules::chargen::SkillAdvancementClass as Shared;
    match sac {
        SkillAdvancementClass::Untrained => Shared::Untrained,
        SkillAdvancementClass::Trained => Shared::Trained,
        SkillAdvancementClass::Specialized => Shared::Specialized,
        _ => Shared::Inactive,
    }
}
