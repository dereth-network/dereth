//! Behaviour: none (experimental)
//!
//! The remaster's box on the options window's client page: the last row of the rendering section
//! where the host can draw the remaster, read from and kept in the preferences file's
//! `[Rendering] Remaster` line by this client's own name for it.

use auberean_interface::preferences::Preferences;
use auberean_shell::preferences::{EnumNames, KEY_NAMES};
use auberean_shell::Settings;
use auberean_ui_game::options::client::{section, Request, PREFERENCE_STRINGS};
use auberean_ui_game::options::helper::{own, Change};
use auberean_ui_game::options::preference::Flag;

const FULL_SCREEN: u32 = 0x4000_0003;

/// The rendering section's rows as listed: each yes-or-no one.
fn flags(p: &Preferences) -> Vec<Flag> {
    let l = p.listing().expect("listed");
    l.sections
        .iter()
        .find(|(s, _)| *s == section::RENDERING)
        .expect("the rendering section")
        .1
        .iter()
        .filter_map(|c| match c {
            Change::Flag(f) => Some(*f),
            _ => None,
        })
        .collect()
}

/// The remaster's box, when listed.
fn remaster_box(p: &Preferences) -> Option<Flag> {
    flags(p)
        .into_iter()
        .find(|f| f.description.label == own::REMASTER.raw())
}

/// The rendering section of a client started with `file`, its box offered or not.
fn started(file: &str, offered: bool) -> (Settings, Preferences) {
    let records = &crate::screens::data().0.records;
    let settings = Settings::read(file);
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.offer_remaster(offered);
    p.set_presentation(&[(1024, 768)], &settings);
    p.handle(&Request::Attach(vec![section::RENDERING]));
    (settings, p)
}

/// Where the host can draw the remaster, the rendering section ends with its box, labeled and
/// explained in this client's own words, off by default and holding the file's value. Turned off,
/// it reaches the host when submitted and again when put back; kept, it goes to the file as the
/// rendering section's `Remaster` line, which the settings read back. The data's enumeration of
/// keys names no key as this client's own one is named, nor by its number.
#[test]
fn the_remaster_box_ends_the_rendering_section_and_keeps_its_line() {
    let (settings, mut p) = started("[Rendering]\r\nRemaster=1\r\n", true);
    let all = flags(&p);
    let last = all.last().copied().expect("rows");
    assert_eq!(remaster_box(&p), Some(last), "the last row");
    assert_eq!(all.len(), 6, "after the device's five");
    assert_eq!(last.description.section, section::RENDERING);
    assert_eq!(last.description.table, PREFERENCE_STRINGS);
    assert_eq!(last.description.help, own::REMASTER_HELP.raw());
    assert_eq!(own::words(last.description.label), Some("Remaster"));
    assert!(
        last.get() && !last.default_value(),
        "the file's, off by default"
    );
    assert!(!last.changed(), "as applied");
    let names = EnumNames::read(&crate::screens::data().0.records, KEY_NAMES);
    assert_eq!(names.value("Remaster"), None);
    assert_eq!(names.name(last.description.key), None);
    // Turned off: the host hears it when submitted, and again when put back.
    let mut off = last;
    off.set_value(false);
    p.handle(&Request::Change(Change::Flag(off)));
    assert_eq!(p.take_presentation(), None, "tried");
    p.handle(&Request::Submit(section::RENDERING));
    let shown = p.take_presentation().expect("told");
    assert!(!shown.remaster);
    p.handle(&Request::Recall(section::RENDERING));
    assert!(p.take_presentation().expect("told again").remaster);
    // Off again, kept: the file's line, as the shell writes it.
    p.handle(&Request::Change(Change::Flag(off)));
    p.handle(&Request::Submit(section::RENDERING));
    let shown = p.take_presentation().expect("told");
    p.handle(&Request::Synchronize(section::RENDERING));
    p.handle(&Request::Save);
    let kept = p.take_kept().expect("kept");
    assert!(
        kept.contains(&("Rendering".into(), "Remaster".into(), "0".into())),
        "{kept:?}"
    );
    let mut s = settings;
    s.set_presentation(&shown);
    for (section, key, value) in &kept {
        s.keep(section, key, value);
    }
    let text = s.write();
    assert_eq!(text.matches("Remaster=").count(), 1, "{text}");
    assert!(!Settings::read(&text).remaster);
    // Read back, the box holds it.
    let (_, p) = started(&text, true);
    assert!(!remaster_box(&p).expect("listed").get());
}

/// Where the host can't draw the remaster there is no box, and the preference keeps its value
/// whatever else the section submits. A box taken off the section once the host finds it can't
/// leaves the preference at the box's value.
#[test]
fn where_the_host_cant_draw_the_remaster_there_is_no_box() {
    let (_, mut p) = started("[Rendering]\r\nRemaster=1\r\n", false);
    assert_eq!(remaster_box(&p), None);
    assert_eq!(flags(&p).len(), 5, "the device's five");
    let full = |p: &Preferences| {
        flags(p)
            .into_iter()
            .find(|f| f.description.key == FULL_SCREEN)
            .expect("listed")
    };
    let mut on = full(&p);
    on.set_value(true);
    p.handle(&Request::Change(Change::Flag(on)));
    p.handle(&Request::Submit(section::RENDERING));
    assert!(
        p.take_presentation().expect("told").remaster,
        "kept as read"
    );
    p.handle(&Request::Synchronize(section::RENDERING));
    // Offered, turned off and submitted, then taken off.
    p.offer_remaster(true);
    let mut off = remaster_box(&p).expect("offered");
    assert!(off.get());
    off.set_value(false);
    p.handle(&Request::Change(Change::Flag(off)));
    p.handle(&Request::Submit(section::RENDERING));
    assert!(!p.take_presentation().expect("told").remaster);
    p.handle(&Request::Synchronize(section::RENDERING));
    p.offer_remaster(false);
    assert_eq!(remaster_box(&p), None, "taken off");
    let mut back = full(&p);
    back.set_value(false);
    p.handle(&Request::Change(Change::Flag(back)));
    p.handle(&Request::Submit(section::RENDERING));
    assert!(!p.take_presentation().expect("told").remaster, "the box's");
}
