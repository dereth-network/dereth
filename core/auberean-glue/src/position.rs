//! Positions: where a place is, how far apart two are, and the place written as text. Vectors: set
//! whole, each part read, and their arithmetic.
//!
//! A vector's parts are set from three floats (x, y, z below the receiver) through the engine's
//! floating-point unit, which quiets a signalling not-a-number; reading a part answers its float
//! as stored.
//!
//! The arithmetic works at double precision, each result kept as a float: the cross product (a
//! new vector), the dot product, the length (the squares summed and rooted); adding or taking
//! away another vector part by part, in place; a copy of another's parts as they are. Two vectors
//! are equal when every part differs by less than 0.0002 (as a float). The squared length answers
//! nothing: the client binds it to a function that does nothing.
//!
//! A position is a cell and a frame within the cell's landblock. The offset from one to another
//! adds the landblocks' offset (160 units a landblock along each axis, by the landblock's column and
//! row in the cell id's top bytes; none within one landblock, and none between two cells of id 0)
//! to the difference of the origins, at double precision, each axis kept as a float. The distance
//! is the offset's length, its squares summed and rooted at double precision and kept as a float;
//! the squared distance and the distance across the ground likewise.
//!
//! A position's parts are read and set whole: its cell, its origin, its height, its frame copied
//! from another. Two positions are equal when their cells are and every part of their frames
//! differs by less than 0.0002. Where it is on the land comes from the region's size (the host's
//! data): whether it is valid (a cell of the world and a frame whose rotation is a unit), whether
//! it is outside, its land cell's coordinates, the distance between two land cells, and its map
//! coordinates written as the client writes them for a player (`12.3N, 45.6W`, or a string
//! reference to the place line's table: north or south, east or west). Before the region is known
//! no cell is valid.
use auberean_primitives::land;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{
    native_type, NativeObject, TextInfo, Variable, VariableValue,
};
use auberean_script::value::{tag, NULL};

use crate::machine::{new_text, text_of};

const POSITION: u32 = 0x5A;
const VECTOR: u32 = 0x70;

/// The places table's enumeration value and mapping.
const PLACES_ENUM: u32 = 0x4000_0054;
const PLACES_MAPPING: u32 = 0x4000_0002;

/// The region record the land's size is read from.
const REGION: u32 = 0x0F00_0000;

/// A land cell's side.
const SQUARE_LENGTH: f64 = 20.0;

/// A landblock's length along each axis.
const BLOCK_LENGTH: f32 = 160.0;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (POSITION, 0x01, get_offset),
    (POSITION, 0x02, get_origin),
    (POSITION, 0x03, get_cell),
    (POSITION, 0x04, set_cell),
    (POSITION, 0x05, in_same_block),
    (POSITION, 0x06, block_id),
    (POSITION, 0x07, get_block_offset),
    (POSITION, 0x08, set_origin),
    (POSITION, 0x09, not_in_world),
    (POSITION, 0x0A, distance),
    (POSITION, 0x0B, distance_squared),
    (POSITION, 0x0E, set_cell),
    (POSITION, 0x0F, from_string),
    (POSITION, 0x10, to_string),
    (POSITION, 0x13, to_real_map_coords),
    (POSITION, 0x14, to_real_map_coords_info),
    (POSITION, 0x15, from_real_map_coords),
    (POSITION, 0x16, is_outside),
    (POSITION, 0x17, position_equals),
    (POSITION, 0x18, null_position),
    (POSITION, 0x19, add_local_offset),
    (POSITION, 0x1A, add_global_offset),
    (POSITION, 0x1C, set_position),
    (POSITION, 0x1D, set_height),
    (POSITION, 0x1E, get_height),
    (POSITION, 0x21, is_valid),
    (POSITION, 0x22, xy_distance),
    (POSITION, 0x23, xy_distance_squared),
    (POSITION, 0x24, cell_distance),
    (POSITION, 0x25, cell_distance_squared),
    (POSITION, 0x26, cylinder_distance),
    (POSITION, 0x27, to_cell_coords_string),
    (POSITION, 0x28, from_cell_coords_string),
    (POSITION, 0x29, cell_x),
    (POSITION, 0x2A, cell_y),
    (POSITION, 0x2B, from_cell_coords),
    (0x58, 0x01, inq_place),
    (0x58, 0x02, inq_place_str),
    (0x58, 0x03, places_dump),
    (VECTOR, 0x01, cross),
    (VECTOR, 0x02, dot),
    (VECTOR, 0x03, magnitude),
    (VECTOR, 0x04, does_nothing),
    (VECTOR, 0x05, normalize),
    (VECTOR, 0x06, set_vector),
    (VECTOR, 0x07, add),
    (VECTOR, 0x08, subtract),
    (VECTOR, 0x0A, copy),
    (VECTOR, 0x0B, equals),
    (VECTOR, 0x0C, vector_part::<0>),
    (VECTOR, 0x0D, vector_part::<1>),
    (VECTOR, 0x0E, vector_part::<2>),
    (VECTOR, 0x0F, vector_string),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The offset from one landblock to another's.
// D2-01968
pub(crate) fn block_offset(from: u32, to: u32) -> [f32; 2] {
    if (from == 0 && to == 0) || (from != 0 && to != 0 && from >> 16 == to >> 16) {
        return [0.0, 0.0];
    }
    let (fx, fy) = (from >> 24, (from >> 16) & 0xFF);
    let (tx, ty) = (to >> 24, (to >> 16) & 0xFF);
    let (dx, dy) = (f64::from(tx) - f64::from(fx), f64::from(ty) - f64::from(fy));
    #[allow(clippy::cast_possible_truncation)]
    [dx as f32 * BLOCK_LENGTH, dy as f32 * BLOCK_LENGTH]
}

/// From one position (a cell and an origin) to another, in the first's landblock frame.
// D2-03291
#[must_use]
pub fn offset(from: (u32, [f32; 3]), to: (u32, [f32; 3])) -> [f32; 3] {
    let b = block_offset(from.0, to.0);
    let (f, t) = (from.1, to.1);
    // At double precision, kept as floats.
    let along = |b: f32, t: f32, f: f32| {
        #[allow(clippy::cast_possible_truncation)]
        let v = ((f64::from(b) + f64::from(t)) - f64::from(f)) as f32;
        v
    };
    [
        along(b[0], t[0], f[0]),
        along(b[1], t[1], f[1]),
        along(0.0, t[2], f[2]),
    ]
}

/// The distance between two positions.
// D2-13177
#[must_use]
pub fn distance_between(a: (u32, [f32; 3]), b: (u32, [f32; 3])) -> f32 {
    let o = offset(a, b).map(f64::from);
    #[allow(clippy::cast_possible_truncation)]
    let d = (o[2] * o[2] + o[1] * o[1] + o[0] * o[0]).sqrt() as f32;
    d
}

/// A position native's cell and origin.
fn place(c: &Call<'_>, r: u32) -> Result<(u32, [f32; 3]), Stop> {
    match c.vm.heap.native(r)? {
        NativeObject::Position { cell, frame } => Ok((
            *cell,
            [
                f32::from_bits(frame[0]),
                f32::from_bits(frame[1]),
                f32::from_bits(frame[2]),
            ],
        )),
        _ => Err(c.fail("not a position")),
    }
}

/// The distance from the receiver to the position below it, a float into the slot below that.
// D2-08875
fn distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let other = c.arg(0)?;
    if other == NULL {
        return Err(Stop::NullReference);
    }
    let d = distance_between(place(c, r)?, place(c, other)?);
    c.write(-3, d.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// A double kept as a float.
#[allow(clippy::cast_possible_truncation)]
fn float(v: f64) -> f32 {
    v as f32
}

/// The land's size in land cells (width, length), from the region the host's data holds; none
/// before there is one.
fn land(c: &mut Call<'_>) -> (i32, i32) {
    c.host
        .database()
        .and_then(|db| db.get::<auberean_assets::region::Region>(REGION))
        .map_or((0, 0), |r| {
            let cells = |blocks: u32| {
                i32::try_from(blocks.saturating_mul(land::CELLS_PER_BLOCK_SIDE)).unwrap_or(0)
            };
            (cells(r.blocks_wide), cells(r.blocks_long))
        })
}

/// A position native's cell and frame (origin, then rotation scalar first, as bits).
fn whole(c: &Call<'_>, r: u32) -> Result<(u32, [u32; 7]), Stop> {
    match c.vm.heap.native(r)? {
        NativeObject::Position { cell, frame } => Ok((*cell, *frame)),
        _ => Err(c.fail("not a position")),
    }
}

/// A position native's cell and frame, to change.
fn whole_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<(&'a mut u32, &'a mut [u32; 7]), Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Position { cell, frame } => Ok((cell, frame)),
        _ => Err(Stop::Native {
            native,
            what: "not a position".into(),
        }),
    }
}

/// The position an argument names (`fp[k]`); a null one stops the run.
fn position_arg(c: &Call<'_>, k: i64) -> Result<(u32, [u32; 7]), Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    whole(c, r)
}

/// The vector an argument names (`fp[k]`), as bits; a null one stops the run.
fn vector_arg(c: &Call<'_>, k: i64) -> Result<[u32; 3], Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::Vector(v) => Ok(*v),
        _ => Err(c.fail("not a vector")),
    }
}

/// A frame's origin, as floats.
fn origin(frame: &[u32; 7]) -> [f32; 3] {
    [
        f32::from_bits(frame[0]),
        f32::from_bits(frame[1]),
        f32::from_bits(frame[2]),
    ]
}

/// A new vector into the slot `fp[k]`.
fn give_vector(c: &mut Call<'_>, k: i64, v: [f32; 3]) -> Result<Status, Stop> {
    let r =
        c.vm.heap
            .add_native(VECTOR, NativeObject::Vector(v.map(f32::to_bits)));
    c.write(k, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The vector as text, a new string at `fp[-2]`.
// D2-09021
fn vector_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = match c.vm.heap.native(r)? {
        NativeObject::Vector(p) => p.map(f32::from_bits),
        _ => return Err(c.fail("not a vector")),
    };
    give_text(
        c,
        &auberean_physics::geom::vector_text(dereth_primitives::Vec3::new(v[0], v[1], v[2])),
    )
}

/// A new string into the slot `fp[-2]`.
fn give_text(c: &mut Call<'_>, s: &str) -> Result<Status, Stop> {
    let r = new_text(c, s.encode_utf16().collect());
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The receiver's cell and frame; `None` for the null receiver (the call crashes).
fn receiver(c: &Call<'_>) -> Result<Option<(u32, [u32; 7])>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    whole(c, r).map(Some)
}

/// Answer a word from the receiver's cell and frame into `fp[-2]`.
fn answer(c: &mut Call<'_>, f: impl FnOnce(u32, &[u32; 7]) -> u32) -> Result<Status, Stop> {
    let Some((cell, frame)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    c.write(-2, f(cell, &frame), tag::MIXED)?;
    Ok(Status::Success)
}

/// Answer a word from the receiver and the position below it into `fp[-3]`.
fn compare(
    c: &mut Call<'_>,
    f: impl FnOnce((u32, [u32; 7]), (u32, [u32; 7])) -> u32,
) -> Result<Status, Stop> {
    let Some(a) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let b = position_arg(c, -2)?;
    c.write(-3, f(a, b), tag::MIXED)?;
    Ok(Status::Success)
}

/// Change the receiver's cell and frame.
fn change(c: &mut Call<'_>, f: impl FnOnce(&mut u32, &mut [u32; 7])) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (cell, frame) = whole_mut(c, r)?;
    f(cell, frame);
    Ok(Status::Success)
}

/// The offset from the receiver to the position below it, a new vector below that.
// D2-08890
fn get_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((cell, frame)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let (to_cell, to) = position_arg(c, -2)?;
    let v = offset((cell, origin(&frame)), (to_cell, origin(&to)));
    give_vector(c, -3, v)
}

/// The receiver's origin, a new vector.
// D2-08891
fn get_origin(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((_, frame)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    give_vector(c, -2, origin(&frame))
}

/// The receiver's cell.
// D2-18959
fn get_cell(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, |cell, _| cell)
}

/// The receiver's cell set to the word below it.
// D2-01370
fn set_cell(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    change(c, |cell, _| *cell = v)
}

/// Whether the receiver and the position below it are in one landblock (two cells of id 0 are;
/// one of id 0 and another are not).
// D2-08878
fn in_same_block(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, |(a, _), (b, _)| {
        u32::from(match (a, b) {
            (0, 0) => true,
            (0, _) | (_, 0) => false,
            _ => a >> 16 == b >> 16,
        })
    })
}

/// The id of the receiver's landblock's own cell.
// D2-08867
fn block_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, |cell, _| cell & 0xFFFF_0000 | 0xFFFF)
}

/// The offset from the receiver's landblock to the landblock of the cell below it, a new vector
/// below that.
// D2-08889, D2-01968
fn get_block_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((cell, _)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let to = c.cx.at(-2)?;
    let [x, y] = block_offset(cell, to);
    give_vector(c, -3, [x, y, 0.0])
}

/// The receiver's origin set to the vector below it, as it is.
// D2-08862
fn set_origin(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = vector_arg(c, -2)?;
    change(c, |_, frame| frame[..3].copy_from_slice(&v))
}

/// Whether the receiver's cell is 0.
// D2-09176
fn not_in_world(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, |cell, _| u32::from(cell == 0))
}

/// The offset from the receiver to the position below it, at double precision.
fn offset_to(a: (u32, [u32; 7]), b: (u32, [u32; 7])) -> [f64; 3] {
    offset((a.0, origin(&a.1)), (b.0, origin(&b.1))).map(f64::from)
}

/// The squared distance from the receiver to the position below it.
// D2-08876, D2-13178
fn distance_squared(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, |a, b| {
        let o = offset_to(a, b);
        float(o[2] * o[2] + o[1] * o[1] + o[0] * o[0]).to_bits()
    })
}

/// The distance across the ground from the receiver to the position below it.
// D2-08881, D2-13179
fn xy_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, |a, b| {
        let o = offset_to(a, b);
        float((o[0] * o[0] + o[1] * o[1]).sqrt()).to_bits()
    })
}

/// The squared distance across the ground from the receiver to the position below it.
// D2-08882, D2-13180
fn xy_distance_squared(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, |a, b| {
        let o = offset_to(a, b);
        float(o[0] * o[0] + o[1] * o[1]).to_bits()
    })
}

/// How near two frames' parts must be for the frames to be equal.
const FRAME_EQUAL_WITHIN: f64 = 0.0002;

/// Whether the receiver and the position below it are equal: one cell, and every part of their
/// frames within [`FRAME_EQUAL_WITHIN`].
// D2-08877, D2-05770
fn position_equals(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, |(a, fa), (b, fb)| {
        let near = fa.iter().zip(&fb).all(|(x, y)| {
            (f64::from(f32::from_bits(*x)) - f64::from(f32::from_bits(*y))).abs()
                < FRAME_EQUAL_WITHIN
        });
        u32::from(a == b && near)
    })
}

/// The receiver's height.
// D2-08536
fn get_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, |_, frame| frame[2])
}

/// The receiver's height set to the float below it, as it is.
// D2-08861
fn set_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    change(c, |_, frame| frame[2] = v)
}

/// The receiver set to the position below it: its cell and its frame.
// D2-08857
fn set_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (to_cell, to) = position_arg(c, -2)?;
    change(c, |cell, frame| {
        *cell = to_cell;
        *frame = to;
    })
}

/// The vector below the receiver added to its origin.
// D2-08858
fn add_global_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = vector_arg(c, -2)?.map(f32::from_bits);
    change(c, |_, frame| {
        for (k, d) in v.iter().enumerate() {
            frame[k] = (d + f32::from_bits(frame[k])).to_bits();
        }
    })
}

/// The vector below the receiver, in the receiver's frame, turned into the landblock's and added
/// to its origin (at double precision, each axis kept as a float).
// D2-08859, D2-07044
fn add_local_offset(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = vector_arg(c, -2)?.map(|w| f64::from(f32::from_bits(w)));
    change(c, |_, frame| {
        let was = *frame;
        let f = |k: usize| f32::from_bits(was[k]);
        let q = dereth_primitives::Quat::new(f(3), f(4), f(5), f(6));
        let m = auberean_physics::geom::rotation_axes(q).map(f64::from);
        for k in 0..3 {
            let moved = m[6 + k] * v[2] + m[k] * v[0] + m[3 + k] * v[1] + f64::from(f(k));
            frame[k] = float(moved).to_bits();
        }
    })
}

/// A new position: the client's position of nothing (cell 0, no offset, no turn).
// D2-08884
fn null_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.vm.heap.add_native(
        POSITION,
        NativeObject::Position {
            cell: 0,
            frame: [0, 0, 0, 1.0f32.to_bits(), 0, 0, 0],
        },
    );
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// Whether the receiver is outside: a cell below the first interior cell.
// D2-08879
fn is_outside(c: &mut Call<'_>) -> Result<Status, Stop> {
    answer(c, |cell, _| u32::from(cell & 0xFFFF < 0x100))
}

/// Whether the receiver is valid: a cell of the world and a frame whose parts are numbers and
/// whose rotation is a unit.
// D2-08872, D2-13189
fn is_valid(c: &mut Call<'_>) -> Result<Status, Stop> {
    let land = land(c);
    answer(c, |cell, frame| {
        let f = |k: usize| f32::from_bits(frame[k]);
        let frame = dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(f(0), f(1), f(2)),
            dereth_primitives::Quat::new(f(3), f(4), f(5), f(6)),
        );
        u32::from(land::is_valid_cell(cell, land) && auberean_physics::geom::frame_is_valid(&frame))
    })
}

/// The squared distance between two land cells' middles (`fp[-2]`, `fp[-3]`), at double
/// precision; the land's length in units when either is not a land cell of the world.
// D2-13194
fn cells_apart_squared(a: u32, b: u32, land: (i32, i32)) -> f64 {
    match (land::land_coords(a, land), land::land_coords(b, land)) {
        (Some((ax, ay)), Some((bx, by))) => {
            let n = (ax - bx) * (ax - bx) + (ay - by) * (ay - by);
            f64::from(n) * SQUARE_LENGTH * SQUARE_LENGTH
        }
        _ => f64::from(land.1) * SQUARE_LENGTH,
    }
}

/// The distance between the middles of the two land cells below the receiver.
// D2-08868, D2-13202
fn cell_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let land = land(c);
    let d = float(cells_apart_squared(a, b, land).sqrt());
    c.write(-4, d.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// The squared distance between the middles of the two land cells below the receiver.
// D2-08869
fn cell_distance_squared(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (a, b) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let land = land(c);
    let d = float(cells_apart_squared(a, b, land));
    c.write(-4, d.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// The distance between two upright cylinders (`fp[-2]` radius, `fp[-3]` height, `fp[-4]` the
/// position each stands at; `fp[-5..-7]` the other's): the gap across the ground past both radii
/// and the gap in height past the lower one's top, each at least zero, put together.
// D2-08873, D2-13187
fn cylinder_distance(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let w = |c: &Call<'_>, k: i64| c.cx.at(k).map(f32::from_bits);
    let (r1, h1, p1) = (w(c, -2)?, w(c, -3)?, position_arg(c, -4)?);
    let (r2, h2, p2) = (w(c, -5)?, w(c, -6)?, position_arg(c, -7)?);
    let o = offset_to(p1, p2);
    let mut across = float((o[0] * o[0] + o[1] * o[1]).sqrt() - (f64::from(r1) + f64::from(r2)));
    if across < 0.0 {
        across = 0.0;
    }
    let (z1, z2) = (origin(&p1.1)[2], origin(&p2.1)[2]);
    let (top, high) = if z1 <= z2 {
        (float(f64::from(z1) + f64::from(h1)), z2)
    } else {
        (float(f64::from(z2) + f64::from(h2)), z1)
    };
    let mut up = float(f64::from(high) - f64::from(top));
    if up < 0.0 {
        up = 0.0;
    }
    let (a, u) = (f64::from(across), f64::from(up));
    let d = float((a * a + u * u).sqrt());
    c.write(-8, d.to_bits(), tag::MIXED)?;
    Ok(Status::Success)
}

/// A position written out: `0x` and its cell in eight hexadecimal digits, its origin in brackets,
/// then its rotation (scalar first), each part with six decimals.
// D2-13204
#[must_use]
pub fn position_text(cell: u32, frame: &[u32; 7]) -> String {
    let f = |k: usize| crate::text::fixed(f64::from(f32::from_bits(frame[k])));
    format!(
        "0x{cell:08X} [{} {} {}] {} {} {} {}",
        f(0),
        f(1),
        f(2),
        f(3),
        f(4),
        f(5),
        f(6)
    )
}

/// A position written out as [`position_text`] writes it, with its rotation either a heading in
/// degrees or four parts, scalar first; the cell and the origin's frame bits and the rotation.
// D2-13195
#[must_use]
pub fn read_position_text(text: &[u16]) -> Option<(u32, [u32; 7])> {
    let s = auberean_text::crt::cstr(text);
    let mut at = 0;
    let literal = |at: &mut usize, want: &str| {
        for c in want.encode_utf16() {
            if s.get(*at) != Some(&c) {
                return false;
            }
            *at += 1;
        }
        true
    };
    if !literal(&mut at, "0x") {
        return None;
    }
    let start = at;
    while at < s.len()
        && at - start < 8
        && char::from_u32(u32::from(s[at])).is_some_and(|c| c.is_ascii_hexdigit())
    {
        at += 1;
    }
    let cell = u32::from_str_radix(&String::from_utf16_lossy(&s[start..at]), 16).ok()?;
    while s.get(at).is_some_and(|c| auberean_text::crt::is_space(*c)) {
        at += 1;
    }
    if !literal(&mut at, "[") {
        return None;
    }
    let x = auberean_text::crt::scan_float(s, &mut at)?;
    let y = auberean_text::crt::scan_float(s, &mut at)?;
    let z = auberean_text::crt::scan_float(s, &mut at)?;
    if !literal(&mut at, "]") {
        return None;
    }
    let first = auberean_text::crt::scan_float(s, &mut at)?;
    let mut frame = dereth_primitives::Frame::new(
        dereth_primitives::Vec3::new(x, y, z),
        dereth_primitives::Quat::IDENTITY,
    );
    match auberean_text::crt::scan_float(s, &mut at) {
        Some(b) => {
            let c = auberean_text::crt::scan_float(s, &mut at)?;
            let d = auberean_text::crt::scan_float(s, &mut at)?;
            auberean_physics::geom::set_rotate(
                &mut frame,
                dereth_primitives::Quat::new(first, b, c, d),
            );
        }
        None => auberean_physics::geom::set_heading(
            &mut frame,
            auberean_physics::geom::Heading::new(first),
        ),
    }
    let (o, q) = (frame.origin, frame.rotation);
    Some((cell, [o.x, o.y, o.z, q.w, q.x, q.y, q.z].map(f32::to_bits)))
}

/// Map coordinates as a player writes them (`12.3N 45.6E`, a comma between them if liked, the last
/// letter left out meaning east): east and north, south and west being negative. Any other letters
/// are `None`.
// D2-13201
#[must_use]
pub fn read_map_coords(text: &[u16]) -> Option<(f32, f32)> {
    let s = auberean_text::crt::cstr(text);
    let mut at = 0;
    let north = auberean_text::crt::scan_float(s, &mut at)?;
    let north_letter = *s.get(at)?;
    at += 1;
    if s.get(at) == Some(&u16::from(b',')) {
        at += 1;
    }
    let east = auberean_text::crt::scan_float(s, &mut at)?;
    let east_letter = match s.get(at) {
        Some(c) => auberean_text::crt::to_upper(*c),
        None => u16::from(b'E'),
    };
    let east = if east_letter == u16::from(b'W') {
        -east
    } else if east_letter == u16::from(b'E') {
        east
    } else {
        return None;
    };
    let north_letter = auberean_text::crt::to_upper(north_letter);
    let north = if north_letter == u16::from(b'S') {
        -north
    } else if north_letter == u16::from(b'N') {
        north
    } else {
        return None;
    };
    Some((east, north))
}

/// The receiver set from the string below it as [`read_position_text`] reads it; answers whether it
/// could be read.
// D2-08866
fn from_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = c.cx.at(-2)?;
    if s == NULL {
        return Err(Stop::NullReference);
    }
    let text = text_of(c, s)?;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let read = read_position_text(&text);
    if let Some((new_cell, new_frame)) = read {
        let (cell, frame) = whole_mut(c, r)?;
        *cell = new_cell;
        *frame = new_frame;
    }
    c.write(-3, u32::from(read.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The receiver moved to the middle of the land cell at the map coordinates the string below it
/// gives, its height and rotation kept; answers whether they could be read and are on the map.
// D2-08865
fn from_real_map_coords(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = c.cx.at(-2)?;
    if s == NULL {
        return Err(Stop::NullReference);
    }
    let text = text_of(c, s)?;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let read = read_map_coords(&text);
    let (cell, frame) = whole_mut(c, r)?;
    let ok = read.is_some_and(|(east, north)| {
        let f = |k: usize| f32::from_bits(frame[k]);
        let mut p = dereth_primitives::Position::new(
            dereth_primitives::CellId(*cell),
            dereth_primitives::Frame::new(
                dereth_primitives::Vec3::new(f(0), f(1), f(2)),
                dereth_primitives::Quat::IDENTITY,
            ),
        );
        let ok = auberean_physics::geom::position::set_from_map_coords(&mut p, east, north);
        *cell = p.cell.0;
        frame[0] = p.frame.origin.x.to_bits();
        frame[1] = p.frame.origin.y.to_bits();
        ok
    });
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Every place of the world's places table written out, a line each (a new string), or the words
/// saying there is no table. No install holds one.
// D2-13676, D2-13677, D2-08848
fn places_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.host.data_id_by_enum(PLACES_ENUM, PLACES_MAPPING);
    let places = c
        .host
        .database()
        .and_then(|db| db.get::<auberean_assets::tables::Places>(id));
    let Some(places) = places else {
        return give_text(c, "Couldn't get places list.\n");
    };
    let mut text = String::new();
    for (name, p) in &places.places {
        let (o, q) = (p.frame.origin, p.frame.rotation);
        let frame = [o.x, o.y, o.z, q.w, q.x, q.y, q.z].map(f32::to_bits);
        text += &format!("{name}: {}.\n", position_text(p.cell.0, &frame));
    }
    give_text(c, &text)
}

/// The place named at `fp[-2]`, from the table the install holds, as its cell and frame.
fn named_place(c: &mut Call<'_>) -> Result<Option<(u32, [u32; 7])>, Stop> {
    let name = c.cx.at(-2)?;
    if name == NULL {
        return Err(Stop::NullReference);
    }
    let name = String::from_utf16_lossy(&text_of(c, name)?);
    let id = c.host.data_id_by_enum(PLACES_ENUM, PLACES_MAPPING);
    let places = c
        .host
        .database()
        .and_then(|db| db.get::<auberean_assets::tables::Places>(id));
    Ok(places.and_then(|t| {
        t.place(&name).map(|p| {
            let (o, q) = (p.frame.origin, p.frame.rotation);
            (
                p.cell.0,
                [o.x, o.y, o.z, q.w, q.x, q.y, q.z].map(f32::to_bits),
            )
        })
    }))
}

/// The object an out reference's slot (`fp[-3]`) holds.
fn out_object(c: &mut Call<'_>) -> Result<u32, Stop> {
    let out = c.cx.at(-3)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Err(Stop::NullReference);
    }
    Ok(target)
}

/// The place named at `fp[-2]` copied into the position `fp[-3]`'s slot holds; whether it was
/// found at `fp[-4]`.
// D2-08850, D2-13679
fn inq_place(c: &mut Call<'_>) -> Result<Status, Stop> {
    let out = out_object(c)?;
    let got = named_place(c)?;
    if let Some((to_cell, to)) = got {
        let (cell, frame) = whole_mut(c, out)?;
        *cell = to_cell;
        *frame = to;
    }
    c.write(-4, u32::from(got.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The place named at `fp[-2]` written out into the string `fp[-3]`'s slot holds; whether it was
/// found at `fp[-4]`.
// D2-08851, D2-13680
fn inq_place_str(c: &mut Call<'_>) -> Result<Status, Stop> {
    let out = out_object(c)?;
    let got = named_place(c)?;
    if let Some((cell, frame)) = got {
        let units = position_text(cell, &frame).encode_utf16().collect();
        match c.vm.heap.native_mut(out)? {
            NativeObject::Text(t) => *t = units,
            _ => return Err(c.fail("not a string")),
        }
    }
    c.write(-4, u32::from(got.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The receiver written out.
// D2-08888
fn to_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((cell, frame)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    give_text(c, &position_text(cell, &frame))
}

/// The receiver's map coordinates, as the client writes them for a player (a new string).
// D2-08886
fn to_real_map_coords(c: &mut Call<'_>) -> Result<Status, Stop> {
    let land = land(c);
    let Some((cell, _)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    give_text(c, &auberean_text::place::real_map_coords(cell, land))
}

/// The place line's string table, by enumeration value.
const PLACE_TABLE_ENUM: u32 = 0x4000_0007;

/// The receiver's map coordinates as a string reference to the place line's table (a new one):
/// `Inside` when the cell is not a land cell of the world, else `RealMapCoords` with the north or
/// south distance (`NUM1`, to a tenth, and `DIR1`: `North` from 0 up, else `South`) and the east
/// or west (`NUM2`, `DIR2`).
// D2-08887, D2-13199
fn to_real_map_coords_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    use crate::text::{table_or_none, token_of};
    let land = land(c);
    let Some((cell, _)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let table = table_or_none(c.host.data_id_by_enum(PLACE_TABLE_ENUM, 0x4000_0004));
    let token = |s: &str| token_of(&s.encode_utf16().collect::<Vec<u16>>());
    let mut info = TextInfo {
        table,
        ..TextInfo::default()
    };
    match land::map_coords(cell, land) {
        None => info.string_id = token("Inside"),
        Some((east, north)) => {
            info.string_id = token("RealMapCoords");
            let number = |v: f32| VariableValue::Double(f64::from(v.abs()).to_bits(), 1);
            let named = |s: &str| {
                VariableValue::Nested(Box::new(TextInfo {
                    string_id: token(s),
                    table,
                    ..TextInfo::default()
                }))
            };
            for (id, kind, value) in [
                ("NUM1", 4, number(north)),
                (
                    "DIR1",
                    9,
                    named(if north >= 0.0 { "North" } else { "South" }),
                ),
                ("NUM2", 4, number(east)),
                ("DIR2", 9, named(if east >= 0.0 { "East" } else { "West" })),
            ] {
                info.set_variable(Variable {
                    id: token(id),
                    kind,
                    value,
                });
            }
        }
    }
    let r =
        c.vm.heap
            .add_native(native_type::STRING_INFO, NativeObject::TextInfo(info));
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The receiver's land cell's coordinates, `across up` (a new string), or `<error>` when it is not
/// a land cell of the world.
// D2-08885, D2-13205
fn to_cell_coords_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let land = land(c);
    let Some((cell, _)) = receiver(c)? else {
        return Ok(Status::Crash);
    };
    let s = land::land_coords(cell, land)
        .map_or_else(|| "<error>".to_owned(), |(x, y)| format!("{x} {y}"));
    give_text(c, &s)
}

/// The receiver's land cell's column (across), or -1 when it is not a land cell of the world.
// D2-08870, D2-13196
fn cell_x(c: &mut Call<'_>) -> Result<Status, Stop> {
    let land = land(c);
    #[allow(clippy::cast_sign_loss)]
    answer(c, |cell, _| {
        land::land_coords(cell, land).map_or(-1, |(x, _)| x) as u32
    })
}

/// The receiver's land cell's row (up), or -1 when it is not a land cell of the world.
// D2-08871, D2-13197
fn cell_y(c: &mut Call<'_>) -> Result<Status, Stop> {
    let land = land(c);
    #[allow(clippy::cast_sign_loss)]
    answer(c, |cell, _| {
        land::land_coords(cell, land).map_or(-1, |(_, y)| y) as u32
    })
}

/// The receiver moved to the middle of the land cell at land coordinates (across, up), its
/// height kept; its cell becomes 0 (and the answer 0) when they are off the land.
// D2-13198
fn place_at_cell(cell: &mut u32, frame: &mut [u32; 7], x: i32, y: i32, land: (i32, i32)) -> bool {
    *cell = land::cell_at(x, y, land);
    if *cell == 0 {
        return false;
    }
    let n = (*cell & 0xFFFF) - 1;
    let middle = |k: u32| float(f64::from(k) * SQUARE_LENGTH + SQUARE_LENGTH / 2.0).to_bits();
    frame[0] = middle(n >> 3);
    frame[1] = middle(n & 7);
    true
}

/// The receiver moved to the middle of the land cell at the land coordinates below it (`fp[-2]`
/// across, `fp[-3]` up); answers whether that is on the land.
// D2-08863
fn from_cell_coords(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    let (x, y) = (c.cx.at(-2)? as i32, c.cx.at(-3)? as i32);
    let land = land(c);
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (cell, frame) = whole_mut(c, r)?;
    let ok = place_at_cell(cell, frame, x, y, land);
    c.write(-4, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// The receiver moved to the middle of the land cell whose land coordinates the string below it
/// gives (`across up`, as the C library reads `%d %d`); answers whether both were read and are on
/// the land.
// D2-08864, D2-13203
fn from_cell_coords_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = c.cx.at(-2)?;
    if s == NULL {
        return Err(Stop::NullReference);
    }
    let text = text_of(c, s)?;
    let land = land(c);
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let read = scan_two_ints(&text);
    let (cell, frame) = whole_mut(c, r)?;
    let ok = read.is_some_and(|(x, y)| place_at_cell(cell, frame, x, y, land));
    c.write(-3, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Two whole numbers as the C library's `%d %d` reads them from a wide string: each after any
/// white space, a sign and decimal digits (wrapping past the 32-bit range); `None` unless both were
/// read.
fn scan_two_ints(text: &[u16]) -> Option<(i32, i32)> {
    let s = auberean_text::crt::cstr(text);
    let mut i = 0;
    let mut int = || -> Option<i32> {
        while i < s.len() && auberean_text::crt::is_space(s[i]) {
            i += 1;
        }
        let negative = match s.get(i) {
            Some(&0x2D) => {
                i += 1;
                true
            }
            Some(&0x2B) => {
                i += 1;
                false
            }
            _ => false,
        };
        let start = i;
        let mut v: i32 = 0;
        while let Some(d) = s
            .get(i)
            .and_then(|&u| char::from_u32(u32::from(u))?.to_digit(10))
        {
            #[allow(clippy::cast_possible_wrap)]
            let d = d as i32;
            v = v.wrapping_mul(10).wrapping_add(d);
            i += 1;
        }
        (i > start).then(|| if negative { v.wrapping_neg() } else { v })
    };
    let x = int()?;
    let y = int()?;
    Some((x, y))
}

/// The parts of the vector a reference names, at double precision; a null one stops the run.
fn parts(c: &Call<'_>, r: u32) -> Result<[f64; 3], Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::Vector(p) => Ok(p.map(|w| f64::from(f32::from_bits(w)))),
        _ => Err(c.fail("not a vector")),
    }
}

/// A double kept as a float, as its bits.
#[allow(clippy::cast_possible_truncation)]
fn kept(v: f64) -> u32 {
    (v as f32).to_bits()
}

/// Two vectors' parts.
type Pair = ([f64; 3], [f64; 3]);

/// The receiver's and the argument's parts (`fp[-1]`, `fp[-2]`); `None` for the null receiver.
fn both(c: &Call<'_>) -> Result<Option<Pair>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    Ok(Some((parts(c, r)?, parts(c, c.arg(0)?)?)))
}

/// The cross product of the receiver and the argument, a new vector below the argument.
// D2-09020
fn cross(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((a, b)) = both(c)? else {
        return Ok(Status::Crash);
    };
    let v = [
        kept(a[1] * b[2] - a[2] * b[1]),
        kept(b[0] * a[2] - b[2] * a[0]),
        kept(a[0] * b[1] - b[0] * a[1]),
    ];
    let r = c.vm.heap.add_native(VECTOR, NativeObject::Vector(v));
    c.write(-3, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The dot product of the receiver and the argument, a float below the argument.
// D2-09017
fn dot(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((a, b)) = both(c)? else {
        return Ok(Status::Crash);
    };
    let d = b[2] * a[2] + b[1] * a[1] + b[0] * a[0];
    c.write(-3, kept(d), tag::MIXED)?;
    Ok(Status::Success)
}

/// The receiver's length, a float below it.
// D2-09019
fn magnitude(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let p = parts(c, r)?;
    let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    c.write(-2, kept(l), tag::MIXED)?;
    Ok(Status::Success)
}

/// A vector made one long, as the client's math library does it on a processor with SSE2: the
/// squared length summed in single precision (`x² + y²`, then `+ z²`); below 2^-46 the vector
/// becomes zero; otherwise each part is multiplied by one Newton step from an estimate `e` of the
/// reciprocal square root, `0.5 e (3 - s e e)`, every step rounded to single precision. The
/// processor's own estimate differs between processors; here it is the reciprocal square root
/// itself, rounded.
#[must_use]
pub fn normalized(v: [f32; 3]) -> [f32; 3] {
    /// The squared length below which the vector becomes zero: 2^-46.
    const TINY: f32 = f32::from_bits(0x2880_0000);
    let s = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if s < TINY {
        return [0.0; 3];
    }
    #[allow(clippy::cast_possible_truncation)]
    let e = (1.0 / f64::from(s).sqrt()) as f32;
    let k = (0.5 * e) * (3.0 - (s * e) * e);
    [k * v[0], k * v[1], k * v[2]]
}

/// Make the receiver one long, in place ([`normalized`]).
// D2-09013
fn normalize(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    match c.vm.heap.native_mut(r)? {
        NativeObject::Vector(p) => *p = normalized(p.map(f32::from_bits)).map(f32::to_bits),
        _ => return Err(c.fail("the receiver is not a vector")),
    }
    Ok(Status::Success)
}

/// Nothing, answering nothing.
// D2-08799
fn does_nothing(c: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(if c.receiver()? == NULL {
        Status::Crash
    } else {
        Status::Success
    })
}

/// Change the receiver's parts with the argument's, one part at a time.
fn in_place(c: &mut Call<'_>, f: fn(f64, f64) -> f64) -> Result<Status, Stop> {
    let Some((a, b)) = both(c)? else {
        return Ok(Status::Crash);
    };
    let v = [
        kept(f(a[0], b[0])),
        kept(f(a[1], b[1])),
        kept(f(a[2], b[2])),
    ];
    let r = c.receiver()?;
    if let NativeObject::Vector(p) = c.vm.heap.native_mut(r)? {
        *p = v;
    }
    Ok(Status::Success)
}

/// The argument added to the receiver.
// D2-09012
fn add(c: &mut Call<'_>) -> Result<Status, Stop> {
    in_place(c, |a, b| b + a)
}

/// The argument taken from the receiver.
// D2-09016
fn subtract(c: &mut Call<'_>) -> Result<Status, Stop> {
    in_place(c, |a, b| a - b)
}

/// The receiver takes the argument's parts as they are.
// D2-08589
fn copy(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let from = c.arg(0)?;
    if from == NULL {
        return Err(Stop::NullReference);
    }
    let NativeObject::Vector(v) = *c.vm.heap.native(from)? else {
        return Err(c.fail("not a vector"));
    };
    match c.vm.heap.native_mut(r)? {
        NativeObject::Vector(p) => *p = v,
        _ => return Err(c.fail("not a vector")),
    }
    Ok(Status::Success)
}

/// How near two vectors' parts must be to be equal.
const EQUAL_WITHIN: f32 = 0.0002;

/// Whether the receiver and the argument are equal, every part within [`EQUAL_WITHIN`], below
/// the argument (1 or 0).
// D2-09018
fn equals(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((a, b)) = both(c)? else {
        return Ok(Status::Crash);
    };
    let within = f64::from(EQUAL_WITHIN);
    let same = (0..3).all(|k| (a[k] - b[k]).abs() < within);
    c.write(-3, u32::from(same), tag::MIXED)?;
    Ok(Status::Success)
}

/// A float through the floating-point unit: a signalling not-a-number comes out quiet.
const fn through_fpu(v: u32) -> u32 {
    if f32::from_bits(v).is_nan() {
        v | 1 << 22
    } else {
        v
    }
}

/// Set the receiver's vector: x, y and z (`fp[-2]`, `fp[-3]`, `fp[-4]`).
// D2-09015
fn set_vector(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = [
        through_fpu(c.arg(0)?),
        through_fpu(c.arg(1)?),
        through_fpu(c.arg(2)?),
    ];
    match c.vm.heap.native_mut(r)? {
        NativeObject::Vector(p) => *p = v,
        _ => return Err(c.fail("not a vector")),
    }
    Ok(Status::Success)
}

/// One part of the receiver's vector (0 x, 1 y, 2 z), below the receiver.
// D2-01489, D2-01490, D2-18959, D2-08505
fn vector_part<const K: usize>(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = match c.vm.heap.native(r)? {
        NativeObject::Vector(p) => p[K],
        _ => return Err(c.fail("not a vector")),
    };
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the land helpers behind the position natives, held to hand-worked values)
    use super::*;

    const LAND: (i32, i32) = (255 * 8, 255 * 8);

    #[test]
    fn land_cells_are_measured_and_found_by_their_coordinates() {
        // Three across and four up: five cells, 100 units.
        assert_eq!(
            cells_apart_squared(0x0000_0001, 0x0000_001D, LAND),
            10_000.0
        );
        // Off the land: the land's length in units, not squared.
        assert_eq!(
            cells_apart_squared(0x0000_0001, 0x0000_0100, LAND),
            40_800.0
        );
        let (mut cell, mut frame) = (7, [0, 0, 5, 0, 0, 0, 0]);
        assert!(place_at_cell(&mut cell, &mut frame, 9, 17, LAND));
        assert_eq!(cell, 0x0102_000A);
        assert_eq!(frame[..3], [30.0f32.to_bits(), 30.0f32.to_bits(), 5]);
        assert!(!place_at_cell(&mut cell, &mut frame, 2040, 0, LAND));
        assert_eq!(cell, 0);
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(scan_two_ints(&wide("  12 -7x")), Some((12, -7)));
        assert_eq!(scan_two_ints(&wide("12,7")), None);
        assert_eq!(scan_two_ints(&wide("+3\t4")), Some((3, 4)));
        assert_eq!(scan_two_ints(&wide("")), None);
    }

    /// A position reads back as it is written, its rotation as four parts or a heading.
    #[test]
    fn a_position_reads_back_as_it_is_written() {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let frame = [1.5f32, -2.25, 3.0, 1.0, 0.0, 0.0, 0.0].map(f32::to_bits);
        let text = position_text(0xC156_0035, &frame);
        assert_eq!(read_position_text(&wide(&text)), Some((0xC156_0035, frame)));
        let (cell, f) =
            read_position_text(&wide("0x00000102 [1 2 3] 90")).expect("a heading in degrees");
        assert_eq!(cell, 0x102);
        assert_eq!(f[..3], [1.0f32, 2.0, 3.0].map(f32::to_bits));
        assert_ne!(f[3..], frame[3..], "turned");
        assert_eq!(read_position_text(&wide("0x102 1 2 3")), None);
        assert_eq!(read_position_text(&wide("0x102 [1 2 3] 1 2")), None);
    }

    /// Map coordinates read as a player writes them: south and west negative, a comma if liked, the
    /// last letter left out meaning east, any other letter refused.
    #[test]
    fn map_coordinates_read_as_a_player_writes_them() {
        let wide = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(read_map_coords(&wide("12.5N 40W")), Some((-40.0, 12.5)));
        assert_eq!(read_map_coords(&wide("12.5s, 40e")), Some((40.0, -12.5)));
        assert_eq!(read_map_coords(&wide("3N 7")), Some((7.0, 3.0)));
        assert_eq!(read_map_coords(&wide("3Q 7E")), None);
        assert_eq!(read_map_coords(&wide("3N 7Q")), None);
        assert_eq!(read_map_coords(&wide("")), None);
    }
}
