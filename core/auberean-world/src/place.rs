//! Where a placed object stands and what it is made of: a landblock's or a cell's entity, or a sky
//! object, resolved through its visual descriptions and setups to meshes, each with the transform
//! that puts it in the world and what its look does to it (the mesh swapped in, the materials
//! changed).

use std::sync::{Arc, RwLock};

use auberean_assets::entity::Entity;
use auberean_assets::landblock::Cell;
use auberean_assets::property::PropertyValue;
use auberean_assets::setup::{BoneTransform, DetailDistances, Setup};
use auberean_primitives::{DataId, Vec3};

use crate::look::{appearance_atoms, Atoms, Look, LookRecords, PartLook};
use crate::math::Mat4;
use crate::records::WorldRecords;

/// The records placement reads: besides a look's, setups, entities and meshes' kinds and weights.
pub trait PlaceRecords: LookRecords {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>>;
    fn entity(&self, id: DataId) -> Option<Arc<Entity>>;
    /// A mesh's kind (which body part it is), when the mesh is in the data.
    fn mesh_kind(&self, _id: DataId) -> Option<u32> {
        None
    }
    /// Whether a mesh's vertices are weighted to bones (its vertex format names matrices); false
    /// when the mesh is not in the data.
    fn mesh_weighted(&self, _id: DataId) -> bool {
        false
    }
}

/// A setup as loaded. Records that keep their setups hand the same one to everything made from a
/// setup, so a change to it (an object's step-down height, its spheres) reaches every object made
/// from that setup, and whatever reads the setup afterwards, for as long as the records keep it.
#[derive(Debug)]
pub struct LoadedSetup(RwLock<Arc<Setup>>);

impl LoadedSetup {
    #[must_use]
    pub fn new(setup: Arc<Setup>) -> Self {
        Self(RwLock::new(setup))
    }

    /// The setup as it is now.
    #[must_use]
    pub fn get(&self) -> Arc<Setup> {
        match self.0.read() {
            Ok(s) => Arc::clone(&s),
            Err(e) => Arc::clone(&e.into_inner()),
        }
    }

    /// Changes the setup for everything that shares it.
    pub fn change(&self, f: impl FnOnce(&mut Setup)) {
        let mut s = match self.0.write() {
            Ok(s) => s,
            Err(e) => e.into_inner(),
        };
        f(Arc::make_mut(&mut s));
    }
}

/// One mesh of a placed object and where it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub mesh: DataId,
    /// Mesh space to the object's frame of reference (the landblock's, for a placed entity).
    pub transform: Mat4,
    /// The distances the part's setup gives for choosing a level of detail, one set for each
    /// model detail setting (low, medium, high).
    pub detail: Option<[DetailDistances; 3]>,
    /// What the object's look does to the part's materials, when it changes them.
    pub look: Option<Arc<PartLook>>,
    /// Where the part takes its pose from, when its object plays.
    pub played: Option<PlayedBy>,
}

/// The body a part of a placed object takes its pose from: an object placed by the world whose look
/// has a behavior table plays it through one body kept for the look, which every object showing
/// that look shares, so they all move together. `Part::transform` is the part where it stands
/// when nothing poses it.
// D2-06554, D2-05768
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayedBy {
    /// The look the body is made from.
    pub look: DataId,
    /// The part group of the look the part belongs to.
    pub group: u32,
    /// The mesh's place in the group's setup, whose bone carries an unweighted mesh.
    pub mesh: usize,
    /// The object's own frame, with its scale, which carries the pose.
    pub object: Mat4,
}

/// The entity kinds a landblock or cell makes into drawn objects; the others only reserve room.
pub const DRAWN_KINDS: [u32; 2] = [0x4000_0001, 0x4000_0007];

/// The record kind of a data id, by its top byte.
fn kind(id: DataId) -> u8 {
    (id.0 >> 24) as u8
}

/// A bone's or connection point's transform as a matrix, with or without its own scale.
fn fill(t: &BoneTransform, with_scale: bool) -> Mat4 {
    let s = if with_scale {
        t.scale
    } else {
        Vec3::new(1.0, 1.0, 1.0)
    };
    Mat4::from_parts(t.offset, t.rotation, s)
}

/// The parts of object `id` (a setup, a visual description or an entity record) placed by `world`
/// with extra `scale`. Returns nothing for an id it cannot resolve.
// D2-06362, D2-06549, D2-06607, D2-06604
#[must_use]
pub fn parts_of(records: &dyn PlaceRecords, id: DataId, world: &Mat4, scale: Vec3) -> Vec<Part> {
    match kind(id) {
        0x47 => {
            let Some(e) = records.entity(id) else {
                return Vec::new();
            };
            let s = e.scale.unwrap_or(Vec3::new(1.0, 1.0, 1.0));
            let inner = e.object.map_or(DataId::NONE, DataId);
            parts_of(
                records,
                inner,
                world,
                Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z),
            )
        }
        0x04 => {
            let mut look = Look::default();
            let _ = look.add_group(&crate::look::Group {
                key: 1,
                setup: id,
                ..crate::look::Group::default()
            });
            draw_tree(records, &look, world, scale, false, None)
        }
        0x1F => {
            let Ok(look) = Look::of_record(records, id) else {
                return Vec::new();
            };
            let s = Vec3::new(
                scale.x * look.scale.x,
                scale.y * look.scale.y,
                scale.z * look.scale.z,
            );
            let played = (!look.behavior_table.is_none()).then_some(id);
            draw_tree(records, &look, world, s, true, played)
        }
        _ => Vec::new(),
    }
}

/// The parts of a look already built, placed by `world` with extra `scale` (the look's own scale
/// not applied), each mesh as the look swaps it.
#[must_use]
pub fn look_parts(records: &dyn PlaceRecords, look: &Look, world: &Mat4, scale: Vec3) -> Vec<Part> {
    draw_tree(records, look, world, scale, true, None)
}

/// The parts of a look's group tree from its root, each part's mesh as the look swaps it and its
/// materials' changes (`with_look` false: the setup's meshes as they are). A part follows the bone
/// of its own index, then its group's frame; but when any of the setup's own meshes is weighted to
/// bones, every part of that group takes the group's frame alone, the unweighted ones too.
// D2-06597, D2-06589, D2-06632
fn draw_tree(
    records: &dyn PlaceRecords,
    look: &Look,
    world: &Mat4,
    scale: Vec3,
    with_look: bool,
    played: Option<DataId>,
) -> Vec<Part> {
    let base = Mat4::from_parts(Vec3::ZERO, auberean_primitives::Quat::IDENTITY, scale).then(world);
    let mut out = Vec::new();
    let Some(root) = look.root else {
        return out;
    };
    let mut stack = vec![(root, Mat4::IDENTITY, 0usize)];
    while let Some((key, g_mat, depth)) = stack.pop() {
        if depth > 16 {
            continue;
        }
        let Some(group) = look.groups.get(key) else {
            continue;
        };
        let setup = records.setup(group.setup);
        let bones = setup.as_deref().map(posed_bones).unwrap_or_default();
        let part_look = if with_look {
            look.atoms(records, key)
                .map(PartLook::from_atoms)
                .filter(|l| *l != PartLook::default())
                .map(Arc::new)
        } else {
            None
        };
        if let Some(setup) = &setup {
            let detail = Some(setup.detail);
            let weighted = has_bone_weights(records, setup);
            for (i, mesh) in setup.meshes.iter().enumerate() {
                let bone = if weighted {
                    Mat4::IDENTITY
                } else {
                    bones.get(i).copied().unwrap_or(Mat4::IDENTITY)
                };
                let mesh = match &part_look {
                    Some(l) => l.mesh(*mesh, &mut |m| records.mesh_kind(m)),
                    None => *mesh,
                };
                out.push(Part {
                    mesh,
                    transform: bone.then(&g_mat).then(&base),
                    detail,
                    look: part_look
                        .clone()
                        .filter(|l| !l.materials.is_empty() || l.opacity.is_some()),
                    played: played.map(|look| PlayedBy {
                        look,
                        group: key,
                        mesh: i,
                        object: base,
                    }),
                });
            }
        }
        for child_key in group.children.iter().rev() {
            let Some(child) = look.groups.get(*child_key) else {
                continue;
            };
            let attach = setup.as_deref().and_then(|s| {
                let cp = s
                    .connection_points
                    .iter()
                    .find(|c| c.kind == child.connection)?;
                let bone = bones
                    .get(usize::try_from(cp.bone).ok()?)
                    .copied()
                    .unwrap_or(Mat4::IDENTITY);
                Some(fill(&cp.transform, true).then(&bone))
            });
            stack.push((
                *child_key,
                attach.unwrap_or(Mat4::IDENTITY).then(&g_mat),
                depth + 1,
            ));
        }
    }
    out
}

/// Whether any of a setup's own meshes (before a look swaps them) is weighted to bones.
// D2-06624
fn has_bone_weights(records: &dyn PlaceRecords, setup: &Setup) -> bool {
    setup.meshes.iter().any(|m| records.mesh_weighted(*m))
}

/// Each bone's transform in the setup's resting pose (pose kind 0, or the first pose): its own
/// offset and rotation, then its parent's.
fn posed_bones(setup: &Setup) -> Vec<Mat4> {
    let pose = setup
        .poses
        .iter()
        .find(|p| p.kind == 0)
        .or(setup.poses.first());
    let mut out: Vec<Mat4> = Vec::with_capacity(setup.bones.len());
    for (i, bone) in setup.bones.iter().enumerate() {
        let local = pose
            .and_then(|p| p.bones.get(i))
            .map_or(Mat4::IDENTITY, |t| fill(t, false));
        let parent = usize::try_from(bone.parent)
            .ok()
            .filter(|&p| p < i)
            .map(|p| out[p]);
        out.push(match parent {
            Some(p) => local.then(&p),
            None => local,
        });
    }
    out
}

/// The parts a landblock or cell entity puts in its landblock's frame, or nothing when the entity
/// is not one that is drawn.
// D2-03687
#[must_use]
pub fn entity_parts(records: &dyn PlaceRecords, e: &Entity) -> Vec<Part> {
    if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) {
        return Vec::new();
    }
    let Some(object) = e.object.map(DataId) else {
        return Vec::new();
    };
    let world = e.offset.as_ref().map_or(Mat4::IDENTITY, |f| {
        Mat4::from_parts(f.frame.origin, f.frame.rotation, Vec3::new(1.0, 1.0, 1.0))
    });
    let scale = e.scale.unwrap_or(Vec3::new(1.0, 1.0, 1.0));
    parts_of(records, object, &world, scale)
}

/// The property group a cell's own settings are in.
const CELL_GROUP: u32 = 0x4000_0010;
/// A cell's appearance: its key (an enumeration) and the key's strength.
const CELL_APPEARANCE_KEY: u32 = 0x4000_005D;
const CELL_APPEARANCE_STRENGTH: u32 = 0x4000_005E;
/// The appearance key that names none.
const NO_APPEARANCE: u32 = 0x4000_0001;
/// The enumeration value and mapping the cells' appearance table is found through.
const CELL_APPEARANCE_TABLE: (u32, u32) = (0x4000_0008, 0x4000_0002);

/// What an interior cell's appearance does to its mesh's materials, when it names one: the
/// change set its key and strength (0 unless set) pick in the cells' appearance table, the
/// material changes alone. The same mesh dressed as a grassy hall or a sandy canyon is one cell
/// mesh with two keys.
// D2-03528, D2-03606, D2-03605, D2-03558
#[must_use]
pub fn cell_look(records: &WorldRecords, cell: &Cell) -> Option<PartLook> {
    let mut key = 0u32;
    let mut strength = 0.0f32;
    let mut applied = None;
    let groups = cell.properties.iter().flat_map(|p| p.groups.iter());
    for g in groups.filter(|g| g.group == CELL_GROUP) {
        for p in &g.properties {
            let changed = match (p.id, &p.value) {
                (CELL_APPEARANCE_KEY, PropertyValue::Enum(k)) => {
                    let c = *k != key;
                    key = *k;
                    c
                }
                #[allow(clippy::float_cmp)]
                (CELL_APPEARANCE_STRENGTH, PropertyValue::Float(f)) => {
                    let c = *f != strength;
                    strength = *f;
                    c
                }
                _ => false,
            };
            if changed && key != NO_APPEARANCE {
                applied = Some((key, strength));
            }
        }
    }
    let (key, strength) = applied?;
    let table = records.data_id_by_enum(CELL_APPEARANCE_TABLE.0, CELL_APPEARANCE_TABLE.1);
    let mut atoms = Atoms::default();
    if !appearance_atoms(records, DataId(table), key, strength, &mut atoms, false) {
        return None;
    }
    let look = PartLook {
        materials: atoms.materials,
        ..PartLook::default()
    };
    (!look.materials.is_empty()).then_some(look)
}
