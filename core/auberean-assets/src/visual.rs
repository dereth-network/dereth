//! How objects look: visual descriptions (`.vd`), appearances (`.apr`) and scenes (`.scn`).
//!
//! A visual description puts an object together: a parent description it extends, its motion,
//! behavior and mode tables, its scale, its icon, and per part group the setup, animation map,
//! appearance keys and effects. An appearance table says what each appearance key does to the
//! parts (material modifiers, opacity, mesh and part-group replacements, effects, icon layers) at
//! each strength. A scene is the set of objects a landscape scatters over terrain, by distance.

use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader};
use auberean_primitives::{DataId, Vec3};

use crate::common::Color32;
use crate::fx::FxData;
use crate::{Context, Decode};

/// One layer of an icon: which layer, its picture, and a color it is shifted by.
// D2-13910, D2-13908, D2-03199, D2-14015, D2-03200
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IconLayer {
    pub layer: u32,
    pub picture: DataId,
    pub shift_color: Color32,
}

impl IconLayer {
    // D2-13911
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            layer: r.u32()?,
            picture: r.data_id()?,
            shift_color: Color32::read(r)?,
        })
    }
}

/// Appearance or effect keys with their strengths.
pub type KeyStrengths = Vec<(u32, f32)>;

/// A part group's description: where it attaches, how it is built and animated, what its
/// appearance keys are, and its effects.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PartGroup {
    /// Which fields are stored: `0x1` key, `0x2` parent key, `0x4` connection point, `0x8` setup,
    /// `0x10` animation map, `0x20` appearance keys, `0x40` effect table, `0x80` startup effects,
    /// `0x100` effect overrides.
    pub flags: u32,
    pub key: Option<u32>,
    pub parent_key: Option<u32>,
    pub connection_point: Option<u32>,
    pub setup: Option<DataId>,
    pub anim_map: Option<DataId>,
    /// Per appearance, its keys and their strengths.
    pub appearance_keys: Option<Vec<(u32, KeyStrengths)>>,
    pub fx_table: Option<DataId>,
    /// Effects started with the part group, and their strengths.
    pub startup_fx: Option<KeyStrengths>,
    /// Effects replaced for this part group.
    pub fx_overrides: Option<Vec<(u32, Vec<FxData>)>>,
}

fn key_scalars(r: &mut Reader<'_>) -> Result<KeyStrengths> {
    r.hash_table(8, |r| Ok((r.u32()?, r.f32()?)))
}

impl PartGroup {
    /// A flags word, then each flagged field in flag order.
    // D2-14041, D2-04492, D2-15662, D2-01473, D2-14055
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let flags = r.u32()?;
        r.ensure(flags & !0x1FF == 0, || {
            format!("part group flags {flags:#x}")
        })?;
        let has = |bit: u32| flags & bit != 0;
        Ok(Self {
            flags,
            key: if has(0x1) { Some(r.u32()?) } else { None },
            parent_key: if has(0x2) { Some(r.u32()?) } else { None },
            connection_point: if has(0x4) { Some(r.u32()?) } else { None },
            setup: if has(0x8) { Some(r.data_id()?) } else { None },
            anim_map: if has(0x10) { Some(r.data_id()?) } else { None },
            appearance_keys: if has(0x20) {
                Some(r.hash_table(8, |r| Ok((r.u32()?, key_scalars(r)?)))?)
            } else {
                None
            },
            fx_table: if has(0x40) { Some(r.data_id()?) } else { None },
            startup_fx: if has(0x80) {
                Some(key_scalars(r)?)
            } else {
                None
            },
            fx_overrides: if has(0x100) {
                Some(r.hash_table(8, |r| Ok((r.u32()?, FxData::read_list(r, ctx)?)))?)
            } else {
                None
            },
        })
    }
}

/// A visual description (`.vd`). Every field is optional: a flags word says which are stored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct VisualDesc {
    /// Which fields are stored: `0x1` id, `0x2` parent, `0x4` motion description, `0x8` behavior
    /// table, `0x10` valid modes, `0x20` scale, `0x40` child scale, `0x400` particle scale,
    /// `0x80` icon, `0x100` changes applied to every part group, `0x200` part groups.
    pub flags: u32,
    pub id: Option<u32>,
    pub parent: Option<DataId>,
    pub motion_desc: Option<DataId>,
    pub behavior_table: Option<DataId>,
    pub valid_modes: Option<DataId>,
    pub scale: Option<Vec3>,
    pub child_scale: Option<f32>,
    pub particle_scale: Option<f32>,
    pub icon: Option<Vec<IconLayer>>,
    pub global: Option<PartGroup>,
    /// Part groups by key.
    pub part_groups: Option<Vec<(u32, PartGroup)>>,
}

impl VisualDesc {
    /// The flags word, then each flagged field in this order: id, parent, motion description,
    /// behavior table, valid modes, scale, child scale, particle scale, icon (a counted list of
    /// layers), the changes for every part group, the part groups (a hash table by key).
    // D2-13814, D2-13884, D2-13898, D2-13835
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let flags = r.u32()?;
        r.ensure(flags & !0x7FF == 0, || {
            format!("visual description flags {flags:#x}")
        })?;
        let has = |bit: u32| flags & bit != 0;
        Ok(Self {
            flags,
            id: if has(0x1) { Some(r.u32()?) } else { None },
            parent: if has(0x2) { Some(r.data_id()?) } else { None },
            motion_desc: if has(0x4) { Some(r.data_id()?) } else { None },
            behavior_table: if has(0x8) { Some(r.data_id()?) } else { None },
            valid_modes: if has(0x10) { Some(r.data_id()?) } else { None },
            scale: if has(0x20) { Some(r.vec3()?) } else { None },
            child_scale: if has(0x40) { Some(r.f32()?) } else { None },
            particle_scale: if has(0x400) { Some(r.f32()?) } else { None },
            icon: if has(0x80) {
                Some(r.list(12, IconLayer::read)?)
            } else {
                None
            },
            global: if has(0x100) {
                Some(PartGroup::read(r, ctx)?)
            } else {
                None
            },
            part_groups: if has(0x200) {
                Some(r.hash_table(8, |r| Ok((r.u32()?, PartGroup::read(r, ctx)?)))?)
            } else {
                None
            },
        })
    }
}

impl Decode for VisualDesc {
    /// A standalone description stores its own id as its id field.
    // D2-13837, D2-13849, D2-05043, D2-13838, D2-13834, D2-13807
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        let v = Self::read(r, ctx)?;
        r.ensure(v.id == Some(id), || {
            format!("visual description id {:?}, filed as {id:08X}", v.id)
        })?;
        Ok(v)
    }
}

/// A material modifier applied to parts of a given mesh and material kind.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ModifierChange {
    pub priority: f32,
    pub template: DataId,
    pub modifier: DataId,
    pub mesh_kind: u32,
    pub material_kind: u32,
}

/// An opacity applied to the parts.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct OpacityChange {
    pub priority: f32,
    pub opacity: f32,
}

/// A mesh swapped in for the parts of a mesh kind.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MeshChange {
    pub priority: f32,
    pub mesh_kind: u32,
    pub mesh: DataId,
}

/// A part group description swapped in.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PartGroupChange {
    pub priority: f32,
    pub part_group: PartGroup,
}

/// Effects added: their ids and their descriptions.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxChange {
    pub ids: Vec<u32>,
    pub effects: Vec<FxData>,
}

/// Everything one appearance key does at one strength.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AppearanceChanges {
    pub intensity: f32,
    pub modifiers: Vec<ModifierChange>,
    pub opacities: Vec<OpacityChange>,
    pub meshes: Vec<MeshChange>,
    pub part_groups: Vec<PartGroupChange>,
    pub effects: Vec<FxChange>,
    pub icon_layers: Vec<IconLayer>,
}

impl AppearanceChanges {
    /// The intensity, then counted lists of material modifiers, opacities, mesh replacements,
    /// part-group replacements, effects and icon layers.
    // D2-13999, D2-14018, D2-14019, D2-14020, D2-14021, D2-13996, D2-13652, D2-03213, D2-03227, D2-03237, D2-03233, D2-03244
    fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            intensity: r.f32()?,
            modifiers: r.list(20, |r| {
                Ok(ModifierChange {
                    priority: r.f32()?,
                    template: r.data_id()?,
                    modifier: r.data_id()?,
                    mesh_kind: r.u32()?,
                    material_kind: r.u32()?,
                })
            })?,
            opacities: r.list(8, |r| {
                Ok(OpacityChange {
                    priority: r.f32()?,
                    opacity: r.f32()?,
                })
            })?,
            meshes: r.list(12, |r| {
                Ok(MeshChange {
                    priority: r.f32()?,
                    mesh_kind: r.u32()?,
                    mesh: r.data_id()?,
                })
            })?,
            part_groups: r.list(8, |r| {
                Ok(PartGroupChange {
                    priority: r.f32()?,
                    part_group: PartGroup::read(r, ctx)?,
                })
            })?,
            effects: r.list(8, |r| {
                Ok(FxChange {
                    ids: r.u32_list()?,
                    effects: FxData::read_list(r, ctx)?,
                })
            })?,
            icon_layers: r.list(12, IconLayer::read)?,
        })
    }
}

/// An appearance table (`.apr`): the base appearances, and what each appearance key does.
// D2-13960, D2-13961
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Appearance {
    pub base: Vec<u32>,
    /// Per appearance key, the changes at each strength step.
    pub keys: Vec<(u32, Vec<AppearanceChanges>)>,
    /// Per appearance key, how many strength steps it has.
    pub step_counts: Vec<(u32, i32)>,
}

impl Decode for Appearance {
    /// The id, a counted list of base appearances, a hash table from key to a counted list of
    /// changes, and a hash table from key to its step count.
    // D2-13928, D2-13942, D2-14066, D2-07314, D2-13953
    // D2-13944, D2-13947, D2-05042, D2-13943, D2-13946, D2-13945, D2-13932, D2-13934, D2-14069
    // D2-14070
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let base = r.u32_list()?;
        let keys = r.hash_table(8, |r| {
            Ok((r.u32()?, r.list(28, |r| AppearanceChanges::read(r, ctx))?))
        })?;
        let step_counts = r.hash_table(8, |r| Ok((r.u32()?, r.i32()?)))?;
        Ok(Self {
            base,
            keys,
            step_counts,
        })
    }
}

/// A group of objects a scene places, and the rules for where they may go.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScatterGroup {
    pub min_location: Vec3,
    pub max_location: Vec3,
    pub min_rotation: f32,
    pub max_rotation: f32,
    pub frequency: f32,
    pub min_scale: Vec3,
    pub max_scale: Vec3,
    pub scale_depth: f32,
    pub min_slope: f32,
    pub max_slope: f32,
    pub align: bool,
    pub orient: bool,
    pub min_above_dry_land: f32,
    pub max_above_dry_land: f32,
    pub min_above_water: f32,
    pub max_above_water: f32,
    pub min_below_water: f32,
    pub max_below_water: f32,
    pub min_water_depth: f32,
    pub max_water_depth: f32,
    pub min_below_ground: f32,
    pub max_below_ground: f32,
    pub clearance_radius: f32,
    pub check_slope_radius: f32,
    pub scale_check_radius: bool,
    pub min_slope_in_radius: f32,
    pub max_slope_in_radius: f32,
    pub align_max_heading_offset: f32,
    pub max_objects: u32,
    pub day_frequency: f32,
    pub scale_subobject_locations: bool,
    pub scale_subobject_sizes: bool,
    pub rotate_subobjects_for_location: bool,
    pub rotate_subobjects_for_orientation: bool,
    pub scale_clearance: bool,
    pub skip_position_check: bool,
    pub radius_in_3d: bool,
    pub choose_one: bool,
    pub ignore_clearance: bool,
    pub height_relative: u32,
    pub scale_type: u32,
    pub above_dry_land_type: u32,
    pub above_water_type: u32,
    pub below_water_type: u32,
    pub water_depth_type: u32,
    pub below_ground_type: u32,
    pub objects: Vec<SceneObject>,
}

// D2-12429, D2-12430, D2-12436, D2-12438
impl ScatterGroup {
    /// Every rule in the stored order, then a counted list of objects.
    // D2-12439
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            min_location: r.vec3()?,
            max_location: r.vec3()?,
            min_rotation: r.f32()?,
            max_rotation: r.f32()?,
            frequency: r.f32()?,
            min_scale: r.vec3()?,
            max_scale: r.vec3()?,
            scale_depth: r.f32()?,
            min_slope: r.f32()?,
            max_slope: r.f32()?,
            align: r.bool32()?,
            orient: r.bool32()?,
            min_above_dry_land: r.f32()?,
            max_above_dry_land: r.f32()?,
            min_above_water: r.f32()?,
            max_above_water: r.f32()?,
            min_below_water: r.f32()?,
            max_below_water: r.f32()?,
            min_water_depth: r.f32()?,
            max_water_depth: r.f32()?,
            min_below_ground: r.f32()?,
            max_below_ground: r.f32()?,
            clearance_radius: r.f32()?,
            check_slope_radius: r.f32()?,
            scale_check_radius: r.bool32()?,
            min_slope_in_radius: r.f32()?,
            max_slope_in_radius: r.f32()?,
            align_max_heading_offset: r.f32()?,
            max_objects: r.u32()?,
            day_frequency: r.f32()?,
            scale_subobject_locations: r.bool32()?,
            scale_subobject_sizes: r.bool32()?,
            rotate_subobjects_for_location: r.bool32()?,
            rotate_subobjects_for_orientation: r.bool32()?,
            scale_clearance: r.bool32()?,
            skip_position_check: r.bool32()?,
            radius_in_3d: r.bool32()?,
            choose_one: r.bool32()?,
            ignore_clearance: r.bool32()?,
            height_relative: r.u32()?,
            scale_type: r.u32()?,
            above_dry_land_type: r.u32()?,
            above_water_type: r.u32()?,
            below_water_type: r.u32()?,
            water_depth_type: r.u32()?,
            below_ground_type: r.u32()?,
            objects: r.list(8, SceneObject::read)?,
        })
    }
}

/// An object a scene places: a record by id (kinds 1 and 3), or a nested group (kind 2).
#[derive(Clone, Debug, PartialEq)]
pub enum SceneObject {
    Record { kind: u32, id: DataId },
    Group(Box<ScatterGroup>),
}

// D2-12440, D2-12445
impl SceneObject {
    /// A kind word, then an id (kinds 1 and 3) or a group (kind 2).
    // D2-12444
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        Ok(match kind {
            1 | 3 => Self::Record {
                kind,
                id: r.data_id()?,
            },
            2 => Self::Group(Box::new(ScatterGroup::read(r)?)),
            k => return r.fail(format!("scene object kind {k}")),
        })
    }
}

/// The objects a scene places at one level of detail, and the distances it applies at.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SceneLevel {
    pub min_distance: f32,
    pub ideal_distance: f32,
    pub max_distance: f32,
    pub objects: Vec<SceneObject>,
}

/// A scene (`.scn`): objects scattered over terrain, by level of detail.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Scene {
    pub version: u32,
    pub allow_rotate: bool,
    /// Objects placed only on clients, for decoration.
    pub decoration: Option<ScatterGroup>,
    pub levels: Vec<SceneLevel>,
}

// D2-12414, D2-12415, D2-12417, D2-12418, D2-12405, D2-12406, D2-12411, D2-05019, D2-12409, D2-15333, D2-12410
impl Decode for Scene {
    /// A version word **before** the id, the id, the rotate flag, a word saying a decoration group
    /// follows and the group, then a counted list of levels (three distances and a counted list
    /// of objects).
    // D2-12412, D2-12419
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        let allow_rotate = r.bool32()?;
        let decoration = if r.bool32()? {
            Some(ScatterGroup::read(r)?)
        } else {
            None
        };
        let levels = r.list(16, |r| {
            Ok(SceneLevel {
                min_distance: r.f32()?,
                ideal_distance: r.f32()?,
                max_distance: r.f32()?,
                objects: r.list(8, SceneObject::read)?,
            })
        })?;
        Ok(Self {
            version,
            allow_rotate,
            decoration,
            levels,
        })
    }
}

// ------------------------------------------------------------------- the records a scene names

impl SceneObject {
    /// An object's record, or everything a group names.
    // D2-12441
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        match self {
            Self::Record { id, .. } => {
                out.add(id.0);
            }
            Self::Group(g) => g.sub_ids(out),
        }
    }
}

impl ScatterGroup {
    /// Every member's records.
    // D2-12431
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for o in &self.objects {
            o.sub_ids(out);
        }
    }
}

impl SceneLevel {
    /// Every object's records.
    // D2-12416
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for o in &self.objects {
            o.sub_ids(out);
        }
    }
}

impl Scene {
    /// Every level's objects' records, then the decoration group's.
    // D2-12407
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for l in &self.levels {
            l.sub_ids(out);
        }
        if let Some(d) = &self.decoration {
            d.sub_ids(out);
        }
    }
}

// ------------------------------------------------- the records a description and an appearance name

impl IconLayer {
    /// Its picture.
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.picture.0);
    }
}

/// The records an icon names: each layer's picture.
// D2-13888
pub fn icon_sub_ids(layers: &[IconLayer], out: &mut QualifiedIds) {
    for l in layers {
        l.sub_ids(out);
    }
}

impl PartGroup {
    /// Its setup and animation map, each appearance table it names, its effect table, and the
    /// records of its effect overrides (unless the gathering leaves effects out).
    // D2-14043
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        for id in [self.setup, self.anim_map].into_iter().flatten() {
            out.add(id.0);
        }
        for (apr, _) in self.appearance_keys.iter().flatten() {
            out.add(*apr);
        }
        if let Some(t) = self.fx_table {
            out.add(t.0);
        }
        if let Some(o) = &self.fx_overrides {
            crate::fx::fx_map_sub_ids(o, gathering, out);
        }
    }
}

impl VisualDesc {
    /// Its parent description, motion description and behavior table, its valid modes (unless the
    /// gathering leaves effects out), the changes to every part group, each part group, and its
    /// icon's pictures.
    // D2-13806
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        for id in [self.parent, self.motion_desc, self.behavior_table]
            .into_iter()
            .flatten()
        {
            out.add(id.0);
        }
        if gathering != crate::fx::GATHER_WITHOUT_EFFECTS {
            if let Some(m) = self.valid_modes {
                out.add(m.0);
            }
        }
        if let Some(g) = &self.global {
            g.sub_ids(gathering, out);
        }
        for (_, g) in self.part_groups.iter().flatten() {
            g.sub_ids(gathering, out);
        }
        if let Some(icon) = &self.icon {
            icon_sub_ids(icon, out);
        }
    }
}

impl ModifierChange {
    /// Its modifier and its template.
    // D2-14003
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.modifier.0);
        out.add(self.template.0);
    }
}

impl OpacityChange {
    /// None: an opacity names no record.
    // D2-16787
    pub fn sub_ids(&self, _out: &mut QualifiedIds) {}
}

impl MeshChange {
    /// The mesh it swaps in.
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.mesh.0);
    }
}

impl PartGroupChange {
    /// The part group's records.
    // D2-14014
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        self.part_group.sub_ids(gathering, out);
    }
}

impl FxChange {
    /// Its effects' records, unless the gathering leaves effects out.
    // D2-14024
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        if gathering == crate::fx::GATHER_WITHOUT_EFFECTS {
            return;
        }
        for d in &self.effects {
            d.sub_ids(out);
        }
    }
}

impl AppearanceChanges {
    /// Every change's records: material modifiers, opacities, meshes, part groups, effects, then
    /// the icon layers' pictures.
    // D2-13973, D2-14017
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        for m in &self.modifiers {
            m.sub_ids(out);
        }
        for o in &self.opacities {
            o.sub_ids(out);
        }
        for m in &self.meshes {
            m.sub_ids(out);
        }
        for g in &self.part_groups {
            g.sub_ids(gathering, out);
        }
        for f in &self.effects {
            f.sub_ids(gathering, out);
        }
        for l in &self.icon_layers {
            l.sub_ids(out);
        }
    }
}

impl Appearance {
    /// Its base tables, then every key's change sets' records.
    // D2-13933, D2-14068
    pub fn sub_ids(&self, gathering: u32, out: &mut QualifiedIds) {
        out.add_ids(&self.base);
        for (_, sets) in &self.keys {
            for s in sets {
                s.sub_ids(gathering, out);
            }
        }
    }
}
