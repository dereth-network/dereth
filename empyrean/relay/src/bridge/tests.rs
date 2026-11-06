//! Behaviour: none (one session's bridge, driven with made-up messages and no socket).

use super::*;

fn upstream() -> SocketAddr {
    "127.0.0.1:9110".parse().unwrap()
}

#[test]
fn debug_output_never_shows_the_password() {
    let bridge = Bridge::new(upstream(), "a world", "probe", "hunter22");
    let text = format!("{bridge:?}");
    assert!(text.contains("probe"), "{text}");
    assert!(!text.contains("hunter22"), "{text}");
}
