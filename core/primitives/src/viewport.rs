//! The 3D viewport rectangle.
//!
//! Plain data, and here rather than in `dereth-render` because two of `dereth-client`'s logic modules
//! (`interaction` and `pick`) need the rectangle and nothing else the renderer owns.
//! `dereth_render::camera::Viewport` re-exports it, so every existing import keeps working.

/// A viewport rectangle in back-buffer pixels. `IDirect3DDevice9::SetViewport` also carries
/// `MinZ = 0.0`, `MaxZ = 1.0`, which the client never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
