//! Every player setup the client can select collides with the same two spheres, with or without
//! its parts: none carries a physics BSP or a cylsphere, so building the local body from the setup
//! alone (`character::setup_geometry`) and from its parts agree.
//!
//! Fixture: the retail character-generation record `0x0E000002`, the setups it and the barber
//! globals name, and their parts' meshes.

use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_assets::{CharGen, Decode, GfxObj, Setup};
use dereth_client::object_physics::{setup_geometry_with_parts, SetupPartStats};
use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;

const CHARGEN: DataId = DataId(0x0E00_0002);

// The barber setup globals for the two crown families and for the skeleton/zombie flame/no-flame
// variants. Some crown/flame entries already occur as the selected sex's setup or a hairstyle's
// `alternate_setup`; a set deliberately removes those overlaps.
const SPECIAL_BARBER_SETUPS: [DataId; 16] = [
    DataId(0x0200_196F),
    DataId(0x0200_1A5F),
    DataId(0x0200_1970),
    DataId(0x0200_1A5E),
    DataId(0x0200_196E),
    DataId(0x0200_1A5D),
    DataId(0x0200_196D),
    DataId(0x0200_1A5C),
    DataId(0x0200_1A9C),
    DataId(0x0200_1A9E),
    DataId(0x0200_1A9D),
    DataId(0x0200_1A96),
    DataId(0x0200_1AA0),
    DataId(0x0200_1A9F),
    DataId(0x0200_1AA1),
    DataId(0x0200_1AA2),
];

fn store() -> Arc<RetailDatStore> {
    let dir = dereth_dat::testing::dat_dir();
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "no retail dats under {} -- set DERETH_TEST_DAT_DIR",
            dir.display()
        )
    }))
}

fn decode<T: Decode>(store: &RetailDatStore, id: DataId) -> T {
    let bytes = store
        .read_typed(T::TYPE, id)
        .unwrap_or_else(|e| panic!("{id:?} ({:?}): {e}", T::TYPE));
    T::decode_payload(id, &bytes).unwrap_or_else(|e| panic!("{id:?} ({:?}): {e}", T::TYPE))
}

/// Behaviour: objects.player.every-player-setup-collides-with-two-spheres
#[test]
fn every_reachable_player_setup_uses_the_same_two_sphere_arm_with_or_without_parts() {
    let store = store();
    let chargen: CharGen = decode(&store, CHARGEN);

    let sex_setups: BTreeSet<_> = chargen
        .heritage_groups
        .values()
        .flat_map(|heritage| heritage.sexes.values().map(|sex| sex.setup))
        .collect();
    assert_eq!(
        sex_setups.len(),
        18,
        "the shipped sex-specific setup family moved"
    );

    let hair_alternates: BTreeSet<_> = chargen
        .heritage_groups
        .values()
        .flat_map(|heritage| heritage.sexes.values())
        .flat_map(|sex| sex.hair_styles.iter().map(|hair| hair.alternate_setup))
        .filter(|id| id.0 != 0)
        .collect();
    assert_eq!(
        hair_alternates.len(),
        6,
        "the shipped nonzero hair alternate family moved"
    );

    let mut reachable = sex_setups;
    reachable.extend(hair_alternates);
    reachable.extend(SPECIAL_BARBER_SETUPS);
    assert_eq!(
        reachable.len(),
        30,
        "the deduplicated reachable player setup family moved"
    );

    let mut total_parts = 0usize;
    for id in reachable {
        let setup: Setup = decode(&store, id);
        assert_eq!(
            setup.spheres.len(),
            2,
            "{id:?} left the player two-sphere arm"
        );
        assert!(
            setup.cylspheres.is_empty(),
            "{id:?} gained a cylsphere collision arm"
        );
        assert!(!setup.has_physics_bsp, "{id:?} serializes HAS_PHYSICS_BSP");

        for part in &setup.parts {
            let gfx: GfxObj = decode(&store, *part);
            assert!(
                gfx.physics_bsp.is_none(),
                "{id:?} part {part:?} has a physics BSP"
            );
        }
        total_parts += setup.parts.len();

        let simple = dereth_client::character::setup_geometry(&setup);
        let mut stats = SetupPartStats::default();
        let complete = setup_geometry_with_parts(&store, &setup, &mut stats);
        assert!(
            !simple.caches_physics_bsp(),
            "{id:?} simple geometry selects BSP"
        );
        assert!(
            !complete.caches_physics_bsp(),
            "{id:?} complete geometry selects BSP"
        );
        assert_eq!(
            simple.spheres, complete.spheres,
            "{id:?} sphere geometry changed with parts"
        );
    }

    assert_eq!(
        total_parts, 1_008,
        "the reachable setup part population moved"
    );
}
