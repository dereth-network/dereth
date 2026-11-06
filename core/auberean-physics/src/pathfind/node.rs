//! The graph a path map holds: nodes (a place in a cell, what the ground there is like, and the
//! links out of it) and links (the node they lead to, how far, how wide and how high the way is,
//! and the doorway it passes).
//!
//! A record stores places in tenths of a metre, each coordinate as a signed 16-bit count (so
//! within about 3 km of the block's corner), and widths and heights as one of seven size classes
//! in four bits.

use std::collections::BTreeSet;

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom;
use crate::geom::Ray;
use crate::pack::Writer;

/// The record grid: points per metre.
pub const GRID_PER_METRE: f32 = 10.0;
/// A stored coordinate's limit: 30,000 grid points. The bound a coordinate is tested against is
/// the product of a tenth and 30,000 as the client works it out (in wide precision, from the
/// single-precision tenth), a hair over 3,000 m, so 3,000 m itself is still on the grid.
const GRID_LIMIT: i16 = 30_000;
const TENTH: f32 = 0.1;

/// A place in a cell: the cell and the offset from its landblock's corner.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Spot {
    pub cell: CellId,
    pub offset: Vec3,
}

impl Spot {
    #[must_use]
    pub fn new(cell: CellId, offset: Vec3) -> Self {
        Self { cell, offset }
    }

    /// The place of a position (its rotation dropped).
    #[must_use]
    pub fn of(p: &Position) -> Self {
        Self {
            cell: p.cell,
            offset: p.frame.origin,
        }
    }

    /// The position at this place, unturned.
    #[must_use]
    pub fn to_position(&self) -> Position {
        Position::new(self.cell, Frame::new(self.offset, Quat::IDENTITY))
    }
}

/// A coordinate on the record grid: tenths of a metre rounded to nearest, or ±30,000 off it (a
/// coordinate not past the upper bound, NaN among them, takes the lower).
// D2-13433
#[must_use]
pub fn grid_point(v: f32) -> i16 {
    let limit = f64::from(TENTH) * f64::from(GRID_LIMIT);
    let x = f64::from(v);
    if x < limit && -limit < x {
        let r = (f64::from(GRID_PER_METRE) * x + 0.5).floor();
        #[allow(clippy::cast_possible_truncation)]
        return dereth_primitives::num::to_i32_f64(r) as i16;
    }
    if x > limit {
        GRID_LIMIT
    } else {
        -GRID_LIMIT
    }
}

/// A coordinate read back from the grid: the count times a single-precision tenth.
#[must_use]
pub fn from_grid(g: i16) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    {
        (f64::from(g) * f64::from(TENTH)) as f32
    }
}

/// A width or height's size class: 0 for unbounded (-1), then under 1, 2, 5, 12 and 40 m, and 6
/// for the rest.
// D2-13442
#[must_use]
pub fn compress_size(v: f32) -> u32 {
    #[allow(clippy::float_cmp)]
    if v == -1.0 {
        0
    } else if v < 1.0 {
        1
    } else if v < 2.0 {
        2
    } else if v < 5.0 {
        3
    } else if v < 12.0 {
        4
    } else if v < 40.0 {
        5
    } else {
        6
    }
}

/// The least size a class stands for: -1 (unbounded), 0, 1, 2, 5, 12, 40; a class past 6 reads
/// as unbounded.
// D2-13443
#[must_use]
pub fn uncompress_size(class: u32) -> f32 {
    match class {
        1 => 0.0,
        2 => 1.0,
        3 => 2.0,
        4 => 5.0,
        5 => 12.0,
        6 => 40.0,
        _ => -1.0,
    }
}

/// What the ground at a node is like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeData {
    /// How steep (0 to 15); a search refuses a node steeper than it allows.
    pub slope: u16,
    pub passable: bool,
    pub water: bool,
    pub solid: bool,
    /// The headroom above the node and the width around it, by size class (-1 unbounded).
    pub height: f32,
    pub width: f32,
}

impl Default for NodeData {
    // D2-13450
    fn default() -> Self {
        Self {
            slope: 0,
            passable: true,
            water: false,
            solid: false,
            height: 0.0,
            width: 0.0,
        }
    }
}

const PASSABLE: u16 = 0x1000;
const SOLID: u16 = 0x2000;
const WATER: u16 = 0x4000;

impl NodeData {
    /// Packed in 16 bits: the slope in the low four, the height's class in the next four, the
    /// width's in the next, then passable, solid and water.
    // D2-13451
    #[must_use]
    pub fn compress(&self) -> u16 {
        let mut w = self.slope;
        if self.passable {
            w |= PASSABLE;
        }
        if self.water {
            w |= WATER;
        }
        if self.solid {
            w |= SOLID;
        }
        #[allow(clippy::cast_possible_truncation)]
        {
            ((compress_size(self.width) as u8 as u16) << 8)
                | w
                | ((compress_size(self.height) << 4) as u16)
        }
    }

    /// Unpacked onto what is already there: a flag bit only ever sets its flag, so a node read
    /// from a record is always passable (a fresh node starts so).
    // D2-13452
    pub fn uncompress(&mut self, w: u16) {
        self.slope = w & 0xF;
        if w & PASSABLE != 0 {
            self.passable = true;
        }
        if w & WATER != 0 {
            self.water = true;
        }
        if w & SOLID != 0 {
            self.solid = true;
        }
        self.height = uncompress_size(u32::from(w >> 4) & 0xF);
        self.width = uncompress_size(u32::from(w >> 8) & 0xF);
    }

    // D2-13453
    pub fn read(&mut self, r: &mut Reader<'_>) -> Result<()> {
        let w = r.u16()?;
        self.uncompress(w);
        Ok(())
    }

    pub fn write(&self, w: &mut Writer) {
        w.u16(self.compress());
    }
}

/// The way along a link.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LinkData {
    /// How far: what a search pays to take the link.
    pub distance: f32,
    /// How wide the way is at its start, middle and end, and the narrowest of them is not kept:
    /// [`LinkData::width`] is the widest of the three.
    pub start_width: f32,
    pub mid_width: f32,
    pub end_width: f32,
    pub width: f32,
    pub height: f32,
    /// The doorway the link passes, from one post to the other.
    pub portal_begin: Vec3,
    pub portal_end: Vec3,
}

impl LinkData {
    // D2-13437
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The four size classes in 16 bits: start, middle, end, height.
    // D2-13434
    #[must_use]
    pub fn compress(&self) -> u32 {
        compress_size(self.start_width)
            | (compress_size(self.mid_width) << 4)
            | (compress_size(self.end_width) << 8)
            | (compress_size(self.height) << 12)
    }

    /// The four sizes, and the width as the widest of start, middle and end.
    // D2-13435
    pub fn uncompress(&mut self, w: u32) {
        self.start_width = uncompress_size(w & 0xF);
        self.mid_width = uncompress_size((w >> 4) & 0xF);
        self.end_width = uncompress_size((w >> 8) & 0xF);
        self.width = self.start_width;
        if self.width < self.mid_width {
            self.width = self.mid_width;
        }
        if self.width < self.end_width {
            self.width = self.end_width;
        }
        self.height = uncompress_size((w >> 12) & 0xF);
    }

    /// The distance, the sizes, the doorway's posts on the grid, then alignment.
    // D2-13438
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut d = Self::new();
        d.distance = r.f32()?;
        let w = r.u32()?;
        d.uncompress(w);
        let mut g = [0i16; 6];
        for v in &mut g {
            *v = r.i16()?;
        }
        d.portal_begin = Vec3::new(from_grid(g[0]), from_grid(g[1]), from_grid(g[2]));
        d.portal_end = Vec3::new(from_grid(g[3]), from_grid(g[4]), from_grid(g[5]));
        r.align()?;
        Ok(d)
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.distance);
        w.u32(self.compress());
        for v in [self.portal_begin, self.portal_end] {
            w.i16(grid_point(v.x));
            w.i16(grid_point(v.y));
            w.i16(grid_point(v.z));
        }
        w.align();
    }
}

/// A link out of a node.
// D2-03794, D2-03801
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Link {
    /// The node it leads to.
    pub dest_id: u32,
    /// Whether that node is in the same map (found when the map was read); a link to a node of
    /// another map finds it through the maps loaded during a search.
    pub dest_in_map: bool,
    pub data: Option<LinkData>,
}

impl Link {
    /// Finds the node it leads to among the map's (`ids`).
    // D2-13430
    pub fn link(&mut self, ids: &BTreeSet<u32>) {
        self.dest_in_map = ids.contains(&self.dest_id);
    }

    /// Its id, then its way.
    // D2-13431, D2-13444
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let dest_id = r.u32()?;
        let data = LinkData::read(r)?;
        Ok(Self {
            dest_id,
            dest_in_map: false,
            data: Some(data),
        })
    }

    /// A link with no way writes none and fails.
    pub fn write(&self, w: &mut Writer) -> bool {
        w.u32(self.dest_id);
        match &self.data {
            Some(d) => {
                d.write(w);
                true
            }
            None => false,
        }
    }

    /// The doorway the link passes, as a ray from post to post (or `ray` unchanged with no way),
    /// narrowed for a body `width` wide (-1: any). Where some part of the way is narrower than
    /// the body (by more than 0.0002) and the middle is wide enough, each narrow end's post is
    /// drawn in toward the doorway's middle, as far as the body is wider than that end relative
    /// to how much wider the middle is. Where the middle is too narrow, only the half on a side
    /// wide enough is kept (the near side when the start is, else the far side when the end is,
    /// else the doorway is left whole), from that post to the point as far out from the middle as
    /// the body is wider than the middle relative to how much wider that side is. Answers whether
    /// the link has a way.
    // D2-13432
    pub fn portal_constrained(&self, ray: &mut Ray, width: f32) -> bool {
        let Some(d) = self.data else {
            return false;
        };
        ray.init_from_offset(
            d.portal_begin,
            Vec3::new(
                d.portal_end.x - d.portal_begin.x,
                d.portal_end.y - d.portal_begin.y,
                d.portal_end.z - d.portal_begin.z,
            ),
        );
        #[allow(clippy::float_cmp)]
        if width == -1.0 {
            return true;
        }
        let (start, mid, end) = (d.start_width, d.mid_width, d.end_width);
        let w = width - 0.0002;
        if !(start < w || end < w || mid < w) {
            return true;
        }
        if width <= mid {
            let mut a = ray.point;
            let mut b = ray.end_point();
            let c = ray.center_point();
            if start < w {
                let t = (width - start) / (mid - start);
                a = Vec3::new(
                    (c.x - a.x) * t + a.x,
                    (c.y - a.y) * t + a.y,
                    t * (c.z - a.z) + a.z,
                );
            }
            if end < w {
                let t = (width - end) / (mid - end);
                b = Vec3::new(
                    (c.x - b.x) * t + b.x,
                    (c.y - b.y) * t + b.y,
                    (c.z - b.z) * t + b.z,
                );
            }
            ray.init_from_offset(a, Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z));
            return true;
        }
        let (post, side, far) = if start < width {
            if end < width {
                return true;
            }
            (ray.end_point(), end, true)
        } else {
            (ray.point, start, false)
        };
        let c = ray.center_point();
        let t = (width - mid) / (side - mid);
        let p = Vec3::new(
            (post.x - c.x) * t + c.x,
            (post.y - c.y) * t + c.y,
            (post.z - c.z) * t + c.z,
        );
        if far {
            ray.init_line_segment(p, post);
        } else {
            ray.init_line_segment(post, p);
        }
        true
    }
}

/// A node of a path map.
// D2-13446
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Node {
    pub id: u32,
    pub center: Spot,
    pub cluster: u32,
    pub data: Option<NodeData>,
    pub links: Vec<Link>,
}

impl Node {
    // D2-13448
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The ground here, made (with the defaults) when the node has none.
    pub fn node_data(&mut self) -> &mut NodeData {
        self.data.get_or_insert_with(NodeData::default)
    }

    /// Finds each link's node among the map's (`ids`).
    // D2-13445
    pub fn link(&mut self, ids: &BTreeSet<u32>) {
        for l in &mut self.links {
            l.link(ids);
        }
    }

    /// The straight-line distance to another node.
    // D2-13447
    #[must_use]
    pub fn distance(&self, other: &Node) -> f32 {
        geom::distance(&self.center.to_position(), &other.center.to_position())
    }

    /// The id, the cell, the place on the grid, the ground, alignment, the links, alignment.
    // D2-13449
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut n = Self::new();
        n.id = r.u32()?;
        let cell = r.u32()?;
        let (x, y, z) = (r.i16()?, r.i16()?, r.i16()?);
        n.center = Spot::new(
            CellId(cell),
            Vec3::new(from_grid(x), from_grid(y), from_grid(z)),
        );
        let mut data = NodeData::default();
        data.read(r)?;
        n.data = Some(data);
        r.align()?;
        let count = r.count(4)?;
        for _ in 0..count {
            n.links.push(Link::read(r)?);
        }
        r.align()?;
        Ok(n)
    }

    /// A node without ground data, or with a link without a way, writes up to it and fails.
    pub fn write(&self, w: &mut Writer) -> bool {
        w.u32(self.id);
        w.u32(self.center.cell.0);
        w.i16(grid_point(self.center.offset.x));
        w.i16(grid_point(self.center.offset.y));
        w.i16(grid_point(self.center.offset.z));
        let Some(d) = &self.data else {
            return false;
        };
        d.write(w);
        w.align();
        w.u32(u32::try_from(self.links.len()).unwrap_or(u32::MAX));
        for l in &self.links {
            if !l.write(w) {
                return false;
            }
        }
        w.align();
        true
    }
}
