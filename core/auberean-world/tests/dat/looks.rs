//! Looks put together from the September 2013 install: a person whose description sets one
//! appearance value, and a player as the 2017 server described him, in his clothes.

use auberean_primitives::{DataId, Vec3};
use auberean_world::look::{
    Atoms, Desc, Group, Keyed, Look, LookRecords, ObjectLook, PartLook, GLOBAL_KEY,
};
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;

use crate::common::need_world;

fn keys(entries: &[(u32, f32)]) -> Keyed<f32> {
    Keyed::stored(47, entries.to_vec())
}

/// Each group of the tree, with each of its setup's meshes as the look leaves it.
fn meshes(w: &WorldRecords, look: &Look) -> Vec<(u32, Vec<(u32, u32)>)> {
    look.tree()
        .iter()
        .map(|g| {
            let l = look
                .atoms(w, g.key)
                .map(PartLook::from_atoms)
                .unwrap_or_default();
            let m = w
                .setup(g.setup)
                .map(|s| {
                    s.meshes
                        .iter()
                        .map(|m| (m.0, l.mesh(*m, &mut |id| w.mesh_kind(id)).0))
                        .collect()
                })
                .unwrap_or_default();
            (g.key, m)
        })
        .collect()
}

/// The Shelter Guard's description has no parent: one part group and one appearance value, 0.5 of
/// its table's key 2. That key's change sets stand at 0, 0.5 and 0.75: the value picks the set at
/// 0.5 (a set exactly at the value counts), whose one change is a material modifier on every mesh
/// for one material kind. No mesh is swapped.
///
/// Behaviour: world.look.a-value-picks-the-change-set-it-reaches
#[test]
fn a_value_picks_the_change_set_it_reaches() {
    let w = need_world!();
    let look = Look::of_record(&w.records, DataId(0x1F00_0D74)).expect("the Shelter Guard");
    let root = look.root_group().expect("a root group");
    assert_eq!(look.tree().len(), 1, "one part group");
    let atoms = look.atoms(&w.records, root.key).expect("his tables exist");
    assert!(
        atoms.meshes.is_empty(),
        "no mesh swapped: {:?}",
        atoms.meshes
    );
    let table = w
        .records
        .appearance(DataId(0x2000_008C))
        .expect("his table");
    let sets = &table.keys.iter().find(|(k, _)| *k == 2).expect("key 2").1;
    let at: Vec<f32> = sets.iter().map(|s| s.intensity).collect();
    assert_eq!(at, vec![0.0, 0.5, 0.75], "the table's steps");
    let want: Vec<(DataId, u32, u32)> = sets[1]
        .modifiers
        .iter()
        .map(|m| (m.modifier, m.mesh_kind, m.material_kind))
        .collect();
    let got: Vec<(DataId, u32, u32)> = atoms
        .materials
        .iter()
        .map(|m| (m.modifier, m.mesh_kind, m.material_kind))
        .collect();
    assert_eq!(got, want, "the set at 0.5");
    assert_eq!(got, vec![(DataId(0x3000_05B4), 1, 0x8100_0009)]);
}

/// A human man as a 2017 server sent him (`CreateObject` `0x10058`, January 2017): the human body as
/// the parent, his build and height as the scale, and per appearance table his values: skin, hair
/// and beard, markings, ranger pants and breastplate and quilted gloves worn, a green skin, a helmet
/// worn at a dye. Put together: the human body's tree (the body, the head under it, a third group
/// under the body) and a helmet group the helmet's table hangs from the head; each worn garment
/// swaps its meshes in by mesh kind (the body's four meshes, the head's hair and beard, the
/// helmet's); the skin, hair, markings and the helmet's dye change materials, the helmet's with a
/// replacement material too.
///
/// Behaviour: world.look.a-player-in-his-clothes-puts-together-as-sent
#[test]
fn a_player_in_his_clothes_puts_together_as_sent() {
    let w = need_world!();
    let mut global = Group {
        key: GLOBAL_KEY,
        ..Group::default()
    };
    for (apr, entries) in [
        (0x2000_004E, &[(0x10, 1.0)][..]),
        (0x2000_0027, &[(0x10, 1.0)][..]),
        (0x2000_0029, &[(0x10, 1.0)][..]),
        (0x2000_000C, &[(0x1, 0.0)][..]),
        (0x2000_000D, &[(0x3, 0.55), (0xB, 0.55), (0xC, 0.0)][..]),
        (0x2000_000E, &[(0x4100_001C, 0.01)][..]),
        (0x2000_00FA, &[(0x10, 1.0)][..]),
        (0x2000_02A7, &[(0x2, 0.52), (0x10, 1.0)][..]),
    ] {
        global.appearance.entries.push((apr, keys(entries)));
    }
    let desc = Desc {
        parent: DataId(0x1F00_0023),
        scale: Vec3::new(0.873_6, 0.873_6, 0.96),
        child_scale: 1.0,
        particle_scale: 1.0,
        global,
        ..Desc::default()
    };
    let look = Look::of(&w.records, &desc).expect("he puts together");
    let got = meshes(&w.records, &look);
    assert_eq!(
        got,
        vec![
            (
                0x8100_0001,
                vec![
                    (0x0600_0039, 0x0600_0380),
                    (0x0600_003A, 0x0600_01A7),
                    (0x0600_003B, 0x0600_01A9),
                    (0x0600_003C, 0x0600_00C3),
                ]
            ),
            (
                0x8100_0002,
                vec![(0x0600_003D, 0x0600_00F8), (0x0600_00E1, 0x0600_07BA)]
            ),
            (0x8100_000B, vec![(0x0600_07F1, 0x0600_0A16)]),
            (0x8100_0009, vec![(0x0600_03D9, 0x0600_03D9)]),
        ],
        "each group's meshes, as the garments swap them"
    );
    // The breastplate's own change swaps the mesh of its kind on the body.
    let plate = w
        .records
        .appearance(DataId(0x2000_0029))
        .expect("the breastplate");
    let worn = &plate.keys.iter().find(|(k, _)| *k == 0x10).expect("worn").1[0];
    let swap = worn.meshes.first().expect("the breastplate swaps a mesh");
    let body = &got[0].1;
    assert!(
        body.iter().any(|(_, now)| *now == swap.mesh.0),
        "the breastplate's mesh {:#x} is on the body: {body:x?}",
        swap.mesh.0
    );
    // His skin is the skin table's first step (value 0).
    let skin = w.records.appearance(DataId(0x2000_000C)).expect("skin");
    let first = &skin.keys.iter().find(|(k, _)| *k == 1).expect("key 1").1[0];
    let atoms = look
        .atoms(&w.records, 0x8100_0001)
        .expect("the body's atoms");
    for m in &first.modifiers {
        assert!(
            atoms
                .materials
                .iter()
                .any(|a| a.modifier == m.modifier && a.material_kind == m.material_kind),
            "skin modifier {:#x} on kind {:#x}",
            m.modifier.0,
            m.material_kind
        );
    }
    assert!(
        atoms
            .materials
            .iter()
            .any(|a| !a.template.is_none() && !a.modifier.is_none()),
        "the helmet's dye replaces a material and modifies it"
    );
    assert_eq!(look.scale, Vec3::new(0.873_6, 0.873_6, 0.96), "his scale");
}

/// What a look draws on one part group: per mesh kind and material kind its atoms reach, the
/// template and the modifiers in order, the mesh a part of that kind ends with, and the opacity.
fn drawn(atoms: Atoms) -> Vec<(u32, u32, Option<u32>, Vec<u32>)> {
    let mut mesh_kinds: Vec<u32> = atoms
        .materials
        .iter()
        .map(|m| m.mesh_kind)
        .chain(atoms.meshes.iter().map(|m| m.mesh_kind))
        .collect();
    let mut material_kinds: Vec<u32> = atoms.materials.iter().map(|m| m.material_kind).collect();
    mesh_kinds.sort_unstable();
    mesh_kinds.dedup();
    material_kinds.sort_unstable();
    material_kinds.dedup();
    let look = PartLook::from_atoms(atoms);
    let mut out = Vec::new();
    for mk in &mesh_kinds {
        for tk in &material_kinds {
            let (t, m) = look.material(*mk, *tk, &mut |_| true);
            out.push((*mk, *tk, t.map(|x| x.0), m.iter().map(|x| x.0).collect()));
        }
        let mesh = look.mesh(DataId(1), &mut |_| Some(*mk));
        out.push((*mk, 0, Some(mesh.0), Vec::new()));
    }
    out.push((0, 0, look.opacity.map(f32::to_bits), Vec::new()));
    out
}

/// An object's own description is a copy of the one it was given, and a copy turns each table's
/// buckets around: of the 6,615 stored descriptions, 87 put together with their appearance tables
/// in another order through a copy, and 71 of those gather their atoms in another order. All but
/// two draw the same templates, modifiers, meshes and opacity on every part group as the stored
/// description put together directly; `0x1F000DDF` and `0x1F001051` apply two modifiers of one
/// material in the other order.
///
/// Behaviour: world.look.an-objects-own-copy-draws-as-the-description-it-was-given
#[test]
fn an_objects_own_copy_draws_as_the_description_it_was_given() {
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::DatFile;
    let w = need_world!();
    let dir = auberean_dat::testing::require_target_install();
    let portal = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .find(|(n, _)| *n == DatName::Portal)
        .map(|(_, p)| DatFile::open(&p).expect("the portal file opens"))
        .expect("a portal file");
    let (mut seen, mut reordered, mut atoms_reordered) = (0u32, 0u32, 0u32);
    let mut differ = Vec::new();
    for e in portal.entries_in(0x1F00_0000, 0x1FFF_FFFF) {
        let Some(v) = w.records.visual(DataId(e.id)) else {
            continue;
        };
        seen += 1;
        let desc = Desc::from_stored(&v);
        let direct = Look::of(&w.records, &desc).expect("the description puts together");
        let own = ObjectLook::new(&desc)
            .total(&w.records)
            .expect("its copy puts together");
        if direct == own {
            continue;
        }
        reordered += 1;
        let mut atoms_differ = false;
        for (k, _) in &direct.groups.entries {
            let a = direct.atoms(&w.records, *k).unwrap_or_default();
            let b = own.atoms(&w.records, *k).unwrap_or_default();
            atoms_differ |= a != b;
            if drawn(a) != drawn(b) && !differ.contains(&e.id) {
                differ.push(e.id);
            }
        }
        atoms_reordered += u32::from(atoms_differ);
    }
    assert_eq!(seen, 6_615);
    assert_eq!((reordered, atoms_reordered), (87, 71));
    assert_eq!(differ, vec![0x1F00_0DDF, 0x1F00_1051]);
}
