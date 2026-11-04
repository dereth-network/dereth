//! Obstacles: an object's footprint on the path maps. An obstacle description (record type
//! `0x4000005B`, ids `0x38000000` on in the portal file) holds the boxes and upright cylinders the
//! object covers, the path map of its own floors (a building's), the links from its nodes out to
//! the cells around it, what lies beneath it, and the model it was made from.
//!
//! The record has no id: the footprint (its boxes, then its cylinders), whether a path map follows
//! and the map, the external links, what lies beneath, two strings (when it was made, and the
//! model), then its scale.

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::map::PathMap;
use crate::geom::Ray;
use crate::pack::{read_vec3, write_vec3, Writer, BBOX_SIZE};

/// The record type of an obstacle description.
// D2-13644
pub const RECORD_TYPE: u32 = 0x4000_005B;

/// A box, corner to corner.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    /// The two corners, low then high: six floats.
    // D2-13224
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            min: read_vec3(r)?,
            max: read_vec3(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        write_vec3(w, self.min);
        write_vec3(w, self.max);
    }
}

/// An upright cylinder: its bottom's centre, radius and height.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Cylinder {
    pub low: Vec3,
    pub radius: f32,
    pub height: f32,
}

impl Cylinder {
    /// The bottom's centre, then the radius, then the height.
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            low: read_vec3(r)?,
            radius: r.f32()?,
            height: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        write_vec3(w, self.low);
        w.f32(self.radius);
        w.f32(self.height);
    }
}

/// What an obstacle covers.
// D2-13653, D2-13654
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Footprint {
    pub boxes: Vec<Bounds>,
    pub cylinders: Vec<Cylinder>,
}

impl Footprint {
    /// The boxes, then the cylinders, each counted.
    // D2-13656, D2-13657
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            boxes: r.list(BBOX_SIZE, Bounds::read)?,
            cylinders: r.list(20, Cylinder::read)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(u32::try_from(self.boxes.len()).unwrap_or(u32::MAX));
        for b in &self.boxes {
            b.write(w);
        }
        w.u32(u32::try_from(self.cylinders.len()).unwrap_or(u32::MAX));
        for c in &self.cylinders {
            c.write(w);
        }
    }
}

/// What lies beneath an obstacle: the ground cells it spans, its box, where it stands (not
/// stored) and the heights probed under it.
// D2-13672, D2-13670, D2-13673
#[derive(Clone, Debug, PartialEq)]
pub struct UndersideInfo {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub bounds: Bounds,
    pub pos: Position,
    pub probes: Vec<f32>,
}

fn null_position() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

impl Default for UndersideInfo {
    // D2-13671
    fn default() -> Self {
        Self {
            min_x: 0,
            min_y: 0,
            max_x: 0,
            max_y: 0,
            bounds: Bounds::default(),
            pos: null_position(),
            probes: Vec::new(),
        }
    }
}

impl UndersideInfo {
    /// Cleared: no cells, an empty box, standing nowhere. The probes are kept.
    // D2-13668
    pub fn destroy(&mut self) {
        self.min_x = 0;
        self.max_x = 0;
        self.min_y = 0;
        self.max_y = 0;
        self.bounds = Bounds::default();
        self.pos = null_position();
    }

    /// The four cell bounds, the box, then the probes, counted.
    // D2-13669
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            min_x: r.i32()?,
            min_y: r.i32()?,
            max_x: r.i32()?,
            max_y: r.i32()?,
            bounds: Bounds::read(r)?,
            pos: null_position(),
            probes: r.list(4, Reader::f32)?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in [self.min_x, self.min_y, self.max_x, self.max_y] {
            w.i32(v);
        }
        self.bounds.write(w);
        w.u32(u32::try_from(self.probes.len()).unwrap_or(u32::MAX));
        for p in &self.probes {
            w.f32(*p);
        }
    }
}

/// A link from one of an obstacle's nodes out through a doorway to a cell around it.
// D2-13666, D2-13642
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExternalLink {
    pub portal_id: u32,
    /// The cell and doorway on the other side (not stored).
    pub other_cell_id: u32,
    pub other_portal_id: u32,
    pub node_id: u32,
    pub portal: Ray,
    /// The headroom (-1: any).
    pub height: f32,
}

impl Default for ExternalLink {
    // D2-13665
    fn default() -> Self {
        Self {
            portal_id: u32::MAX,
            other_cell_id: u32::MAX,
            other_portal_id: u32::MAX,
            node_id: 0,
            portal: Ray::default(),
            height: -1.0,
        }
    }
}

impl ExternalLink {
    /// The doorway's id, the node's, the doorway as a ray (start, direction, length), then the
    /// headroom.
    // D2-13667
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let portal_id = r.u32()?;
        let node_id = r.u32()?;
        let portal = Ray {
            point: read_vec3(r)?,
            dir: read_vec3(r)?,
            length: r.f32()?,
        };
        Ok(Self {
            portal_id,
            node_id,
            portal,
            height: r.f32()?,
            ..Self::default()
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.portal_id);
        w.u32(self.node_id);
        write_vec3(w, self.portal.point);
        write_vec3(w, self.portal.dir);
        w.f32(self.portal.length);
        w.f32(self.height);
    }
}

/// An obstacle description.
// D2-13648, D2-05060, D2-13647
#[derive(Clone, Debug, PartialEq)]
pub struct ObstacleShape {
    /// The model it was made from.
    pub ref_file: String,
    pub scale: Vec3,
    /// When it was made.
    pub timestamp: String,
    pub footprint: Footprint,
    pub map: Option<PathMap>,
    pub underside: UndersideInfo,
    pub external_links: Vec<ExternalLink>,
}

impl Default for ObstacleShape {
    // D2-13643
    fn default() -> Self {
        Self {
            ref_file: String::new(),
            scale: Vec3::new(1.0, 1.0, 1.0),
            timestamp: String::new(),
            footprint: Footprint::default(),
            map: None,
            underside: UndersideInfo::default(),
            external_links: Vec::new(),
        }
    }
}

impl ObstacleShape {
    /// Cleared: its map emptied (`first_insert_node` as for [`PathMap::clear`]), no strings, unit
    /// scale, no footprint, no external links, nothing beneath.
    // D2-13645
    pub fn destroy(&mut self, first_insert_node: u32) {
        if let Some(m) = self.map.as_mut() {
            m.clear(first_insert_node);
        }
        self.ref_file.clear();
        self.timestamp.clear();
        self.scale = Vec3::new(1.0, 1.0, 1.0);
        self.footprint = Footprint::default();
        self.external_links.clear();
        self.underside.destroy();
    }

    /// The record (see the module's header); its path map is read as a record of its own, id
    /// first.
    // D2-13651, D2-13649
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let footprint = Footprint::read(r)?;
        let has_map = r.u32()? != 0;
        let map = if has_map {
            let id = r.clone().u32()?;
            Some(PathMap::read(r, id)?)
        } else {
            None
        };
        let external_links = r.list(36, ExternalLink::read)?;
        let underside = UndersideInfo::read(r)?;
        let timestamp = r.narrow_string()?;
        let ref_file = r.narrow_string()?;
        let scale = read_vec3(r)?;
        Ok(Self {
            ref_file,
            scale,
            timestamp,
            footprint,
            map,
            underside,
            external_links,
        })
    }

    pub fn write(&self, w: &mut Writer) -> bool {
        self.footprint.write(w);
        w.u32(u32::from(self.map.is_some()));
        if let Some(m) = &self.map {
            if !m.write(w) {
                return false;
            }
        }
        w.u32(u32::try_from(self.external_links.len()).unwrap_or(u32::MAX));
        for l in &self.external_links {
            l.write(w);
        }
        self.underside.write(w);
        w.narrow_string(&self.timestamp);
        w.narrow_string(&self.ref_file);
        write_vec3(w, self.scale);
        true
    }
}
