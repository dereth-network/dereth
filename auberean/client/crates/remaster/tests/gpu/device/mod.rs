//! The devices the remaster draws on: one it shares with wgpu (on Vulkan, and on Direct3D 12 on
//! Windows), and one of its own on Direct3D 12; and the buffers it keeps on them for the lists
//! frames draw.

#[cfg(windows)]
mod d3d12;
mod lists;
mod shared;
