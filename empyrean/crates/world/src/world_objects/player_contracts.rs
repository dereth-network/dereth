// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Contracts.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Contracts.cs`.

/// Non-property fields declared in `Player_Contracts.cs`.
#[derive(Debug, Default)]
pub struct PlayerContractsFields {}

// ---- dispatch targets ----

// ACE: Player.HandleActionAbandonContract
pub fn handle_action_abandon_contract(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    contract_id: u32,
) {
    // DIVERGE: a world without the contract tracker (`EraFeatures::contracts`) refuses
    // abandoning a contract (V426).
    if !crate::world_objects::era_gates::has(w, this, w.era.features.contracts, "contracts") {
        return;
    }
    crate::world_objects::managers::contract_manager::abandon(w, this, contract_id);
}
