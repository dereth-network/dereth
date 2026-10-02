// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Networking.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Networking.cs`.
//!
//! A creature's object description is its base model data plus its equipped clothing and armor,
//! layered in ACE's priority order, then the "naked" setup parts no item covers.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C# casts

use dereth_primitives::DataId;
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, CoverageMask, EquipMask, ItemType, PropertyBool, SetupConst,
};
use empyrean_entity::models::{PropertiesAnimPart, PropertiesPalette, PropertiesTextureMap};
use empyrean_entity::{ObjDesc, ObjectGuid};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::network::game_messages::messages::game_message_system_chat;
use crate::network::structure::creature_profile::{self, creature_profile_new};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::{add_base_model_data, shims};
use crate::World;

/// Non-property fields declared in `Creature_Networking.cs`.
#[derive(Debug, Default)]
pub struct CreatureNetworkingFields {}

// ACE: Creature.HandleActionWorldBroadcast
pub fn handle_action_world_broadcast(
    w: &mut World,
    this: ObjectGuid,
    message: String,
    message_type: ChatMessageType,
) {
    let mut chain = ActionChain::new();
    chain.add_action(Actor::Object(this), move |w| {
        do_world_broadcast(w, this, &message, message_type)
    });
    chain.enqueue_chain(w);
}

// ACE: Creature.DoWorldBroadcast
/// A system chat message to every online player, logged as an `AllBroadcast` chat
/// (through `PlayerManager`).
pub fn do_world_broadcast(
    w: &mut World,
    this: ObjectGuid,
    message: &str,
    message_type: ChatMessageType,
) {
    let sys_message = game_message_system_chat::game_message_system_chat(message, message_type);

    crate::managers::player_manager::broadcast_to_all(w, &sys_message);
    crate::managers::player_manager::log_broadcast_chat(
        w,
        empyrean_entity::enums::Channel::AllBroadcast,
        Some(this),
        message,
    );
}

/// `EquipMask.Armor | EquipMask.Extremity`.
const ARMOR_OR_EXTREMITY: EquipMask = EquipMask(EquipMask::Armor.0 | EquipMask::Extremity.0);

/// `(x.CurrentWieldedLocation & mask) != 0` with C#'s lifted operators: a null location gives a
/// null `&`, and `null != 0` is **true**.
fn wielded_intersects(location: Option<EquipMask>, mask: EquipMask) -> bool {
    location.is_none_or(|l| (l & mask).0 != 0)
}

/// The values `CalculateObjDesc` reads from an equipped item.
struct EquippedView {
    item_type: ItemType,
    current_wielded_location: Option<EquipMask>,
    top_layer_priority: Option<bool>,
    visual_clothing_priority: Option<CoverageMask>,
    clothing_priority: Option<CoverageMask>,
    clothing_base: Option<u32>,
    palette_template: Option<i32>,
    shade: Option<f64>,
}

impl EquippedView {
    fn of(w: &World, g: ObjectGuid) -> Self {
        let o: &WorldObject = w.objects.get(g).expect("ACE: null equipped object");
        EquippedView {
            item_type: o.item_type(),
            current_wielded_location: o.current_wielded_location(),
            top_layer_priority: o.get_property(PropertyBool::TopLayerPriority),
            visual_clothing_priority: o.visual_clothing_priority(),
            clothing_priority: o.clothing_priority(),
            clothing_base: o.clothing_base(),
            palette_template: o.palette_template(),
            shade: o.shade(),
        }
    }
}

/// The clothing-table setup a player's alternate setup uses: the alternate setups have no
/// entries of their own.
fn clothing_setup_of(setup_table_id: u32) -> u32 {
    let s = SetupConst(setup_table_id);
    if s == SetupConst::UmbraenMaleCrownGen
        || s == SetupConst::UmbraenMaleNoCrown
        || s == SetupConst::UmbraenMaleVoid
    {
        //case (uint)SetupConst.UmbraenMaleCrown:
        SetupConst::UmbraenMaleCrown.0
    } else if s == SetupConst::UmbraenFemaleNoCrown || s == SetupConst::UmbraenFemaleVoid {
        //case (uint)SetupConst.UmbraenFemaleCrown:
        //case (uint)SetupConst.UmbraenFemaleCrownGen:
        SetupConst::UmbraenFemaleCrown.0
    } else if s == SetupConst::PenumbraenMaleCrownGen
        || s == SetupConst::PenumbraenMaleNoCrown
        || s == SetupConst::PenumbraenMaleVoid
    {
        //case (uint)SetupConst.PenumbraenMaleCrown:
        SetupConst::PenumbraenMaleCrown.0
    } else if s == SetupConst::PenumbraenFemaleNoCrown || s == SetupConst::PenumbraenFemaleVoid {
        //case (uint)SetupConst.PenumbraenFemaleCrown:
        //case (uint)SetupConst.PenumbraenFemaleCrownGen:
        SetupConst::PenumbraenFemaleCrown.0
    } else if s == SetupConst::UndeadMaleUndeadGen
        || s == SetupConst::UndeadMaleSkeleton
        || s == SetupConst::UndeadMaleSkeletonNoFlame
        || s == SetupConst::UndeadMaleZombie
        || s == SetupConst::UndeadMaleZombieNoFlame
    {
        SetupConst::UndeadMaleUndead.0
    } else if s == SetupConst::UndeadFemaleUndeadGen
        || s == SetupConst::UndeadFemaleSkeleton
        || s == SetupConst::UndeadFemaleSkeletonNoFlame
        || s == SetupConst::UndeadFemaleZombie
        || s == SetupConst::UndeadFemaleZombieNoFlame
    {
        SetupConst::UndeadFemaleUndead.0
    } else if s == SetupConst::AnakshayMale {
        SetupConst::HumanMale.0
    } else if s == SetupConst::AnakshayFemale {
        SetupConst::HumanFemale.0
    } else {
        setup_table_id
    }
}

// ACE: Creature.CalculateObjDesc
#[allow(clippy::too_many_lines)]
pub fn creature_calculate_obj_desc(w: &mut World, this: ObjectGuid) -> ObjDesc {
    let mut obj_desc = ObjDesc::default();

    add_base_model_data(w, this, &mut obj_desc);

    let mut coverage: Vec<u32> = Vec::new();

    let o = w.objects.get(this).expect("ACE: this is null");
    let setup_table_id = o.setup_table_id();
    let mut show_helm = true;
    let mut show_cloak = true;
    let is_player = o.is_player();
    if is_player {
        show_helm =
            shims::player_get_character_option(w, this, CharacterOption::ShowYourHelmOrHeadGear);
        show_cloak = shims::player_get_character_option(w, this, CharacterOption::ShowYourCloak);
    }

    // Some player races use an AlternateSetupDid, either at creation or via Barber options.
    // BUT -- those values do not correspond with entries in the Clothing Table.
    // So, we need to make some adjustments to look up something that DOES exist and is appropriate for the AlternateSetup model.
    let this_setup_id = clothing_setup_of(setup_table_id);

    let equipped = crate::world_objects::creature_equipment::equipped_objects_values(w, this);

    // get all the Armor Items, and any Clothing items that might be equipped (robes, slippers, gloves, kasa, etc) so we can calculate their priority
    let armor_items: Vec<ObjectGuid> = equipped
        .iter()
        .copied()
        .filter(|x| {
            let x = EquippedView::of(w, *x);
            x.item_type == ItemType::Armor
                || wielded_intersects(x.current_wielded_location, ARMOR_OR_EXTREMITY)
        })
        .collect();
    for item in &armor_items {
        shims::set_visual_clothing_priority(w, *item);
    }

    // sort the armor into the proper order... TopLayerPriority first, then no priority, then TopLayerPriority=false.
    // Secondary sort field is the calculated "VisualClothingPriority"
    let by_layer = |w: &World, layer: Option<bool>| -> Vec<ObjectGuid> {
        let mut v: Vec<ObjectGuid> = armor_items
            .iter()
            .copied()
            .filter(|x| EquippedView::of(w, *x).top_layer_priority == layer)
            .collect();
        // `OrderBy(x => x.VisualClothingPriority)`: stable; a null key sorts first.
        v.sort_by_key(|x| {
            EquippedView::of(w, *x)
                .visual_clothing_priority
                .map(|c| c.0)
        });
        v
    };
    let top = by_layer(w, Some(true));
    let no_layer = by_layer(w, None);
    let bottom = by_layer(w, Some(false));
    let sorted_armor_items: Vec<ObjectGuid> =
        bottom.into_iter().chain(no_layer).chain(top).collect();

    let mut clothes_and_cloaks: Vec<ObjectGuid> = equipped
        .iter()
        .copied()
        .filter(|x| {
            let x = EquippedView::of(w, *x);
            // Extremity, Head/Foot/Hands, is included in the ArmorItems above
            x.item_type == ItemType::Clothing
                && !wielded_intersects(x.current_wielded_location, ARMOR_OR_EXTREMITY)
        })
        .collect();
    clothes_and_cloaks.sort_by_key(|x| EquippedView::of(w, *x).clothing_priority.map(|c| c.0));

    let eo: Vec<ObjectGuid> = clothes_and_cloaks
        .into_iter()
        .chain(sorted_armor_items)
        .collect();

    if eo.is_empty() {
        // Check if there is any defined ObjDesc in the Biota and, if so, apply them
        let biota = &w.objects.get(this).expect("ACE: this is null").biota;
        let anim = biota.properties_anim_part.as_ref().map_or(0, Vec::len);
        let pal = biota.properties_palette.as_ref().map_or(0, Vec::len);
        let tex = biota.properties_texture_map.as_ref().map_or(0, Vec::len);
        if anim > 0 || pal > 0 || tex > 0 {
            obj_desc
                .anim_part_changes
                .extend(biota.properties_anim_part.iter().flatten().cloned());

            obj_desc
                .sub_palettes
                .extend(biota.properties_palette.iter().flatten().cloned());

            obj_desc
                .texture_changes
                .extend(biota.properties_texture_map.iter().flatten().cloned());

            return obj_desc;
        }
    }

    for wg in &eo {
        let wi = EquippedView::of(w, *wg);
        if wi.current_wielded_location == Some(EquipMask::HeadWear) && !show_helm && is_player {
            continue;
        }

        if wi.current_wielded_location == Some(EquipMask::Cloak) && !show_cloak && is_player {
            continue;
        }

        // We can wield things that are not part of our model, only use those items that can cover our model.
        let covering = EquipMask(EquipMask::Clothing.0 | EquipMask::Armor.0 | EquipMask::Cloak.0);
        if !wielded_intersects(wi.current_wielded_location, covering) {
            continue;
        }

        let Some(clothing_base) = wi.clothing_base else {
            obj_desc = add_setup_as_clothing_base(w, obj_desc, *wg);
            // Add any potentially added parts back into the coverage list
            for a in &obj_desc.anim_part_changes {
                if !coverage.contains(&u32::from(a.index)) {
                    coverage.push(u32::from(a.index));
                }
            }
            continue;
        };
        // `ReadFromDat<ClothingTable>`: a missing file is ACE's empty table (no effects).
        let Some(item) = shims::read_clothing_table(w, clothing_base) else {
            continue;
        };

        // Check if the player model has data. Gear Knights, this is usually you.
        let clothing_base_effect = item
            .clothing_bases
            .get(&DataId(setup_table_id))
            .or_else(|| item.clothing_bases.get(&DataId(this_setup_id)));
        let Some(clothing_base_effect) = clothing_base_effect else {
            continue;
        };

        // Add the model and texture(s). Check if the original model has AnimParts defined, otherwise use the fallback if different
        for t in clothing_base_effect {
            let part_num = t.part_num as u8;
            coverage.push(u32::from(part_num));

            obj_desc.add_anim_part_change(PropertiesAnimPart {
                index: t.part_num as u8,
                animation_id: t.object_id.0,
            });

            for t1 in &t.texture_effects {
                obj_desc.add_texture_change(PropertiesTextureMap {
                    part_index: t.part_num as u8,
                    old_texture: t1.old_texture.0,
                    new_texture: t1.new_texture.0,
                });
            }
        }

        if !item.palette_templates.is_empty() {
            let pal_option = wi.palette_template.unwrap_or(0);
            let item_sub_pal = match item.palette_templates.get(&(pal_option as u32)) {
                Some(p) => p.clone(),
                None => shims::first_palette_template(w, clothing_base, &item),
            };

            // `(float)w.Shade`, widened again by `GetPaletteID(double)`.
            let shade = f64::from(wi.shade.map_or(0.0f32, |s| s as f32));
            for sub in &item_sub_pal.subpalette_effects {
                let item_pal =
                    shims::palette_set_get_palette_id(w, sub.palette_set.0, shade) as u16;

                for range in &sub.ranges {
                    let pal_offset = (range.offset / 8) as u16;
                    let num_colors = (range.length / 8) as u16;
                    obj_desc.sub_palettes.push(PropertiesPalette {
                        sub_palette_id: u32::from(item_pal),
                        offset: pal_offset,
                        length: num_colors,
                    });
                }
            }
        }
    }

    // Add the "naked" body parts. These are the ones not already covered.
    // Note that this is the original SetupTableId, not thisSetupId.
    if setup_table_id > 0 {
        // `ReadFromDat<SetupModel>`: a missing file is ACE's empty setup (no parts).
        if let Some(base_setup) = shims::c_setup(w, setup_table_id) {
            // A body of the character-creation table wears that table's bare parts where they
            // are not the setup's own (V430).
            let bare = bare_body_parts(w, this, setup_table_id);
            // `for (byte i = 0; i < baseSetup.Parts.Count; i++)`
            for (i, part) in base_setup.parts.iter().enumerate() {
                let i = i as u8;
                // Don't add body parts for those that are already covered. Also don't add the head, that was already covered by AddCharacterBaseModelData()
                if !coverage.contains(&u32::from(i)) && i != 0x10 {
                    let model = bare
                        .iter()
                        .find(|(p, _)| *p == i)
                        .map_or(part.0, |(_, g)| *g);
                    obj_desc.anim_part_changes.push(PropertiesAnimPart {
                        index: i,
                        animation_id: model,
                    });
                }
                //AddModel(i, baseSetup.Parts[i]);
            }
        }
    }

    if coverage.is_empty()
        && w.objects
            .get(this)
            .expect("ACE: this is null")
            .clothing_base()
            .is_some()
    {
        return crate::world_objects::world_object_networking::world_object_calculate_obj_desc(
            w, this,
        );
    }

    obj_desc
}

/// The bare parts the character-creation table gives the body of `this`'s heritage and sex, as
/// `(part, model)`, where its base description names one: empty for an object of no heritage or
/// sex, or built on another setup than that body's (V430). The end-of-retail table's are the
/// setup's own parts but for the female abdomen; the February 2005 table's arms and hands are
/// bare models the setup does not carry (its own are armoured).
fn bare_body_parts(w: &World, this: ObjectGuid, setup_table_id: u32) -> Vec<(u8, u32)> {
    let o = w.objects.get(this).expect("ACE: this is null");
    let (Some(heritage), Some(gender)) = (o.heritage(), o.gender()) else {
        return Vec::new();
    };
    let cg = w.dats.portal_dat().char_gen();
    let sex = u32::try_from(heritage)
        .ok()
        .and_then(|h| cg.heritage_groups.get(&h))
        .zip(u32::try_from(gender).ok())
        .and_then(|(hg, g)| hg.sexes.get(&g));
    match sex {
        Some(sex) if sex.setup.0 == setup_table_id => sex
            .base_objdesc
            .anim_part_changes
            .iter()
            .map(|(p, g)| (*p, g.0))
            .collect(),
        _ => Vec::new(),
    }
}

// ACE: Creature.AddSetupAsClothingBase
/// Certain items do not contain a ClothingBase (Ursuin Guise, WCID 32155, is one of them): the
/// item's own setup parts stand in for one.
pub fn add_setup_as_clothing_base(w: &World, mut obj_desc: ObjDesc, wo: ObjectGuid) -> ObjDesc {
    let setup_table_id = w.objects.get(wo).expect("ACE: wo is null").setup_table_id();
    // `wo.CSetup` is never null: a setup the portal dat does not have is ACE's empty SetupModel
    // (`ReadFromDat` caches `new T()`), with no parts.
    let parts = shims::c_setup(w, setup_table_id)
        .map(|c| c.parts.clone())
        .unwrap_or_default();
    // Loop over the parts in the Setup of the WorldObject
    for (i, part) in parts.iter().enumerate() {
        // This is essentially a "null" part, so do not add it for the head
        if part.0 != 0x0100_01EC || i != 16 {
            obj_desc.anim_part_changes.push(PropertiesAnimPart {
                index: i as u8,
                animation_id: part.0,
            });
        }
    }

    obj_desc
}

// ACE: Creature.WriteIdentifyObjectCreatureProfile
pub fn write_identify_object_creature_profile(
    w: &mut World,
    writer: &mut Vec<u8>,
    creature: ObjectGuid,
    success: bool,
) {
    let creature_profile = creature_profile_new(w, creature, success);
    creature_profile::write(writer, &creature_profile);
}
