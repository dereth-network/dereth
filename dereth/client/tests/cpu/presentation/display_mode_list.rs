//! The resolution drop-down's list is the adapter's display modes, filtered to at least 800x600
//! at 32 bits per pixel, deduplicated by size, sorted by area, with 1024x768 appended as the
//! fallback and the default; an adapter that reports nothing still yields that one row.
//!
//! Behaviour: none (the display-mode store's contract, on a made-up adapter; the drop-down built
//! from it is anchored in the gpu tier's `presentation::resolution_dropdown`)
//!
//! Fixture: a made-up adapter mode list fed to the display-preference store; no device, no dats.

use dereth_ui_screens::options::store::{self, DisplayMode};

/// The preference under test, spelled once.
const RESOLUTION: &str = "Display.Resolution";

/// A made-up adapter. Every test that needs a list uses this one, so the expected rows are
/// arithmetic rather than whatever this machine happens to report.
///
/// It carries, deliberately: one mode below each of the two extent floors, one 16-bit mode at a
/// legal size, one mode **on** both floors (800x600, which `jb` keeps), two refresh rates for the
/// same 1920x1080, and 1024x768 **absent** so the synthetic fallback's append is visible.
fn made_up_adapter() -> Vec<DisplayMode> {
    let m = |width, height, refresh_rate, bits_per_pixel| DisplayMode {
        width,
        height,
        refresh_rate,
        bits_per_pixel,
    };
    vec![
        m(1920, 1080, 60, 32),
        m(640, 480, 60, 32), // below the 800-pixel width floor
        m(1280, 720, 60, 32),
        m(1920, 1080, 144, 32), // a duplicate (w, h) with a second refresh rate
        m(800, 600, 60, 32),    // exactly on both inclusive size floors
        m(1280, 1024, 75, 16),  // below the 32-bit color-depth floor
        m(2560, 1440, 60, 32),
        m(1024, 480, 60, 32), // below the 600-pixel height floor
    ]
}

// =============================================================================================
// 1. The display-mode initialization transcription oracle
// =============================================================================================

/// The filter, the dedupe, the append and the sort, on a mode list this file made up.
///
/// It is the oracle the drop-down's tests in the gpu tier rest on.
#[test]
fn the_list_is_the_adapter_modes_filtered_deduped_and_sorted_by_area() {
    store::clear_display_choices();
    let default = store::initialize_display_preferences(&made_up_adapter());

    let rows = store::display_choices(RESOLUTION);
    let labels: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        ["800x600", "1024x768", "1280x720", "1920x1080", "2560x1440"],
        "800x600 sits exactly on both inclusive floors; 640x480 and 1024x480 are rejected by \
         unsigned-below comparisons; 1280x1024 is 16-bit; 1920x1080 appears once; 1024x768 is \
         the appended fallback although the adapter never reported it -- all ordered by width * height"
    );
    for c in &rows {
        let (w, h) = store::mode_size(c.value);
        assert_eq!(
            c.label,
            format!("{w}x{h}"),
            "the row label and packed mode descriptor must agree"
        );
    }
    assert_eq!(
        rows.iter()
            .map(|c| store::mode_size(c.value).0 * store::mode_size(c.value).1)
            .collect::<Vec<_>>(),
        [480_000, 786_432, 921_600, 2_073_600, 3_686_400],
        "display-mode rows are sorted by width * height"
    );
    assert_eq!(
        default, 0x0400_0300,
        "the initial display resolution is the appended 1024x768 fallback"
    );

    // Refresh-rate collection and row collection deliberately have different deduplication rules:
    //
    // * an accepted duplicate width and height still contributes its refresh rate, so the second
    //   1920x1080 mode contributes 144 Hz even though it contributes no second mode row;
    // * a mode rejected by a size or color-depth floor contributes neither a row nor a refresh
    //   rate, so the 16-bit 1280x1024 mode's 75 Hz is absent.
    //
    // The appended fallback contributes refresh rate zero, displayed as `"Auto"`.
    let rates: Vec<String> = store::display_choices("Display.RefreshRate")
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert_eq!(rates, ["Auto", "60hz", "144hz"]);
    store::clear_display_choices();
}

/// The floor is a *filter*, not a clamp, and an adapter that reports nothing still yields the one
/// synthetic fallback row. This is the negative control for the test above.
#[test]
fn an_adapter_with_nothing_to_report_still_yields_the_appended_fallback() {
    store::clear_display_choices();
    store::initialize_display_preferences(&[]);
    let rows = store::display_choices(RESOLUTION);
    assert_eq!(
        rows.len(),
        1,
        "the synthetic final iteration yields one fallback row for an empty adapter list"
    );
    assert_eq!(rows[0].label, "1024x768");
    assert_eq!(rows[0].value, 0x0400_0300);
    store::clear_display_choices();
}
