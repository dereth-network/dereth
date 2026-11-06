//! The objects' own visual descriptions as their scripts read and change them, and the visual
//! descriptions scripts hold.
//!
//! - **A description a script holds** ([`ScriptLook`]) is kept whole, as the client keeps one: its
//!   tables, scales, icon, its changes to every part group and its part groups in their table's
//!   order. A new one is empty; one read from a packed graph or a call's arguments is unpacked
//!   from the description's own form; packed again it is written in that form, the fields marked
//!   present being those that differ from an empty description's ([`wire_of`]).
//! - **An object's own description** is the one the server last sent it, put in the client's own
//!   form ([`Look::local`]). A script may start it again from the object's physics record (the
//!   object's first data-id quality: a visual description, or an entity followed to its visual
//!   description), add another description to it or take one out. The changed description is the
//!   scripts' own until they have the object's body take it: asked for again it answers the change,
//!   and the body still shows the description it took last. Taken, it becomes the object's
//!   description (the front end dresses the body from it as from one the server sent); a
//!   description the server sends, or a new creation, replaces the scripts' change.
//! - **Whether an object has a motion interpreter**: whether the total of the description its
//!   body took names a motion description.
//!
//! An object with no body answers 0 and keeps nothing; its own description reads as an empty one.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::common::Color32;
use auberean_assets::entity::Entity;
use auberean_assets::visual::{Appearance, IconLayer, VisualDesc as StoredVisual};
use auberean_assets::Database;
use auberean_dat::qualified::divine_type;
use auberean_primitives::{DataId, Vec3};
use auberean_protocol::table::{Table, BUCKET_COUNTS, SMALL};
use auberean_protocol::types::{
    IconLayer as WireIcon, PartGroupDesc as WireGroup, VisualDesc as WireVisual,
};
use auberean_protocol::wire::{Field, Reader, Writer};
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{tag, NativeId, NULL};
use auberean_world::look::{Desc, Group, Keyed, Look, LookRecords, GLOBAL_KEY};

use crate::body::holds_body;
use crate::host::{host, receiver_object, ClientHost};

/// A visual description, as a native type.
pub const VISUAL_DESC: u32 = 0x71;
/// The scripts' physics object.
const PHYSICS_OBJECT: u32 = 0x80;
/// The record types an object's physics record may be.
const VISUAL_DESC_TYPE: u32 = 0x4000_0021;
const ENTITY_TYPE: u32 = 0x4000_0049;
/// The data-id quality naming an object's physics record.
const PHYSICS_RECORD: u32 = 1;
/// How far an entity is followed to the visual description it is made from.
const ENTITY_CHAIN: usize = 8;

/// The objects' own descriptions the scripts changed and their bodies have not taken, by object.
pub type Looks = HashMap<u64, Look>;

// ---------------------------------------------------------------------------------------------
// The description's own form
// ---------------------------------------------------------------------------------------------

fn buckets_of(size_class: u8) -> u32 {
    BUCKET_COUNTS
        .get(usize::from(size_class))
        .copied()
        .unwrap_or(BUCKET_COUNTS[0])
}

fn size_class_of(buckets: u32) -> u8 {
    BUCKET_COUNTS
        .iter()
        .position(|b| *b == buckets)
        .and_then(|i| u8::try_from(i).ok())
        .unwrap_or(SMALL)
}

fn keyed<V: Clone>(t: &Table<V>) -> Keyed<V> {
    Keyed::stored(buckets_of(t.size_class), t.entries.clone())
}

fn table<V: Clone>(k: &Keyed<V>) -> Table<V> {
    Table {
        size_class: size_class_of(k.buckets),
        entries: k.entries.clone(),
    }
}

fn group_of(g: &WireGroup, key: u32) -> Group {
    let has = |bit: u32| g.bits & bit != 0;
    let id = |bit: u32, v: u32| DataId(if has(bit) { v } else { 0 });
    let appearance = if has(WireGroup::APPEARANCE) {
        Keyed::stored(
            buckets_of(g.appearance.size_class),
            g.appearance
                .entries
                .iter()
                .map(|(apr, keys)| (*apr, keyed(keys)))
                .collect(),
        )
    } else {
        Keyed::default()
    };
    Group {
        key,
        parent_key: if has(WireGroup::PARENT_KEY) {
            g.parent_key
        } else {
            0
        },
        connection: if has(WireGroup::CONNECTION_POINT) {
            g.connection_point
        } else {
            0
        },
        setup: id(WireGroup::SETUP, g.setup),
        anim_map: id(WireGroup::ANIM_MAP, g.anim_map),
        appearance,
        fx_table: id(WireGroup::FX_TABLE, g.fx_table),
        startup_fx: if has(WireGroup::STARTUP_FX) {
            keyed(&g.startup_fx)
        } else {
            Keyed::default()
        },
        ..Group::default()
    }
}

/// A description in its own form as the client holds one: its fields, its changes to every
/// group, and its groups hung from their parents by key, in their order. A parent `entity` turns
/// into a visual description stands for that description.
// D2-13814
#[must_use]
pub fn desc_of(v: &WireVisual, entity: impl Fn(DataId) -> Option<DataId>) -> Desc {
    let has = |bit: u32| v.bits & bit != 0;
    let id = |bit: u32, x: u32| DataId(if has(bit) { x } else { 0 });
    let mut parent = id(WireVisual::PARENT, v.parent);
    if divine_type(parent.0) == ENTITY_TYPE {
        parent = entity(parent).unwrap_or(DataId::NONE);
    }
    let mut groups: Vec<Group> = if has(WireVisual::PART_GROUPS) {
        v.part_groups
            .entries
            .iter()
            .map(|(k, g)| {
                let key = if g.bits & WireGroup::KEY != 0 {
                    g.key
                } else {
                    *k
                };
                group_of(g, key)
            })
            .collect()
    } else {
        Vec::new()
    };
    for i in 0..groups.len() {
        let pk = groups[i].parent_key;
        if pk == 0 {
            continue;
        }
        if let Some(p) = groups.iter().position(|g| g.key == pk) {
            let child = groups[i].key;
            groups[p].children.push(child);
            groups[i].parent = Some(pk);
        }
    }
    let mut global = if has(WireVisual::GLOBAL) {
        group_of(&v.global, GLOBAL_KEY)
    } else {
        Group::default()
    };
    global.key = GLOBAL_KEY;
    let f = |bit: u32, x: f32| if has(bit) { x } else { 1.0 };
    Desc {
        parent,
        motion_desc: id(WireVisual::MOTION_INTERP, v.motion_interp),
        behavior_table: id(WireVisual::BEHAVIOR_TABLE, v.behavior_table),
        modes: id(WireVisual::MODES, v.modes),
        scale: if has(WireVisual::SCALE) {
            v.scale
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        },
        child_scale: f(WireVisual::CHILD_SCALE, v.child_scale),
        particle_scale: f(WireVisual::PARTICLE_SCALE, v.particle_scale),
        global,
        groups,
        icon: if has(WireVisual::ICON) {
            v.icon
                .iter()
                .map(|l| IconLayer {
                    layer: l.0,
                    picture: DataId(l.1),
                    shift_color: auberean_assets::common::Color32(l.2),
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// Whether a scale differs from one by as much as the description's own form notices.
fn not_one(x: f32) -> bool {
    (f64::from(x) - 1.0).abs() >= f64::from(0.0002_f32)
}

/// A part group in the description's own form: each field marked present when it is set (its
/// effects' overrides have no form here and are left out).
// D2-14040, D2-14041
fn wire_group(g: &Group) -> WireGroup {
    let mut bits = 0;
    for (on, bit) in [
        (g.key != 0, WireGroup::KEY),
        (g.parent_key != 0, WireGroup::PARENT_KEY),
        (g.connection != 0, WireGroup::CONNECTION_POINT),
        (!g.setup.is_none(), WireGroup::SETUP),
        (!g.anim_map.is_none(), WireGroup::ANIM_MAP),
        (!g.appearance.is_empty(), WireGroup::APPEARANCE),
        (!g.fx_table.is_none(), WireGroup::FX_TABLE),
        (!g.startup_fx.is_empty(), WireGroup::STARTUP_FX),
    ] {
        if on {
            bits |= bit;
        }
    }
    WireGroup {
        bits,
        key: g.key,
        parent_key: g.parent_key,
        connection_point: g.connection,
        setup: g.setup.0,
        anim_map: g.anim_map.0,
        appearance: Table {
            size_class: size_class_of(g.appearance.buckets),
            entries: g
                .appearance
                .entries
                .iter()
                .map(|(apr, keys)| (*apr, table(keys)))
                .collect(),
        },
        fx_table: g.fx_table.0,
        startup_fx: table(&g.startup_fx),
    }
}

/// Does a part group change nothing: no setup, animation map, appearance or effects?
fn group_empty(g: &Group) -> bool {
    g.setup.is_none()
        && g.anim_map.is_none()
        && g.appearance.is_empty()
        && g.fx_table.is_none()
        && g.startup_fx.is_empty()
        && g.fx_overrides.is_empty()
}

/// A description in its own form: each field marked present when it differs from an empty
/// description's (a scale by at least 0.0002), its part groups in their table's order.
// D2-13803, D2-13814
#[must_use]
pub fn wire_of(look: &Look) -> WireVisual {
    let mut bits = 0;
    let s = look.scale;
    for (on, bit) in [
        (!look.parent.is_none(), WireVisual::PARENT),
        (!look.motion_desc.is_none(), WireVisual::MOTION_INTERP),
        (!look.behavior_table.is_none(), WireVisual::BEHAVIOR_TABLE),
        (!look.modes.is_none(), WireVisual::MODES),
        (
            not_one(s.x) || not_one(s.y) || not_one(s.z),
            WireVisual::SCALE,
        ),
        (not_one(look.child_scale), WireVisual::CHILD_SCALE),
        (not_one(look.particle_scale), WireVisual::PARTICLE_SCALE),
        (!look.icon.is_empty(), WireVisual::ICON),
        (!group_empty(&look.global), WireVisual::GLOBAL),
        (!look.groups.is_empty(), WireVisual::PART_GROUPS),
    ] {
        if on {
            bits |= bit;
        }
    }
    WireVisual {
        bits,
        did: 0,
        parent: look.parent.0,
        motion_interp: look.motion_desc.0,
        behavior_table: look.behavior_table.0,
        modes: look.modes.0,
        scale: look.scale,
        child_scale: look.child_scale,
        particle_scale: look.particle_scale,
        icon: look
            .icon
            .iter()
            .map(|l| WireIcon(l.layer, l.picture.0, l.shift_color.0))
            .collect(),
        global: wire_group(&look.global),
        part_groups: Table {
            size_class: size_class_of(look.groups.buckets),
            entries: look
                .groups
                .entries
                .iter()
                .map(|(k, g)| (*k, wire_group(g)))
                .collect(),
        },
    }
}

/// A description in its own form held as the client holds one it read or copied: its fields, and
/// its tables in the order read (a copy keeps every table's order), the root found.
// D2-13839, D2-13809
#[must_use]
pub fn held(v: &WireVisual, entity: impl Fn(DataId) -> Option<DataId>) -> Look {
    let d = desc_of(v, entity);
    let mut groups = Look::default().groups;
    if v.bits & WireVisual::PART_GROUPS != 0 {
        groups.buckets = buckets_of(v.part_groups.size_class);
    }
    groups.entries = d.groups.into_iter().map(|g| (g.key, g)).collect();
    let mut l = Look {
        parent: d.parent,
        motion_desc: d.motion_desc,
        behavior_table: d.behavior_table,
        modes: d.modes,
        scale: d.scale,
        child_scale: d.child_scale,
        particle_scale: d.particle_scale,
        global: d.global,
        groups,
        root: None,
        icon: d.icon,
    };
    l.reset_root();
    l
}

/// A description read from its own form, in the client's own form.
#[must_use]
pub fn unpack(bytes: &[u8]) -> Option<Look> {
    let mut r = Reader::new(bytes);
    let v = WireVisual::read(&mut r).ok()?;
    r.finish().ok()?;
    Some(held(&v, |_| None))
}

/// A description written in its own form.
#[must_use]
pub fn pack(look: &Look) -> Vec<u8> {
    let mut w = Writer::default();
    wire_of(look).write(&mut w);
    w.finish()
}

// ---------------------------------------------------------------------------------------------
// The descriptions scripts hold
// ---------------------------------------------------------------------------------------------

/// A visual description a script holds, kept whole.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScriptLook(pub Look);

impl EngineValue for ScriptLook {
    fn native_type(&self) -> u32 {
        VISUAL_DESC
    }

    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn pack(&self) -> Option<Vec<u8>> {
        Some(pack(&self.0))
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl ScriptLook {
    /// The description a native object holds: one no native has touched yet is empty; one read
    /// from a packed graph is unpacked.
    // D2-13836
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Look> {
        match o {
            NativeObject::Engine(e) => e.get::<Self>().map(|s| s.0.clone()),
            NativeObject::Opaque { ty } if *ty == VISUAL_DESC => Some(Look::default()),
            NativeObject::Packed { ty, bytes } if *ty == VISUAL_DESC => unpack(bytes),
            _ => None,
        }
    }

    /// A native object holding `look`.
    #[must_use]
    pub fn native(look: Look) -> NativeObject {
        NativeObject::Engine(Engine::new(Self(look)))
    }
}

// ---------------------------------------------------------------------------------------------
// The objects' own descriptions
// ---------------------------------------------------------------------------------------------

/// The visual descriptions and appearance tables of the data files.
#[derive(Debug)]
pub struct Records<'a>(pub &'a Database);

impl LookRecords for Records<'_> {
    fn visual(&self, id: DataId) -> Option<Arc<StoredVisual>> {
        self.0.get::<StoredVisual>(id.0).map(|o| o.arc())
    }

    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.0.get::<Appearance>(id.0).map(|o| o.arc())
    }
}

impl Records<'_> {
    /// The visual description an entity is made from, its parent entities followed.
    // D2-13021
    #[must_use]
    pub fn entity_visual(&self, id: DataId) -> Option<DataId> {
        let mut id = id.0;
        for _ in 0..ENTITY_CHAIN {
            match divine_type(id) {
                VISUAL_DESC_TYPE => return Some(DataId(id)),
                ENTITY_TYPE => id = self.0.get::<Entity>(id)?.object?,
                _ => return None,
            }
        }
        None
    }
}

/// The data files' records, made ready.
fn database(h: &mut ClientHost) -> Option<&Database> {
    h.data_cache().map(crate::cache::DataCache::database)
}

/// The description an object's body took last: the one the server sent it (a stored description
/// it names copied), in the client's own form; an empty one for an object not here.
// D2-06236, D2-08212
#[must_use]
pub fn taken(h: &mut ClientHost, object: u64) -> Look {
    let Some(v) = h.model.object(object).map(|o| o.visual.clone()) else {
        return Look::default();
    };
    let Some(db) = database(h) else {
        return held(&v, |_| None);
    };
    let records = Records(db);
    if v.bits & WireVisual::DID != 0 && v.did != 0 {
        return Look::local_of_record(&records, DataId(v.did)).unwrap_or_default();
    }
    held(&v, |id| records.entity_visual(id))
}

/// An object's own description: the scripts' change when they made one, else the one its body
/// took.
#[must_use]
pub fn local(h: &mut ClientHost, object: u64) -> Look {
    match h.looks.get(&object) {
        Some(l) => l.clone(),
        None => taken(h, object),
    }
}

/// Whether the object's body has a motion interpreter: the total of the description it took
/// names a motion description. `false` with no body.
// D2-08166, D2-05906, D2-06395, D2-06374
#[must_use]
pub fn has_motion_interpreter(h: &mut ClientHost, object: u64) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    let desc = taken(h, object).to_desc();
    let Some(db) = database(h) else {
        return !desc.motion_desc.is_none();
    };
    Look::of(&Records(db), &desc).is_ok_and(|t| !t.motion_desc.is_none())
}

/// Start the object's own description again from its physics record: emptied, extending the
/// record when it is a visual description, or the visual description an entity record is made
/// from. `false` with no body or no such record (the description is left as it was).
// D2-08136, D2-06199, D2-00418
pub fn init(h: &mut ClientHost, object: u64) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    let record = crate::repository::answer(h, &crate::repository::DATA_ID, object, PHYSICS_RECORD)
        .unwrap_or(0);
    let parent = match divine_type(record) {
        VISUAL_DESC_TYPE => DataId(record),
        ENTITY_TYPE => {
            let Some(db) = database(h) else {
                return false;
            };
            let records = Records(db);
            let Some(entity) = db.get::<Entity>(record) else {
                return false;
            };
            match entity.object.and_then(|o| records.entity_visual(DataId(o))) {
                Some(v) => v,
                None => return false,
            }
        }
        _ => return false,
    };
    h.looks.insert(
        object,
        Look {
            parent,
            ..Look::default()
        },
    );
    true
}

/// Add `desc` to the object's own description; whether it took. `false` with no body.
// D2-08139, D2-06201
pub fn apply_additive(h: &mut ClientHost, object: u64, desc: &Look) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    let mut l = local(h, object);
    let took = l.apply_additive(&desc.to_desc()).is_ok();
    h.looks.insert(object, l);
    took
}

/// Take `desc` out of the object's own description; whether it could. `false` with no body.
// D2-08140, D2-06202
pub fn apply_subtractive(h: &mut ClientHost, object: u64, desc: &Look) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    let mut l = local(h, object);
    let took = match database(h) {
        Some(db) => l.apply_subtractive(&Records(db), &desc.to_desc()),
        None => l.apply_subtractive(&NoRecords, &desc.to_desc()),
    };
    h.looks.insert(object, l);
    took
}

/// The object's body takes its own description: the scripts' change becomes the object's
/// description, which its body is dressed from. Whether the body took it: its total is put
/// together, changes something and has a part group the tree hangs from. `false` with no body.
// D2-08141, D2-05863, D2-06148
pub fn apply_to_body(h: &mut ClientHost, object: u64) -> bool {
    if !holds_body(h, object) {
        return false;
    }
    let Some(l) = h.looks.remove(&object) else {
        let l = taken(h, object);
        return total_takes(h, &l);
    };
    let wire = wire_of(&l);
    if let Some(o) = h.model.object_mut(object) {
        o.visual = wire;
    }
    total_takes(h, &l)
}

fn total_takes(h: &mut ClientHost, l: &Look) -> bool {
    let desc = l.to_desc();
    let total = match database(h) {
        Some(db) => Look::of(&Records(db), &desc),
        None => Look::of(&NoRecords, &desc),
    };
    total.is_ok_and(|t| !t.adds_nothing() && t.root.is_some())
}

/// No records: a host without data files.
struct NoRecords;

impl LookRecords for NoRecords {
    fn visual(&self, _: DataId) -> Option<Arc<StoredVisual>> {
        None
    }
    fn appearance(&self, _: DataId) -> Option<Arc<Appearance>> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// The natives
// ---------------------------------------------------------------------------------------------

/// The description a native object argument holds (`fp[-2]` and on); a null one stops the run.
fn look_arg(c: &mut Call<'_>, n: i64) -> Result<Look, Stop> {
    let r = c.arg(n)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let native = c.native;
    let o = c.vm.heap.native(r).map_err(|e| Stop::Native {
        native,
        what: format!("{e:?}"),
    })?;
    ScriptLook::of(o).ok_or_else(|| Stop::Native {
        native,
        what: format!("{r:#x} is not a visual description"),
    })
}

/// Whether the body has a motion interpreter, into `fp[-2]`.
// D2-04722
fn native_has_motion_interpreter(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let answer = has_motion_interpreter(host(c)?, object);
    c.write(-2, u32::from(answer), tag::MIXED)?;
    Ok(Status::Success)
}

/// A new description, a copy of the object's own, into `fp[-2]`.
// D2-04761
fn native_get_local_visual_desc(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let look = {
        let h = host(c)?;
        if holds_body(h, object) {
            local(h, object)
        } else {
            Look::default()
        }
    };
    let r = c.vm.heap.add_native(VISUAL_DESC, ScriptLook::native(look));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Start the object's own description again from its physics record; whether it could, into
/// `fp[-2]`.
// D2-04724
fn native_init_visual_desc(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let answer = init(host(c)?, object);
    c.write(-2, u32::from(answer), tag::MIXED)?;
    Ok(Status::Success)
}

/// Add the description `fp[-2]` to the object's own; whether it took, into `fp[-3]`.
// D2-04697
fn native_apply_additive(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let desc = look_arg(c, 0)?;
    let answer = apply_additive(host(c)?, object, &desc);
    c.write(-3, u32::from(answer), tag::MIXED)?;
    Ok(Status::Success)
}

/// Take the description `fp[-2]` out of the object's own; whether it could, into `fp[-3]`.
// D2-04698
fn native_apply_subtractive(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let desc = look_arg(c, 0)?;
    let answer = apply_subtractive(host(c)?, object, &desc);
    c.write(-3, u32::from(answer), tag::MIXED)?;
    Ok(Status::Success)
}

/// The object's body takes its own description; whether it did, into `fp[-2]`.
// D2-04699
fn native_apply_to_body(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let object = receiver_object(c)?;
    let answer = apply_to_body(host(c)?, object);
    c.write(-2, u32::from(answer), tag::MIXED)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// The descriptions, icons and appearance tables scripts hold
// ---------------------------------------------------------------------------------------------

/// An icon description, as a native type, and a table of appearance tables' values.
const ICON_DESC: u32 = 0x2E;
const APP_INFO_HASH: u32 = 0x0F;
/// An applied appearance value (an appearance key and its strength), as a native type.
const APPLIED_KEY: u32 = 0x10;
/// A vector, as a native type.
const VECTOR: u32 = 0x70;

/// An icon description a script holds: its layers, each a number, a picture and a color, in
/// order (a new one has none).
#[must_use]
pub fn icon_of(o: &NativeObject) -> Vec<IconLayer> {
    let NativeObject::Packed { bytes, .. } = o else {
        return Vec::new();
    };
    let w: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let n = w.first().copied().unwrap_or(0) as usize;
    w.get(1..)
        .unwrap_or(&[])
        .as_chunks::<3>()
        .0
        .iter()
        .take(n)
        .map(|l| IconLayer {
            layer: l[0],
            picture: DataId(l[1]),
            shift_color: Color32(l[2]),
        })
        .collect()
}

/// An icon description holding `layers`, in its packed form: the count, then each layer.
// D2-13884, D2-13895, D2-13896, D2-13911
#[must_use]
pub fn icon_native(layers: &[IconLayer]) -> NativeObject {
    let mut bytes = u32::try_from(layers.len())
        .unwrap_or(0)
        .to_le_bytes()
        .to_vec();
    for l in layers {
        bytes.extend(l.layer.to_le_bytes());
        bytes.extend(l.picture.0.to_le_bytes());
        bytes.extend(l.shift_color.0.to_le_bytes());
    }
    NativeObject::Packed {
        ty: ICON_DESC,
        bytes,
    }
}

/// A table of appearance tables' values a script holds (a new one is empty).
#[must_use]
pub fn appearance_of(o: &NativeObject) -> Keyed<Keyed<f32>> {
    let NativeObject::Packed { bytes, .. } = o else {
        return Keyed::default();
    };
    let mut r = Reader::new(bytes);
    let Ok(t) = Table::<Table<f32>>::read(&mut r) else {
        return Keyed::default();
    };
    Keyed::stored(
        buckets_of(t.size_class),
        t.entries.iter().map(|(k, v)| (*k, keyed(v))).collect(),
    )
}

/// A table of appearance tables' values, in its packed form.
#[must_use]
pub fn appearance_native(tables: &Keyed<Keyed<f32>>) -> NativeObject {
    let t = Table {
        size_class: size_class_of(tables.buckets),
        entries: tables.entries.iter().map(|(k, v)| (*k, table(v))).collect(),
    };
    let mut w = Writer::default();
    t.write(&mut w);
    NativeObject::Packed {
        ty: APP_INFO_HASH,
        bytes: w.finish(),
    }
}

/// The description a script holds as the receiver (`fp[-1]`), with its reference; `None` for the
/// null reference.
fn receiver_look(c: &mut Call<'_>) -> Result<Option<(u32, Look)>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let o = c.vm.heap.native(r).map_err(|e| Stop::Native {
        native,
        what: format!("{e:?}"),
    })?;
    let look = ScriptLook::of(o).ok_or_else(|| Stop::Native {
        native,
        what: format!("{r:#x} is not a visual description"),
    })?;
    Ok(Some((r, look)))
}

/// Keep `look` in the description a script holds at `r`.
fn put_look(c: &mut Call<'_>, r: u32, look: Look) -> Result<(), Stop> {
    *c.vm.heap.native_mut(r)? = ScriptLook::native(look);
    Ok(())
}

/// The object an out argument (`fp[k]`, a reference to a slot) holds.
fn out_object(c: &mut Call<'_>, k: i64) -> Result<u32, Stop> {
    let out = c.cx.at(k)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Err(Stop::NullReference);
    }
    Ok(target)
}

/// Run `f` over the data files' records (none when the host has no data files).
fn with_records<T>(c: &mut Call<'_>, f: impl FnOnce(&dyn LookRecords) -> T) -> Result<T, Stop> {
    let h = host(c)?;
    Ok(match database(h) {
        Some(db) => f(&Records(db)),
        None => f(&NoRecords),
    })
}

/// Empty the description: as a new one.
// D2-09022
fn native_clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, _)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    put_look(c, r, Look::default())?;
    Ok(Status::Success)
}

/// Whether the description adds nothing, into `fp[-2]`.
// D2-09028
fn native_adds_nothing(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    c.write(-2, u32::from(look.adds_nothing()), tag::MIXED)?;
    Ok(Status::Success)
}

/// Its scale, a new vector, into `fp[-2]`.
// D2-09031
fn native_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let v = look.scale;
    let r = c.vm.heap.add_native(
        VECTOR,
        NativeObject::Vector([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]),
    );
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Set its scale to the vector `fp[-2]`.
// D2-09023
fn native_set_scale(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let v = c.arg(0)?;
    let NativeObject::Vector(w) = c.vm.heap.native(v)? else {
        return Err(c.fail("the scale is not a vector"));
    };
    look.scale = Vec3::new(
        f32::from_bits(w[0]),
        f32::from_bits(w[1]),
        f32::from_bits(w[2]),
    );
    put_look(c, r, look)?;
    Ok(Status::Success)
}

/// Put the description `fp[-2]` in this one's place; whether it took, into `fp[-3]`.
// D2-09030
fn native_replace(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let other = look_arg(c, 0)?;
    let ok = look.replace(&other.to_desc()).is_ok();
    put_look(c, r, look)?;
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Add the description `fp[-2]` to this one; whether it took, into `fp[-3]`.
// D2-09026
fn native_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let other = look_arg(c, 0)?;
    let ok = look.apply_additive(&other.to_desc()).is_ok();
    put_look(c, r, look)?;
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Take the description `fp[-2]` out of this one; whether it could, into `fp[-3]`.
// D2-09027
fn native_take_out(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let other = look_arg(c, 0)?.to_desc();
    let ok = with_records(c, |records| look.apply_subtractive(records, &other))?;
    put_look(c, r, look)?;
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Its icon (its chain's, with its own changes' icon atoms laid over) into the icon description
/// an out argument (`fp[-2]`) holds; whether it has a layer, into `fp[-3]`.
// D2-09032
fn native_total_icon(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let target = out_object(c, -2)?;
    let desc = look.to_desc();
    let icon = with_records(c, |records| Look::total_icon_of(records, &desc))?;
    *c.vm.heap.native_mut(target)? = icon_native(&icon);
    c.write(-3, u32::from(!icon.is_empty()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The stored description `fp[-2]` names, as the client copies one into its own, into the
/// description an out argument (`fp[-3]`) holds; whether there is one, into `fp[-4]` (the
/// description is left as it was when there is none).
// D2-09033
fn native_from_record(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = DataId(c.cx.at(-2)?);
    let target = out_object(c, -3)?;
    let look = with_records(c, |records| Look::local_of_record(records, id))?;
    let found = look.is_some();
    if let Some(look) = look {
        put_look(c, target, look)?;
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

/// The appearance tables' values of group `fp[-2]` into the table an out argument (`fp[-3]`)
/// holds; whether the description has the group, into `fp[-4]` (the table is left as it was
/// when it has not).
// D2-09034
fn native_group_appearance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let group = c.cx.at(-2)?;
    let target = out_object(c, -3)?;
    let tables = look.appearance_of(group).cloned();
    let found = tables.is_some();
    if let Some(t) = tables {
        *c.vm.heap.native_mut(target)? = appearance_native(&t);
    }
    c.write(-4, u32::from(found), tag::MIXED)?;
    Ok(Status::Success)
}

/// Add the appearance tables' values `fp[-3]` to group `fp[-2]`; whether every table took, into
/// `fp[-4]`.
// D2-09024
fn native_add_group_appearance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let group = c.cx.at(-2)?;
    let t = c.cx.at(-3)?;
    if t == NULL {
        return Err(Stop::NullReference);
    }
    let tables = appearance_of(c.vm.heap.native(t)?);
    let ok = with_records(c, |records| {
        look.add_appearance_tables(records, group, &tables)
    })?;
    put_look(c, r, look)?;
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the icon description has no layer, into `fp[-2]`.
// D2-08606
fn native_icon_empty(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let empty = icon_of(c.vm.heap.native(r)?).is_empty();
    c.write(-2, u32::from(empty), tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether the icon description can be drawn, into `fp[-2]`.
// D2-08607
fn native_icon_drawable(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let valid = auberean_world::look::icon_is_valid(&icon_of(c.vm.heap.native(r)?));
    c.write(-2, u32::from(valid), tag::MIXED)?;
    Ok(Status::Success)
}

/// How many appearance tables the table holds, into `fp[-2]`.
// D2-09203
fn native_appearance_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let n = appearance_of(c.vm.heap.native(r)?).len();
    c.write(-2, u32::try_from(n).unwrap_or(u32::MAX), tag::MIXED)?;
    Ok(Status::Success)
}

/// An applied appearance value a script holds: its key and its strength (a new one has both 0).
#[must_use]
pub fn applied_key_of(o: &NativeObject) -> (u32, f32) {
    let NativeObject::Packed { bytes, .. } = o else {
        return (0, 0.0);
    };
    let w = |i: usize| {
        bytes
            .get(i * 4..i * 4 + 4)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    (w(0), f32::from_bits(w(1)))
}

/// An applied appearance value of key `key` and strength `strength`, in its packed form.
#[must_use]
pub fn applied_key_native(key: u32, strength: f32) -> NativeObject {
    let mut bytes = key.to_le_bytes().to_vec();
    bytes.extend(strength.to_bits().to_le_bytes());
    NativeObject::Packed {
        ty: APPLIED_KEY,
        bytes,
    }
}

/// The applied appearance value an argument (`fp[k]`) holds; a null one stops the run.
fn applied_key_arg(c: &mut Call<'_>, k: i64) -> Result<(u32, f32), Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    Ok(applied_key_of(c.vm.heap.native(r)?))
}

/// The applied appearance value's key (`word` 0) or strength (`word` 1), into `fp[-2]`.
fn applied_key_word(c: &mut Call<'_>, word: usize) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (key, strength) = applied_key_of(c.vm.heap.native(r)?);
    let v = if word == 0 { key } else { strength.to_bits() };
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Set the applied appearance value's key (`word` 0) or strength (`word` 1) to `fp[-2]`.
fn set_applied_key_word(c: &mut Call<'_>, word: usize) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = c.cx.at(-2)?;
    let (mut key, mut strength) = applied_key_of(c.vm.heap.native(r)?);
    if word == 0 {
        key = v;
    } else {
        strength = f32::from_bits(v);
    }
    *c.vm.heap.native_mut(r)? = applied_key_native(key, strength);
    Ok(Status::Success)
}

/// The applied appearance value's key, into `fp[-2]`.
// D2-18959
fn native_applied_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    applied_key_word(c, 0)
}

/// Set the applied appearance value's key to `fp[-2]`.
// D2-01370
fn native_set_applied_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_applied_key_word(c, 0)
}

/// The applied appearance value's strength, into `fp[-2]`.
// D2-08505
fn native_applied_strength(c: &mut Call<'_>) -> Result<Status, Stop> {
    applied_key_word(c, 1)
}

/// Set the applied appearance value's strength to `fp[-2]`.
// D2-08413
fn native_set_applied_strength(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_applied_key_word(c, 1)
}

/// Give group `fp[-2]` the applied appearance value `fp[-4]` of appearance table `fp[-3]`;
/// whether it took, into `fp[-5]`.
// D2-09025
fn native_add_appearance_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let group = c.cx.at(-2)?;
    let apr = c.cx.at(-3)?;
    let (key, scalar) = applied_key_arg(c, -4)?;
    let value = auberean_assets::fx::AppearanceKey { key, scalar };
    let ok = with_records(c, |records| {
        look.add_appearance_key(records, group, apr, value)
    })?;
    put_look(c, r, look)?;
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Take the key of the applied appearance value `fp[-4]` of appearance table `fp[-3]` off group
/// `fp[-2]`; whether it could, into `fp[-5]`.
// D2-09029
fn native_remove_appearance_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, mut look)) = receiver_look(c)? else {
        return Ok(Status::Crash);
    };
    let group = c.cx.at(-2)?;
    let apr = c.cx.at(-3)?;
    let (key, _) = applied_key_arg(c, -4)?;
    let ok = with_records(c, |records| {
        look.remove_appearance_key(records, group, apr, key)
    })?;
    put_look(c, r, look)?;
    c.write(-5, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    [
        (NativeId::new(VISUAL_DESC, 0x01), native_clear as NativeFn),
        (NativeId::new(VISUAL_DESC, 0x02), native_adds_nothing),
        (NativeId::new(VISUAL_DESC, 0x03), native_add_appearance_key),
        (
            NativeId::new(VISUAL_DESC, 0x04),
            native_remove_appearance_key,
        ),
        (NativeId::new(VISUAL_DESC, 0x05), native_replace),
        (NativeId::new(VISUAL_DESC, 0x06), native_add),
        (NativeId::new(VISUAL_DESC, 0x07), native_take_out),
        (NativeId::new(VISUAL_DESC, 0x08), native_scale),
        (NativeId::new(VISUAL_DESC, 0x09), native_set_scale),
        (NativeId::new(VISUAL_DESC, 0x0A), native_group_appearance),
        (
            NativeId::new(VISUAL_DESC, 0x0B),
            native_add_group_appearance,
        ),
        (NativeId::new(VISUAL_DESC, 0x0C), native_total_icon),
        (NativeId::new(VISUAL_DESC, 0x0D), native_from_record),
        (NativeId::new(ICON_DESC, 0x01), native_icon_empty),
        (NativeId::new(ICON_DESC, 0x02), native_icon_drawable),
        (NativeId::new(APP_INFO_HASH, 0x01), native_appearance_count),
        (NativeId::new(APPLIED_KEY, 0x01), native_applied_key),
        (NativeId::new(APPLIED_KEY, 0x02), native_set_applied_key),
        (NativeId::new(APPLIED_KEY, 0x03), native_applied_strength),
        (
            NativeId::new(APPLIED_KEY, 0x04),
            native_set_applied_strength,
        ),
        (
            NativeId::new(PHYSICS_OBJECT, 0x59),
            native_get_local_visual_desc,
        ),
        (NativeId::new(PHYSICS_OBJECT, 0x5C), native_init_visual_desc),
        (NativeId::new(PHYSICS_OBJECT, 0x5D), native_apply_additive),
        (
            NativeId::new(PHYSICS_OBJECT, 0x5E),
            native_apply_subtractive,
        ),
        (NativeId::new(PHYSICS_OBJECT, 0x5F), native_apply_to_body),
        (
            NativeId::new(PHYSICS_OBJECT, 0x61),
            native_has_motion_interpreter,
        ),
    ]
    .into_iter()
}
