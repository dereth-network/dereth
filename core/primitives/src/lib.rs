//! The shared vocabulary every crate compiles against: identifiers, space, time, shapes, text, the
//! arithmetic policy and the interface traits between crates.
//!
//! **Depends on** no other workspace crate (its only dependencies are `thiserror`, `pxfm` and
//! `libm`). **Used by** nearly every crate in the workspace, client and server alike; changing it
//! is a coordinated break that needs a migration note.
//!
//! **Must never** hold domain logic, engine code or I/O: nothing here opens a file, a socket, a
//! thread, a clock or a window, and no other `dereth-*` crate or platform crate may appear in its
//! tree. The one host call is `text`'s NLS arm behind the `host-nls` feature, the only code allowed
//! `unsafe`. `cargo xtask seams` checks all of it (`seam: primitives deps`, `seam: primitives
//! code`). A type belongs here when two crates need the same value and agree on its arithmetic;
//! what the value *means* to a subsystem stays in that subsystem.
//!
//! The interfaces are traits over plain data, so either side of one can be a mock in tests:
//! [`asset`] ([`AssetSource`]), [`render`] ([`RenderBackend`] and [`DrawBatch`]), [`transport`]
//! ([`Transport`]), [`motion`] ([`MotionSource`], between physics and animation) and [`host`]
//! ([`HostEncoding`], [`TextSink`]). [`shape`], [`records`] and [`text`] hold the plain data the
//! decoders, animation, physics, rendering and the protocol share, [`frame`] the frame arithmetic
//! physics, animation and the landscape all move frames with, and [`num`] the arithmetic policy: float-to-int conversion, the pseudo-random streams, the hashes and the portable
//! transcendental functions, each reproducing a behaviour observable in play.

pub mod asset;
pub mod era;
pub mod frame;
pub mod host;
pub mod ids;
pub mod motion;
pub mod num;
pub mod property;
pub mod records;
pub mod render;
pub mod shape;
pub mod space;
pub mod text;
pub mod time;
pub mod transport;
pub mod viewport;

pub use asset::{
    AssetError, AssetSource, ContainerEra, MeshData, MeshHandle, TextureData, TextureFormat,
    TextureHandle,
};
pub use era::{EraFeatures, EraId, VitaeRecovery};
pub use host::{HostEncoding, TextSink};
pub use ids::{CellId, DataId, DataType, LandblockId, ObjectId, PropertyId};
pub use motion::{MotionPhysicsState, MotionSource};
pub use render::{DrawBatch, RenderBackend};
pub use space::{Frame, Position, Quat, Vec3};
pub use time::{LocalTime, ServerTime};
pub use transport::{IncomingMessage, NetBlobId, NetQueue, RecipientId, Transport};
pub use viewport::Viewport;

#[cfg(test)]
mod tests {
    use super::*;

    /// The interface traits must be object-safe: their users hold them behind `dyn` so they can
    /// be mocked.
    #[test]
    fn interface_traits_are_object_safe() {
        fn _assert_assets(_: &dyn AssetSource) {}
        fn _assert_render(_: &mut dyn RenderBackend) {}
        fn _assert_transport(_: &mut dyn Transport) {}
        fn _assert_motion(_: &mut dyn MotionSource) {}
        fn _assert_host_encoding(_: &dyn HostEncoding) {}
        fn _assert_text_sink(_: &mut dyn TextSink) {}
    }
}
