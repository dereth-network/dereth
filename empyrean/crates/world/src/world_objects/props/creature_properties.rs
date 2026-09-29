// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Creature_Properties.cs`.

use empyrean_entity::enums::{
    FactionBits, PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString,
};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.ResistSlash
    pub fn resist_slash(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistSlash)
    }

    // ACE: Creature.ResistSlash
    pub fn set_resist_slash(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistSlash),
            Some(v) => self.set_property(PropertyFloat::ResistSlash, v),
        }
    }

    // ACE: Creature.ResistPierce
    pub fn resist_pierce(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistPierce)
    }

    // ACE: Creature.ResistPierce
    pub fn set_resist_pierce(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistPierce),
            Some(v) => self.set_property(PropertyFloat::ResistPierce, v),
        }
    }

    // ACE: Creature.ResistBludgeon
    pub fn resist_bludgeon(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistBludgeon)
    }

    // ACE: Creature.ResistBludgeon
    pub fn set_resist_bludgeon(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistBludgeon),
            Some(v) => self.set_property(PropertyFloat::ResistBludgeon, v),
        }
    }

    // ACE: Creature.ResistFire
    pub fn resist_fire(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistFire)
    }

    // ACE: Creature.ResistFire
    pub fn set_resist_fire(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistFire),
            Some(v) => self.set_property(PropertyFloat::ResistFire, v),
        }
    }

    // ACE: Creature.ResistCold
    pub fn resist_cold(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistCold)
    }

    // ACE: Creature.ResistCold
    pub fn set_resist_cold(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistCold),
            Some(v) => self.set_property(PropertyFloat::ResistCold, v),
        }
    }

    // ACE: Creature.ResistAcid
    pub fn resist_acid(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistAcid)
    }

    // ACE: Creature.ResistAcid
    pub fn set_resist_acid(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistAcid),
            Some(v) => self.set_property(PropertyFloat::ResistAcid, v),
        }
    }

    // ACE: Creature.ResistElectric
    pub fn resist_electric(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistElectric)
    }

    // ACE: Creature.ResistElectric
    pub fn set_resist_electric(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistElectric),
            Some(v) => self.set_property(PropertyFloat::ResistElectric, v),
        }
    }

    // ACE: Creature.ResistHealthDrain
    pub fn resist_health_drain(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistHealthDrain)
    }

    // ACE: Creature.ResistHealthDrain
    pub fn set_resist_health_drain(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistHealthDrain),
            Some(v) => self.set_property(PropertyFloat::ResistHealthDrain, v),
        }
    }

    // ACE: Creature.ResistHealthBoost
    pub fn resist_health_boost(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistHealthBoost)
    }

    // ACE: Creature.ResistHealthBoost
    pub fn set_resist_health_boost(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistHealthBoost),
            Some(v) => self.set_property(PropertyFloat::ResistHealthBoost, v),
        }
    }

    // ACE: Creature.ResistStaminaDrain
    pub fn resist_stamina_drain(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistStaminaDrain)
    }

    // ACE: Creature.ResistStaminaDrain
    pub fn set_resist_stamina_drain(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistStaminaDrain),
            Some(v) => self.set_property(PropertyFloat::ResistStaminaDrain, v),
        }
    }

    // ACE: Creature.ResistStaminaBoost
    pub fn resist_stamina_boost(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistStaminaBoost)
    }

    // ACE: Creature.ResistStaminaBoost
    pub fn set_resist_stamina_boost(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistStaminaBoost),
            Some(v) => self.set_property(PropertyFloat::ResistStaminaBoost, v),
        }
    }

    // ACE: Creature.ResistManaDrain
    pub fn resist_mana_drain(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistManaDrain)
    }

    // ACE: Creature.ResistManaDrain
    pub fn set_resist_mana_drain(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistManaDrain),
            Some(v) => self.set_property(PropertyFloat::ResistManaDrain, v),
        }
    }

    // ACE: Creature.ResistManaBoost
    pub fn resist_mana_boost(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistManaBoost)
    }

    // ACE: Creature.ResistManaBoost
    pub fn set_resist_mana_boost(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistManaBoost),
            Some(v) => self.set_property(PropertyFloat::ResistManaBoost, v),
        }
    }

    // ACE: Creature.ResistNether
    pub fn resist_nether(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResistNether)
    }

    // ACE: Creature.ResistNether
    pub fn set_resist_nether(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResistNether),
            Some(v) => self.set_property(PropertyFloat::ResistNether, v),
        }
    }

    // ACE: Creature.NonProjectileMagicImmune
    pub fn non_projectile_magic_immune(&self) -> bool {
        self.get_property(PropertyBool::NonProjectileMagicImmune)
            .unwrap_or(false)
    }

    // ACE: Creature.NonProjectileMagicImmune
    pub fn set_non_projectile_magic_immune(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::NonProjectileMagicImmune);
        } else {
            self.set_property(PropertyBool::NonProjectileMagicImmune, value);
        }
    }

    // ACE: Creature.HealthRate
    pub fn health_rate(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HealthRate)
    }

    // ACE: Creature.HealthRate
    pub fn set_health_rate(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HealthRate),
            Some(v) => self.set_property(PropertyFloat::HealthRate, v),
        }
    }

    // ACE: Creature.StaminaRate
    pub fn stamina_rate(&self) -> Option<f64> {
        self.get_property(PropertyFloat::StaminaRate)
    }

    // ACE: Creature.StaminaRate
    pub fn set_stamina_rate(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::StaminaRate),
            Some(v) => self.set_property(PropertyFloat::StaminaRate, v),
        }
    }

    // ACE: Creature.NoCorpse
    pub fn no_corpse(&self) -> bool {
        self.get_property(PropertyBool::NoCorpse).unwrap_or(false)
    }

    // ACE: Creature.NoCorpse
    pub fn set_no_corpse(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::NoCorpse);
        } else {
            self.set_property(PropertyBool::NoCorpse, value);
        }
    }

    // ACE: Creature.TreasureCorpse
    pub fn treasure_corpse(&self) -> bool {
        self.get_property(PropertyBool::TreasureCorpse)
            .unwrap_or(false)
    }

    // ACE: Creature.TreasureCorpse
    pub fn set_treasure_corpse(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::TreasureCorpse);
        } else {
            self.set_property(PropertyBool::TreasureCorpse, value);
        }
    }

    // ACE: Creature.DeathTreasureType
    pub fn death_treasure_type(&self) -> Option<u32> {
        self.get_property(PropertyDataId::DeathTreasureType)
    }

    // ACE: Creature.DeathTreasureType
    pub fn set_death_treasure_type(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::DeathTreasureType),
            Some(v) => self.set_property(PropertyDataId::DeathTreasureType, v),
        }
    }

    // ACE: Creature.LuminanceAward
    pub fn luminance_award(&self) -> Option<i32> {
        self.get_property(PropertyInt::LuminanceAward)
    }

    // ACE: Creature.LuminanceAward
    pub fn set_luminance_award(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::LuminanceAward),
            Some(v) => self.set_property(PropertyInt::LuminanceAward, v),
        }
    }

    // ACE: Creature.AiImmobile
    pub fn ai_immobile(&self) -> bool {
        self.get_property(PropertyBool::AiImmobile).unwrap_or(false)
    }

    // ACE: Creature.AiImmobile
    pub fn set_ai_immobile(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AiImmobile);
        } else {
            self.set_property(PropertyBool::AiImmobile, value);
        }
    }

    // ACE: Creature.Overpower
    pub fn overpower(&self) -> Option<i32> {
        self.get_property(PropertyInt::Overpower)
    }

    // ACE: Creature.Overpower
    pub fn set_overpower(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Overpower),
            Some(v) => self.set_property(PropertyInt::Overpower, v),
        }
    }

    // ACE: Creature.OverpowerResist
    pub fn overpower_resist(&self) -> Option<i32> {
        self.get_property(PropertyInt::OverpowerResist)
    }

    // ACE: Creature.OverpowerResist
    pub fn set_overpower_resist(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::OverpowerResist),
            Some(v) => self.set_property(PropertyInt::OverpowerResist, v),
        }
    }

    // ACE: Creature.KillQuest
    pub fn kill_quest(&self) -> Option<String> {
        self.get_property(PropertyString::KillQuest)
    }

    // ACE: Creature.KillQuest
    pub fn set_kill_quest(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::KillQuest),
            Some(v) => self.set_property(PropertyString::KillQuest, v),
        }
    }

    // ACE: Creature.KillQuest2
    pub fn kill_quest2(&self) -> Option<String> {
        self.get_property(PropertyString::KillQuest2)
    }

    // ACE: Creature.KillQuest2
    pub fn set_kill_quest2(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::KillQuest2),
            Some(v) => self.set_property(PropertyString::KillQuest2, v),
        }
    }

    // ACE: Creature.KillQuest3
    pub fn kill_quest3(&self) -> Option<String> {
        self.get_property(PropertyString::KillQuest3)
    }

    // ACE: Creature.KillQuest3
    pub fn set_kill_quest3(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::KillQuest3),
            Some(v) => self.set_property(PropertyString::KillQuest3, v),
        }
    }

    // ACE: Creature.Faction1Bits
    pub fn faction1_bits(&self) -> Option<FactionBits> {
        self.get_property(PropertyInt::Faction1Bits)
            .map(FactionBits)
    }

    // ACE: Creature.Faction1Bits
    pub fn set_faction1_bits(&mut self, value: Option<FactionBits>) {
        match value {
            None => self.remove_property(PropertyInt::Faction1Bits),
            Some(v) => self.set_property(PropertyInt::Faction1Bits, v.0),
        }
    }

    // ACE: Creature.Faction2Bits
    pub fn faction2_bits(&self) -> Option<i32> {
        self.get_property(PropertyInt::Faction2Bits)
    }

    // ACE: Creature.Faction2Bits
    pub fn set_faction2_bits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Faction2Bits),
            Some(v) => self.set_property(PropertyInt::Faction2Bits, v),
        }
    }

    // ACE: Creature.Faction3Bits
    pub fn faction3_bits(&self) -> Option<i32> {
        self.get_property(PropertyInt::Faction3Bits)
    }

    // ACE: Creature.Faction3Bits
    pub fn set_faction3_bits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Faction3Bits),
            Some(v) => self.set_property(PropertyInt::Faction3Bits, v),
        }
    }

    // ACE: Creature.Hatred1Bits
    pub fn hatred1_bits(&self) -> Option<i32> {
        self.get_property(PropertyInt::Hatred1Bits)
    }

    // ACE: Creature.Hatred1Bits
    pub fn set_hatred1_bits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Hatred1Bits),
            Some(v) => self.set_property(PropertyInt::Hatred1Bits, v),
        }
    }

    // ACE: Creature.Hatred2Bits
    pub fn hatred2_bits(&self) -> Option<i32> {
        self.get_property(PropertyInt::Hatred2Bits)
    }

    // ACE: Creature.Hatred2Bits
    pub fn set_hatred2_bits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Hatred2Bits),
            Some(v) => self.set_property(PropertyInt::Hatred2Bits, v),
        }
    }

    // ACE: Creature.Hatred3Bits
    pub fn hatred3_bits(&self) -> Option<i32> {
        self.get_property(PropertyInt::Hatred3Bits)
    }

    // ACE: Creature.Hatred3Bits
    pub fn set_hatred3_bits(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::Hatred3Bits),
            Some(v) => self.set_property(PropertyInt::Hatred3Bits, v),
        }
    }

    // ACE: Creature.SocietyRankCelhan
    pub fn society_rank_celhan(&self) -> Option<i32> {
        self.get_property(PropertyInt::SocietyRankCelhan)
    }

    // ACE: Creature.SocietyRankCelhan
    pub fn set_society_rank_celhan(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::SocietyRankCelhan),
            Some(v) => self.set_property(PropertyInt::SocietyRankCelhan, v),
        }
    }

    // ACE: Creature.SocietyRankEldweb
    pub fn society_rank_eldweb(&self) -> Option<i32> {
        self.get_property(PropertyInt::SocietyRankEldweb)
    }

    // ACE: Creature.SocietyRankEldweb
    pub fn set_society_rank_eldweb(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::SocietyRankEldweb),
            Some(v) => self.set_property(PropertyInt::SocietyRankEldweb, v),
        }
    }

    // ACE: Creature.SocietyRankRadblo
    pub fn society_rank_radblo(&self) -> Option<i32> {
        self.get_property(PropertyInt::SocietyRankRadblo)
    }

    // ACE: Creature.SocietyRankRadblo
    pub fn set_society_rank_radblo(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::SocietyRankRadblo),
            Some(v) => self.set_property(PropertyInt::SocietyRankRadblo, v),
        }
    }
}
