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
    crate::world_objects::managers::contract_manager::abandon(w, this, contract_id);
}
