// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SurfacePixelFormat.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SurfacePixelFormat.cs`; do not edit by hand

/// This is called PixelFormat in the client, but renaming it due to conflict with built in Enum PixelFormat. These are the different image formats that textures (RenderSurface) are stored in the dat files. While these are all defined, only a handful are actually used.
///
/// ACE enum `SurfacePixelFormat`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SurfacePixelFormat(pub u32);

#[allow(non_upper_case_globals)]
impl SurfacePixelFormat {
    pub const PFID_UNKNOWN: Self = Self(0);
    pub const PFID_R8G8B8: Self = Self(20);
    pub const PFID_A8R8G8B8: Self = Self(21);
    pub const PFID_X8R8G8B8: Self = Self(22);
    pub const PFID_R5G6B5: Self = Self(23);
    pub const PFID_X1R5G5B5: Self = Self(24);
    pub const PFID_A1R5G5B5: Self = Self(25);
    pub const PFID_A4R4G4B4: Self = Self(26);
    pub const PFID_R3G3B2: Self = Self(27);
    pub const PFID_A8: Self = Self(28);
    pub const PFID_A8R3G3B2: Self = Self(29);
    pub const PFID_X4R4G4B4: Self = Self(30);
    pub const PFID_A2B10G10R10: Self = Self(31);
    pub const PFID_A8B8G8R8: Self = Self(32);
    pub const PFID_X8B8G8R8: Self = Self(33);
    pub const PFID_A2R10G10B10: Self = Self(35);
    pub const PFID_A8P8: Self = Self(40);
    pub const PFID_P8: Self = Self(41);
    pub const PFID_L8: Self = Self(50);
    pub const PFID_A8L8: Self = Self(51);
    pub const PFID_A4L4: Self = Self(52);
    pub const PFID_V8U8: Self = Self(60);
    pub const PFID_L6V5U5: Self = Self(61);
    pub const PFID_X8L8V8U8: Self = Self(62);
    pub const PFID_Q8W8V8U8: Self = Self(63);
    pub const PFID_V16U16: Self = Self(64);
    pub const PFID_A2W10V10U10: Self = Self(67);
    pub const PFID_D16_LOCKABLE: Self = Self(70);
    pub const PFID_D32: Self = Self(71);
    pub const PFID_D15S1: Self = Self(73);
    pub const PFID_D24S8: Self = Self(75);
    pub const PFID_D24X8: Self = Self(77);
    pub const PFID_D24X4S4: Self = Self(79);
    pub const PFID_D16: Self = Self(80);
    pub const PFID_VERTEXDATA: Self = Self(100);
    pub const PFID_INDEX16: Self = Self(101);
    pub const PFID_INDEX32: Self = Self(102);
    pub const PFID_CUSTOM_R8G8B8A8: Self = Self(240);
    pub const PFID_CUSTOM_FIRST: Self = Self(240);
    pub const PFID_CUSTOM_A8B8G8R8: Self = Self(241);
    pub const PFID_CUSTOM_B8G8R8: Self = Self(242);
    pub const PFID_CUSTOM_LSCAPE_R8G8B8: Self = Self(243);
    pub const PFID_CUSTOM_LSCAPE_ALPHA: Self = Self(244);
    pub const PFID_CUSTOM_LAST: Self = Self(500);
    pub const PFID_CUSTOM_RAW_JPEG: Self = Self(500);
    pub const PFID_DXT1: Self = Self(0x31545844);
    pub const PFID_DXT2: Self = Self(0x32545844);
    pub const PFID_YUY2: Self = Self(0x32595559);
    pub const PFID_DXT3: Self = Self(0x33545844);
    pub const PFID_DXT4: Self = Self(0x34545844);
    pub const PFID_DXT5: Self = Self(0x35545844);
    pub const PFID_G8R8_G8B8: Self = Self(0x42475247);
    pub const PFID_R8G8_B8G8: Self = Self(0x47424752);
    pub const PFID_UYVY: Self = Self(0x59565955);
    pub const PFID_INVALID: Self = Self(0x7FFFFFFF);
}

impl SurfacePixelFormat {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::PFID_UNKNOWN, Self::PFID_R8G8B8, Self::PFID_A8R8G8B8, Self::PFID_X8R8G8B8, Self::PFID_R5G6B5, Self::PFID_X1R5G5B5, Self::PFID_A1R5G5B5, Self::PFID_A4R4G4B4, Self::PFID_R3G3B2, Self::PFID_A8, Self::PFID_A8R3G3B2, Self::PFID_X4R4G4B4, Self::PFID_A2B10G10R10, Self::PFID_A8B8G8R8, Self::PFID_X8B8G8R8, Self::PFID_A2R10G10B10, Self::PFID_A8P8, Self::PFID_P8, Self::PFID_L8, Self::PFID_A8L8, Self::PFID_A4L4, Self::PFID_V8U8, Self::PFID_L6V5U5, Self::PFID_X8L8V8U8, Self::PFID_Q8W8V8U8, Self::PFID_V16U16, Self::PFID_A2W10V10U10, Self::PFID_D16_LOCKABLE, Self::PFID_D32, Self::PFID_D15S1, Self::PFID_D24S8, Self::PFID_D24X8, Self::PFID_D24X4S4, Self::PFID_D16, Self::PFID_VERTEXDATA, Self::PFID_INDEX16, Self::PFID_INDEX32, Self::PFID_CUSTOM_R8G8B8A8, Self::PFID_CUSTOM_FIRST, Self::PFID_CUSTOM_A8B8G8R8, Self::PFID_CUSTOM_B8G8R8, Self::PFID_CUSTOM_LSCAPE_R8G8B8, Self::PFID_CUSTOM_LSCAPE_ALPHA, Self::PFID_CUSTOM_LAST, Self::PFID_CUSTOM_RAW_JPEG, Self::PFID_DXT1, Self::PFID_DXT2, Self::PFID_YUY2, Self::PFID_DXT3, Self::PFID_DXT4, Self::PFID_DXT5, Self::PFID_G8R8_G8B8, Self::PFID_R8G8_B8G8, Self::PFID_UYVY, Self::PFID_INVALID];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["PFID_UNKNOWN", "PFID_R8G8B8", "PFID_A8R8G8B8", "PFID_X8R8G8B8", "PFID_R5G6B5", "PFID_X1R5G5B5", "PFID_A1R5G5B5", "PFID_A4R4G4B4", "PFID_R3G3B2", "PFID_A8", "PFID_A8R3G3B2", "PFID_X4R4G4B4", "PFID_A2B10G10R10", "PFID_A8B8G8R8", "PFID_X8B8G8R8", "PFID_A2R10G10B10", "PFID_A8P8", "PFID_P8", "PFID_L8", "PFID_A8L8", "PFID_A4L4", "PFID_V8U8", "PFID_L6V5U5", "PFID_X8L8V8U8", "PFID_Q8W8V8U8", "PFID_V16U16", "PFID_A2W10V10U10", "PFID_D16_LOCKABLE", "PFID_D32", "PFID_D15S1", "PFID_D24S8", "PFID_D24X8", "PFID_D24X4S4", "PFID_D16", "PFID_VERTEXDATA", "PFID_INDEX16", "PFID_INDEX32", "PFID_CUSTOM_R8G8B8A8", "PFID_CUSTOM_FIRST", "PFID_CUSTOM_A8B8G8R8", "PFID_CUSTOM_B8G8R8", "PFID_CUSTOM_LSCAPE_R8G8B8", "PFID_CUSTOM_LSCAPE_ALPHA", "PFID_CUSTOM_LAST", "PFID_CUSTOM_RAW_JPEG", "PFID_DXT1", "PFID_DXT2", "PFID_YUY2", "PFID_DXT3", "PFID_DXT4", "PFID_DXT5", "PFID_G8R8_G8B8", "PFID_R8G8_B8G8", "PFID_UYVY", "PFID_INVALID"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 12, 15, 26, 20, 7, 9, 13, 19, 16, 10, 2, 39, 40, 38, 43, 42, 41, 37, 44, 29, 33, 27, 30, 32, 31, 28, 45, 46, 48, 49, 50, 51, 35, 36, 54, 22, 18, 17, 24, 8, 4, 1, 52, 0, 53, 25, 21, 34, 5, 11, 14, 23, 3, 47];
}

super::support::ace_enum!(SurfacePixelFormat, u32, plain);
super::support::ace_enum_from!(SurfacePixelFormat, u32 => u64, i64);
