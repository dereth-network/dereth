//! The script's lines and the actions' text form, read.

use auberean_client_sdk::contract::control::{movement, Control, ControlId};
use auberean_client_sdk::contract::{CraftAction, SelectKind, StoreAction, TradeAction};
use auberean_client_sdk::Action;
use auberean_headless::act::{self, TEXT_FORMS};
use auberean_headless::script::{parse, Character, Command, Dump, ScriptError, Selecting};

/// Every verb reads as its command, with its words: names and ids, controls by name or id,
/// numbers decimal or hexadecimal; `say` keeps all after it, a `#` included, any other line
/// loses its comment; blank lines and comments are nothing. A wrong line is told with its number
/// and what is wrong.
///
/// Behaviour: session.headless.a-script-reads-one-command-a-line
#[test]
fn every_verb_reads_as_its_command_and_a_wrong_line_is_told_with_its_number() {
    let text = "\
# a whole script
login
create Tester
enter
enter Tester
enter 0x2130000000000001
tick 30
run 2.5
at 4

act trade-open 0x10
begin forward
end turn-left
press jump
hold run-lock 1
hold 0x40000099 0.5
walk strafe-right 2   # a comment
say hello  # all of it
select foe
select next
select previous player
select self
select last
select attacker
select none
select 0x20
use
use 0x20
craft 0x6F010203 -2
slots
agree on
offer off
snapshot
dump events
dump notices
position
plugin load some/folder # loads
ui click a b c
logoff
quit
";
    let read = parse(text).expect("a script");
    let c = |id: u32| ControlId(id);
    assert_eq!(
        read,
        vec![
            Command::Login,
            Command::Create("Tester".into()),
            Command::Enter(None),
            Command::Enter(Some(Character::Name("Tester".into()))),
            Command::Enter(Some(Character::Id(0x2130_0000_0000_0001))),
            Command::Tick(30),
            Command::Run(2.5),
            Command::At(4.0),
            Command::Act(Action::Trade(TradeAction::Open(0x10))),
            Command::Begin(movement::FORWARD),
            Command::End(movement::TURN_LEFT),
            Command::Press(movement::JUMP),
            Command::Hold(movement::RUN_LOCK, 1.0),
            Command::Hold(c(0x4000_0099), 0.5),
            Command::Hold(movement::STRAFE_RIGHT, 2.0),
            Command::Say("hello  # all of it".into()),
            Command::Select(Selecting::Nearest(SelectKind::Foe)),
            Command::Select(Selecting::Next(SelectKind::Creature)),
            Command::Select(Selecting::Previous(SelectKind::Player)),
            Command::Select(Selecting::Me),
            Command::Select(Selecting::Last),
            Command::Select(Selecting::Attacker),
            Command::Select(Selecting::Nothing),
            Command::Select(Selecting::Object(0x20)),
            Command::Use(None),
            Command::Use(Some(0x20)),
            Command::Craft {
                recipe: 0x6F01_0203,
                spinner: -2
            },
            Command::Slots,
            Command::Agree(true),
            Command::Offer(false),
            Command::Snapshot,
            Command::Dump(Dump::Events),
            Command::Dump(Dump::Notices),
            Command::Position,
            Command::Plugin("plugin load some/folder".into()),
            Command::Plugin("ui click a b c".into()),
            Command::Logoff,
            Command::Quit,
        ]
    );
    let wrong = |text: &str| parse(text).expect_err("a wrong line");
    assert_eq!(
        wrong("login\n\ntick many\n"),
        ScriptError {
            line: 3,
            what: "tick needs a count of frames, not \"many\"".into()
        }
    );
    assert_eq!(wrong("dance").what, "no command \"dance\"");
    assert_eq!(wrong("walk up 2").line, 1);
    assert!(wrong("walk up 2").what.contains("forward, backward"));
    assert_eq!(
        wrong("hold forward -1").what,
        "hold needs seconds, not \"-1\""
    );
    assert_eq!(wrong("select next ghost").what, "select: no kind \"ghost\"");
    assert_eq!(
        wrong("agree maybe").what,
        "agree takes on or off, not \"maybe\""
    );
    assert_eq!(wrong("quit now").what, "quit takes nothing more");
    assert_eq!(wrong("say").what, "say needs its text");
    assert_eq!(wrong("act").what, "act needs an action");
}

/// Every action named reads from its words; a wrong one says what the action takes, and an
/// unknown name lists the names.
///
/// Behaviour: session.headless.actions-read-from-their-text-form
#[test]
fn every_action_named_reads_from_its_words_and_a_wrong_one_says_what_it_takes() {
    let read = |line: &str| act::parse(&line.split_whitespace().collect::<Vec<_>>());
    let cases = [
        (
            "play-character 0x2130000000000001",
            Action::PlayCharacter {
                character: 0x2130_0000_0000_0001,
            },
        ),
        (
            "examination 0x3FF",
            Action::Examination { tabs: Some(0x3FF) },
        ),
        ("examination close", Action::Examination { tabs: None }),
        (
            "select-cycle pet previous",
            Action::SelectCycle {
                kind: SelectKind::Pet,
                previous: true,
            },
        ),
        ("attack off", Action::Attack { start: false }),
        (
            "put-item 1 2 3 4 5 6",
            Action::PutItem {
                item: 1,
                from: 2,
                to: 3,
                old_slot: 4,
                new_slot: 5,
                quantity: 6,
            },
        ),
        (
            "craft-execute 0x6F000001 -1 1:0x10 2:0x11",
            Action::Craft(CraftAction::ExecuteCraft {
                recipe: 0x6F00_0001,
                spinner: -1,
                ingredients: vec![(1, 0x10), (2, 0x11)],
            }),
        ),
        (
            "store-collect 7 coins",
            Action::Store(StoreAction::Collect {
                sale: 7,
                sold: true,
            }),
        ),
        (
            "server-call 0x82120016 0a0b",
            Action::ServerCall {
                function: 0x8212_0016,
                args: vec![0x0A, 0x0B],
            },
        ),
        (
            "say well   met",
            Action::Say {
                text: "well met".into(),
            },
        ),
        (
            "control 0x40000020 0.5",
            Action::Control(Control::at(movement::FORWARD, 0.5)),
        ),
        (
            "release-controls typing",
            Action::ReleaseControls { typing: true },
        ),
        (
            "answer-question 3 off",
            Action::AnswerQuestion {
                question: 3,
                yes: false,
            },
        ),
    ];
    for (line, want) in cases {
        assert_eq!(read(line), Ok(want), "{line}");
    }
    // Every name listed is an action: without its words it says what it takes, or reads.
    for (name, takes) in TEXT_FORMS {
        match read(name) {
            Ok(_) => assert!(
                takes.is_empty() || takes.starts_with('['),
                "{name} read without its words"
            ),
            Err(e) => assert_eq!(e, format!("{name} takes {takes}"), "{name}"),
        }
    }
    assert_eq!(
        read("trade-offer 0x10"),
        Err("trade-offer takes <item> <quantity>".into())
    );
    assert_eq!(
        read("log-off now"),
        Err("log-off takes nothing more".into())
    );
    let unknown = read("dance").expect_err("no such action");
    assert!(unknown.starts_with("no action \"dance\": one of play-character, log-off"));
}
