//! Application-owned DAT facts for part-array initialization and child attachment.
//! No renderer or physics allocation is required to validate a holding location.
//!
//! The answer is returned as the holding-location set rather than as
//! `dereth_client_model::objects::PhysicsSetupFacts`, so this crate does not depend on the game
//! model; `dereth_client_runtime::objects` wraps it back into that enum
//! (`Some` is `Ready`, `None` is `Failed`).
use std::collections::BTreeSet;

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// Whether part-array initialization succeeds for the object-description setup id `wire_id`, and
/// if it does, the setup's holding locations. `None` is initialization failing.
#[must_use]
pub fn resolve(store: &RetailDatStore, wire_id: u32) -> Option<BTreeSet<u32>> {
    // Part-array object initialization rejects INVALID_DID; the data-id type selects mesh/setup,
    // with the legacy unqualified setup-id fallback for a zero high byte.
    if wire_id == 0 {
        return None;
    }
    if (0x0100_0000..0x0101_0000).contains(&wire_id) {
        return if part_initializes(store, DataId(wire_id)) {
            // A simple setup has no holding locations.
            Some(BTreeSet::new())
        } else {
            None
        };
    }
    let id = if wire_id & 0xff00_0000 == 0 {
        wire_id | 0x0200_0000
    } else {
        wire_id
    };
    if !(0x0200_0000..0x0201_0000).contains(&id) {
        return None;
    }
    let did = DataId(id);
    let setup = store
        .read_typed(DbType::Setup, did)
        .ok()
        .and_then(|b| Setup::decode_payload_in(store.era_of(did), did, &b).ok())?;
    // Part initialization fails for a zero-part setup or any part that cannot be constructed.
    if setup.parts.is_empty() || !setup.parts.iter().all(|id| part_initializes(store, *id)) {
        return None;
    }
    Some(setup.holding_locations.keys().copied().collect())
}

fn part_initializes(store: &RetailDatStore, id: DataId) -> bool {
    let Some(gfx) = store
        .read_typed(DbType::GfxObj, id)
        .ok()
        .and_then(|b| GfxObj::decode_payload_in(store.era_of(id), id, &b).ok())
    else {
        return false;
    };
    // Graphics-object loading falls back to the base when degrade info cannot load.
    // If it loads, only element zero is required; later null levels are permitted.
    let degrade = gfx.did_degrade.and_then(|id| {
        store
            .read_typed(DbType::DegradeInfo, id)
            .ok()
            .and_then(|b| GfxObjDegradeInfo::decode_payload_in(store.era_of(id), id, &b).ok())
    });
    let Some(degrade) = degrade else { return true };
    degrade.degrades.first().is_some_and(|level| {
        level.gfxobj_id.0 != 0
            && store
                .read_typed(DbType::GfxObj, level.gfxobj_id)
                .ok()
                .and_then(|b| {
                    GfxObj::decode_payload_in(store.era_of(level.gfxobj_id), level.gfxobj_id, &b)
                        .ok()
                })
                .is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn installed_setup_legacy_id_and_simple_mesh_keep_distinct_holding_facts() {
        let store = dereth_dat::testing::open_store_or_fail();
        let human = resolve(&store, 0x0200_0001);
        assert!(matches!(&human, Some(holding_locations)
            if holding_locations.contains(&1) && !holding_locations.contains(&7)));
        assert_eq!(
            resolve(&store, 1),
            human,
            "legacy low-id setup qualification"
        );
        let id = DataId(0x0200_0001);
        let setup =
            Setup::decode_payload(id, &store.read_typed(DbType::Setup, id).unwrap()).unwrap();
        assert_eq!(setup.parts.len(), 34);
        assert_eq!(
            resolve(&store, setup.parts[0].0),
            Some(BTreeSet::new()),
            "a valid mesh initializes but its simple setup cannot hold anything"
        );
        for invalid in [0, 0x0200_ffff, 0x0800_0001, 0x0201_0000] {
            assert_eq!(
                resolve(&store, invalid),
                None,
                "invalid/missing setup {invalid:08X}"
            );
        }
    }
}
