//! The player's level, experience, vitae, hero status and focus as the view carries them, from the
//! player's own qualities as the server's private updates leave them.

use auberean_client_contract::view::Progress;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::ServerMessage;

use super::download::file;

const PLAYER: u64 = 0x5000_0001;

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![file(0x1_0000_0000, &[]), file(0x2_0000_0001, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

fn progress(c: &mut Client) -> Progress {
    c.refresh_view(&|_| Vec::new());
    c.view().player.as_ref().expect("the player").progress
}

/// The view carries the player's level, its experience in all and to spend, its vitae and the
/// experience to its vitae's next point, each as the server last sent it; none of them until it
/// does, the vitae then counting as full.
///
/// Behaviour: ui.player.the-view-carries-the-level-experience-and-vitae
#[test]
fn the_view_carries_the_players_level_experience_and_vitae_from_its_qualities() {
    let mut c = client();
    c.model.player = Some(PLAYER);
    c.take_message(
        0x0300_0000_0000_0000,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    let none = progress(&mut c);
    assert_eq!(none, Progress::default());
    assert!((none.vitae_or_full() - 100.0).abs() < f32::EPSILON);
    let mut stamp = 0;
    let mut send = |c: &mut Client, m: ServerMessage| {
        stamp += 1;
        c.take_message(0x0100_0000_0000_0000 | (stamp << 32) | (0x10 * stamp), m);
    };
    send(
        &mut c,
        ServerMessage::UpdateInt(PrivateQuality {
            quality: 302,
            value: 21,
        }),
    );
    for (quality, value) in [(300, 9_876_543), (301, 138_195), (316, 1_234)] {
        send(
            &mut c,
            ServerMessage::UpdateLongInt(PrivateQuality { quality, value }),
        );
    }
    send(
        &mut c,
        ServerMessage::UpdateFloat(PrivateQuality {
            quality: 286,
            value: 95.0,
        }),
    );
    assert_eq!(
        progress(&mut c),
        Progress {
            level: Some(21),
            total_experience: Some(9_876_543),
            available_experience: Some(138_195),
            vitae: Some(95.0),
            vitae_experience: Some(1_234),
        }
    );
}

/// The view says whether the player is a hero and its focus of its greatest (a hundred and what
/// its qualities add), as the server last sent each; no hero and no focus until it does.
///
/// Behaviour: ui.player.the-view-says-whether-the-player-is-a-hero-and-its-focus
#[test]
fn the_view_says_whether_the_player_is_a_hero_and_its_focus() {
    let mut c = client();
    c.model.player = Some(PLAYER);
    c.take_message(
        0x0300_0000_0000_0000,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    let hero = |c: &mut Client| {
        c.refresh_view(&|_| Vec::new());
        let p = c.view().player.as_ref().expect("the player");
        (p.hero, p.focus)
    };
    assert_eq!(hero(&mut c), (false, None));
    let mut stamp = 0;
    let mut send = |c: &mut Client, m: ServerMessage| {
        stamp += 1;
        c.take_message(0x0100_0000_0000_0000 | (stamp << 32) | (0x10 * stamp), m);
    };
    send(
        &mut c,
        ServerMessage::UpdateBool(PrivateQuality {
            quality: 4000,
            value: 1,
        }),
    );
    send(
        &mut c,
        ServerMessage::UpdateInt(PrivateQuality {
            quality: 266,
            value: 50,
        }),
    );
    assert_eq!(hero(&mut c), (true, Some([50, 100])));
    send(
        &mut c,
        ServerMessage::UpdateInt(PrivateQuality {
            quality: 267,
            value: 20,
        }),
    );
    assert_eq!(hero(&mut c), (true, Some([50, 120])));
    send(
        &mut c,
        ServerMessage::UpdateBool(PrivateQuality {
            quality: 4000,
            value: 0,
        }),
    );
    assert!(!hero(&mut c).0);
}
