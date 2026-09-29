// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Player_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Player_Properties.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    AetheriaBitfield, Channel, PropertyBool, PropertyFloat, PropertyInstanceId, PropertyInt,
    PropertyInt64, PropertyString, SquelchMask, SubscriptionStatus,
};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Player.IsAdmin
    pub fn is_admin_prop(&self) -> bool {
        self.get_property(PropertyBool::IsAdmin).unwrap_or(false)
    }

    // ACE: Player.IsAdmin
    pub fn set_is_admin_prop(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsAdmin);
        } else {
            self.set_property(PropertyBool::IsAdmin, value);
        }
    }

    // ACE: Player.IsSentinel
    pub fn is_sentinel_prop(&self) -> bool {
        self.get_property(PropertyBool::IsSentinel).unwrap_or(false)
    }

    // ACE: Player.IsSentinel
    pub fn set_is_sentinel_prop(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsSentinel);
        } else {
            self.set_property(PropertyBool::IsSentinel, value);
        }
    }

    // ACE: Player.IsEnvoy
    pub fn is_envoy(&self) -> bool {
        self.get_property(PropertyBool::IsEnvoy).unwrap_or(false)
    }

    // ACE: Player.IsEnvoy
    pub fn set_is_envoy(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsEnvoy);
        } else {
            self.set_property(PropertyBool::IsEnvoy, value);
        }
    }

    // ACE: Player.IsArch
    pub fn is_arch(&self) -> bool {
        self.get_property(PropertyBool::IsArch).unwrap_or(false)
    }

    // ACE: Player.IsArch
    pub fn set_is_arch(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsArch);
        } else {
            self.set_property(PropertyBool::IsArch, value);
        }
    }

    // ACE: Player.IsPsr
    pub fn is_psr(&self) -> bool {
        self.get_property(PropertyBool::IsPsr).unwrap_or(false)
    }

    // ACE: Player.IsPsr
    pub fn set_is_psr(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsPsr);
        } else {
            self.set_property(PropertyBool::IsPsr, value);
        }
    }

    // ACE: Player.IsAdvocate
    pub fn is_advocate(&self) -> bool {
        self.get_property(PropertyBool::IsAdvocate).unwrap_or(false)
    }

    // ACE: Player.IsAdvocate
    pub fn set_is_advocate(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsAdvocate);
        } else {
            self.set_property(PropertyBool::IsAdvocate, value);
        }
    }

    // ACE: Player.GodState
    pub fn god_state(&self) -> Option<String> {
        self.get_property(PropertyString::GodState)
    }

    // ACE: Player.GodState
    pub fn set_god_state(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::GodState),
            Some(v) => self.set_property(PropertyString::GodState, v),
        }
    }

    // ACE: Player.Account15Days
    pub fn account15_days(&self) -> bool {
        self.get_property(PropertyBool::Account15Days)
            .unwrap_or(false)
    }

    // ACE: Player.Account15Days
    pub fn set_account15_days(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Account15Days);
        } else {
            self.set_property(PropertyBool::Account15Days, value);
        }
    }

    // ACE: Player.AccountRequirements
    pub fn account_requirements_player(&self) -> SubscriptionStatus {
        SubscriptionStatus(
            self.get_property(PropertyInt::AccountRequirements)
                .unwrap_or(SubscriptionStatus::AsheronsCall_Subscription.0),
        )
    }

    // ACE: Player.AccountRequirements
    pub fn set_account_requirements_player(&mut self, value: SubscriptionStatus) {
        if value == SubscriptionStatus::AsheronsCall_Subscription {
            self.remove_property(PropertyInt::AccountRequirements);
        } else {
            self.set_property(PropertyInt::AccountRequirements, value.0);
        }
    }

    // ACE: Player.AdvocateQuest
    pub fn advocate_quest_player(&self) -> bool {
        self.get_property(PropertyBool::AdvocateQuest)
            .unwrap_or(false)
    }

    // ACE: Player.AdvocateQuest
    pub fn set_advocate_quest_player(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AdvocateQuest);
        } else {
            self.set_property(PropertyBool::AdvocateQuest, value);
        }
    }

    // ACE: Player.AdvocateState
    pub fn advocate_state(&self) -> bool {
        self.get_property(PropertyBool::AdvocateState)
            .unwrap_or(false)
    }

    // ACE: Player.AdvocateState
    pub fn set_advocate_state(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AdvocateState);
        } else {
            self.set_property(PropertyBool::AdvocateState, value);
        }
    }

    // ACE: Player.AdvocateLevel
    pub fn advocate_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::AdvocateLevel)
    }

    // ACE: Player.AdvocateLevel
    pub fn set_advocate_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AdvocateLevel),
            Some(v) => self.set_property(PropertyInt::AdvocateLevel, v),
        }
    }

    // ACE: Player.ChannelsActive
    pub fn channels_active(&self) -> Option<Channel> {
        self.get_property(PropertyInt::ChannelsActive).map(Channel)
    }

    // ACE: Player.ChannelsActive
    pub fn set_channels_active(&mut self, value: Option<Channel>) {
        match value {
            None => self.remove_property(PropertyInt::ChannelsActive),
            Some(v) => self.set_property(PropertyInt::ChannelsActive, v.0),
        }
    }

    // ACE: Player.ChannelsAllowed
    pub fn channels_allowed(&self) -> Option<Channel> {
        self.get_property(PropertyInt::ChannelsAllowed).map(Channel)
    }

    // ACE: Player.ChannelsAllowed
    pub fn set_channels_allowed(&mut self, value: Option<Channel>) {
        match value {
            None => self.remove_property(PropertyInt::ChannelsAllowed),
            Some(v) => self.set_property(PropertyInt::ChannelsAllowed, v.0),
        }
    }

    // ACE: Player.Age
    pub fn age(&self) -> Option<i32> {
        self.get_property(PropertyInt::Age)
    }

    // ACE: Player.Age
    pub fn set_age(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Age),
            Some(v) => self.set_property(PropertyInt::Age, v),
        }
    }

    // ACE: Player.AvailableExperience
    pub fn available_experience(&self) -> Option<i64> {
        self.get_property(PropertyInt64::AvailableExperience)
    }

    // ACE: Player.AvailableExperience
    pub fn set_available_experience(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::AvailableExperience),
            Some(v) => self.set_property(PropertyInt64::AvailableExperience, v),
        }
    }

    // ACE: Player.TotalExperience
    pub fn total_experience(&self) -> Option<i64> {
        self.get_property(PropertyInt64::TotalExperience)
    }

    // ACE: Player.TotalExperience
    pub fn set_total_experience(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::TotalExperience),
            Some(v) => self.set_property(PropertyInt64::TotalExperience, v),
        }
    }

    // ACE: Player.AvailableLuminance
    pub fn available_luminance(&self) -> Option<i64> {
        self.get_property(PropertyInt64::AvailableLuminance)
    }

    // ACE: Player.AvailableLuminance
    pub fn set_available_luminance(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::AvailableLuminance),
            Some(v) => self.set_property(PropertyInt64::AvailableLuminance, v),
        }
    }

    // ACE: Player.MaximumLuminance
    pub fn maximum_luminance(&self) -> Option<i64> {
        self.get_property(PropertyInt64::MaximumLuminance)
    }

    // ACE: Player.MaximumLuminance
    pub fn set_maximum_luminance(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::MaximumLuminance),
            Some(v) => self.set_property(PropertyInt64::MaximumLuminance, v),
        }
    }

    // ACE: Player.AvailableSkillCredits
    pub fn available_skill_credits(&self) -> Option<i32> {
        self.get_property(PropertyInt::AvailableSkillCredits)
    }

    // ACE: Player.AvailableSkillCredits
    pub fn set_available_skill_credits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AvailableSkillCredits),
            Some(v) => self.set_property(PropertyInt::AvailableSkillCredits, v),
        }
    }

    // ACE: Player.TotalSkillCredits
    pub fn total_skill_credits(&self) -> Option<i32> {
        self.get_property(PropertyInt::TotalSkillCredits)
    }

    // ACE: Player.TotalSkillCredits
    pub fn set_total_skill_credits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::TotalSkillCredits),
            Some(v) => self.set_property(PropertyInt::TotalSkillCredits, v),
        }
    }

    // ACE: Player.NumDeaths
    pub fn num_deaths(&self) -> i32 {
        self.get_property(PropertyInt::NumDeaths).unwrap_or(0)
    }

    // ACE: Player.NumDeaths
    pub fn set_num_deaths(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::NumDeaths);
        } else {
            self.set_property(PropertyInt::NumDeaths, value);
        }
    }

    // ACE: Player.DeathLevel
    pub fn death_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::DeathLevel)
    }

    // ACE: Player.DeathLevel
    pub fn set_death_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::DeathLevel),
            Some(v) => self.set_property(PropertyInt::DeathLevel, v),
        }
    }

    // ACE: Player.VitaeCpPool
    pub fn vitae_cp_pool(&self) -> Option<i32> {
        self.get_property(PropertyInt::VitaeCpPool)
    }

    // ACE: Player.VitaeCpPool
    pub fn set_vitae_cp_pool(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::VitaeCpPool),
            Some(v) => self.set_property(PropertyInt::VitaeCpPool, v),
        }
    }

    // ACE: Player.HousePurchaseTimestamp
    pub fn house_purchase_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::HousePurchaseTimestamp)
    }

    // ACE: Player.HousePurchaseTimestamp
    pub fn set_house_purchase_timestamp(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HousePurchaseTimestamp),
            Some(v) => self.set_property(PropertyInt::HousePurchaseTimestamp, v),
        }
    }

    // ACE: Player.HouseRentTimestamp
    pub fn house_rent_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::HouseRentTimestamp)
    }

    // ACE: Player.HouseRentTimestamp
    pub fn set_house_rent_timestamp(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::HouseRentTimestamp),
            Some(v) => self.set_property(PropertyInt::HouseRentTimestamp, v),
        }
    }

    // ACE: Player.LoginTimestamp
    pub fn login_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::LoginTimestamp)
    }

    // ACE: Player.LoginTimestamp
    pub fn set_login_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::LoginTimestamp),
            Some(v) => self.set_property(PropertyFloat::LoginTimestamp, v),
        }
    }

    // ACE: Player.LogoffTimestamp
    pub fn logoff_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::LogoffTimestamp)
    }

    // ACE: Player.LogoffTimestamp
    pub fn set_logoff_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::LogoffTimestamp),
            Some(v) => self.set_property(PropertyFloat::LogoffTimestamp, v),
        }
    }

    // ACE: Player.LastTeleportStartTimestamp
    pub fn last_teleport_start_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::LastTeleportStartTimestamp)
    }

    // ACE: Player.LastTeleportStartTimestamp
    pub fn set_last_teleport_start_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::LastTeleportStartTimestamp),
            Some(v) => self.set_property(PropertyFloat::LastTeleportStartTimestamp, v),
        }
    }

    // ACE: Player.SpellComponentsRequired
    pub fn spell_components_required(&self) -> bool {
        self.get_property(PropertyBool::SpellComponentsRequired)
            .unwrap_or(true)
    }

    // ACE: Player.SpellComponentsRequired
    pub fn set_spell_components_required(&mut self, value: bool) {
        if value {
            self.remove_property(PropertyBool::SpellComponentsRequired);
        } else {
            self.set_property(PropertyBool::SpellComponentsRequired, value);
        }
    }

    // ACE: Player.SafeSpellComponents
    pub fn safe_spell_components(&self) -> bool {
        self.get_property(PropertyBool::SafeSpellComponents)
            .unwrap_or(false)
    }

    // ACE: Player.SafeSpellComponents
    pub fn set_safe_spell_components(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::SafeSpellComponents);
        } else {
            self.set_property(PropertyBool::SafeSpellComponents, value);
        }
    }

    // ACE: Player.LoginAtLifestone
    pub fn login_at_lifestone(&self) -> bool {
        self.get_property(PropertyBool::LoginAtLifestone)
            .unwrap_or(false)
    }

    // ACE: Player.LoginAtLifestone
    pub fn set_login_at_lifestone(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::LoginAtLifestone);
        } else {
            self.set_property(PropertyBool::LoginAtLifestone, value);
        }
    }

    // ACE: Player.RaresLoginTimestamp
    pub fn rares_login_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresLoginTimestamp)
    }

    // ACE: Player.RaresLoginTimestamp
    pub fn set_rares_login_timestamp(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresLoginTimestamp),
            Some(v) => self.set_property(PropertyInt::RaresLoginTimestamp, v),
        }
    }

    // ACE: Player.RaresTierOneLogin
    pub fn rares_tier_one_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierOneLogin)
    }

    // ACE: Player.RaresTierOneLogin
    pub fn set_rares_tier_one_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierOneLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierOneLogin, v),
        }
    }

    // ACE: Player.RaresTierTwoLogin
    pub fn rares_tier_two_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierTwoLogin)
    }

    // ACE: Player.RaresTierTwoLogin
    pub fn set_rares_tier_two_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierTwoLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierTwoLogin, v),
        }
    }

    // ACE: Player.RaresTierThreeLogin
    pub fn rares_tier_three_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierThreeLogin)
    }

    // ACE: Player.RaresTierThreeLogin
    pub fn set_rares_tier_three_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierThreeLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierThreeLogin, v),
        }
    }

    // ACE: Player.RaresTierFourLogin
    pub fn rares_tier_four_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierFourLogin)
    }

    // ACE: Player.RaresTierFourLogin
    pub fn set_rares_tier_four_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierFourLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierFourLogin, v),
        }
    }

    // ACE: Player.RaresTierFiveLogin
    pub fn rares_tier_five_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierFiveLogin)
    }

    // ACE: Player.RaresTierFiveLogin
    pub fn set_rares_tier_five_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierFiveLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierFiveLogin, v),
        }
    }

    // ACE: Player.RaresTierSixLogin
    pub fn rares_tier_six_login(&self) -> Option<i32> {
        self.get_property(PropertyInt::RaresTierSixLogin)
    }

    // ACE: Player.RaresTierSixLogin
    pub fn set_rares_tier_six_login(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RaresTierSixLogin),
            Some(v) => self.set_property(PropertyInt::RaresTierSixLogin, v),
        }
    }

    // ACE: Player.RaresTierOne
    pub fn rares_tier_one(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierOne).unwrap_or(0)
    }

    // ACE: Player.RaresTierOne
    pub fn set_rares_tier_one(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierOne);
        } else {
            self.set_property(PropertyInt::RaresTierOne, value);
        }
    }

    // ACE: Player.RaresTierTwo
    pub fn rares_tier_two(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierTwo).unwrap_or(0)
    }

    // ACE: Player.RaresTierTwo
    pub fn set_rares_tier_two(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierTwo);
        } else {
            self.set_property(PropertyInt::RaresTierTwo, value);
        }
    }

    // ACE: Player.RaresTierThree
    pub fn rares_tier_three(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierThree).unwrap_or(0)
    }

    // ACE: Player.RaresTierThree
    pub fn set_rares_tier_three(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierThree);
        } else {
            self.set_property(PropertyInt::RaresTierThree, value);
        }
    }

    // ACE: Player.RaresTierFour
    pub fn rares_tier_four(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierFour).unwrap_or(0)
    }

    // ACE: Player.RaresTierFour
    pub fn set_rares_tier_four(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierFour);
        } else {
            self.set_property(PropertyInt::RaresTierFour, value);
        }
    }

    // ACE: Player.RaresTierFive
    pub fn rares_tier_five(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierFive).unwrap_or(0)
    }

    // ACE: Player.RaresTierFive
    pub fn set_rares_tier_five(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierFive);
        } else {
            self.set_property(PropertyInt::RaresTierFive, value);
        }
    }

    // ACE: Player.RaresTierSix
    pub fn rares_tier_six(&self) -> i32 {
        self.get_property(PropertyInt::RaresTierSix).unwrap_or(0)
    }

    // ACE: Player.RaresTierSix
    pub fn set_rares_tier_six(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RaresTierSix);
        } else {
            self.set_property(PropertyInt::RaresTierSix, value);
        }
    }

    // ACE: Player.IsAfk
    pub fn is_afk(&self) -> bool {
        self.get_property(PropertyBool::Afk).unwrap_or(false)
    }

    // ACE: Player.IsAfk
    pub fn set_is_afk(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::Afk);
        } else {
            self.set_property(PropertyBool::Afk, value);
        }
    }

    // ACE: Player.AfkMessage
    pub fn afk_message(&self) -> Option<String> {
        self.get_property(PropertyString::Afk)
    }

    // ACE: Player.AfkMessage
    pub fn set_afk_message(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::Afk),
            Some(v) => self.set_property(PropertyString::Afk, v),
        }
    }

    // ACE: Player.CharacterTitleId
    pub fn character_title_id(&self) -> Option<i32> {
        self.get_property(PropertyInt::CharacterTitleId)
    }

    // ACE: Player.CharacterTitleId
    pub fn set_character_title_id(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::CharacterTitleId),
            Some(v) => self.set_property(PropertyInt::CharacterTitleId, v),
        }
    }

    // ACE: Player.NumCharacterTitles
    pub fn num_character_titles(&self) -> Option<i32> {
        self.get_property(PropertyInt::NumCharacterTitles)
    }

    // ACE: Player.NumCharacterTitles
    pub fn set_num_character_titles(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::NumCharacterTitles),
            Some(v) => self.set_property(PropertyInt::NumCharacterTitles, v),
        }
    }

    // ACE: Player.AugmentationJackOfAllTrades
    pub fn augmentation_jack_of_all_trades(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationJackOfAllTrades)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationJackOfAllTrades
    pub fn set_augmentation_jack_of_all_trades(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationJackOfAllTrades);
        } else {
            self.set_property(PropertyInt::AugmentationJackOfAllTrades, value);
        }
    }

    // ACE: Player.AugmentationCriticalExpertise
    pub fn augmentation_critical_expertise(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationCriticalExpertise)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationCriticalExpertise
    pub fn set_augmentation_critical_expertise(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationCriticalExpertise);
        } else {
            self.set_property(PropertyInt::AugmentationCriticalExpertise, value);
        }
    }

    // ACE: Player.AugmentationDamageReduction
    pub fn augmentation_damage_reduction(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationDamageReduction)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationDamageReduction
    pub fn set_augmentation_damage_reduction(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationDamageReduction);
        } else {
            self.set_property(PropertyInt::AugmentationDamageReduction, value);
        }
    }

    // ACE: Player.AugmentationCriticalDefense
    pub fn augmentation_critical_defense(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationCriticalDefense)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationCriticalDefense
    pub fn set_augmentation_critical_defense(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationCriticalDefense);
        } else {
            self.set_property(PropertyInt::AugmentationCriticalDefense, value);
        }
    }

    // ACE: Player.AugmentationInfusedLifeMagic
    pub fn augmentation_infused_life_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInfusedLifeMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInfusedLifeMagic
    pub fn set_augmentation_infused_life_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInfusedLifeMagic);
        } else {
            self.set_property(PropertyInt::AugmentationInfusedLifeMagic, value);
        }
    }

    // ACE: Player.AugmentationCriticalPower
    pub fn augmentation_critical_power(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationCriticalPower)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationCriticalPower
    pub fn set_augmentation_critical_power(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationCriticalPower);
        } else {
            self.set_property(PropertyInt::AugmentationCriticalPower, value);
        }
    }

    // ACE: Player.AugmentationIncreasedCarryingCapacity
    pub fn augmentation_increased_carrying_capacity(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationIncreasedCarryingCapacity)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationIncreasedCarryingCapacity
    pub fn set_augmentation_increased_carrying_capacity(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationIncreasedCarryingCapacity);
        } else {
            self.set_property(PropertyInt::AugmentationIncreasedCarryingCapacity, value);
        }
    }

    // ACE: Player.AugmentationInfusedCreatureMagic
    pub fn augmentation_infused_creature_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInfusedCreatureMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInfusedCreatureMagic
    pub fn set_augmentation_infused_creature_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInfusedCreatureMagic);
        } else {
            self.set_property(PropertyInt::AugmentationInfusedCreatureMagic, value);
        }
    }

    // ACE: Player.AugmentationInfusedItemMagic
    pub fn augmentation_infused_item_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInfusedItemMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInfusedItemMagic
    pub fn set_augmentation_infused_item_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInfusedItemMagic);
        } else {
            self.set_property(PropertyInt::AugmentationInfusedItemMagic, value);
        }
    }

    // ACE: Player.AugmentationInfusedVoidMagic
    pub fn augmentation_infused_void_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInfusedVoidMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInfusedVoidMagic
    pub fn set_augmentation_infused_void_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInfusedVoidMagic);
        } else {
            self.set_property(PropertyInt::AugmentationInfusedVoidMagic, value);
        }
    }

    // ACE: Player.AugmentationInfusedWarMagic
    pub fn augmentation_infused_war_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInfusedWarMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInfusedWarMagic
    pub fn set_augmentation_infused_war_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInfusedWarMagic);
        } else {
            self.set_property(PropertyInt::AugmentationInfusedWarMagic, value);
        }
    }

    // ACE: Player.AugmentationLessDeathItemLoss
    pub fn augmentation_less_death_item_loss(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationLessDeathItemLoss)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationLessDeathItemLoss
    pub fn set_augmentation_less_death_item_loss(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationLessDeathItemLoss);
        } else {
            self.set_property(PropertyInt::AugmentationLessDeathItemLoss, value);
        }
    }

    // ACE: Player.AugmentationSpellsRemainPastDeath
    pub fn augmentation_spells_remain_past_death(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpellsRemainPastDeath)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpellsRemainPastDeath
    pub fn set_augmentation_spells_remain_past_death(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpellsRemainPastDeath);
        } else {
            self.set_property(PropertyInt::AugmentationSpellsRemainPastDeath, value);
        }
    }

    // ACE: Player.AugmentationBonusXp
    pub fn augmentation_bonus_xp(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationBonusXp)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationBonusXp
    pub fn set_augmentation_bonus_xp(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationBonusXp);
        } else {
            self.set_property(PropertyInt::AugmentationBonusXp, value);
        }
    }

    // ACE: Player.AugmentationFasterRegen
    pub fn augmentation_faster_regen(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationFasterRegen)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationFasterRegen
    pub fn set_augmentation_faster_regen(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationFasterRegen);
        } else {
            self.set_property(PropertyInt::AugmentationFasterRegen, value);
        }
    }

    // ACE: Player.AugmentationExtraPackSlot
    pub fn augmentation_extra_pack_slot(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationExtraPackSlot)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationExtraPackSlot
    pub fn set_augmentation_extra_pack_slot(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationExtraPackSlot);
        } else {
            self.set_property(PropertyInt::AugmentationExtraPackSlot, value);
        }
    }

    // ACE: Player.AugmentationInnateStrength
    pub fn augmentation_innate_strength(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateStrength)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateStrength
    pub fn set_augmentation_innate_strength(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateStrength);
        } else {
            self.set_property(PropertyInt::AugmentationInnateStrength, value);
        }
    }

    // ACE: Player.AugmentationInnateEndurance
    pub fn augmentation_innate_endurance(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateEndurance)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateEndurance
    pub fn set_augmentation_innate_endurance(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateEndurance);
        } else {
            self.set_property(PropertyInt::AugmentationInnateEndurance, value);
        }
    }

    // ACE: Player.AugmentationInnateCoordination
    pub fn augmentation_innate_coordination(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateCoordination)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateCoordination
    pub fn set_augmentation_innate_coordination(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateCoordination);
        } else {
            self.set_property(PropertyInt::AugmentationInnateCoordination, value);
        }
    }

    // ACE: Player.AugmentationInnateQuickness
    pub fn augmentation_innate_quickness(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateQuickness)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateQuickness
    pub fn set_augmentation_innate_quickness(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateQuickness);
        } else {
            self.set_property(PropertyInt::AugmentationInnateQuickness, value);
        }
    }

    // ACE: Player.AugmentationInnateFocus
    pub fn augmentation_innate_focus(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateFocus)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateFocus
    pub fn set_augmentation_innate_focus(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateFocus);
        } else {
            self.set_property(PropertyInt::AugmentationInnateFocus, value);
        }
    }

    // ACE: Player.AugmentationInnateSelf
    pub fn augmentation_innate_self(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateSelf)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateSelf
    pub fn set_augmentation_innate_self(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateSelf);
        } else {
            self.set_property(PropertyInt::AugmentationInnateSelf, value);
        }
    }

    // ACE: Player.AugmentationInnateFamily
    pub fn augmentation_innate_family(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationInnateFamily)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationInnateFamily
    pub fn set_augmentation_innate_family(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationInnateFamily);
        } else {
            self.set_property(PropertyInt::AugmentationInnateFamily, value);
        }
    }

    // ACE: Player.AugmentationResistanceSlash
    pub fn augmentation_resistance_slash(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceSlash)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceSlash
    pub fn set_augmentation_resistance_slash(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceSlash);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceSlash, value);
        }
    }

    // ACE: Player.AugmentationResistancePierce
    pub fn augmentation_resistance_pierce(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistancePierce)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistancePierce
    pub fn set_augmentation_resistance_pierce(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistancePierce);
        } else {
            self.set_property(PropertyInt::AugmentationResistancePierce, value);
        }
    }

    // ACE: Player.AugmentationResistanceBlunt
    pub fn augmentation_resistance_blunt(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceBlunt)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceBlunt
    pub fn set_augmentation_resistance_blunt(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceBlunt);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceBlunt, value);
        }
    }

    // ACE: Player.AugmentationResistanceFire
    pub fn augmentation_resistance_fire(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceFire)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceFire
    pub fn set_augmentation_resistance_fire(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceFire);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceFire, value);
        }
    }

    // ACE: Player.AugmentationResistanceFrost
    pub fn augmentation_resistance_frost(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceFrost)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceFrost
    pub fn set_augmentation_resistance_frost(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceFrost);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceFrost, value);
        }
    }

    // ACE: Player.AugmentationResistanceAcid
    pub fn augmentation_resistance_acid(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceAcid)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceAcid
    pub fn set_augmentation_resistance_acid(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceAcid);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceAcid, value);
        }
    }

    // ACE: Player.AugmentationResistanceLightning
    pub fn augmentation_resistance_lightning(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceLightning)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceLightning
    pub fn set_augmentation_resistance_lightning(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceLightning);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceLightning, value);
        }
    }

    // ACE: Player.AugmentationResistanceFamily
    pub fn augmentation_resistance_family(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationResistanceFamily)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationResistanceFamily
    pub fn set_augmentation_resistance_family(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationResistanceFamily);
        } else {
            self.set_property(PropertyInt::AugmentationResistanceFamily, value);
        }
    }

    // ACE: Player.AugmentationDamageBonus
    pub fn augmentation_damage_bonus(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationDamageBonus)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationDamageBonus
    pub fn set_augmentation_damage_bonus(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationDamageBonus);
        } else {
            self.set_property(PropertyInt::AugmentationDamageBonus, value);
        }
    }

    // ACE: Player.AugmentationBonusSalvage
    pub fn augmentation_bonus_salvage(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationBonusSalvage)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationBonusSalvage
    pub fn set_augmentation_bonus_salvage(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationBonusSalvage);
        } else {
            self.set_property(PropertyInt::AugmentationBonusSalvage, value);
        }
    }

    // ACE: Player.AugmentationBonusImbueChance
    pub fn augmentation_bonus_imbue_chance(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationBonusImbueChance)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationBonusImbueChance
    pub fn set_augmentation_bonus_imbue_chance(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationBonusImbueChance);
        } else {
            self.set_property(PropertyInt::AugmentationBonusImbueChance, value);
        }
    }

    // ACE: Player.AugmentationSpecializeArmorTinkering
    pub fn augmentation_specialize_armor_tinkering(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpecializeArmorTinkering)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpecializeArmorTinkering
    pub fn set_augmentation_specialize_armor_tinkering(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpecializeArmorTinkering);
        } else {
            self.set_property(PropertyInt::AugmentationSpecializeArmorTinkering, value);
        }
    }

    // ACE: Player.AugmentationSpecializeItemTinkering
    pub fn augmentation_specialize_item_tinkering(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpecializeItemTinkering)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpecializeItemTinkering
    pub fn set_augmentation_specialize_item_tinkering(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpecializeItemTinkering);
        } else {
            self.set_property(PropertyInt::AugmentationSpecializeItemTinkering, value);
        }
    }

    // ACE: Player.AugmentationSpecializeMagicItemTinkering
    pub fn augmentation_specialize_magic_item_tinkering(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpecializeMagicItemTinkering)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpecializeMagicItemTinkering
    pub fn set_augmentation_specialize_magic_item_tinkering(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpecializeMagicItemTinkering);
        } else {
            self.set_property(PropertyInt::AugmentationSpecializeMagicItemTinkering, value);
        }
    }

    // ACE: Player.AugmentationSpecializeWeaponTinkering
    pub fn augmentation_specialize_weapon_tinkering(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpecializeWeaponTinkering)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpecializeWeaponTinkering
    pub fn set_augmentation_specialize_weapon_tinkering(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpecializeWeaponTinkering);
        } else {
            self.set_property(PropertyInt::AugmentationSpecializeWeaponTinkering, value);
        }
    }

    // ACE: Player.AugmentationSpecializeSalvaging
    pub fn augmentation_specialize_salvaging(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSpecializeSalvaging)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSpecializeSalvaging
    pub fn set_augmentation_specialize_salvaging(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSpecializeSalvaging);
        } else {
            self.set_property(PropertyInt::AugmentationSpecializeSalvaging, value);
        }
    }

    // ACE: Player.AugmentationSkilledMelee
    pub fn augmentation_skilled_melee(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSkilledMelee)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSkilledMelee
    pub fn set_augmentation_skilled_melee(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSkilledMelee);
        } else {
            self.set_property(PropertyInt::AugmentationSkilledMelee, value);
        }
    }

    // ACE: Player.AugmentationSkilledMagic
    pub fn augmentation_skilled_magic(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSkilledMagic)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSkilledMagic
    pub fn set_augmentation_skilled_magic(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSkilledMagic);
        } else {
            self.set_property(PropertyInt::AugmentationSkilledMagic, value);
        }
    }

    // ACE: Player.AugmentationSkilledMissile
    pub fn augmentation_skilled_missile(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationSkilledMissile)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationSkilledMissile
    pub fn set_augmentation_skilled_missile(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationSkilledMissile);
        } else {
            self.set_property(PropertyInt::AugmentationSkilledMissile, value);
        }
    }

    // ACE: Player.AugmentationIncreasedSpellDuration
    pub fn augmentation_increased_spell_duration(&self) -> i32 {
        self.get_property(PropertyInt::AugmentationIncreasedSpellDuration)
            .unwrap_or(0)
    }

    // ACE: Player.AugmentationIncreasedSpellDuration
    pub fn set_augmentation_increased_spell_duration(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AugmentationIncreasedSpellDuration);
        } else {
            self.set_property(PropertyInt::AugmentationIncreasedSpellDuration, value);
        }
    }

    // ACE: Player.LumAugSurgeChanceRating
    pub fn lum_aug_surge_chance_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugSurgeChanceRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugSurgeChanceRating
    pub fn set_lum_aug_surge_chance_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugSurgeChanceRating);
        } else {
            self.set_property(PropertyInt::LumAugSurgeChanceRating, value);
        }
    }

    // ACE: Player.LumAugSkilledCraft
    pub fn lum_aug_skilled_craft(&self) -> i32 {
        self.get_property(PropertyInt::LumAugSkilledCraft)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugSkilledCraft
    pub fn set_lum_aug_skilled_craft(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugSkilledCraft);
        } else {
            self.set_property(PropertyInt::LumAugSkilledCraft, value);
        }
    }

    // ACE: Player.LumAugCritDamageRating
    pub fn lum_aug_crit_damage_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugCritDamageRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugCritDamageRating
    pub fn set_lum_aug_crit_damage_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugCritDamageRating);
        } else {
            self.set_property(PropertyInt::LumAugCritDamageRating, value);
        }
    }

    // ACE: Player.LumAugItemManaUsage
    pub fn lum_aug_item_mana_usage(&self) -> i32 {
        self.get_property(PropertyInt::LumAugItemManaUsage)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugItemManaUsage
    pub fn set_lum_aug_item_mana_usage(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugItemManaUsage);
        } else {
            self.set_property(PropertyInt::LumAugItemManaUsage, value);
        }
    }

    // ACE: Player.LumAugItemManaGain
    pub fn lum_aug_item_mana_gain(&self) -> i32 {
        self.get_property(PropertyInt::LumAugItemManaGain)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugItemManaGain
    pub fn set_lum_aug_item_mana_gain(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugItemManaGain);
        } else {
            self.set_property(PropertyInt::LumAugItemManaGain, value);
        }
    }

    // ACE: Player.LumAugDamageReductionRating
    pub fn lum_aug_damage_reduction_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugDamageReductionRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugDamageReductionRating
    pub fn set_lum_aug_damage_reduction_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugDamageReductionRating);
        } else {
            self.set_property(PropertyInt::LumAugDamageReductionRating, value);
        }
    }

    // ACE: Player.LumAugHealingRating
    pub fn lum_aug_healing_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugHealingRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugHealingRating
    pub fn set_lum_aug_healing_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugHealingRating);
        } else {
            self.set_property(PropertyInt::LumAugHealingRating, value);
        }
    }

    // ACE: Player.LumAugCritReductionRating
    pub fn lum_aug_crit_reduction_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugCritReductionRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugCritReductionRating
    pub fn set_lum_aug_crit_reduction_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugCritReductionRating);
        } else {
            self.set_property(PropertyInt::LumAugCritReductionRating, value);
        }
    }

    // ACE: Player.LumAugDamageRating
    pub fn lum_aug_damage_rating(&self) -> i32 {
        self.get_property(PropertyInt::LumAugDamageRating)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugDamageRating
    pub fn set_lum_aug_damage_rating(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugDamageRating);
        } else {
            self.set_property(PropertyInt::LumAugDamageRating, value);
        }
    }

    // ACE: Player.LumAugAllSkills
    pub fn lum_aug_all_skills(&self) -> i32 {
        self.get_property(PropertyInt::LumAugAllSkills).unwrap_or(0)
    }

    // ACE: Player.LumAugAllSkills
    pub fn set_lum_aug_all_skills(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugAllSkills);
        } else {
            self.set_property(PropertyInt::LumAugAllSkills, value);
        }
    }

    // ACE: Player.LumAugSkilledSpec
    pub fn lum_aug_skilled_spec(&self) -> i32 {
        self.get_property(PropertyInt::LumAugSkilledSpec)
            .unwrap_or(0)
    }

    // ACE: Player.LumAugSkilledSpec
    pub fn set_lum_aug_skilled_spec(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugSkilledSpec);
        } else {
            self.set_property(PropertyInt::LumAugSkilledSpec, value);
        }
    }

    // ACE: Player.MeleeMastery
    pub fn melee_mastery(&self) -> i32 {
        self.get_property(PropertyInt::MeleeMastery).unwrap_or(0)
    }

    // ACE: Player.MeleeMastery
    pub fn set_melee_mastery(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::MeleeMastery);
        } else {
            self.set_property(PropertyInt::MeleeMastery, value);
        }
    }

    // ACE: Player.RangedMastery
    pub fn ranged_mastery(&self) -> i32 {
        self.get_property(PropertyInt::RangedMastery).unwrap_or(0)
    }

    // ACE: Player.RangedMastery
    pub fn set_ranged_mastery(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::RangedMastery);
        } else {
            self.set_property(PropertyInt::RangedMastery, value);
        }
    }

    // ACE: Player.Enlightenment
    pub fn enlightenment(&self) -> i32 {
        self.get_property(PropertyInt::Enlightenment).unwrap_or(0)
    }

    // ACE: Player.Enlightenment
    pub fn set_enlightenment(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::Enlightenment);
        } else {
            self.set_property(PropertyInt::Enlightenment, value);
        }
    }

    // ACE: Player.LumAugVitality
    pub fn lum_aug_vitality(&self) -> i32 {
        self.get_property(PropertyInt::LumAugVitality).unwrap_or(0)
    }

    // ACE: Player.LumAugVitality
    pub fn set_lum_aug_vitality(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::LumAugVitality);
        } else {
            self.set_property(PropertyInt::LumAugVitality, value);
        }
    }

    // ACE: Player.LastRareUsedTimestamp
    pub fn last_rare_used_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::LastRareUsedTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: Player.LastRareUsedTimestamp
    pub fn set_last_rare_used_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::LastRareUsedTimestamp);
        } else {
            self.set_property(PropertyFloat::LastRareUsedTimestamp, value);
        }
    }

    // ACE: Player.BarberActive
    pub fn barber_active(&self) -> bool {
        self.get_property(PropertyBool::BarberActive)
            .unwrap_or(false)
    }

    // ACE: Player.BarberActive
    pub fn set_barber_active(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::BarberActive);
        } else {
            self.set_property(PropertyBool::BarberActive, value);
        }
    }

    // ACE: Player.AllegianceGagDuration
    pub fn allegiance_gag_duration(&self) -> f64 {
        self.get_property(PropertyFloat::AllegianceGagDuration)
            .unwrap_or(0.0)
    }

    // ACE: Player.AllegianceGagDuration
    pub fn set_allegiance_gag_duration(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::AllegianceGagDuration);
        } else {
            self.set_property(PropertyFloat::AllegianceGagDuration, value);
        }
    }

    // ACE: Player.AllegianceGagTimestamp
    pub fn allegiance_gag_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::AllegianceGagTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: Player.AllegianceGagTimestamp
    pub fn set_allegiance_gag_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::AllegianceGagTimestamp);
        } else {
            self.set_property(PropertyFloat::AllegianceGagTimestamp, value);
        }
    }

    // ACE: Player.GagDuration
    pub fn gag_duration(&self) -> f64 {
        self.get_property(PropertyFloat::GagDuration).unwrap_or(0.0)
    }

    // ACE: Player.GagDuration
    pub fn set_gag_duration(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::GagDuration);
        } else {
            self.set_property(PropertyFloat::GagDuration, value);
        }
    }

    // ACE: Player.GagTimestamp
    pub fn gag_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::GagTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: Player.GagTimestamp
    pub fn set_gag_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::GagTimestamp);
        } else {
            self.set_property(PropertyFloat::GagTimestamp, value);
        }
    }

    // ACE: Player.IsAllegianceGagged
    pub fn is_allegiance_gagged(&self) -> bool {
        self.get_property(PropertyBool::IsAllegianceGagged)
            .unwrap_or(false)
    }

    // ACE: Player.IsAllegianceGagged
    pub fn set_is_allegiance_gagged(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsAllegianceGagged);
        } else {
            self.set_property(PropertyBool::IsAllegianceGagged, value);
        }
    }

    // ACE: Player.IsGagged
    pub fn is_gagged(&self) -> bool {
        self.get_property(PropertyBool::IsGagged).unwrap_or(false)
    }

    // ACE: Player.IsGagged
    pub fn set_is_gagged(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsGagged);
        } else {
            self.set_property(PropertyBool::IsGagged, value);
        }
    }

    // ACE: Player.RecallsDisabled
    pub fn recalls_disabled(&self) -> bool {
        self.get_property(PropertyBool::RecallsDisabled)
            .unwrap_or(false)
    }

    // ACE: Player.RecallsDisabled
    pub fn set_recalls_disabled(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::RecallsDisabled);
        } else {
            self.set_property(PropertyBool::RecallsDisabled, value);
        }
    }

    // ACE: Player.AetheriaFlags
    pub fn aetheria_flags(&self) -> AetheriaBitfield {
        AetheriaBitfield(
            self.get_property(PropertyInt::AetheriaBitfield)
                .unwrap_or(0),
        )
    }

    // ACE: Player.AetheriaFlags
    pub fn set_aetheria_flags(&mut self, value: AetheriaBitfield) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::AetheriaBitfield);
        } else {
            self.set_property(PropertyInt::AetheriaBitfield, value.0);
        }
    }

    // ACE: Player.SquelchGlobal
    pub fn squelch_global(&self) -> SquelchMask {
        SquelchMask(
            self.get_property(PropertyInt::SquelchGlobal)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: Player.SquelchGlobal
    pub fn set_squelch_global(&mut self, value: SquelchMask) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::SquelchGlobal);
        } else {
            self.set_property(PropertyInt::SquelchGlobal, value.0.cs_cast());
        }
    }

    // ACE: Player.RequestedAppraisalTarget
    pub fn requested_appraisal_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::RequestedAppraisalTarget)
    }

    // ACE: Player.RequestedAppraisalTarget
    pub fn set_requested_appraisal_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::RequestedAppraisalTarget),
            Some(v) => self.set_property(PropertyInstanceId::RequestedAppraisalTarget, v),
        }
    }

    // ACE: Player.AppraisalRequestedTimestamp
    pub fn appraisal_requested_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::AppraisalRequestedTimestamp)
    }

    // ACE: Player.AppraisalRequestedTimestamp
    pub fn set_appraisal_requested_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::AppraisalRequestedTimestamp),
            Some(v) => self.set_property(PropertyFloat::AppraisalRequestedTimestamp, v),
        }
    }

    // ACE: Player.CurrentAppraisalTarget
    pub fn current_appraisal_target(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::CurrentAppraisalTarget)
    }

    // ACE: Player.CurrentAppraisalTarget
    pub fn set_current_appraisal_target(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::CurrentAppraisalTarget),
            Some(v) => self.set_property(PropertyInstanceId::CurrentAppraisalTarget, v),
        }
    }

    // ACE: Player.LastPortalTeleportTimestamp
    pub fn last_portal_teleport_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::LastPortalTeleportTimestamp)
    }

    // ACE: Player.LastPortalTeleportTimestamp
    pub fn set_last_portal_teleport_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::LastPortalTeleportTimestamp),
            Some(v) => self.set_property(PropertyFloat::LastPortalTeleportTimestamp, v),
        }
    }

    // ACE: Player.OlthoiPk
    pub fn olthoi_pk(&self) -> bool {
        self.get_property(PropertyBool::OlthoiPk).unwrap_or(false)
    }

    // ACE: Player.OlthoiPk
    pub fn set_olthoi_pk(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::OlthoiPk);
        } else {
            self.set_property(PropertyBool::OlthoiPk, value);
        }
    }

    // ACE: Player.NoOlthoiTalk
    pub fn no_olthoi_talk(&self) -> bool {
        self.get_property(PropertyBool::NoOlthoiTalk)
            .unwrap_or(false)
    }

    // ACE: Player.NoOlthoiTalk
    pub fn set_no_olthoi_talk(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::NoOlthoiTalk);
        } else {
            self.set_property(PropertyBool::NoOlthoiTalk, value);
        }
    }

    // ACE: Player.OlthoiLootTimestamp
    pub fn olthoi_loot_timestamp(&self) -> Option<i32> {
        self.get_property(PropertyInt::OlthoiLootTimestamp)
    }

    // ACE: Player.OlthoiLootTimestamp
    pub fn set_olthoi_loot_timestamp(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::OlthoiLootTimestamp),
            Some(v) => self.set_property(PropertyInt::OlthoiLootTimestamp, v),
        }
    }

    // ACE: Player.OlthoiLootStep
    pub fn olthoi_loot_step(&self) -> Option<i32> {
        self.get_property(PropertyInt::OlthoiLootStep)
    }

    // ACE: Player.OlthoiLootStep
    pub fn set_olthoi_loot_step(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::OlthoiLootStep),
            Some(v) => self.set_property(PropertyInt::OlthoiLootStep, v),
        }
    }

    // ACE: Player.ImbueAttempts
    pub fn imbue_attempts(&self) -> i32 {
        self.get_property(PropertyInt::ImbueAttempts).unwrap_or(0)
    }

    // ACE: Player.ImbueAttempts
    pub fn set_imbue_attempts(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::ImbueAttempts);
        } else {
            self.set_property(PropertyInt::ImbueAttempts, value);
        }
    }

    // ACE: Player.ImbueSuccesses
    pub fn imbue_successes(&self) -> i32 {
        self.get_property(PropertyInt::ImbueSuccesses).unwrap_or(0)
    }

    // ACE: Player.ImbueSuccesses
    pub fn set_imbue_successes(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::ImbueSuccesses);
        } else {
            self.set_property(PropertyInt::ImbueSuccesses, value);
        }
    }
}
