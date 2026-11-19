//! The story's scenes: played over the screen step by step with their sounds, the world hidden
//! meanwhile, the game told which played; cut short with Escape; a portal's scene until the player
//! arrives.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{
    InterfaceNotice, Keyframe, Notice, QuestNotice, Said, Scene, Scenes,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::Action;
use auberean_horizon::state::GameState;
use auberean_shell::interface::{Request, SoundOrder, SoundSlot};

use crate::windows::journal::{follow, view};
use crate::Harness;

fn scenes() -> Scenes {
    let step = |picture, music, seconds| Keyframe {
        picture: Some(picture),
        music,
        voice: None,
        subtitle: Some(TextRef::literal("Long ago")),
        seconds,
    };
    Scenes {
        scenes: vec![
            Scene {
                id: 1,
                playable: true,
                keyframes: vec![
                    step(0x4100_0001, Some(0x2C00_0001), Some(1.0)),
                    step(0x4100_0002, None, None),
                ],
            },
            Scene {
                id: 2,
                playable: false,
                keyframes: vec![step(0x4100_0003, None, Some(30.0))],
            },
        ],
        packed: vec![1, 2, 3],
    }
}

fn play(scenes: Scenes) -> Notice {
    Notice {
        about: None,
        said: Said::Quest(QuestNotice::Scenes(Some(scenes))),
    }
}

fn portal(playing: bool, scenes: Option<Scenes>) -> Notice {
    Notice {
        about: None,
        said: Said::Interface(InterfaceNotice::PortalScene { playing, scenes }),
    }
}

/// Frames at 60 a second for `seconds`; every request and action they asked, in order.
fn run(h: &mut Harness, s: &GameState, seconds: f64) -> (Vec<Request>, Vec<Action>) {
    let (mut requests, mut acts) = (Vec::new(), Vec::new());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(seconds * 60.0) as usize {
        let out = h.frame(s);
        requests.extend(out.requests);
        acts.extend(out.acts);
    }
    (requests, acts)
}

fn music_played(requests: &[Request]) -> Vec<u32> {
    requests
        .iter()
        .filter_map(|r| match r {
            Request::Sound(SoundOrder::Play {
                data,
                slot: Some(SoundSlot::StoryMusic),
            }) => Some(data.id),
            _ => None,
        })
        .collect()
}

#[test]
fn the_scenes_play_their_playable_steps_in_turn_then_the_world_comes_back_and_the_game_is_told() {
    let mut s = GameState::default();
    let vw = view(Vec::new(), 1);
    follow(&mut s, &vw, &[play(scenes())]);
    let mut h = Harness::new();
    let out = h.frame(&s);
    assert_eq!(out.requests.first(), Some(&Request::ShowWorld(false)));
    assert_eq!(music_played(&out.requests), vec![0x2C00_0001]);
    let playing = h.ui.story.playing.as_ref().expect("playing");
    assert_eq!(playing.picture, Some(0x4100_0001));
    assert_eq!(
        s.journal.subtitle(&TextRef::literal("Long ago")),
        Some("Long ago")
    );
    follow(&mut s, &vw, &[]);
    // The first step lasts a second; the second its five.
    let (requests, acts) = run(&mut h, &s, 1.1);
    assert!(acts.is_empty() && !requests.contains(&Request::ShowWorld(true)));
    assert_eq!(
        h.ui.story.playing.as_ref().and_then(|p| p.picture),
        Some(0x4100_0002)
    );
    let (requests, acts) = run(&mut h, &s, 5.0);
    assert!(
        h.ui.story.playing.is_none(),
        "the scene not playable was left out"
    );
    assert!(requests.contains(&Request::ShowWorld(true)));
    assert!(requests.contains(&Request::Sound(SoundOrder::Stop {
        slot: SoundSlot::StoryMusic,
        fade: 2.0
    })));
    assert_eq!(
        acts,
        vec![Action::PlayScenesDone {
            scenes: vec![1, 2, 3]
        }]
    );
}

#[test]
fn escape_cuts_the_scenes_short_and_the_game_hears_they_played() {
    let mut s = GameState::default();
    let vw = view(Vec::new(), 1);
    follow(&mut s, &vw, &[play(scenes())]);
    let mut h = Harness::new();
    h.frame(&s);
    follow(&mut s, &vw, &[]);
    assert!(h.ui.window_open(), "Escape has something to do");
    let mut acts = Vec::new();
    assert!(h.ui.close_top(&mut acts));
    let out = h.frame(&s);
    assert!(h.ui.story.playing.is_none());
    assert!(out.requests.contains(&Request::ShowWorld(true)));
    assert_eq!(
        out.acts,
        vec![Action::PlayScenesDone {
            scenes: vec![1, 2, 3]
        }]
    );
}

#[test]
fn a_portals_scene_plays_until_the_player_arrives_and_escape_does_not_cut_it_short() {
    let mut s = GameState::default();
    let vw = view(Vec::new(), 1);
    follow(&mut s, &vw, &[portal(true, Some(scenes()))]);
    let mut h = Harness::new();
    let out = h.frame(&s);
    assert_eq!(
        out.requests.first(),
        Some(&Request::PortalScenePlaying(true))
    );
    assert!(h.ui.story.is_playing());
    assert!(!h.ui.story.skippable());
    follow(&mut s, &vw, &[]);
    // Played to its end, the game is not told while the portal's scene is on.
    let (_, acts) = run(&mut h, &s, 7.0);
    assert!(acts.is_empty(), "{acts:?}");
    assert!(h.ui.story.portal, "still on the way");
    // The player arrives: the portal's scene is over.
    follow(&mut s, &vw, &[]);
    let mut arrived = s.clone();
    arrived.journal.arrived = true;
    let out = h.frame(&arrived);
    assert!(out.requests.contains(&Request::PortalScenePlaying(false)));
    assert_eq!(
        out.acts,
        vec![Action::PlayScenesDone { scenes: Vec::new() }]
    );
    assert!(!h.ui.story.portal);
}
