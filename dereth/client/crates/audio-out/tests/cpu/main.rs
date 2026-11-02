//! The endpoint's bookkeeping, which needs no sound card.
//!
//! Behaviour: none (the device seam's own bookkeeping: what a host may ask of it before the device
//! answers)

use dereth_audio::output::AudioOutput;
use dereth_audio_out::CpalOutput;

/// A stream is refused until a probe has found an endpoint, so a host that skipped the probe runs
/// silent instead of opening some other device.
#[test]
fn a_stream_is_refused_before_an_endpoint_is_probed() {
    let mut out = CpalOutput::default();
    let started = out.start(Box::new(|b: &mut [f32]| b.fill(0.0)));
    assert_eq!(started.err().as_deref(), Some("no endpoint was probed"));
}
