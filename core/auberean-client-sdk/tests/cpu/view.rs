//! The game's view the client keeps, refreshed by its host, against a server double with no
//! scripts: what the server told, as the model holds it.

use auberean_client_sdk::contract::event::SessionEvent;
use auberean_client_sdk::contract::view::ServerCalls;
use auberean_client_sdk::{no_icons, Action, FixedStep, GameEvent, Phase};

use crate::driver::{driven, FRAMES, PLAYER};

/// Refreshed each frame, the view lists the account's characters as the server listed them (their
/// names, the slots, the account), then, once the player is in the world, the player and where its
/// body stands; until a refresh, it says what the last one read.
///
/// Behaviour: session.view.the-view-says-what-the-server-told
#[test]
fn the_view_lists_the_characters_then_the_player_where_the_server_put_it() {
    let (mut driver, mut port) = driven();
    let mut clock = FixedStep::new(1.0 / 60.0);
    assert!(driver.view().pregame.characters.is_none(), "nothing yet");
    let mut listed = None;
    let mut in_world = false;
    for _ in 0..FRAMES {
        driver.frame(&mut clock, &mut port);
        let events = driver.take_events();
        if listed.is_none() {
            if let Some(list) = events.iter().find_map(|e| match e {
                GameEvent::Session(SessionEvent::Characters(l)) => Some(l.clone()),
                _ => None,
            }) {
                assert!(
                    driver.view().pregame.characters.is_none(),
                    "not refreshed yet"
                );
                driver.refresh_view(&no_icons);
                listed = driver.view().pregame.characters.clone();
                assert_eq!(listed.as_ref(), Some(&list), "the list the events told");
                driver.act(Action::PlayCharacter { character: PLAYER });
            }
        }
        if events.contains(&GameEvent::Session(SessionEvent::Phase(Phase::World))) {
            in_world = true;
            break;
        }
    }
    let list = listed.expect("the characters were listed");
    let names: Vec<&str> = list.characters.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Tester"]);
    assert_eq!((list.slots, list.account.as_str()), (6, "tester"));
    assert!(in_world, "in the world");
    assert!(
        driver.view().player.is_none(),
        "the last refresh was at the list"
    );
    driver.refresh_view(&no_icons);
    let view = driver.view();
    let player = view.player.as_ref().expect("the player");
    assert_eq!(player.id, PLAYER);
    assert_eq!(view.player_id, Some(PLAYER));
    assert_eq!(player.cell, Some(0x0298_0109), "where the server put it");
    assert_eq!(player.body.cell, 0x0298_0109, "its body there");
    assert!(view.world.is_some(), "the objects around the player");
    assert_eq!(view.refreshes, 2);
}

/// The view counts the server's calls the player's scripts ran and those that ran to their end,
/// as they stand at the refresh.
///
/// Behaviour: session.view.the-view-counts-the-servers-calls
#[test]
fn the_view_counts_the_servers_calls_run_and_run_to_their_end() {
    let (mut driver, _) = driven();
    driver.refresh_view(&no_icons);
    assert_eq!(driver.view().calls, ServerCalls::default(), "none yet");
    driver.client_mut().calls.calls = 7;
    driver.client_mut().calls.completed = 5;
    assert_eq!(driver.view().calls.run, 0, "not refreshed yet");
    driver.refresh_view(&no_icons);
    assert_eq!(
        driver.view().calls,
        ServerCalls {
            run: 7,
            completed: 5
        }
    );
}
