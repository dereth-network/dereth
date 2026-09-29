// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Triangle.cs
//! Port of `Source/ACE.Server/Entity/Triangle.cs`.

use empyrean_entity::numerics::{Vector2, Vector3};

// ACE: Triangle
/// Represents a 3D triangle for meshes, with some methods that operate in 2-space.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Triangle {
    /// To avoid storing many redundant vertices, only indices into the mesh vertices.
    // ACE: Triangle.Indices
    pub indices: [i32; 3],
}

/// `vertices[index]` (a negative or out-of-range index is ACE's `ArgumentOutOfRangeException`).
fn at(vertices: &[Vector3], index: i32) -> Vector3 {
    vertices[usize::try_from(index).expect("ACE: ArgumentOutOfRangeException")]
}

impl Triangle {
    // ACE: Triangle.Triangle
    /// Constructs a new triangle from 3 vertex indices (`new Triangle()` is all zeros:
    /// [`Triangle::default`]).
    #[must_use]
    pub const fn new(a: i32, b: i32, c: i32) -> Self {
        Self { indices: [a, b, c] }
    }

    // ACE: Triangle.GetVertices
    /// Returns the vertices of the triangle.
    #[must_use]
    pub fn get_vertices(&self, vertices: &[Vector3]) -> [Vector3; 3] {
        // TODO: out-of-bounds exception
        [
            at(vertices, self.indices[0]),
            at(vertices, self.indices[1]),
            at(vertices, self.indices[2]),
        ]
    }

    // ACE: Triangle.Contains
    /// Returns TRUE if point is contained within triangle.
    #[must_use]
    pub fn contains(&self, point: Vector2, vertices: &[Vector3]) -> bool {
        let p1 = at(vertices, self.indices[0]);
        let p2 = at(vertices, self.indices[1]);
        let p3 = at(vertices, self.indices[2]);

        // TODO: further optimizations listed
        // https://stackoverflow.com/questions/2049582/how-to-determine-if-a-point-is-in-a-2d-triangle
        let area = Self::area(p1, p2, p3);

        let s = 1.0f32 / (2.0 * area)
            * (p1.y * p3.x - p1.x * p3.y + (p3.y - p1.y) * point.x + (p1.x - p3.x) * point.y);
        if s < 0.0 {
            return false;
        }

        let t = 1.0f32 / (2.0 * area)
            * (p1.x * p2.y - p1.y * p2.x + (p1.y - p2.y) * point.x + (p2.x - p1.x) * point.y);
        if t < 0.0 || 1.0 - s - t < 0.0 {
            return false;
        }

        true
    }

    // ACE: Triangle.Area
    /// Returns the area of the 2D triangle.
    #[must_use]
    pub fn area(p1: Vector3, p2: Vector3, p3: Vector3) -> f32 {
        0.5f32 * (-p2.y * p3.x + p1.y * (-p2.x + p3.x) + p1.x * (p2.y - p3.y) + p2.x * p3.y)
    }

    // ACE: Triangle.GetZ
    /// Consider the triangle as a plane, find the Z-coordinate for a 2D coordinate on the plane.
    #[must_use]
    pub fn get_z(&self, vertices: &[Vector3], point: Vector2) -> f32 {
        // Reference:
        // https://social.msdn.microsoft.com/Forums/en-US/1b32dc40-f84d-4365-a677-b59e49d41eb0/how-to-calculate-a-point-on-a-plane-based-on-a-plane-from-3-points

        let p1 = at(vertices, self.indices[0]);
        let p2 = at(vertices, self.indices[1]);
        let p3 = at(vertices, self.indices[2]);

        let v1 = Vector3::new(p1.x - p3.x, p1.y - p3.y, p1.z - p3.z);
        let v2 = Vector3::new(p2.x - p3.x, p2.y - p3.y, p2.z - p3.z);

        let abc = Vector3::new(
            (v1.y * v2.z) - (v1.z * v2.y),
            (v1.z * v2.x) - (v1.x * v2.z),
            (v1.x * v2.y) - (v1.y * v2.x),
        );

        let d = (abc.x * p3.x) + (abc.y * p3.y) + (abc.z * p3.z);

        (d - (abc.x * point.x) - (abc.y * point.y)) / abc.z
    }
}
