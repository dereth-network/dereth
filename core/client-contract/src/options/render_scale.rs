//! The render scale: the share of its screen's own pixels the client draws while it fills the
//! screen. Fewer pixels draw faster, and the picture, enlarged to fill the screen, looks softer.
//!
//! It is this client's own option, not a retail one, and only a host that can draw the game at
//! fewer pixels than its screen and enlarge the picture offers it: the browser, whose page is the
//! screen in full screen. Such a host registers the option as the client starts ([`register`]), at
//! the share it starts at, which in a browser is the front page's choice. A host that does not
//! offer it never registers it, and the options pages leave its row out
//! ([`super::sheet::Row::hosted`]). Every interface that shows the row shows it under Display,
//! after Full Screen; the host reads the share from the store every frame ([`percent`]), so a
//! change applies at once. The front page keeps the choice, and the host hands a change made on a
//! page back to it, so the profile does not keep it ([`left_out_of_save`]).

use crate::view::PrefValue;

/// `[Display] RenderScale`: the share of the screen's own pixels drawn while the client fills
/// the screen, in percent.
pub const RENDER_SCALE: &str = crate::options::names::DISPLAY_RENDER_SCALE;

/// The option's caption, literal text: no string table has one.
pub const CAPTION: &str = "Render Scale";

/// The shares a page offers, in percent, the whole screen first.
pub const CHOICES: [u32; 5] = [100, 85, 75, 67, 50];

/// All of the screen's pixels.
pub const ALL: u32 = 100;

/// The least share drawn: half the screen's pixels each way.
pub const LEAST: u32 = 50;

/// `share`, kept between [`LEAST`] and [`ALL`].
const fn kept(share: u32) -> u32 {
    if share < LEAST {
        LEAST
    } else if share > ALL {
        ALL
    } else {
        share
    }
}

/// Register the option in the option value store at `start` percent (kept between [`LEAST`] and
/// [`ALL`]): what a host that offers it does as the client starts. A store that already has it
/// keeps what it holds.
pub fn register(start: u32) -> usize {
    usize::from(super::store::register_preference(
        RENDER_SCALE,
        PrefValue::Int(i32::try_from(kept(start)).unwrap_or(100)),
        super::store::DataType::UInt,
    ))
}

/// Whether the host offers the option: whether it registered it.
#[must_use]
pub fn offered() -> bool {
    super::store::is_registered(RENDER_SCALE)
}

/// The share of the screen's pixels to draw while the client fills the screen, in percent: the
/// store's, kept between [`LEAST`] and [`ALL`], or all of them where the host does not offer the
/// option.
#[must_use]
pub fn percent() -> u32 {
    match super::store::inq_value(RENDER_SCALE) {
        Some(PrefValue::Int(v)) => u32::try_from(v).map_or(LEAST, kept),
        _ => ALL,
    }
}

/// The value a word stands for: a share in percent, with or without its sign, between [`LEAST`]
/// and [`ALL`]. `None` for another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(RENDER_SCALE) {
        return None;
    }
    let share: u32 = raw.trim().trim_end_matches('%').trim_end().parse().ok()?;
    (LEAST..=ALL)
        .contains(&share)
        .then(|| i32::try_from(share).ok())
        .flatten()
}

/// The words for a value: the share in percent. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(RENDER_SCALE) {
        return None;
    }
    match v {
        PrefValue::Int(i) => Some(format!("{i}%")),
        _ => None,
    }
}

/// The shares a page offers ([`CHOICES`]), each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(RENDER_SCALE) {
        return None;
    }
    Some(
        CHOICES
            .iter()
            .map(|&share| super::store::Choice {
                label: format!("{share}%"),
                value: i32::try_from(share).unwrap_or(100),
            })
            .collect(),
    )
}

/// Whether the store's save leaves `name` out of the profile: the render scale always, which
/// the browser's front page keeps.
#[must_use]
pub fn left_out_of_save(name: &str) -> bool {
    name.eq_ignore_ascii_case(RENDER_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::interface::Interface;
    use crate::options::{sheet, store};

    /// The Client Options page of `face`, as the preferences of its rows in order.
    fn client_page(face: Interface) -> Vec<&'static str> {
        sheet::rows_for(sheet::PageId::Client, face)
            .filter_map(sheet::Row::preference)
            .collect()
    }

    /// A host that offers the render scale registers it at the share it starts at, and from then
    /// every interface's Client Options page has the row, Render Scale, under Display after Full
    /// Screen, with the five shares read as their percent alone and no note, all of the screen
    /// restored by Defaults. A host that does not offer it has
    /// no such row on any page, and draws all of the screen.
    ///
    /// Behaviour: window.full-screen.in-a-browser-the-render-scale-is-a-client-option
    #[test]
    fn a_host_that_offers_the_render_scale_has_its_row_on_every_interface_s_page() {
        store::init();
        assert!(!offered());
        assert_eq!(percent(), ALL);
        for face in [Interface::Modern, Interface::Classic, Interface::Horizon] {
            assert!(!client_page(face).contains(&RENDER_SCALE), "{face:?}");
            assert!(sheet::defaults(sheet::PageId::Client, face)
                .iter()
                .all(|(name, _)| *name != RENDER_SCALE));
        }

        assert_eq!(register(75), 1);
        assert!(offered());
        assert_eq!(percent(), 75);
        for face in [Interface::Modern, Interface::Classic, Interface::Horizon] {
            let page = client_page(face);
            let at = page.iter().position(|p| *p == RENDER_SCALE);
            let full_screen = page
                .iter()
                .position(|p| *p == crate::options::names::DISPLAY_FULL_SCREEN);
            assert_eq!(at, full_screen.map(|i| i + 1), "{face:?}: {page:?}");
            assert!(sheet::defaults(sheet::PageId::Client, face).contains(&(
                RENDER_SCALE,
                crate::options::config::PrefValueConst::Int(100)
            )));
        }
        let row = sheet::row_of_preference(sheet::PageId::Client, RENDER_SCALE).unwrap();
        assert_eq!(row.caption, "Render Scale");
        assert_eq!(row.note, None, "the choices say it all");
        assert_eq!(row.value, sheet::Value::Menu(RENDER_SCALE));
        let shares: Vec<(String, i32)> = store::choice_rows(RENDER_SCALE)
            .unwrap()
            .into_iter()
            .map(|c| (c.label, c.value))
            .collect();
        assert_eq!(
            shares,
            [
                ("100%".to_owned(), 100),
                ("85%".to_owned(), 85),
                ("75%".to_owned(), 75),
                ("67%".to_owned(), 67),
                ("50%".to_owned(), 50)
            ]
        );
        store::clear();
    }

    /// The share is the store's from the next read, kept between half and all of the screen, and
    /// a second registration keeps what the store holds.
    ///
    /// Behaviour: window.full-screen.in-a-browser-the-render-scale-is-a-client-option
    #[test]
    fn the_share_is_the_store_s_and_never_below_half_or_above_all() {
        store::init();
        assert_eq!(register(30), 1);
        assert_eq!(percent(), LEAST, "a start below half starts at half");
        assert!(store::set_value(RENDER_SCALE, PrefValue::Int(85)));
        assert_eq!(percent(), 85);
        assert_eq!(register(100), 0);
        assert_eq!(percent(), 85, "a second registration keeps the choice");
        assert!(store::set_value(RENDER_SCALE, PrefValue::Int(250)));
        assert_eq!(percent(), ALL);
        store::clear();
        assert_eq!(percent(), ALL);
    }

    /// The front page keeps the choice, so the profile does not: the save leaves the option out,
    /// and a profile that names it does not change it.
    ///
    /// Behaviour: window.full-screen.in-a-browser-the-render-scale-is-a-client-option
    #[test]
    fn the_profile_neither_keeps_nor_changes_the_render_scale() {
        store::init();
        register(67);
        let text = store::save().to_text();
        assert!(!text.contains("RenderScale"), "{text}");
        let ini =
            crate::persist::preferences::UserPreferences::parse("[Display]\r\nRenderScale=50\r\n")
                .unwrap();
        store::init();
        store::load(&ini);
        register(67);
        assert_eq!(percent(), 67);
        store::clear();
    }

    /// The words the option reads and writes are its share in percent.
    ///
    /// Behaviour: none (the option's own spelling).
    #[test]
    fn a_share_reads_and_writes_as_its_percent() {
        assert_eq!(parse_value(RENDER_SCALE, "75"), Some(75));
        assert_eq!(parse_value(RENDER_SCALE, " 67 % "), Some(67));
        assert_eq!(parse_value(RENDER_SCALE, "40"), None);
        assert_eq!(parse_value(RENDER_SCALE, "big"), None);
        assert_eq!(parse_value("Display.FullScreen", "75"), None);
        assert_eq!(
            convert_to_string(RENDER_SCALE, &PrefValue::Int(85)).as_deref(),
            Some("85%")
        );
        assert_eq!(convert_to_string("Other.Name", &PrefValue::Int(85)), None);
    }
}
