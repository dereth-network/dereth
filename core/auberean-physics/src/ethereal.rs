//! What a body passes through: a mask of the physics types it is ethereal to, its own lists of the
//! objects it passes through and of those it does not, and the state of a line-of-sight test that
//! carries both.
//!
//! A physics type is a flag property of the install's property list; the bit it takes in a mask is
//! the number its property descriptor carries.

use auberean_assets::property::PropertyList;
use auberean_assets::Context;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::{self, Ray};
use crate::pack::{read_ids, read_vec3, write_ids, write_vec3, Pack, Writer};

/// The physics type a body made solid only to its surroundings stays solid to.
pub const ENVIRONMENT: u32 = 0x4000_001B;
/// The water layer.
pub const WATER: u32 = 0x4000_001C;
/// The physics type the camera is ethereal to.
pub const CAMERA: u32 = 0x4000_0026;

/// The value type of a flag property.
const FLAG_KIND: u32 = 0x4000_0001;

/// The number a flag property's descriptor carries: its bit in a physics-type mask. Only a flag
/// property has one.
// D2-16228
#[must_use]
pub fn flag_bit(list: &PropertyList, property: u32) -> Option<u32> {
    let d = list.descriptor(property)?;
    (d.value_kind == FLAG_KIND).then_some(d.data)
}

/// The mask word of one bit as the client builds it: a 32-bit shift (the count taken modulo 32)
/// widened with its sign, so bit 31 also sets the whole upper word and bits 32 to 63 land on bits
/// 0 to 31.
fn bit32(bit: u32) -> u64 {
    #[allow(clippy::cast_sign_loss)]
    let w = i64::from(1i32.wrapping_shl(bit)) as u64;
    w
}

fn not_bit32(bit: u32) -> u64 {
    #[allow(clippy::cast_sign_loss)]
    let w = i64::from(!1i32.wrapping_shl(bit)) as u64;
    w
}

/// The physics types a body is ethereal to (or, for a body's own physics type, those it is), one
/// bit each.
// D2-15714
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicsTypes {
    pub mask: u64,
}

impl PhysicsTypes {
    #[must_use]
    pub const fn new(mask: u64) -> Self {
        Self { mask }
    }

    /// Sets or clears one bit; a bit past 63 is ignored. Retail builds the bit with a 32-bit shift
    /// widened with its sign (see [`bit32`]), kept here.
    // D2-05831
    pub fn set_bit(&mut self, bit: u32, on: bool) {
        if bit < 64 {
            if on {
                self.mask |= bit32(bit);
            } else {
                self.mask &= not_bit32(bit);
            }
        }
    }

    /// Sets or clears the bit of a flag property; false when the property is not one.
    // D2-15709
    pub fn set_property(&mut self, list: &PropertyList, property: u32, on: bool) -> bool {
        match flag_bit(list, property) {
            Some(bit) => {
                self.set_bit(bit, on);
                true
            }
            None => false,
        }
    }

    // D2-03612
    pub fn set_property_ethereal(&mut self, list: &PropertyList, property: u32) -> bool {
        self.set_property(list, property, true)
    }

    /// Takes on the lowest bit `other` has that this mask lacks, and only that one.
    // D2-15710
    pub fn update_ethereality(&mut self, other: &Self) {
        if let Some(bit) = (0..64).find(|b| other.mask & (1 << b) != 0 && self.mask & (1 << b) == 0)
        {
            self.mask |= 1 << bit;
        }
    }

    /// Ethereal to everything but the environment.
    // D2-15711
    pub fn set_solid_only_to_environment(&mut self, list: &PropertyList) {
        self.mask = u64::MAX;
        if let Some(bit) = flag_bit(list, ENVIRONMENT).filter(|b| *b < 64) {
            self.mask &= not_bit32(bit);
        }
    }

    /// Ethereal only to what the camera passes through.
    // D2-15712
    pub fn set_camera_ethereal(&mut self, list: &PropertyList) {
        self.mask = 0;
        if let Some(bit) = flag_bit(list, CAMERA).filter(|b| *b < 64) {
            self.mask |= bit32(bit);
        }
    }

    /// Retail sets the water bit when asked to pass through water, then clears it whatever was
    /// asked: the water layer always ends solid.
    // D2-15713
    pub fn set_ethereal_to_water_layer(&mut self, list: &PropertyList, ethereal: bool) {
        if ethereal {
            self.set_property_ethereal(list, WATER);
        }
        if let Some(bit) = flag_bit(list, WATER).filter(|b| *b < 64) {
            self.mask &= not_bit32(bit);
        }
    }
}

impl Pack for PhysicsTypes {
    fn pack(&self, w: &mut Writer) {
        w.u64(self.mask);
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self { mask: r.u64()? })
    }
}

/// Which objects a body passes through: a mask (all bits set for solid, none for ethereal) and its
/// own lists of objects it passes through and of objects it does not.
// D2-15700, D2-15701
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ObjectEthereality {
    pub mask: u64,
    pub ethereal: Vec<u64>,
    pub solid: Vec<u64>,
}

impl Clone for ObjectEthereality {
    // D2-03658
    fn clone(&self) -> Self {
        let mut c = Self::default();
        self.copy_into(&mut c);
        c
    }
}

impl ObjectEthereality {
    // D2-15695
    pub fn set_to_ethereal(&mut self) {
        self.mask = 0;
    }

    // D2-15696
    pub fn set_to_solid(&mut self) {
        self.mask = u64::MAX;
    }

    /// Whether a line of sight must test objects at all: not when ethereal to all with no exceptions.
    // D2-15697
    #[must_use]
    pub fn need_los_check(&self) -> bool {
        self.mask != 0 || !self.solid.is_empty()
    }

    // D2-15698
    #[must_use]
    pub fn find_in_ethereal_list(&self, id: u64) -> Option<usize> {
        self.ethereal.iter().position(|i| *i == id)
    }

    // D2-15699
    #[must_use]
    pub fn find_in_solid_list(&self, id: u64) -> Option<usize> {
        self.solid.iter().position(|i| *i == id)
    }

    // D2-15703
    #[must_use]
    pub fn in_ethereal_list(&self, id: u64) -> bool {
        self.find_in_ethereal_list(id).is_some()
    }

    // D2-15704
    #[must_use]
    pub fn in_solid_list(&self, id: u64) -> bool {
        self.find_in_solid_list(id).is_some()
    }

    /// Makes `to` this: its lists emptied, then the mask and both lists copied.
    // D2-15705
    pub fn copy_into(&self, to: &mut Self) {
        to.ethereal.clear();
        to.solid.clear();
        to.mask = self.mask;
        to.ethereal.extend_from_slice(&self.ethereal);
        to.solid.extend_from_slice(&self.solid);
    }

    // D2-15706
    pub fn add_to_ethereal_list(&mut self, id: u64) {
        if !self.in_ethereal_list(id) {
            self.ethereal.push(id);
        }
    }

    // D2-15707
    pub fn add_to_solid_list(&mut self, id: u64) {
        if !self.in_solid_list(id) {
            self.solid.push(id);
        }
    }

    /// Brings `to` up to this: with no lists here it becomes a copy; otherwise it takes the mask
    /// and gains this one's listed objects, keeping its own.
    // D2-15708
    pub fn update(&self, to: &mut Self) {
        if self.ethereal.is_empty() && self.solid.is_empty() {
            self.copy_into(to);
            return;
        }
        to.mask = self.mask;
        for id in &self.ethereal {
            to.add_to_ethereal_list(*id);
        }
        for id in &self.solid {
            to.add_to_solid_list(*id);
        }
    }
}

impl Pack for ObjectEthereality {
    // D2-15702
    fn pack(&self, w: &mut Writer) {
        w.u64(self.mask);
        write_ids(w, &self.ethereal);
        write_ids(w, &self.solid);
    }

    fn unpack(r: &mut Reader<'_>, _ctx: &Context) -> Result<Self> {
        Ok(Self {
            mask: r.u64()?,
            ethereal: read_ids(r)?,
            solid: read_ids(r)?,
        })
    }
}

fn at(cell: u32, origin: Vec3) -> Position {
    Position::new(CellId(cell), Frame::new(origin, Quat::IDENTITY))
}

/// A line-of-sight test's state: what it passes through, its first ray and cell, the ray and cell
/// it has reached, how far along the current ray it stopped, and the ray from the start to where it
/// stopped.
// D2-15687, D2-15688
#[derive(Debug, PartialEq)]
pub struct SightLine {
    pub objects: ObjectEthereality,
    pub physics: PhysicsTypes,
    /// What the test intersects: 0 the physics shapes.
    pub intersect: u32,
    pub traverse_portals: bool,
    pub checked_outside: bool,
    pub initial_cell: u32,
    pub initial_ray: Ray,
    // D2-03543
    pub current_ray: Ray,
    pub current_cell: u32,
    /// How far along the current ray the test stopped, 0 to 1.
    pub delta: f32,
    pub trace_result: u32,
    pub final_cell: u32,
    // D2-03542
    pub final_ray: Ray,
    // D2-03478
    pub impact_normal: Vec3,
    pub portal_index: u32,
    pub direction: u32,
    pub next_block: u32,
}

impl Default for SightLine {
    fn default() -> Self {
        Self {
            objects: ObjectEthereality::default(),
            physics: PhysicsTypes::default(),
            intersect: 0,
            traverse_portals: true,
            checked_outside: false,
            initial_cell: 0,
            initial_ray: Ray::default(),
            current_ray: Ray::default(),
            current_cell: 0,
            delta: 0.0,
            trace_result: 0,
            final_cell: 0,
            final_ray: Ray::default(),
            impact_normal: Vec3::ZERO,
            portal_index: u32::MAX,
            direction: 0,
            next_block: 0,
        }
    }
}

impl Clone for SightLine {
    // D2-15691, D2-04260
    fn clone(&self) -> Self {
        Self {
            objects: self.objects.clone(),
            ..*self
        }
    }
}

impl SightLine {
    // D2-15685
    pub fn set_intersect_type(&mut self, intersect: u32) {
        self.intersect = intersect;
    }

    /// Asks for water to block the line or not; as [`PhysicsTypes::set_ethereal_to_water_layer`]
    /// does, water ends up blocking either way.
    // D2-15686
    pub fn set_blocked_by_water(&mut self, list: &PropertyList, blocked: bool) {
        self.physics.set_ethereal_to_water_layer(list, !blocked);
    }

    // D2-15689
    pub fn set_los_params(&mut self, objects: &ObjectEthereality, physics: PhysicsTypes) {
        objects.copy_into(&mut self.objects);
        self.physics = physics;
    }

    /// Starts at `ray` in `cell`.
    // D2-15692
    pub fn set_ray(&mut self, ray: Ray, cell: u32) {
        self.initial_ray = ray;
        self.initial_cell = cell;
        self.current_cell = cell;
    }

    /// Back to the start: the first ray and cell, the whole ray to go, nothing checked outside.
    // D2-15693
    pub fn reset_state(&mut self) {
        self.current_ray = self.initial_ray;
        self.current_cell = self.initial_cell;
        self.delta = 1.0;
        self.checked_outside = false;
        self.next_block = 0;
    }

    /// The ray from the start to where the current ray stopped. Fails, leaving the final ray, when
    /// one of the start and final cells is unset and the other is not.
    // D2-15694
    pub fn calc_final_ray(&mut self) -> bool {
        let (c, d, l) = (
            self.current_ray.point,
            self.current_ray.dir,
            self.current_ray.length,
        );
        let k = self.delta;
        let end = at(
            self.final_cell,
            Vec3::new(k * d.x * l + c.x, k * d.y * l + c.y, k * d.z * l + c.z),
        );
        let start = at(self.initial_cell, self.initial_ray.point);
        if (self.initial_cell == 0) != (self.final_cell == 0) {
            return false;
        }
        let offset = geom::offset(&start, &end);
        self.final_ray = Ray::from_offset(self.initial_ray.point, offset);
        true
    }

    /// Where the final ray ends, in the start cell's landblock frame.
    // D2-06112
    #[must_use]
    pub fn end_point(&self) -> Vec3 {
        let r = &self.final_ray;
        let l = r.length;
        let b = geom::block_offset(CellId(self.initial_cell), CellId(self.final_cell));
        Vec3::new(
            (l * r.dir.x + r.point.x) - b.x,
            (l * r.dir.y + r.point.y) - b.y,
            (l * r.dir.z + r.point.z) - b.z,
        )
    }

    /// Starts at `from` and runs along `offset`.
    pub fn set_from_position_and_offset(&mut self, from: &Position, offset: Vec3) {
        self.set_ray(Ray::from_offset(from.frame.origin, offset), from.cell.0);
    }

    /// Starts at `from` and runs to `to`.
    // D2-08609
    pub fn set_from_begin_and_end_position(&mut self, from: &Position, to: &Position) {
        let offset = geom::offset(from, to);
        self.set_ray(Ray::from_offset(from.frame.origin, offset), from.cell.0);
    }
}

impl Pack for SightLine {
    // D2-15690
    fn pack(&self, w: &mut Writer) {
        self.objects.pack(w);
        self.physics.pack(w);
        w.u32(self.intersect);
        w.bool32(self.traverse_portals);
        w.bool32(self.checked_outside);
        w.u32(self.initial_cell);
        self.initial_ray.pack(w);
        self.current_ray.pack(w);
        w.u32(self.current_cell);
        w.f32(self.delta);
        w.u32(self.trace_result);
        w.u32(self.final_cell);
        self.final_ray.pack(w);
        write_vec3(w, self.impact_normal);
        w.u32(self.portal_index);
        w.u32(self.direction);
        w.u32(self.next_block);
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            objects: ObjectEthereality::unpack(r, ctx)?,
            physics: PhysicsTypes::unpack(r, ctx)?,
            intersect: r.u32()?,
            traverse_portals: r.bool32()?,
            checked_outside: r.bool32()?,
            initial_cell: r.u32()?,
            initial_ray: Ray::unpack(r, ctx)?,
            current_ray: Ray::unpack(r, ctx)?,
            current_cell: r.u32()?,
            delta: r.f32()?,
            trace_result: r.u32()?,
            final_cell: r.u32()?,
            final_ray: Ray::unpack(r, ctx)?,
            impact_normal: read_vec3(r)?,
            portal_index: r.u32()?,
            direction: r.u32()?,
            next_block: r.u32()?,
        })
    }
}
