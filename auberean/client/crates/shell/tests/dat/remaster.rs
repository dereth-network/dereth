//! Behaviour: none (experimental)
//!
//! The remaster between the client's preferences and the host: the host says whether it can draw
//! it, and the remaster asked for on the command line goes to the host whatever the preference
//! says.

use auberean_options::page::{section, Change, Request};
use auberean_shell::host::TestHost;
use auberean_shell::interface::NoInterface;
use auberean_shell::Shell;

use crate::session::data;

/// Whether the client page's rendering section lists the remaster's box.
fn lists_the_box<H: auberean_shell::Host>(s: &Shell<'_, H>) -> bool {
    let label = auberean_assets::strings::token("ID_Rendering_Remaster");
    s.preferences.listing().is_some_and(|l| {
        l.sections.iter().any(|(at, prefs)| {
            *at == section::RENDERING
                && prefs
                    .iter()
                    .any(|c| matches!(c, Change::Flag(f) if f.description.label == label))
        })
    })
}

/// The remaster asked for on the command line reaches the host this run though the preference is
/// off, and the settings keep the preference's value, not the command line's. What the host can
/// draw decides whether the client page offers the box.
#[test]
fn the_remaster_asked_for_this_run_reaches_the_host_and_the_box_is_kept() {
    let (world, data) = data();
    let mut s = Shell::new(
        TestHost::default(),
        world,
        data.clone(),
        Box::new(NoInterface),
    );
    s.settings.remaster_this_run = true;
    s.set_display_modes(&[(800, 600), (1024, 768)]);
    s.preferences
        .handle(&Request::Attach(vec![section::RENDERING]));
    s.offer_remaster(true);
    assert!(lists_the_box(&s), "offered");
    s.offer_remaster(false);
    assert!(!lists_the_box(&s), "taken off");
    s.toggle_full_screen();
    assert_eq!(s.host.presentations.len(), 1);
    assert!(s.host.presentations[0].remaster, "drawn this run");
    assert!(!s.settings.remaster, "the preference's value");
    assert!(s.settings.write().contains("Remaster=0"));
    // Without the command line the preference's value is what the host is told.
    s.settings.remaster_this_run = false;
    s.toggle_full_screen();
    assert!(!s.host.presentations[1].remaster);
}
