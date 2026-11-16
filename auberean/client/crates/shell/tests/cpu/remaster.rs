//! Behaviour: none (experimental)
//!
//! The remaster in the settings: the preferences file's `[Rendering] Remaster` line, kept as the
//! options window's box sets it, and the remaster asked for on the command line for one run.

use auberean_shell::Settings;

/// The remaster is off on a first start, read from the rendering section's `Remaster` line, shown
/// in how the game is shown, and kept back in that line as the options window last set it.
#[test]
fn the_remaster_is_read_from_and_kept_in_the_rendering_remaster_line() {
    let first = Settings::default();
    assert!(
        !first.remaster && !first.presentation().remaster,
        "off at first"
    );
    let s = Settings::read("[Rendering]\r\nRemaster=1\r\n");
    assert!(s.remaster);
    assert!(s.presentation().remaster, "drawn");
    let mut s = s;
    let mut off = s.presentation();
    off.remaster = false;
    s.set_presentation(&off);
    let text = s.write();
    assert!(text.contains("Remaster=0"), "{text}");
    assert!(!Settings::read(&text).remaster, "kept off");
}

/// The remaster asked for on the command line is drawn this run whatever the box says, and never
/// kept: the file keeps the box's value, turned on or off.
#[test]
fn the_remaster_asked_for_this_run_is_drawn_and_never_kept() {
    let mut s = Settings::read("[Rendering]\r\nRemaster=0\r\n");
    s.remaster_this_run = true;
    assert!(s.presentation().remaster, "drawn this run");
    assert!(s.write().contains("Remaster=0"), "not kept");
    let mut asked = s.presentation();
    asked.remaster = false;
    s.set_presentation(&asked);
    assert!(s.presentation().remaster, "the box off leaves it drawn");
    assert!(s.write().contains("Remaster=0"));
    asked.remaster = true;
    s.set_presentation(&asked);
    assert!(s.write().contains("Remaster=1"), "the box on is kept");
    asked.remaster = false;
    s.set_presentation(&asked);
    assert!(s.write().contains("Remaster=0"), "and off again");
    assert!(s.presentation().remaster);
}
