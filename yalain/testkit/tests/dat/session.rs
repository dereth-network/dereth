//! A whole session of our client on the server, both in this process: every step an action of
//! the client's, every check the client's model (and, where the claim is the server's, the
//! server's state). The clock is the harness's: nothing waits on real time.

use yalain_testkit::authored;
use yalain_testkit::session::{self as play, sent, Drive, InProcess};

fn ok<T>(r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// Our client and our server, together in one process with the September 2013 data, play a whole
/// session: log in to the list, make a character and enter the world with it, walk, see the
/// people and creatures around, select and examine a creature, attack it until it dies, open its
/// corpse and take what it held, say something, hear the guard's line when it uses him, take the
/// hall's shard to the town, log off and enter again where the character left. The world is a
/// world database, compiled against the install as the server compiles any. After each step the
/// client's model holds what the step should leave in it.
/// Behaviour: session.server.our-client-plays-a-whole-session-on-our-server
#[test]
fn our_client_plays_a_whole_session_on_our_server() {
    let mut s = InProcess::new();

    ok(play::character_list(&mut s));
    assert!(s
        .client_ref()
        .model
        .characters
        .as_ref()
        .unwrap()
        .characters
        .is_empty());
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    assert_eq!(s.client_ref().body.cell, authored::HALL, "at the start");

    ok(play::see(
        &mut s,
        &[authored::GUARD, authored::SHARD, authored::SENTINEL],
    ));
    // A weapon in hand (a human has no attack with an empty hand), and nothing selected again.
    ok(play::wield(&mut s, authored::SWORD).map(|_| ()));
    s.client()
        .act(auberean_client_sdk::Action::Select { target: 0 });
    s.client().act(auberean_client_sdk::Action::Examination {
        tabs: Some(play::TABS),
    });

    // Walking: reported as it goes, and the server has the player where the client put it.
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let at = sentinel.place.expect("placed").origin;
    let reports = ok(play::walk_to(&mut s, [at.x, at.y, at.z], 1.5));
    assert!(reports >= 2, "the walk is reported ({reports} reports)");
    let body = s.client_ref().body.origin;
    let server =
        s.h.server
            .players
            .values()
            .next()
            .expect("the player")
            .position;
    assert!(
        (server.origin[0] - body.x).powi(2) + (server.origin[1] - body.y).powi(2) < 0.25,
        "the server has the player where the client put it: {:?} against {body:?}",
        server.origin
    );

    // A click on the sentinel: the selection, its examination, the look at it, in that order.
    let order = ok(play::select_and_examine(&mut s, sentinel.id));
    assert_eq!(
        order,
        [sent::SET_SELECTION, sent::QUERY_EXAMINATION, sent::LOOK_AT]
    );

    ok(play::attack_until_dead(&mut s, sentinel.id));
    let (_, item) = ok(play::open_corpse(&mut s));
    ok(play::take(&mut s, item));
    ok(play::say(&mut s, "Well met."));

    // The guard, used, says the line the world database gives him.
    let guard = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    let at = guard.place.expect("placed").origin;
    ok(play::walk_to(&mut s, [at.x, at.y, at.z], 2.0));
    ok(play::talk_to(&mut s, guard.id));

    // The shard to the town.
    let shard = play::find(s.client_ref(), authored::SHARD).expect("the shard");
    ok(play::walk_to(&mut s, authored::SHARD_AT, 2.0));
    ok(play::take_portal(&mut s, shard.id, authored::TOWN));
    let town = s
        .client_ref()
        .model
        .player_object()
        .cloned()
        .expect("the player");
    assert!(
        play::ground_distance(authored::TOWN_AT, &town).is_some_and(|d| d < 1.0),
        "where the shard leads: {:?}",
        town.place
    );
    play::wait(&mut s, 1.0);

    // Out, and in again where the character left.
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    let again = s
        .client_ref()
        .model
        .player_object()
        .cloned()
        .expect("the player");
    assert_eq!(s.client_ref().body.cell, authored::TOWN, "where it left");
    assert!(
        play::ground_distance(authored::TOWN_AT, &again).is_some_and(|d| d < 1.0),
        "the place it left: {:?}",
        again.place
    );
}

/// Our client wields its starting sword, logs off and enters again: in its model the sword still
/// hangs from the player and the player wields it, and the server keeps it wielded.
/// Behaviour: session.server.our-client-enters-with-the-weapon-it-left-wielded
#[test]
fn our_client_enters_again_with_the_weapon_it_left_wielded() {
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    let sword = ok(play::wield(&mut s, authored::SWORD));
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    ok(play::until(&mut s, play::HANG, "the sword made", |c| {
        c.model.object(sword).is_some()
    }));
    let o = s
        .client_ref()
        .model
        .object(sword)
        .cloned()
        .expect("the sword");
    assert_eq!(o.parent.map(|(p, _)| p), Some(me), "in the player's hand");
    assert_eq!(o.wielder, Some(me), "wielded by the player");
    let held =
        s.h.server
            .players
            .values()
            .next()
            .and_then(|p| p.play.items.iter().find(|i| i.id == sword).copied())
            .expect("held");
    assert_ne!(held.location, 0, "the server keeps it wielded");
}

/// Our client keeps a panel's place as the play screen keeps one (through the player's interface
/// into its options, then sent to the server to keep), logs off and enters again: its scripts
/// answer the place it kept. A new character keeps none.
/// Behaviour: session.server.our-clients-kept-window-places-come-back-on-the-next-entry
#[test]
fn our_clients_kept_window_places_come_back_on_the_next_entry() {
    use auberean_client_sdk::protocol::ClientMessage;
    use auberean_script::ui_locations::UiLocations;
    const WINDOW_SETTINGS: u32 = 0x8212_002F;
    const RADAR: (u32, u32) = (0x2100_0050, 0x1000_0207);
    fn kept(s: &mut InProcess) -> UiLocations {
        for _ in 0..20_000 {
            if let Some(t) = s.client().ui_save_locations() {
                return UiLocations::read(&t).expect("a table that reads");
            }
            s.step();
        }
        panic!("the scripts never answered the kept places");
    }
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    assert_eq!(
        kept(&mut s).location(RADAR.0, RADAR.1),
        None,
        "a new character keeps none"
    );
    let place = [
        0.5f32.to_bits(),
        0.75f32.to_bits(),
        0.9f32.to_bits(),
        0.25f32.to_bits(),
        1,
    ];
    let mut t = UiLocations::new();
    assert!(t.add_location(RADAR.0, RADAR.1, place));
    let from = s.client_ref().sent().len();
    s.client().set_ui_save_locations(t.write());
    s.client().upload_ui_settings();
    ok(play::until(&mut s, play::HANG, "the places sent", |c| {
        play::sent_since(c, from)
            .iter()
            .any(|m| matches!(m, ClientMessage::ServerCall(c) if c.function.0 == WINDOW_SETTINGS))
    }));
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    assert_eq!(kept(&mut s).location(RADAR.0, RADAR.1), Some(place));
}

/// A character that logged off with a quest under way enters again: the whole-list notice the
/// scripts send the journal at the session's start carries the quest, as the journal asks for it.
/// Behaviour: session.server.a-character-entering-again-gives-the-journal-its-quests
#[test]
fn a_character_entering_again_gives_the_journal_its_quests() {
    use auberean_script::native_object::NativeObject;
    use yalain_content::quests::QuestDef;
    use yalain_content::QuestTable;
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x4000_0088;
    const UNDER_WAY: u32 = 0x1000_0000;
    /// The journal's notice, and how it says the whole list is to be read.
    const UPDATE_QUESTS: u32 = 0x008E << 16 | 0x0001;
    const WHOLE_LIST: u32 = 0x4000_0002;
    let mut s = InProcess::new();
    s.h.server.quests = QuestTable {
        quests: vec![QuestDef {
            id: QUEST,
            name: (0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(1, 0x2500_17EE)],
            challenge: -999,
            duration: 0.0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
        }],
        steps: Vec::new(),
    };
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    let p = s.h.server.players.values_mut().next().expect("the player");
    p.play.quests.push(QuestState {
        quest: QUEST,
        status: UNDER_WAY,
        phase: 1,
        started: 1,
        updated: 1,
    });
    ok(play::log_off(&mut s));
    s.client().notices.clear();
    ok(play::enter(&mut s, me));
    let whole = s
        .client_ref()
        .notices
        .iter()
        .find(|n| n.native == UPDATE_QUESTS && n.words.first() == Some(&WHOLE_LIST))
        .expect("the session's start tells the journal to read its list");
    let listed: Vec<u32> = whole
        .objects
        .iter()
        .flatten()
        .filter_map(|o| match o {
            NativeObject::QuestList(l) => Some(l.iter().map(|q| q.id)),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(
        listed,
        [QUEST],
        "the notice carries the quest the player holds"
    );
}
