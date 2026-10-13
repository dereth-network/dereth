//! A body's description recolors only the parts painted against its palette: an Empyrean in the
//! Undead Guise or the Armored Undead Guise keeps the guise's own colors on every limb the guise
//! swaps in, while the parts left of the body take the skin, hair and eye ranges; a bare body
//! takes them everywhere.
//! Fixture: the shipped retail DAT records and the description the server sends for an Empyrean
//! man in each guise.

use std::sync::Arc;

use dereth_animation::data::AnimAssets;
use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, PartArray, TextureMapChange};
use dereth_animation::seq::Sequence;
use dereth_primitives::DataId;
use dereth_world_data::anim_assets::DatAnimAssets;

/// The Empyrean man's body.
const EMPYREAN_MALE: DataId = DataId(0x0200_1A16);
/// The palette every body's description names, and the skin, hair and eye ranges laid over it.
const BODY_PALETTE: DataId = DataId(0x0400_007E);
/// The palettes the Undead Guise's and the Armored Undead Guise's limbs are painted against.
const UNDEAD_GUISE_PALETTE: DataId = DataId(0x0400_0742);
const ARMORED_GUISE_PALETTE: DataId = DataId(0x0400_16C4);
/// The part every body draws its head on.
const HEAD: u32 = 16;

fn assets() -> DatAnimAssets {
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    DatAnimAssets::new(Arc::new(store))
}

/// The skin, hair and eye ranges in the server's order, in wire units.
fn ranges() -> Vec<PaletteRange> {
    [
        (0x0400_1FB7, 0x18, 8),
        (0x0400_1F9E, 0, 0x18),
        (0x0400_1F37, 0x20, 8),
    ]
    .into_iter()
    .map(|(p, offset, length)| PaletteRange {
        palette_set: DataId(p),
        offset,
        length,
    })
    .collect()
}

/// The description the server sends for an Empyrean man wearing a guise whose clothing table
/// gives the body `limbs`: the face on the head, the guise's limbs, and the null model on every
/// part past the body.
fn wearing(limbs: &[(u32, u32)]) -> ObjDesc {
    let mut part_changes = vec![AnimPartChange {
        part_index: HEAD,
        part_id: DataId(0x0100_47F6),
    }];
    part_changes.extend(limbs.iter().map(|&(part_index, id)| AnimPartChange {
        part_index,
        part_id: DataId(id),
    }));
    part_changes.extend((17..34).map(|part_index| AnimPartChange {
        part_index,
        part_id: DataId(0x0100_01EC),
    }));
    ObjDesc {
        part_changes,
        texture_changes: [
            (0x0500_0098, 0x0500_10B7),
            (0x0500_024C, 0x0500_10FF),
            (0x0500_02F5, 0x0500_1156),
            (0x0500_025C, 0x0500_118F),
        ]
        .into_iter()
        .map(|(old, new)| TextureMapChange {
            part_index: HEAD,
            old_texture: DataId(old),
            new_texture: DataId(new),
        })
        .collect(),
        palette_id: BODY_PALETTE,
        subpalettes: ranges(),
    }
}

/// The Undead Guise's limbs on an Empyrean man (clothing table `0x100003F9`).
const UNDEAD_GUISE: [(u32, u32); 16] = [
    (0, 0x0100_07E1),
    (1, 0x0100_07E8),
    (2, 0x0100_07EA),
    (3, 0x0100_07E3),
    (4, 0x0100_07EC),
    (5, 0x0100_07E9),
    (6, 0x0100_07EB),
    (7, 0x0100_07E4),
    (8, 0x0100_07ED),
    (9, 0x0100_07E7),
    (10, 0x0100_07E2),
    (11, 0x0100_07DE),
    (13, 0x0100_07E0),
    (14, 0x0100_07DF),
    (15, 0x0100_07E5),
    (12, 0x0100_07E6),
];

/// The Armored Undead Guise's limbs and helm on an Empyrean man (clothing table `0x10000580`).
const ARMORED_UNDEAD_GUISE: [(u32, u32); 17] = [
    (0, 0x0100_30C2),
    (1, 0x0100_30CC),
    (2, 0x0100_30BF),
    (3, 0x0100_30C6),
    (4, 0x0100_30CA),
    (5, 0x0100_30CD),
    (6, 0x0100_30C0),
    (7, 0x0100_30C3),
    (8, 0x0100_30CB),
    (9, 0x0100_30C8),
    (10, 0x0100_30BE),
    (11, 0x0100_30C7),
    (13, 0x0100_30C1),
    (14, 0x0100_30C9),
    (15, 0x0100_30C5),
    (12, 0x0100_30C4),
    (HEAD, 0x0100_30CE),
];

/// The Empyrean man's body dressed by `od` as a newly created object is.
fn dressed(a: &DatAnimAssets, od: &ObjDesc) -> PartArray {
    let setup = a.setup(EMPYREAN_MALE).expect("the Empyrean man's setup");
    let mut seq = Sequence::new();
    let mut pa = PartArray::create_setup(setup, true, &mut seq, a).expect("the body is built");
    assert!(
        pa.do_obj_desc_changes_from_default_with(od, a),
        "the description applies"
    );
    pa
}

fn shift(pa: &PartArray, part: u32) -> Option<DataId> {
    pa.parts[part as usize]
        .surface_overrides
        .as_ref()
        .and_then(|s| s.shift_palette)
}

/// Behaviour: objects.appearance.a-palette-recolors-only-the-parts-painted-against-it
///
/// Every limb the Undead Guise swaps in is painted against the guise's own palette, not the
/// body's, so the body's skin, hair and eye ranges leave it alone and its pictures read their own
/// colors. The head the guise leaves is the body's and takes all three ranges. Recoloring the
/// limbs would index their pictures through the body's palette.
#[test]
fn an_empyrean_in_the_undead_guise_keeps_the_guises_colors_and_its_own_skin() {
    let a = assets();
    let pa = dressed(&a, &wearing(&UNDEAD_GUISE));
    for &(part, id) in &UNDEAD_GUISE {
        let p = &pa.parts[part as usize];
        assert_eq!(p.gfxobj_id, DataId(id), "part {part} wears the guise");
        assert_eq!(p.original_palette_id, UNDEAD_GUISE_PALETTE, "part {part}");
        assert_eq!(
            shift(&pa, part),
            None,
            "part {part} keeps the guise's colors"
        );
    }
    let head = &pa.parts[HEAD as usize];
    assert_eq!(head.original_palette_id, BODY_PALETTE);
    assert_eq!(
        shift(&pa, HEAD),
        Some(BODY_PALETTE),
        "the head takes the skin"
    );
    assert_eq!(
        head.surface_overrides
            .as_ref()
            .expect("recolored")
            .subpalettes,
        ranges()
    );
}

/// The Armored Undead Guise replaces the head too, and every part it swaps in, helm included,
/// keeps the guise's palette.
#[test]
fn an_empyrean_in_the_armored_undead_guise_keeps_the_guises_colors_everywhere() {
    let a = assets();
    let pa = dressed(&a, &wearing(&ARMORED_UNDEAD_GUISE));
    for &(part, id) in &ARMORED_UNDEAD_GUISE {
        let p = &pa.parts[part as usize];
        assert_eq!(p.gfxobj_id, DataId(id), "part {part} wears the guise");
        assert_eq!(p.original_palette_id, ARMORED_GUISE_PALETTE, "part {part}");
        assert_eq!(
            shift(&pa, part),
            None,
            "part {part} keeps the guise's colors"
        );
    }
}

/// With nothing worn every part of the body is painted against the body's palette and takes the
/// skin, hair and eye ranges.
#[test]
fn a_bare_empyrean_takes_the_skin_on_every_part_of_the_body() {
    let a = assets();
    let pa = dressed(&a, &wearing(&[]));
    for part in 0..=HEAD {
        assert_eq!(
            pa.parts[part as usize].original_palette_id, BODY_PALETTE,
            "part {part}"
        );
        assert_eq!(shift(&pa, part), Some(BODY_PALETTE), "part {part}");
    }
}
