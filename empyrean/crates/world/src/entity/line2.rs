// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Line2.cs
//! Port of `Source/ACE.Server/Entity/Line2.cs`: a 2D line. ACE's static and instance overloads
//! become one associated function per argument shape.

use empyrean_common::dotnet::numerics::{Vector2, Vector3};

// ACE: Line2
/// Represents a 2D line.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Line2 {
    /// The start point.
    pub start: Vector2,
    /// The end point.
    pub end: Vector2,
}

impl Line2 {
    // ACE: Line2.Line2
    /// Constructs a line from 2 points.
    #[must_use]
    pub const fn new(start: Vector2, end: Vector2) -> Self {
        Self { start, end }
    }

    // ACE: Line2.Line2
    /// Constructs a line from 2 points.
    #[must_use]
    pub const fn from_coords(start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> Self {
        Self {
            start: Vector2::new(start_x, start_y),
            end: Vector2::new(end_x, end_y),
        }
    }

    // ACE: Line2.Line2
    /// Constructs a 2D line from 3D points, dropping the Z-component.
    #[must_use]
    pub const fn from_vector3(start: Vector3, end: Vector3) -> Self {
        Self {
            start: Vector2::new(start.x, start.y),
            end: Vector2::new(end.x, end.y),
        }
    }

    // ACE: Line2.Determinant
    /// Returns the determinant of a line at point.
    #[must_use]
    pub fn determinant_xy(line: &Line2, x: f32, y: f32) -> f32 {
        (line.end.x - line.start.x) * (y - line.start.y)
            - (line.end.y - line.start.y) * (x - line.start.x)
    }

    // ACE: Line2.Determinant
    #[must_use]
    pub fn determinant(&self, point: Vector2) -> f32 {
        Self::determinant_xy(self, point.x, point.y)
    }

    // ACE: Line2.Collinear
    /// Returns TRUE if point resides on a line.
    #[must_use]
    pub fn collinear(&self, point: Vector2) -> bool {
        self.determinant(point) == 0.0
    }

    // ACE: Line2.Collinear
    #[must_use]
    pub fn collinear_xy(&self, x: f32, y: f32) -> bool {
        Self::determinant_xy(self, x, y) == 0.0
    }

    // ACE: Line2.LeftSide
    /// Returns TRUE if point is on left side of line. If we draw a horizontal line from left to
    /// right, the left side is considered to be the top.
    #[must_use]
    pub fn left_side(&self, point: Vector2) -> bool {
        self.determinant(point) < 0.0
    }

    // ACE: Line2.LeftSide
    #[must_use]
    pub fn left_side_xy(&self, x: f32, y: f32) -> bool {
        Self::determinant_xy(self, x, y) < 0.0
    }

    // ACE: Line2.RightSide
    /// Returns TRUE if point is on right side of line. If we draw a horizontal line from left to
    /// right, the right side is considered to be the bottom.
    #[must_use]
    pub fn right_side(&self, point: Vector2) -> bool {
        self.determinant(point) > 0.0
    }

    // ACE: Line2.RightSide
    #[must_use]
    pub fn right_side_xy(&self, x: f32, y: f32) -> bool {
        Self::determinant_xy(self, x, y) > 0.0
    }
}
