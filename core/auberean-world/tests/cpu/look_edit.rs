//! A visual description the client keeps and changes, on synthetic records: appearance values
//! added to a group copied from the description it extends and taken off (overridden when only
//! the extended one gives them), descriptions taken out and put in place, an effect's appearance
//! laid over an object's own, the appearance tables' questions, and the icon atoms' replacement.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::common::Color32;
use auberean_assets::fx::AppearanceKey;
use auberean_assets::visual::{
    Appearance, AppearanceChanges, IconLayer, ModifierChange, PartGroup, VisualDesc,
};
use auberean_primitives::{DataId, Vec3};
use auberean_world::chargen::key_values;
use auberean_world::look::{
    appearance_has_key, appearance_key_steps, appearance_step_count, total_icon, Atoms, Desc,
    Group, Keyed, Look, LookRecords, ObjectLook, ANY_KIND, GLOBAL_KEY, REMOVED,
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

const BODY: u32 = 0x1F00_0001;
const ROOT: u32 = 0x8100_0001;
const HEAD: u32 = 0x8100_0002;
const HAT: u32 = 0x8100_0003;
const SKIN: u32 = 0x2000_0001;
const GLOW: u32 = 0x2000_0002;
const BODY_KIND: u32 = 0x8100_0016;

fn values(entries: &[(u32, f32)]) -> Keyed<f32> {
    Keyed::stored(47, entries.to_vec())
}

fn plain(parent: u32) -> Desc {
    let mut d = Desc {
        parent: DataId(parent),
        scale: Vec3::new(1.0, 1.0, 1.0),
        child_scale: 1.0,
        particle_scale: 1.0,
        ..Desc::default()
    };
    d.global.key = GLOBAL_KEY;
    d
}

/// A body record: a root group, a head hung from it at connection point 1 and a hat hung from the
/// head; the body's skin value is key 1 at 0.5 for every group.
fn records() -> Records {
    let mut r = Records::default();
    let group = |key: u32, parent: Option<u32>, connection: Option<u32>| PartGroup {
        key: Some(key),
        parent_key: parent,
        connection_point: connection,
        setup: Some(DataId(0x0400_0000 | (key & 0xFF))),
        ..PartGroup::default()
    };
    r.visuals.insert(
        BODY,
        Arc::new(VisualDesc {
            id: Some(BODY),
            global: Some(PartGroup {
                appearance_keys: Some(vec![(SKIN, vec![(1, 0.5)])]),
                ..PartGroup::default()
            }),
            part_groups: Some(vec![
                (ROOT, group(ROOT, None, None)),
                (HEAD, group(HEAD, Some(ROOT), Some(1))),
                (HAT, group(HAT, Some(HEAD), Some(2))),
            ]),
            ..VisualDesc::default()
        }),
    );
    let set = |intensity: f32, modifier: u32| AppearanceChanges {
        intensity,
        modifiers: vec![ModifierChange {
            priority: 1.0,
            template: DataId::NONE,
            modifier: DataId(modifier),
            mesh_kind: ANY_KIND,
            material_kind: BODY_KIND,
        }],
        ..AppearanceChanges::default()
    };
    r.tables.insert(
        SKIN,
        Arc::new(Appearance {
            keys: vec![(1, vec![set(0.0, 0x3000_0001), set(0.5, 0x3000_0002)])],
            step_counts: vec![(1, 2)],
            ..Appearance::default()
        }),
    );
    r.tables.insert(
        GLOW,
        Arc::new(Appearance {
            base: vec![SKIN],
            keys: vec![(
                2,
                vec![
                    set(0.25, 0x3000_0007),
                    set(0.25, 0x3000_0008),
                    set(0.75, 0x3000_0009),
                ],
            )],
            ..Appearance::default()
        }),
    );
    r
}

fn materials(r: &Records, look: &Look, key: u32) -> Vec<u32> {
    look.atoms(r, key)
        .expect("atoms")
        .materials
        .iter()
        .map(|m| m.modifier.0)
        .collect()
}

/// A value for a group the object's own description lacks goes to a copy of the extended
/// description's group: its key, connection point and parent key only, hung from its parent; the
/// object's groups that hang from it in the extended description are marked as hanging from the
/// copy. The value applies on top of the extended description's.
///
/// Behaviour: world.look.a-value-for-a-missing-group-goes-to-a-copy-of-the-parents
#[test]
fn a_value_for_a_missing_group_goes_to_a_copy_of_the_parents() {
    let r = records();
    let mut own = Look::local(&plain(BODY));
    assert!(own.groups.is_empty());
    let added = own.add_appearance_key(
        &r,
        HEAD,
        GLOW,
        AppearanceKey {
            key: 2,
            scalar: 0.8,
        },
    );
    assert!(added);
    let head = own.group(HEAD).expect("a copy of the head");
    assert_eq!(
        (head.key, head.connection, head.parent_key, head.setup),
        (HEAD, 1, ROOT, DataId::NONE)
    );
    assert_eq!(head.applied_value(GLOW, 2), Some(0.8));
    assert_eq!(
        own.root,
        Some(HEAD),
        "the copy is the only group, so the root"
    );
    let total = Look::of(&r, &own.to_desc()).expect("a total");
    let setup = total.group(HEAD).map(|g| g.setup);
    assert_eq!(
        setup,
        Some(DataId(0x0400_0002)),
        "the body's head keeps its setup"
    );
    // The whole object's skin first (its set at 0.5), then the head's glow at 0.8 (its set at
    // 0.75); the glow's base table, the skin's, has no key 2.
    assert_eq!(materials(&r, &total, HEAD), vec![0x3000_0002, 0x3000_0009]);
    // Neither a group nor a table of 0 takes a value.
    let zero = AppearanceKey {
        key: 2,
        scalar: 1.0,
    };
    assert!(!own.add_appearance_key(&r, 0, GLOW, zero));
    assert!(!own.add_appearance_key(&r, HEAD, 0, zero));
}

/// Taking a value off: the own description's group loses it (a table left with no keys goes);
/// a value only the extended description gives is overridden with -1, which applies nothing; a
/// value neither gives is not taken off.
///
/// Behaviour: world.look.a-value-only-the-parent-gives-is-overridden-to-come-off
#[test]
fn a_value_only_the_parent_gives_is_overridden_to_come_off() {
    let r = records();
    let mut own = Look::local(&plain(BODY));
    own.add_appearance(&r, GLOBAL_KEY, GLOW, &values(&[(2, 0.3)]));
    assert!(own.remove_appearance_key(&r, GLOBAL_KEY, GLOW, 2));
    assert!(
        own.global.appearance.get(GLOW).is_none(),
        "an emptied table goes"
    );
    assert!(own.remove_appearance_key(&r, GLOBAL_KEY, SKIN, 1));
    assert_eq!(own.applied_value(GLOBAL_KEY, SKIN, 1), Some(REMOVED));
    let total = Look::of(&r, &own.to_desc()).expect("a total");
    assert!(
        materials(&r, &total, ROOT).is_empty(),
        "the skin no longer applies"
    );
    assert!(!own.remove_appearance_key(&r, GLOBAL_KEY, GLOW, 7));
}

/// An object's descriptions: an effect's appearance value is kept apart and laid over the object's
/// own; setting the own description again keeps it laid over; when the effect's value comes off,
/// it comes off both.
///
/// Behaviour: world.look.an-effects-appearance-is-laid-over-the-objects-own
#[test]
fn an_effects_appearance_is_laid_over_the_objects_own() {
    let r = records();
    let mut o = ObjectLook::new(&plain(BODY));
    let glow = AppearanceKey {
        key: 2,
        scalar: 0.3,
    };
    assert!(o.do_appearance_fx(&r, GLOW, glow));
    assert_eq!(o.effects.applied_value(GLOBAL_KEY, GLOW, 2), Some(0.3));
    let total = o.total(&r).expect("a total");
    assert_eq!(
        materials(&r, &total, ROOT),
        vec![0x3000_0002, 0x3000_0008],
        "the skin, then the glow: the last of its two sets at 0.25"
    );
    o.set_local(&plain(BODY));
    assert_eq!(o.local.applied_value(GLOBAL_KEY, GLOW, 2), Some(0.3));
    o.remove_appearance_fx(&r, GLOW, 2);
    assert_eq!(o.local.applied_value(GLOBAL_KEY, GLOW, 2), None);
    assert_eq!(o.effects.applied_value(GLOBAL_KEY, GLOW, 2), None);
    let total = o.total(&r).expect("a total");
    assert_eq!(materials(&r, &total, ROOT), vec![0x3000_0002]);
}

/// Taking a description out: the values it gives every group come off; a group of it with
/// appearance values loses those (each as one value comes off); a group with neither values nor
/// effects goes with every group hanging from it, those first.
///
/// Behaviour: world.look.a-description-taken-out-takes-its-values-and-bare-groups
#[test]
fn a_description_taken_out_takes_its_values_and_bare_groups() {
    let r = records();
    let mut own = Look::local(&Desc::from_stored(&r.visuals[&BODY]));
    own.parent = DataId::NONE;
    own.add_appearance(&r, HEAD, GLOW, &values(&[(2, 0.3)]));
    let mut out = plain(0);
    out.global.appearance = Keyed::stored(47, vec![(SKIN, values(&[(1, 0.0)]))]);
    let mut head = Group {
        key: HEAD,
        ..Group::default()
    };
    head.appearance = Keyed::stored(47, vec![(GLOW, values(&[(2, 0.0)]))]);
    let hat = Group {
        key: HAT,
        ..Group::default()
    };
    out.groups = vec![head, hat];
    assert!(own.apply_subtractive(&r, &out));
    assert!(own.global.appearance.is_empty());
    assert_eq!(own.applied_value(HEAD, GLOW, 2), None);
    assert!(
        own.group(HEAD).is_some(),
        "a group with values keeps its place"
    );
    assert!(own.group(HAT).is_none(), "a bare group goes");
    assert!(own.group(HEAD).is_some_and(|g| g.children.is_empty()));
    // Taking the root's tree out takes every group.
    assert!(own.remove_group_tree(ROOT));
    assert!(own.groups.is_empty());
    assert_eq!(own.root, None);
}

/// Putting a description in place of one extending the same description adds it and drops every
/// group it does not name; one extending another description is copied in whole.
///
/// Behaviour: world.look.a-description-put-in-place-drops-the-groups-it-does-not-name
#[test]
fn a_description_put_in_place_drops_the_groups_it_does_not_name() {
    let r = records();
    let mut stored = Desc::from_stored(&r.visuals[&BODY]);
    stored.parent = DataId(0x1F00_0002);
    let mut own = Look::local(&stored);
    assert_eq!(own.groups.len(), 3);
    let mut new = plain(0x1F00_0002);
    new.groups = stored
        .groups
        .iter()
        .filter(|g| g.key != HAT)
        .cloned()
        .map(|mut g| {
            g.children.retain(|c| *c != HAT);
            g
        })
        .collect();
    own.replace(&new).expect("put in place");
    let keys: Vec<u32> = own.groups.entries.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![ROOT, HEAD]);
    assert!(own.group(HEAD).is_some_and(|g| g.children.is_empty()));
    let other = plain(BODY);
    own.replace(&other).expect("copied in");
    assert_eq!(own.parent, DataId(BODY));
    assert!(own.groups.is_empty());
}

/// Changing an object to a stored description keeps the values the whole object had: its
/// total's values for every group come back on the new description.
///
/// Behaviour: world.look.a-combined-description-keeps-the-whole-objects-values
#[test]
fn a_combined_description_keeps_the_whole_objects_values() {
    let mut r = records();
    r.visuals.insert(
        0x1F00_0005,
        Arc::new(VisualDesc {
            id: Some(0x1F00_0005),
            parent: Some(DataId(BODY)),
            ..VisualDesc::default()
        }),
    );
    let mut own = plain(BODY);
    own.global.appearance = Keyed::stored(47, vec![(GLOW, values(&[(2, 0.3)]))]);
    let mut o = ObjectLook::new(&own);
    assert!(o.combine(&r, DataId(0x1F00_0005)));
    assert_eq!(o.local.parent, DataId(BODY));
    assert_eq!(o.local.applied_value(GLOBAL_KEY, GLOW, 2), Some(0.3));
    assert_eq!(
        o.local.applied_value(GLOBAL_KEY, SKIN, 1),
        Some(0.5),
        "the body's skin, a value of the old total, is added too"
    );
    assert!(!o.combine(&r, DataId(0x1F00_0009)));
}

/// An appearance table's questions: a key is had when the table or one of its bases has it; a
/// key's strength steps are its change sets' intensities, the bases' first, put in order; the
/// step count is the table's own.
///
/// Behaviour: world.look.an-appearance-tables-keys-steps-and-counts
#[test]
fn an_appearance_tables_keys_steps_and_counts() {
    let mut r = records();
    assert!(appearance_has_key(&r, DataId(GLOW), 1));
    assert!(appearance_has_key(&r, DataId(GLOW), 2));
    assert!(!appearance_has_key(&r, DataId(SKIN), 2));
    assert!(!appearance_has_key(&r, DataId(0x2000_0009), 1));
    assert_eq!(
        appearance_key_steps(&r, DataId(GLOW), 2),
        Some(vec![0.25, 0.25, 0.75])
    );
    assert_eq!(
        appearance_key_steps(&r, DataId(GLOW), 1),
        Some(vec![0.0, 0.5])
    );
    assert_eq!(appearance_key_steps(&r, DataId(0x2000_0009), 1), None);
    assert_eq!(key_values(&r, 0x2000_0009, 1), Vec::<f32>::new());
    let stops = |list: &[f32]| {
        list.iter()
            .map(|intensity| AppearanceChanges {
                intensity: *intensity,
                ..AppearanceChanges::default()
            })
            .collect::<Vec<_>>()
    };
    r.tables.insert(
        0x2000_0020,
        Arc::new(Appearance {
            keys: vec![(5, stops(&[0.2, 0.9]))],
            ..Appearance::default()
        }),
    );
    r.tables.insert(
        0x2000_0021,
        Arc::new(Appearance {
            base: vec![0x2000_0020],
            keys: vec![(5, stops(&[0.5]))],
            ..Appearance::default()
        }),
    );
    assert_eq!(
        key_values(&r, 0x2000_0021, 5),
        vec![0.2, 0.5, 0.9],
        "a choice's stops take in its base tables"
    );
    assert_eq!(appearance_step_count(&r, DataId(SKIN), 1), Some(2));
    assert_eq!(appearance_step_count(&r, DataId(GLOW), 1), None);
}

/// An icon atom replaces the first one gathered of its layer whose picture and color are both
/// there or both missing as its own; the replaced one goes and the new one goes last. An object's
/// icon is the chain's layers with the atoms laid over in that order.
///
/// Behaviour: world.look.an-icon-atom-replaces-one-of-its-layer-and-kind
#[test]
fn an_icon_atom_replaces_one_of_its_layer_and_kind() {
    let layer = |layer: u32, picture: u32, color: u32| IconLayer {
        layer,
        picture: DataId(picture),
        shift_color: Color32(color),
    };
    let mut atoms = Atoms::default();
    assert!(atoms.add_icon(layer(1, 0x0600_0001, 0)));
    assert!(atoms.add_icon(layer(1, 0, 0xFF00_00FF)));
    assert!(atoms.add_icon(layer(2, 0x0600_0002, 0xFF00_FF00)));
    assert!(atoms.add_icon(layer(1, 0x0600_0003, 0)));
    assert_eq!(
        atoms.icons,
        vec![
            layer(1, 0, 0xFF00_00FF),
            layer(2, 0x0600_0002, 0xFF00_FF00),
            layer(1, 0x0600_0003, 0),
        ]
    );
    // Laid over a stored icon: the dye's color over the first layer, a new third layer.
    let mut r = Records::default();
    r.visuals.insert(
        0x1F00_0010,
        Arc::new(VisualDesc {
            id: Some(0x1F00_0010),
            icon: Some(vec![layer(1, 0x0600_0010, 0xFFFF_FFFF)]),
            ..VisualDesc::default()
        }),
    );
    r.tables.insert(
        0x2000_0010,
        Arc::new(Appearance {
            keys: vec![(
                5,
                vec![AppearanceChanges {
                    intensity: 0.0,
                    icon_layers: vec![layer(1, 0, 0xFF12_3456), layer(3, 0x0600_0011, 0)],
                    ..AppearanceChanges::default()
                }],
            )],
            ..Appearance::default()
        }),
    );
    let mut global = Group {
        key: GLOBAL_KEY,
        ..Group::default()
    };
    global.augment_appearance(0x2000_0010, &values(&[(5, 1.0)]));
    let icon = total_icon(&r, DataId(0x1F00_0010), &[], &global);
    assert_eq!(
        icon,
        vec![layer(1, 0x0600_0010, 0xFF12_3456), layer(3, 0x0600_0011, 0)]
    );
}
