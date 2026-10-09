//! Where a recorded frame's world begins and ends, and the steps inside it.
//!
//! The scene notes a [`Mark`] as it records the frame. The marks change nothing the frame draws:
//! with no high-fidelity sidecar installed they are not even kept. With one installed, they let
//! the sidecar find the world inside the recording, replay it into its own targets and act at the
//! points the marks name.

/// One step of the recorded frame, noted in recording order beside the command it precedes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mark {
    /// The world viewport is set and the world is about to be drawn.
    WorldBegin,
    /// The world has been drawn; what follows is the interface.
    WorldEnd,
    /// A sky pass begins: 0 before the landscape, 1 after it.
    SkyBegin(u8),
    /// A sky pass ends.
    SkyEnd(u8),
    /// One sky object is about to be drawn, with its index in the day group and its property
    /// bits.
    SkyObject {
        /// The object's index in the day group's sky objects.
        index: u32,
        /// The object's property bits.
        properties: u32,
    },
    /// One landscape cell is about to be drawn.
    TerrainCell {
        /// The landblock id, `0xXXYY`.
        block: u16,
        /// The cell within the block, 0..64.
        cell: u8,
    },
    /// The landscape cells have all been drawn.
    TerrainEnd,
    /// The building interiors interleaved with the landscape begin.
    Interiors,
    /// The building interiors end.
    InteriorsEnd,
    /// An object pass begins.
    Objects,
    /// The particle pass begins.
    Particles,
    /// The particle pass ends.
    ParticlesEnd,
    /// The translucent draws held back so far are drawn now.
    AlphaFlush,
    /// The held-back draws are flushed before an indoor depth clear.
    IndoorFlush,
    /// One indoor cell is about to be drawn.
    EnvCell {
        /// The cell's full id.
        cell: u32,
    },
}
