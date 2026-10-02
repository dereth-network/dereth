// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/AppraiseInfo.cs
//! Port of `Source/ACE.Server/Network/Structure/AppraiseInfo.cs`.
//!
//! The property tables are written through a `SortedDictionary` with a bucket comparer, so their
//! order on the wire does not depend on dictionary order. What `BuildProfile` reads from systems
//! other units port (the enchantment manager, ratings, allegiance, fellowship, lock and house
//! helpers, character options, the property manager) goes through the named shims in
//! `world_object_networking::shims`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // C# casts

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::ext::assessment_properties;
use empyrean_entity::enums::{
    AppraisalLongDescDecorations, CharacterOption, FactionBits, Gender, HeritageGroup, HouseStatus,
    HouseType, IdentifyResponseFlags, ImbuedEffectType, ItemType, MagicSchool, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString,
    Skill, SpellCategory, WeenieType,
};
use empyrean_entity::ObjectGuid;

use super::armor_level::{self, armor_level_new, ArmorLevel};
use super::armor_profile::{self, armor_profile_new, ArmorProfile};
use super::creature_profile::{self, creature_profile_new, CreatureProfile};
use super::hash_comparer::{
    sorted, PropertyBoolComparer, PropertyDataIdComparer, PropertyFloatComparer,
    PropertyInt64Comparer, PropertyIntComparer, PropertyStringComparer,
};
use super::hook_profile::{self, HookFlags, HookProfile};
use super::weapon_profile::{self, weapon_profile_new, WeaponProfile};
use crate::network::game_messages::game_message::{ace_str, write_record};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;
use dereth_protocol::archive::PackedHash;

const ENCHANTMENT_MASK: u32 = 0x8000_0000;

// ACE: AppraiseInfo
/// Handles calculating and sending all object appraisal info.
#[derive(Debug, Clone, Default)]
pub struct AppraiseInfo {
    // ACE: AppraiseInfo.Flags
    pub flags: IdentifyResponseFlags,

    // ACE: AppraiseInfo.Success
    /// assessment successful?
    pub success: bool,

    // ACE: AppraiseInfo.PropertiesInt
    pub properties_int: DotNetDict<PropertyInt, i32>,
    // ACE: AppraiseInfo.PropertiesInt64
    pub properties_int64: DotNetDict<PropertyInt64, i64>,
    // ACE: AppraiseInfo.PropertiesBool
    pub properties_bool: DotNetDict<PropertyBool, bool>,
    // ACE: AppraiseInfo.PropertiesFloat
    pub properties_float: DotNetDict<PropertyFloat, f64>,
    // ACE: AppraiseInfo.PropertiesString
    pub properties_string: DotNetDict<PropertyString, String>,
    // ACE: AppraiseInfo.PropertiesDID
    pub properties_did: DotNetDict<PropertyDataId, u32>,
    // ACE: AppraiseInfo.PropertiesIID
    pub properties_iid: DotNetDict<PropertyInstanceId, u32>,

    // ACE: AppraiseInfo.SpellBook
    pub spell_book: Vec<u32>,

    // ACE: AppraiseInfo.ArmorProfile
    pub armor_profile: Option<ArmorProfile>,
    // ACE: AppraiseInfo.CreatureProfile
    pub creature_profile: Option<CreatureProfile>,
    // ACE: AppraiseInfo.WeaponProfile
    pub weapon_profile: Option<WeaponProfile>,
    // ACE: AppraiseInfo.HookProfile
    pub hook_profile: Option<HookProfile>,

    // ACE: AppraiseInfo.ArmorHighlight
    /// `ArmorMask`.
    pub armor_highlight: u32,
    // ACE: AppraiseInfo.ArmorColor
    pub armor_color: u32,
    // ACE: AppraiseInfo.WeaponHighlight
    /// `WeaponMask`.
    pub weapon_highlight: u32,
    // ACE: AppraiseInfo.WeaponColor
    pub weapon_color: u32,
    // ACE: AppraiseInfo.ResistHighlight
    /// `ResistMask`.
    pub resist_highlight: u32,
    // ACE: AppraiseInfo.ResistColor
    pub resist_color: u32,

    // ACE: AppraiseInfo.ArmorLevels
    pub armor_levels: Option<ArmorLevel>,

    // ACE: AppraiseInfo.NPCLooksLikeObject
    /// This helps ensure the item will identify properly. Some "items" are technically
    /// "Creatures". (Private in ACE.)
    pub npc_looks_like_object: bool,
}

// ACE: AppraiseInfo.AppraiseInfo
/// `new AppraiseInfo()`: the empty appraisal (no flags, not a success).
#[must_use]
pub fn appraise_info_empty() -> AppraiseInfo {
    AppraiseInfo {
        flags: IdentifyResponseFlags::None,
        success: false,
        ..Default::default()
    }
}

// ACE: AppraiseInfo.AppraiseInfo
/// `new AppraiseInfo(WorldObject wo, Player examiner, bool success = true)`: construct all of
/// the info required for appraising any WorldObject.
pub fn appraise_info_new(
    w: &mut World,
    wo: ObjectGuid,
    examiner: ObjectGuid,
    success: bool,
) -> AppraiseInfo {
    let mut info = AppraiseInfo::default();
    info.build_profile(w, wo, examiner, success);
    info
}

/// A `HashSet<ushort>` of the given property ids (ACE's `AssessmentProperties.*`).
fn set<T: Copy>(ids: &[T], id: impl Fn(T) -> u16) -> DotNetHashSet<u16> {
    ids.iter().map(|&i| id(i)).collect()
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .expect("ACE: null WorldObject (NullReferenceException)")
}

/// `dict[key] += value` on a key that must exist (`KeyNotFoundException` otherwise).
fn add_to<K: std::hash::Hash + Eq + Clone + std::fmt::Debug, V: std::ops::AddAssign + Copy>(
    dict: &mut DotNetDict<K, V>,
    key: K,
    value: V,
) {
    let entry = dict
        .get_mut(&key)
        .unwrap_or_else(|| panic!("ACE: KeyNotFoundException {key:?}"));
    *entry += value;
}

impl AppraiseInfo {
    // ACE: AppraiseInfo.BuildProfile
    #[allow(clippy::too_many_lines)]
    pub fn build_profile(
        &mut self,
        w: &mut World,
        wo: ObjectGuid,
        examiner: ObjectGuid,
        success: bool,
    ) {
        //Console.WriteLine("Appraise: " + wo.Guid);
        self.success = success;

        self.build_properties(w, wo);
        self.build_spells(w, wo);

        let o = obj(w, wo);

        // Help us make sure the item identify properly
        self.npc_looks_like_object = o
            .get_property(PropertyBool::NpcLooksLikeObject)
            .unwrap_or(false);

        let allowed_wielder = o.get_property(PropertyInstanceId::AllowedWielder);
        if allowed_wielder.is_some_and(|a| a > 0)
            && !self
                .properties_bool
                .contains_key(&PropertyBool::AppraisalHasAllowedWielder)
        {
            self.properties_bool
                .insert(PropertyBool::AppraisalHasAllowedWielder, true);
        }

        let allowed_activator = o.get_property(PropertyInstanceId::AllowedActivator);
        if allowed_activator.is_some_and(|a| a > 0)
            && !self
                .properties_bool
                .contains_key(&PropertyBool::AppraisalHasAllowedActivator)
        {
            self.properties_bool
                .insert(PropertyBool::AppraisalHasAllowedActivator, true);
        }

        if self
            .properties_string
            .contains_key(&PropertyString::ScribeAccount)
            && !shims::player_is_staff(w, examiner)
        {
            self.properties_string
                .remove(&PropertyString::ScribeAccount);
        }

        // not used
        //var houseOwnerAccount = wo.GetProperty(PropertyString.HouseOwnerAccount);
        //if (!string.IsNullOrWhiteSpace(houseOwnerAccount) && (examiner.IsAdmin || examiner.IsSentinel || examiner.IsEnvoy || examiner.IsArch || examiner.IsPsr))
        //    PropertiesString[PropertyString.HouseOwnerAccount] = houseOwnerAccount;

        if self.properties_int.contains_key(&PropertyInt::Lifespan) {
            let remaining = shims::get_remaining_lifespan(w, wo);
            self.properties_int
                .insert(PropertyInt::RemainingLifespan, remaining);
        }

        if let Some(&faction1_bits) = self.properties_int.get(&PropertyInt::Faction1Bits) {
            // hide any non-default factions, prevent client from displaying ???
            // this is only needed for non-standard faction creatures that use templates, to hide the ??? in the client
            let send_bits = faction1_bits & FactionBits::ValidFactions.0;
            if send_bits != faction1_bits {
                if send_bits != 0 {
                    self.properties_int
                        .insert(PropertyInt::Faction1Bits, send_bits);
                } else {
                    self.properties_int.remove(&PropertyInt::Faction1Bits);
                }
            }
        }

        let o = obj(w, wo);

        // armor / clothing / shield
        if o.is_clothing() || o.is_shield() {
            self.build_armor(w, wo);
        }

        if obj(w, wo).is_creature() {
            self.build_creature(w, wo);
        }

        let o = obj(w, wo);
        if o.damage().is_some() && !o.is_clothing()
            || o.is_melee_weapon()
            || o.is_missile()
            || o.is_missile_launcher()
            || o.is_ammunition()
            || o.is_caster()
        {
            self.build_weapon(w, wo);
        }

        let o = obj(w, wo);
        // TODO: Resolve this issue a better way?
        // Because of the way ACE handles default base values in recipe system (or rather the lack thereof)
        // we need to check the following weapon properties to see if they're below expected minimum and adjust accordingly
        // The issue is that the recipe system likely added 0.005 to 0 instead of 1, which is what *should* have happened.
        let imbue_stacking_bits = o.get_property(PropertyInt::ImbueStackingBits).unwrap_or(0);
        if o.weapon_magic_defense().is_some_and(|v| v > 0.0 && v < 1.0)
            && (imbue_stacking_bits & 1) != 0
        {
            add_to(
                &mut self.properties_float,
                PropertyFloat::WeaponMagicDefense,
                1.0,
            );
        }
        if o.weapon_missile_defense()
            .is_some_and(|v| v > 0.0 && v < 1.0)
            && (imbue_stacking_bits & 1) != 0
        {
            add_to(
                &mut self.properties_float,
                PropertyFloat::WeaponMissileDefense,
                1.0,
            );
        }

        // Mask real value of AbsorbMagicDamage and/or Add AbsorbMagicDamage for ImbuedEffectType.IgnoreSomeMagicProjectileDamage
        if self
            .properties_float
            .contains_key(&PropertyFloat::AbsorbMagicDamage)
            || shims::has_imbued_effect(w, wo, ImbuedEffectType::IgnoreSomeMagicProjectileDamage)
        {
            self.properties_float
                .insert(PropertyFloat::AbsorbMagicDamage, 1.0);
        }

        let o = obj(w, wo);
        if o.is_door() || o.is_chest() {
            // If wo is not locked, do not send ResistLockpick value. If ResistLockpick is sent for unlocked objects, id panel shows bonus to Lockpick skill
            if !o.is_locked()
                && self
                    .properties_int
                    .contains_key(&PropertyInt::ResistLockpick)
            {
                self.properties_int.remove(&PropertyInt::ResistLockpick);
            }

            // If wo is locked, append skill check percent, as int, to properties for id panel display on chances of success
            if o.is_locked() {
                let resist_lockpick = shims::lock_helper_get_resist_lockpick(w, wo);

                if let Some(resist_lockpick) = resist_lockpick {
                    self.properties_int
                        .insert(PropertyInt::ResistLockpick, resist_lockpick);

                    let pick_skill = shims::creature_skill_current(w, examiner, Skill::Lockpick);

                    let success_chance = crate::world_objects::skill_check::get_skill_chance(
                        pick_skill as i32,
                        resist_lockpick,
                        0.03,
                    ) * 100.0;

                    if !self
                        .properties_int
                        .contains_key(&PropertyInt::AppraisalLockpickSuccessPercent)
                    {
                        self.properties_int.add(
                            PropertyInt::AppraisalLockpickSuccessPercent,
                            empyrean_common::dotnet::CsCast::<i32>::cs_cast(success_chance),
                        );
                    }
                }
            } else {
                // if wo has DefaultLocked property and is unlocked, add that state to the property buckets
                let default_locked = o.get_property(PropertyBool::DefaultLocked);
                if default_locked.unwrap_or(false) {
                    self.properties_bool.insert(PropertyBool::Locked, false);
                }
            }
        }

        let o = obj(w, wo);
        if o.is_corpse() {
            self.properties_bool.clear();
            self.properties_did.clear();
            self.properties_float.clear();
            self.properties_int64.clear();

            let discard_ints: Vec<PropertyInt> = self
                .properties_int
                .keys()
                .copied()
                .filter(|k| *k != PropertyInt::EncumbranceVal && *k != PropertyInt::Value)
                .collect();
            for key in discard_ints {
                self.properties_int.remove(&key);
            }
            let discard_string: Vec<PropertyString> = self
                .properties_string
                .keys()
                .copied()
                .filter(|k| *k != PropertyString::LongDesc)
                .collect();
            for key in discard_string {
                self.properties_string.remove(&key);
            }

            self.properties_int.insert(PropertyInt::Value, 0);
        }

        if o.is_portal()
            && self
                .properties_int
                .contains_key(&PropertyInt::EncumbranceVal)
        {
            self.properties_int.remove(&PropertyInt::EncumbranceVal);
        }

        if o.is_slum_lord() {
            self.build_slum_lord(w, wo);
        }

        let o = obj(w, wo);
        if o.is_container() && self.properties_int.contains_key(&PropertyInt::Value) {
            // Value is masked to base value of Weenie
            let base_value = shims::weenie_get_value(w, o.biota.weenie_class_id).unwrap_or(0);
            self.properties_int.insert(PropertyInt::Value, base_value);
        }

        if o.is_storage() {
            let mut long_desc = String::new();

            if o.house_owner().is_some_and(|h| h > 0) {
                long_desc = format!(
                    "Owned by {}\n",
                    shims::parent_link_house_owner_name(w, wo).unwrap_or_default()
                );
            }

            let discard_string: Vec<PropertyString> = self
                .properties_string
                .keys()
                .copied()
                .filter(|k| *k != PropertyString::Use)
                .collect();
            for key in discard_string {
                self.properties_string.remove(&key);
            }

            self.properties_string
                .add(PropertyString::LongDesc, long_desc);
        }

        if obj(w, wo).is_hook() {
            self.build_hook(w, wo, examiner, success);
        }

        let o = obj(w, wo);
        if o.is_mana_stone() {
            let use_message = if o.item_cur_mana().is_some() {
                "Use on a magic item to give the stone's stored Mana to that item."
            } else {
                "Use on a magic item to destroy that item and drain its Mana."
            };

            self.properties_string
                .insert(PropertyString::Use, use_message.to_owned());
        }

        let wcid = o.biota.weenie_class_id;
        if o.is_craft_tool()
            && (o.item_type() == ItemType::TinkeringMaterial
                || (36619..=36628).contains(&wcid)
                || (36634..=36636).contains(&wcid))
            && self.properties_int.contains_key(&PropertyInt::Structure)
        {
            self.properties_int.remove(&PropertyInt::Structure);
        }

        if !self.success {
            // todo: what specifically to keep/what to clear

            //PropertiesBool.Clear();
            //PropertiesDID.Clear();
            //PropertiesFloat.Clear();
            //PropertiesIID.Clear();
            //PropertiesInt.Clear();
            //PropertiesInt64.Clear();
            //PropertiesString.Clear();
        }

        self.build_flags();
    }

    /// The `wo is SlumLord slumLord` block of `BuildProfile`.
    fn build_slum_lord(&mut self, w: &World, wo: ObjectGuid) {
        self.properties_bool.clear();
        self.properties_did.clear();
        self.properties_float.clear();
        self.properties_iid.clear();
        //PropertiesInt.Clear();
        self.properties_int64.clear();
        self.properties_string.clear();

        let slum_lord = obj(w, wo);

        let mut long_desc;

        if slum_lord.house_owner().is_some_and(|h| h > 0) {
            let paid = shims::slum_lord_is_rent_paid(w, wo)
                || !shims::property_manager_get_bool(w, "house_rent_enabled", false);
            long_desc = format!(
                "The current maintenance has {}been paid.\n",
                if paid { "" } else { "not " }
            );

            self.properties_int.clear();
        } else {
            //longDesc = $"This house is {(slumLord.HouseStatus == HouseStatus.Disabled ? "not " : "")}available for purchase.\n"; // this was the retail msg.
            let house = shims::slum_lord_house(w, wo)
                .map(|h| obj(w, h))
                .expect("ACE: slumLord.House is null");
            let house_type = house.house_type();
            let type_name = if house_type == HouseType::Undef {
                "house".to_owned()
            } else {
                house_type.to_dotnet_string().to_lowercase()
            };
            long_desc = format!(
                "This {type_name} is {}available for purchase.\n",
                if house.house_status() == HouseStatus::Disabled {
                    "not "
                } else {
                    ""
                }
            );

            let discard_ints: Vec<PropertyInt> = self
                .properties_int
                .keys()
                .copied()
                .filter(|k| {
                    *k != PropertyInt::HouseStatus
                        && *k != PropertyInt::HouseType
                        && *k != PropertyInt::MinLevel
                        && *k != PropertyInt::MaxLevel
                        && *k != PropertyInt::AllegianceMinLevel
                        && *k != PropertyInt::AllegianceMaxLevel
                })
                .collect();
            for key in discard_ints {
                self.properties_int.remove(&key);
            }
        }

        let slum_lord = obj(w, wo);
        if slum_lord.house_requires_monarch() {
            long_desc += "You must be a monarch to purchase and maintain this dwelling.\n";
        }

        if let Some(slum_lord_min) = slum_lord.allegiance_min_level() {
            let mut allegiance_min_level =
                shims::property_manager_get_long(w, "mansion_min_rank", -1);
            if allegiance_min_level == -1 {
                allegiance_min_level = i64::from(slum_lord_min);
            }

            long_desc += &format!(
                "Restricted to characters of allegiance rank {allegiance_min_level} or greater.\n"
            );
        }

        self.properties_string
            .add(PropertyString::LongDesc, long_desc);
    }

    /// The `wo is Hook` block of `BuildProfile`: a hook with one item shows that item's profile.
    fn build_hook(&mut self, w: &mut World, wo: ObjectGuid, examiner: ObjectGuid, success: bool) {
        // If the hook has any inventory, we need to send THOSE properties instead.
        let hook_inventory = crate::world_objects::container::inventory_values(w, wo);

        let mut base_desc_string = String::new();
        if shims::parent_link_house_owner(w, wo).is_some() {
            // This is for backwards compatibility. This value was not set/saved in earlier versions.
            // It will get the player's name and save that to the HouseOwnerName property of the house. This is now done when a player purchases a house.
            if shims::parent_link_house_owner_name(w, wo).is_none() {
                let owner = shims::parent_link_house_owner(w, wo).expect("checked above");
                if let Some(house_owner_player) = shims::player_manager_find_by_guid(w, owner) {
                    shims::parent_link_set_house_owner_name_and_save(
                        w,
                        wo,
                        house_owner_player.name,
                    );
                }
            }
            //if house is owned, display this text
            base_desc_string = format!(
                "This hook is owned by {}. ",
                shims::parent_link_house_owner_name(w, wo).unwrap_or_default()
            );
        }

        let mut contains_string = String::new();
        if hook_inventory.len() == 1 {
            let hooked_item = hook_inventory[0];

            // Hooked items have a custom "description", containing the desc of the sub item and who the owner of the house is (if any)
            self.build_profile(w, hooked_item, examiner, success);

            contains_string = "It contains: \n".to_owned();

            let hooked = obj(w, hooked_item);
            match hooked.long_desc() {
                Some(long_desc) if !long_desc.chars().all(char::is_whitespace) => {
                    contains_string += &long_desc
                }
                //else if (PropertiesString.ContainsKey(PropertyString.ShortDesc) && PropertiesString[PropertyString.ShortDesc] != null)
                //{
                //    containsString += PropertiesString[PropertyString.ShortDesc];
                //}
                _ => {
                    contains_string +=
                        &crate::dispatch::name::name(w, hooked_item).unwrap_or_default()
                }
            }

            self.build_hook_profile(w, hooked_item);
        }

        //if (PropertiesString.ContainsKey(PropertyString.LongDesc) && PropertiesString[PropertyString.LongDesc] != null)
        //    PropertiesString[PropertyString.LongDesc] = baseDescString + containsString;
        ////else if (PropertiesString.ContainsKey(PropertyString.ShortDesc) && PropertiesString[PropertyString.ShortDesc] != null)
        ////    PropertiesString[PropertyString.LongDesc] = baseDescString + containsString;
        //else
        //    PropertiesString[PropertyString.LongDesc] = baseDescString + containsString;

        self.properties_string.insert(
            PropertyString::LongDesc,
            base_desc_string + &contains_string,
        );

        self.properties_int.remove(&PropertyInt::Structure);

        // retail should have removed this property and then server side built the same result for the hook longdesc replacement but didn't and ends up with some odd looking appraisals as seen on video/pcaps
        //PropertiesInt.Remove(PropertyInt.AppraisalLongDescDecoration);
    }

    // ACE: AppraiseInfo.BuildProperties
    fn build_properties(&mut self, w: &World, wo: ObjectGuid) {
        let o = obj(w, wo);
        self.properties_int =
            o.get_all_property_int_where(&set(assessment_properties::PROPERTIES_INT, |p| p.0));
        self.properties_int64 =
            o.get_all_property_int64_where(&set(assessment_properties::PROPERTIES_INT64, |p| p.0));
        self.properties_bool =
            o.get_all_property_bools_where(&set(assessment_properties::PROPERTIES_BOOL, |p| p.0));
        self.properties_float =
            o.get_all_property_float_where(&set(assessment_properties::PROPERTIES_DOUBLE, |p| p.0));
        self.properties_string = o
            .get_all_property_string_where(&set(assessment_properties::PROPERTIES_STRING, |p| p.0));
        self.properties_did = o
            .get_all_property_data_id_where(&set(assessment_properties::PROPERTIES_DATA_ID, |p| {
                p.0
            }));
        self.properties_iid = o.get_all_property_instance_id_where(&set(
            assessment_properties::PROPERTIES_INSTANCE_ID,
            |p| p.0,
        ));

        if o.is_player() {
            let player = wo;
            // handle character options
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourDateOfBirth,
            ) {
                self.properties_string.remove(&PropertyString::DateOfBirth);
            }
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourAge,
            ) {
                self.properties_int.remove(&PropertyInt::Age);
            }
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourChessRank,
            ) {
                self.properties_int.remove(&PropertyInt::ChessRank);
            }
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourFishingSkill,
            ) {
                self.properties_int.remove(&PropertyInt::FakeFishingSkill);
            }
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourNumberOfDeaths,
            ) {
                self.properties_int.remove(&PropertyInt::NumDeaths);
            }
            if !shims::player_get_character_option(
                w,
                player,
                CharacterOption::AllowOthersToSeeYourNumberOfTitles,
            ) {
                self.properties_int.remove(&PropertyInt::NumCharacterTitles);
            }

            // handle dynamic properties for appraisal
            if let Some(allegiance) = shims::player_allegiance_appraisal(w, player) {
                if let Some(name) = allegiance.allegiance_name {
                    self.properties_string
                        .insert(PropertyString::AllegianceName, name);
                }

                if allegiance.is_monarch {
                    self.properties_int
                        .insert(PropertyInt::AllegianceFollowers, allegiance.total_followers);
                } else {
                    let (monarch, patron) = (allegiance.monarch, allegiance.patron);

                    let title = |m: &shims::AllegianceMemberView| {
                        format!(
                            "{} {}",
                            crate::entity::allegiance_rank::get_title(
                                HeritageGroup(m.heritage.unwrap_or(0)),
                                Gender(m.gender.unwrap_or(0)),
                                m.rank
                            ),
                            m.name
                        )
                    };
                    self.properties_string
                        .insert(PropertyString::MonarchsTitle, title(&monarch));
                    self.properties_string
                        .insert(PropertyString::PatronsTitle, title(&patron));
                }
            }

            if let Some(fellowship_name) = shims::player_fellowship_name(w, player) {
                self.properties_string
                    .insert(PropertyString::Fellowship, fellowship_name);
            }
        }

        self.add_property_enchantments(w, wo);
    }

    // ACE: AppraiseInfo.AddPropertyEnchantments
    fn add_property_enchantments(&mut self, w: &World, wo: ObjectGuid) {
        let Some(o) = w.objects.get(wo) else { return };

        if self.properties_int.contains_key(&PropertyInt::ArmorLevel) {
            add_to(
                &mut self.properties_int,
                PropertyInt::ArmorLevel,
                shims::enchantment_manager_get_armor_mod(w, wo),
            );
        }

        if let Some(item_skill_limit) = o.item_skill_limit() {
            self.properties_int
                .insert(PropertyInt::AppraisalItemSkill, item_skill_limit.0);
        } else {
            self.properties_int.remove(&PropertyInt::AppraisalItemSkill);
        }

        if self
            .properties_float
            .contains_key(&PropertyFloat::WeaponDefense)
            && !o.is_ammunition()
        {
            let defense_mod = shims::enchantment_manager_get_defense_mod(w, wo);
            let aura_defense_mod = match o.wielder {
                Some(wielder) if shims::is_enchantable(w, wo) => {
                    shims::enchantment_manager_get_defense_mod(w, wielder)
                }
                _ => 0.0,
            };

            add_to(
                &mut self.properties_float,
                PropertyFloat::WeaponDefense,
                f64::from(defense_mod + aura_defense_mod),
            );
        }

        if let Some(&mana_conv_mod) = self.properties_float.get(&PropertyFloat::ManaConversionMod) {
            if mana_conv_mod != 0.0 {
                // hermetic link/void
                let enchantment_mod = shims::resist_mask_helper_get_mana_conversion_mod(w, wo);

                if enchantment_mod != 1.0 {
                    let v = self
                        .properties_float
                        .get_mut(&PropertyFloat::ManaConversionMod)
                        .expect("present");
                    *v *= f64::from(enchantment_mod);

                    self.resist_highlight = shims::resist_mask_helper_get_highlight_mask(w, wo);
                    self.resist_color = shims::resist_mask_helper_get_color_mask(w, wo);
                }
            } else if !shims::property_manager_get_bool(w, "show_mana_conv_bonus_0", false) {
                self.properties_float
                    .remove(&PropertyFloat::ManaConversionMod);
            }
        }

        if self
            .properties_float
            .contains_key(&PropertyFloat::ElementalDamageMod)
        {
            let enchantment_bonus = shims::resist_mask_helper_get_elemental_damage_bonus(w, wo);

            if enchantment_bonus != 0.0 {
                add_to(
                    &mut self.properties_float,
                    PropertyFloat::ElementalDamageMod,
                    f64::from(enchantment_bonus),
                );

                self.resist_highlight = shims::resist_mask_helper_get_highlight_mask(w, wo);
                self.resist_color = shims::resist_mask_helper_get_color_mask(w, wo);
            }
        }

        let mut appraisal_long_desc_decoration = AppraisalLongDescDecorations::None;

        if o.item_workmanship().is_some_and(|v| v > 0) {
            appraisal_long_desc_decoration |= AppraisalLongDescDecorations::PrependWorkmanship;
        }
        if o.material_type().is_some_and(|v| v.0 > 0) {
            appraisal_long_desc_decoration |= AppraisalLongDescDecorations::PrependMaterial;
        }
        if o.gem_type().is_some_and(|v| v.0 > 0) && o.gem_count().is_some_and(|v| v > 0) {
            appraisal_long_desc_decoration |= AppraisalLongDescDecorations::AppendGemInfo;
        }

        // `LongDesc.StartsWith(Name)`: culture-sensitive in .NET; ordinal here (names are ASCII
        // in practice). A null Name throws in ACE.
        let long_desc_starts_with_name = o.long_desc().is_some_and(|long_desc| {
            let name = crate::dispatch::name::name(w, wo)
                .expect("ACE: Name is null (ArgumentNullException)");
            long_desc.starts_with(&name)
        });
        if appraisal_long_desc_decoration.0 > 0 && long_desc_starts_with_name {
            self.properties_int.insert(
                PropertyInt::AppraisalLongDescDecoration,
                appraisal_long_desc_decoration.0,
            );
        } else {
            self.properties_int
                .remove(&PropertyInt::AppraisalLongDescDecoration);
        }
    }

    // ACE: AppraiseInfo.BuildSpells
    fn build_spells(&mut self, w: &World, wo: ObjectGuid) {
        self.spell_book = Vec::new();

        let o = obj(w, wo);
        if o.is_creature() {
            return;
        }

        // add primary spell, if exists
        if let Some(spell_did) = o.spell_did() {
            self.spell_book.push(spell_did);
        }

        // add proc spell, if exists
        if let Some(proc_spell) = o.proc_spell() {
            self.spell_book.push(proc_spell);
        }

        let wo_spell_did = o.spell_did(); // prevent recursive lock
        let wo_proc_spell = o.proc_spell();

        // `Biota.GetKnownSpellsIdsWhere(i => i != woSpellDID && i != woProcSpell)`: the spell book
        // keys in dictionary order (`int` compared with `uint?` as `long`).
        if let Some(spell_book) = &o.biota.properties_spell_book {
            for spell_id in spell_book.keys() {
                let id = i64::from(*spell_id);
                if Some(id) != wo_spell_did.map(i64::from)
                    && Some(id) != wo_proc_spell.map(i64::from)
                {
                    self.spell_book.push(*spell_id as u32);
                }
            }
        }
    }

    // ACE: AppraiseInfo.AddEnchantments
    fn add_enchantments(&mut self, w: &World, wo: ObjectGuid) {
        let Some(o) = w.objects.get(wo) else { return };

        // get all currently active item enchantments on the item
        let wo_enchantments =
            shims::enchantment_manager_get_enchantments(w, wo, MagicSchool::ItemEnchantment);

        for enchantment in &wo_enchantments {
            self.spell_book
                .push(enchantment.spell_id.cast_unsigned() | ENCHANTMENT_MASK);
        }

        // show auras from wielder, if applicable

        // this technically wasn't a feature in retail

        let Some(wielder) = o.wielder else { return };
        if shims::is_enchantable(w, wo)
            && o.biota.weenie_type != WeenieType::Clothing
            && !o.is_shield()
            && shims::property_manager_get_bool(w, "show_aura_buff", false)
        {
            // get all currently active item enchantment auras on the player
            let wielder_enchantments = shims::enchantment_manager_get_enchantments(
                w,
                wielder,
                MagicSchool::ItemEnchantment,
            );

            // Only show reflected Auras from player appropriate for wielded weapons
            for enchantment in &wielder_enchantments {
                let c = enchantment.spell_category;
                let show = if o.is_caster() {
                    // Caster weapon only item Auras
                    c == SpellCategory::DefenseModRaising
                        || c == SpellCategory::DefenseModRaisingRare
                        || c == SpellCategory::ManaConversionModRaising
                        || c == SpellCategory::SpellDamageRaising
                } else if o.is_missile() || o.is_ammunition() {
                    c == SpellCategory::DamageRaising || c == SpellCategory::DamageRaisingRare
                } else {
                    // Other weapon type Auras
                    c == SpellCategory::AttackModRaising
                        || c == SpellCategory::AttackModRaisingRare
                        || c == SpellCategory::DamageRaising
                        || c == SpellCategory::DamageRaisingRare
                        || c == SpellCategory::DefenseModRaising
                        || c == SpellCategory::DefenseModRaisingRare
                        || c == SpellCategory::WeaponTimeRaising
                        || c == SpellCategory::WeaponTimeRaisingRare
                };
                if show {
                    self.spell_book
                        .push(enchantment.spell_id.cast_unsigned() | ENCHANTMENT_MASK);
                }
            }
        }
    }

    // ACE: AppraiseInfo.BuildArmor
    fn build_armor(&mut self, w: &World, wo: ObjectGuid) {
        if !self.success {
            return;
        }

        self.armor_profile = Some(armor_profile_new(w, wo));
        self.armor_highlight = shims::armor_mask_helper_get_highlight_mask(w, wo);
        self.armor_color = shims::armor_mask_helper_get_color_mask(w, wo);

        self.add_enchantments(w, wo);
    }

    // ACE: AppraiseInfo.BuildCreature
    fn build_creature(&mut self, w: &mut World, creature: ObjectGuid) {
        self.creature_profile = Some(creature_profile_new(w, creature, self.success));

        // only creatures?
        self.resist_highlight = shims::resist_mask_helper_get_highlight_mask(w, creature);
        self.resist_color = shims::resist_mask_helper_get_color_mask(w, creature);

        // DIVERGE: an era before assessment showed armour levels and ratings
        // (`EraFeatures::assessed_armor_and_ratings`) sends neither (ClassicACE's `BuildCreature`).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Network/Structure/AppraiseInfo.cs
        if w.era.features.assessed_armor_and_ratings {
            let c = obj(w, creature);
            if self.success && (c.is_player() || !c.attackable()) {
                self.armor_levels = Some(armor_level_new(w, creature));
            }

            self.add_ratings(w, creature);
        }

        let c = obj(w, creature);
        if self.npc_looks_like_object {
            let has_encumbrance = match &c.weenie {
                Some(weenie) => weenie.get_property(PropertyInt::EncumbranceVal).is_some(),
                None => shims::weenie_get_property_int(
                    w,
                    c.biota.weenie_class_id,
                    PropertyInt::EncumbranceVal,
                )
                .is_some(),
            };

            if !has_encumbrance {
                self.properties_int.remove(&PropertyInt::EncumbranceVal);
            }
        } else {
            self.properties_int.remove(&PropertyInt::EncumbranceVal);
        }

        // see notes in CombatPet.Init()
        if c.is_combat_pet() && self.properties_int.contains_key(&PropertyInt::Faction1Bits) {
            self.properties_int.remove(&PropertyInt::Faction1Bits);
        }
    }

    // ACE: AppraiseInfo.AddRatings
    fn add_ratings(&mut self, w: &mut World, creature: ObjectGuid) {
        if !self.success {
            return;
        }

        let r = shims::creature_ratings(w, creature);

        if r.damage_rating != 0 {
            self.properties_int
                .insert(PropertyInt::DamageRating, r.damage_rating);
        }
        if r.damage_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::DamageResistRating, r.damage_resist_rating);
        }

        if r.crit_rating != 0 {
            self.properties_int
                .insert(PropertyInt::CritRating, r.crit_rating);
        }
        if r.crit_damage_rating != 0 {
            self.properties_int
                .insert(PropertyInt::CritDamageRating, r.crit_damage_rating);
        }

        if r.crit_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::CritResistRating, r.crit_resist_rating);
        }
        if r.crit_damage_resist_rating != 0 {
            self.properties_int.insert(
                PropertyInt::CritDamageResistRating,
                r.crit_damage_resist_rating,
            );
        }

        if r.healing_boost_rating != 0 {
            self.properties_int
                .insert(PropertyInt::HealingBoostRating, r.healing_boost_rating);
        }
        if r.nether_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::NetherResistRating, r.nether_resist_rating);
        }
        if r.dot_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::DotResistRating, r.dot_resist_rating);
        }

        if r.life_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::LifeResistRating, r.life_resist_rating);
        }
        if r.gear_max_health != 0 {
            self.properties_int
                .insert(PropertyInt::GearMaxHealth, r.gear_max_health);
        }

        if r.pk_damage_rating != 0 {
            self.properties_int
                .insert(PropertyInt::PKDamageRating, r.pk_damage_rating);
        }
        if r.pk_damage_resist_rating != 0 {
            self.properties_int
                .insert(PropertyInt::PKDamageResistRating, r.pk_damage_resist_rating);
        }

        // add ratings from equipped items?
    }

    // ACE: AppraiseInfo.BuildWeapon
    fn build_weapon(&mut self, w: &World, weapon: ObjectGuid) {
        if !self.success {
            return;
        }

        let weapon_profile = weapon_profile_new(w, weapon);

        //WeaponHighlight = WeaponMaskHelper.GetHighlightMask(weapon, wielder);
        //WeaponColor = WeaponMaskHelper.GetColorMask(weapon, wielder);
        self.weapon_highlight = shims::weapon_mask_helper_get_highlight_mask(w, &weapon_profile);
        self.weapon_color = shims::weapon_mask_helper_get_color_mask(w, &weapon_profile);

        if !obj(w, weapon).is_caster() {
            self.weapon_profile = Some(weapon_profile);
        }

        // item enchantments can also be on wielder currently
        self.add_enchantments(w, weapon);
    }

    // ACE: AppraiseInfo.BuildHookProfile
    fn build_hook_profile(&mut self, w: &World, hooked_item: ObjectGuid) {
        let mut hook_profile = HookProfile::default();
        let h = obj(w, hooked_item);
        if h.inscribable() {
            hook_profile.flags |= HookFlags::Inscribable;
        }
        if h.is_healer() {
            hook_profile.flags |= HookFlags::IsHealer;
        }
        if h.is_food() {
            hook_profile.flags |= HookFlags::IsFood;
        }
        if h.is_lockpick() {
            hook_profile.flags |= HookFlags::IsLockpick;
        }
        if let Some(valid_locations) = h.valid_locations() {
            hook_profile.valid_locations = valid_locations;
        }
        if let Some(ammo_type) = h.ammo_type() {
            hook_profile.ammo_type = ammo_type;
        }
        self.hook_profile = Some(hook_profile);
    }

    // ACE: AppraiseInfo.BuildFlags
    /// Constructs the bitflags for appraising a WorldObject. Flags only accumulate (a hook's item
    /// profile is built into the same appraisal first).
    fn build_flags(&mut self) {
        if !self.properties_int.is_empty() {
            self.flags |= IdentifyResponseFlags::IntStatsTable;
        }
        if !self.properties_int64.is_empty() {
            self.flags |= IdentifyResponseFlags::Int64StatsTable;
        }
        if !self.properties_bool.is_empty() {
            self.flags |= IdentifyResponseFlags::BoolStatsTable;
        }
        if !self.properties_float.is_empty() {
            self.flags |= IdentifyResponseFlags::FloatStatsTable;
        }
        if !self.properties_string.is_empty() {
            self.flags |= IdentifyResponseFlags::StringStatsTable;
        }
        if !self.properties_did.is_empty() {
            self.flags |= IdentifyResponseFlags::DidStatsTable;
        }
        if !self.spell_book.is_empty() {
            self.flags |= IdentifyResponseFlags::SpellBook;
        }

        if self.resist_highlight != 0 {
            self.flags |= IdentifyResponseFlags::ResistEnchantmentBitfield;
        }
        if self.armor_profile.is_some() {
            self.flags |= IdentifyResponseFlags::ArmorProfile;
        }
        if self.creature_profile.is_some() && !self.npc_looks_like_object {
            self.flags |= IdentifyResponseFlags::CreatureProfile;
        }
        if self.weapon_profile.is_some() {
            self.flags |= IdentifyResponseFlags::WeaponProfile;
        }
        if self.hook_profile.is_some() {
            self.flags |= IdentifyResponseFlags::HookProfile;
        }
        if self.armor_highlight != 0 {
            self.flags |= IdentifyResponseFlags::ArmorEnchantmentBitfield;
        }
        if self.weapon_highlight != 0 {
            self.flags |= IdentifyResponseFlags::WeaponEnchantmentBitfield;
        }
        if self.armor_levels.is_some() {
            self.flags |= IdentifyResponseFlags::ArmorLevels;
        }
    }
}

// ACE: AppraiseInfoExtensions.Write
/// Writes the AppraiseInfo to the network stream. A flagged section whose value is null throws in
/// ACE (it cannot happen: each flag is set from its value).
pub fn write(writer: &mut Vec<u8>, info: &AppraiseInfo) {
    let record = record(info);
    let strings: Vec<&str> = record
        .tables
        .strings
        .iter()
        .flat_map(|t| t.entries.iter().map(|(_, v)| v.as_str()))
        .collect();
    write_record(writer, &strings, |w| record.write(w));
}

/// The dereth-protocol record the `Write` extension above writes: each section the flags name, in
/// ACE's order, the property tables in their comparers' bucket order.
#[must_use]
pub fn record(info: &AppraiseInfo) -> dereth_protocol::types::AppraisalProfile {
    let f = info.flags;
    let has = |flag: IdentifyResponseFlags| f.contains(flag);
    // Each enchantment bitfield is two ushorts, the highlight then the colour.
    let bitfield =
        |highlight: u32, color: u32| u32::from(highlight as u16) | (u32::from(color as u16) << 16);
    dereth_protocol::types::AppraisalProfile {
        flags: f.0 as u32,
        success_flag: u32::from(info.success),
        tables: dereth_protocol::types::PropertyTables {
            ints: has(IdentifyResponseFlags::IntStatsTable)
                .then(|| int_record(&info.properties_int)),
            int64s: has(IdentifyResponseFlags::Int64StatsTable)
                .then(|| int64_record(&info.properties_int64)),
            bools: has(IdentifyResponseFlags::BoolStatsTable)
                .then(|| bool_record(&info.properties_bool)),
            floats: has(IdentifyResponseFlags::FloatStatsTable)
                .then(|| float_record(&info.properties_float)),
            strings: has(IdentifyResponseFlags::StringStatsTable)
                .then(|| string_record(&info.properties_string)),
            dids: has(IdentifyResponseFlags::DidStatsTable)
                .then(|| did_record(&info.properties_did)),
            iids: None,
            positions: None,
        },
        spell_book: has(IdentifyResponseFlags::SpellBook).then(|| info.spell_book.clone()),
        armor_profile: has(IdentifyResponseFlags::ArmorProfile).then(|| {
            armor_profile::record(info.armor_profile.as_ref().expect("ACE: null ArmorProfile"))
        }),
        creature_profile: has(IdentifyResponseFlags::CreatureProfile).then(|| {
            creature_profile::record(
                info.creature_profile
                    .as_ref()
                    .expect("ACE: null CreatureProfile"),
            )
        }),
        weapon_profile: has(IdentifyResponseFlags::WeaponProfile).then(|| {
            weapon_profile::record(
                info.weapon_profile
                    .as_ref()
                    .expect("ACE: null WeaponProfile"),
            )
        }),
        hook_profile: has(IdentifyResponseFlags::HookProfile).then(|| {
            hook_profile::record(info.hook_profile.as_ref().expect("ACE: null HookProfile"))
        }),
        armor_enchantment: has(IdentifyResponseFlags::ArmorEnchantmentBitfield)
            .then(|| bitfield(info.armor_highlight, info.armor_color)),
        weapon_enchantment: has(IdentifyResponseFlags::WeaponEnchantmentBitfield)
            .then(|| bitfield(info.weapon_highlight, info.weapon_color)),
        resist_enchantment: has(IdentifyResponseFlags::ResistEnchantmentBitfield)
            .then(|| bitfield(info.resist_highlight, info.resist_color)),
        base_armor: has(IdentifyResponseFlags::ArmorLevels).then(|| {
            armor_level::record(info.armor_levels.as_ref().expect("ACE: null ArmorLevels"))
        }),
    }
}

const PROPERTY_INT_COMPARER: PropertyIntComparer = PropertyIntComparer::new(16);
const PROPERTY_INT64_COMPARER: PropertyInt64Comparer = PropertyInt64Comparer::new(8);
const PROPERTY_BOOL_COMPARER: PropertyBoolComparer = PropertyBoolComparer::new(8);
const PROPERTY_FLOAT_COMPARER: PropertyFloatComparer = PropertyFloatComparer::new(8);
const PROPERTY_STRING_COMPARER: PropertyStringComparer = PropertyStringComparer::new(8);
const PROPERTY_DATA_ID_COMPARER: PropertyDataIdComparer = PropertyDataIdComparer::new(8);

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyInt, int>)`: 16 buckets.
pub fn write_int(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyInt, i32>) {
    write_table(writer, &[], &int_record(properties), |w, v| {
        w.i32(*v);
        Ok(())
    });
}

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyInt64, long>)`: 8 buckets.
pub fn write_int64(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyInt64, i64>) {
    write_table(writer, &[], &int64_record(properties), |w, v| {
        w.i64(*v);
        Ok(())
    });
}

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyBool, bool>)`: 8 buckets, each value as a `uint`.
pub fn write_bool(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyBool, bool>) {
    write_table(writer, &[], &bool_record(properties), |w, v| {
        w.i32(*v);
        Ok(())
    });
}

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyFloat, double>)`: 8 buckets.
pub fn write_float(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyFloat, f64>) {
    write_table(writer, &[], &float_record(properties), |w, v| {
        w.f64(*v);
        Ok(())
    });
}

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyString, string>)`: 8 buckets.
pub fn write_string(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyString, String>) {
    let table = string_record(properties);
    let strings: Vec<&str> = table.entries.iter().map(|(_, v)| v.as_str()).collect();
    write_table(writer, &strings, &table, |w, v| w.pstring(v));
}

// ACE: AppraiseInfoExtensions.Write
/// `writer.Write(Dictionary<PropertyDataId, uint>)`: 8 buckets.
pub fn write_did(writer: &mut Vec<u8>, properties: &DotNetDict<PropertyDataId, u32>) {
    write_table(writer, &[], &did_record(properties), |w, v| {
        w.u32(*v);
        Ok(())
    });
}

/// Writes one property table record: its header, then each key and value.
fn write_table<V>(
    writer: &mut Vec<u8>,
    strings: &[&str],
    table: &PackedHash<u32, V>,
    mut value: impl FnMut(&mut dereth_protocol::Writer, &V) -> Result<(), dereth_protocol::MessageError>,
) {
    write_record(writer, strings, |w| {
        w.packed_hash(table, |w, k, v| {
            w.u32(*k);
            value(w, v)
        })
    });
}

/// One table as dereth-protocol's record: the comparer's bucket count and its sorted order.
fn table<K: Copy, V: Clone>(
    entries: impl Iterator<Item = (K, V)>,
    comparer: &impl super::hash_comparer::KeyComparer<K>,
    buckets: u16,
    key: impl Fn(K) -> u32,
) -> PackedHash<u32, V> {
    PackedHash {
        table_size: u32::from(buckets),
        entries: sorted(entries, comparer)
            .into_iter()
            .map(|(k, v)| (key(k), v))
            .collect(),
    }
}

fn int_record(properties: &DotNetDict<PropertyInt, i32>) -> PackedHash<u32, i32> {
    table(
        properties.iter().map(|(k, v)| (*k, *v)),
        &PROPERTY_INT_COMPARER,
        PROPERTY_INT_COMPARER.num_buckets,
        u32::from,
    )
}

fn int64_record(properties: &DotNetDict<PropertyInt64, i64>) -> PackedHash<u32, i64> {
    table(
        properties.iter().map(|(k, v)| (*k, *v)),
        &PROPERTY_INT64_COMPARER,
        PROPERTY_INT64_COMPARER.num_buckets,
        u32::from,
    )
}

fn bool_record(properties: &DotNetDict<PropertyBool, bool>) -> PackedHash<u32, i32> {
    table(
        properties.iter().map(|(k, v)| (*k, i32::from(*v))),
        &PROPERTY_BOOL_COMPARER,
        PROPERTY_BOOL_COMPARER.num_buckets,
        u32::from,
    )
}

fn float_record(properties: &DotNetDict<PropertyFloat, f64>) -> PackedHash<u32, f64> {
    table(
        properties.iter().map(|(k, v)| (*k, *v)),
        &PROPERTY_FLOAT_COMPARER,
        PROPERTY_FLOAT_COMPARER.num_buckets,
        u32::from,
    )
}

fn string_record(properties: &DotNetDict<PropertyString, String>) -> PackedHash<u32, String> {
    table(
        properties.iter().map(|(k, v)| (*k, ace_str(v.as_str()))),
        &PROPERTY_STRING_COMPARER,
        PROPERTY_STRING_COMPARER.num_buckets,
        u32::from,
    )
}

fn did_record(properties: &DotNetDict<PropertyDataId, u32>) -> PackedHash<u32, u32> {
    table(
        properties.iter().map(|(k, v)| (*k, *v)),
        &PROPERTY_DATA_ID_COMPARER,
        PROPERTY_DATA_ID_COMPARER.num_buckets,
        u32::from,
    )
}
