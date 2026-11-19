//! Horizon in the browser. The module carries none of Horizon's art: the page fetches its files from
//! beside the first game's module (`pkg/horizon/`, the same files, since the art names no game) and
//! hands them over ([`hand_over_art`]). From then on the browser offers Horizon beside the game's own
//! interface; while the page is still fetching the files, or when it could not, a choice of Horizon
//! is refused with [`NO_ART`] (and the page's reason, when it gave one).

use std::cell::RefCell;
use std::sync::Arc;

pub use auberean_horizon::art::FILES;
use auberean_horizon::art::{Pieces, Records};
use auberean_horizon::{Horizon, Start, NO_ART};
use auberean_shell::interface::{Choice, Interface, InterfaceMaker};

thread_local! {
    /// Horizon's art as the page handed it over, or why it could not; `None` until the page has
    /// said.
    static ART: RefCell<Option<Result<Arc<Pieces>, String>>> = const { RefCell::new(None) };
}

/// The page's word on Horizon's art: the files it fetched, or why it could not fetch them. Files
/// that fail [`Pieces::check`] are not taken. From then on Horizon is offered, or refused with the
/// reason.
///
/// # Errors
/// Why the art is not taken: the page's reason, or what is wrong with the files.
pub fn hand_over_art(fetched: Result<Pieces, String>) -> Result<(), String> {
    let art = fetched.and_then(|pieces| pieces.check().map(|()| Arc::new(pieces)));
    let taken = art.as_ref().map(|_| ()).map_err(Clone::clone);
    ART.with(|slot| *slot.borrow_mut() = Some(art));
    taken
}

/// Horizon's art as the page handed it over.
///
/// # Errors
/// [`NO_ART`] while the page has not handed it over, and with the page's reason when it could not.
pub fn art() -> Result<Arc<Pieces>, String> {
    ART.with(|art| match &*art.borrow() {
        Some(Ok(pieces)) => Ok(Arc::clone(pieces)),
        Some(Err(why)) => Err(format!("{NO_ART}: {why}")),
        None => Err(NO_ART.to_owned()),
    })
}

/// The interfaces the browser offers: the game's own `retail` one, and Horizon over the art that
/// `art` gives, with the game's `records` behind it and started as `start` says. Horizon is built
/// the first time it is chosen once `art` has the files; until then a choice of it is refused with
/// why, and may be made again.
pub fn interfaces<'w>(
    retail: impl Interface + 'w,
    records: Option<Records>,
    start: Start,
    art: impl Fn() -> Result<Arc<Pieces>, String> + 'w,
) -> Box<dyn InterfaceMaker<'w> + 'w> {
    auberean_horizon::maker(retail, move || {
        let horizon = Horizon::new(art()?, records.clone()).with_start(start.clone());
        Ok(Box::new(horizon) as Box<dyn Interface + 'w>)
    })
}

/// Whether the client will start in Horizon: the preferences file kept, `settings`, names it as
/// the `[UI] Interface` chosen. The page waits for the art first then.
#[must_use]
pub fn wanted(settings: Option<&str>) -> bool {
    settings.is_some_and(|text| {
        auberean_shell::Settings::read(text)
            .profile()
            .value("UI", "Interface")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(Choice::Horizon.name()))
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own interface in the browser: the art the page hands over)
    use super::*;
    use auberean_shell::interface::NoInterface;

    fn built_in() -> Pieces {
        Pieces::built_in().expect("a host build carries the art")
    }

    /// Horizon is refused, saying why, until the art is there; then it is built when chosen.
    #[test]
    fn horizon_is_refused_until_the_art_is_there_and_built_once_it_is() {
        let handed: std::rc::Rc<RefCell<Option<Arc<Pieces>>>> = std::rc::Rc::default();
        let source = std::rc::Rc::clone(&handed);
        let mut maker = interfaces(NoInterface, None, Start::default(), move || {
            source.borrow().clone().ok_or_else(|| NO_ART.to_owned())
        });
        let refused = maker.make(Choice::Horizon).err();
        assert_eq!(refused.as_deref(), Some(NO_ART), "loading: refused");
        *handed.borrow_mut() = Some(Arc::new(built_in()));
        assert!(maker.make(Choice::Horizon).is_ok(), "the art is there");
        assert!(
            maker.make(Choice::Retail).is_ok(),
            "the game's own beside it"
        );
    }

    /// The page's art is there only once the page has handed over files that pass the check; a
    /// failure says the page's reason.
    #[test]
    fn the_art_is_there_only_once_the_page_has_handed_over_files_that_pass_the_check() {
        assert_eq!(
            art().err().as_deref(),
            Some(NO_ART),
            "nothing handed over yet"
        );
        let why = "pkg/horizon/fonts.json: HTTP 404";
        assert_eq!(hand_over_art(Err(why.to_owned())), Err(why.to_owned()));
        let refused = art().unwrap_err();
        assert!(
            refused.starts_with(NO_ART) && refused.ends_with(why),
            "{refused}"
        );
        let mut wrong = built_in();
        wrong.insert("fonts.json", &b"<html>not found</html>"[..]);
        let refused = hand_over_art(Ok(wrong)).unwrap_err();
        assert!(
            refused.contains("fonts.json is not a JSON table"),
            "{refused}"
        );
        assert!(art().is_err(), "not taken");
        assert_eq!(hand_over_art(Ok(built_in())), Ok(()));
        let pieces = art().expect("taken");
        assert!(pieces.get("manifest.json").is_some());
        let mut maker = interfaces(NoInterface, None, Start::default(), art);
        assert!(
            maker.make(Choice::Horizon).is_ok(),
            "built from the page's art"
        );
    }

    #[test]
    fn the_client_starts_in_horizon_only_when_the_preferences_file_names_it() {
        assert!(!wanted(None), "a new player starts in the game's own");
        assert!(!wanted(Some("[UI]\r\nInterface=Retail\r\n")));
        assert!(!wanted(Some("[Display]\r\nWidth=800\r\n")));
        assert!(wanted(Some("[UI]\r\nInterface=Horizon\r\n")));
    }
}
