// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/LandblockMesh.cs
//! Port of `Source/ACE.Server/Entity/LandblockMesh.cs`.
//!
//! The polygonal landblock mesh. ACE builds it only from `Landblock.LoadMeshes`, which ACE never
//! calls; it is ported for completeness. Its base class `Mesh` and the `Triangle`/`Line2` helpers
//! are not ported yet, so the mesh keeps `Mesh.Vertices` and each triangle's `Indices` itself, and
//! the members that need `Triangle`/`Line2` geometry are `not_ported!`.

// C# casts: `(float)Math.Floor(..)` and `(int)(float)`.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use empyrean_dat::file_types::CellLandblock;
use empyrean_dat::DatManager;
use empyrean_entity::numerics::{Vector2, Vector3};
use empyrean_entity::LandblockId;

// ACE: LandblockMesh.CellDim
/// A landblock has this many cells squared.
pub const CELL_DIM: i32 = 8;

// ACE: LandblockMesh.LandblockSize
/// A landblock is this unit size squared.
pub const LANDBLOCK_SIZE: i32 = 192;

// ACE: LandblockMesh.CellSize
/// A landblock cell is this unit size squared.
pub const CELL_SIZE: i32 = LANDBLOCK_SIZE / CELL_DIM;

// ACE: LandblockMesh.VertexDim
/// A landblock has this many vertices squared.
pub const VERTEX_DIM: i32 = CELL_DIM + 1;

/// `new Triangle(i0, i1, i2)`: the vertex indices (ACE's `Triangle.Indices`).
pub type TriangleIndices = [i32; 3];

// ACE: LandblockMesh
#[derive(Debug, Clone)]
pub struct LandblockMesh {
    // ACE: LandblockMesh.LandblockId
    pub landblock_id: LandblockId,
    // ACE: LandblockMesh.VertexHeights
    /// The heights of the landblock cell vertices, `[x][y]`.
    pub vertex_heights: Vec<Vec<f32>>,
    /// `Mesh.Vertices`.
    pub vertices: Vec<Vector3>,
    /// `Mesh.Triangles`.
    pub triangles: Vec<TriangleIndices>,
}

impl LandblockMesh {
    // ACE: LandblockMesh.LandblockMesh
    /// Constructs a new mesh for a landblock. ACE's static constructor reads `RegionDesc`
    /// (`0x13000000`) once; it is read here from `dats` (a missing region throws in ACE's static
    /// constructor; here it panics).
    pub fn new(dats: &DatManager, id: LandblockId) -> Self {
        let mut mesh = LandblockMesh {
            landblock_id: id,
            vertex_heights: Vec::new(),
            vertices: Vec::new(),
            triangles: Vec::new(),
        };

        mesh.build_vertices(dats);
        mesh.build_triangles();
        mesh
    }

    // ACE: LandblockMesh.BuildVertices
    /// Builds the vertices for the landblock cells.
    pub fn build_vertices(&mut self, dats: &DatManager) {
        self.vertex_heights = self.get_vertex_heights(dats);
        let heights = self.vertex_heights.clone();
        self.load_vertices(&heights);
    }

    // ACE: LandblockMesh.GetVertexHeights
    /// Reads the heights for each vertex in the landblock cells. The vertex heights in the cell
    /// database are stored in bytes, which map to offsets in the land height table from the
    /// region file in the portal database.
    pub fn get_vertex_heights(&self, dats: &DatManager) -> Vec<Vec<f32>> {
        let region_desc = dats
            .portal_dat()
            .try_region_desc()
            .expect("TypeInitializationException: LandblockMesh.RegionDesc");
        let cell_landblock = dats
            .cell_dat()
            .read_from_dat::<CellLandblock>(self.landblock_id.raw() | 0xFFFF)
            .expect("CellLandblock for LandblockMesh");

        let dim = VERTEX_DIM as usize;
        let mut heights = vec![vec![0.0f32; dim]; dim];

        for (x, column) in heights.iter_mut().enumerate() {
            for (y, height) in column.iter_mut().enumerate() {
                *height = region_desc.land_defs.land_height_table
                    [usize::from(cell_landblock.height[x * dim + y])];
            }
        }
        heights
    }

    // ACE: LandblockMesh.LoadVertices
    /// Loads the vertices for a landblock mesh.
    pub fn load_vertices(&mut self, height: &[Vec<f32>]) {
        let x_size = height.len();
        let y_size = height.first().map_or(0, Vec::len);

        self.vertices = Vec::with_capacity(x_size * y_size);

        for (x, column) in height.iter().enumerate() {
            for (y, &h) in column.iter().enumerate() {
                self.vertices.push(Vector3::new(
                    (x as i32 * CELL_SIZE) as f32,
                    (y as i32 * CELL_SIZE) as f32,
                    h,
                ));
            }
        }
    }

    // ACE: LandblockMesh.BuildTriangles
    /// Generates the triangles from the mesh vertices.
    pub fn build_triangles(&mut self) {
        self.triangles = Vec::new();

        for x in 0..CELL_DIM {
            for y in 0..CELL_DIM {
                let lower_left = x + y * VERTEX_DIM;
                let lower_right = (x + 1) + y * VERTEX_DIM;
                let top_left = x + (y + 1) * VERTEX_DIM;
                let top_right = (x + 1) + (y + 1) * VERTEX_DIM;

                // determine where to draw the split line
                if self.get_split_dir(self.landblock_id, x, y) {
                    // clockwise winding order
                    self.triangles.push([top_left, lower_right, lower_left]);
                    self.triangles.push([top_left, top_right, lower_right]);
                } else {
                    self.triangles.push([top_right, lower_right, lower_left]);
                    self.triangles.push([top_right, lower_left, top_left]);
                }
            }
        }
    }

    // ACE: LandblockMesh.GetSplitDir
    /// Determines the split line direction for a cell triangulation: true for a NW-SE split,
    /// false for NE-SW. C# `int` arithmetic, unchecked.
    #[allow(clippy::unused_self)]
    pub fn get_split_dir(&self, id: LandblockId, cell_x: i32, cell_y: i32) -> bool {
        // get the global tile offsets
        let x = i32::from(id.landblock_x()) * 8 + cell_x;
        let y = i32::from(id.landblock_y()) * 8 + cell_y;

        let dw = x
            .wrapping_mul(y)
            .wrapping_mul(0x0CCA_C033)
            .wrapping_sub(x.wrapping_mul(0x421B_E3BD))
            .wrapping_add(y.wrapping_mul(0x6C1A_C587))
            .wrapping_sub(0x519B_8F25);
        // `dw & 0x80000000` promotes to `long`: bit 31 of `dw`.
        (i64::from(dw) & 0x8000_0000) == 0
    }

    // ACE: LandblockMesh.GetSplitter
    /// Returns the shared line between 2 triangles.
    #[must_use]
    pub fn get_splitter(&self, triangles: &[TriangleIndices]) -> crate::entity::line2::Line2 {
        use crate::entity::line2::Line2;
        let v =
            |i: i32| self.vertices[usize::try_from(i).expect("ACE: ArgumentOutOfRangeException")];
        if triangles[0][1] == triangles[1][2] {
            Line2::from_vector3(v(triangles[0][0]), v(triangles[0][1]))
        } else {
            Line2::from_vector3(v(triangles[0][0]), v(triangles[0][2]))
        }
    }

    // ACE: LandblockMesh.GetTriangle
    /// Returns the triangle containing a pair of x,y coordinates.
    #[must_use]
    pub fn get_triangle(&self, point: Vector2) -> TriangleIndices {
        // find the cell which contains these coordinates
        let cell_offset = Self::get_cell(point);

        // get the triangles for this cell
        let cell_triangles = self.get_cell_triangles(cell_offset);

        // return the triangle containing this point
        if (crate::entity::triangle::Triangle {
            indices: cell_triangles[0],
        })
        .contains(point, &self.vertices)
        {
            cell_triangles[0]
        } else {
            cell_triangles[1]
        }
    }

    // ACE: LandblockMesh.GetCell
    /// Given a pair of 2D coordinates within a landblock, returns the cell that contains these
    /// coordinates.
    pub fn get_cell(point: Vector2) -> Vector2 {
        let mut cell_x = f64::from(point.x / CELL_SIZE as f32).floor() as f32;
        let mut cell_y = f64::from(point.y / CELL_SIZE as f32).floor() as f32;

        if cell_x < 0.0 {
            cell_x = 0.0;
        }
        if cell_y < 0.0 {
            cell_y = 0.0;
        }

        if cell_x >= CELL_DIM as f32 {
            cell_x = (CELL_DIM - 1) as f32;
        }
        if cell_y >= CELL_DIM as f32 {
            cell_y = (CELL_DIM - 1) as f32;
        }

        Vector2::new(cell_x, cell_y)
    }

    // ACE: LandblockMesh.GetCellTriangles
    /// Returns the 2 triangles for a cell.
    ///
    /// ACE-BUG: `BuildTriangles` lays the cells out x-major (`(x * 8 + y) * 2`), but this reads
    /// them y-major (`(y * 8 + x) * 2`), so off the diagonal it returns another cell's triangles.
    pub fn get_cell_triangles(&self, cell_offset: Vector2) -> [TriangleIndices; 2] {
        let offset = ((cell_offset.y * CELL_DIM as f32 + cell_offset.x) * 2.0) as usize;
        // (ACE notes: "ensure within bounds".)
        [self.triangles[offset], self.triangles[offset + 1]]
    }

    // ACE: LandblockMesh.GetZ
    /// Returns the z height coordinate for a 2D position within a landblock.
    #[must_use]
    pub fn get_z(&self, point: Vector2) -> f32 {
        // find the triangle that contains this point
        let triangle = self.get_triangle(point);

        // calculate the z coordinate at x,y
        // for the plane defined by this triangle
        crate::entity::triangle::Triangle { indices: triangle }.get_z(&self.vertices, point)
    }
}
