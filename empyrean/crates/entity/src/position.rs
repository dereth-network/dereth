// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Position.cs
//! `Position`: a cell id plus a frame (origin and rotation) in that cell's landblock.
//!
//! ACE's `Position` is a class; this is a plain value. Everything that reads like a field
//! assignment in ACE and runs code (`Pos = ...`, `LandblockId = ...`, `Rotation = ...`) is a
//! setter method here, and the setters have ACE's side effects: setting [`Position::set_pos`]
//! re-homes the position into the right landblock and outdoor cell, as ACE's `Pos` setter does.
//!
//! `Clone` is a field copy. ACE's copy constructor `new Position(p)` is **not** a field copy (it
//! goes through the `Pos` setter), so ported code that copies with `new Position(p)` calls
//! [`Position::from_position`].
//!
//! Methods whose ACE parameter may be `null` take `impl Into<Option<&Position>>`, so both
//! `a.distance_to(&b)` and `a.distance_to(maybe_b.as_ref())` work.

// C# narrowing integer casts are two's-complement truncation, i.e. `as`; float-to-integer casts go
// through `CsCast`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::fmt;

use empyrean_common::dotnet::{format, to_string, CsCast};

use crate::binary_io::BinaryReader;
use crate::enums::PositionFlags;
use crate::landblock_id::LandblockId;
use crate::numerics::{Quaternion, Vector2, Vector3};

/// The `Exception("Bad coordinates")` thrown by [`Position::from_coordinates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadCoordinates;

impl fmt::Display for BadCoordinates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Bad coordinates")
    }
}

impl std::error::Error for BadCoordinates {}

/// ACE: Position. No `PartialEq`: C#'s `==` on this class is reference identity; compare with
/// [`Position::equals`].
#[derive(Debug, Clone, Copy)]
pub struct Position {
    // ACE: Position.landblockId
    landblock_id: LandblockId,

    // ACE: Position.PositionX
    pub position_x: f32,
    // ACE: Position.PositionY
    pub position_y: f32,
    // ACE: Position.PositionZ
    pub position_z: f32,
    // ACE: Position.RotationW
    pub rotation_w: f32,
    // ACE: Position.RotationX
    pub rotation_x: f32,
    // ACE: Position.RotationY
    pub rotation_y: f32,
    // ACE: Position.RotationZ
    pub rotation_z: f32,
}

impl Default for Position {
    fn default() -> Self {
        Position::new()
    }
}

impl Position {
    // ACE: Position.BlockLength
    pub const BLOCK_LENGTH: i32 = 192;
    // ACE: Position.CellSide
    pub const CELL_SIDE: i32 = 8;
    // ACE: Position.CellLength
    pub const CELL_LENGTH: i32 = 24;

    // ------------------------------------------------------------------------------------------
    // Properties
    // ------------------------------------------------------------------------------------------

    /// The getter substitutes `new LandblockId(Cell)` for a zero id; `Cell` is the same zero, so
    /// this is the stored id in practice.
    // ACE: Position.LandblockId
    #[must_use]
    pub fn landblock_id(&self) -> LandblockId {
        if self.landblock_id.raw() != 0 {
            self.landblock_id
        } else {
            LandblockId::new(self.cell())
        }
    }

    // ACE: Position.LandblockId
    pub fn set_landblock_id(&mut self, value: LandblockId) {
        self.landblock_id = value;
    }

    // ACE: Position.Landblock
    #[must_use]
    pub fn landblock(&self) -> u32 {
        self.landblock_id.raw() >> 16
    }

    /// ACE's own comment: "FIXME: this is returning landblock + cell". It is the full cell id.
    // ACE: Position.Cell
    #[must_use]
    pub fn cell(&self) -> u32 {
        self.landblock_id.raw()
    }

    // ACE: Position.CellX
    #[must_use]
    pub fn cell_x(&self) -> u32 {
        self.landblock_id.raw() >> 8 & 0xFF
    }

    // ACE: Position.CellY
    #[must_use]
    pub fn cell_y(&self) -> u32 {
        self.landblock_id.raw() & 0xFF
    }

    // ACE: Position.LandblockX
    #[must_use]
    pub fn landblock_x(&self) -> u32 {
        self.landblock_id.raw() >> 24 & 0xFF
    }

    // ACE: Position.LandblockY
    #[must_use]
    pub fn landblock_y(&self) -> u32 {
        self.landblock_id.raw() >> 16 & 0xFF
    }

    // ACE: Position.GlobalCellX
    #[must_use]
    pub fn global_cell_x(&self) -> u32 {
        self.landblock_x()
            .wrapping_mul(8)
            .wrapping_add(self.cell_x())
    }

    // ACE: Position.GlobalCellY
    #[must_use]
    pub fn global_cell_y(&self) -> u32 {
        self.landblock_y()
            .wrapping_mul(8)
            .wrapping_add(self.cell_y())
    }

    // ACE: Position.Pos
    #[must_use]
    pub fn pos(&self) -> Vector3 {
        Vector3::new(self.position_x, self.position_y, self.position_z)
    }

    /// The `Pos` setter: [`Position::set_position`] with the result discarded.
    // ACE: Position.Pos
    pub fn set_pos(&mut self, value: Vector3) {
        let _ = self.set_position(value);
    }

    /// Stores `pos`, then moves across landblock boundaries and recomputes the outdoor cell.
    /// Returns `(blockUpdate, cellUpdate)`.
    // ACE: Position.SetPosition
    pub fn set_position(&mut self, pos: Vector3) -> (bool, bool) {
        self.position_x = pos.x;
        self.position_y = pos.y;
        self.position_z = pos.z;

        let block_update = self.set_landblock();
        let cell_update = self.set_land_cell();

        (block_update, cell_update)
    }

    // ACE: Position.Rotation
    #[must_use]
    pub fn rotation(&self) -> Quaternion {
        Quaternion::new(
            self.rotation_x,
            self.rotation_y,
            self.rotation_z,
            self.rotation_w,
        )
    }

    // ACE: Position.Rotation
    pub fn set_rotation(&mut self, value: Quaternion) {
        self.rotation_w = value.w;
        self.rotation_x = value.x;
        self.rotation_y = value.y;
        self.rotation_z = value.z;
    }

    /// Faces the heading of `dir` in the XY plane.
    // ACE: Position.Rotate
    pub fn rotate(&mut self, dir: Vector3) {
        let heading = empyrean_common::math::atan2(f64::from(-dir.x), f64::from(dir.y)) as f32;
        self.set_rotation(Quaternion::create_from_yaw_pitch_roll(0.0, 0.0, heading));
    }

    // ACE: Position.Indoors
    #[must_use]
    pub fn indoors(&self) -> bool {
        self.landblock_id.indoors()
    }

    /// The normalized heading direction.
    // ACE: Position.GetCurrentDir
    #[must_use]
    pub fn get_current_dir(&self) -> Vector3 {
        Vector3::normalize(Vector3::transform(Vector3::UNIT_Y, self.rotation()))
    }

    /// `v` scaled by the reciprocal of its length (not `Vector3.Normalize`, which divides).
    // ACE: Position.Normalize
    #[must_use]
    pub fn normalize(&self, v: Vector3) -> Vector3 {
        let inv_len = 1.0f32 / v.length();
        v * inv_len
    }

    /// A position `distance_in_front` ahead along the heading, 0.05 higher, keeping only the
    /// yaw (Z/W) part of the rotation.
    // ACE: Position.InFrontOf
    #[must_use]
    pub fn in_front_of(&self, distance_in_front: f64, rotate180: bool) -> Position {
        let qw = self.rotation_w; // north
        let qz = self.rotation_z; // south

        let x = f64::from(2.0f32 * qw * qz);
        let y = f64::from(1.0f32 - 2.0f32 * qz * qz);

        let heading = empyrean_common::math::atan2(x, y);
        // `-1 * x`: an exact sign flip.
        let dx = -((empyrean_common::math::sin(heading) * distance_in_front) as f32);
        let dy = (empyrean_common::math::cos(heading) * distance_in_front) as f32;

        // move the Z slightly up and let gravity pull it down.  just makes things easier.
        let bump_height = 0.05f32;
        if rotate180 {
            let rotate = Quaternion::new(0.0, 0.0, qz, qw)
                * Quaternion::create_from_yaw_pitch_roll(0.0, 0.0, std::f64::consts::PI as f32);
            Position::from_components(
                self.landblock_id().raw(),
                self.position_x + dx,
                self.position_y + dy,
                self.position_z + bump_height,
                0.0,
                0.0,
                rotate.z,
                rotate.w,
                false,
            )
        } else {
            Position::from_components(
                self.landblock_id().raw(),
                self.position_x + dx,
                self.position_y + dy,
                self.position_z + bump_height,
                0.0,
                0.0,
                qz,
                qw,
                false,
            )
        }
    }

    /// Handles the position crossing landblock boundaries. Outdoors only.
    ///
    /// ACE-BUG: a failed transition past the east or north edge clamps the coordinate to 192.0,
    /// one past the last cell, and [`Position::set_land_cell`] then computes a cell id above 64.
    /// A coordinate of exactly `-192 * k` over-shoots by one block and is brought back by the
    /// following `>= BlockLength` check.
    // ACE: Position.SetLandblock
    pub fn set_landblock(&mut self) -> bool {
        if self.indoors() {
            return false;
        }

        let mut changed_block = false;

        if self.position_x < 0.0 {
            let block_offset = CsCast::<i32>::cs_cast(self.position_x) / Self::BLOCK_LENGTH - 1;
            let landblock = self.landblock_id().transition_x(block_offset);
            if let Some(landblock) = landblock {
                self.set_landblock_id(landblock);
                self.position_x -= Self::BLOCK_LENGTH.wrapping_mul(block_offset) as f32;
                changed_block = true;
            } else {
                self.position_x = 0.0;
            }
        }

        if self.position_x >= Self::BLOCK_LENGTH as f32 {
            let block_offset = CsCast::<i32>::cs_cast(self.position_x) / Self::BLOCK_LENGTH;
            let landblock = self.landblock_id().transition_x(block_offset);
            if let Some(landblock) = landblock {
                self.set_landblock_id(landblock);
                self.position_x -= Self::BLOCK_LENGTH.wrapping_mul(block_offset) as f32;
                changed_block = true;
            } else {
                self.position_x = Self::BLOCK_LENGTH as f32;
            }
        }

        if self.position_y < 0.0 {
            let block_offset = CsCast::<i32>::cs_cast(self.position_y) / Self::BLOCK_LENGTH - 1;
            let landblock = self.landblock_id().transition_y(block_offset);
            if let Some(landblock) = landblock {
                self.set_landblock_id(landblock);
                self.position_y -= Self::BLOCK_LENGTH.wrapping_mul(block_offset) as f32;
                changed_block = true;
            } else {
                self.position_y = 0.0;
            }
        }

        if self.position_y >= Self::BLOCK_LENGTH as f32 {
            let block_offset = CsCast::<i32>::cs_cast(self.position_y) / Self::BLOCK_LENGTH;
            let landblock = self.landblock_id().transition_y(block_offset);
            if let Some(landblock) = landblock {
                self.set_landblock_id(landblock);
                self.position_y -= Self::BLOCK_LENGTH.wrapping_mul(block_offset) as f32;
                changed_block = true;
            } else {
                self.position_y = Self::BLOCK_LENGTH as f32;
            }
        }

        changed_block
    }

    /// Determines the outdoor land cell for the current position. Outdoors only.
    // ACE: Position.SetLandCell
    pub fn set_land_cell(&mut self) -> bool {
        if self.indoors() {
            return false;
        }

        // `uint / int` is a `long` division in C#.
        let cell_x =
            i64::from(CsCast::<u32>::cs_cast(self.position_x)) / i64::from(Self::CELL_LENGTH);
        let cell_y =
            i64::from(CsCast::<u32>::cs_cast(self.position_y)) / i64::from(Self::CELL_LENGTH);

        let cell_id = cell_x * i64::from(Self::CELL_SIDE) + cell_y + 1;

        let cur_cell_id = self.landblock_id().raw() & 0xFFFF;

        if cell_id == i64::from(cur_cell_id) {
            return false;
        }

        self.set_landblock_id(LandblockId::new(
            (i64::from(self.landblock_id().raw() & 0xFFFF_0000) | cell_id) as u32,
        ));
        true
    }

    // ------------------------------------------------------------------------------------------
    // Constructors
    // ------------------------------------------------------------------------------------------

    /// `new Position()`: cell 0, origin zero, identity rotation.
    // ACE: Position.Position
    #[must_use]
    pub fn new() -> Self {
        let mut p = Position {
            landblock_id: LandblockId::default(),
            position_x: 0.0,
            position_y: 0.0,
            position_z: 0.0,
            rotation_w: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
        };
        //Pos = Vector3.Zero;
        p.set_rotation(Quaternion::IDENTITY);
        p
    }

    /// `new Position(Position pos)`: goes through the `Pos` setter, so an outdoor position is
    /// re-homed.
    // ACE: Position.Position
    #[must_use]
    pub fn from_position(pos: &Position) -> Self {
        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(pos.landblock_id().raw()));
        p.set_pos(pos.pos());
        p.set_rotation(pos.rotation());
        p
    }

    /// `new Position(blockCellID, x, y, z, rotX, rotY, rotZ, rotW, relativePos)`. Note the
    /// quaternion argument order: x, y, z, w. A relative position is stored as given.
    // ACE: Position.Position
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn from_components(
        block_cell_id: u32,
        new_position_x: f32,
        new_position_y: f32,
        new_position_z: f32,
        new_rotation_x: f32,
        new_rotation_y: f32,
        new_rotation_z: f32,
        new_rotation_w: f32,
        relative_pos: bool,
    ) -> Self {
        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(block_cell_id));

        if !relative_pos {
            p.set_pos(Vector3::new(new_position_x, new_position_y, new_position_z));
            p.set_rotation(Quaternion::new(
                new_rotation_x,
                new_rotation_y,
                new_rotation_z,
                new_rotation_w,
            ));

            if (block_cell_id & 0xFFFF) == 0 {
                let pos = p.pos();
                let _ = p.set_position(pos);
            }
        } else {
            // position is marked as relative so pass in raw values and make no further adjustments.
            p.position_x = new_position_x;
            p.position_y = new_position_y;
            p.position_z = new_position_z;
            p.set_rotation(Quaternion::new(
                new_rotation_x,
                new_rotation_y,
                new_rotation_z,
                new_rotation_w,
            ));
        }
        p
    }

    /// `new Position(blockCellID, Vector3 position, Quaternion rotation)`.
    // ACE: Position.Position
    #[must_use]
    pub fn from_vectors(block_cell_id: u32, position: Vector3, rotation: Quaternion) -> Self {
        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(block_cell_id));

        p.set_pos(position);
        p.set_rotation(rotation);

        if (block_cell_id & 0xFFFF) == 0 {
            let pos = p.pos();
            let _ = p.set_position(pos);
        }
        p
    }

    /// `new Position(BinaryReader payload)`: cell, origin, then the rotation in wire order
    /// (w, x, y, z). `None` is .NET's `EndOfStreamException`.
    // ACE: Position.Position
    pub fn from_reader(payload: &mut BinaryReader<'_>) -> Option<Self> {
        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(payload.read_u32().ok()?));

        p.position_x = payload.read_f32().ok()?;
        p.position_y = payload.read_f32().ok()?;
        p.position_z = payload.read_f32().ok()?;

        // packet stream isn't the same order as the quaternion constructor
        p.rotation_w = payload.read_f32().ok()?;
        p.rotation_x = payload.read_f32().ok()?;
        p.rotation_y = payload.read_f32().ok()?;
        p.rotation_z = payload.read_f32().ok()?;
        Some(p)
    }

    /// `new Position(float northSouth, float eastWest)`: map coordinates to the centre of the
    /// outdoor cell, Z 0, identity rotation.
    ///
    /// # Errors
    /// [`BadCoordinates`] off the map, where ACE throws.
    // ACE: Position.Position
    pub fn from_coordinates(north_south: f32, east_west: f32) -> Result<Self, BadCoordinates> {
        let north_south = (north_south - 0.5f32) * 10.0f32;
        let east_west = (east_west - 0.5f32) * 10.0f32;

        let base_x: u32 = (east_west + 0x400 as f32).cs_cast();
        let base_y: u32 = (north_south + 0x400 as f32).cs_cast();

        if base_x >= 0x7F8 || base_y >= 0x7F8 {
            return Err(BadCoordinates); // TODO: Instead of throwing exception should we set to a default location?
        }

        let x_offset = ((base_x & 7) as f32 * 24.0f32) + 12.0;
        let y_offset = ((base_y & 7) as f32 * 24.0f32) + 12.0;
        // float zOffset = GetZFromCellXY(LandblockId.Raw, xOffset, yOffset);
        const Z_OFFSET: f32 = 0.0;

        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(Self::get_cell_from_base(base_x, base_y)));
        p.position_x = x_offset;
        p.position_y = y_offset;
        p.position_z = Z_OFFSET;
        p.set_rotation(Quaternion::IDENTITY);
        Ok(p)
    }

    /// `new Position(Vector2 coordinates)`: map coordinates (east-west in X, north-south in Y) to
    /// a global position; Z is left 0 for `PositionExtensions.AdjustMapCoords`.
    // ACE: Position.Position
    #[must_use]
    pub fn from_map_coordinates(mut coordinates: Vector2) -> Self {
        // convert from (-101.95, 102.05) to (0, 204)
        coordinates += Vector2::ONE * 101.95f32;

        // 204 = map clicks across dereth
        // 2040 = number of cells across dereth
        // 24 = meters per cell
        //var globalPos = coordinates / 204 * 2040 * 24;
        let global_pos = coordinates * 240.0f32; // simplified

        // inlining, this logic is in PositionExtensions.FromGlobal()
        let block_x = CsCast::<i32>::cs_cast(global_pos.x) / Self::BLOCK_LENGTH;
        let block_y = CsCast::<i32>::cs_cast(global_pos.y) / Self::BLOCK_LENGTH;

        let origin_x = global_pos.x % Self::BLOCK_LENGTH as f32;
        let origin_y = global_pos.y % Self::BLOCK_LENGTH as f32;

        let cell_x = CsCast::<i32>::cs_cast(origin_x) / Self::CELL_LENGTH;
        let cell_y = CsCast::<i32>::cs_cast(origin_y) / Self::CELL_LENGTH;

        let cell = cell_x
            .wrapping_mul(Self::CELL_SIDE)
            .wrapping_add(cell_y)
            .wrapping_add(1);

        let obj_cell_id = (block_x.wrapping_shl(24) | block_y.wrapping_shl(16) | cell) as u32;

        let mut p = Position::zeroed();
        p.set_landblock_id(LandblockId::new(obj_cell_id));

        p.set_pos(Vector3::new(origin_x, origin_y, 0.0)); // must use PositionExtensions.AdjustMapCoords() to get Z

        p.set_rotation(Quaternion::IDENTITY);
        p
    }

    /// The field state every C# constructor starts from: all zero (the rotation too).
    fn zeroed() -> Self {
        Position {
            landblock_id: LandblockId::default(),
            position_x: 0.0,
            position_y: 0.0,
            position_z: 0.0,
            rotation_w: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
        }
    }

    // ------------------------------------------------------------------------------------------
    // Serialization
    // ------------------------------------------------------------------------------------------

    /// `Serialize(BinaryWriter, PositionFlags, int animationFrame, bool writeLandblock)`.
    // ACE: Position.Serialize
    pub fn serialize_with_flags(
        &self,
        payload: &mut Vec<u8>,
        position_flags: PositionFlags,
        animation_frame: i32,
        write_landblock: bool,
    ) {
        payload.extend_from_slice(&position_flags.0.to_le_bytes());

        if write_landblock {
            payload.extend_from_slice(&self.landblock_id().raw().to_le_bytes());
        }

        payload.extend_from_slice(&self.position_x.to_le_bytes());
        payload.extend_from_slice(&self.position_y.to_le_bytes());
        payload.extend_from_slice(&self.position_z.to_le_bytes());

        if (position_flags & PositionFlags::OrientationHasNoW).0 == 0 {
            payload.extend_from_slice(&self.rotation_w.to_le_bytes());
        }

        if (position_flags & PositionFlags::OrientationHasNoX).0 == 0 {
            payload.extend_from_slice(&self.rotation_x.to_le_bytes());
        }

        if (position_flags & PositionFlags::OrientationHasNoY).0 == 0 {
            payload.extend_from_slice(&self.rotation_y.to_le_bytes());
        }

        if (position_flags & PositionFlags::OrientationHasNoZ).0 == 0 {
            payload.extend_from_slice(&self.rotation_z.to_le_bytes());
        }

        if (position_flags & PositionFlags::HasPlacementID).0 != 0 {
            // TODO: this is current animationframe_id when we are animating (?) - when we are not, how are we setting on the ground Position_id.
            payload.extend_from_slice(&animation_frame.to_le_bytes());
        }

        if (position_flags & PositionFlags::HasVelocity).0 != 0 {
            // velocity would go here
            payload.extend_from_slice(&0f32.to_le_bytes());
            payload.extend_from_slice(&0f32.to_le_bytes());
            payload.extend_from_slice(&0f32.to_le_bytes());
        }
    }

    /// `Serialize(BinaryWriter, bool writeQuaternion, bool writeLandblock)`.
    // ACE: Position.Serialize
    pub fn serialize(&self, payload: &mut Vec<u8>, write_quaternion: bool, write_landblock: bool) {
        if write_landblock {
            payload.extend_from_slice(&self.landblock_id().raw().to_le_bytes());
        }

        payload.extend_from_slice(&self.position_x.to_le_bytes());
        payload.extend_from_slice(&self.position_y.to_le_bytes());
        payload.extend_from_slice(&self.position_z.to_le_bytes());

        if write_quaternion {
            payload.extend_from_slice(&self.rotation_w.to_le_bytes());
            payload.extend_from_slice(&self.rotation_x.to_le_bytes());
            payload.extend_from_slice(&self.rotation_y.to_le_bytes());
            payload.extend_from_slice(&self.rotation_z.to_le_bytes());
        }
    }

    // ACE: Position.GetCellFromBase
    fn get_cell_from_base(base_x: u32, base_y: u32) -> u32 {
        let block_x = (base_x >> 3) as u8;
        let block_y = (base_y >> 3) as u8;
        let cell_x = (base_x & 7) as u8;
        let cell_y = (base_y & 7) as u8;

        let block = (i32::from(block_x) << 8 | i32::from(block_y)) as u32;
        let cell = (i32::from(cell_x) << 3 | i32::from(cell_y)) as u32;

        (block << 16) | (cell + 1)
    }

    // ------------------------------------------------------------------------------------------
    // Distances
    // ------------------------------------------------------------------------------------------

    /// `(this.LandblockX - p.LandblockX) * 192 + this.PositionX - p.PositionX`, in C#'s order:
    /// the `int` block offset is converted to `float` before the two additions.
    fn block_delta(a: u8, b: u8) -> f32 {
        ((i32::from(a) - i32::from(b)) * 192) as f32
    }

    /// The 3D squared distance; `float.MaxValue` for a null `p`.
    // ACE: Position.SquaredDistanceTo
    #[must_use]
    pub fn squared_distance_to<'a>(&self, p: impl Into<Option<&'a Position>>) -> f32 {
        let Some(p) = p.into() else { return f32::MAX };

        if p.landblock_id() == self.landblock_id() {
            let dx = self.position_x - p.position_x;
            let dy = self.position_y - p.position_y;
            let dz = self.position_z - p.position_z;
            dx * dx + dy * dy + dz * dz
        } else {
            // verify this is working correctly if one of these is indoors
            let dx = Self::block_delta(
                self.landblock_id().landblock_x(),
                p.landblock_id().landblock_x(),
            ) + self.position_x
                - p.position_x;
            let dy = Self::block_delta(
                self.landblock_id().landblock_y(),
                p.landblock_id().landblock_y(),
            ) + self.position_y
                - p.position_y;
            let dz = self.position_z - p.position_z;
            dx * dx + dy * dy + dz * dz
        }
    }

    /// The 2D distance; `float.MaxValue` for a null `p`.
    // ACE: Position.Distance2D
    #[must_use]
    pub fn distance_2d<'a>(&self, p: impl Into<Option<&'a Position>>) -> f32 {
        let Some(p) = p.into() else { return f32::MAX };

        // originally this returned the offset instead of distance...
        if p.landblock_id() == self.landblock_id() {
            let dx = self.position_x - p.position_x;
            let dy = self.position_y - p.position_y;
            f64::from(dx * dx + dy * dy).sqrt() as f32
        } else {
            // verify this is working correctly if one of these is indoors
            let dx = Self::block_delta(
                self.landblock_id().landblock_x(),
                p.landblock_id().landblock_x(),
            ) + self.position_x
                - p.position_x;
            let dy = Self::block_delta(
                self.landblock_id().landblock_y(),
                p.landblock_id().landblock_y(),
            ) + self.position_y
                - p.position_y;
            f64::from(dx * dx + dy * dy).sqrt() as f32
        }
    }

    /// The squared 2D distance; `float.MaxValue` for a null `p`.
    // ACE: Position.Distance2DSquared
    #[must_use]
    pub fn distance_2d_squared<'a>(&self, p: impl Into<Option<&'a Position>>) -> f32 {
        let Some(p) = p.into() else { return f32::MAX };

        // originally this returned the offset instead of distance...
        if p.landblock_id() == self.landblock_id() {
            let dx = self.position_x - p.position_x;
            let dy = self.position_y - p.position_y;
            dx * dx + dy * dy
        } else {
            // verify this is working correctly if one of these is indoors
            let dx = Self::block_delta(
                self.landblock_id().landblock_x(),
                p.landblock_id().landblock_x(),
            ) + self.position_x
                - p.position_x;
            let dy = Self::block_delta(
                self.landblock_id().landblock_y(),
                p.landblock_id().landblock_y(),
            ) + self.position_y
                - p.position_y;
            dx * dx + dy * dy
        }
    }

    /// The 3D distance; `float.MaxValue` for a null `p`.
    // ACE: Position.DistanceTo
    #[must_use]
    pub fn distance_to<'a>(&self, p: impl Into<Option<&'a Position>>) -> f32 {
        let Some(p) = p.into() else { return f32::MAX };

        // originally this returned the offset instead of distance...
        if p.landblock_id() == self.landblock_id() {
            let dx = self.position_x - p.position_x;
            let dy = self.position_y - p.position_y;
            let dz = self.position_z - p.position_z;
            f64::from(dx * dx + dy * dy + dz * dz).sqrt() as f32
        } else {
            // verify this is working correctly if one of these is indoors
            let dx = Self::block_delta(
                self.landblock_id().landblock_x(),
                p.landblock_id().landblock_x(),
            ) + self.position_x
                - p.position_x;
            let dy = Self::block_delta(
                self.landblock_id().landblock_y(),
                p.landblock_id().landblock_y(),
            ) + self.position_y
                - p.position_y;
            let dz = self.position_z - p.position_z;

            f64::from(dx * dx + dy * dy + dz * dz).sqrt() as f32
        }
    }

    /// The offset from this position to `p`, across landblocks; all `float.MaxValue` for a null
    /// `p`.
    // ACE: Position.GetOffset
    #[must_use]
    pub fn get_offset<'a>(&self, p: impl Into<Option<&'a Position>>) -> Vector3 {
        let Some(p) = p.into() else {
            return Vector3::new(f32::MAX, f32::MAX, f32::MAX);
        };

        let dx = Self::block_delta(
            p.landblock_id().landblock_x(),
            self.landblock_id().landblock_x(),
        ) + p.position_x
            - self.position_x;
        let dy = Self::block_delta(
            p.landblock_id().landblock_y(),
            self.landblock_id().landblock_y(),
        ) + p.position_y
            - self.position_y;
        let dz = p.position_z - self.position_z;

        Vector3::new(dx, dy, dz)
    }

    /// `0xCCCCCCCC [x y z] w x y z` with six decimals.
    // ACE: Position.ToLOCString
    #[must_use]
    pub fn to_loc_string(&self) -> String {
        format!(
            "0x{} [{} {} {}] {} {} {} {}",
            format(self.landblock_id().raw(), "X8"),
            format(self.position_x, "F6"),
            format(self.position_y, "F6"),
            format(self.position_z, "F6"),
            format(self.rotation_w, "F6"),
            format(self.rotation_x, "F6"),
            format(self.rotation_y, "F6"),
            format(self.rotation_z, "F6"),
        )
    }

    /// Same cell, and `Pos`/`Rotation` equal by `Equals` (NaN equals NaN); false for a null `p`.
    // ACE: Position.Equals
    #[must_use]
    pub fn equals<'a>(&self, p: impl Into<Option<&'a Position>>) -> bool {
        match p.into() {
            Some(p) => {
                self.cell() == p.cell()
                    && self.pos().equals(p.pos())
                    && self.rotation().equals(p.rotation())
            }
            None => false,
        }
    }
}

/// `CCCCCCCC [x y z]`, the floats in .NET's shortest round-trip form.
// ACE: Position.ToString
impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{} {} {}]",
            format(self.landblock_id().raw(), "X8"),
            to_string(self.position_x),
            to_string(self.position_y),
            to_string(self.position_z)
        )
    }
}
