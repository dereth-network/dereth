// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/GeneratorRegistryNode.cs
//! `GeneratorRegistryNode` (fields only).

/// ACE: GeneratorRegistryNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeneratorRegistryNode {
    // ACE: GeneratorRegistryNode.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: GeneratorRegistryNode.Timestamp
    pub timestamp: f64,
    // ACE: GeneratorRegistryNode.TreasureType
    pub treasure_type: u32,
    // ACE: GeneratorRegistryNode.Slot
    pub slot: u32,
    // ACE: GeneratorRegistryNode.Checkpointed
    pub checkpointed: u32,
    // ACE: GeneratorRegistryNode.Shop
    pub shop: u32,
    // ACE: GeneratorRegistryNode.Amount
    pub amount: u32,
}
