//! An object drawn with another era's look keeps the world's setup and takes the other era's
//! parts, surfaces and pictures; the texture changes its object description carries are moved
//! onto the other era's pictures, slot by slot, because the eras painted the same part with
//! different picture ids. These are the data halves; the drawn frames are the `gpu` tier's
//! `rendering::object_modes`.
//!
//! Fixture: the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) with the retail dats
//! (`DERETH_TEST_DAT_DIR`) beside them. A missing input fails.

use dereth_animation::parts::{PhysicsPart, SurfaceOverrides};
use dereth_assets::{Decode, Setup};
use dereth_client::models::{parts_for_look, surface_textures};
use dereth_dat::{ContainerEra, DbType, RetailDatStore};
use dereth_primitives::DataId;

/// The February 2005 world with the end-of-retail files beside it.
fn older_world() -> RetailDatStore {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    RetailDatStore::open_pre_tod_with_later(&world, &dereth_dat::testing::dat_dir())
        .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}"))
}

fn part(gfxobj: u32, maps: &[(u32, u32)]) -> PhysicsPart {
    let mut p = PhysicsPart::new(DataId(gfxobj));
    p.surface_overrides = Some(SurfaceOverrides {
        texture_maps: maps.iter().map(|&(o, n)| (DataId(o), DataId(n))).collect(),
        ..SurfaceOverrides::default()
    });
    p
}

fn maps(p: &PhysicsPart) -> Vec<(u32, u32)> {
    p.surface_overrides
        .as_ref()
        .map(|s| s.texture_maps.iter().map(|(o, n)| (o.0, n.0)).collect())
        .unwrap_or_default()
}

fn parts_of(store: &RetailDatStore, setup: u32) -> Vec<u32> {
    let id = DataId(setup);
    let bytes = store
        .read_typed(DbType::Setup, id)
        .expect("the setup reads");
    Setup::decode_payload_in(store.era_of(id), id, &bytes)
        .expect("the setup decodes")
        .parts
        .iter()
        .map(|d| d.0)
        .collect()
}

/// Behaviour: rendering.objects.a-clothing-picture-follows-the-surface-onto-the-other-eras-part
/// The February 2005 shirt replaces the human arm's picture `0x050002CC` with its sleeve
/// `0x050002BE`. The later files repainted the arm (`0x01000055`): its one surface draws
/// `0x050003DD`. With the later look, the change goes to that surface, which is what the later
/// shirt table names for the same garment; a change the later part carries already is left
/// alone.
#[test]
fn a_clothing_picture_follows_the_surface_onto_the_later_arm() {
    let world = older_world();
    let look = world
        .object_files(ContainerEra::Tod)
        .expect("the later files are beside the older world");
    assert_eq!(
        surface_textures(&world, DataId(0x0100_0055)),
        Some(vec![Some(DataId(0x0500_02CC))]),
        "the February 2005 arm"
    );
    assert_eq!(
        surface_textures(&look, DataId(0x0100_0055)),
        Some(vec![Some(DataId(0x0500_03DD))]),
        "the later arm"
    );
    let drawn = parts_for_look(
        &world,
        &look,
        &[
            part(0x0100_0055, &[(0x0500_02CC, 0x0500_02BE)]),
            part(0x0100_0055, &[(0x0500_03DD, 0x0500_02BE)]),
        ],
    )
    .expect("both changes can be placed");
    assert_eq!(maps(&drawn[0]), [(0x0500_03DD, 0x0500_02BE)]);
    assert_eq!(maps(&drawn[1]), [(0x0500_03DD, 0x0500_02BE)]);
    // The world's own records are untouched by the look.
    let same = parts_for_look(
        &world,
        &world,
        &[part(0x0100_0055, &[(0x0500_02CC, 0x0500_02BE)])],
    )
    .expect("the world's own look");
    assert_eq!(maps(&same[0]), [(0x0500_02CC, 0x0500_02BE)]);
}

/// Behaviour: rendering.objects.an-object-whose-picture-change-cannot-be-placed-keeps-the-worlds-look
/// The reward cow's head (`0x0100007B`) lists six surfaces in February 2005 and four in the later
/// files, and its texture change names a picture only the older head carries: there is no slot
/// to move it to, so the object is drawn with the world's records.
#[test]
fn a_change_with_no_matching_slot_draws_the_object_with_the_worlds_records() {
    let world = older_world();
    let look = world
        .object_files(ContainerEra::Tod)
        .expect("the later files");
    assert_eq!(
        surface_textures(&world, DataId(0x0100_007B)).map(|v| v.len()),
        Some(6)
    );
    assert_eq!(
        surface_textures(&look, DataId(0x0100_007B)).map(|v| v.len()),
        Some(4)
    );
    assert!(parts_for_look(
        &world,
        &look,
        &[part(0x0100_007B, &[(0x0500_0024, 0x0500_1B73)])],
    )
    .is_none());
}

/// Behaviour: rendering.objects.an-object-keeps-the-worlds-setup-under-another-eras-look
/// The human body's setup has 17 parts in February 2005 and 34 in the later files (the later
/// adds 17 placeholder parts for layered armour). The first 17 are the same parts, so the
/// world's setup, which the world's motion data animates, is drawn with the later look's records
/// of those parts.
#[test]
fn the_human_body_keeps_the_worlds_seventeen_parts_and_the_later_files_name_the_same_ones() {
    let world = older_world();
    let look = world
        .object_files(ContainerEra::Tod)
        .expect("the later files");
    let older = parts_of(&world, 0x0200_0001);
    let later = parts_of(&look, 0x0200_0001);
    assert_eq!(older.len(), 17);
    assert_eq!(later.len(), 34);
    assert_eq!(&later[..17], &older[..]);
    assert!(later[17..].iter().all(|&p| p == 0x0100_01EC));
    // Each of those parts reads in the later layout from the later files.
    for p in &older {
        assert_eq!(look.era_of(DataId(*p)), ContainerEra::Tod, "{p:08X}");
    }
}
