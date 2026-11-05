//! The looks of every object the 2017 sessions create, through the object's own copy of the
//! description sent. Fixture: the end-of-retail install and the capture index.

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_primitives::DataId;
use auberean_world::look::{Atoms, Look, ObjectLook, PartLook};
use auberean_world::records::WorldRecords;

/// The messages read: objects created.
const CREATE: u32 = 0x10058;

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

/// Every object the 2017 sessions create whose description extends another or names part groups
/// puts together through its own copy of the description; 2,063 of the 160,633 gather their atoms
/// in another order than the description sent would (players' clothes and dyes among them, their
/// appearance tables sharing buckets), and none of those draws another template, modifier order,
/// mesh or opacity on any part group.
///
/// Behaviour: desktop.looks.every-recorded-look-draws-through-the-objects-own-copy
#[test]
fn every_recorded_look_draws_through_the_objects_own_copy() {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
    let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    let r = WorldRecords::new(portal, cell, open(&|n| *n == DatName::HighRes), ctx);
    let index = auberean_testkit::data::require_capture_index();
    let (mut creates, mut failed, mut reordered) = (0u64, 0u64, 0u64);
    let mut differ = Vec::new();
    auberean_testkit::captures::messages(&index, &[CREATE], &[2017], |m| {
        let Ok(ServerMessage::CreateObject(c)) = ServerMessage::decode(&m.raw) else {
            return;
        };
        creates += 1;
        let desc = auberean_scene::desc_of(&r, &c.visual);
        if desc.parent.is_none() && desc.groups.is_empty() {
            return;
        }
        let (Ok(sent), Ok(own)) = (Look::of(&r, &desc), ObjectLook::new(&desc).total(&r)) else {
            failed += 1;
            return;
        };
        let mut atoms_differ = false;
        for (k, _) in &sent.groups.entries {
            let a = sent.atoms(&r, *k).unwrap_or_default();
            let b = own.atoms(&r, *k).unwrap_or_default();
            atoms_differ |= a != b;
            if drawn(a) != drawn(b) {
                differ.push((m.session, c.id, *k));
            }
        }
        reordered += u64::from(atoms_differ);
    })
    .expect("the capture index reads");
    eprintln!("{creates} objects created, {reordered} gathering their atoms in another order");
    assert_eq!(creates, 160_633);
    assert_eq!(failed, 0);
    assert_eq!(reordered, 2_063);
    assert!(differ.is_empty(), "{:?}", &differ[..differ.len().min(8)]);
}

/// Every look of the data that puts a figure together makes a body to stand in the world, the
/// looks whose mode their motion description does not list among them (a town's crafting workshop,
/// `0x1F00031B`): an object without a body is drawn by nothing.
///
/// Behaviour: desktop.bodies.every-look-that-makes-a-figure-makes-a-body
#[test]
fn every_look_that_makes_a_figure_makes_a_body() {
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).expect("a portal file");
    let looks: Vec<u32> = portal
        .entries()
        .map(|e| e.id)
        .filter(|id| id >> 24 == 0x1F)
        .collect();
    let cell = open(&|n| *n == DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    let r = WorldRecords::new(portal, cell, open(&|n| *n == DatName::HighRes), ctx);
    let anims = auberean_animation::CachedRecords::new(&r);
    let physics = auberean_client_sdk::protocol::types::PhysicsDesc::default();
    let (mut figures, mut bodyless) = (0, Vec::new());
    for id in looks {
        let Ok(look) = Look::of_record(&anims, DataId(id)) else {
            continue;
        };
        let Some(figure) = auberean_animation::body::Body::from_look(&anims, &look) else {
            continue;
        };
        figures += 1;
        if auberean_scene::body_config(&r, &look, &figure, &physics).is_none() {
            bodyless.push(id);
        }
    }
    assert!(figures > 6_000, "{figures}");
    assert!(bodyless.is_empty(), "{bodyless:08X?}");
}
