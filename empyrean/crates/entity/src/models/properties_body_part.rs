// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesBodyPart.cs
//! `PropertiesBodyPart`: a creature body part: damage and armor, and the hit-location weights.

use crate::enums::DamageType;

/// ACE: PropertiesBodyPart. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesBodyPart::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesBodyPart {
    // ACE: PropertiesBodyPart.DType
    pub d_type: DamageType,
    // ACE: PropertiesBodyPart.DVal
    pub d_val: i32,
    // ACE: PropertiesBodyPart.DVar
    pub d_var: f32,
    // ACE: PropertiesBodyPart.BaseArmor
    pub base_armor: i32,
    // ACE: PropertiesBodyPart.ArmorVsSlash
    pub armor_vs_slash: i32,
    // ACE: PropertiesBodyPart.ArmorVsPierce
    pub armor_vs_pierce: i32,
    // ACE: PropertiesBodyPart.ArmorVsBludgeon
    pub armor_vs_bludgeon: i32,
    // ACE: PropertiesBodyPart.ArmorVsCold
    pub armor_vs_cold: i32,
    // ACE: PropertiesBodyPart.ArmorVsFire
    pub armor_vs_fire: i32,
    // ACE: PropertiesBodyPart.ArmorVsAcid
    pub armor_vs_acid: i32,
    // ACE: PropertiesBodyPart.ArmorVsElectric
    pub armor_vs_electric: i32,
    // ACE: PropertiesBodyPart.ArmorVsNether
    pub armor_vs_nether: i32,
    // ACE: PropertiesBodyPart.BH
    pub bh: i32,
    // ACE: PropertiesBodyPart.HLF
    pub hlf: f32,
    // ACE: PropertiesBodyPart.MLF
    pub mlf: f32,
    // ACE: PropertiesBodyPart.LLF
    pub llf: f32,
    // ACE: PropertiesBodyPart.HRF
    pub hrf: f32,
    // ACE: PropertiesBodyPart.MRF
    pub mrf: f32,
    // ACE: PropertiesBodyPart.LRF
    pub lrf: f32,
    // ACE: PropertiesBodyPart.HLB
    pub hlb: f32,
    // ACE: PropertiesBodyPart.MLB
    pub mlb: f32,
    // ACE: PropertiesBodyPart.LLB
    pub llb: f32,
    // ACE: PropertiesBodyPart.HRB
    pub hrb: f32,
    // ACE: PropertiesBodyPart.MRB
    pub mrb: f32,
    // ACE: PropertiesBodyPart.LRB
    pub lrb: f32,
}

impl PropertiesBodyPart {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesBodyPart.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesBodyPart {
        PropertiesBodyPart {
            d_type: self.d_type,
            d_val: self.d_val,
            d_var: self.d_var,
            base_armor: self.base_armor,
            armor_vs_slash: self.armor_vs_slash,
            armor_vs_pierce: self.armor_vs_pierce,
            armor_vs_bludgeon: self.armor_vs_bludgeon,
            armor_vs_cold: self.armor_vs_cold,
            armor_vs_fire: self.armor_vs_fire,
            armor_vs_acid: self.armor_vs_acid,
            armor_vs_electric: self.armor_vs_electric,
            armor_vs_nether: self.armor_vs_nether,
            bh: self.bh,
            hlf: self.hlf,
            mlf: self.mlf,
            llf: self.llf,
            hrf: self.hrf,
            mrf: self.mrf,
            lrf: self.lrf,
            hlb: self.hlb,
            mlb: self.mlb,
            llb: self.llb,
            hrb: self.hrb,
            mrb: self.mrb,
            lrb: self.lrb,
        }
    }
}
