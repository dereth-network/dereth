//! Entities: the placed things of the world (`.entity`, `0x47000000`+), and groups of them with
//! the links between them (`.entitygroup`, `0x46000000`+, also embedded in cells and landblock
//! infos).
//!
//! An entity's object field names what it is made from: a visual description, a script package
//! (a number below `0x8000`), or another entity, its parent. A derived entity's properties are its
//! parent's (recursively) updated with its own ([`Entity::total_properties`]); a property query on
//! the entity itself ([`Entity::inq_integer`] and the rest) answers from its own properties only.

use std::sync::Arc;

use auberean_dat::qualified::divine_type;
use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader, Writer};
use auberean_primitives::{DataId, Frame, Vec3};

use crate::common::{Position, Waveform};
use crate::property::{Property, PropertyCollection, PropertyList, PropertyValue, StringInfo};
use crate::{Context, Decode, Encode};

/// The record type of an entity.
const ENTITY_TYPE: u32 = 0x4000_0049;
/// The record type of a visual description.
const VISUAL_DESC_TYPE: u32 = 0x4000_0021;

/// How many entities deep a chain is followed (the game follows it without a bound; no retail
/// chain is near this deep, and a loop in edited data must end).
pub const CHAIN_LIMIT: u32 = 64;

/// Finds an entity record by id, for the walks along an entity's chain.
pub type EntityLookup<'a> = &'a dyn Fn(u32) -> Option<Arc<Entity>>;

/// A frame as an entity stores it: origin, rotation, and the 3 x 3 rotation matrix the writer had
/// cached (64 bytes in all).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct StoredFrame {
    pub frame: Frame,
    pub matrix: [f32; 9],
}

impl StoredFrame {
    // D2-13005
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let frame = r.frame()?;
        let mut matrix = [0f32; 9];
        for m in &mut matrix {
            *m = r.f32()?;
        }
        Ok(Self { frame, matrix })
    }

    pub fn write(&self, w: &mut Writer) {
        w.frame(self.frame);
        for m in self.matrix {
            w.f32(m);
        }
    }
}

/// One placed thing. Every field but the flags is optional: a flags word says which follow.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Entity {
    /// Which fields are stored: `0x1` id, `0x2` kind, `0x4` instance, `0x8` object, `0x10`
    /// offset, `0x20` scale, `0x80` properties, `0x40` version, `0x100` name.
    pub flags: u32,
    pub id: Option<u32>,
    pub kind: Option<u32>,
    /// The instance id the world gives it.
    pub instance: Option<u64>,
    /// What it is made from: a visual description id (`0x1F...`) in every standalone entity, a
    /// parent entity, or a script package.
    pub object: Option<u32>,
    pub offset: Option<StoredFrame>,
    pub scale: Option<Vec3>,
    pub properties: Option<PropertyCollection>,
    pub version: Option<u32>,
    pub name: Option<String>,
}

impl Entity {
    /// The flags word, then each flagged field in this order: id, kind, instance (8 bytes),
    /// object, offset (64 bytes), scale, properties, version, name (a narrow string).
    // D2-13026
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let flags = r.u32()?;
        r.ensure(flags & !0x1FF == 0, || format!("entity flags {flags:#x}"))?;
        let has = |bit: u32| flags & bit != 0;
        Ok(Self {
            flags,
            id: if has(0x1) { Some(r.u32()?) } else { None },
            kind: if has(0x2) { Some(r.u32()?) } else { None },
            instance: if has(0x4) { Some(r.u64()?) } else { None },
            object: if has(0x8) { Some(r.u32()?) } else { None },
            offset: if has(0x10) {
                Some(StoredFrame::read(r)?)
            } else {
                None
            },
            scale: if has(0x20) { Some(r.vec3()?) } else { None },
            properties: if has(0x80) {
                Some(PropertyCollection::read(r, ctx)?)
            } else {
                None
            },
            version: if has(0x40) { Some(r.u32()?) } else { None },
            name: if has(0x100) {
                Some(r.narrow_string()?)
            } else {
                None
            },
        })
    }

    /// The flags an entity is written with, from what it holds: its id, kind and object when not
    /// 0, its instance when it has one, its offset when it is not the identity and its scale when
    /// it is not 1 (either by 0.0002 or more in any part), its properties when it has a group, its
    /// version when not 0, and its name when not empty.
    // D2-13004
    #[must_use]
    pub fn pack_flags(&self) -> u32 {
        const NEAR: f32 = 0.0002;
        let set = |v: Option<u32>| v.is_some_and(|v| v != 0);
        let mut f = 0;
        if set(self.id) {
            f |= 0x1;
        }
        if set(self.kind) {
            f |= 0x2;
        }
        if self.instance.is_some_and(|i| i != 0) {
            f |= 0x4;
        }
        if set(self.object) {
            f |= 0x8;
        }
        if let Some(o) = &self.offset {
            let (p, q) = (o.frame.origin, o.frame.rotation);
            let identity = [p.x, p.y, p.z, q.w - 1.0, q.x, q.y, q.z]
                .iter()
                .all(|d| d.abs() < NEAR);
            if !identity {
                f |= 0x10;
            }
        }
        if let Some(s) = self.scale {
            if [s.x, s.y, s.z].iter().any(|c| (c - 1.0).abs() >= NEAR) {
                f |= 0x20;
            }
        }
        if self
            .properties
            .as_ref()
            .is_some_and(|p| !p.groups.is_empty())
        {
            f |= 0x80;
        }
        if set(self.version) {
            f |= 0x40;
        }
        if self.name.as_ref().is_some_and(|n| !n.is_empty()) {
            f |= 0x100;
        }
        f
    }

    /// Write the entity as [`Entity::read`] reads it: the flags it is written with
    /// ([`Entity::pack_flags`]), then each field they name.
    // D2-13026
    pub fn write(&self, w: &mut Writer) {
        let flags = self.pack_flags();
        w.u32(flags);
        let has = |bit: u32| flags & bit != 0;
        if has(0x1) {
            w.u32(self.id.unwrap_or(0));
        }
        if has(0x2) {
            w.u32(self.kind.unwrap_or(0));
        }
        if has(0x4) {
            w.u64(self.instance.unwrap_or(0));
        }
        if has(0x8) {
            w.u32(self.object.unwrap_or(0));
        }
        if has(0x10) {
            self.offset.unwrap_or_default().write(w);
        }
        if has(0x20) {
            w.vec3(self.scale.unwrap_or_default());
        }
        if has(0x80) {
            match &self.properties {
                Some(p) => p.write(w),
                None => w.count(0),
            }
        }
        if has(0x40) {
            w.u32(self.version.unwrap_or(0));
        }
        if has(0x100) {
            w.narrow_string(self.name.as_deref().unwrap_or(""));
        }
    }

    /// Replace the entity's properties.
    // D2-12994
    pub fn set_properties(&mut self, p: PropertyCollection) {
        self.properties = Some(p);
    }

    /// One of the entity's own stored properties.
    fn property(&self, list: &PropertyList, id: u32) -> Option<&Property> {
        self.properties.as_ref()?.get(list, id)
    }

    /// A truth-value property of the entity's own (not its parent's); `None` when the entity
    /// has none, or the property is of another type. The other queries are alike.
    // D2-12995
    #[must_use]
    pub fn inq_bool(&self, list: &PropertyList, id: u32) -> Option<bool> {
        self.property(list, id)?.value.inq_bool()
    }

    // D2-12996
    #[must_use]
    pub fn inq_integer(&self, list: &PropertyList, id: u32) -> Option<i32> {
        self.property(list, id)?.value.inq_integer()
    }

    // D2-12997
    #[must_use]
    pub fn inq_float(&self, list: &PropertyList, id: u32) -> Option<f32> {
        self.property(list, id)?.value.inq_float()
    }

    // D2-12998
    #[must_use]
    pub fn inq_vector(&self, list: &PropertyList, id: u32) -> Option<Vec3> {
        self.property(list, id)?.value.inq_vector()
    }

    // D2-12999
    #[must_use]
    pub fn inq_enum(&self, list: &PropertyList, id: u32) -> Option<u32> {
        self.property(list, id)?.value.inq_enum()
    }

    // D2-13000
    #[must_use]
    pub fn inq_data_file(&self, list: &PropertyList, id: u32) -> Option<DataId> {
        self.property(list, id)?.value.inq_data_file()
    }

    // D2-13001
    #[must_use]
    pub fn inq_package(&self, list: &PropertyList, id: u32) -> Option<u32> {
        self.property(list, id)?.value.inq_package()
    }

    // D2-13002
    #[must_use]
    pub fn inq_string_info(&self, list: &PropertyList, id: u32) -> Option<&StringInfo> {
        self.property(list, id)?
            .value
            .inq_string_info()
            .map(|(i, _)| i)
    }

    // D2-13016
    #[must_use]
    pub fn inq_string(&self, list: &PropertyList, id: u32) -> Option<&str> {
        self.property(list, id)?.string_value()
    }

    #[must_use]
    pub fn inq_waveform(&self, list: &PropertyList, id: u32) -> Option<&Waveform> {
        self.property(list, id)?.value.inq_waveform()
    }

    #[must_use]
    pub fn inq_position(&self, list: &PropertyList, id: u32) -> Option<&Position> {
        self.property(list, id)?.value.inq_position()
    }

    /// A property's value, as stored.
    #[must_use]
    pub fn inq_value(&self, list: &PropertyList, id: u32) -> Option<&PropertyValue> {
        Some(&self.property(list, id)?.value)
    }

    /// The records the entity names: its object when that is a record id (not a script package
    /// number, which is below `0x8000`), and what its properties name.
    // D2-13003
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        if let Some(o) = self.object.filter(|o| *o > 0x7FFF) {
            out.add(o);
        }
        if let Some(p) = &self.properties {
            p.sub_ids(out);
        }
    }

    /// The entity this one's object names, when it names an entity.
    fn parent(&self, entity: EntityLookup<'_>) -> Option<Arc<Entity>> {
        let o = self.object.filter(|o| *o != 0)?;
        (divine_type(o) == ENTITY_TYPE).then(|| entity(o)).flatten()
    }

    /// The entity's properties with its ancestors' beneath them: an entity whose object is
    /// another entity starts from that one's (recursively) and updates them with its own.
    // D2-13020
    #[must_use]
    pub fn total_properties(&self, entity: EntityLookup<'_>) -> PropertyCollection {
        let mut total = PropertyCollection::default();
        self.add_total_properties(entity, &mut total, 0);
        total
    }

    fn add_total_properties(
        &self,
        entity: EntityLookup<'_>,
        out: &mut PropertyCollection,
        depth: u32,
    ) {
        if depth < CHAIN_LIMIT {
            if let Some(parent) = self.parent(entity) {
                parent.add_total_properties(entity, out, depth + 1);
            }
        }
        if let Some(p) = &self.properties {
            out.update_collection(p);
        }
    }

    /// The visual description the entity shows as: its object when that is one, else its
    /// parent entity's, else none (0).
    // D2-13021
    #[must_use]
    pub fn visual_desc(&self, entity: EntityLookup<'_>) -> u32 {
        let mut held: Option<Arc<Entity>> = None;
        for _ in 0..CHAIN_LIMIT {
            let o = held.as_deref().unwrap_or(self).object.unwrap_or(0);
            match divine_type(o) {
                VISUAL_DESC_TYPE => return o,
                ENTITY_TYPE => match entity(o) {
                    Some(p) => held = Some(p),
                    None => return 0,
                },
                _ => return 0,
            }
        }
        0
    }
}

impl Decode for Entity {
    /// A standalone entity stores its own id as its id field.
    // D2-13007, D2-13023, D2-05015, D2-13006, D2-13024, D2-13009, D2-13018, D2-13017
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        let e = Self::read(r, ctx)?;
        r.ensure(e.id == Some(id), || {
            format!("entity id {:?}, filed as {id:08X}", e.id)
        })?;
        Ok(e)
    }
}

/// One end of a link between entities: an instance id and a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LinkEnd {
    pub instance: u64,
    pub kind: u32,
    /// The stored end's last word, which carries nothing.
    pub unused: u32,
}

impl LinkEnd {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            instance: r.u64()?,
            kind: r.u32()?,
            unused: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u64(self.instance);
        w.u32(self.kind);
        w.u32(self.unused);
    }
}

/// A named link between two entities of a group, with properties.
// D2-13031, D2-03562, D2-03741
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Link {
    pub name: String,
    pub input: LinkEnd,
    pub output: LinkEnd,
    pub properties: PropertyCollection,
}

impl Link {
    /// The instance at the other end from `id`: the output's when `id` is the input's, the
    /// input's when it is the output's, else 0 (no instance).
    // D2-13029
    #[must_use]
    pub fn opposite(&self, id: u64) -> u64 {
        if self.input.instance == id {
            self.output.instance
        } else if self.output.instance == id {
            self.input.instance
        } else {
            0
        }
    }

    /// The text of the link's string property `id`, as an entity's is read.
    // D2-13033
    #[must_use]
    pub fn inq_string(&self, list: &PropertyList, id: u32) -> Option<&str> {
        self.properties.get(list, id)?.string_value()
    }

    // D2-13034
    fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            name: r.narrow_string()?,
            input: LinkEnd::read(r)?,
            output: LinkEnd::read(r)?,
            properties: PropertyCollection::read(r, ctx)?,
        })
    }

    // D2-13034
    fn write(&self, w: &mut Writer) {
        w.narrow_string(&self.name);
        self.input.write(w);
        self.output.write(w);
        self.properties.write(w);
    }
}

/// A group of entities and the links between them.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EntityGroup {
    /// The group's own id word (0 when embedded in a cell or landblock info).
    pub id: u32,
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

impl EntityGroup {
    /// The group's id word, a counted list of entities, a counted list of links.
    // D2-13035, D2-10461, D2-13043
    pub fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let id = r.u32()?;
        let entities = r.list(4, |r| Entity::read(r, ctx))?;
        let links = r.list(40, |r| Link::read(r, ctx))?;
        Ok(Self {
            id,
            entities,
            links,
        })
    }

    /// Write the group as [`EntityGroup::read`] reads it.
    // D2-13035, D2-10461, D2-13043
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.id);
        w.list(&self.entities, |w, e| e.write(w));
        w.list(&self.links, |w, l| l.write(w));
    }

    /// The records the group's entities name.
    // D2-13036
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for e in &self.entities {
            e.sub_ids(out);
        }
    }
}

impl Encode for Entity {
    fn encode(&self, _id: u32, w: &mut Writer) {
        self.write(w);
    }
}

impl Encode for EntityGroup {
    fn encode(&self, _id: u32, w: &mut Writer) {
        self.write(w);
    }
}

impl Decode for EntityGroup {
    // D2-13040, D2-13044, D2-05016, D2-13039, D2-13041, D2-13037, D2-13046
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        let g = Self::read(r, ctx)?;
        r.ensure(g.id == id, || {
            format!("group id {:08X}, filed as {id:08X}", g.id)
        })?;
        Ok(g)
    }
}
