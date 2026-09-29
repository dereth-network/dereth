//! Application-owned DAT facts for part-array initialization and child attachment. The dat walk
//! is `dereth_world_data::physics_setup`; this wraps its answer into the game
//! model's `PhysicsSetupFacts`.
use dereth_client_model::objects::PhysicsSetupFacts;
use dereth_dat::RetailDatStore;

pub(super) fn resolve(store: &RetailDatStore, wire_id: u32) -> PhysicsSetupFacts {
    match dereth_world_data::physics_setup::resolve(store, wire_id) {
        Some(holding_locations) => PhysicsSetupFacts::Ready { holding_locations },
        None => PhysicsSetupFacts::Failed,
    }
}
