//! How an object's look is put together, on synthetic descriptions and appearance tables: the
//! change set a value picks, base tables first, the replacement rules, a description chain, the
//! part-group swaps and the meshes a part ends with.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::visual::{
    Appearance, AppearanceChanges, MeshChange, ModifierChange, PartGroup, PartGroupChange,
    VisualDesc,
};
use auberean_primitives::{DataId, Vec3};
use auberean_world::look::{
    Desc, Keyed, Look, LookError, LookRecords, MeshAtom, PartLook, ANY_KIND, GLOBAL_KEY, REMOVED,
};

#[derive(Default)]
struct Records {
    visuals: HashMap<u32, Arc<VisualDesc>>,
    tables: HashMap<u32, Arc<Appearance>>,
}

impl LookRecords for Records {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.visuals.get(&id.0).cloned()
    }
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.tables.get(&id.0).cloned()
    }
}

const ROOT: u32 = 0x8100_0001;
const HEAD: u32 = 0x8100_0002;
const BODY_KIND: u32 = 0x8100_0016;

fn modifier(priority: f32, id: u32, mesh_kind: u32, material_kind: u32) -> ModifierChange {
    ModifierChange {
        priority,
        template: DataId::NONE,
        modifier: DataId(id),
        mesh_kind,
        material_kind,
    }
}

fn set(intensity: f32) -> AppearanceChanges {
    AppearanceChanges {
        intensity,
        ..AppearanceChanges::default()
    }
}

fn group(key: u32, parent: Option<u32>, connection: Option<u32>, setup: u32) -> PartGroup {
    PartGroup {
        key: Some(key),
        parent_key: parent,
        connection_point: connection,
        setup: Some(DataId(setup)),
        ..PartGroup::default()
    }
}

/// A description whose one group is `ROOT` and whose whole-object values are `values`.
fn wearing(values: &[(u32, &[(u32, f32)])]) -> Desc {
    let mut d = Desc {
        scale: Vec3::new(1.0, 1.0, 1.0),
        child_scale: 1.0,
        particle_scale: 1.0,
        ..Desc::default()
    };
    d.global.key = GLOBAL_KEY;
    for (apr, keys) in values {
        d.global
            .appearance
            .entries
            .push((*apr, Keyed::stored(47, keys.to_vec())));
    }
    let mut root = auberean_world::look::Group::from_stored(&group(ROOT, None, None, 0x0400_0001));
    root.key = ROOT;
    d.groups.push(root);
    d
}

fn materials(r: &Records, d: &Desc) -> Vec<u32> {
    let look = Look::of(r, d).expect("a look");
    look.atoms(r, ROOT)
        .expect("atoms")
        .materials
        .iter()
        .map(|m| m.modifier.0)
        .collect()
}

/// A value picks the last change set whose intensity is not above it: 0.15 the set at 0.1, 0.2 the
/// set at 0.2; a value below the first set picks none, and a taken-off value (-1) applies nothing.
///
/// Behaviour: world.look.a-value-picks-the-last-change-set-not-above-it
#[test]
fn a_value_picks_the_last_change_set_not_above_it() {
    let mut r = Records::default();
    let sets = (0..3)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let mut s = set(i as f32 / 10.0);
            s.modifiers
                .push(modifier(1.0, 0x3000_0001 + i, ANY_KIND, BODY_KIND));
            s
        })
        .collect();
    r.tables.insert(
        0x2000_0001,
        Arc::new(Appearance {
            keys: vec![(1, sets)],
            ..Appearance::default()
        }),
    );
    let at = |v: f32| materials(&r, &wearing(&[(0x2000_0001, &[(1, v)])]));
    assert_eq!(at(0.15), vec![0x3000_0002]);
    assert_eq!(at(0.2), vec![0x3000_0003]);
    assert_eq!(at(1.0), vec![0x3000_0003]);
    assert_eq!(at(-0.5), Vec::<u32>::new(), "below the first set");
    assert_eq!(at(REMOVED), Vec::<u32>::new(), "taken off");
}

/// A table's base tables apply before its own changes: the own mesh change of the same kind at an
/// equal priority takes the base's place; at a lower priority the base's stays.
///
/// Behaviour: world.look.base-tables-apply-first
#[test]
fn base_tables_apply_first() {
    for (own_priority, want) in [(1.0, 0x0600_0002), (0.5, 0x0600_0001)] {
        let mut r = Records::default();
        let mut base = set(1.0);
        base.meshes.push(MeshChange {
            priority: 1.0,
            mesh_kind: BODY_KIND,
            mesh: DataId(0x0600_0001),
        });
        let mut own = set(1.0);
        own.meshes.push(MeshChange {
            priority: own_priority,
            mesh_kind: BODY_KIND,
            mesh: DataId(0x0600_0002),
        });
        r.tables.insert(
            0x2000_0002,
            Arc::new(Appearance {
                keys: vec![(0x10, vec![base])],
                ..Appearance::default()
            }),
        );
        r.tables.insert(
            0x2000_0003,
            Arc::new(Appearance {
                base: vec![0x2000_0002],
                keys: vec![(0x10, vec![own])],
                ..Appearance::default()
            }),
        );
        let look = Look::of(&r, &wearing(&[(0x2000_0003, &[(0x10, 1.0)])])).expect("a look");
        let meshes = look.atoms(&r, ROOT).expect("atoms").meshes;
        assert_eq!(
            meshes,
            vec![MeshAtom {
                priority: if want == 0x0600_0002 {
                    own_priority
                } else {
                    1.0
                },
                mesh_kind: BODY_KIND,
                mesh: DataId(want),
            }],
            "own priority {own_priority}"
        );
    }
}

/// Material changes: a higher priority replaces a lower one on the same material kind (for every
/// mesh kind, when it names every mesh kind), equal priorities both stay in order, and a change for
/// another material kind or a narrower mesh kind replaces nothing.
///
/// Behaviour: world.look.material-changes-replace-by-priority-on-their-kind
#[test]
fn material_changes_replace_by_priority_on_their_kind() {
    let mut r = Records::default();
    let one = |m: ModifierChange| {
        let mut s = set(0.0);
        s.modifiers.push(m);
        Arc::new(Appearance {
            keys: vec![(2, vec![s])],
            ..Appearance::default()
        })
    };
    r.tables.insert(
        0x2000_0010,
        one(modifier(1.0, 0x3000_0010, 0x8100_0003, BODY_KIND)),
    );
    r.tables.insert(
        0x2000_0011,
        one(modifier(1.0, 0x3000_0011, 0x8100_0003, BODY_KIND)),
    );
    r.tables.insert(
        0x2000_0012,
        one(modifier(2.0, 0x3000_0012, ANY_KIND, BODY_KIND)),
    );
    r.tables.insert(
        0x2000_0013,
        one(modifier(3.0, 0x3000_0013, 0x8100_0004, BODY_KIND)),
    );
    r.tables.insert(
        0x2000_0014,
        one(modifier(3.0, 0x3000_0014, ANY_KIND, 0x8100_0005)),
    );
    let v: &[(u32, f32)] = &[(2, 0.0)];
    assert_eq!(
        materials(&r, &wearing(&[(0x2000_0010, v), (0x2000_0011, v)])),
        vec![0x3000_0010, 0x3000_0011],
        "equal priorities stay, in order"
    );
    assert_eq!(
        materials(
            &r,
            &wearing(&[(0x2000_0010, v), (0x2000_0011, v), (0x2000_0012, v)])
        ),
        vec![0x3000_0012],
        "every mesh kind at a higher priority replaces both"
    );
    assert_eq!(
        materials(
            &r,
            &wearing(&[(0x2000_0012, v), (0x2000_0013, v), (0x2000_0014, v)])
        ),
        vec![0x3000_0012, 0x3000_0013, 0x3000_0014],
        "a narrower mesh kind or another material kind replaces nothing"
    );
    assert_eq!(
        materials(&r, &wearing(&[(0x2000_0012, v), (0x2000_0010, v)])),
        vec![0x3000_0012],
        "a lower priority is not added"
    );
}

/// A chain puts together root first: the nearest behavior table, the scales multiplied, the
/// groups from the first description that has them (a later description's setup for a group it
/// already has is not taken), the head hung from the root at its connection point, and the last
/// description's value for an appearance key over the earlier ones'.
///
/// Behaviour: world.look.a-description-chain-puts-together-root-first
#[test]
fn a_description_chain_puts_together_root_first() {
    let mut r = Records::default();
    let values = PartGroup {
        key: Some(GLOBAL_KEY),
        appearance_keys: Some(vec![(0x2000_0020, vec![(1, 0.3), (3, 0.5)])]),
        ..PartGroup::default()
    };
    r.visuals.insert(
        0x1F00_0001,
        Arc::new(VisualDesc {
            behavior_table: Some(DataId(0x4400_0001)),
            scale: Some(Vec3::new(2.0, 2.0, 2.0)),
            part_groups: Some(vec![
                (ROOT, group(ROOT, None, None, 0x0400_0001)),
                (
                    HEAD,
                    group(HEAD, Some(ROOT), Some(0x8100_0007), 0x0400_0002),
                ),
            ]),
            ..VisualDesc::default()
        }),
    );
    r.visuals.insert(
        0x1F00_0002,
        Arc::new(VisualDesc {
            parent: Some(DataId(0x1F00_0001)),
            behavior_table: Some(DataId(0x4400_0002)),
            scale: Some(Vec3::new(3.0, 3.0, 3.0)),
            global: Some(values),
            part_groups: Some(vec![(ROOT, group(ROOT, None, None, 0x0400_0099))]),
            ..VisualDesc::default()
        }),
    );
    let mut d = wearing(&[(0x2000_0020, &[(1, 0.9)])]);
    d.groups.clear();
    d.parent = DataId(0x1F00_0002);
    d.scale = Vec3::new(0.5, 0.5, 0.5);
    let look = Look::of(&r, &d).expect("a look");
    assert_eq!(look.behavior_table, DataId(0x4400_0002));
    assert_eq!(look.scale, Vec3::new(3.0, 3.0, 3.0));
    let tree: Vec<(u32, u32, Option<u32>)> = look
        .tree()
        .iter()
        .map(|g| (g.key, g.setup.0, g.parent))
        .collect();
    assert_eq!(
        tree,
        vec![(ROOT, 0x0400_0001, None), (HEAD, 0x0400_0002, Some(ROOT))]
    );
    let values = look.global.appearance.get(0x2000_0020).expect("the table");
    assert_eq!(values.entries, vec![(1, 0.9), (3, 0.5)]);
}

/// A group hung at a connection point its parent already holds another group at fails the whole
/// description.
///
/// Behaviour: world.look.a-taken-connection-point-fails-the-description
#[test]
fn a_taken_connection_point_fails_the_description() {
    let mut r = Records::default();
    r.visuals.insert(
        0x1F00_0003,
        Arc::new(VisualDesc {
            part_groups: Some(vec![
                (ROOT, group(ROOT, None, None, 0x0400_0001)),
                (HEAD, group(HEAD, Some(ROOT), Some(7), 0x0400_0002)),
                (
                    0x8100_0003,
                    group(0x8100_0003, Some(ROOT), Some(7), 0x0400_0003),
                ),
            ]),
            ..VisualDesc::default()
        }),
    );
    assert_eq!(
        Look::of_record(&r, DataId(0x1F00_0003)),
        Err(LookError::Occupied {
            group: 0x8100_0003,
            connection: 7
        })
    );
}

/// The whole object's part-group changes swap the root's child of their key (its setup replaced)
/// and add a group the root has no child of.
///
/// Behaviour: world.look.a-whole-object-change-swaps-a-part-group
#[test]
fn a_whole_object_change_swaps_a_part_group() {
    let mut r = Records::default();
    r.visuals.insert(
        0x1F00_0004,
        Arc::new(VisualDesc {
            part_groups: Some(vec![
                (ROOT, group(ROOT, None, None, 0x0400_0001)),
                (HEAD, group(HEAD, Some(ROOT), Some(7), 0x0400_0002)),
            ]),
            ..VisualDesc::default()
        }),
    );
    let mut s = set(1.0);
    s.part_groups.push(PartGroupChange {
        priority: 1.0,
        part_group: group(HEAD, Some(ROOT), Some(7), 0x0400_0009),
    });
    s.part_groups.push(PartGroupChange {
        priority: 1.0,
        part_group: group(0x8100_000B, Some(HEAD), Some(3), 0x0400_000B),
    });
    r.tables.insert(
        0x2000_0030,
        Arc::new(Appearance {
            keys: vec![(0x10, vec![s])],
            ..Appearance::default()
        }),
    );
    let mut d = wearing(&[(0x2000_0030, &[(0x10, 1.0)])]);
    d.groups.clear();
    d.parent = DataId(0x1F00_0004);
    let look = Look::of(&r, &d).expect("a look");
    let tree: Vec<(u32, u32)> = look.tree().iter().map(|g| (g.key, g.setup.0)).collect();
    assert_eq!(
        tree,
        vec![
            (ROOT, 0x0400_0001),
            (HEAD, 0x0400_0009),
            (0x8100_000B, 0x0400_000B)
        ]
    );
}

/// A part's mesh goes through each mesh change in order, each matching the kind of the mesh the
/// part has by then (or every kind); a change of kind 0 changes nothing.
///
/// Behaviour: world.look.a-mesh-change-matches-the-mesh-the-part-has-by-then
#[test]
fn a_mesh_change_matches_the_mesh_the_part_has_by_then() {
    let kinds: HashMap<u32, u32> = [(1, 0x10), (2, 0x20), (3, 0x30)].into_iter().collect();
    let look = |atoms: &[(u32, u32)]| PartLook {
        meshes: atoms
            .iter()
            .map(|(k, m)| MeshAtom {
                priority: 1.0,
                mesh_kind: *k,
                mesh: DataId(*m),
            })
            .collect(),
        ..PartLook::default()
    };
    let mut kind_of = |id: DataId| kinds.get(&id.0).copied();
    assert_eq!(
        look(&[(0x10, 2), (0x20, 3)]).mesh(DataId(1), &mut kind_of),
        DataId(3)
    );
    assert_eq!(
        look(&[(0x20, 3), (0x10, 2)]).mesh(DataId(1), &mut kind_of),
        DataId(2)
    );
    assert_eq!(
        look(&[(ANY_KIND, 3)]).mesh(DataId(1), &mut kind_of),
        DataId(3)
    );
    assert_eq!(look(&[(0, 3)]).mesh(DataId(1), &mut kind_of), DataId(1));
}

/// A keyed table walks bucket by bucket, newest first in a bucket: an entry added later goes
/// before the bucket's older ones and after lower buckets'; setting a key keeps its place.
///
/// Behaviour: world.look.an-added-entry-goes-first-in-its-bucket
#[test]
fn an_added_entry_goes_first_in_its_bucket() {
    let mut t: Keyed<f32> = Keyed::stored(47, vec![(1, 0.1), (48, 0.2), (5, 0.3)]);
    assert!(t.add(95, 0.4), "a new key");
    assert!(!t.add(1, 9.0), "a key it has");
    t.set(5, 0.5);
    t.set(2, 0.6);
    assert_eq!(
        t.entries,
        vec![(95, 0.4), (1, 0.1), (48, 0.2), (2, 0.6), (5, 0.5)]
    );
}

/// Records that also place: setups, and which meshes are weighted to bones.
#[derive(Default)]
struct Placing {
    looks: Records,
    setups: HashMap<u32, Arc<auberean_assets::setup::Setup>>,
    weighted: Vec<u32>,
}

impl LookRecords for Placing {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.looks.visual(id)
    }
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.looks.appearance(id)
    }
}

impl auberean_world::place::PlaceRecords for Placing {
    fn setup(&self, id: DataId) -> Option<Arc<auberean_assets::setup::Setup>> {
        self.setups.get(&id.0).cloned()
    }
    fn entity(&self, _id: DataId) -> Option<Arc<auberean_assets::entity::Entity>> {
        None
    }
    fn mesh_weighted(&self, id: DataId) -> bool {
        self.weighted.contains(&id.0)
    }
}

/// A tree: a canopy mesh and a trunk mesh, two bones, the second 8 m up and 2 m aside in the
/// resting pose.
fn tree_setup() -> auberean_assets::setup::Setup {
    use auberean_assets::setup::{Bone, BoneTransform, Pose};
    let rest = |offset: Vec3| BoneTransform {
        offset,
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: auberean_primitives::Quat::IDENTITY,
    };
    auberean_assets::setup::Setup {
        meshes: vec![DataId(0x0600_0001), DataId(0x0600_0002)],
        bones: vec![
            Bone {
                parent: u32::MAX,
                id: 0,
            },
            Bone { parent: 0, id: 1 },
        ],
        poses: vec![Pose {
            kind: 0,
            bones: vec![rest(Vec3::ZERO), rest(Vec3::new(0.0, -2.0, 8.0))],
        }],
        ..auberean_assets::setup::Setup::default()
    }
}

/// When any of a setup's meshes is weighted to bones, every part of it is drawn at the object's
/// frame, the unweighted ones too, whatever bone the resting pose puts them on; when none is,
/// each part follows the bone of its own index.
///
/// Behaviour: world.place.a-weighted-setup-draws-every-part-at-its-frame
#[test]
fn a_weighted_setup_draws_every_part_at_its_frame() {
    use auberean_world::math::Mat4;
    use auberean_world::place::parts_of;
    let mut r = Placing::default();
    r.setups.insert(0x0400_0001, Arc::new(tree_setup()));
    let one = Vec3::new(1.0, 1.0, 1.0);
    let world = Mat4::translation(Vec3::new(10.0, 20.0, 30.0));
    let origins = |r: &Placing| -> Vec<[f32; 3]> {
        parts_of(r, DataId(0x0400_0001), &world, one)
            .iter()
            .map(|p| {
                [
                    p.transform.0[3][0],
                    p.transform.0[3][1],
                    p.transform.0[3][2],
                ]
            })
            .collect()
    };
    assert_eq!(
        origins(&r),
        vec![[10.0, 20.0, 30.0], [10.0, 18.0, 38.0]],
        "no mesh weighted: the trunk on its bone"
    );
    r.weighted.push(0x0600_0001);
    assert_eq!(
        origins(&r),
        vec![[10.0, 20.0, 30.0], [10.0, 20.0, 30.0]],
        "the canopy weighted: the trunk at the object's frame too"
    );
}
