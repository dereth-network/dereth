//! The **detail-texture subsystem** and the four surface and tiling slots it fills.
//!
//! Without a detail pass the *Environment Detail Textures* option would have no effect.
//! This module is the generation half of that pass, recovered from the executable bytes.
//!
//! # Observable behavior
//!
//! Exactly three paths configure detail texturing. All three disable the landscape and object
//! surfaces and give the same preference value to the environment and building surfaces. Thus the
//! shipped client never creates landscape or object detail surfaces; the preference affects only
//! **buildings** and **environment cells**.
//!
//! Configuration follows this order:
//!
//! ```text
//! if no current region exists: return false
//! release all four existing detail surfaces
//! for cls in [landscape(0), environment(2), building(1), object(3)]:   ; that order, always
//!     set the class's tiling from the region terrain descriptor
//! if landscape is enabled:   generate class 0
//! if environment is enabled: generate class 2
//! if building is enabled:    generate class 1
//! if object is enabled:      generate class 3
//! return true
//! ```
//!
//! The **tiling is set for all four unconditionally**, even for a class whose surface is not
//! generated; only the surfaces are gated. Cleanup releases landscape, environment, building and
//! object surfaces in that order.
//!
//! Generating a detail surface for a class:
//!
//! ```text
//! tex = region detail texture id for cls
//! if tex is invalid: return no surface
//! allocate a custom surface of kind 4
//! load the surface texture for tex
//! if loading fails: release the custom surface and return no surface
//! attach the texture map with usage 4 and set the DETAIL bit 0x20000
//! return the surface
//! ```
//!
//! Both accessors bottom out in the **region's `TexMerge`**. An out-of-range class returns the
//! invalid value; otherwise the texture id or tiling comes from `terrain_desc[cls]`.
//!
//! **The class index selects the region's terrain-descriptor array; it is not a terrain type.** In the
//! shipped region the first four rows carry the four detail classes:
//!
//! | class | row | `detail_tex_gid` | `detail_tex_tiling` |
//! |---|---|---|---|
//! | [`DetailClass::Landscape`] | 0 | `0x05001786` | 4 |
//! | [`DetailClass::Building`] | 1 | `0x05001787` | 4 |
//! | [`DetailClass::Environment`] | 2 | `0x05001787` | 4 |
//! | [`DetailClass::Object`] | 3 | `0x05001786` | 4 |
//!
//! (rows 4..32 all carry `0x050012AF` at tiling 1, which nothing ever reads.)
//!
//! # What the tiling factor means
//!
//! Detail tiling is a **uniform scale of the mesh's second UV set from its first**:
//!
//! ```text
//! if mesh.fvf != 0x252: return false          // XYZ|NORMAL|DIFFUSE|TEX2, 44-byte vertices
//! for each vertex v:
//!     v.uv1.x = factor * v.uv0.x
//!     v.uv1.y = factor * v.uv0.y
//! mesh.detail_tiling_factor = factor
//! ```
//!
//! which is why this build needs no second UV set in its vertex buffers: `uv1 = uv0 * tiling` is a
//! shader constant. Mesh setup selects FVF `0x252` over `0x152`
//! exactly when the tiling factor it is handed is `> 0`, and its two
//! callers pass **1.0** for an object mesh and **3.0** for an interior-cell mesh. Every world mesh is
//! therefore detail-capable, and draw time re-tiles it when the factor differs.

use dereth_assets::region::Region;
use dereth_primitives::DataId;

/// One of the four detail classes, indexed into the region's terrain-descriptor array. Surface generation uses
/// indices 0, 2, 1, and 3 for landscape, environment, building, and object respectively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum DetailClass {
    /// The landscape's detail surface and tiling.
    /// Read by the landscape draw and **never set**: see the module note.
    Landscape = 0,
    /// The building detail surface and tiling, selected by the building draw.
    Building = 1,
    /// The environment-cell detail surface and tiling, selected by the cell draw.
    Environment = 2,
    /// The object detail surface and tiling. Nothing
    /// reads it in the shipped client, and nothing sets it either.
    Object = 3,
}

impl DetailClass {
    /// The four classes in the order their **tiling** factors are written:
    /// landscape, environment, building, object.
    pub const TILING_ORDER: [Self; 4] = [
        Self::Landscape,
        Self::Environment,
        Self::Building,
        Self::Object,
    ];

    /// Index used by the region's detail-texture and detail-tiling lookups.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The current detail source and destination blend factors as the draw that selects this
    /// class sets them, as D3D9 `D3DBLEND` numbers.
    ///
    /// * the landscape block draw: 5 `SRCALPHA`, 6 `INVSRCALPHA`.
    /// * the environment-cell and building draws: 9 `DESTCOLOR`, 6 `INVSRCALPHA`.
    #[must_use]
    pub const fn blend(self) -> (u32, u32) {
        match self {
            Self::Landscape => (5, 6),
            Self::Building | Self::Environment | Self::Object => (9, 6),
        }
    }
}

/// One generated detail surface, reduced to the two things a build with no surface object needs:
/// the texture it wraps and the
/// tiling factor its class carries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetailSurface {
    /// The returned `SurfaceTexture` (`DB_TYPE` `0x0B`) id.
    pub texture: DataId,
    /// Region detail-tiling value converted from `u32` to `f32` with the unsigned fixup.
    pub tiling: f32,
    /// Set the `DETAIL` surface bit `0x20000`. Always true for a surface
    /// this function produced; carried so a caller can assert it rather than assume it.
    pub detail_bit: bool,
}

/// The renderer's complete detail state: the four surfaces and the four tiling factors.
///
/// This is the landscape's four surface slots and the renderer's four surface and tiling
/// globals in one place, because in this build they have one owner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DetailTexturing {
    surfaces: [Option<DetailSurface>; 4],
    tiling: [f32; 4],
    /// How many times [`Self::set`] has run — the instrument that separates "never asked" from
    /// "asked and generated nothing".
    applies: u32,
}

/// The id returned for an out-of-range class and checked before surface generation.
///
/// A `DataId` of 0 is this build's sentinel because
/// [`dereth_assets::region::TerrainDesc::detail_tex_gid`] is already decoded and an absent id
/// decodes as 0.
pub const NO_DETAIL_TEX: DataId = DataId(0);

impl DetailTexturing {
    /// Nothing generated and every tiling factor zero: the state before first configuration and
    /// the surface state after cleanup.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            surfaces: [None, None, None, None],
            tiling: [0.0; 4],
            applies: 0,
        }
    }

    /// Release all four surfaces in landscape, environment, building, object order. It does
    /// **not** clear the
    /// tiling factors, and neither does this.
    pub fn cleanup(&mut self) {
        self.surfaces = [None, None, None, None];
    }

    /// Set the landscape, building, environment and object detail-texture flags.
    ///
    /// Returns `false` without touching anything when there is no region, which is the
    /// client's no-current-region early-out.
    pub fn set(
        &mut self,
        region: Option<&Region>,
        landscape: bool,
        building: bool,
        environment: bool,
        object: bool,
    ) -> bool {
        let Some(region) = region else { return false };
        self.applies += 1;
        self.cleanup();
        for cls in DetailClass::TILING_ORDER {
            self.tiling[cls.index()] = detail_tiling(region, cls);
        }
        // In the client's own order: landscape, environment, building,
        // object. The order is unobservable — the four write four different globals — and is kept
        // to match the client's order.
        for (cls, want) in [
            (DetailClass::Landscape, landscape),
            (DetailClass::Environment, environment),
            (DetailClass::Building, building),
            (DetailClass::Object, object),
        ] {
            if want {
                self.surfaces[cls.index()] = generate_detail_surface(region, cls);
            }
        }
        true
    }

    /// Current detail surface for a class.
    #[must_use]
    pub const fn surface(&self, cls: DetailClass) -> Option<DetailSurface> {
        self.surfaces[cls as usize]
    }

    /// Current detail tiling. Zero until the first [`Self::set`], and zero for a class
    /// whose region row carries no tiling.
    #[must_use]
    pub const fn tiling(&self, cls: DetailClass) -> f32 {
        self.tiling[cls as usize]
    }

    /// The surface, tiling, and two blend factors installed immediately before
    /// draw content of this class: `None` when the class has no surface, which is what leaves
    /// the current detail surface null and the draw's want-detail flag false.
    #[must_use]
    pub fn current(&self, cls: DetailClass) -> Option<CurrentDetail> {
        let surface = self.surface(cls)?;
        let (src_blend, dst_blend) = cls.blend();
        Some(CurrentDetail {
            class: cls,
            surface,
            tiling: self.tiling(cls),
            src_blend,
            dst_blend,
        })
    }

    /// How many times [`Self::set`] has run with a region. **The instrument**: a
    /// preference that reaches a subsystem which then generates nothing must not print like a
    /// preference that reaches nothing at all.
    #[must_use]
    pub const fn applies(&self) -> u32 {
        self.applies
    }

    /// How many of the four classes currently hold a surface.
    #[must_use]
    pub fn generated(&self) -> usize {
        self.surfaces.iter().filter(|s| s.is_some()).count()
    }
}

/// Detail state for one draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentDetail {
    pub class: DetailClass,
    pub surface: DetailSurface,
    pub tiling: f32,
    /// `D3DBLEND` numbers; see [`DetailClass::blend`].
    pub src_blend: u32,
    pub dst_blend: u32,
}

/// The detail tiling accessor, with the client's integer-to-float conversion and unsigned fixup
/// applied: `if (cls >= count) 0 else terrain_desc[cls]`.
#[must_use]
pub fn detail_tiling(region: &Region, cls: DetailClass) -> f32 {
    region
        .land_surf
        .tex_merge
        .as_ref()
        .and_then(|tm| tm.terrain_desc.get(cls.index()))
        // The client converts the word as a signed integer and adds 2^32 when it is negative,
        // which is an unsigned conversion.
        .map_or(0.0, |d| d.detail_tex_tiling as f32)
}

/// The detail texture accessor.
#[must_use]
pub fn detail_tex(region: &Region, cls: DetailClass) -> DataId {
    region
        .land_surf
        .tex_merge
        .as_ref()
        .and_then(|tm| tm.terrain_desc.get(cls.index()))
        .map_or(NO_DETAIL_TEX, |d| d.detail_tex_gid)
}

/// The dat-backed half — "does the `SurfaceTexture`
/// actually load" — belongs to the caller, which is the only side that has a dat store; this is
/// the id test and the `DETAIL` bit.
#[must_use]
pub fn generate_detail_surface(region: &Region, cls: DetailClass) -> Option<DetailSurface> {
    let texture = detail_tex(region, cls);
    if texture == NO_DETAIL_TEX {
        return None;
    }
    Some(DetailSurface {
        texture,
        tiling: detail_tiling(region, cls),
        detail_bit: true,
    })
}

/// The **landscape** detail pass's per-vertex distance fade, written into the diffuse alpha byte.
///
/// ```text
/// if (z <  10.0f) return 0xFF;
/// if (z >  50.0f) return 0x00;
/// return trunc_u8((1.0f - (z - 10.0f) * 0.025) * 255.0f)
/// ```
///
/// `0.025` is `1 / (far - near)`. This is the only
/// distance term the whole subsystem has, and because a successor that revives the landscape arm
/// needs it — **nothing in the shipped client reaches it**, since the landscape's detail surface is
/// never set (see the module note).
#[must_use]
#[allow(clippy::neg_cmp_op_on_partial_ord)] // a NaN depth takes the far arm, as the client's compare does
#[allow(clippy::cast_possible_truncation)] // truncation toward zero is the client's; clamped to a byte
pub fn get_alpha_for_z(z: f32) -> u8 {
    const NEAR: f32 = 10.0;
    const FAR: f32 = 50.0;
    if z < NEAR {
        return 0xFF;
    }
    if !(z <= FAR) {
        return 0x00;
    }
    // The client's float-to-int conversion truncates toward zero, and the expression cannot be
    // negative in this band.
    dereth_primitives::num::to_i32((1.0 - (z - NEAR) / (FAR - NEAR)) * 255.0).clamp(0, 255) as u8
}

/// Gate on the landscape detail arm:
///
/// ```text
///   cellSide = the landblock's cell-grid side
///   isFullBlock = (cellSide == 8)          ; -> the `if`
/// ```
///
/// i.e. **only a block drawn at the full 8x8 cell grid** takes the landscape detail surface; a
/// LOD'd ring block does not. Dead alongside the rest of the landscape arm, and recorded because
/// a successor reviving it must not skip it.
#[must_use]
pub const fn block_takes_landscape_detail(cell_grid_side: u32) -> bool {
    cell_grid_side == 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::region::{CodeTexture, LandSurf, TerrainDesc, TexMerge};

    fn desc(i: u32, tex: u32, tiling: u32) -> TerrainDesc {
        TerrainDesc {
            terrain_type: i,
            tex_gid: DataId(0x0500_0000 + i),
            tex_tiling: 2,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: tiling,
            detail_tex_gid: DataId(tex),
        }
    }

    /// The shipped region's first four rows, which is what the four classes read.
    fn region() -> Region {
        let tm = TexMerge {
            base_tex_size: 1024,
            corner_terrain_maps: Vec::<CodeTexture>::new(),
            side_terrain_maps: Vec::new(),
            road_maps: Vec::new(),
            terrain_desc: vec![
                desc(0, 0x0500_1786, 4),
                desc(1, 0x0500_1787, 4),
                desc(2, 0x0500_1787, 4),
                desc(3, 0x0500_1786, 4),
                desc(4, 0x0500_12AF, 1),
            ],
        };
        Region {
            id: DataId(0x1300_0000),
            region_number: 1,
            version: 3,
            region_name: String::from("Dereth"),
            land_defs: dereth_assets::region::LandDefs {
                num_block_length: 255,
                num_block_width: 255,
                square_length: 24.0,
                lblock_length: 8,
                vertex_per_cell: 1,
                max_obj_height: 200.0,
                sky_height: 1000.0,
                road_width: 5.0,
                land_height_table: vec![0.0; 256],
            },
            game_time: dereth_assets::region::GameTime {
                zero_time_of_year: 0.0,
                zero_year: 0,
                day_length: 1.0,
                days_per_year: 1,
                year_spec: String::new(),
                times_of_day: Vec::new(),
                days_of_the_week: Vec::new(),
                seasons: Vec::new(),
            },
            parts_mask: 0,
            sky_info: None,
            sound_info: None,
            scene_info: None,
            terrain_types: Vec::new(),
            land_surf: LandSurf {
                surf_type: 0,
                tex_merge: Some(tm),
                pal_shift: None,
            },
            region_misc: None,
        }
    }

    #[test]
    fn the_shipped_call_shape_generates_exactly_building_and_environment() {
        let r = region();
        let mut d = DetailTexturing::new();
        // Disabling landscape detail with value `v` produces `(0, v, v, 0)`.
        assert!(d.set(Some(&r), false, true, true, false));
        assert_eq!(
            d.generated(),
            2,
            "landscape and object are never generated in retail"
        );
        assert_eq!(d.surface(DetailClass::Landscape), None);
        assert_eq!(d.surface(DetailClass::Object), None);
        assert_eq!(
            d.surface(DetailClass::Building).unwrap().texture,
            DataId(0x0500_1787)
        );
        assert_eq!(
            d.surface(DetailClass::Environment).unwrap().texture,
            DataId(0x0500_1787)
        );
        // The tiling is written for all four whatever the gates said -- the four writes run
        // before the four `if`s.
        for cls in DetailClass::TILING_ORDER {
            assert!((d.tiling(cls) - 4.0).abs() < f32::EPSILON, "{cls:?}");
        }
        assert_eq!(d.applies(), 1);
    }

    #[test]
    fn turning_the_preference_off_releases_every_surface_and_keeps_the_tiling() {
        let r = region();
        let mut d = DetailTexturing::new();
        d.set(Some(&r), false, true, true, false);
        d.set(Some(&r), false, false, false, false);
        assert_eq!(d.generated(), 0);
        assert!((d.tiling(DetailClass::Building) - 4.0).abs() < f32::EPSILON);
        assert_eq!(d.applies(), 2, "the poll reached the subsystem both times");
        assert_eq!(d.current(DetailClass::Building), None);
    }

    #[test]
    fn with_no_region_nothing_happens_at_all() {
        let mut d = DetailTexturing::new();
        assert!(!d.set(None, false, true, true, false));
        assert_eq!(
            d.applies(),
            0,
            "the null-region early-out returns before the counter would move"
        );
    }

    #[test]
    fn the_two_live_classes_carry_destcolor_invsrcalpha() {
        assert_eq!(DetailClass::Building.blend(), (9, 6));
        assert_eq!(DetailClass::Environment.blend(), (9, 6));
        assert_eq!(DetailClass::Landscape.blend(), (5, 6));
    }

    #[test]
    fn get_alpha_for_z_is_the_ten_to_fifty_metre_ramp() {
        assert_eq!(get_alpha_for_z(0.0), 0xFF);
        assert_eq!(get_alpha_for_z(9.999), 0xFF);
        assert_eq!(get_alpha_for_z(10.0), 0xFF);
        assert_eq!(get_alpha_for_z(30.0), 127);
        assert_eq!(get_alpha_for_z(50.0), 0);
        assert_eq!(get_alpha_for_z(50.001), 0);
        assert_eq!(
            get_alpha_for_z(f32::NAN),
            0,
            "the `jnp` arm of the far compare"
        );
    }

    #[test]
    fn only_a_full_eight_by_eight_block_takes_the_landscape_surface() {
        assert!(block_takes_landscape_detail(8));
        assert!(!block_takes_landscape_detail(4));
        assert!(!block_takes_landscape_detail(1));
    }
}
