//! An order to put a body somewhere: the place, how to treat it (slide to a free spot, force it into
//! its cell, scatter around it), the line and radii a scatter uses, and how many tries it gets. Its
//! binary form is the place, then the flags, the line, the two radii and the tries.

use auberean_dat::reader::Result;

use crate::pack::{
    read_position, read_vec3, write_position, write_vec3, Reader, Writer, POSITION_SIZE,
};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom;

/// The order's flags.
pub mod flags {
    /// Take the place as the body's placement afterwards.
    pub const PLACEMENT: u32 = 0x1;
    /// Accept a free spot the transition slides the body to.
    pub const SLIDE: u32 = 0x10;
    /// Put the body in the place's cell without checking for room.
    pub const FORCE: u32 = 0x40;
    /// Scatter around the place when the place itself is taken.
    pub const SCATTER_IF_BLOCKED: u32 = 0x100;
    /// Scatter around the place.
    pub const SCATTER: u32 = 0x200;
    /// Set by every simple placement.
    pub const SIMPLE: u32 = 0x1000;
}

/// An order to put a body somewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetPosition {
    pub position: Position,
    pub flags: u32,
    /// The unit line a scatter keeps to.
    pub line: Vec3,
    /// The scatter's nearest and farthest radius.
    pub radius_min: f32,
    pub radius_max: f32,
    pub tries: u32,
}

impl Default for SetPosition {
    // D2-06701, D2-06702
    fn default() -> Self {
        Self {
            position: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            flags: 0,
            line: Vec3::ZERO,
            radius_min: 0.0,
            radius_max: 0.0,
            tries: 0,
        }
    }
}

impl SetPosition {
    /// The binary size: the place and 28 bytes more.
    // D2-06703, D2-13184
    #[must_use]
    pub const fn pack_size() -> usize {
        POSITION_SIZE + 0x1c
    }

    // D2-06704
    pub fn write(&self, w: &mut Writer) {
        write_position(w, &self.position);
        w.u32(self.flags);
        write_vec3(w, self.line);
        w.f32(self.radius_min);
        w.f32(self.radius_max);
        w.u32(self.tries);
    }

    /// Reads an order; none from a buffer shorter than its size.
    // D2-06705
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let position = read_position(r)?;
        let flags = r.u32()?;
        let line = read_vec3(r)?;
        Ok(Self {
            position,
            flags,
            line,
            radius_min: r.f32()?,
            radius_max: r.f32()?,
            tries: r.u32()?,
        })
    }

    // D2-06706
    pub fn add_flags(&mut self, f: u32) {
        self.flags |= f;
    }

    // D2-06707
    pub fn remove_flags(&mut self, f: u32) {
        self.flags &= !f;
    }

    /// The scatter's radii; a farthest radius below zero means the nearest one.
    // D2-06708
    pub fn set_scatter_radius(&mut self, min: f32, max: f32) {
        self.radius_min = min;
        self.radius_max = if max < 0.0 { min } else { max };
    }

    // D2-06709
    pub fn set_tries(&mut self, n: u32) {
        self.tries = n;
    }

    /// The line a scatter keeps to, made a unit vector; a line under 0.0002 m long is refused.
    // D2-06710
    pub fn set_line(&mut self, v: Vec3) -> bool {
        let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        if len < 0.0002 {
            return false;
        }
        let k = 1.0 / len;
        self.line = Vec3::new(v.x * k, v.y * k, v.z * k);
        true
    }

    /// The line from one place to another.
    // D2-06711
    pub fn set_line_by_positions(&mut self, from: &Position, to: &Position) -> bool {
        self.set_line(geom::offset(from, to))
    }

    // D2-06712
    pub fn set_position(&mut self, p: Position) {
        self.position = p;
    }

    /// The order a simple placement makes: the place, sliding to a free spot if asked.
    #[must_use]
    pub fn simple(p: Position, slide: bool) -> Self {
        let mut s = Self::default();
        s.set_position(p);
        s.add_flags(if slide {
            flags::SIMPLE | flags::SLIDE
        } else {
            flags::SIMPLE
        });
        s
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the order's binary form and setters, held to made-up values)
    use super::*;

    #[test]
    fn an_order_round_trips_and_its_radii_and_line_follow_their_rules() {
        let mut s = SetPosition::simple(
            Position::new(
                CellId(0x1234_0005),
                Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY),
            ),
            true,
        );
        assert_eq!(s.flags, flags::SIMPLE | flags::SLIDE);
        s.set_scatter_radius(2.0, -1.0);
        assert!((s.radius_max - 2.0).abs() < f32::EPSILON);
        assert!(!s.set_line(Vec3::new(0.0001, 0.0, 0.0)));
        assert!(s.set_line(Vec3::new(0.0, 3.0, 4.0)));
        assert!((s.line.y - 0.6).abs() < 1e-6 && (s.line.z - 0.8).abs() < 1e-6);
        s.set_tries(7);
        let mut w = Writer::new();
        s.write(&mut w);
        let bytes = w.finish();
        assert_eq!(bytes.len(), SetPosition::pack_size());
        let back = SetPosition::read(&mut Reader::new(&bytes)).expect("reads back");
        assert_eq!(back, s);
    }
}
