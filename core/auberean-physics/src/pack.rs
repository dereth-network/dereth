//! The binary form a body's state is saved and sent in, written and read with the records' own
//! [`Writer`] and [`Reader`]: little-endian words and floats with no implicit padding, alignment to
//! four bytes counted from the start, 64-bit object ids, counted lists and keyed tables, places as
//! their cell, origin and rotation, and properties as the property system writes them.

pub use auberean_dat::{Reader, Writer};

use auberean_assets::Context;
use auberean_dat::reader::Result;
use dereth_primitives::shape::{BBox, Plane};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

/// A type with a binary form: written by [`Pack::pack`], read back by [`Pack::unpack`]. Reading a
/// property needs the install's property list, which `ctx` carries; nothing else reads it.
pub trait Pack: Sized {
    fn pack(&self, w: &mut Writer);
    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self>;
}

/// A place's binary size: its cell, origin and rotation.
pub const POSITION_SIZE: usize = 32;

/// Three floats.
pub fn write_vec3(w: &mut Writer, v: Vec3) {
    w.f32(v.x);
    w.f32(v.y);
    w.f32(v.z);
}

pub fn read_vec3(r: &mut Reader<'_>) -> Result<Vec3> {
    Ok(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
}

// D2-13185
pub fn write_position(w: &mut Writer, p: &Position) {
    w.u32(p.cell.0);
    let o = p.frame.origin;
    let q = p.frame.rotation;
    for v in [o.x, o.y, o.z, q.w, q.x, q.y, q.z] {
        w.f32(v);
    }
}

// D2-13186
pub fn read_position(r: &mut Reader<'_>) -> Result<Position> {
    let cell = CellId(r.u32()?);
    let origin = read_vec3(r)?;
    let rotation = Quat::new(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
    Ok(Position::new(cell, Frame::new(origin, rotation)))
}

/// A rotation's binary size.
pub const QUAT_SIZE: usize = 16;

/// A rotation: w, x, y, z.
// D2-13255
pub fn write_quat(w: &mut Writer, q: Quat) {
    for v in [q.w, q.x, q.y, q.z] {
        w.f32(v);
    }
}

/// A frame's binary size: its origin and its rotation.
pub const FRAME_SIZE: usize = 28;

/// A frame: its origin, then its rotation.
// D2-13156
pub fn write_frame(w: &mut Writer, f: &Frame) {
    write_vec3(w, f.origin);
    write_quat(w, f.rotation);
}

/// A box's binary size: its least corner, then its greatest.
// D2-13222
pub const BBOX_SIZE: usize = 24;

/// A box: its least corner, then its greatest.
pub fn write_bbox(w: &mut Writer, b: &BBox) {
    write_vec3(w, b.min);
    write_vec3(w, b.max);
}

/// A physics polygon's binary size: its id, its corner count and its corners' vertex numbers (16
/// bits each), brought to a multiple of four bytes.
#[must_use]
pub fn polygon_size(p: &auberean_assets::mesh::MeshPolygon) -> usize {
    (p.vertices.len() * 2 + 4).next_multiple_of(4)
}

/// A physics polygon: its id, its corner count, its corners' vertex numbers, zeros to the next
/// multiple of four bytes of the stream.
pub fn write_polygon(w: &mut Writer, p: &auberean_assets::mesh::MeshPolygon) {
    w.u16(p.id);
    #[allow(clippy::cast_possible_truncation)]
    w.u16(p.vertices.len() as u16);
    for &v in &p.vertices {
        w.u16(v);
    }
    while !w.pos().is_multiple_of(4) {
        w.u8(0);
    }
}

/// A plane's binary size.
// D2-13254
pub const PLANE_SIZE: usize = 16;

/// A plane: its normal, then its offset.
// D2-13134, D2-13139
pub fn write_plane(w: &mut Writer, p: &Plane) {
    write_vec3(w, p.normal);
    w.f32(p.d);
}

// D2-13030, D2-13629
pub fn read_plane(r: &mut Reader<'_>) -> Result<Plane> {
    Ok(Plane::new(read_vec3(r)?, r.f32()?))
}

/// A counted list of packed values.
pub fn write_packs<T: Pack>(w: &mut Writer, items: &[T]) {
    w.list(items, |w, i| i.pack(w));
}

pub fn read_packs<T: Pack>(r: &mut Reader<'_>, ctx: &Context) -> Result<Vec<T>> {
    r.list(4, |r| T::unpack(r, ctx))
}

/// A counted list of 64-bit object ids.
pub fn write_ids(w: &mut Writer, items: &[u64]) {
    w.list(items, |w, i| w.u64(*i));
}

// D2-03672
pub fn read_ids(r: &mut Reader<'_>) -> Result<Vec<u64>> {
    r.list(8, Reader::u64)
}

/// A keyed table's header word: the entry count in the low 24 bits; the top byte sizes the
/// table's buckets for the reader, and is written as the smallest size.
pub fn write_table_header(w: &mut Writer, n: usize) {
    w.u32(u32::try_from(n).unwrap_or(u32::MAX) & 0x00FF_FFFF);
}
