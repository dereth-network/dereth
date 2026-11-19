//! Choosing the interface: the shell builds the interfaces the host's maker offers as they are
//! chosen, switches between them at the start of a frame, refuses one that can't be shown and says
//! why, and keeps the choice in the preferences file as `[UI] Interface`.

use std::cell::RefCell;
use std::rc::Rc;

use auberean_options::page::{section, Change, Request as PageRequest};
use auberean_options::preference::Choice as ChoicePref;
use auberean_shell::host::TestHost;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{
    Choice, Game, Interface, InterfaceMaker, Request, NOT_AVAILABLE_YET,
};
use auberean_shell::Shell;

use crate::session::data;

/// What the interfaces did, in order, each line naming the interface.
type Log = Rc<RefCell<Vec<String>>>;

/// An interface that says what happens to it, sends the requests it is handed at its next update,
/// and keeps what the client told it.
struct Stub {
    name: &'static str,
    log: Log,
    send: Rc<RefCell<Vec<Request>>>,
    told: Rc<RefCell<Vec<String>>>,
    changed: Rc<RefCell<Vec<Change>>>,
}

impl Interface for Stub {
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.log.borrow_mut().push(format!("{} update", self.name));
        assert_eq!(game.interface.name(), self.name, "the one shown");
        self.told.borrow_mut().extend(game.told.iter().cloned());
        self.changed
            .borrow_mut()
            .extend(game.preferences_changed.iter().cloned());
        requests.append(&mut self.send.borrow_mut());
    }
    fn enter(&mut self, _game: &Game<'_>) {
        self.log.borrow_mut().push(format!("{} enter", self.name));
    }
    fn leave(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {
        self.log.borrow_mut().push(format!("{} leave", self.name));
    }
}

/// The stubs' shared ends: the log, the requests to send, what they were told and what changed.
#[derive(Clone, Default)]
struct Ends {
    log: Log,
    send: Rc<RefCell<Vec<Request>>>,
    told: Rc<RefCell<Vec<String>>>,
    changed: Rc<RefCell<Vec<Change>>>,
}

impl Ends {
    /// A maker that builds a stub for each interface, or refuses Horizon when `horizon` is false.
    fn maker<'w>(&self, horizon: bool) -> Box<dyn InterfaceMaker<'w> + 'w> {
        let ends = self.clone();
        Box::new(
            move |choice: Choice| -> Result<Box<dyn Interface + 'w>, String> {
                if choice == Choice::Horizon && !horizon {
                    return Err(NOT_AVAILABLE_YET.to_owned());
                }
                ends.log
                    .borrow_mut()
                    .push(format!("{} made", choice.name()));
                Ok(Box::new(Stub {
                    name: choice.name(),
                    log: Rc::clone(&ends.log),
                    send: Rc::clone(&ends.send),
                    told: Rc::clone(&ends.told),
                    changed: Rc::clone(&ends.changed),
                }))
            },
        )
    }

    /// What was logged since last asked, leaving out the updates.
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
            .into_iter()
            .filter(|l| !l.ends_with(" update"))
            .collect()
    }
}

/// A shell over a host whose preferences file holds `settings`, with the stubs' maker.
fn shell<'w>(ends: &Ends, settings: Option<&str>, horizon: bool) -> Shell<'w, TestHost> {
    let (world, data) = data();
    let host = TestHost {
        settings: settings.map(str::to_owned),
        ..TestHost::default()
    };
    Shell::with_interfaces(host, world, data.clone(), ends.maker(horizon))
}

/// The interface choice as the client page lists it.
fn interface_row(s: &Shell<'_, TestHost>) -> ChoicePref {
    s.preferences
        .listing()
        .and_then(|l| l.sections.iter().find(|(at, _)| *at == section::UI))
        .and_then(|(_, prefs)| {
            prefs.iter().find_map(|c| match c {
                Change::Choice(c) => Some(c.clone()),
                _ => None,
            })
        })
        .expect("the interface section lists its choice")
}

/// What the client page sends to choose `choice` and keep it: the row set, the section applied,
/// the preferences kept.
fn choose_on_the_page(s: &mut Shell<'_, TestHost>, ends: &Ends, choice: Choice) {
    s.preferences
        .handle(&PageRequest::Attach(vec![section::UI]));
    let mut row = interface_row(s);
    assert!(row.set_value(choice.value()), "one of its choices");
    ends.send.borrow_mut().extend([
        Request::Preferences(PageRequest::Change(Change::Choice(row))),
        Request::Preferences(PageRequest::Apply(section::UI)),
        Request::Preferences(PageRequest::Save),
    ]);
}

/// A choice taken switches at the start of the next frame: the interface shown is left, then the
/// one chosen (built the first time it is chosen, and kept) is entered and updated from then on;
/// what the world's controls held is let go and mouse look ends.
///
/// Behaviour: desktop.shell.switching-the-interface-leaves-one-then-enters-the-other
#[test]
fn switching_the_interface_leaves_one_then_enters_the_other() {
    let ends = Ends::default();
    let mut s = shell(&ends, None, true);
    assert_eq!(s.interface(), Choice::Retail, "the default");
    assert_eq!(ends.take(), ["Retail made", "Retail enter"]);
    // A session, whose client carries the controls (no server answers: none is needed).
    ends.send.borrow_mut().push(Request::Connect {
        server: "127.0.0.1:9".parse().expect("an address"),
        account: "tester".into(),
        password: "t".into(),
    });
    s.frame();
    // Forward held and mouse look on.
    s.event(&Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    });
    let heading = s.scene.status.heading;
    s.scene.camera.set_mouse_look(true, heading);
    let in_force = |s: &Shell<'_, TestHost>| {
        s.client
            .as_ref()
            .expect("the session")
            .controls
            .in_force()
            .len()
    };
    assert_eq!(in_force(&s), 1, "forward held");
    ends.send
        .borrow_mut()
        .push(Request::ChooseInterface(Choice::Horizon));
    s.frame();
    assert_eq!(ends.take(), ["Horizon made"], "built when chosen");
    assert_eq!(s.interface(), Choice::Retail, "until the next frame");
    s.frame();
    assert_eq!(
        std::mem::take(&mut *ends.log.borrow_mut()),
        ["Retail leave", "Horizon enter", "Horizon update"],
        "left, entered, then the one shown updated"
    );
    assert_eq!(s.interface(), Choice::Horizon);
    assert_eq!(s.preferences.interface(), Choice::Horizon, "the preference");
    assert_eq!(in_force(&s), 0, "let go");
    assert!(!s.scene.camera.mouse_look, "mouse look ended");
    // Back: the retail interface was kept, not built again.
    ends.send
        .borrow_mut()
        .push(Request::ChooseInterface(Choice::Retail));
    s.frame();
    s.frame();
    assert_eq!(ends.take(), ["Horizon leave", "Retail enter"]);
}

/// An interface that can't be shown is refused when chosen: the one shown stays, the preference
/// is put back to it (the client page told, and the file keeps it), and the player is told why.
///
/// Behaviour: desktop.shell.a-refused-interface-keeps-the-one-shown-and-says-why
#[test]
fn a_refused_interface_keeps_the_one_shown_and_says_why() {
    let ends = Ends::default();
    let mut s = shell(&ends, None, false);
    s.frame();
    choose_on_the_page(&mut s, &ends, Choice::Horizon);
    s.frame();
    s.frame();
    assert_eq!(ends.take(), ["Retail made", "Retail enter"], "no switch");
    assert_eq!(s.interface(), Choice::Retail);
    assert_eq!(s.preferences.interface(), Choice::Retail, "put back");
    assert_eq!(ends.told.borrow().as_slice(), [NOT_AVAILABLE_YET]);
    assert!(
        ends.changed.borrow().iter().any(|c| matches!(
            c,
            Change::Choice(c) if c.get() == Choice::Retail.value() && !c.changed()
        )),
        "the page told: {:?}",
        ends.changed.borrow()
    );
    let kept = s.host.settings.clone().expect("kept");
    assert!(kept.contains("[UI]\r\nInterface=Retail"), "{kept}");
}

/// The interface chosen is kept in the preferences file as `[UI] Interface`, by its name, and the
/// next run starts in it; a run whose file names an interface that can't be shown starts in the
/// retail one.
///
/// Behaviour: desktop.shell.the-interface-chosen-is-kept-in-the-preferences-file
#[test]
fn the_interface_chosen_is_kept_in_the_preferences_file() {
    let ends = Ends::default();
    let mut s = shell(&ends, None, true);
    s.frame();
    choose_on_the_page(&mut s, &ends, Choice::Horizon);
    s.frame();
    s.frame();
    assert_eq!(s.interface(), Choice::Horizon);
    let kept = s.host.settings.clone().expect("kept");
    assert!(kept.contains("[UI]\r\nInterface=Horizon"), "{kept}");
    let row = interface_row(&s);
    assert_eq!(row.literal_choice(0), Some("Retail"));
    assert_eq!(row.literal_choice(1), Some("Horizon"));
    // The next run starts in it.
    let next = Ends::default();
    let s = shell(&next, Some(&kept), true);
    assert_eq!(s.interface(), Choice::Horizon);
    assert_eq!(next.take(), ["Horizon made", "Horizon enter"]);
    // Where it can't be shown, the retail one, and the player is told why.
    let refused = Ends::default();
    let mut s = shell(&refused, Some(&kept), false);
    assert_eq!(s.interface(), Choice::Retail);
    assert_eq!(s.preferences.interface(), Choice::Retail);
    s.frame();
    assert_eq!(refused.told.borrow().as_slice(), [NOT_AVAILABLE_YET]);
}

/// Horizon's own key maps are in use only while it is shown: shown, its keys come before the
/// player's, which stand aside (A turns, Q strafes, V looks at the front, Z the player's maps bind
/// does nothing), with the base map still under them (Shift); the retail interface shown again,
/// the player's maps are back as they were. An orbit camera Horizon asked for ends with the switch.
///
/// Behaviour: desktop.keys.horizons-own-keys-are-in-use-only-while-it-is-shown
#[test]
fn horizons_own_keys_are_in_use_only_while_it_is_shown() {
    use auberean_shell::interface::{CameraRequest, OrbitSettings};
    use auberean_shell::interface_keys::own;
    let ends = Ends::default();
    let mut s = shell(&ends, None, true);
    s.frame();
    let word = |k: Key| k.input_word();
    let retail = |s: &Shell<'_, TestHost>| {
        [Key::A, Key::Q, Key::V, Key::Z, Key::ShiftLeft].map(|k| s.keys.stack.action(word(k), 0))
    };
    let before = retail(&s);
    assert!(
        s.keys.stack.interface().is_empty(),
        "the retail interface has none"
    );
    assert!(
        before[3].is_some(),
        "the player's maps bind Z: {before:08X?}"
    );
    ends.send
        .borrow_mut()
        .push(Request::ChooseInterface(Choice::Horizon));
    s.frame();
    ends.send
        .borrow_mut()
        .push(Request::Camera(CameraRequest::Scheme(Some(
            OrbitSettings::default(),
        ))));
    s.frame();
    assert_eq!(s.interface(), Choice::Horizon);
    assert!(
        s.scene.orbit.is_some(),
        "the orbit camera Horizon asked for"
    );
    let stack = &s.keys.stack;
    assert_eq!(
        stack.interface().len(),
        3,
        "keys alone, with Shift, with Ctrl"
    );
    assert_eq!(
        stack.action(word(Key::A), 0),
        Some(0x4000_0024),
        "A turns left"
    );
    assert_eq!(
        stack.action(word(Key::Q), 0),
        Some(0x4000_0022),
        "Q strafes left"
    );
    assert_eq!(stack.action(word(Key::V), 0), Some(own::LOOK_AT_FRONT));
    assert_eq!(
        stack.action(word(Key::Z), 0),
        None,
        "the player's maps stand aside"
    );
    assert_eq!(
        stack.action(word(Key::ShiftLeft), 0),
        before[4],
        "the base map under"
    );
    ends.send
        .borrow_mut()
        .push(Request::ChooseInterface(Choice::Retail));
    s.frame();
    s.frame();
    assert_eq!(s.interface(), Choice::Retail);
    assert!(s.keys.stack.interface().is_empty());
    assert_eq!(retail(&s), before, "the player's maps back as they were");
    assert!(s.scene.orbit.is_none(), "the game's own camera again");
}
