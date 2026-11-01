//! The binary forms of what tracking keeps (what a watcher hears, a watcher), and the place a missile
//! aims at with its own binary form.

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};
use dereth_primitives::{Position, Vec3};

use super::{origin, TargetInfo, Voyeur};
use crate::pack::{read_position, read_vec3, write_position, write_vec3, POSITION_SIZE};

impl TargetInfo {
    /// The binary size: two places and 68 bytes more.
    // D2-07219
    #[must_use]
    pub const fn pack_size() -> usize {
        2 * POSITION_SIZE + 0x44
    }

    /// Its binary form. The heading toward the projected place is not kept; the object's height
    /// comes before its radius.
    // D2-07220
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.context);
        w.u64(self.object);
        w.f32(self.radius);
        w.f64(self.quantum);
        write_position(w, &self.target_position);
        write_position(w, &self.interpolated_position);
        write_vec3(w, self.velocity);
        w.u32(self.status);
        w.f64(self.last_update_time);
        w.f32(self.object_height);
        w.f32(self.object_radius);
        write_vec3(w, self.scale);
    }

    // D2-07221
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let context = r.u32()?;
        let object = r.u64()?;
        let radius = r.f32()?;
        let quantum = r.f64()?;
        let target_position = read_position(r)?;
        let interpolated_position = read_position(r)?;
        let velocity = read_vec3(r)?;
        let status = r.u32()?;
        let last_update_time = r.f64()?;
        let object_height = r.f32()?;
        let object_radius = r.f32()?;
        let scale = read_vec3(r)?;
        Ok(Self {
            context,
            object,
            radius,
            quantum,
            target_position,
            interpolated_position,
            interpolated_heading: Vec3::ZERO,
            velocity,
            status,
            last_update_time,
            object_radius,
            object_height,
            scale,
        })
    }
}

impl Voyeur {
    /// The binary size: the place last told and 20 bytes more.
    // D2-07213
    #[must_use]
    pub const fn pack_size() -> usize {
        POSITION_SIZE + 0x14
    }

    // D2-07214
    pub fn write(&self, w: &mut Writer) {
        w.u64(self.object);
        w.f64(self.quantum);
        w.f32(self.radius);
        write_position(w, &self.last_sent);
    }

    // D2-07215
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let object = r.u64()?;
        let quantum = r.f64()?;
        let radius = r.f32()?;
        let last_sent = read_position(r)?;
        Ok(Self {
            object,
            radius,
            quantum,
            last_sent,
        })
    }
}

/// The place a missile aims at: an object (none for a bare place), the place, an offset from it,
/// and whether it holds anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub object: u64,
    pub position: Position,
    pub offset: Vec3,
    pub valid: bool,
}

/// The object id that names no object.
pub const NO_OBJECT: u64 = 0;

impl Default for Aim {
    // D2-04008
    fn default() -> Self {
        Self {
            object: NO_OBJECT,
            position: origin(),
            offset: Vec3::ZERO,
            valid: false,
        }
    }
}

impl Aim {
    /// Aims at a bare place: no object, no offset.
    // D2-07169
    pub fn reset(&mut self, p: Position) {
        self.position = p;
        self.object = NO_OBJECT;
        self.offset = Vec3::ZERO;
        self.valid = true;
    }

    /// Aims at a place with an offset, keeping the object.
    // D2-07170
    pub fn set_target_info(&mut self, p: Position, offset: Vec3) {
        self.valid = true;
        self.position = p;
        self.offset = offset;
    }

    /// Its binary form: the object, the place, the offset, whether it holds anything.
    // D2-04047
    pub fn write(&self, w: &mut Writer) {
        w.u64(self.object);
        write_position(w, &self.position);
        write_vec3(w, self.offset);
        w.u32(u32::from(self.valid));
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            object: r.u64()?,
            position: read_position(r)?,
            offset: read_vec3(r)?,
            valid: r.u32()? != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (binary forms round-tripped over made-up values)
    use super::*;
    use crate::target::status;
    use dereth_primitives::{CellId, Frame, Quat};

    fn at(x: f32) -> Position {
        Position::new(
            CellId(0x2020_0011),
            Frame::new(Vec3::new(x, 2.0, 3.0), Quat::new(0.6, 0.0, 0.0, 0.8)),
        )
    }

    #[test]
    fn what_a_watcher_hears_round_trips_with_height_before_radius() {
        let info = TargetInfo {
            context: 3,
            object: 0x1_0000_0002,
            radius: 1.5,
            quantum: 0.5,
            target_position: at(1.0),
            interpolated_position: at(2.0),
            velocity: Vec3::new(1.0, 0.0, 0.0),
            status: status::OK,
            last_update_time: 12.25,
            object_radius: 0.4,
            object_height: 1.8,
            ..TargetInfo::default()
        };
        let mut w = Writer::new();
        info.write(&mut w);
        let b = w.finish();
        assert_eq!(b.len(), TargetInfo::pack_size());
        let h = 4 + 8 + 4 + 8 + 2 * POSITION_SIZE + 12 + 4 + 8;
        assert_eq!(
            f32::from_le_bytes([b[h], b[h + 1], b[h + 2], b[h + 3]]),
            1.8
        );
        assert_eq!(TargetInfo::read(&mut Reader::new(&b)).expect("reads"), info);
    }

    #[test]
    fn a_watcher_and_an_aim_round_trip() {
        let v = Voyeur {
            last_sent: at(5.0),
            ..Voyeur::new(9, 2.0, 0.25)
        };
        let mut w = Writer::new();
        v.write(&mut w);
        let b = w.finish();
        assert_eq!(b.len(), Voyeur::pack_size());
        assert_eq!(Voyeur::read(&mut Reader::new(&b)).expect("reads"), v);

        let mut t = Aim {
            object: 4,
            ..Aim::default()
        };
        t.set_target_info(at(1.0), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(t.object, 4);
        let mut w = Writer::new();
        t.write(&mut w);
        let b = w.finish();
        assert_eq!(Aim::read(&mut Reader::new(&b)).expect("reads"), t);
        t.reset(at(3.0));
        assert_eq!(t.object, NO_OBJECT);
        assert!(t.valid && t.offset == Vec3::ZERO);
    }
}
