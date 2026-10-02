//! Asset access: how a consumer reads dat content without depending on the decoders.
//!
//! [`AssetSource`] is read access to the dat files by id. The dat reader implements it; anything
//! that only needs bytes by id (a decoder cache, a fixture loader, a tool) takes a `&dyn
//! AssetSource` and can be driven by an in-memory mock in tests.
//!
//! The rest of the module is the plain data an asset hands to a renderer once it has been decoded:
//! [`TextureData`] and [`MeshData`], and the opaque [`TextureHandle`] and [`MeshHandle`] a
//! renderer gives back for them. Palette resolution and format conversion happen before this
//! point, so the data here is already in a form a device accepts.

use crate::ids::{DataId, DataType};

/// Why an asset could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("no such object: {0}")]
    NotFound(DataId),
    #[error("object {id} is malformed: {reason}")]
    Malformed { id: DataId, reason: String },
    #[error("i/o error reading {id}: {source}")]
    Io {
        id: DataId,
        #[source]
        source: std::io::Error,
    },
}

/// Read access to the dat content.
///
/// Implementations must not panic on malformed input: the dat files are real data and a decoder will
/// meet records it does not expect. Return [`AssetError::Malformed`] instead.
pub trait AssetSource {
    /// The raw bytes of one object, with the container's framing already removed.
    fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError>;

    fn exists(&self, id: DataId) -> bool;

    /// Every id of a given type, in ascending order.
    fn iter_type(&self, kind: DataType) -> Box<dyn Iterator<Item = DataId> + '_>;

    /// Which dat set the records come from, and so which record layouts they are in. Anything
    /// that is not the dat files from before Throne of Destiny is the later layout.
    fn container_era(&self) -> ContainerEra {
        ContainerEra::Tod
    }
}

/// Which of the two dat sets a file or record belongs to. The container layout, and the layout of
/// a few record types, changed once, with the file renaming at Throne of Destiny (June 2005):
/// `portal.dat` and `cell.dat` before it, the four `client_*.dat` files from it on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerEra {
    /// `portal.dat` and `cell.dat`: a 44-byte header at `0x12C` holding the whole file's iteration,
    /// no transaction journal, no data set or version stamp, and 12-byte directory entries (id,
    /// first block, size) with no per-entry date, version or iteration.
    PreTod,
    /// The `client_*.dat` files: the 80-byte header at `0x140`, the journal at `0x100`, 24-byte
    /// directory entries and the `0xFFFF0001` iteration list.
    Tod,
}

/// An opaque handle to an uploaded mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub u32);

/// An opaque handle to an uploaded texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

/// Decoded pixels ready for upload. Format conversion happens when the asset is decoded, not in
/// the renderer.
#[derive(Debug, Clone)]
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    /// Provided/system-memory levels, level 0 first. Retail caps compressed system chains at four;
    /// eligible one-level uncompressed image resources separately request runtime GPU autogen
    /// when the texture is created. UI, font and movie resources do not inherit that policy.
    pub levels: Vec<Vec<u8>>,
}

/// The formats the renderer accepts, after decoding has resolved palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextureFormat {
    Bgra8,
    Bc1,
    /// DXT3 explicit alpha, with non-premultiplied RGB for system filtering.
    Bc2,
    /// DXT5 interpolated alpha, with non-premultiplied RGB for system filtering.
    Bc3,
    /// DXT2 source provenance; the GPU layout is still BC2, but filtering must
    /// unpremultiply/re-premultiply through the source codec. This is not a new
    /// device pixel format; generic uploads still preserve only supplied levels.
    Bc2Premultiplied,
    /// DXT4 source provenance. Same BC3 device layout, but system filtering uses
    /// the premultiplied codec. Generic uploads preserve only supplied levels.
    Bc3Premultiplied,
}

/// Geometry ready for upload, in the renderer's vertex layout.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    /// Interleaved vertices; the layout is the renderer's contract and is documented with it.
    pub vertices: Vec<u8>,
    pub indices: Vec<u32>,
    pub stride: u32,
}
