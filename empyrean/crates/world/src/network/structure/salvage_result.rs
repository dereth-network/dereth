// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/SalvageResult.cs
//! Port of `Source/ACE.Server/Network/Structure/SalvageResult.cs`.

use empyrean_entity::enums::{MaterialType, Skill};

use crate::network::game_messages::game_message::write_record;

/// Stand-in for `ACE.Server.Entity.SalvageMessage` (`Entity/SalvageResults.cs`, not ported yet
/// by its owner): its fields.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SalvageMessage {
    /// `SalvageMessage.Amount`.
    pub amount: u32,
    /// `SalvageMessage.MaterialType`.
    pub material_type: MaterialType,
    /// `SalvageMessage.Workmanship`.
    pub workmanship: f32,
    /// `SalvageMessage.NumItemsInMaterial`.
    pub num_items_in_material: i32,
    /// `SalvageMessage.Skill`.
    pub skill: Skill,
}

// ACE: SalvageResult
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SalvageResult {
    // ACE: SalvageResult.MaterialType
    pub material_type: MaterialType,
    // ACE: SalvageResult.Workmanship
    pub workmanship: f64,
    // ACE: SalvageResult.Units
    pub units: u32,
}

// ACE: SalvageResult.SalvageResult
/// `new SalvageResult(SalvageMessage message)`: `Workmanship / NumItemsInMaterial` is a `float`
/// division (the `int` promotes), stored as `double`.
#[allow(clippy::cast_precision_loss)] // `int` to `float`, as C# promotes it
pub fn salvage_result_new(message: &SalvageMessage) -> SalvageResult {
    SalvageResult {
        material_type: message.material_type,
        workmanship: f64::from(message.workmanship / message.num_items_in_material as f32),
        units: message.amount,
    }
}

// ACE: SalvageResultExtensions.Write
pub fn write(writer: &mut Vec<u8>, result: &SalvageResult) {
    write_record(writer, &[], |w| record(result).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(result: &SalvageResult) -> dereth_protocol::items::SalvageResult {
    dereth_protocol::items::SalvageResult {
        material: result.material_type.0,
        workmanship: result.workmanship,
        units: result.units.cast_signed(),
    }
}
