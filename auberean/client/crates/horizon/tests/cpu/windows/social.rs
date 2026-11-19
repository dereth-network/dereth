//! The social window: its key, its tabs, and what each page asks of the game; and the social
//! state followed from the view and the notices.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{
    self as n, ChatNotice, FriendState, FriendsNotice, Notice, Said,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{friend_mode, Body, Player, Social as ViewSocial, View};
use auberean_client_sdk::contract::SocialAction;
use auberean_client_sdk::Action;
use auberean_horizon::keys::Keyed;
use auberean_horizon::social::{Allegiance, Friend, Rank};
use auberean_horizon::state::{Frame, GameState, Target};
use auberean_horizon::ui::social::{
    fellow_rect, friend_rects, looting_rect, parts, states_rect, vassal_rect, ALLEGIANCE, FRIENDS,
};
use auberean_horizon::ui::vk;

use crate::play::party::{in_party, CARA, DOV};
use crate::{centre, in_world, Harness, PLAYER};

fn social(a: SocialAction) -> Action {
    Action::Social(a)
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// The window opened on its key, at `tab`, after a frame drawn.
fn opened(h: &mut Harness, s: &GameState, tab: usize) -> Vec<Action> {
    let mut acts = h.ui.keyed(Keyed::Social, s);
    h.ui.social.tab = tab;
    acts.extend(h.frame(s).acts);
    acts
}

fn page(h: &Harness) -> auberean_horizon::ui::social::Parts {
    parts(h.ui.social.body().expect("open"), h.k())
}

fn select(s: &mut GameState, id: u64, name: &str) {
    s.target = Some(Target {
        id,
        name: name.into(),
        health: None,
    });
}

#[test]
fn the_social_key_opens_the_window_and_escape_closes_it() {
    let mut h = Harness::new();
    let s = in_world();
    assert!(opened(&mut h, &s, 0).is_empty());
    assert!(h.ui.window_open() && h.ui.social.is_open());
    let mut acts = Vec::new();
    assert!(h.ui.close_top(&mut acts));
    assert!(!h.ui.social.is_open());
    assert!(h.ui.keyed(Keyed::Social, &s).is_empty());
    assert!(h.ui.social.is_open(), "the key opens it again");
    assert!(h.ui.keyed(Keyed::Social, &s).is_empty());
    assert!(!h.ui.social.is_open(), "and closes it");
}

#[test]
fn out_of_a_fellowship_create_asks_for_one_with_its_name_experience_and_loot() {
    let mut h = Harness::new();
    let s = in_world();
    opened(&mut h, &s, 0);
    let p = page(&h);
    let create = p.foot[3];
    assert!(h.click(&s, centre(create)).acts.is_empty(), "no name yet");
    h.click(&s, centre(p.field));
    assert!(h.ui.typing(), "the name box has the keys");
    h.type_text(&s, "The Hunt");
    h.click(&s, centre(looting_rect(p.page, h.k(), 3)));
    let share =
        auberean_horizon::draw::Rect::new(p.page.x + 2.0, p.page.y + 28.0 * h.k(), 10.0, 20.0);
    h.click(&s, centre(share));
    let out = h.click(&s, centre(create));
    assert_eq!(
        out.acts,
        [social(SocialAction::CreateFellowship {
            name: units("The Hunt"),
            social: false,
            looting: 3,
        })]
    );
    h.click(&s, centre(p.field));
    h.type_text(&s, "Solo");
    let out = h.key(&s, vk::ENTER);
    assert_eq!(
        out.acts,
        [social(SocialAction::CreateFellowship {
            name: units("Solo"),
            social: false,
            looting: 3,
        })],
        "Enter in the box creates it too"
    );
    let mut fresh = Harness::new();
    opened(&mut fresh, &s, 0);
    let p = page(&fresh);
    fresh.click(&s, centre(p.field));
    fresh.type_text(&s, "Plain");
    assert_eq!(
        fresh.click(&s, centre(p.foot[3])).acts,
        [social(SocialAction::CreateFellowship {
            name: units("Plain"),
            social: true,
            looting: 1,
        })],
        "as the game's page starts: no experience shared, loot shared"
    );
}

#[test]
fn in_a_fellowship_the_leader_recruits_dismisses_and_disbands_and_anyone_leaves() {
    let mut h = Harness::new();
    let mut s = in_party();
    select(&mut s, DOV, "Dov");
    opened(&mut h, &s, 0);
    let foot = page(&h).foot;
    for b in [0, 1, 3] {
        assert!(
            h.click(&s, centre(foot[b])).acts.is_empty(),
            "button {b} is the leader's"
        );
    }
    assert_eq!(
        h.click(&s, centre(foot[2])).acts,
        [social(SocialAction::LeaveFellowship)]
    );
    if let Some(p) = &mut s.social.party {
        p.leader = PLAYER;
    }
    let asked: Vec<Vec<Action>> = [0, 1, 3]
        .into_iter()
        .map(|b| h.click(&s, centre(foot[b])).acts)
        .collect();
    assert_eq!(
        asked,
        [
            vec![social(SocialAction::Recruit(DOV))],
            vec![social(SocialAction::Dismiss(DOV))],
            vec![social(SocialAction::DisbandFellowship)],
        ]
    );
    s.target = None;
    assert!(
        h.click(&s, centre(foot[0])).acts.is_empty(),
        "nothing to recruit"
    );
    let row = fellow_rect(page(&h).page, h.k(), 2);
    assert_eq!(
        h.click(&s, centre(row)).acts,
        [Action::SetSelection { target: DOV }],
        "a row selects its fellow"
    );
    assert_eq!(
        h.click(&s, centre(row)).acts,
        [
            Action::SetSelection { target: DOV },
            Action::AssistedTell { name: "Dov".into() },
        ],
        "and pressed again starts a tell"
    );
}

fn rank(id: u64, name: &str) -> Rank {
    Rank {
        id,
        name: name.into(),
        level: 30,
        online: true,
        ..Rank::default()
    }
}

/// A profile about `member`, its patron and vassals.
fn profile(member: u64, patron: Option<u64>, vassals: &[u64]) -> Allegiance {
    Allegiance {
        name: "The Wardens".into(),
        total: 4,
        monarch: Some(rank(CARA, "Cara")),
        member: Some(rank(member, "Member")),
        patron: patron.map(|p| rank(p, "Patron")),
        vassals: vassals.iter().map(|v| rank(*v, "Vassal")).collect(),
        ..Allegiance::default()
    }
}

#[test]
fn the_allegiance_is_asked_for_only_while_its_tab_shows_and_follows_the_member_shown() {
    let mut h = Harness::new();
    let mut s = in_world();
    assert_eq!(
        opened(&mut h, &s, ALLEGIANCE),
        [social(SocialAction::QueryAllegiance(PLAYER))],
        "the player's, as the tab shows"
    );
    assert!(h.frame(&s).acts.is_empty(), "asked once");
    s.social.allegiance = Some(profile(PLAYER, Some(CARA), &[DOV]));
    h.frame(&s);
    let p = page(&h);
    assert_eq!(
        h.click(&s, centre(vassal_rect(p.page, h.k(), 0))).acts,
        [social(SocialAction::QueryAllegiance(DOV))],
        "a vassal's row shows that vassal"
    );
    assert_eq!(
        h.click(&s, centre(p.foot[0])).acts,
        [social(SocialAction::QueryAllegiance(PLAYER))],
        "Home shows the player"
    );
    assert_eq!(
        h.click(&s, centre(p.foot[1])).acts,
        [social(SocialAction::QueryAllegiance(CARA))],
        "Up shows the player's patron"
    );
    h.ui.social.tab = FRIENDS;
    assert_eq!(
        h.frame(&s).acts,
        [social(SocialAction::QueryAllegiance(0))],
        "another tab: no longer asked"
    );
    h.ui.social.tab = ALLEGIANCE;
    assert_eq!(
        h.frame(&s).acts,
        [social(SocialAction::QueryAllegiance(CARA))]
    );
    let mut acts = Vec::new();
    h.ui.close_top(&mut acts);
    assert_eq!(acts, [social(SocialAction::QueryAllegiance(0))], "closed");
}

#[test]
fn breaking_with_the_patron_asks_first_and_swearing_needs_no_patron_and_a_player_selected() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.social.allegiance = Some(profile(PLAYER, Some(CARA), &[]));
    opened(&mut h, &s, ALLEGIANCE);
    let foot = page(&h).foot;
    select(&mut s, DOV, "Dov");
    assert!(
        h.click(&s, centre(foot[2])).acts.is_empty(),
        "Swear: the player has a patron"
    );
    assert!(
        h.click(&s, centre(foot[3])).acts.is_empty(),
        "Break: the player itself is shown"
    );
    h.click(&s, centre(foot[1]));
    // Cara's own patron is someone else: the player, her vassal, is shown after the break.
    s.social.allegiance = Some(profile(CARA, Some(0x2130_0000_0000_00D4), &[PLAYER]));
    h.frame(&s);
    assert!(h.click(&s, centre(foot[3])).acts.is_empty(), "asked first");
    assert_eq!(h.ui.social.breaking.as_ref().map(|b| b.id), Some(CARA));
    assert_eq!(
        h.click(&s, centre(foot[2])).acts,
        [
            social(SocialAction::Break(CARA)),
            social(SocialAction::QueryAllegiance(PLAYER)),
        ],
        "yes breaks, and the player is shown again"
    );
    assert!(h.ui.social.breaking.is_none());

    let mut h = Harness::new();
    s.social.allegiance = Some(profile(PLAYER, None, &[]));
    opened(&mut h, &s, ALLEGIANCE);
    assert_eq!(
        h.click(&s, centre(foot[2])).acts,
        [social(SocialAction::Swear)]
    );
    select(&mut s, 0x8130_0000_0000_0001, "a drudge");
    assert!(
        h.click(&s, centre(foot[2])).acts.is_empty(),
        "only to a player"
    );
}

fn friend(name: &str, mode: u32) -> Friend {
    Friend {
        name: name.into(),
        mode,
    }
}

#[test]
fn the_friends_page_adds_removes_and_tells_and_sets_the_players_state() {
    let mut h = Harness::new();
    h.ui.social.emote_table = Some(0x2500_0042);
    let mut s = in_world();
    s.social.friends = vec![
        friend("Dov", 0),
        friend("Cara", friend_mode::AWAY),
        friend("Eli", friend_mode::AVAILABLE),
    ];
    opened(&mut h, &s, FRIENDS);
    let p = page(&h);
    let rows = friend_rects(p.page, h.k(), &s.social.friends);
    let order: Vec<&str> = rows
        .iter()
        .map(|(i, _)| s.social.friends[*i].name.as_str())
        .collect();
    assert_eq!(order, ["Cara", "Eli", "Dov"], "online first, then offline");
    let row = |name: &str| {
        rows.iter()
            .find(|(i, _)| s.social.friends[*i].name == name)
            .map(|(_, r)| *r)
            .expect("listed")
    };
    let (remove, tell) = (p.foot[1], p.foot[2]);
    assert!(h.click(&s, centre(remove)).acts.is_empty(), "none chosen");
    h.click(&s, centre(row("Dov")));
    assert!(h.click(&s, centre(tell)).acts.is_empty(), "Dov is offline");
    assert_eq!(
        h.click(&s, centre(remove)).acts,
        [social(SocialAction::RemoveFriend(units("Dov")))]
    );
    h.click(&s, centre(row("Cara")));
    assert_eq!(
        h.click(&s, centre(tell)).acts,
        [Action::AssistedTell {
            name: "Cara".into()
        }]
    );

    let add = p.foot[0];
    assert!(
        h.click(&s, centre(add)).acts.is_empty(),
        "no name, nobody selected"
    );
    select(&mut s, 0x2130_0000_0000_00F0, "Fay");
    assert_eq!(
        h.click(&s, centre(add)).acts,
        [social(SocialAction::AddFriend(units("Fay")))],
        "the player selected"
    );
    select(&mut s, 0x2130_0000_0000_00F1, "Eli");
    assert!(h.click(&s, centre(add)).acts.is_empty(), "already a friend");
    h.click(&s, centre(p.field));
    h.type_text(&s, "Gil");
    assert_eq!(
        h.click(&s, centre(add)).acts,
        [social(SocialAction::AddFriend(units("Gil")))],
        "the name typed first"
    );

    let states = states_rect(p.page, h.k());
    let away = auberean_horizon::draw::Rect::new(
        states.x + states.w * 2.0 / 3.0 + 2.0,
        states.y,
        12.0,
        states.h,
    );
    let token = |name: &str| auberean_ui::token::Token::of(name).raw();
    assert_eq!(
        h.click(&s, centre(away)).acts,
        [
            social(SocialAction::SetState {
                mode: friend_mode::AWAY,
                location: Vec::new(),
            }),
            Action::Emote {
                string: token("ID_EmoteAFKCmd"),
                table: 0x2500_0042,
            },
        ]
    );
    s.social.moving = true;
    assert_eq!(
        h.frame(&s).acts,
        [
            social(SocialAction::SetState {
                mode: friend_mode::AVAILABLE,
                location: Vec::new(),
            }),
            Action::Emote {
                string: token("ID_EmoteYawnCmd"),
                table: 0x2500_0042,
            },
        ],
        "moving while away: available again"
    );
    assert!(h.frame(&s).acts.is_empty(), "once");
    s.social.moving = false;
    s.social.friend_added = true;
    assert_eq!(
        h.frame(&s).acts,
        [social(SocialAction::GetFriends)],
        "a friend added: the friends asked for again"
    );
}

/// The words a reference is written as: its literal.
fn write(t: &TextRef) -> Option<String> {
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &write,
        unknown_command: "",
    });
}

fn member(id: u64, name: &str) -> (n::Member, bool) {
    (
        n::Member {
            id,
            name: TextRef::literal(name),
            level: 12,
            ..n::Member::default()
        },
        false,
    )
}

#[test]
fn the_social_state_follows_the_views_social_part_with_its_names_written() {
    let fellow = |name: &str, health| n::Fellow {
        name: TextRef::literal(name),
        level: 7,
        health,
        max_health: 50,
        ..n::Fellow::default()
    };
    let mut view = View {
        player_id: Some(PLAYER),
        player: Some(Player {
            id: PLAYER,
            body: Body {
                moving: true,
                ..Body::default()
            },
            ..Player::default()
        }),
        social: ViewSocial {
            fellowship: Some(n::Fellowship {
                fellows: vec![(PLAYER, fellow("Me", 40)), (CARA, fellow("Cara", 25))],
                name: "The Hunt".into(),
                leader: CARA,
                social: 1,
                looting: 2,
            }),
            allegiance: Some(n::Allegiance {
                monarch: (n::Member::default(), false),
                patron: member(CARA, "Cara"),
                member: member(PLAYER, "Me"),
                vassals: vec![member(DOV, "Dov")],
                total: 3,
                ..n::Allegiance::default()
            }),
            friends: vec![FriendState {
                name: "Eli".into(),
                location: String::new(),
                mode: friend_mode::AVAILABLE,
            }],
            own_state: Some(friend_mode::BUSY),
            ..ViewSocial::default()
        },
        ..View::default()
    };
    let mut s = GameState::default();
    follow(&mut s, &view, &[]);
    let party = s.social.party.clone().expect("a fellowship");
    assert_eq!(party.leader, CARA);
    assert!(!party.shares_experience, "a social fellowship shares none");
    assert_eq!(party.looting, 2);
    let names: Vec<(&str, u64)> = party
        .fellows
        .iter()
        .map(|f| (f.name.as_str(), f.health.current))
        .collect();
    assert_eq!(names, [("Me", 40), ("Cara", 25)]);
    let a = s.social.allegiance.clone().expect("a profile");
    assert_eq!(
        a.monarch.map(|m| m.name),
        Some("Cara".into()),
        "no monarch named: the patron is the monarch"
    );
    assert_eq!(a.vassals.len(), 1);
    assert_eq!(s.social.friends, [friend("Eli", friend_mode::AVAILABLE)]);
    assert_eq!(s.social.own_state, Some(friend_mode::BUSY));
    assert!(s.social.moving);

    let tell = Notice {
        about: None,
        said: Said::Chat(ChatNotice::StartTell {
            to: Some(TextRef::literal("Cara")),
        }),
    };
    let added = Notice {
        about: None,
        said: Said::Friends(FriendsNotice::Added { name: None }),
    };
    view.social.fellowship = None;
    follow(&mut s, &view, &[tell, added]);
    assert!(s.social.party.is_none(), "left the fellowship");
    assert_eq!(s.social.start_tell.as_deref(), Some("Cara"));
    assert!(s.social.friend_added);
    let mut h = Harness::new();
    s.in_world = true;
    h.frame(&s);
    assert!(h.ui.typing(), "the chat entry opened for the tell");
    assert_eq!(h.ui.chat.text(), "/tell Cara, ");
    follow(&mut s, &view, &[]);
    assert_eq!(s.social.start_tell, None, "for one frame");
}

#[test]
fn a_fellows_level_is_shown_once_the_game_has_told_it() {
    let mut s = in_party();
    if let Some(p) = &mut s.social.party {
        // The player's own level not told yet.
        p.fellows[1].level = 0;
    }
    let mut h = Harness::with_built_in_art();
    opened(&mut h, &s, 0);
    let k = h.k();
    let glyphs_right_of = |h: &Harness, i: usize| {
        let row = fellow_rect(page(h).page, k, i);
        let misc =
            auberean_horizon::draw::Rect::new(row.right() - 110.0 * k, row.y, 110.0 * k, 20.0 * k);
        h.list
            .quads
            .iter()
            .filter(|q| q.tex.is_some() && misc.contains(q.dst.x, q.dst.y + q.dst.h / 2.0))
            .count()
    };
    // The rows: the player first, then Cara and Dov.
    assert_eq!(glyphs_right_of(&h, 0), 0, "no level while it is not known");
    assert!(glyphs_right_of(&h, 1) > 0, "Cara's level 20");
}

#[test]
fn the_fellowships_settings_line_leaves_a_whole_space_between_its_words() {
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        let style = auberean_horizon::ui::social::settings_style(
            &dereth_ui_kit::colours::Colours::default(),
        );
        let gap = crate::measured(&h, |p| {
            p.measure(&style, "not shared") - p.measure(&style, "not") - p.measure(&style, "shared")
        });
        assert!(gap >= 4.0, "{screen:?}: {gap} pixels");
    }
}

/// A fellowship, an allegiance and friends whose names and words run long: the most each page
/// shows on one line.
fn long_social() -> GameState {
    let mut s = in_party();
    if let Some(p) = &mut s.social.party {
        p.name = "The Long Hunt of the Northern Reaches".into();
        p.shares_experience = false;
        p.looting = 3;
        for (f, name) in p.fellows.iter_mut().zip([
            "Bartholomew Longshanks",
            "Wilhelmina Thistledown",
            "Maximilian Featherstone",
        ]) {
            f.name = name.into();
            f.level = 150;
            f.share = Some(0.333);
        }
    }
    let long = |id, name: &str| Rank {
        title: "High Chancellor".into(),
        level: 150,
        ..rank(id, name)
    };
    s.social.allegiance = Some(Allegiance {
        name: "The Order of the Ever-Watchful Eye".into(),
        total: 1234,
        monarch: Some(long(CARA, "Wilhelmina Thistledown")),
        member: Some(long(PLAYER, "Bartholomew Longshanks")),
        patron: Some(long(CARA, "Wilhelmina Thistledown")),
        vassals: (0..3)
            .map(|i| Rank {
                experience: 123_456_789,
                ..long(DOV + i, "Maximilian Featherstone")
            })
            .collect(),
        message: "Meet at the gates of Cragstone at dusk for the raid on the drudge citadel, \
                  and bring every healing kit you can carry, for the road is long."
            .into(),
        ..Allegiance::default()
    });
    s.social.friends = vec![
        friend("Maximilian Featherstone", friend_mode::AWAY),
        friend("Wilhelmina Thistledown", friend_mode::BUSY),
        friend("Bartholomew Longshanks", 0),
    ];
    s
}

/// The pictures the interface's letters are drawn from, in every face and size the pages use.
fn letter_textures(h: &Harness) -> Vec<dereth_ui_kit::art::TexId> {
    let art = std::sync::Arc::clone(h.ui.art());
    let mut list = auberean_horizon::draw::DrawList::default();
    let mut p = dereth_ui_kit::paint::Painter {
        list: &mut list,
        art: &art,
        scale: h.ui.layout_scale(h.screen),
        screen: h.screen,
        fade: 1.0,
    };
    for family in [
        dereth_ui_kit::art::Family::Body,
        dereth_ui_kit::art::Family::Heading,
        dereth_ui_kit::art::Family::Numerals,
    ] {
        for size in [12.0, 13.0, 14.0, 15.0] {
            let style = dereth_ui_kit::paint::TextStyle::new(family, size, 0xFFFF_FFFF);
            p.text(&style, 0.0, 0.0, "Aa1");
        }
    }
    let mut out: Vec<_> = list.quads.iter().filter_map(|q| q.tex).collect();
    out.dedup();
    out
}

/// The social window opens 352 units wide, four fifths of what it was, and still holds its
/// contents on every screen and scale step: on each page (and the question before a break), with
/// long names and words, nothing drawn in the body runs out of it, a fellow's gauges stand apart
/// within its row, and each button's label is whole at the button's own size.
#[test]
fn the_social_window_opens_narrow_and_each_page_holds_its_contents() {
    let s = long_social();
    let labels = [
        "Recruit", "Dismiss", "Quit", "Disband", "Create", "Home", "Up", "Swear", "Break", "Yes",
        "No", "Add", "Remove", "Tell",
    ];
    for (screen, step) in crate::play::fit::screens() {
        for page_shown in [0, ALLEGIANCE, ALLEGIANCE + 10, FRIENDS] {
            let mut h = crate::play::fit::on(screen, step);
            opened(&mut h, &s, page_shown % 10);
            h.ui.social.shown = PLAYER;
            if page_shown > FRIENDS {
                h.ui.social.breaking = Some(auberean_horizon::ui::social::Breaking {
                    id: CARA,
                    name: "Wilhelmina Thistledown".into(),
                });
            }
            h.frame(&s);
            let k = h.ui.layout_scale(screen);
            let body = h.ui.social.body().expect("open");
            let window = h.ui.social.rect().expect("open");
            let at = format!("{screen:?} {step:?} page {page_shown}");
            assert!(
                (body.w - 352.0 * k).abs() <= 1.0,
                "{at}: {} wide",
                body.w / k
            );
            let range =
                h.ui.stack
                    .drawn
                    .iter()
                    .find(|(w, _)| *w == auberean_horizon::ui::stack::Window::Social)
                    .map(|(_, r)| r.clone())
                    .expect("drawn");
            let letters = letter_textures(&h);
            let inside = |x: f32, y: f32| {
                x >= body.x && x < body.right() - 1.0 && y >= body.y && y < body.bottom() - 1.0
            };
            for q in &h.list.quads[range] {
                assert!(
                    q.dst.x >= window.x - 0.5
                        && q.dst.right() <= window.right() + 0.5
                        && q.dst.bottom() <= window.bottom() + 0.5,
                    "{at}: {:?} outside the window {window:?}",
                    q.dst
                );
                // The words and plain fills the pages draw, not the frame's pieces, which reach
                // into the body; the buttons and gauges are measured below.
                if inside(q.dst.x, q.dst.y) && q.tex.is_none_or(|t| letters.contains(&t)) {
                    assert!(
                        q.dst.right() <= body.right() + 2.0,
                        "{at}: {:?} runs out of the body {body:?}",
                        q.dst
                    );
                }
            }
            let parts = parts(body, k);
            for (i, b) in parts.foot.iter().enumerate() {
                assert!(
                    b.x >= body.x && b.right() <= body.right(),
                    "{at}: button {i}"
                );
                if let Some(next) = parts.foot.get(i + 1) {
                    assert!(b.right() < next.x, "{at}: button {i} apart from the next");
                }
            }
            let room = parts.foot[0].w - 12.0 * k;
            for label in labels {
                let wide =
                    crate::measured(&h, |p| dereth_ui_kit::widgets::button_label_width(p, label));
                assert!(wide <= room, "{at}: {label} {wide} in {room}");
            }
            let row = fellow_rect(parts.page, k, 0);
            let (health, vigor) = auberean_horizon::ui::social::fellow_gauges(row, k);
            assert!(
                health.x >= row.x && health.right() < vigor.x && vigor.right() <= row.right(),
                "{at}: {health:?} and {vigor:?} in {row:?}"
            );
        }
    }
}
