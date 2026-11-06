//! The social window on the retail layouts: the fellowship page and the fellows' rows, the small
//! panel, the allegiance page, the friends page.

use auberean_text::StringInfo;
use auberean_ui::token::attr;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::gameplay::{dialog, GamePlay, Notice, Record, Request, View};
use auberean_ui_game::map::Navigation;
use auberean_ui_game::social::{
    fellows, notice, Fellow, Fellowship, Member, Profile, Request as Social, SocialWindow,
    UserStatus,
};
use auberean_ui_game::toolbar::Toolbar;
use auberean_ui_game::world::{Thing, World};

use crate::common::records;

/// The player, another player, a third.
const ME: u64 = 0x2000_0000_0000_0001;
const OTHER: u64 = 0x2000_0000_0000_0002;
const THIRD: u64 = 0x2000_0000_0000_0003;

/// An instance id's high and low words, as a notice carries one.
#[allow(clippy::cast_possible_truncation)]
const fn hi(id: u64) -> u32 {
    (id >> 32) as u32
}

#[allow(clippy::cast_possible_truncation)]
const fn lo(id: u64) -> u32 {
    id as u32
}

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

fn thing(id: u64, name: &str) -> Thing {
    Thing {
        id,
        is_player: id == ME,
        kind: 0x1F,
        name: name.to_owned(),
        ..Thing::default()
    }
}

/// The world with the player and two other players in it, `selection` selected.
fn world(selection: u64) -> World {
    World {
        player: ME,
        cell: 0x7F7F_0021,
        blocks: (255, 255),
        things: vec![
            thing(ME, "Me"),
            thing(OTHER, "Other"),
            thing(THIRD, "Third"),
        ],
        selection,
        ..World::default()
    }
}

/// One frame with the world and the notices given.
fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, w: &World, notices: &[Notice]) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    let view = View {
        world: Some(w),
        options: Some(auberean_ui_game::options::Values::default()),
        ..View::default()
    };
    p.tick(ui, &view, notices, &mut out);
    out
}

fn el(ui: &Ui, name: &str) -> Handle {
    ui.element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"))
}

fn child(ui: &Ui, under: Handle, name: &str) -> Handle {
    ui.child_recursive(under, Token::of(name))
        .unwrap_or_else(|| panic!("{name} under its parent"))
}

/// Point at the middle of an element and click it, the clock moving between.
fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, w: &World, h: Handle) -> Vec<Request> {
    let r = ui.screen_rect(h).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = tick(ui, p, t, w, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(ui, p, t, w, &[]));
    }
    out
}

/// The texts an element's own children show (not its popup's rows).
fn texts_under(ui: &Ui, h: Handle) -> Vec<String> {
    ui.node(h)
        .map(|n| n.children.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| ui.text(c))
        .collect()
}

fn ghosted(ui: &Ui, h: Handle) -> bool {
    ui.node(h).is_some_and(|n| n.bool_attr(attr::GHOSTED))
}

fn social(out: &[Request]) -> Vec<Social> {
    out.iter()
        .filter_map(|r| match r {
            Request::Social(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn dialogs(out: &[Request]) -> Vec<(u32, String)> {
    out.iter()
        .filter_map(|r| match r {
            Request::Dialog(d) if d.kind == dialog::CONFIRMATION => Some((d.id, d.text.clone())),
            _ => None,
        })
        .collect()
}

fn answer(id: u32, yes: bool) -> Notice {
    Notice {
        native: notice::CLOSE_CONFIRMATION,
        words: vec![id, u32::from(yes)],
        ..Notice::default()
    }
}

fn window(p: &GamePlay) -> &SocialWindow {
    p.child::<Toolbar>()
        .and_then(|t| t.window("SocialField"))
        .and_then(|c| c.child.as_any().downcast_ref::<SocialWindow>())
        .expect("the social window")
}

/// The friends' tab: the social window's tab furthest right.
fn friends_tab(ui: &Ui) -> Handle {
    let a = el(ui, "AllegianceTab");
    let parent = ui.node(a).and_then(|n| n.parent).expect("the tabs' panel");
    ui.node(parent)
        .map(|n| n.children.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| {
            ui.node(*c)
                .is_some_and(|n| n.kind == Token::of("panel_page"))
        })
        .max_by_key(|c| ui.screen_rect(*c).map_or(0, |r| r.x0))
        .expect("the friends' tab")
}

/// The social window open (on its first page, the fellowship's).
fn open(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, w: &World) {
    tick(ui, p, t, w, &[]);
    let b = el(ui, "SocialButton");
    click(ui, p, t, w, b);
    assert!(
        ui.shown(el(ui, "SocialField")),
        "the button shows the window"
    );
}

fn fellow(name: &str, level: u32, health: u32, cell: u32) -> Fellow {
    Fellow {
        name: StringInfo::literal(name),
        level,
        health,
        max_health: 100,
        vigor: 50,
        max_vigor: 80,
        cell,
        ..Fellow::default()
    }
}

/// A fellowship of three led by `leader`.
fn fellowship(leader: u64) -> Notice {
    let f = Fellowship {
        fellows: vec![
            (ME, fellow("Me", 10, 90, 0x7F7F_0021)),
            (OTHER, fellow("Other", 12, 40, 0x7A7F_0021)),
            (THIRD, fellow("Third", 9, 100, 0x7F7A_0021)),
        ],
        name: "The Band".to_owned(),
        leader,
        social: 0,
        looting: 2,
    };
    Notice {
        native: notice::UPDATE_FELLOWSHIP,
        about: Some(ME),
        words: vec![1],
        records: vec![Some(Record::Fellowship(Box::new(f)))],
    }
}

/// A list's rows' names, in its order.
fn row_names(ui: &Ui, list: Handle) -> Vec<String> {
    ui.list_items(list)
        .into_iter()
        .map(|r| {
            let n = child(ui, r, "FellowTemplateNameText");
            ui.text(n).unwrap_or_default()
        })
        .collect()
}

/// With no fellowship the page offers to make one: the looting menu holds the five ways with the
/// second chosen; Create asks for a fellowship with the name entered, the experience setting as
/// the experience button is not lit, and the way chosen.
///
/// Behaviour: ui.social.the-fellowship-page-asks-for-a-new-fellowship
#[test]
fn the_fellowship_page_asks_for_a_new_fellowship() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    open(&mut ui, &mut p, &mut t, &w);
    let page = el(&ui, "FellowshipPage");
    assert_eq!(
        ui.node(page).map(|n| n.state),
        Some(Token::of("nofellowship"))
    );
    let menu = child(&ui, page, "LootingMethodMenu");
    assert_eq!(ui.menu_len(menu), 5, "the five ways");
    assert_eq!(ui.menu_selected_index(menu), Some(1));
    assert!(
        texts_under(&ui, menu).iter().any(|t| t == "Shared"),
        "the menu shows its choice: {:?}",
        texts_under(&ui, menu)
    );
    let entry = child(&ui, page, "NoFellowshipNameEntryText");
    ui.set_text(entry, "Wanderers");
    let toggle = child(&ui, page, "NoFellowshipXPToggleButton");
    let lit = ui
        .node(toggle)
        .is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED));
    let create = child(&ui, page, "FellowshipCreateButton");
    let out = click(&mut ui, &mut p, &mut t, &w, create);
    assert_eq!(
        social(&out),
        [Social::CreateFellowship {
            name: "Wanderers".to_owned(),
            social: !lit,
            looting: 1,
        }]
    );
}

/// The fellowship described: the page in its fellowship state with the fellowship's name and a
/// row per fellow, the leader first and each named with its place; the leader's buttons ghosted
/// for a player who does not lead; the small panel gets its rows too; each fellow but the player
/// is put on the map.
///
/// Behaviour: ui.social.a-fellowship-lists-its-fellows-the-leader-first
#[test]
fn a_fellowship_lists_its_fellows_the_leader_first() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    open(&mut ui, &mut p, &mut t, &w);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[fellowship(OTHER)]);
    let page = el(&ui, "FellowshipPage");
    assert_eq!(
        ui.node(page).map(|n| n.state),
        Some(Token::of("fellowship"))
    );
    let name = child(&ui, page, "FellowshipNameText");
    assert_eq!(ui.text(name).as_deref(), Some("The Band"));
    let list = child(&ui, page, "FellowshipListBox");
    let page_rows = window(&p)
        .fellowship
        .as_ref()
        .and_then(|f| f.fellows.as_ref())
        .map_or(0, |f| f.rows().len());
    let names = row_names(&ui, list);
    assert_eq!(
        names.len(),
        3,
        "{names:?} (the page keeps {page_rows} rows)"
    );
    assert!(names[0].contains("Other"), "the leader first: {names:?}");
    assert!(names[0].contains('1'), "named with its place: {names:?}");
    for b in [
        "FellowshipDisbandButton",
        "FellowshipRecruitButton",
        "FellowshipDismissButton",
    ] {
        assert!(ghosted(&ui, child(&ui, page, b)), "{b} is the leader's");
    }
    let mini = el(&ui, "FellowMiniPanel");
    assert_eq!(
        ui.node(mini).map(|n| n.state),
        Some(Token::of("fellowship"))
    );
    let mini_list = child(&ui, mini, "FellowMiniPanelListBox");
    assert_eq!(ui.list_items(mini_list).len(), 3);
    let _ = out;
    let map = p
        .child::<Toolbar>()
        .and_then(|t| t.window("NavigationField"))
        .and_then(|c| c.child.as_any().downcast_ref::<Navigation>())
        .expect("the map");
    let marked: Vec<u64> = map
        .markers
        .iter()
        .filter(|(k, _)| *k == fellows::marker::FELLOW)
        .flat_map(|(_, l)| l.iter().map(|m| m.id))
        .collect();
    assert!(
        marked.contains(&OTHER) && marked.contains(&THIRD),
        "{marked:?}"
    );
    assert!(!marked.contains(&ME), "not the player");
    // A new leader: the player now leads.
    let lead = Notice {
        native: notice::UPDATE_LEADER,
        about: Some(ME),
        words: vec![hi(ME), lo(ME)],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[lead]);
    let names = row_names(&ui, list);
    assert!(names[0].contains("Me"), "{names:?}");
    assert!(!ghosted(&ui, child(&ui, page, "FellowshipDisbandButton")));
}

/// A fellow's bars follow its notices: its health, and its greatest; the fellows' keys select
/// them by place.
///
/// Behaviour: ui.social.a-fellows-row-follows-its-notices-and-keys-select-fellows
#[test]
fn a_fellows_row_follows_its_notices_and_keys_select_fellows() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    open(&mut ui, &mut p, &mut t, &w);
    tick(&mut ui, &mut p, &mut t, &w, &[fellowship(OTHER)]);
    let health = Notice {
        native: notice::UPDATE_FELLOW_HEALTH,
        about: Some(ME),
        words: vec![hi(THIRD), lo(THIRD), 25, 0],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[health]);
    let page = el(&ui, "FellowshipPage");
    let list = child(&ui, page, "FellowshipListBox");
    let row = ui
        .list_items(list)
        .into_iter()
        .find(|r| {
            ui.node(*r)
                .and_then(|n| n.instance_id_attr(Token::of("FellowIIDToken")))
                == Some(THIRD)
        })
        .expect("the third's row");
    let text = child(&ui, row, "FellowTemplateHealthText");
    let shown = ui.text(text).unwrap_or_default();
    assert!(shown.contains("25") && shown.contains("100"), "{shown}");
    // The first fellow's key selects the leader.
    ui.key(fellows::action::SELECT_FELLOW_1, true);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(fellows::action::SELECT_FELLOW_1, false);
    assert!(out.contains(&Request::SetSelection(OTHER)), "{out:?}");
}

/// Someone offering the player a place is asked about; yes takes it, no turns it down.
///
/// Behaviour: ui.social.a-place-offered-in-a-fellowship-is-asked-about
#[test]
fn a_place_offered_in_a_fellowship_is_asked_about() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let offer = Notice {
        native: notice::REQUEST_RECRUITMENT,
        about: Some(ME),
        words: vec![hi(OTHER), lo(OTHER), 7, 8],
        records: vec![
            None,
            None,
            Some(Record::Text(StringInfo::literal("Other"))),
            Some(Record::Text(StringInfo::literal("The Band"))),
        ],
    };
    let out = tick(&mut ui, &mut p, &mut t, &w, std::slice::from_ref(&offer));
    let asked = dialogs(&out);
    assert_eq!(asked.len(), 1, "{out:?}");
    assert!(
        asked[0].1.contains("Other") && asked[0].1.contains("The Band"),
        "{}",
        asked[0].1
    );
    let out = tick(&mut ui, &mut p, &mut t, &w, &[answer(asked[0].0, true)]);
    assert_eq!(social(&out), [Social::AcceptRecruitment]);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[offer]);
    let asked = dialogs(&out);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[answer(asked[0].0, false)]);
    assert_eq!(social(&out), [Social::DeclineRecruitment(0x138D)]);
}

fn member(id: u64, name: &str, level: u32) -> Member {
    Member {
        id,
        name: StringInfo::literal(name),
        title: StringInfo::literal("Sir"),
        level,
        species: 1,
        gender: 0x1000,
        xp_passed: 1234,
        ..Member::default()
    }
}

fn profile_notice(p: Profile) -> Notice {
    Notice {
        native: notice::UPDATE_ALLEGIANCE_PROFILE,
        about: Some(ME),
        words: vec![1],
        records: vec![Some(Record::Allegiance(Box::new(p)))],
    }
}

/// The allegiance page asks for the player's profile as it opens; a profile shows the
/// allegiance's name, its size, its monarch, the member, its patron, each vassal and the message
/// of the day; a vassal's Down asks for that vassal's profile; a profile with no member says the
/// player has no allegiance.
///
/// Behaviour: ui.social.the-allegiance-page-shows-a-member-its-patron-and-vassals
#[test]
fn the_allegiance_page_shows_a_member_its_patron_and_vassals() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    open(&mut ui, &mut p, &mut t, &w);
    let tab = el(&ui, "AllegianceTab");
    let out = click(&mut ui, &mut p, &mut t, &w, tab);
    assert!(
        social(&out).contains(&Social::QueryAllegiance(ME)),
        "{out:?}"
    );
    let page = el(&ui, "AllegiancePage");
    assert_eq!(ui.node(page).map(|n| n.state), Some(Token::of("pending")));
    let prof = Profile {
        monarch: (member(THIRD, "Third", 40), true),
        patron: (member(THIRD, "Third", 40), true),
        member: (member(ME, "Me", 10), true),
        vassals: vec![(member(OTHER, "Other", 5), false)],
        total: 3,
        motd: StringInfo::literal("Hunt at dawn"),
        name: StringInfo::literal("The Order"),
        ..Profile::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[profile_notice(prof)]);
    assert_eq!(
        ui.node(page).map(|n| n.state),
        Some(Token::of("allegiance"))
    );
    assert_eq!(
        ui.text(child(&ui, page, "AllegianceName")).as_deref(),
        Some("The Order")
    );
    assert!(ui
        .text(child(&ui, page, "MonarchName"))
        .unwrap_or_default()
        .contains("Third"));
    let list = child(&ui, page, "MemberListBox");
    let rows = ui.list_items(list);
    assert_eq!(rows.len(), 3, "the member, a vassal, the message");
    let member_text = ui
        .text(child(&ui, rows[0], "MemberNameText"))
        .unwrap_or_default();
    assert!(
        member_text.contains("Me") && member_text.contains("Sir"),
        "{member_text}"
    );
    let patron_text = ui
        .text(child(&ui, rows[0], "MemberMiscText"))
        .unwrap_or_default();
    assert!(patron_text.contains("Third"), "{patron_text}");
    let vassal_text = ui
        .text(child(&ui, rows[1], "MemberNameText"))
        .unwrap_or_default();
    assert!(vassal_text.contains("Other"), "{vassal_text}");
    assert!(ui
        .text(rows[2])
        .unwrap_or_default()
        .contains("Hunt at dawn"));
    assert!(
        ghosted(&ui, child(&ui, rows[0], "HomeButton")),
        "the player shown"
    );
    let down = child(&ui, rows[1], "DownButton");
    let out = click(&mut ui, &mut p, &mut t, &w, down);
    assert!(
        social(&out).contains(&Social::QueryAllegiance(OTHER)),
        "{out:?}"
    );
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &w,
        &[profile_notice(Profile::default())],
    );
    assert_eq!(
        ui.node(page).map(|n| n.state),
        Some(Token::of("noallegiance"))
    );
}

/// Breaking with the player's patron asks first; yes breaks and shows the player again. An oath
/// offered is asked about; yes takes it, no turns it down. Swear offers the player's oath to
/// another player selected.
///
/// Behaviour: ui.social.breaking-and-swearing-ask-first
#[test]
fn breaking_and_swearing_ask_first() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0);
    open(&mut ui, &mut p, &mut t, &w);
    let tab = el(&ui, "AllegianceTab");
    click(&mut ui, &mut p, &mut t, &w, tab);
    let mine = Profile {
        patron: (member(THIRD, "Third", 40), true),
        member: (member(ME, "Me", 10), true),
        ..Profile::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[profile_notice(mine)]);
    let page = el(&ui, "AllegiancePage");
    let breaks = child(&ui, page, "BreakButton");
    assert!(ghosted(&ui, breaks), "the player shown: nothing to break");
    let list = child(&ui, page, "MemberListBox");
    let up = child(&ui, ui.list_items(list)[0], "UpButton");
    let out = click(&mut ui, &mut p, &mut t, &w, up);
    assert!(social(&out).contains(&Social::QueryAllegiance(THIRD)));
    let theirs = Profile {
        member: (member(THIRD, "Third", 40), true),
        vassals: vec![(member(ME, "Me", 10), true)],
        ..Profile::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[profile_notice(theirs)]);
    let breaks = child(&ui, page, "BreakButton");
    assert!(!ghosted(&ui, breaks), "the patron shown: Break lit");
    let out = click(&mut ui, &mut p, &mut t, &w, breaks);
    let asked = dialogs(&out);
    assert_eq!(asked.len(), 1, "{out:?}");
    assert!(asked[0].1.contains("Third"), "{}", asked[0].1);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[answer(asked[0].0, true)]);
    assert_eq!(
        social(&out),
        [Social::Break(THIRD), Social::QueryAllegiance(ME)]
    );
    // An oath offered.
    let offer = Notice {
        native: notice::REQUEST_VASSAL,
        about: Some(ME),
        words: vec![hi(OTHER), lo(OTHER), 9],
        records: vec![None, None, Some(Record::Text(StringInfo::literal("Other")))],
    };
    let out = tick(&mut ui, &mut p, &mut t, &w, std::slice::from_ref(&offer));
    let asked = dialogs(&out);
    assert!(asked[0].1.contains("Other"), "{}", asked[0].1);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[answer(asked[0].0, false)]);
    assert_eq!(social(&out), [Social::RefuseOath(0x1585)]);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[offer]);
    let asked = dialogs(&out);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[answer(asked[0].0, true)]);
    assert_eq!(social(&out), [Social::TakeOath]);
    // Swear: no patron, another player selected.
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &w,
        &[profile_notice(Profile {
            member: (member(ME, "Me", 10), true),
            ..Profile::default()
        })],
    );
    let picked = Notice {
        native: notice::SELECTION_CHANGED,
        about: Some(ME),
        words: vec![hi(OTHER), lo(OTHER)],
        ..Notice::default()
    };
    let w = world(OTHER);
    tick(&mut ui, &mut p, &mut t, &w, &[picked]);
    let swear = child(&ui, page, "SwearButton");
    assert!(!ghosted(&ui, swear), "another player selected: Swear lit");
    let out = click(&mut ui, &mut p, &mut t, &w, swear);
    assert!(social(&out).contains(&Social::Swear), "{out:?}");
}

/// A player selected with the friends page showing lights Add at once, though the notice of the
/// new selection comes before the frame's view of it, and Add then asks for that player.
///
/// Behaviour: ui.social.the-friends-page-lists-friends-by-state
#[test]
fn selecting_a_player_with_the_friends_page_showing_lights_add() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let none = world(0);
    open(&mut ui, &mut p, &mut t, &none);
    let tab = friends_tab(&ui);
    click(&mut ui, &mut p, &mut t, &none, tab);
    let page = el(&ui, "FriendsPage");
    let add = child(&ui, page, "FriendsAddButton");
    assert!(ghosted(&ui, add), "no one selected");
    let picked = Notice {
        native: notice::SELECTION_CHANGED,
        about: Some(ME),
        words: vec![hi(OTHER), lo(OTHER)],
        ..Notice::default()
    };
    let w = world(OTHER);
    tick(&mut ui, &mut p, &mut t, &w, &[picked]);
    assert!(!ghosted(&ui, add), "another player selected");
    let out = click(&mut ui, &mut p, &mut t, &w, add);
    assert_eq!(social(&out), [Social::AddFriend("Other".to_owned())]);
}

/// The chat service's friends: those online (lit by their state) under one heading, those
/// offline under another; a friend's row selected lights Remove, which asks to take that friend
/// off; another player selected lights Add; the state menu sets the player's state with its
/// emote.
///
/// Behaviour: ui.social.the-friends-page-lists-friends-by-state
#[test]
fn the_friends_page_lists_friends_by_state() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(OTHER);
    open(&mut ui, &mut p, &mut t, &w);
    let all = vec![
        UserStatus {
            name: "Alpha".to_owned(),
            mode: 1,
            ..UserStatus::default()
        },
        UserStatus {
            name: "Beta".to_owned(),
            mode: 2,
            ..UserStatus::default()
        },
        UserStatus {
            name: "Gamma".to_owned(),
            mode: 0,
            ..UserStatus::default()
        },
    ];
    let list_notice = Notice {
        native: notice::GET_FRIENDS_LIST,
        about: Some(ME),
        words: vec![1],
        records: vec![Some(Record::Friends(all))],
    };
    tick(&mut ui, &mut p, &mut t, &w, &[list_notice]);
    let tab = friends_tab(&ui);
    click(&mut ui, &mut p, &mut t, &w, tab);
    let page = el(&ui, "FriendsPage");
    assert!(ui.shown(page), "its tab shows the page");
    let list = child(&ui, page, "FriendsListBox");
    let rows = ui.list_items(list);
    assert_eq!(rows.len(), 5, "two headings and three friends");
    assert_eq!(
        ui.node(rows[0]).map(|n| n.id),
        Some(Token::of("OnlineFriendsMarker"))
    );
    assert_eq!(
        ui.node(rows[2]).map(|n| n.state),
        Some(Token::of("awayfromkeyboard"))
    );
    assert_eq!(
        ui.node(rows[3]).map(|n| n.id),
        Some(Token::of("OfflineFriendsMarker"))
    );
    assert_eq!(
        ui.node(rows[4]).map(|n| n.state),
        Some(Token::of("offline"))
    );
    let add = child(&ui, page, "FriendsAddButton");
    let remove = child(&ui, page, "FriendsRemoveButton");
    let picked = Notice {
        native: notice::SELECTION_CHANGED,
        about: Some(ME),
        words: vec![hi(OTHER), lo(OTHER)],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[picked]);
    assert!(
        !ghosted(&ui, add) && ghosted(&ui, remove),
        "another player: Add"
    );
    let out = click(&mut ui, &mut p, &mut t, &w, add);
    assert_eq!(social(&out), [Social::AddFriend("Other".to_owned())]);
    ui.list_select_index(list, 1, true);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert!(!ghosted(&ui, remove), "a friend's row: Remove");
    let out = click(&mut ui, &mut p, &mut t, &w, remove);
    assert_eq!(social(&out), [Social::RemoveFriend("Alpha".to_owned())]);
    let menu = child(&ui, page, "StatusDropDownMenu");
    assert!(
        texts_under(&ui, menu).iter().any(|t| t == "Available"),
        "the state menu shows its choice: {:?}",
        texts_under(&ui, menu)
    );
    ui.menu_select_index(menu, 2);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[]);
    let asked = social(&out);
    assert_eq!(
        asked[0],
        Social::SetLocation {
            mode: 2,
            location: String::new()
        }
    );
    assert!(
        matches!(asked[1], Social::Emote { string, .. } if string == Token::of("ID_EmoteAFKCmd").raw())
    );
}
