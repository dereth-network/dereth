//! Every landscape-rendering global and its value. Nothing here is recomputed at run time.

/// A landblock side, in world units: the landscape window's.
pub use dereth_landscape::BLOCK_LENGTH;

/// One land cell, in world units -- the landblock's `square_length`.
pub const CELL_SIZE: f32 = 24.0;

/// Half a land cell.
pub const HALF_SQUARE_LENGTH: f32 = 12.0;

/// The "column is outside this plane" sentinel used by every culling test.
pub const OUTSIDE_VAL: f32 = 1001.0;

/// The z-fight adjustment. Terrain vertices are **drawn** 0.01 below their true z, and the
/// same constant is the epsilon passed to the back-face sidedness test in the landscape draw.
pub const Z_FIGHT_TERRAIN_ADJUST: f32 = 0.01;

/// Nine vertices to a side at full detail, indexed `x * 9 + y` — **x major**.
pub const SIDE_VERTEX_COUNT: usize = 9;

/// `9 * 9`. The size of the raw height and terrain arrays in every landblock record.
pub const VERTEX_COUNT: usize = SIDE_VERTEX_COUNT * SIDE_VERTEX_COUNT;

/// Eight land cells per landblock side at full detail.
pub const BLOCK_SIDE: usize = 8;

/// The landscape definitions' maximum object height; the client adds it to the block's
/// tallest terrain vertex so buildings and tall objects stay inside the culling column.
pub const MAX_OBJECT_HEIGHT: f32 = 200.0;

/// The landscape definitions' sky height. Also the cut-off.
pub const SKY_HEIGHT: f32 = 1000.0;

/// The landscape definitions' eighth output: the road **half**-width the on-road test uses.
pub const ROAD_WIDTH: f32 = 5.0;

/// The mirrored road threshold, `CELL_SIZE - ROAD_WIDTH`. `on_road` writes it as the literal 19.
pub const ROAD_WIDTH_FAR: f32 = CELL_SIZE - ROAD_WIDTH;

/// Number of entries in the land-height table.
pub const LAND_HEIGHT_TABLE_LEN: usize = 256;

/// The loader copies entries until the first one outside `[0, 800]`,
/// then **returns 0 and leaves the rest at their previous values**. It does not reject the table.
pub const LAND_HEIGHT_MAX: f32 = 800.0;

/// The universal comparison epsilon, `2e-4`. Every plane test in the client uses it.
pub const EPSILON: f32 = 2.0e-4;

/// The four candidate UVs per land vertex, installed when a landblock is initialised.
pub const LAND_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

/// The uv index of the SW corner for `Rotation` 0/90/180/270.
pub const SW_CORNER: [u8; 4] = [0, 3, 2, 1];
/// The same for the SE corner.
pub const SE_CORNER: [u8; 4] = [1, 0, 3, 2];
/// The same for the NE corner.
pub const NE_CORNER: [u8; 4] = [2, 1, 0, 3];
/// The same for the NW corner.
pub const NW_CORNER: [u8; 4] = [3, 2, 1, 0];

/// The shift per `ImageScaleType` (FULL, HALF, QUARTER, EIGHTH, —).
/// Note the last entry is 8, not 4: the table is `{0,1,2,4,8}` and not `{0,1,2,3,4}`.
pub const IMAGE_SHIFT: [u32; 5] = [0, 1, 2, 4, 8];

/// The floor for the merged land texture size.
pub const MIN_TEX_SIZE: u32 = 8;

/// The terrain texture's U size.
pub const TEXTURE_U_SIZE: f32 = 1.0;
/// The terrain texture's V size.
pub const TEXTURE_V_SIZE: f32 = 1.0;

/// The landscape's initial mid-radius; a width of 11 covers 121 blocks.
pub const DEFAULT_MID_RADIUS: u32 = 5;

/// The landscape's `mid_radius` presets for quality 1..=5: the landscape window's.
pub use dereth_landscape::MID_RADIUS_PRESETS;

/// Terrain type 32 is the road entry in `terrain_desc`. The landscape fetches it
/// by that index.
pub const ROAD_TERRAIN_TYPE: u32 = 0x20;

/// Each alpha list holds at most 3,000 entries.
pub const ALPHA_LIST_CAP: usize = 3000;

/// The alpha-delay mask -- which subset kinds are deferred to the alpha
/// lists rather than drawn in place.
///
/// The shipped value is **`0x0E` = Alpha(2) | Translucent(4) | ClipMap(8)**, read out of the
/// image's data rather than inferred; the client's own registry help string for
/// `RenderD3D.AlphaDelayMask` is *"Mask for what mesh types will get delay-rendered. Alpha=2,
/// Translucent=4, ClipMap=8."* So **every**
/// non-opaque subset is deferred and only mask 0 draws in place.
pub const S_ALPHA_DELAY_MASK: u32 = 0x0E;

/// The near clamp on part degradation.
pub const S_R_DEGRADE_DISTANCE: f32 = 50.0;

// -------------------------------------------------------------------------------------------
// Open-question defaults. Each is a branch selector with a documented shipped default; they are
// named constants so that a future answer is a one-line change rather than a hunt.
// -------------------------------------------------------------------------------------------

/// Gates the per-vertex alpha override in the detail pass. Default 1.
/// UNVERIFIED.
pub const VERTEX_ALPHA_OVERRIDE: bool = true;
/// Gates single-pass detail texturing. Default 1. UNVERIFIED.
pub const SINGLE_PASS_DETAIL: bool = true;
/// Gates the surface-type detail veto. Default 1. UNVERIFIED.
pub const SURFACE_TYPE_VETO: bool = true;
/// Draws a cell's sort contents even when the cell is not in view.
/// Default 1. UNVERIFIED.
pub const ALWAYS_DRAW_SORT_CELL: bool = true;
/// The per-cell alpha flush threshold. Default 0.75, i.e. `< 1`, so the flush
/// happens. UNVERIFIED.
pub const ALPHA_FLUSH_MIN_Z: f32 = 0.75;

/// The pinned adaptive-degrade multiplier. The feedback loop nudges a global `deg_mul` in
/// `[-1, +1]` from measured frame rate; a modern machine drifts to `+1` where a 2013 machine did
/// not. Every test pins it here so no measurement becomes a function of the test machine's speed.
/// the value 2013 hardware settled at was never measured.
pub const PINNED_DEG_MUL: f32 = 0.0;
