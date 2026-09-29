// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesBodyPart.cs
//! `BiotaPropertiesBodyPart`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesBodyPart
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesBodyPart {
    // ACE: BiotaPropertiesBodyPart.Id
    pub id: u32,
    // ACE: BiotaPropertiesBodyPart.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesBodyPart.Key
    pub key: u16,
    // ACE: BiotaPropertiesBodyPart.DType
    pub d_type: i32,
    // ACE: BiotaPropertiesBodyPart.DVal
    pub d_val: i32,
    // ACE: BiotaPropertiesBodyPart.DVar
    pub d_var: f32,
    // ACE: BiotaPropertiesBodyPart.BaseArmor
    pub base_armor: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsSlash
    pub armor_vs_slash: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsPierce
    pub armor_vs_pierce: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsBludgeon
    pub armor_vs_bludgeon: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsCold
    pub armor_vs_cold: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsFire
    pub armor_vs_fire: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsAcid
    pub armor_vs_acid: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsElectric
    pub armor_vs_electric: i32,
    // ACE: BiotaPropertiesBodyPart.ArmorVsNether
    pub armor_vs_nether: i32,
    // ACE: BiotaPropertiesBodyPart.BH
    pub bh: i32,
    // ACE: BiotaPropertiesBodyPart.HLF
    pub hlf: f32,
    // ACE: BiotaPropertiesBodyPart.MLF
    pub mlf: f32,
    // ACE: BiotaPropertiesBodyPart.LLF
    pub llf: f32,
    // ACE: BiotaPropertiesBodyPart.HRF
    pub hrf: f32,
    // ACE: BiotaPropertiesBodyPart.MRF
    pub mrf: f32,
    // ACE: BiotaPropertiesBodyPart.LRF
    pub lrf: f32,
    // ACE: BiotaPropertiesBodyPart.HLB
    pub hlb: f32,
    // ACE: BiotaPropertiesBodyPart.MLB
    pub mlb: f32,
    // ACE: BiotaPropertiesBodyPart.LLB
    pub llb: f32,
    // ACE: BiotaPropertiesBodyPart.HRB
    pub hrb: f32,
    // ACE: BiotaPropertiesBodyPart.MRB
    pub mrb: f32,
    // ACE: BiotaPropertiesBodyPart.LRB
    pub lrb: f32,
}
