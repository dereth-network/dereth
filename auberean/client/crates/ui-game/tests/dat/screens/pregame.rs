//! The screens before the world, each driven as the game's flow drives it: made, ticked with the
//! interface's clock, told what the client tells it; its dialogs carried out by the factory, what
//! else it asks kept for the test.

use auberean_ui::token::{attr, state};
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::character_select::CharacterSelect;
use auberean_ui_game::creation::{self, AvatarModification, AvatarSelection, Choice};
use auberean_ui_game::credits::Credits;
use auberean_ui_game::datapatch::{self, DataPatch};
use auberean_ui_game::dialog::{self, kind, Dialogs};
use auberean_ui_game::disconnected::Disconnected;
use auberean_ui_game::flow::mode;
use auberean_ui_game::intro::Intro;
use auberean_ui_game::pregame::{
    Avatar, Character, CharacterSet, ConnectError, Connection, Data, Notice, Request, Screen,
};

use crate::common::records;

/// A screen before the world as the flow drives it, with what lives across screens and the
/// dialogs.
pub struct Run {
    pub ui: Ui,
    pub data: Data,
    pub dialogs: Dialogs,
    pub screen: Option<Box<dyn Screen>>,
    /// What the screen asked besides dialogs, oldest first.
    pub asked: Vec<Request>,
    /// How many please-wait dialogs went up.
    pub waits: u32,
    pub now: f64,
}

impl Run {
    pub fn new() -> Self {
        let mut ui = Ui::new(records(), 1024, 768);
        ui.use_time(1.0);
        Self {
            ui,
            data: Data::default(),
            dialogs: Dialogs::default(),
            screen: None,
            asked: Vec::new(),
            waits: 0,
            now: 1.0,
        }
    }

    pub fn with_characters(mut self, names: &[(&str, u32)], slots: i32) -> Self {
        self.data.characters = Some(CharacterSet {
            characters: names
                .iter()
                .enumerate()
                .map(|(i, (n, g))| Character {
                    id: 0x5000_0001 + u64::try_from(i).unwrap(),
                    name: (*n).into(),
                    greyed: *g,
                })
                .collect(),
            slots,
            account: "tester".into(),
        });
        self.data.lists += 1;
        self
    }

    pub fn show(
        &mut self,
        make: impl FnOnce(&mut Ui, &mut Data, &mut Vec<Request>) -> Box<dyn Screen>,
    ) {
        let mut out = Vec::new();
        let s = make(&mut self.ui, &mut self.data, &mut out);
        self.screen = Some(s);
        self.carry(out);
    }

    /// What the screen asked: dialogs up and down, the rest kept.
    fn carry(&mut self, out: Vec<Request>) {
        let mut said = Vec::new();
        for r in out {
            match r {
                Request::Dialog { dialog, parent } => {
                    if dialog.kind == kind::WAIT {
                        self.waits += 1;
                    }
                    self.dialogs.make_dialog(
                        &mut self.ui,
                        Some(parent),
                        &(&dialog).into(),
                        &mut said,
                    );
                }
                Request::DialogDown(id) => self.dialogs.close_dialog(&mut self.ui, id, &mut said),
                // The screens' music is not this test's.
                Request::Sound(_) | Request::Silence => {}
                r => self.asked.push(r),
            }
        }
    }

    /// A frame: the clock moves on `dt` seconds, the dialogs' answers told the screen, its tick.
    pub fn step_by(&mut self, dt: f64) {
        self.now += dt;
        self.ui.use_time(self.now);
        let mut said = Vec::new();
        self.dialogs.listen(&mut self.ui, &mut said);
        let mut out = Vec::new();
        let s = self.screen.as_mut().expect("a screen");
        for n in &said {
            if let Some((id, yes)) = dialog::confirmation_closed(n) {
                s.notice(
                    &mut self.ui,
                    &mut self.data,
                    &Notice::ConfirmationClosed { id, yes },
                    &mut out,
                );
            }
        }
        s.tick(&mut self.ui, &mut self.data, &mut out);
        self.carry(out);
        self.ui.take_messages();
    }

    pub fn step(&mut self) {
        self.step_by(1.0 / 60.0);
    }

    pub fn tell(&mut self, n: &Notice) {
        let mut out = Vec::new();
        let s = self.screen.as_mut().expect("a screen");
        s.notice(&mut self.ui, &mut self.data, n, &mut out);
        self.carry(out);
    }

    pub fn update(&mut self) {
        let mut out = Vec::new();
        let s = self.screen.as_mut().expect("a screen");
        s.update(&mut self.ui, &mut self.data, &mut out);
        self.carry(out);
    }

    pub fn find(&self, name: &str) -> Handle {
        self.ui
            .find(Token::of(name))
            .unwrap_or_else(|| panic!("{name} on the screen"))
    }

    /// Point at an element's middle, press, let go: a frame after each.
    pub fn click(&mut self, h: Handle) {
        let r = self.ui.screen_rect(h).expect("placed");
        self.click_at((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    }

    pub fn click_at(&mut self, x: i32, y: i32) {
        self.ui.pointer_move(x, y);
        self.step();
        self.ui.pointer_press();
        self.step();
        self.ui.pointer_release();
        self.step();
    }

    pub fn press(&mut self, name: &str) {
        let h = self.find(name);
        self.click(h);
    }

    pub fn ghosted(&self, name: &str) -> bool {
        let h = self.find(name);
        self.ui.node(h).unwrap().bool_attr(attr::GHOSTED)
    }

    pub fn text(&self, name: &str) -> String {
        self.ui.text(self.find(name)).unwrap_or_default()
    }

    pub fn take(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.asked)
    }

    pub fn screen<T: Screen>(&self) -> &T {
        self.screen
            .as_ref()
            .and_then(|s| s.as_any().downcast_ref::<T>())
            .expect("the screen")
    }
}

pub fn queued(asked: &[Request]) -> Vec<Token> {
    asked
        .iter()
        .filter_map(|r| match r {
            Request::Mode(m, _) => Some(*m),
            _ => None,
        })
        .collect()
}

/// The game opens on its pictures: three of them, each four seconds, then on to connecting; a key
/// pressed, or a press let go, goes on at once.
///
/// Behaviour: ui.intro.the-pictures-pass-and-a-key-or-a-press-goes-on
#[test]
fn the_pictures_pass_and_a_key_or_a_press_goes_on() {
    let mut r = Run::new();
    r.show(|ui, _, out| Box::new(Intro::new(ui, out)));
    let field = r.find("IntroField");
    let mut pictures = Vec::new();
    let mut went_on = None;
    for frame in 0..(14 * 60) {
        r.step();
        let p = r.ui.node(field).and_then(|n| n.picture).map(|i| i.id);
        if pictures.last() != Some(&p) {
            pictures.push(p);
        }
        if went_on.is_none() && queued(&r.take()) == vec![mode::DATA_PATCH] {
            went_on = Some(frame);
        }
    }
    assert_eq!(
        pictures,
        vec![Some(0x4100_38AA), Some(0x4100_4521), Some(0x4100_38AB)],
        "three pictures in turn"
    );
    let at = went_on.expect("on to connecting");
    // Three pauses of four seconds, and the state changes between them.
    assert!(
        (715..=800).contains(&at),
        "after twelve seconds or so: {at} frames"
    );

    let mut r = Run::new();
    r.show(|ui, _, out| Box::new(Intro::new(ui, out)));
    r.step();
    r.ui.key(auberean_ui::types::key_id::ESCAPE, true);
    r.step();
    assert_eq!(queued(&r.take()), vec![mode::DATA_PATCH], "a key");
    let mut r = Run::new();
    r.show(|ui, _, out| Box::new(Intro::new(ui, out)));
    r.click_at(500, 400);
    assert_eq!(queued(&r.take()), vec![mode::DATA_PATCH], "a press let go");
}

/// Connecting: made, the screen connects and says so at its foot; the connection's progress is
/// written there; a connection that fails leads to the disconnected screen saying why; the
/// account's characters listed lead to the character list, or into the world as the character
/// the client was started with; the meter follows the data downloaded.
///
/// Behaviour: ui.datapatch.connecting-says-how-it-goes-and-the-list-leads-on
#[test]
fn connecting_says_how_it_goes_and_the_list_leads_on() {
    let mut r = Run::new();
    r.show(|ui, data, out| Box::new(DataPatch::new(ui, data, out)));
    assert_eq!(r.take(), vec![Request::Connect], "it connects");
    assert_eq!(
        r.text("ErrorText"),
        "Connecting to Authentication Server ..."
    );
    r.tell(&Notice::Net(Connection::Connecting {
        so_far: 1,
        total: 3,
    }));
    assert_eq!(
        r.text("ErrorText"),
        "Searching for logon server ... (Attempt 1 of 3)"
    );
    r.tell(&Notice::Net(Connection::Connected));
    assert_eq!(r.text("ErrorText"), "Connection successful!");
    r.tell(&Notice::Net(Connection::Failed {
        host: "127.0.0.1".into(),
        port: 9000,
        error: ConnectError::Named("ID_ConnectionError_Generic".into()),
    }));
    assert_eq!(
        r.take(),
        vec![Request::Mode(
            mode::DISCONNECTED,
            "Connection failed:\n Couldn't set up a User Datagram Protocol (UDP) connection to \
             127.0.0.1 on port 9,000."
                .into()
        )]
    );

    // The data download: the meter and its line.
    let ds = r
        .screen
        .as_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<DataPatch>()
        .unwrap();
    ds.on_ddd_event(&mut r.ui, datapatch::event::PATCHING, 10 * 1024);
    ds.on_ddd_event(&mut r.ui, datapatch::event::PROGRESS, 5 * 1024);
    assert_eq!(ds.progress(), (10 * 1024, 5 * 1024));
    assert_eq!(r.text("PatchText"), "50% of 10K complete...");
    assert_eq!(r.text("ErrorText"), "Patching data...");

    // The list comes: on to it.
    r.data = Run::new().with_characters(&[("Firstlight", 0)], 4).data;
    r.update();
    assert_eq!(queued(&r.take()), vec![mode::CHARACTER_SELECTION]);

    // Started with a character: into the world with it, its name in any case.
    let mut r = Run::new();
    r.show(|ui, data, out| Box::new(DataPatch::new(ui, data, out)));
    r.take();
    r.data = Run::new()
        .with_characters(&[("Firstlight", 0), ("Embertail", 0)], 4)
        .data;
    r.data.start_character = "embertail".into();
    r.update();
    assert_eq!(
        r.take(),
        vec![
            Request::Play {
                character: 0x5000_0002
            },
            Request::Mode(mode::GAMEPLAY, String::new())
        ]
    );
}

/// The character named to play goes before the one named to make and play, which stands in when
/// none is named to play; the names are asked after once, so the next list stays a list; a name
/// the list lacks, or a deleted character's, leads to the list.
///
/// Behaviour: ui.datapatch.the-characters-named-on-the-line-are-asked-after-once
#[test]
fn the_characters_named_on_the_line_are_asked_after_once() {
    let listed = |user: &str, create: &str| {
        let mut r = Run::new();
        r.show(|ui, data, out| Box::new(DataPatch::new(ui, data, out)));
        r.take();
        r.data = Run::new()
            .with_characters(&[("Firstlight", 0), ("Embertail", 0), ("Gone", 5)], 4)
            .data;
        r.data.start_character = user.into();
        r.data.create_character = create.into();
        r.update();
        (r.take(), r)
    };
    let play = |character| {
        vec![
            Request::Play { character },
            Request::Mode(mode::GAMEPLAY, String::new()),
        ]
    };
    let list = vec![Request::Mode(mode::CHARACTER_SELECTION, String::new())];
    assert_eq!(listed("Embertail", "Firstlight").0, play(0x5000_0002));
    assert_eq!(listed("", "Firstlight").0, play(0x5000_0001));
    assert_eq!(
        listed("nobody", "Firstlight").0,
        list,
        "the first name only"
    );
    assert_eq!(listed("gone", "").0, list);
    let (_, r) = listed("Embertail", "Firstlight");
    assert!(r.data.start_character.is_empty());
    assert!(r.data.create_character.is_empty());
}

/// A login refused, or a login server letting its connection go, with an error leads from
/// connecting to the disconnected screen with that error's own line; an address the client cannot
/// understand names the host; an error with no line of its own leaves the screen as it is.
///
/// Behaviour: ui.datapatch.a-failed-connection-says-its-errors-own-line
#[test]
fn a_failed_connection_says_its_errors_own_line() {
    let failed = |error: ConnectError| {
        let mut r = Run::new();
        r.show(|ui, data, out| Box::new(DataPatch::new(ui, data, out)));
        r.take();
        r.tell(&Notice::Net(Connection::Failed {
            host: "example".into(),
            port: 9000,
            error,
        }));
        r.take()
    };
    let shown = |error: ConnectError| match failed(error.clone()).as_slice() {
        [Request::Mode(m, text)] if *m == mode::DISCONNECTED => text.clone(),
        other => panic!("{error:?}: {other:?}"),
    };
    let named = |id: &str| ConnectError::Named(id.into());
    assert_eq!(
        shown(named("ID_ConnectionError_ServerFull")),
        "Connection failed:\n Server Full"
    );
    assert_eq!(
        shown(ConnectError::BadAddress),
        "Connection failed:\n Couldn't look up server hostname 'example'."
    );
    assert_eq!(failed(named("")), Vec::new(), "no line of its own");
}

/// The disconnected screen shows why the session ended, and its button leaves the game.
///
/// Behaviour: ui.disconnected.the-reason-shows-and-quit-leaves
#[test]
fn the_reason_shows_and_quit_leaves() {
    let mut r = Run::new();
    r.show(|ui, _, _| Box::new(Disconnected::new(ui)));
    r.screen
        .as_mut()
        .unwrap()
        .set_error_msg(&mut r.ui, "The connection to the server has been lost!");
    assert_eq!(
        r.screen::<Disconnected>().error_text(&r.ui).as_deref(),
        Some("The connection to the server has been lost!")
    );
    r.press("QuitButton");
    assert_eq!(r.take(), vec![Request::Quit]);
}

/// The character list keeps its buttons ghosted until the characters arrive; then a living
/// character can be entered or deleted, a deleted one only restored, and creation is offered
/// while slots remain. Enter puts please wait up, ghosts the buttons, asks for the world and the
/// play screen.
///
/// Behaviour: ui.charselect.the-buttons-follow-the-selected-character
#[test]
fn the_buttons_follow_the_selected_character() {
    let mut r = Run::new();
    r.show(|ui, data, out| Box::new(CharacterSelect::new(ui, data, out)));
    for b in [
        "CreateButton",
        "EnterButton",
        "DeleteButton",
        "OptionsButton",
    ] {
        assert!(r.ghosted(b), "{b} waits for the list");
    }
    r.data = Run::new()
        .with_characters(&[("Firstlight", 0), ("Embertail", 3600)], 4)
        .data;
    r.update();
    r.step();
    assert!(!r.ghosted("CreateButton"));
    assert!(!r.ghosted("EnterButton"));
    assert!(!r.ghosted("DeleteButton"));
    let list = r.find("CharacterListBox");
    let rows = r.ui.list_items(list);
    assert_eq!(r.ui.text(rows[0]).as_deref(), Some("Firstlight"));
    assert_eq!(r.ui.node(rows[0]).unwrap().state, state::HIGHLIGHT);

    // Press the deleted character's row.
    r.click(rows[1]);
    assert!(r.ghosted("EnterButton"), "a deleted character cannot enter");
    let field = r.find("DeleteRestoreField");
    assert_eq!(r.ui.node(field).unwrap().state, Token::of("restore"));
    assert!(!r.ghosted("RestoreButton"));

    // Enter with the first: please wait, the world asked, the play screen next.
    r.click(rows[0]);
    r.take();
    let enter = r.find("EnterButton");
    let er = r.ui.screen_rect(enter).unwrap();
    r.click_at(er.x0 + 60, er.y0 + 60);
    assert_eq!(
        r.take(),
        vec![
            Request::Play {
                character: 0x5000_0001
            },
            Request::Mode(mode::GAMEPLAY, String::new())
        ]
    );
    assert_eq!(r.waits, 1, "please wait");
    assert!(r.ghosted("CreateButton") && r.ghosted("OptionsButton"));
}

/// An account with no character: once the (empty) list arrives, creation, the controls and the
/// credits are offered, but Enter and Delete stay ghosted, since no row is selected to light them,
/// and Delete is shown, not Restore.
///
/// Behaviour: ui.charselect.an-empty-list-leaves-enter-and-delete-ghosted
#[test]
fn an_empty_list_leaves_enter_and_delete_ghosted() {
    let mut r = Run::new().with_characters(&[], 4);
    r.show(|ui, data, out| Box::new(CharacterSelect::new(ui, data, out)));
    r.step();
    for b in ["CreateButton", "OptionsButton", "CreditsButton"] {
        assert!(!r.ghosted(b), "{b} is offered");
    }
    assert!(r.ghosted("EnterButton"), "nothing to enter with");
    assert!(r.ghosted("DeleteButton"), "nothing to delete");
    let field = r.find("DeleteRestoreField");
    assert_eq!(r.ui.node(field).unwrap().state, Token::of("delete"));
    let enter = r.find("EnterButton");
    let er = r.ui.screen_rect(enter).unwrap();
    r.click_at(er.x0 + 60, er.y0 + 60);
    assert!(r.take().is_empty(), "a ghosted Enter asks nothing");
}

/// Create, Options and Credits each put please wait up, ghost the buttons and ask for their
/// screen: creation's hall, the key map, the credits.
///
/// Behaviour: ui.charselect.create-options-and-credits-wait-and-go-on
#[test]
fn create_options_and_credits_wait_and_go_on() {
    for (button, to) in [
        ("CreateButton", mode::AVATAR_SELECTION),
        ("OptionsButton", mode::KEYBOARD),
        ("CreditsButton", mode::CREDITS),
    ] {
        let mut r = Run::new().with_characters(&[("Firstlight", 0)], 4);
        r.show(|ui, data, out| Box::new(CharacterSelect::new(ui, data, out)));
        r.step();
        r.press(button);
        assert_eq!(queued(&r.take()), vec![to], "{button}");
        assert_eq!(r.waits, 1, "{button}: please wait");
        assert!(r.ghosted("CreateButton") && r.ghosted("CreditsButton"));
    }
}

/// The carousel: an arm per slot, the first at the front and the others round it; the list comes
/// back on its row at once; selecting another row turns the carousel an arm's step at a time, a
/// second and a half each, the shorter way round; a press on a character's figure selects its row.
///
/// Behaviour: ui.charselect.the-carousel-turns-to-the-selected-row
#[test]
fn the_carousel_turns_to_the_selected_row() {
    let mut r = Run::new().with_characters(&[("A", 0), ("B", 0), ("C", 0)], 6);
    r.data.selected = 2;
    r.show(|ui, data, out| Box::new(CharacterSelect::new(ui, data, out)));
    r.step();
    let cs = r.screen::<CharacterSelect>();
    assert_eq!(
        cs.carousel(),
        (2, 2),
        "the list comes back on its row at once"
    );
    let arms = cs.arm_headings(r.now);
    assert_eq!(arms.len(), 6, "an arm per slot");
    assert!(
        arms[2].rem_euclid(360.0).abs() < 1e-3,
        "its arm at the front: {arms:?}"
    );

    // The first row: two steps back, each turning over a second and a half.
    let list = r.find("CharacterListBox");
    let rows = r.ui.list_items(list);
    r.click(rows[0]);
    assert_eq!(r.screen::<CharacterSelect>().carousel(), (1, 0));
    r.step_by(0.75);
    let half = r.screen::<CharacterSelect>().arm_headings(r.now);
    let h1 = half[1].rem_euclid(360.0);
    assert!(h1 > 1.0 && h1 < 359.0, "turning: {half:?}");
    r.step_by(0.8);
    r.step();
    assert_eq!(r.screen::<CharacterSelect>().carousel(), (0, 0));
    r.step_by(1.6);
    let done = r.screen::<CharacterSelect>().arm_headings(r.now);
    assert!(
        done[0].rem_euclid(360.0).abs() < 1e-3,
        "first at the front: {done:?}"
    );

    // A figure pressed: the scene asked; its answer selects the row.
    let view = r.find("Viewport");
    let vr = r.ui.screen_rect(view).unwrap();
    r.click_at((vr.x0 + vr.x1) / 2, (vr.y0 + vr.y1) / 2);
    assert!(r.take().contains(&Request::FindUnderPointer));
    r.data.selection = Some(Some(1));
    r.step();
    assert_eq!(r.ui.list_selected_index(list), Some(1));
}

/// The character list's Delete asks first, naming the character, with the buttons ghosted while
/// it waits; yes deletes that character, no lights the buttons that never wait and shows the list
/// again.
///
/// Behaviour: ui.charselect.delete-asks-first
#[test]
fn delete_asks_first() {
    let mut r = Run::new().with_characters(&[("Firstlight", 0), ("Embertail", 0)], 4);
    r.show(|ui, data, out| Box::new(CharacterSelect::new(ui, data, out)));
    r.step();
    let list = r.find("CharacterListBox");
    let rows = r.ui.list_items(list);
    r.click(rows[1]);
    r.press("DeleteButton");
    assert!(r.take().is_empty(), "asks first");
    let w = r.dialogs.window().expect("the question up");
    let q = r.ui.child_recursive(w, Token::of("QuestionText")).unwrap();
    assert!(
        r.ui.text(q).unwrap_or_default().contains("Embertail"),
        "names the character: {:?}",
        r.ui.text(q)
    );
    assert!(r.ghosted("EnterButton") && r.ghosted("OptionsButton"));
    let no = r.ui.child_recursive(w, Token::of("NoButton")).unwrap();
    r.click(no);
    r.step();
    assert!(r.take().is_empty());
    assert!(!r.ghosted("OptionsButton"), "no: the list again");
    assert!(r.dialogs.up().is_none());

    let rows = r.ui.list_items(list);
    r.click(rows[1]);
    r.press("DeleteButton");
    let w = r.dialogs.window().expect("asked again");
    let yes = r.ui.child_recursive(w, Token::of("YesButton")).unwrap();
    r.click(yes);
    r.step();
    assert_eq!(
        r.take(),
        vec![Request::Delete {
            character: 0x5000_0002
        }]
    );
}

/// The hall: the pointer over an avatar asks the scene which it is; then the description at the
/// top names its race and sex, and the pointer's tooltip says a press chooses it. A press chooses
/// it and, once its leaving behavior is done, please wait goes up and its appearance comes next.
/// Cancel goes back to the list.
///
/// Behaviour: ui.creation.the-hall-describes-the-avatar-under-the-pointer-and-a-press-chooses-it
#[test]
fn the_hall_describes_the_avatar_under_the_pointer_and_a_press_chooses_it() {
    let mut r = Run::new();
    r.data.hall = vec![
        Avatar {
            race: 2,
            sex: 0x1000,
        },
        Avatar {
            race: 1,
            sex: 0x1000,
        },
    ];
    r.show(|ui, data, out| Box::new(AvatarSelection::new(ui, data, out)));
    assert!(r.data.in_creation, "creation under way");
    r.ui.pointer_move(829, 470);
    r.ui.pointer_move(830, 470);
    r.step();
    assert!(
        r.take().contains(&Request::FindUnderPointer),
        "the scene asked"
    );
    r.data.selection = Some(Some(1));
    r.step();
    assert_eq!(r.screen::<AvatarSelection>().under(), Some(1));
    let desc = r.find("RaceDescField");
    assert_eq!(r.ui.node(desc).unwrap().state, Token::of("humanmale"));
    let view = r.find("AvatarSelectionViewport");
    assert_eq!(
        r.ui.element_tooltip(view),
        Some("Click to choose this avatar.")
    );

    r.ui.pointer_press();
    r.step();
    r.ui.pointer_release();
    r.step();
    let asked = r.take();
    let man = Avatar {
        race: 1,
        sex: 0x1000,
    };
    assert!(asked.contains(&Request::Choose(man)), "{asked:?}");
    assert!(asked.contains(&Request::Behave {
        figure: 1,
        behavior: creation::LEAVE_BEHAVIOR
    }));
    assert!(queued(&asked).is_empty(), "the behavior first");
    r.step();
    assert!(queued(&r.take()).is_empty(), "while the figure plays it");
    r.data.behaving.clear();
    r.step();
    assert_eq!(queued(&r.take()), vec![mode::AVATAR_MODIFICATION]);
    assert_eq!(r.waits, 1, "please wait");

    let mut r = Run::new();
    r.show(|ui, data, out| Box::new(AvatarSelection::new(ui, data, out)));
    r.ui.pointer_move(829, 470);
    r.ui.pointer_move(830, 470);
    r.step();
    r.data.selection = Some(Some(0));
    r.step();
    // Away from the figures: the description goes.
    r.ui.pointer_move(500, 201);
    r.ui.pointer_move(500, 200);
    r.step();
    r.data.selection = Some(None);
    r.step();
    let desc = r.find("RaceDescField");
    assert_eq!(r.ui.node(desc).unwrap().state, state::STATELESS);
    r.press("CancelButton");
    assert_eq!(queued(&r.take()), vec![mode::CHARACTER_SELECTION]);
}

/// The data's rows for a human man: build, height and skin tone on the body page, a head and a
/// clothing row, at the stops given.
fn human_rows() -> Vec<Choice> {
    let row = |physique, label: &str, stops, at| Choice {
        physique,
        label: label.into(),
        stops,
        at,
    };
    vec![
        row(0x1, "Build", 16, 7),
        row(0x2, "Height", 16, 7),
        row(0x4, "Skin Tone", 5, 3),
        row(0x10, "Head", 7, 2),
        row(0x100, "Shirt", 9, 0),
    ]
}

/// The appearance: the chosen avatar's rows (a kind its race lacks gone), each labelled, its
/// slider and buttons' tooltips naming it, at the avatar's stop; a page's button shows its rows
/// and frames the head on the head's page; a slider moved sets the avatar's value; Randomize asks
/// for this page's or every page's; the pointer dragged over the figure turns the view; Back goes
/// to the hall; Next puts the name's panel up, whose Cancel takes it down.
///
/// Behaviour: ui.creation.the-appearance-shows-the-avatars-rows-and-shapes-it
#[test]
fn the_appearance_shows_the_avatars_rows_and_shapes_it() {
    let mut r = Run::new();
    r.data.avatar = Some(Avatar {
        race: 1,
        sex: 0x1000,
    });
    r.data.choices = human_rows();
    r.show(|ui, data, out| Box::new(AvatarModification::new(ui, data, out)));
    assert!(r.take().contains(&Request::Zoom(false)), "the body framed");
    assert!(
        r.ui.find(Token::of("humanViewport")).is_some(),
        "the race's scene"
    );
    assert_eq!(r.text("HeightLabel"), "Height");
    let height = r.find("HeightSlider");
    let at = r.ui.node(height).unwrap().float_attr(attr::X_POSITION);
    assert!((at.unwrap() - 7.0 / 15.0).abs() < 1e-4, "{at:?}");
    assert_eq!(
        r.ui.element_tooltip(height),
        Some("Move slider to change Height.")
    );
    assert_eq!(
        r.ui.element_tooltip(r.find("HeightRightButton")),
        Some("Click for next Height option.")
    );
    assert!(
        r.ui.find(Token::of("SkinDetailField")).is_none(),
        "a kind the race lacks goes"
    );
    assert!(r.ui.shown(r.find("BodyField")) && !r.ui.shown(r.find("HeadField")));

    r.press("HeadButton");
    assert!(r.take().contains(&Request::Zoom(true)), "the head framed");
    assert!(r.ui.shown(r.find("HeadField")) && r.ghosted("HeadButton"));
    r.press("BodyButton");
    r.take();

    // One stop taller by the slider's right button.
    r.press("HeightRightButton");
    let asked = r.take();
    assert!(
        asked.contains(&Request::Choice {
            physique: 0x2,
            stop: 8
        }),
        "the next stop: {asked:?}"
    );
    r.press("RandomizeCurrentButton");
    assert!(r.take().contains(&Request::Randomize { mask: 0x8F }));
    r.press("RandomizeAllButton");
    assert!(r.take().contains(&Request::Randomize {
        mask: creation::ALL
    }));

    // Dragged over the figure to the left: the view turns.
    let over = r.find("MouseRotationOverField");
    let o = r.ui.screen_rect(over).unwrap();
    let (x, y) = ((o.x0 + o.x1) / 2, (o.y0 + o.y1) / 2);
    r.ui.pointer_move(x, y);
    r.step();
    r.ui.pointer_press();
    r.step();
    r.ui.pointer_move(x - 60, y);
    r.step();
    r.ui.pointer_release();
    r.step();
    assert!(
        r.take().iter().any(|q| matches!(q, Request::Turn { .. })),
        "the view turned"
    );

    r.press("NextButton");
    assert!(r.screen::<AvatarModification>().confirmation().is_some());
    r.press("CancelButton");
    assert!(
        r.screen::<AvatarModification>().confirmation().is_none(),
        "Cancel takes the name's panel down"
    );
    r.press("BackButton");
    assert_eq!(queued(&r.take()), vec![mode::AVATAR_SELECTION]);
}

/// An administrator's client shows the administrator's button beside Cancel and Enter; lit, the
/// character is asked for as an administrator's; let off again, as a player's.
///
/// Behaviour: ui.creation.an-admin-client-offers-the-admin-character
#[test]
fn an_admin_client_offers_the_admin_character() {
    let asked = |presses: usize| {
        let mut r = Run::new().with_characters(&[], 4);
        r.data.admin_client = true;
        r.data.avatar = Some(Avatar {
            race: 1,
            sex: 0x1000,
        });
        r.data.choices = human_rows();
        r.show(|ui, data, out| Box::new(AvatarModification::new(ui, data, out)));
        r.take();
        r.press("NextButton");
        assert!(r.ui.find(Token::of("EnterButton")).is_some());
        assert!(r.ui.find(Token::of("CancelButton")).is_some());
        for _ in 0..presses {
            r.press("AdminButton");
        }
        r.press("NameEntry");
        r.ui.type_text("Name");
        r.step();
        r.take();
        r.press("EnterButton");
        r.take()
            .into_iter()
            .find_map(|q| match q {
                Request::Create { admin, .. } => Some(admin),
                _ => None,
            })
            .expect("the character asked for")
    };
    assert!(!asked(0), "a player's unless lit");
    assert!(asked(1), "lit");
    assert!(!asked(2), "let off");
}

/// The name is asked with Cancel and Enter, the pair of buttons the player's client shows; the
/// entry shows its hint and takes no keys until it is pressed, its hint goes then, it takes only
/// a name's letters, and at most sixteen of them. Enter asks for the character; the server's
/// refusal shows under the name and lights the buttons again; made, it enters the world with it.
///
/// Behaviour: ui.creation.the-name-is-asked-with-cancel-and-enter
#[test]
fn the_name_is_asked_with_cancel_and_enter() {
    let mut r = Run::new().with_characters(&[], 4);
    r.data.avatar = Some(Avatar {
        race: 1,
        sex: 0x1000,
    });
    r.data.choices = human_rows();
    r.show(|ui, data, out| Box::new(AvatarModification::new(ui, data, out)));
    r.take();
    r.press("NextButton");
    let c = r
        .screen::<AvatarModification>()
        .confirmation()
        .expect("the name's panel");
    assert!(r.ui.find(Token::of("EnterButton")).is_some());
    assert!(r.ui.find(Token::of("CancelButton")).is_some());
    assert!(r.ui.find(Token::of("AdminButton")).is_none());
    assert_eq!(c.name(&r.ui), "Enter name here.");
    assert!(!r.ui.type_text("x"), "no key before the entry is pressed");
    r.press("NameEntry");
    let name = |r: &Run| {
        r.screen::<AvatarModification>()
            .confirmation()
            .unwrap()
            .name(&r.ui)
    };
    assert_eq!(name(&r), "", "the hint goes when it takes the keyboard");
    r.ui.type_text("Row4an_light");
    assert_eq!(name(&r), "Rowanlight", "a name's letters only");
    r.ui.type_text("abcdefghij");
    assert_eq!(name(&r), "Rowanlightabcdef");
    r.ui.backspace();
    assert_eq!(name(&r), "Rowanlightabcde");

    let enter = r.find("EnterButton");
    let er = r.ui.screen_rect(enter).unwrap();
    r.click_at((er.x0 + er.x1) / 2, (er.y0 + er.y1) / 2);
    assert_eq!(
        r.take(),
        vec![Request::Create {
            name: "Rowanlightabcde".into(),
            admin: false,
        }]
    );
    assert!(r.ghosted("EnterButton"), "waits for the server");
    r.tell(&Notice::CharGenVerification {
        response: 3,
        character: None,
        error: 0,
    });
    assert!(
        !r.text("GenerationConfirmationErrorMsg").is_empty(),
        "why, under the name"
    );
    assert!(!r.ghosted("EnterButton"));
    r.click_at((er.x0 + er.x1) / 2, (er.y0 + er.y1) / 2);
    r.take();
    r.tell(&Notice::CharGenVerification {
        response: 1,
        character: Some(Character {
            id: 0x5000_0009,
            name: "Rowanlightabcde".into(),
            greyed: 0,
        }),
        error: 0,
    });
    assert_eq!(
        r.take(),
        vec![
            Request::Play {
                character: 0x5000_0009
            },
            Request::Mode(mode::GAMEPLAY, String::new())
        ]
    );
}

/// The credits stop the song playing as they open, once, and show the second of their pictures
/// first.
///
/// Behaviour: ui.credits.they-stop-the-song-and-open-on-their-second-picture
#[test]
fn they_stop_the_song_and_open_on_their_second_picture() {
    let mut r = Run::new();
    let mut credits = Credits::open(&mut r.ui, false).expect("the credits open");
    let silences = |out: &[Request]| out.iter().filter(|a| matches!(a, Request::Silence)).count();
    let mut out = Vec::new();
    credits.tick(&mut r.ui, &mut r.data, &mut out);
    assert_eq!(silences(&out), 1, "{out:?}");
    r.ui.use_time(1.0);
    let mut out = Vec::new();
    credits.tick(&mut r.ui, &mut r.data, &mut out);
    assert_eq!(silences(&out), 0, "only as they open");
    let field = r.ui.element(Token::of("PictureField")).expect("made");
    let n = r.ui.node(field).unwrap();
    assert_eq!(
        n.picture.map(|p| p.id),
        n.id_attr(Token::of("ID_Picture1")),
        "the one after the first"
    );
}

/// The credits scroll their text up from under the screen's foot until the last line has gone
/// over its top, which takes about four minutes (the retail client's credits run 236 to 242
/// seconds), and then close by themselves: please wait, and the character list; an interface key
/// closes them at once.
///
/// Behaviour: ui.credits.the-text-scrolls-up-and-the-credits-end-by-themselves
#[test]
fn the_text_scrolls_up_and_the_credits_end_by_themselves() {
    let mut r = Run::new();
    r.show(|ui, _, _| Box::new(Credits::open(ui, false).expect("the credits open")));
    let d = f64::from(r.screen::<Credits>().duration());
    assert!((230.0..250.0).contains(&d), "about four minutes: {d}");
    r.step_by(0.0);
    assert_eq!(
        r.screen::<Credits>().text_top(&r.ui),
        Some(768),
        "the text starts under the foot"
    );
    r.step_by(d / 2.0);
    let mid = r.screen::<Credits>().text_top(&r.ui).unwrap();
    assert!(mid < 768 && mid > -100_000, "half way up: {mid}");
    assert!(r.take().is_empty());
    r.step_by(d / 2.0);
    assert!(
        r.screen::<Credits>().text_top(&r.ui).unwrap() < 0,
        "all of it above the top"
    );
    assert_eq!(queued(&r.take()), vec![mode::CHARACTER_SELECTION]);
    assert_eq!(r.waits, 1, "please wait");

    let mut r = Run::new();
    r.show(|ui, _, _| Box::new(Credits::open(ui, false).unwrap()));
    r.ui.key(auberean_ui::types::key_id::ESCAPE, true);
    r.step();
    assert_eq!(
        queued(&r.take()),
        vec![mode::CHARACTER_SELECTION],
        "a key ends them"
    );
}

/// A small key map: two named actions and an unnamed one, the plain and Ctrl maps, the base map's
/// Ctrl, and a preset that swaps the two actions' keys.
fn keys() -> auberean_ui_game::keyboard::KeyData {
    use auberean_ui_game::keyboard::{meta, preset, KeyData, KeyMaps};
    let k = |code: u32| 0x8100_0400 + code;
    let (examine, inventory, hidden) = (0x4100_0001, 0x4100_0002, 0x4100_0051);
    let maps = KeyMaps {
        base: vec![(meta::LEFT_CTRL, 0x4000_0020)],
        normal: vec![(k(0x12), examine), (k(0x17), inventory)],
        shift: Vec::new(),
        control: vec![(k(0x3F), examine), (k(0x22), hidden)],
    };
    let swapped = KeyMaps {
        normal: vec![(k(0x17), examine), (k(0x12), inventory)],
        control: Vec::new(),
        ..maps.clone()
    };
    KeyData {
        actions: vec![
            (examine, "Examine panel.".into()),
            (inventory, "Inventory panel.".into()),
        ],
        unnamed: vec![hidden],
        maps,
        presets: vec![(preset::WOW, swapped)],
    }
}

/// The key map: each action that has a name, its name and its two keys (a key held with Ctrl
/// written with it); Clear, Apply and Undo ghosted until there is something to clear or apply. A
/// key cell pressed waits for a key: a key the game keeps for an action without a name is
/// refused; one bound to another action asks first, and yes moves it; Apply hands the client the
/// maps; a preset asks and puts its maps in place; Back with changes not applied asks, then
/// please wait and the character list.
///
/// Behaviour: ui.keyboard.the-key-map-shows-each-actions-keys-and-rebinds-them
#[test]
fn the_key_map_shows_each_actions_keys_and_rebinds_them() {
    use auberean_ui_game::keyboard::{meta, Keyboard};
    let mut r = Run::new();
    r.data.keys = Some(keys());
    r.show(|ui, data, out| Box::new(Keyboard::new(ui, data, out)));
    let rows = |r: &Run| r.screen::<Keyboard>().rows(&r.ui);
    assert_eq!(
        rows(&r),
        vec![
            "Examine panel.",
            "E",
            "Ctrl - F5",
            "Inventory panel.",
            "I",
            ""
        ]
    );
    for b in ["clear_button", "save_button", "undo_button"] {
        assert!(r.ghosted(b), "{b} waits");
    }

    // The examine row: its first key cell waits for a key.
    let list = r.find("list_box");
    let items = r.ui.list_items(list);
    r.click(items[0]);
    assert_eq!(r.ui.list_selected_index(list), Some(1));
    assert!(!r.ghosted("clear_button"));
    // Shift on the way to a chord: no key by itself, the cell still waiting.
    r.data.key_hits.push((0x8100_042A, 0));
    r.step();
    assert_eq!(rows(&r)[1], "E", "Shift alone binds nothing");
    assert_eq!(r.ui.list_selected_index(list), Some(1), "still waiting");
    // Ctrl and G: the game keeps it for an action without a name.
    r.data.key_hits.push((0x8100_0422, meta::LEFT_CTRL));
    r.step();
    assert_eq!(rows(&r)[1], "E", "refused");
    // I: bound to the inventory; asked, yes moves it.
    r.data.key_hits.push((0x8100_0417, 0));
    r.step();
    let w = r.dialogs.window().expect("asks first");
    let q = r.ui.child_recursive(w, Token::of("QuestionText")).unwrap();
    assert_eq!(
        r.ui.text(q).as_deref(),
        Some("I is already used by Inventory panel.. Replace?")
    );
    let yes = r.ui.child_recursive(w, Token::of("YesButton")).unwrap();
    r.click(yes);
    r.step();
    assert_eq!(rows(&r)[1], "I");
    assert_eq!(rows(&r)[4], "", "moved from the inventory");
    assert!(!r.ghosted("save_button") && !r.ghosted("undo_button"));

    r.press("save_button");
    let applied = r.take();
    let Some(Request::Keys(maps)) = applied.first() else {
        panic!("the maps applied: {applied:?}");
    };
    assert_eq!(maps.normal, vec![(0x8100_0417, 0x4100_0001)]);
    assert_eq!(maps.control, vec![(0x8100_043F, 0x4100_0001)]);
    assert!(r.ghosted("save_button"));

    // A preset: asked, its maps in place.
    r.press("restore_wow_button");
    let w = r.dialogs.window().expect("asks first");
    let yes = r.ui.child_recursive(w, Token::of("YesButton")).unwrap();
    r.click(yes);
    r.step();
    assert!(
        r.take().iter().any(|q| matches!(q, Request::LiveKeys(_))),
        "bound at once, nothing written"
    );
    assert_eq!(rows(&r)[1..3], ["I".to_owned(), String::new()]);
    assert_eq!(rows(&r)[4], "E");

    // Back with changes: asked; yes, please wait and the list.
    r.press("quit_button");
    assert!(queued(&r.take()).is_empty(), "asks first");
    let w = r.dialogs.window().expect("asks first");
    let yes = r.ui.child_recursive(w, Token::of("YesButton")).unwrap();
    r.click(yes);
    r.step();
    assert_eq!(queued(&r.take()), vec![mode::CHARACTER_SELECTION]);
    assert_eq!(r.waits, 1, "please wait");
}

/// A key is bound with the first of the modifiers held, in the client's order, left and right
/// told apart.
///
/// Behaviour: ui.keyboard.a-key-is-bound-with-the-first-modifier-held
#[test]
fn a_key_is_bound_with_the_first_modifier_held() {
    use auberean_ui_game::keyboard::meta;
    assert_eq!(meta::of_held(0), 0);
    assert_eq!(meta::of_held(1), meta::LEFT_SHIFT);
    assert_eq!(meta::of_held(2), meta::RIGHT_SHIFT);
    assert_eq!(meta::of_held(4), meta::LEFT_CTRL);
    assert_eq!(meta::of_held(8), meta::RIGHT_CTRL);
    assert_eq!(meta::of_held(4 | 1), meta::LEFT_SHIFT, "Shift over Ctrl");
    assert_eq!(meta::of_held(8 | 2), meta::RIGHT_SHIFT);
    assert_eq!(meta::of_held(2 | 1), meta::LEFT_SHIFT, "left over right");
}
