//! Horizon's own state, followed from the view, the notices, the events and the lines.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::event::{ChatEvent, GameEvent};
use auberean_client_sdk::contract::notice::{
    vital, Notice, PlayerNotice, Said, SelectionNotice, SelectionVitals,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_client_sdk::contract::ChatHistory;
use auberean_horizon::state::{Frame, GameState, Vital, ERROR_TEXT, LOG_LINES};

use crate::PLAYER;

const DRUDGE: u64 = 0x7000_0000_0000_0042;

fn view_with(vitals: [u32; 4]) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            vitals: Some(vitals),
            name: Some("Rowan".into()),
            ..v::Player::default()
        }),
        ..View::default()
    }
}

fn no_name(_: &TextRef) -> Option<String> {
    None
}

/// Follow one frame of `view` and `notices`, nothing else.
fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &no_name,
        unknown_command: "",
    });
}

fn about_player(said: Said) -> Notice {
    Notice {
        about: Some(PLAYER),
        said,
    }
}

#[test]
fn the_views_vitals_set_the_gauges_and_a_notice_since_is_not_undone_by_the_same_view() {
    let mut s = GameState::default();
    let view = view_with([80, 100, 55, 120]);
    follow(&mut s, &view, &[]);
    assert!(s.in_world);
    assert_eq!(
        s.health,
        Vital {
            current: 80,
            max: 100
        }
    );
    assert_eq!(
        s.vigor,
        Vital {
            current: 55,
            max: 120
        }
    );
    let hurt = about_player(Said::Player(PlayerNotice::Vital {
        vital: vital::HEALTH,
        current: 40,
        max: 100,
    }));
    follow(&mut s, &view, &[hurt]);
    assert_eq!(s.health.current, 40, "the notice");
    follow(&mut s, &view, &[]);
    assert_eq!(s.health.current, 40, "the same view again leaves it");
    follow(&mut s, &view_with([90, 100, 55, 120]), &[]);
    assert_eq!(s.health.current, 90, "a view that changed sets it");
    assert!((s.health.ratio() - 0.9).abs() < 1e-6);
}

#[test]
fn focus_comes_from_its_vital_notice_and_the_hero_notice() {
    let mut s = GameState::default();
    let view = view_with([1, 1, 1, 1]);
    let focus = about_player(Said::Player(PlayerNotice::Vital {
        vital: vital::FOCUS,
        current: 12,
        max: 50,
    }));
    follow(&mut s, &view, &[focus]);
    assert_eq!(
        s.focus,
        Vital {
            current: 12,
            max: 50
        }
    );
    let hero = about_player(Said::Player(PlayerNotice::Hero {
        hero: true,
        focus: 30,
        max_focus: 60,
    }));
    follow(&mut s, &view, &[hero]);
    assert_eq!(
        s.focus,
        Vital {
            current: 30,
            max: 60
        }
    );
    // Another object's vital is not the player's.
    let other = Notice {
        about: Some(DRUDGE),
        said: Said::Player(PlayerNotice::Vital {
            vital: vital::FOCUS,
            current: 1,
            max: 2,
        }),
    };
    follow(&mut s, &view, &[other]);
    assert_eq!(s.focus.current, 30);
}

fn selecting(view: &mut View, id: u64, thing: v::Thing) {
    view.world = Some(v::Surroundings {
        player: PLAYER,
        things: vec![thing],
        selection: v::Selection {
            current: id,
            ..v::Selection::default()
        },
        ..v::Surroundings::default()
    });
}

#[test]
fn the_selection_is_named_from_the_view_and_its_health_from_the_scripts_notice() {
    let mut s = GameState::default();
    let mut view = view_with([1, 1, 1, 1]);
    selecting(
        &mut view,
        DRUDGE,
        v::Thing {
            id: DRUDGE,
            name: Some("Drudge".into()),
            ..v::Thing::default()
        },
    );
    follow(&mut s, &view, &[]);
    let t = s.target.clone().expect("a target");
    assert_eq!((t.id, t.name.as_str(), t.health), (DRUDGE, "Drudge", None));
    let told = about_player(Said::Selection(SelectionNotice::Vitals(Some(
        SelectionVitals {
            health: 30,
            max_health: 60,
            ..SelectionVitals::default()
        },
    ))));
    follow(&mut s, &view, &[told]);
    assert_eq!(
        s.target.as_ref().and_then(|t| t.health),
        Some(Vital {
            current: 30,
            max: 60
        })
    );
    // Another selection starts with no health told.
    selecting(
        &mut view,
        DRUDGE + 1,
        v::Thing {
            id: DRUDGE + 1,
            name: Some("Lump".into()),
            ..v::Thing::default()
        },
    );
    follow(&mut s, &view, &[]);
    let t = s.target.clone().expect("a target");
    assert_eq!((t.name.as_str(), t.health), ("Lump", None));
    // Nothing selected: no target.
    selecting(&mut view, 0, v::Thing::default());
    follow(&mut s, &view, &[]);
    assert_eq!(s.target, None);
}

#[test]
fn a_name_the_view_gives_only_as_a_reference_is_written_by_the_strings() {
    let mut s = GameState::default();
    let mut view = view_with([1, 1, 1, 1]);
    let own = TextRef {
        string: 5,
        table: 0x2500_0001,
        ..TextRef::default()
    };
    selecting(
        &mut view,
        DRUDGE,
        v::Thing {
            id: DRUDGE,
            own_name: own.clone(),
            ..v::Thing::default()
        },
    );
    let write = |t: &TextRef| (t.string == 5).then(|| "Written".to_owned());
    s.follow(&Frame {
        view: Some(&view),
        notices: &[],
        events: &[],
        told: &[],
        lines: &[],
        write: &write,
        unknown_command: "",
    });
    assert_eq!(s.target.map(|t| t.name).as_deref(), Some("Written"));
}

#[test]
fn an_effect_shows_what_is_left_of_it_on_the_games_clock_and_one_for_good_shows_none() {
    let mut s = GameState::default();
    let mut view = view_with([1, 1, 1, 1]);
    view.clock.game_time = 130.0;
    view.effects = Some(v::Effects {
        version: 1,
        now: 130.0,
        effects: vec![
            v::Effect {
                id: 1,
                name: "Haste".into(),
                icon: 0x0600_1234,
                time_cast: 100.0,
                duration: 60.0,
                pulsed: false,
            },
            v::Effect {
                id: 2,
                name: "Blessing".into(),
                time_cast: 0.0,
                duration: -1.0,
                ..v::Effect::default()
            },
        ],
    });
    follow(&mut s, &view, &[]);
    assert_eq!(s.effects.len(), 2);
    assert_eq!(
        (s.effects[0].name.as_str(), s.effects[0].icon),
        ("Haste", 0x0600_1234)
    );
    assert_eq!(s.effects[0].left, Some(30.0));
    assert_eq!(s.effects[1].left, None);
}

#[test]
fn the_log_takes_the_clients_lines_as_errors_then_the_chat_windows_lines_in_order() {
    let mut s = GameState::default();
    let view = view_with([1, 1, 1, 1]);
    let events = [
        GameEvent::Chat(ChatEvent::UnknownCommand {
            text: "/nonsense".into(),
        }),
        GameEvent::Chat(ChatEvent::History {
            list: ChatHistory::Commands,
            text: Some("/emotelist".into()),
        }),
    ];
    s.follow(&Frame {
        view: Some(&view),
        notices: &[],
        events: &events,
        told: &["The Horizon interface is not available yet".to_owned()],
        lines: &[
            (0x10, "Rowan says, \"hi\"".to_owned()),
            (0x20, "A tell".to_owned()),
        ],
        write: &no_name,
        unknown_command: "Unknown command.",
    });
    let log: Vec<(u32, &str)> = s.log.iter().map(|l| (l.kind, l.text.as_str())).collect();
    assert_eq!(
        log,
        [
            (ERROR_TEXT, "The Horizon interface is not available yet"),
            (ERROR_TEXT, "Unknown command."),
            (0x10, "Rowan says, \"hi\""),
            (0x20, "A tell"),
        ]
    );
    assert_eq!(s.recalled, [Some("/emotelist".to_owned())]);
    for n in 0..LOG_LINES {
        s.say(0x10, n.to_string());
    }
    assert_eq!(s.log.len(), LOG_LINES, "the oldest let go");
    assert_eq!(s.log.front().map(|l| l.text.as_str()), Some("0"));
}

#[test]
fn a_line_ending_in_a_line_break_is_logged_without_it_and_a_blank_line_inside_is_kept() {
    let mut s = GameState::default();
    s.say(
        0x10,
        "Please type /help for a list of available commands.\n".into(),
    );
    s.say(0x10, "Rowan says, \"hi\"\r\n".into());
    s.say(0x80, "A heading\n\nand what follows it\n".into());
    s.say(0x80, "Two breaks\n\n".into());
    s.say(0x10, "No break".into());
    let log: Vec<&str> = s.log.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        log,
        [
            "Please type /help for a list of available commands.",
            "Rowan says, \"hi\"",
            "A heading\n\nand what follows it",
            "Two breaks\n",
            "No break",
        ]
    );
}

#[test]
fn the_chat_options_follow_the_views_player_options() {
    let mut s = GameState::default();
    let mut view = view_with([1, 1, 1, 1]);
    follow(&mut s, &view, &[]);
    assert_eq!(s.chat_options, None, "none before the options are told");
    let mut o = v::GameOptions::default();
    o.chat.filters = [0x10, 0x8_0000, 0, 0];
    o.chat.devoted = [0x10, 0x10_0000, 0, 0];
    view.options = Some(o.clone());
    follow(&mut s, &view, &[]);
    assert_eq!(s.chat_options, Some(o.chat));
}

#[test]
fn whether_the_player_is_a_hero_and_its_focus_come_from_the_view() {
    let mut s = GameState::default();
    let mut view = view_with([1, 1, 1, 1]);
    follow(&mut s, &view, &[]);
    assert!(!s.hero);
    let player = view.player.as_mut().expect("the player");
    player.hero = true;
    player.focus = Some([40, 120]);
    follow(&mut s, &view, &[]);
    assert!(s.hero);
    assert_eq!(
        s.focus,
        Vital {
            current: 40,
            max: 120
        }
    );
    // A notice since is not undone by the same view.
    let hero = about_player(Said::Player(PlayerNotice::Hero {
        hero: true,
        focus: 30,
        max_focus: 120,
    }));
    follow(&mut s, &view, &[hero]);
    follow(&mut s, &view, &[]);
    assert_eq!(s.focus.current, 30);
}
