//! Behaviour: none (checks data formats, fixture conformance or host contracts)
//! The UTC offset, URL launch and caret blink hooks answer their portable default until the host
//! installs a reader, then the host answer.
//! Fixture: independently installed process-wide host hooks.

use dereth_client_runtime::platform::{caret, clock, shell};

#[test]
fn the_local_utc_offset_is_utc_until_the_host_installs_its_answer() {
    // Before installation: UTC, whatever the instant.
    assert_eq!(clock::local_utc_offset_secs(0), 0);
    assert_eq!(clock::local_utc_offset_secs(1_700_000_000), 0);

    // The host's answer is asked with the instant it is for.
    fn host(unix_secs: i64) -> i32 {
        if unix_secs >= 1_700_000_000 {
            -7 * 3600
        } else {
            -8 * 3600
        }
    }
    clock::install_local_utc_offset(host);
    assert_eq!(clock::local_utc_offset_secs(0), -8 * 3600);
    assert_eq!(clock::local_utc_offset_secs(1_700_000_000), -7 * 3600);

    // A second installation is ignored.
    fn other(_: i64) -> i32 {
        3600
    }
    clock::install_local_utc_offset(other);
    assert_eq!(
        clock::local_utc_offset_secs(0),
        -8 * 3600,
        "the first installation stands"
    );
}

#[test]
fn the_url_launch_answers_zero_until_the_host_installs_its_launch() {
    // Before installation: `0`, which fails the caller's `> 32` success test.
    assert_eq!(shell::launch_uri("https://example.invalid/ticket"), 0);

    // The host's launch is handed the URL, and its result comes back unchanged.
    fn host(url: &str) -> i32 {
        if url.starts_with("https://") {
            42
        } else {
            2
        }
    }
    shell::install_uri_launcher(host);
    assert_eq!(shell::launch_uri("https://example.invalid/ticket"), 42);
    assert_eq!(shell::launch_uri("not a url"), 2);

    fn other(_: &str) -> i32 {
        33
    }
    shell::install_uri_launcher(other);
    assert_eq!(
        shell::launch_uri("not a url"),
        2,
        "the first installation stands"
    );
}

#[test]
fn the_caret_blink_is_the_portable_530_ms_until_the_host_installs_its_reader() {
    // Before installation: the portable 530 ms, through the same conversion the host's
    // millisecond answer takes.
    let portable = caret::caret_blink_time_seconds();
    assert_eq!(
        portable,
        dereth_client_contract::window_proc::caret_blink_time_seconds_from_millis(530)
    );
    assert!(portable > 0.0);

    fn host() -> f64 {
        0.25
    }
    caret::install_caret_blink(host);
    assert_eq!(caret::caret_blink_time_seconds(), 0.25);

    fn other() -> f64 {
        1.0
    }
    caret::install_caret_blink(other);
    assert_eq!(
        caret::caret_blink_time_seconds(),
        0.25,
        "the first installation stands"
    );
}
