//! The audio endpoint, as the host opens it.
//!
//! The client's audio system -- the voice pool, the mixer, the sound tables -- names no device.
//! What it needs from the host is two things, in this order: whether there is a default output
//! endpoint at all (the answer decides the CRT seed, before the audio system is built), and a
//! stream on that endpoint that calls the mixer for every block. `AudioOutput` is those two
//! calls; the executable implements it over its audio library, and a run with no sound passes
//! none.

/// The host's audio output.
pub trait AudioOutput {
    /// Find the default output endpoint: its sample rate and channel count, or why there is
    /// none. The reason is printed beside "running silent", so it is the host library's own text.
    ///
    /// # Errors
    /// No endpoint, or one whose default configuration will not read.
    fn probe(&mut self) -> Result<(u32, u16), String>;

    /// Start a stream on the endpoint [`Self::probe`] found. `fill` runs on the device's own
    /// thread for every block the endpoint asks for, with interleaved `f32` samples. The returned
    /// handle keeps the stream playing; dropping it stops the stream.
    ///
    /// # Errors
    /// The stream would not build or would not start.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn start(
        &mut self,
        fill: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> Result<Box<dyn std::any::Any>, String>;
}
