//! Divergence: V373
//! The overlay clock is fixed and ignores process environment variables.
//! Fixture: a child process and the deterministic overlay clock.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_content::{import::default_now, overlay};
use std::process::Command;
const EPOCH_2001: &str = "978307200";

#[test]
fn the_overlay_default_clock_is_2000_01_01() {
    assert_eq!(
        overlay::default_now(),
        DotNetDateTime::new_hms(2000, 1, 1, 0, 0, 0)
    );
    assert_eq!(overlay::default_now(), default_now());
}

/// [`the_overlay_default_clock_is_2000_01_01`] again, in a child process of this test binary that
/// has `SOURCE_DATE_EPOCH` set.
#[test]
fn the_overlay_default_clock_ignores_source_date_epoch() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "content::content_clock::the_overlay_default_clock_is_2000_01_01",
            "--test-threads",
            "1",
        ])
        .env("SOURCE_DATE_EPOCH", EPOCH_2001)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("1 passed"), "the child ran the test: {text}");
}
