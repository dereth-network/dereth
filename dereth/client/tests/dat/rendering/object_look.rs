//! An object drawn with another era's look keeps the world's setup, and each of its parts draws
//! wholly from one era: the other era's model, surfaces, pictures and palettes where the other
//! era's model of that id is the same object, the world's otherwise. The texture changes its
//! object description carries are moved onto the other era's pictures, slot by slot, because the
//! eras painted the same part with different picture ids, and a part wearing the world's bare
//! model draws the other era's bare model, a head the other era's head for the same hair style, a
//! colour the other era lacks its own colour from the same place, and a creature the other era
//! remodelled its remodel where it rests as the world's does. These are the data halves; the
//! drawn frames are the `gpu` tier's `rendering::object_modes`.
//!
//! Fixture: the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) with the retail dats
//! (`DERETH_TEST_DAT_DIR`) beside them. A missing input fails.

use dereth_animation::parts::{PaletteRange, PhysicsPart, SurfaceOverrides};
use dereth_assets::{Decode, PaletteSet, Setup, Surface};
use dereth_client::models::{parts_for_look, surface_textures};
use dereth_client::object_identity::ObjectIdentity;
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

/// The end-of-retail world with the February 2005 portal beside it for presentation.
fn end_of_retail_world() -> RetailDatStore {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let legacy = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    dereth_dat::testing::open_store_or_fail()
        .with_legacy_portal(&legacy)
        .unwrap_or_else(|e| panic!("the February 2005 portal did not attach: {e}"))
}

/// The other era's files for `world` and the verdicts for the pair.
fn look_of(world: &RetailDatStore, era: ContainerEra) -> (RetailDatStore, ObjectIdentity) {
    let look = world
        .object_files(era)
        .expect("the other era's files are beside the world");
    let (identity, _) = ObjectIdentity::build(world.portal(), look.portal());
    (look, identity)
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

fn picture_of_surface(store: &RetailDatStore, surface: u32) -> Option<u32> {
    let id = DataId(surface);
    let bytes = store.read_typed(DbType::Surface, id).ok()?;
    Surface::decode_payload_in(store.era_of(id), id, &bytes)
        .ok()?
        .orig_texture_id
        .map(|t| t.0)
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
    let (look, identity) = look_of(&world, ContainerEra::Tod);
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
    let drawn: Vec<PhysicsPart> = parts_for_look(
        &world,
        &look,
        &identity,
        None,
        &[
            part(0x0100_0055, &[(0x0500_02CC, 0x0500_02BE)]),
            part(0x0100_0055, &[(0x0500_03DD, 0x0500_02BE)]),
        ],
    )
    .into_iter()
    .map(|p| p.expect("both changes can be placed"))
    .collect();
    assert_eq!(maps(&drawn[0]), [(0x0500_03DD, 0x0500_02BE)]);
    assert_eq!(maps(&drawn[1]), [(0x0500_03DD, 0x0500_02BE)]);
}

/// Behaviour: rendering.objects.an-object-whose-picture-change-cannot-be-placed-keeps-the-worlds-look
/// The reward cow's head (`0x0100007B`) lists six surfaces in February 2005 and four in the later
/// files, and its texture change names a picture only the older head carries: there is no slot
/// to move it to, so the head is drawn with the world's records.
#[test]
fn a_change_with_no_matching_slot_draws_the_object_with_the_worlds_records() {
    let world = older_world();
    let (look, identity) = look_of(&world, ContainerEra::Tod);
    assert_eq!(
        surface_textures(&world, DataId(0x0100_007B)).map(|v| v.len()),
        Some(6)
    );
    assert_eq!(
        surface_textures(&look, DataId(0x0100_007B)).map(|v| v.len()),
        Some(4)
    );
    let drawn = parts_for_look(
        &world,
        &look,
        &identity,
        None,
        &[part(0x0100_007B, &[(0x0500_0024, 0x0500_1B73)])],
    );
    assert!(
        drawn[0].is_none(),
        "the head is drawn with the world's records"
    );
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

/// Behaviour: rendering.objects.a-part-draws-wholly-from-one-era
/// The end-of-retail hair `0x0100481D` is a later model the February 2005 files do not hold. Its
/// first surface, `0x08001757`, is an id the older files also hold, for an unrelated surface
/// with another picture: the eras number surfaces apart, and of the 2,349 surface ids both
/// portals hold, two are named by a model of the same id in both. So with the older look the
/// hair is drawn wholly from the world's records, never as the world's model with the older
/// files' surfaces, and a later model the older files do hold (the sleeve `0x0100122B`, the
/// same geometry in both) is drawn wholly from the older files.
#[test]
fn a_later_model_the_older_files_lack_is_drawn_wholly_from_the_worlds_records() {
    let world = end_of_retail_world();
    let look = world
        .object_files(ContainerEra::PreTod)
        .expect("the older files");
    let (identity, census) = ObjectIdentity::build(world.portal(), look.portal());
    let hair = DataId(0x0100_481D);
    assert!(
        !look.portal().contains(hair),
        "the older files lack the hair"
    );
    assert_eq!(picture_of_surface(&world, 0x0800_1757), Some(0x0500_30EF));
    assert_eq!(picture_of_surface(&look, 0x0800_1757), Some(0x0500_1A72));
    assert!(census.surfaces_shared > 2_000, "{}", census.surfaces_shared);
    assert!(
        census.surfaces_named_alike <= 2,
        "{} of {} shared surfaces named alike",
        census.surfaces_named_alike,
        census.surfaces_shared
    );
    let drawn = parts_for_look(
        &world,
        &look,
        &identity,
        None,
        &[
            part(hair.0, &[(0x0500_0098, 0x0500_10B7)]),
            part(0x0100_122B, &[(0x0500_02CC, 0x0500_02BE)]),
        ],
    );
    assert!(drawn[0].is_none(), "the hair is the world's");
    let sleeve = drawn[1].as_ref().expect("the sleeve is the older files'");
    assert_eq!(sleeve.gfxobj_id, DataId(0x0100_122B));
    assert_eq!(maps(sleeve), [(0x0500_02CC, 0x0500_02BE)]);
    assert!(identity.same_geometry(DataId(0x0100_122B)));
}

/// Behaviour: rendering.objects.a-bare-body-part-draws-as-the-other-eras-bare-part
/// The arm the end-of-retail body wears with nothing on it, `0x01000055`, is in the February
/// 2005 files the armoured arm, painted with mail (`0x050002CC`); that era's bare arm is
/// `0x01000497`, which its character-creation table puts on every new body. So with the older
/// look a bare end-of-retail arm draws the older bare arm, and a dressed one keeps its own
/// model.
#[test]
fn a_bare_arm_draws_as_the_other_eras_bare_arm_and_not_its_armoured_one() {
    let world = end_of_retail_world();
    let (look, identity) = look_of(&world, ContainerEra::PreTod);
    assert_eq!(
        surface_textures(&look, DataId(0x0100_0055)),
        Some(vec![Some(DataId(0x0500_02CC))]),
        "the older files paint that arm with mail"
    );
    let bare = identity
        .bare_part(10, DataId(0x0100_0055))
        .expect("the upper arm translates");
    assert_eq!(bare.model, DataId(0x0100_0497));
    let mut parts: Vec<PhysicsPart> = (0..17)
        .map(|_| PhysicsPart::new(DataId(0x0100_01EC)))
        .collect();
    parts[10] = PhysicsPart::new(DataId(0x0100_0055));
    parts[11] = part(0x0100_11F4, &[(0x0500_02C4, 0x0500_140D)]);
    let drawn = parts_for_look(&world, &look, &identity, None, &parts);
    assert_eq!(
        drawn[10].as_ref().map(|p| p.gfxobj_id),
        Some(DataId(0x0100_0497)),
        "the bare upper arm"
    );
    assert_eq!(
        drawn[11].as_ref().map(|p| p.gfxobj_id),
        Some(DataId(0x0100_11F4)),
        "a sleeve keeps its model"
    );
}

/// Behaviour: rendering.objects.a-building-shell-takes-the-look-only-with-the-worlds-geometry
/// Holtburg's cottage shell `0x01000BC3` has the same geometry in both eras, and the shell
/// `0x01000830` every vertex in the same place (only its polygons were redone), so their doorways
/// meet the world's interiors under either look. The shell `0x01002EF1` was reshaped (some
/// corners moved by up to 29 cm): still the same building, but it keeps the world's look.
#[test]
fn a_building_shell_takes_the_older_look_only_where_its_geometry_is_the_worlds() {
    let world = end_of_retail_world();
    let (_, identity) = look_of(&world, ContainerEra::PreTod);
    for shell in [0x0100_0BC3, 0x0100_0830] {
        assert!(identity.same(DataId(shell)), "{shell:08X}");
        assert!(identity.same_geometry(DataId(shell)), "{shell:08X}");
    }
    assert!(identity.same(DataId(0x0100_2EF1)));
    assert!(!identity.same_geometry(DataId(0x0100_2EF1)));
}

/// Behaviour: rendering.objects.an-interior-room-draws-from-the-other-eras-record-where-it-is-the-same-room
/// The February 2005 cell file holds 447,201 of the end-of-retail world's 734,976 interior cells
/// as the same room in the same place (the same environment and room of it, every vertex where the
/// world has it, the same frame and portals); 283,660 it lacks (the later dungeons and rebuilt
/// buildings) and 4,115 it holds elsewhere or joined otherwise. Those rooms list surfaces the
/// later files renumbered, so a room is drawn from the older record of it, its surfaces read in
/// the older files: Holtburg's cottage room `0xA9B40111` is the same room with other surface ids,
/// and `0xA9B40123` of the house beside it is not the same room. The cottage's rooms are the ones
/// its outdoor portals reach. The other way round, the end-of-retail cell file holds the same
/// 447,201 of the February 2005 world's 455,641.
#[test]
fn an_interior_takes_the_other_eras_room_only_where_it_is_the_same_room_in_the_same_place() {
    use dereth_client_runtime::env_cells::{building_cells, EnvCellLoader};
    use dereth_primitives::CellId;
    let world = end_of_retail_world();
    let interiors = world
        .interior_files(ContainerEra::PreTod)
        .expect("the older cell file is in the legacy folder");
    let (mut identity, _) = ObjectIdentity::build(world.portal(), interiors.portal());
    let census = identity.build_rooms(
        world.cell(),
        interiors.cell(),
        world.portal(),
        interiors.portal(),
    );
    assert_eq!(
        (census.cells, census.same, census.absent, census.other_place),
        (734_976, 447_201, 283_660, 4_115),
        "{census:?}"
    );
    assert_eq!(
        (
            census.undecodable,
            census.other_shape,
            census.surfaces_missing
        ),
        (0, 0, 0)
    );
    let cottage = CellId(0xA9B4_0111);
    assert!(identity.same_room(cottage.0));
    assert!(!identity.same_room(0xA9B4_0123));
    let mut ours = EnvCellLoader::new();
    let mut theirs = EnvCellLoader::new();
    let a = ours.load_cell(&world, cottage).expect("the world's room");
    let b = theirs
        .load_cell(&interiors, cottage)
        .expect("the older room");
    assert_eq!(a.cell.frame, b.cell.frame);
    assert_eq!(a.structure.vertex_array, b.structure.vertex_array);
    assert_ne!(
        a.cell.surfaces, b.cell.surfaces,
        "the surfaces were renumbered"
    );

    let block = ours.load_block(&world, 0xA9B4);
    let lbi = DataId(0xA9B4_FFFE);
    let bytes = world
        .read_typed(DbType::Lbi, lbi)
        .expect("the block info reads");
    let info =
        dereth_assets::world::LandblockInfo::decode_payload_in(world.era_of(lbi), lbi, &bytes)
            .expect("the block info decodes");
    let owned = building_cells(0xA9B4, &info.buildings, &block);
    assert_eq!(info.buildings[1].id, DataId(0x0100_0BC3), "the cottage");
    assert_eq!(
        owned[1],
        (0x0111..=0x0115)
            .map(|i| CellId(0xA9B4_0000 | i))
            .collect::<Vec<_>>()
    );

    let older = older_world();
    let later = older
        .interior_files(ContainerEra::Tod)
        .expect("the later cell file is beside the older world");
    let (mut identity, _) = ObjectIdentity::build(older.portal(), later.portal());
    let census = identity.build_rooms(older.cell(), later.cell(), older.portal(), later.portal());
    assert_eq!(
        (census.cells, census.same),
        (455_641, 447_201),
        "{census:?}"
    );
}

/// The end-of-retail body the server sends a new Aluvian man at Holtburg in his starter clothes:
/// its 17 parts as the description leaves them, each carrying the description's changes for it
/// and the object's colours. The hair colour `0x04001FC5` is a palette the February 2005 files
/// do not hold.
fn end_of_retail_starter_body() -> Vec<PhysicsPart> {
    let models: [u32; 17] = [
        0x0100_120B,
        0x0100_120C,
        0x0100_122A,
        0x0100_1211,
        0x0100_120E,
        0x0100_11FB,
        0x0100_1228,
        0x0100_1210,
        0x0100_120F,
        0x0100_0054,
        0x0100_122B,
        0x0100_11F4,
        0x0100_0058,
        0x0100_122C,
        0x0100_11F5,
        0x0100_005B,
        0x0100_481D,
    ];
    let changes: [(usize, u32, u32); 26] = [
        (16, 0x0500_0098, 0x0500_10B7),
        (16, 0x0500_024C, 0x0500_10FF),
        (16, 0x0500_02F5, 0x0500_117A),
        (16, 0x0500_025C, 0x0500_11CD),
        (5, 0x0500_03D8, 0x0500_00A1),
        (1, 0x0500_03D8, 0x0500_00A1),
        (9, 0x0500_03D5, 0x0500_025F),
        (9, 0x0500_03D4, 0x0500_025E),
        (0, 0x0500_0BB0, 0x0500_025D),
        (0, 0x0500_0CBE, 0x0500_0CEA),
        (10, 0x0500_02CC, 0x0500_02BE),
        (13, 0x0500_02CC, 0x0500_02BE),
        (11, 0x0500_02C4, 0x0500_140D),
        (14, 0x0500_02C4, 0x0500_140D),
        (2, 0x0500_03DA, 0x0500_03CB),
        (6, 0x0500_03DA, 0x0500_03CB),
        (3, 0x0500_0CC0, 0x0500_03CE),
        (7, 0x0500_0CC0, 0x0500_03CE),
        (4, 0x0500_03DC, 0x0500_03CE),
        (8, 0x0500_03DC, 0x0500_03CE),
        (0, 0x0500_0BB0, 0x0500_0BB0),
        (0, 0x0500_0CBE, 0x0500_0CBE),
        (16, 0x0500_0098, 0x0500_10B7),
        (16, 0x0500_0098, 0x0500_10B7),
        (16, 0x0500_0098, 0x0500_10B7),
        (16, 0x0500_0098, 0x0500_10B7),
    ];
    let colours: [(u32, u32, u32); 8] = [
        (0x0400_1FC5, 192, 64),
        (0x0400_02BA, 0, 192),
        (0x0400_04B0, 256, 64),
        (0x0400_05DD, 512, 64),
        (0x0400_0696, 576, 64),
        (0x0400_05D7, 320, 192),
        (0x0400_044C, 736, 32),
        (0x0400_05BE, 1280, 64),
    ];
    models
        .iter()
        .enumerate()
        .map(|(i, &m)| {
            let mut p = PhysicsPart::new(DataId(m));
            let mut ov = SurfaceOverrides {
                shift_palette: Some(DataId(0x0400_007E)),
                subpalettes: colours
                    .iter()
                    .map(|&(c, offset, length)| PaletteRange {
                        palette_set: DataId(c),
                        offset,
                        length,
                    })
                    .collect(),
                ..SurfaceOverrides::default()
            };
            for &(at, o, n) in &changes {
                let m = (DataId(o), DataId(n));
                if at == i && !ov.texture_maps.contains(&m) {
                    ov.texture_maps.push(m);
                }
            }
            p.surface_overrides = Some(ov);
            p
        })
        .collect()
}

fn palettes(p: &PhysicsPart) -> Vec<u32> {
    p.surface_overrides
        .as_ref()
        .map(|s| s.subpalettes.iter().map(|r| r.palette_set.0).collect())
        .unwrap_or_default()
}

/// Behaviour: rendering.objects.a-colour-the-other-era-lacks-is-that-eras-colour-from-the-same-place
/// A new end-of-retail character's hair colour is `0x04001FC5`, the eighth of the thirteen shades
/// of the second hair colour (`0x0F000B51`) the Aluvian table offers; the February 2005 files hold
/// neither, and the colour applies to every part of the body. With the older look it is that
/// era's second Aluvian hair colour (`0x0F000017`) at the same share of the way along, so the
/// whole body takes the older look but the head, whose end-of-retail hair style (the 46th) the
/// older table lacks.
#[test]
fn a_new_end_of_retail_body_takes_the_older_look_with_the_older_hair_colour() {
    let world = end_of_retail_world();
    let (look, identity) = look_of(&world, ContainerEra::PreTod);
    assert!(!look.portal().contains(DataId(0x0400_1FC5)));
    let set = {
        let id = DataId(0x0F00_0017);
        let bytes = look
            .read_typed(DbType::PalSet, id)
            .expect("the older hair colour");
        PaletteSet::decode_payload_in(look.era_of(id), id, &bytes).expect("decodes")
    };
    let hair = identity
        .palette(DataId(0x0400_1FC5))
        .expect("the hair colour translates");
    assert_eq!(set.palette_ids.len(), 8);
    assert_eq!(
        hair, set.palette_ids[4],
        "the eighth of thirteen is the fifth of eight"
    );
    let drawn = parts_for_look(
        &world,
        &look,
        &identity,
        None,
        &end_of_retail_starter_body(),
    );
    for (i, d) in drawn.iter().enumerate().take(16) {
        let d = d
            .as_ref()
            .unwrap_or_else(|| panic!("part {i} takes the older look"));
        assert_eq!(
            palettes(d)[0],
            hair.0,
            "part {i} wears the older hair colour"
        );
    }
    assert!(drawn[16].is_none(), "the head keeps the world's hair");
}

/// Behaviour: rendering.objects.a-head-draws-the-other-eras-head-for-the-same-hair-style
/// The second hair style of the end-of-retail man's list is the head `0x0100481B`; the February
/// 2005 list's second is `0x010004A7`, and neither is bald. With the older look the head is the
/// older head with that style's own picture and the same face; with the later look, the other
/// way round. A style the other list has in another place, bald where the other is not (the
/// end-of-retail shaved head `0x010047F6`, first in its list; the February 2005 list starts with
/// hair), keeps the world's head.
#[test]
fn a_head_draws_the_other_eras_head_for_the_same_hair_style() {
    let face = [
        (0x0500_024C, 0x0500_1110),
        (0x0500_02F5, 0x0500_117A),
        (0x0500_025C, 0x0500_11CD),
    ];
    let with_face = |head: u32, style: (u32, u32)| {
        let mut m = vec![style];
        m.extend(face);
        part(head, &m)
    };
    let mut body: Vec<PhysicsPart> = (0..17)
        .map(|_| PhysicsPart::new(DataId(0x0100_01EC)))
        .collect();

    let world = end_of_retail_world();
    let (look, identity) = look_of(&world, ContainerEra::PreTod);
    body[16] = with_face(0x0100_481B, (0x0500_0098, 0x0500_11FD));
    let drawn = parts_for_look(&world, &look, &identity, None, &body);
    let head = drawn[16].as_ref().expect("the second style translates");
    assert_eq!(head.gfxobj_id, DataId(0x0100_04A7));
    let mut want = vec![(0x0500_0098, 0x0500_11FD)];
    want.extend(face);
    assert_eq!(maps(head), want);
    body[16] = with_face(0x0100_47F6, (0x0500_0098, 0x0500_10B7));
    let drawn = parts_for_look(&world, &look, &identity, None, &body);
    assert!(drawn[16].is_none(), "the shaved head keeps the world's");

    let world = older_world();
    let (look, identity) = look_of(&world, ContainerEra::Tod);
    body[16] = with_face(0x0100_04A7, (0x0500_0098, 0x0500_11FD));
    let drawn = parts_for_look(&world, &look, &identity, None, &body);
    let head = drawn[16]
        .as_ref()
        .expect("the second style translates back");
    assert_eq!(head.gfxobj_id, DataId(0x0100_481B));
}

/// Behaviour: rendering.objects.a-remodelled-creature-draws-the-other-eras-remodel
/// The skeleton's setup `0x02000059` was remodelled for the later files: all 17 parts are new
/// models, each resting within 2 cm of where the older part of its index rests, so the world's
/// motion places the other era's parts. The green deru `0x020002D9` is one part in February 2005
/// and two later; with the older look the later world's deru draws the one older part where its
/// first part stands and nothing for the second, and with the later look the older world's deru,
/// which has no frame for a second part, keeps the world's. A bone pile the eras built from one
/// part and from seventeen parts at other places (`0x020003CC`) keeps the world's either way, and
/// a description that changed a part keeps the part-by-part rules.
#[test]
fn a_remodelled_creature_draws_the_other_eras_remodel_where_it_rests_as_the_worlds_does() {
    let world = end_of_retail_world();
    let (look, identity) = look_of(&world, ContainerEra::PreTod);
    let body = |setup: u32| -> Vec<PhysicsPart> {
        parts_of(&world, setup)
            .into_iter()
            .map(|m| PhysicsPart::new(DataId(m)))
            .collect()
    };
    let skeleton = parts_of(&look, 0x0200_0059);
    let drawn = parts_for_look(
        &world,
        &look,
        &identity,
        Some(DataId(0x0200_0059)),
        &body(0x0200_0059),
    );
    let drawn: Vec<u32> = drawn
        .iter()
        .map(|p| {
            p.as_ref()
                .expect("every part from the older files")
                .gfxobj_id
                .0
        })
        .collect();
    assert_eq!(drawn, skeleton);

    let deru = parts_for_look(
        &world,
        &look,
        &identity,
        Some(DataId(0x0200_02D9)),
        &body(0x0200_02D9),
    );
    assert_eq!(deru.len(), 2);
    assert_eq!(
        deru[0].as_ref().map(|p| p.gfxobj_id),
        Some(DataId(parts_of(&look, 0x0200_02D9)[0]))
    );
    assert_eq!(
        deru[1].as_ref().map(|p| p.gfxobj_id),
        Some(DataId(0)),
        "nothing drawn"
    );

    assert!(identity.remodel(DataId(0x0200_03CC)).is_none());
    let mut dressed = body(0x0200_0059);
    dressed[0] = PhysicsPart::new(DataId(0x0100_0AAA));
    let drawn = parts_for_look(
        &world,
        &look,
        &identity,
        Some(DataId(0x0200_0059)),
        &dressed,
    );
    assert!(
        drawn[1].is_none(),
        "a changed skeleton keeps the part-by-part rules"
    );

    let world = older_world();
    let (_, identity) = look_of(&world, ContainerEra::Tod);
    assert!(identity.remodel(DataId(0x0200_0059)).is_some());
    assert!(
        identity.remodel(DataId(0x0200_02D9)).is_none(),
        "one part cannot place two"
    );
    assert!(identity.remodel(DataId(0x0200_03CC)).is_none());
}
