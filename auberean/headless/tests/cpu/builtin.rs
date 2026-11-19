//! The program's flags as the built-in script, and that script run against a server double.

use auberean_client_sdk::contract::control::movement::{FORWARD, TURN_RIGHT};
use auberean_client_sdk::contract::TradeAction;
use auberean_client_sdk::Action;
use auberean_headless::builtin::{script, Flags};
use auberean_headless::script::{Character, Command};

use crate::double::{played_of, with};
use crate::world_entry;

/// The flags make the script that does what they ask: the character made when named to make and
/// entered, the standing orders set and the examination's tabs asked for at the entry; then from
/// the entry the walk's legs (forward from the second second for two, turning right for one, over
/// and over), the trade asked at the third second, the makings from the third second twelve
/// apart, the line said from the third second every ten; at the end the slots told, the log-off
/// and the stop. With no flags, the first character listed is entered for thirty seconds.
///
/// Behaviour: session.headless.the-flags-make-the-built-in-script
#[test]
fn the_flags_make_the_script_that_does_what_they_ask() {
    let f = Flags {
        create: Some("Tester".into()),
        seconds: 16.0,
        walk: true,
        say: Some("hi".into()),
        agree: true,
        offer: true,
        trade_with: Some(0x20),
        crafts: vec![(0x6F00_0001, 1), (0x6F00_0002, 0)],
        ..Flags::default()
    };
    let at = Command::At;
    let say = || Command::Say("hi".into());
    let craft = |recipe, spinner| Command::Craft { recipe, spinner };
    assert_eq!(
        script(&f),
        vec![
            Command::Login,
            Command::Create("Tester".into()),
            Command::Enter(Some(Character::Name("Tester".into()))),
            Command::Agree(true),
            Command::Offer(true),
            Command::Act(Action::Examination { tabs: Some(0x3FF) }),
            at(2.0),
            Command::Begin(FORWARD),
            Command::Position,
            at(3.0),
            Command::Act(Action::Trade(TradeAction::Open(0x20))),
            craft(0x6F00_0001, 1),
            say(),
            at(4.0),
            Command::End(FORWARD),
            Command::Begin(TURN_RIGHT),
            Command::Position,
            at(5.0),
            Command::End(TURN_RIGHT),
            Command::Begin(FORWARD),
            Command::Position,
            at(7.0),
            Command::End(FORWARD),
            Command::Begin(TURN_RIGHT),
            Command::Position,
            at(8.0),
            Command::End(TURN_RIGHT),
            Command::Begin(FORWARD),
            Command::Position,
            at(10.0),
            Command::End(FORWARD),
            Command::Begin(TURN_RIGHT),
            Command::Position,
            at(11.0),
            Command::End(TURN_RIGHT),
            Command::Begin(FORWARD),
            Command::Position,
            at(13.0),
            Command::End(FORWARD),
            Command::Begin(TURN_RIGHT),
            Command::Position,
            say(),
            at(14.0),
            Command::End(TURN_RIGHT),
            Command::Begin(FORWARD),
            Command::Position,
            at(15.0),
            craft(0x6F00_0002, 0),
            at(16.0),
            Command::Slots,
            Command::Logoff,
            Command::Quit,
        ]
    );
    assert_eq!(
        script(&Flags::default()),
        vec![
            Command::Login,
            Command::Enter(None),
            Command::Act(Action::Examination { tabs: Some(0x3FF) }),
            at(30.0),
            Command::Logoff,
            Command::Quit,
        ]
    );
    let by_id = script(&Flags {
        character: Some(0x2130_0000_0000_0001),
        ..Flags::default()
    });
    assert_eq!(
        by_id[1],
        Command::Enter(Some(Character::Id(0x2130_0000_0000_0001)))
    );
}

/// The built-in script of `--say`, `--trade-with` and `--seconds` does what the flags did: in the
/// world it asks for the trade and says its line three seconds after entering, the line again ten
/// seconds later, and logs off when its time is up.
///
/// Behaviour: session.headless.the-built-in-script-does-what-its-flags-did
#[test]
fn the_built_in_script_says_asks_for_a_trade_and_logs_off_when_its_flags_say() {
    let commands = script(&Flags {
        seconds: 15.0,
        say: Some("hello".into()),
        trade_with: Some(0x2130_0000_0000_0002),
        ..Flags::default()
    });
    let (_, lines, _) = played_of(world_entry::script(), commands);
    let at = |text: &str| -> Vec<f64> {
        with(&lines, text)
            .iter()
            .map(|l| {
                l.split_whitespace()
                    .next()
                    .expect("a time")
                    .parse()
                    .expect("seconds")
            })
            .collect()
    };
    let trade = at("acts: Trade(Open(2391411402133733378))");
    assert_eq!(trade.len(), 1, "{lines:#?}");
    assert!((3.0..3.05).contains(&trade[0]), "{trade:?}");
    let said = at("saying: hello");
    assert_eq!(said.len(), 2);
    assert!((3.0..3.05).contains(&said[0]) && (13.0..13.05).contains(&said[1]));
    let off = at("logging off");
    assert!((15.0..15.05).contains(&off[0]), "{off:?}");
    assert_eq!(lines.last().map(String::as_str), Some("done"));
}
