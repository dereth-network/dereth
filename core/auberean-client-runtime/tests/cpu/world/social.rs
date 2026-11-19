//! The player's fellowship, allegiance, friends and kingdom, kept for every front end: what each
//! notice of the scripts changes, and the part of the view that carries them.

use auberean_client_contract::notice::{
    Allegiance, AllegianceNotice, Fellow, Fellowship, FellowshipNotice, FriendState, FriendsNotice,
    Member, Notice, PlayerNotice, Said,
};
use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::{conflict, friend_mode, kingdom};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::said::native;
use auberean_client_runtime::social_view::SocialRecords;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::login::Empty;
use auberean_protocol::physics::CreateObject;
use auberean_protocol::types::WeenieDesc;
use auberean_protocol::ServerMessage;

use super::download::file;

const PLAYER: u64 = 0x2130_0000_0000_00A1;
const CARA: u64 = 0x2130_0000_0000_00B2;
const DOV: u64 = 0x2130_0000_0000_00C3;

fn said(said: Said) -> Notice {
    Notice { about: None, said }
}

fn fellowship(f: FellowshipNotice) -> Notice {
    said(Said::Fellowship(f))
}

fn fellow(name: &str, health: u32) -> Fellow {
    Fellow {
        name: TextRef::literal(name),
        level: 10,
        health,
        max_health: 100,
        vigor: 50,
        max_vigor: 60,
        cell: 0x0101_0001,
        ..Fellow::default()
    }
}

/// The fellows kept, by id, and their health.
fn fellows(r: &SocialRecords) -> Vec<(u64, u32)> {
    r.social()
        .fellowship
        .as_ref()
        .map(|f| f.fellows.iter().map(|(id, f)| (*id, f.health)).collect())
        .unwrap_or_default()
}

/// A fellowship described whole is kept; a fellow described keeps its place and one new to it
/// goes last; a fellow gone leaves; its health, vigor, their greatest, its level and its cell,
/// and the leader, are kept as told; the fellowship gone, nothing is kept, and what is told of
/// fellows then is let be.
///
/// Behaviour: ui.social.the-fellowship-is-kept-as-its-notices-tell-it
#[test]
fn the_fellowship_is_kept_as_its_notices_tell_it() {
    let mut r = SocialRecords::default();
    r.hear(
        &fellowship(FellowshipNotice::Fellow {
            fellow: CARA,
            record: Some(Box::new(fellow("Cara", 1))),
        }),
        Some(PLAYER),
    );
    assert_eq!(r.social().fellowship, None, "in none: let be");
    r.hear(
        &fellowship(FellowshipNotice::Changed(Some(Box::new(Fellowship {
            fellows: vec![(PLAYER, fellow("Me", 90)), (CARA, fellow("Cara", 80))],
            name: "The Hunt".into(),
            leader: PLAYER,
            social: 1,
            looting: 1,
        })))),
        Some(PLAYER),
    );
    assert_eq!(fellows(&r), [(PLAYER, 90), (CARA, 80)]);
    r.hear(
        &fellowship(FellowshipNotice::Fellow {
            fellow: CARA,
            record: Some(Box::new(fellow("Cara", 70))),
        }),
        Some(PLAYER),
    );
    r.hear(
        &fellowship(FellowshipNotice::Fellow {
            fellow: DOV,
            record: Some(Box::new(fellow("Dov", 60))),
        }),
        Some(PLAYER),
    );
    assert_eq!(fellows(&r), [(PLAYER, 90), (CARA, 70), (DOV, 60)]);
    for n in [
        FellowshipNotice::Health {
            fellow: DOV,
            health: 40,
            taken_by_players: 5,
        },
        FellowshipNotice::MaxHealth {
            fellow: DOV,
            max: 120,
        },
        FellowshipNotice::Vigor {
            fellow: DOV,
            vigor: 30,
            taken_by_players: 2,
        },
        FellowshipNotice::MaxVigor {
            fellow: DOV,
            max: 70,
        },
        FellowshipNotice::Level {
            fellow: DOV,
            level: 12,
        },
        FellowshipNotice::Cell {
            fellow: DOV,
            cell: 0x0202_0002,
        },
        FellowshipNotice::Leader { leader: CARA },
        FellowshipNotice::FellowGone { fellow: PLAYER },
    ] {
        r.hear(&fellowship(n), Some(PLAYER));
    }
    let f = r.social().fellowship.clone().expect("a fellowship");
    assert_eq!(f.leader, CARA);
    assert_eq!(f.name, "The Hunt");
    assert_eq!(fellows(&r), [(CARA, 70), (DOV, 40)]);
    let dov = &f.fellows[1].1;
    assert_eq!(
        (
            dov.health_taken_by_players,
            dov.max_health,
            dov.vigor,
            dov.vigor_taken_by_players,
            dov.max_vigor,
            dov.level,
            dov.cell
        ),
        (5, 120, 30, 2, 70, 12, 0x0202_0002)
    );
    r.hear(&fellowship(FellowshipNotice::Cleared), Some(PLAYER));
    assert_eq!(r.social().fellowship, None);
}

/// The service's list of friends replaces those kept, a friend with no name left out; a friend's
/// state is kept for a friend listed, one going online or offline goes last, one not listed is
/// let be; a friend taken off leaves; the player's own state is kept. Each allegiance profile told
/// replaces the last, and the conflict the place allows is kept as told about the player.
///
/// Behaviour: ui.social.friends-the-allegiance-and-the-conflict-are-kept
#[test]
fn friends_the_allegiance_and_the_conflict_are_kept_as_told() {
    let mut r = SocialRecords::default();
    let friend = |name: &str, mode| FriendState {
        name: name.into(),
        location: String::new(),
        mode,
    };
    let friends = |r: &SocialRecords| -> Vec<(String, u32)> {
        r.social()
            .friends
            .iter()
            .map(|f| (f.name.clone(), f.mode))
            .collect()
    };
    let hear = |r: &mut SocialRecords, f| r.hear(&said(Said::Friends(f)), Some(PLAYER));
    hear(
        &mut r,
        FriendsNotice::List(vec![
            friend("Cara", friend_mode::AVAILABLE),
            friend("", friend_mode::AVAILABLE),
            friend("Dov", 0),
            friend("Eli", friend_mode::AWAY),
        ]),
    );
    assert_eq!(
        friends(&r),
        [("Cara".into(), 1), ("Dov".into(), 0), ("Eli".into(), 2)]
    );
    hear(&mut r, FriendsNotice::State(Some(friend("Dov", 4))));
    hear(&mut r, FriendsNotice::State(Some(friend("Eli", 1))));
    hear(&mut r, FriendsNotice::State(Some(friend("Fay", 1))));
    assert_eq!(
        friends(&r),
        [("Cara".into(), 1), ("Eli".into(), 1), ("Dov".into(), 4)],
        "Dov came online and went last; Eli stayed in its place; Fay is no friend"
    );
    hear(
        &mut r,
        FriendsNotice::Removed {
            name: Some(TextRef::literal("Cara")),
        },
    );
    assert_eq!(friends(&r), [("Eli".into(), 1), ("Dov".into(), 4)]);
    hear(
        &mut r,
        FriendsNotice::OwnState {
            mode: friend_mode::BUSY,
        },
    );
    assert_eq!(r.social().own_state, Some(friend_mode::BUSY));

    let profile = |id| Allegiance {
        member: (
            Member {
                id,
                ..Member::default()
            },
            true,
        ),
        total: 3,
        ..Allegiance::default()
    };
    for id in [PLAYER, CARA] {
        r.hear(
            &said(Said::Allegiance(AllegianceNotice::Changed(Some(Box::new(
                profile(id),
            ))))),
            Some(PLAYER),
        );
    }
    assert_eq!(r.social().allegiance, Some(profile(CARA)));

    let place = |about, kind| Notice {
        about,
        said: Said::Player(PlayerNotice::Conflict { kind }),
    };
    r.hear(&place(None, conflict::CONTROLLED), Some(PLAYER));
    r.hear(&place(Some(CARA), conflict::FREE_FOR_ALL), Some(PLAYER));
    assert_eq!(
        r.social().conflict,
        Some(conflict::CONTROLLED),
        "about another: let be"
    );
}

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

/// A notice as the scripts send it to every panel: its words.
fn sent(native: u32, words: Vec<u32>) -> PanelNotice {
    let n = words.len();
    PanelNotice {
        native,
        about: None,
        words,
        objects: vec![None; n],
        requests: vec![None; n],
        effect_lines: vec![Vec::new(); n],
        listed: vec![Vec::new(); n],
        selections: vec![None; n],
    }
}

fn refresh(c: &mut Client) {
    c.refresh_view(&|_| Vec::new());
}

/// The view carries what the scripts' notices told of the player's social life (here the conflict
/// the place allows and the player's own state on the chat service) and the player's kingdom from
/// its description; leaving the world lets all of it go.
///
/// Behaviour: ui.social.the-view-carries-the-players-social-life
#[test]
fn the_view_carries_the_players_social_life_until_it_leaves_the_world() {
    let mut c = client();
    c.model.player = Some(PLAYER);
    let mut weenie = WeenieDesc::default();
    weenie.mask |= 0x2000;
    weenie.numbers[2] = kingdom::ORDER.cast_signed();
    c.take_message(
        3 << 56,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            weenie,
            ..CreateObject::default()
        }),
    );
    c.notices.push(sent(
        native::CONFLICT,
        vec![conflict::KINGDOM_AGAINST_KINGDOM],
    ));
    c.notices
        .push(sent(native::OWN_STATE, vec![friend_mode::AWAY]));
    refresh(&mut c);
    refresh(&mut c);
    let s = &c.view().social;
    assert_eq!(s.conflict, Some(conflict::KINGDOM_AGAINST_KINGDOM));
    assert_eq!(s.own_state, Some(friend_mode::AWAY));
    assert_eq!(s.kingdom, Some(kingdom::ORDER));
    c.take_message(3 << 56, ServerMessage::CharExitGame(Empty));
    refresh(&mut c);
    let s = &c.view().social;
    assert_eq!((s.conflict, s.own_state, s.kingdom), (None, None, None));
}
