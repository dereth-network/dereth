//! The audio endpoint, as the host opens it.
//!
//! The client's audio system -- the voice pool, the mixer, the sound tables -- names no device.
//! What it needs from the host is two things, in this order: whether there is a default output
//! endpoint at all (the answer decides the CRT seed, before the audio system is built), and a
//! stream on that endpoint that calls the mixer for every block. `AudioOutput` is those two
//! calls; the executable implements it over its audio library, and a run with no sound passes
//! none. The trait is the sound engine's (`dereth_audio::output`), so a host's endpoint serves any
//! client built on that engine.

pub use dereth_audio::output::AudioOutput;
