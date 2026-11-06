//! ac1999 object-create descriptors, normalized without reusing later wire layouts.

use dereth_primitives::ObjectId;

use dereth_protocol::{
    objects::ObjectCreatePayload,
    types::{
        physicsdesc::{flags as physics, ChildLink},
        AnimPartChange, ObjDesc, PhysicsDesc, PhysicsTimestamps, PositionWire, PublicWeenieDesc,
        Subpalette, TextureMapChange, Vec3,
    },
    Message, MessageError, Opcode, Reader, Writer,
};

use super::{invalid, read_string, write_string, MAX_NAME};

const PALETTE: u32 = 0x0400_0000;
const TEXTURE: u32 = 0x0500_0000;
const MODEL: u32 = 0x0100_0000;
const ICON: u32 = 0x0600_0000;
const VENDOR_CLASS: u32 = 0x0020_0000;
const PUBLIC_FIELDS: u32 = 0x00ff_ffbf;
/// The physics fields the November 1999 client reads. It has no translucency and no timestamp
/// flag; other bits are ignored.
const PHYSICS_FIELDS: u32 = 0x0003_ffff & !(physics::TIMESTAMPS);
/// Policy: the child links one object may list.
const MAX_CHILDREN: usize = 256;

/// The physics description as the November 1999 client reads it: the later order, without
/// translucency. Bits it does not read are dropped, and an animation frame is not read beside a
/// movement buffer, so the result is in the later shape with nothing it could misread.
fn read_physics(r: &mut Reader<'_>) -> Result<PhysicsDesc, MessageError> {
    let mut bitfield = r.u32()? & PHYSICS_FIELDS;
    if bitfield & physics::MOVEMENT != 0 {
        bitfield &= !physics::ANIMFRAME;
    }
    let mut d = PhysicsDesc {
        bitfield,
        state: r.u32()?,
        ..PhysicsDesc::default()
    };
    let has = |b: u32| bitfield & b != 0;
    if has(physics::MOVEMENT) {
        let len = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 movement length"))?;
        d.movement = Some(if len == 0 {
            (Vec::new(), 0)
        } else {
            (r.bytes(len)?.to_vec(), r.u32()?)
        });
    } else if has(physics::ANIMFRAME) {
        d.animframe_id = Some(r.u32()?);
    }
    if has(physics::POSITION) {
        d.position = Some(PositionWire::read(r)?);
    }
    macro_rules! word {
        ($bit:expr, $field:ident) => {
            if has($bit) {
                d.$field = Some(r.u32()?);
            }
        };
    }
    word!(physics::MTABLE, mtable_id);
    word!(physics::STABLE, stable_id);
    word!(physics::PETABLE, phstable_id);
    word!(physics::SETUP, setup_id);
    if has(physics::PARENT) {
        d.parent = Some((ObjectId(r.u32()?), r.u32()?));
    }
    if has(physics::CHILDREN) {
        let n = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 child count"))?;
        if n > MAX_CHILDREN || n * 8 > r.remaining() {
            return Err(invalid("ac1999 child count"));
        }
        let mut children = Vec::with_capacity(n);
        for _ in 0..n {
            children.push(ChildLink {
                child_id: ObjectId(r.u32()?),
                location_id: r.u32()?,
            });
        }
        d.children = Some(children);
    }
    if has(physics::OBJSCALE) {
        d.object_scale = Some(r.f32()?);
    }
    if has(physics::FRICTION) {
        d.friction = Some(r.f32()?);
    }
    if has(physics::ELASTICITY) {
        d.elasticity = Some(r.f32()?);
    }
    if has(physics::VELOCITY) {
        d.velocity = Some(Vec3::read(r)?);
    }
    if has(physics::ACCELERATION) {
        d.acceleration = Some(Vec3::read(r)?);
    }
    if has(physics::OMEGA) {
        d.omega = Some(Vec3::read(r)?);
    }
    word!(physics::DEFAULT_SCRIPT, default_script);
    if has(physics::DEFAULT_SCRIPT_INTENSITY) {
        d.default_script_intensity = Some(r.f32()?);
    }
    d.timestamps = PhysicsTimestamps::read(r)?;
    align(r)?;
    Ok(d)
}

/// Skip to the next four-byte boundary. The client does not look at the padding.
fn align(r: &mut Reader<'_>) -> Result<(), MessageError> {
    let n = r.blob_offset().wrapping_neg() & 3;
    r.bytes(n)?;
    Ok(())
}

fn asset(w: &mut Writer, id: u32, base: u32) -> Result<(), MessageError> {
    let offset = id
        .checked_sub(base)
        .and_then(|v| u16::try_from(v).ok())
        .ok_or(invalid("ac1999 object asset id"))?;
    w.u16(offset);
    Ok(())
}

fn read_appearance(r: &mut Reader<'_>) -> Result<ObjDesc, MessageError> {
    if r.u8()? != 0x11 {
        return Err(invalid("ac1999 appearance version"));
    }
    let palettes = r.u8()?;
    let textures = r.u8()?;
    let models = r.u8()?;
    let mut value = ObjDesc::default();
    if palettes != 0 {
        value.palette_id = PALETTE | u32::from(r.u16()?);
        for _ in 0..palettes {
            value.subpalettes.push(Subpalette {
                sub_id: PALETTE | u32::from(r.u16()?),
                offset: r.u8()?,
                num_colors: r.u8()?,
            });
        }
    }
    for _ in 0..textures {
        value.texture_changes.push(TextureMapChange {
            part_index: r.u8()?,
            old_tex_id: TEXTURE | u32::from(r.u16()?),
            new_tex_id: TEXTURE | u32::from(r.u16()?),
        });
    }
    for _ in 0..models {
        value.anim_part_changes.push(AnimPartChange {
            part_index: r.u8()?,
            part_id: MODEL | u32::from(r.u16()?),
        });
    }
    align(r)?;
    Ok(value)
}

fn write_appearance(w: &mut Writer, value: &ObjDesc) -> Result<(), MessageError> {
    w.u8(0x11);
    for n in [
        value.subpalettes.len(),
        value.texture_changes.len(),
        value.anim_part_changes.len(),
    ] {
        w.u8(u8::try_from(n).map_err(|_| invalid("ac1999 appearance count"))?);
    }
    if !value.subpalettes.is_empty() {
        asset(w, value.palette_id, PALETTE)?;
        for p in &value.subpalettes {
            asset(w, p.sub_id, PALETTE)?;
            w.u8(p.offset);
            w.u8(p.num_colors);
        }
    } else if value.palette_id != 0 {
        return Err(invalid("ac1999 absent palette"));
    }
    for p in &value.texture_changes {
        w.u8(p.part_index);
        asset(w, p.old_tex_id, TEXTURE)?;
        asset(w, p.new_tex_id, TEXTURE)?;
    }
    for p in &value.anim_part_changes {
        w.u8(p.part_index);
        asset(w, p.part_id, MODEL)?;
    }
    w.align4();
    Ok(())
}

/// `0xF625`: an object's new appearance, in the November 1999 client's form, normalized to the
/// later message: the appearance as in a create, then the instance and appearance stamps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjDescEvent(pub dereth_protocol::objects::ItemObjDescEvent);

impl Message for ObjDescEvent {
    const OPCODE: Opcode = Opcode(0xf625);

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = ObjectId(r.u32()?);
        if id.0 == 0 {
            return Err(invalid("ac1999 object identity"));
        }
        let objdesc = read_appearance(r)?;
        let timestamps = dereth_protocol::types::physicsdesc::PhysicsEventStamp::read(r)?;
        Ok(Self(dereth_protocol::objects::ItemObjDescEvent {
            id,
            objdesc,
            timestamps,
        }))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.0.id.0);
        write_appearance(w, &self.0.objdesc)?;
        self.0.timestamps.write(w);
        Ok(())
    }
}

/// The 1999 public description, as a create and a vendor's ware carry it, and the vendor class it
/// may name, as the ac1999 server writes them.
pub(crate) fn read_public_desc(
    r: &mut Reader<'_>,
) -> Result<(PublicWeenieDesc, Option<u16>), MessageError> {
    let header = r.u32()?;
    if header & !PUBLIC_FIELDS != 0 {
        return Err(invalid("ac1999 public fields"));
    }
    let mut d = PublicWeenieDesc {
        header: header & !VENDOR_CLASS,
        name: read_string(r, MAX_NAME)?,
        wcid: u32::from(r.u16()?),
        icon_id: ICON | u32::from(r.u16()?),
        obj_type: r.u32()?,
        bitfield: r.u32()?,
        ..Default::default()
    };
    if d.bitfield & 0xfc00_0000 != 0 {
        return Err(invalid("ac1999 object classification"));
    }
    macro_rules! field {
        ($bit:expr, $name:ident, $read:expr) => {
            if header & $bit != 0 {
                d.$name = Some($read);
            }
        };
    }
    field!(1, plural_name, read_string(r, MAX_NAME)?);
    field!(2, items_capacity, r.u8()?);
    field!(4, containers_capacity, r.u8()?);
    field!(8, value, r.u32()?);
    field!(0x10, useability, r.u32()?);
    field!(0x20, use_radius, r.f32()?);
    field!(0x80000, target_type, r.u32()?);
    field!(0x80, effects, r.u32()?);
    field!(0x100, ammo_type, u16::from(r.u8()?));
    field!(0x200, combat_use, r.u8()?);
    field!(0x400, structure, r.u16()?);
    field!(0x800, max_structure, r.u16()?);
    field!(0x1000, stack_size, r.u16()?);
    field!(0x2000, max_stack_size, r.u16()?);
    field!(0x4000, container_id, ObjectId(r.u32()?));
    field!(0x8000, wielder_id, ObjectId(r.u32()?));
    field!(0x10000, valid_locations, r.u32()?);
    field!(0x20000, location, r.u32()?);
    field!(0x40000, priority, r.u32()?);
    field!(0x100000, blip_color, r.u8()?);
    field!(0x800000, radar_enum, r.u8()?);
    let vendor_class = if header & VENDOR_CLASS != 0 {
        Some(r.u16()?)
    } else {
        None
    };
    field!(0x400000, spell_id, r.u16()?);
    align(r)?;
    Ok((d, vendor_class))
}

/// A typed early create. The trade-note vendor class is not the later burden field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreateObject {
    pub object: ObjectCreatePayload,
    pub vendor_class: Option<u16>,
}

impl Message for CreateObject {
    const OPCODE: Opcode = Opcode(0xf745);

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = ObjectId(r.u32()?);
        if id.0 == 0 {
            return Err(invalid("ac1999 object identity"));
        }
        let objdesc = read_appearance(r)?;
        let physicsdesc = read_physics(r)?;
        let (wdesc, vendor_class) = read_public_desc(r)?;
        r.expect_exhausted()?;
        let value = Self {
            object: ObjectCreatePayload {
                id,
                objdesc,
                physicsdesc,
                wdesc,
            },
            vendor_class,
        };
        value.validate()?;
        Ok(value)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.validate()?;
        w.u32(self.object.id.0);
        write_appearance(w, &self.object.objdesc)?;
        self.object.physicsdesc.write(w)?;
        let d = &self.object.wdesc;
        let header = d.header
            | if self.vendor_class.is_some() {
                VENDOR_CLASS
            } else {
                0
            };
        w.u32(header);
        write_string(w, &d.name, MAX_NAME)?;
        w.u16(u16::try_from(d.wcid).map_err(|_| invalid("ac1999 object class"))?);
        asset(w, d.icon_id, ICON)?;
        w.u32(d.obj_type);
        w.u32(d.bitfield);
        if let Some(text) = &d.plural_name {
            write_string(w, text, MAX_NAME)?;
        }
        macro_rules! field {
            ($name:ident, $write:ident) => {
                if let Some(v) = d.$name {
                    w.$write(v);
                }
            };
        }
        field!(items_capacity, u8);
        field!(containers_capacity, u8);
        field!(value, u32);
        field!(useability, u32);
        field!(use_radius, f32);
        field!(target_type, u32);
        field!(effects, u32);
        if let Some(v) = d.ammo_type {
            w.u8(u8::try_from(v).map_err(|_| invalid("ac1999 ammo width"))?);
        }
        field!(combat_use, u8);
        field!(structure, u16);
        field!(max_structure, u16);
        field!(stack_size, u16);
        field!(max_stack_size, u16);
        if let Some(v) = d.container_id {
            w.u32(v.0);
        }
        if let Some(v) = d.wielder_id {
            w.u32(v.0);
        }
        field!(valid_locations, u32);
        field!(location, u32);
        field!(priority, u32);
        field!(blip_color, u8);
        field!(radar_enum, u8);
        if let Some(v) = self.vendor_class {
            w.u16(v);
        }
        field!(spell_id, u16);
        w.align4();
        Ok(())
    }
}

impl CreateObject {
    fn validate(&self) -> Result<(), MessageError> {
        let p = &self.object.physicsdesc;
        let d = &self.object.wdesc;
        if self.object.id.0 == 0
            || p.bitfield & !PHYSICS_FIELDS != 0
            || p.translucency.is_some()
            || d.header & !(PUBLIC_FIELDS & !VENDOR_CLASS) != 0
            || d.bitfield & 0xfc00_0000 != 0
        {
            return Err(invalid("ac1999 object fields"));
        }
        for value in [
            p.object_scale,
            p.friction,
            p.elasticity,
            p.translucency,
            p.default_script_intensity,
            d.use_radius,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() {
                return Err(invalid("ac1999 object scalar"));
            }
        }
        for v in [
            p.velocity,
            p.acceleration,
            p.omega,
            p.position.map(|v| v.frame.origin),
        ]
        .into_iter()
        .flatten()
        {
            if ![v.x, v.y, v.z].iter().all(|v| v.is_finite()) {
                return Err(invalid("ac1999 object vector"));
            }
        }
        if let Some(position) = p.position {
            let q = position.frame.orientation;
            let values = [q.w, q.x, q.y, q.z];
            if !values.iter().all(|v| v.is_finite()) || values.iter().all(|&v| v == 0.0) {
                return Err(invalid("ac1999 object orientation"));
            }
        }
        // The canonical writer verifies optional-field gates by decoding its own bounded output.
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            self.object.clone(),
        ))?;
        let roundtrip =
            dereth_protocol::read_body::<dereth_protocol::objects::ItemCreateObject>(&body)?;
        if roundtrip.0 != self.object {
            return Err(invalid("ac1999 object field gates"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
