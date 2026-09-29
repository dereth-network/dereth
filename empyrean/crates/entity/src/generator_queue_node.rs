// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/GeneratorQueueNode.cs
//! `GeneratorQueueNode` (fields only).

/// ACE: GeneratorQueueNode
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeneratorQueueNode {
    // ACE: GeneratorQueueNode.Slot
    pub slot: u32,
    // ACE: GeneratorQueueNode.SpawnTime
    pub spawn_time: f64,
}
