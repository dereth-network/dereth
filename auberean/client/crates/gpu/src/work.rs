//! What frames ask of the device: how much is made anew and how much is written, and how long each
//! step takes on the thread that draws. A developer's measure (`--dev-perf`) and the claim that a
//! frame like the one before makes nothing new read it; nothing the world shows depends on it.

use web_time::Instant;

/// What the frames since the last [`crate::Renderer::take_work`] made, wrote and drew.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Work {
    /// Buffers made.
    pub buffers: u32,
    /// The bytes those buffers were made holding, or the size of those made empty.
    pub buffer_bytes: u64,
    /// The bytes written into buffers kept between frames.
    pub written_bytes: u64,
    /// Textures made, pictures and drawing targets alike.
    pub textures: u32,
    /// The texels' bytes those textures took when made, drawing targets only by their size.
    pub texture_bytes: u64,
    /// Bind groups made.
    pub bind_groups: u32,
    /// Meshes found on the device.
    pub mesh_hits: u32,
    /// Meshes not on the device, which had to be put there.
    pub mesh_misses: u32,
    /// Draw calls recorded.
    pub draws: u32,
    /// Frames drawn.
    pub frames: u32,
    /// Milliseconds in each step.
    pub ms: Stages,
}

/// The steps of a frame on the thread that draws, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stages {
    /// The frame's uniforms and its drawing targets.
    pub setup: f32,
    /// The sun's shadow map.
    pub shadow: f32,
    /// Every draw's buffers, uniforms, textures and pipeline.
    pub prepare: f32,
    /// The bodies' spots on the shadow map.
    pub spots: f32,
    /// The water's reflection and ripples.
    pub reflection: f32,
    /// The world's own pass.
    pub world: f32,
    /// The uniforms written and the commands closed.
    pub finish: f32,
    /// The commands handed to the device.
    pub submit: f32,
    /// The interface's pictures and the composing of the window's picture.
    pub compose: f32,
    /// Waiting for the window's next picture to draw in.
    pub acquire: f32,
    /// The composed picture's commands handed to the device.
    pub compose_submit: f32,
    /// The window's picture shown.
    pub show: f32,
    /// What finished work leaves behind given back.
    pub reclaim: f32,
}

/// A stopwatch read one lap at a time.
#[derive(Debug)]
pub struct Lap(Instant);

impl Lap {
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }

    /// Milliseconds since the last lap.
    pub fn lap(&mut self) -> f32 {
        let now = Instant::now();
        let ms = now.duration_since(self.0).as_secs_f32() * 1000.0;
        self.0 = now;
        ms
    }
}
