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

/// Our client changes an option in play and logs off without sending it: it is gone at the next
/// entry. It changes it again and the play screen's end sends the options before the log off: its
/// scripts answer the option as changed at the next entry.
/// Behaviour: session.server.our-clients-options-sent-at-log-off-come-back-on-the-next-entry
#[test]
fn our_clients_options_sent_at_log_off_come_back_on_the_next_entry() {
    use auberean_client_sdk::contract::PlayerOption;
    use auberean_client_sdk::runtime::interface::function;
    use auberean_client_sdk::Action;
    const OPTION: PlayerOption = PlayerOption::ShortcutHeight;
    fn shown(s: &mut InProcess) -> u32 {
        for _ in 0..20_000 {
            if let Some(v) = s.client().option(OPTION).filter(|v| *v != 0) {
                return v;
            }
            s.step();
        }
        panic!("the scripts never answered the option");
    }
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    let first = shown(&mut s);
    let other = first + 1;

    s.client().set_option(OPTION, other);
    assert_eq!(shown(&mut s), other, "the scripts keep it for now");
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    assert_eq!(shown(&mut s), first, "not sent, not kept");

    s.client().set_option(OPTION, other);
    s.client().act(Action::KeepOptions);
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    assert_eq!(shown(&mut s), other, "sent, kept");

    // The radar's button: the change, then the options sent, in one step.
    let coordinates = |s: &mut InProcess| {
        s.client()
            .interface(function::GET_SHOW_RADAR_COORDS, &[])
            .map(|(_, v)| v)
    };
    let before = coordinates(&mut s).expect("answered");
    s.client()
        .act(Action::ShowRadarCoordinates { on: before == 0 });
    s.client().act(Action::KeepOptions);
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    shown(&mut s);
    assert_ne!(coordinates(&mut s), Some(before), "the change is kept");
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
            timed_phase: 0,
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
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        }],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
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

/// Moccasins looted from a corpse and used from the pack go on the feet, the weapon staying in the
/// hand. These moccasins' template keeps its places only in its qualities record, and an item
/// without places is wielded as a sword is.
///
/// Behaviour: session.server.a-looted-item-with-its-places-in-its-qualities-goes-where-they-say
#[test]
fn looted_moccasins_used_go_on_the_feet_and_the_weapon_stays_in_hand() {
    use yalain_testkit::authored::Portal;
    /// Initiate Moccasins.
    const MOCCASINS: u32 = 0x4700_0E0C;
    /// The inventory's notice of an item's change, how it says an item was put on, and the feet.
    const CHANGED: u32 = 0x008B << 16 | 0x0001;
    const PUT_ON: u32 = 0x4000_0002;
    const FEET: u32 = 0x10;
    let mut s = InProcess::new();
    // Tumerok's by their class, on a character of any race here.
    let mut moccasins = Portal::open().template(MOCCASINS);
    moccasins
        .props
        .set("Usage_RequiredRace", yalain_content::Prop::Word(0));
    s.h.server.catalog.insert(moccasins);
    for profile in &mut s.h.server.loot.profiles {
        profile.items = vec![(MOCCASINS, 1)];
    }
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    ok(play::see(&mut s, &[authored::SENTINEL]));
    let held = ok(play::wield(&mut s, authored::SWORD));
    s.client()
        .act(auberean_client_sdk::Action::Select { target: 0 });
    s.client().act(auberean_client_sdk::Action::Examination {
        tabs: Some(play::TABS),
    });
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let at = sentinel.place.expect("placed").origin;
    ok(play::walk_to(&mut s, [at.x, at.y, at.z], 1.5));
    ok(play::select_and_examine(&mut s, sentinel.id).map(|_| ()));
    ok(play::attack_until_dead(&mut s, sentinel.id));
    let (_, item) = ok(play::open_corpse(&mut s));
    ok(play::take(&mut s, item));
    s.client().notices.clear();
    play::use_object(&mut s, item);
    play::wait(&mut s, 4.0);
    let put_on: Vec<Vec<u32>> = s
        .client_ref()
        .notices
        .iter()
        .filter(|n| n.native == CHANGED && n.words.first() == Some(&PUT_ON))
        .map(|n| n.words.clone())
        .collect();
    let at = |w: &Vec<u32>| w.last().copied();
    assert!(
        put_on.len() == 1 && at(&put_on[0]) == Some(FEET),
        "on the feet: {put_on:x?}"
    );
    let weapon = s.client_ref().model.object(held).expect("the sword");
    assert!(weapon.wielder.is_some(), "the sword still in hand");
}

/// An item whose template names itself only in its qualities record is named and described in its
/// profile from there, in its own table.
///
/// Behaviour: session.server.a-looted-item-with-its-places-in-its-qualities-goes-where-they-say
#[test]
fn an_item_named_only_in_its_qualities_has_its_name_and_description() {
    use yalain_content::Prop;
    use yalain_testkit::authored::Portal;
    /// Initiate Moccasins, and the table holding their text.
    const MOCCASINS: u32 = 0x4700_0E0C;
    const TABLE: u32 = 0x2500_1907;
    let t = Portal::open().template(MOCCASINS);
    let line = yalain_world::profile::item_line::NAME;
    assert_eq!(t.props.get("Name"), Some(&Prop::Str(line, TABLE)));
    let line = yalain_world::profile::item_line::DESCRIPTION;
    assert_eq!(t.props.get("Description"), Some(&Prop::Str(line, TABLE)));
}

/// A player killed asks to be brought back, as death's dialog does, and lives: the player's scripts
/// hear it.
///
/// Behaviour: session.server.a-dead-player-comes-back-by-the-dialogs-ask
#[test]
fn a_dead_player_asks_and_comes_back() {
    use auberean_client_sdk::Action;
    const STARTING_WEAPON: u32 = 0x4700_051C;
    /// The player died (1) or lives again (0).
    const DEATH: u32 = 0x0085 << 16 | 0x0008;
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    ok(play::see(&mut s, &[authored::SENTINEL]));
    ok(play::wield(&mut s, STARTING_WEAPON).map(|_| ()));
    s.client().act(Action::Select { target: 0 });
    s.client().act(Action::Examination {
        tabs: Some(play::TABS),
    });
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let at = sentinel.place.expect("placed").origin;
    ok(play::walk_to(&mut s, [at.x, at.y, at.z], 1.5));
    ok(play::select_and_examine(&mut s, sentinel.id).map(|_| ()));
    // A sentinel that does not fall, and a player that does at its first blow.
    s.h.server
        .things
        .get_mut(sentinel.id)
        .expect("the sentinel")
        .health = 100_000;
    let player = s.h.server.players.values_mut().next().expect("the player");
    yalain_world::quals::set_int(
        &mut player.qualities,
        yalain_world::player::quality::HEALTH,
        1,
    );
    let fight = play::attack_until_dead(&mut s, sentinel.id);
    assert!(fight.is_err(), "the player falls first");
    // Dead, and its corpse on the ground: death's dialog waits on the player.
    ok(play::until(&mut s, 30.0, "told it died", |c| {
        c.notices
            .iter()
            .any(|n| n.native == DEATH && n.words.first() == Some(&1))
    }));
    play::wait(&mut s, 15.0);
    s.client().notices.clear();
    s.client().act(Action::Resurrect);
    play::wait(&mut s, 15.0);
    let back = s
        .client_ref()
        .notices
        .iter()
        .any(|n| n.native == DEATH && n.words.first() == Some(&0));
    assert!(back, "told it lives again");
}

/// A person hands over an item the client has not been told of yet: the move's answer comes first,
/// so the client's own line for it says "the item", not its name, as retail's does.
/// Behaviour: server.gives.a-gift-is-answered-before-its-object
#[test]
fn a_gift_from_a_person_is_called_the_item_by_the_clients_line() {
    use auberean_script::native_object::VariableValue;
    const RECEIVED: u32 = 0x4CF8_CDB5;
    const ITEM: u32 = 0xD0DC_9036;
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    ok(play::see(&mut s, &[authored::GUARD]));
    let guard = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    let world = *s.h.server.players.keys().next().expect("the player");
    yalain_world::inventory::give_from(
        &mut s.h.server,
        &mut s.h.net,
        world,
        authored::SWORD,
        1,
        guard.id,
    );
    play::wait(&mut s, 1.0);
    let lines = &s.client_ref().shown;
    let ids: Vec<(u32, u32)> = lines.iter().map(|l| (l.kind, l.text.string_id)).collect();
    let line = lines
        .iter()
        .find(|l| l.text.string_id == RECEIVED)
        .unwrap_or_else(|| panic!("its line for what it received: {ids:x?}"));
    let named: Vec<u32> = line
        .text
        .variables
        .iter()
        .filter_map(|v| match &v.value {
            VariableValue::Nested(t) => Some(t.string_id),
            _ => None,
        })
        .collect();
    assert!(named.contains(&ITEM), "{named:x?}");
}

/// Our client takes a sale's time running out as its scripts read it: told as it runs out, then on
/// its next entry reminded, it marks it expired and forgets its reminder. Its scripts stop nowhere on
/// the way.
/// Behaviour: server.market.a-sale-past-its-time-is-not-sold
#[test]
fn our_clients_scripts_read_a_sale_running_out_of_time() {
    use yalain_world::market::{self, Broker, Consignment};
    let mut s = InProcess::new();
    ok(play::character_list(&mut s));
    let me = ok(play::create(&mut s, "Aubrey"));
    ok(play::enter(&mut s, me));
    ok(play::see(&mut s, &[authored::GUARD]));
    let keeper = play::find(s.client_ref(), authored::GUARD).expect("the guard");
    // The guard keeps a sale for it, as a broker would.
    s.h.server.brokers.insert(
        authored::GUARD,
        Broker {
            catalog: 0x6F00_1503,
            location: (0x0D3C_90C4, 0x2500_0CE6),
        },
    );
    let item =
        yalain_world::inventory::make_item(&mut s.h.server, authored::SWORD, 1, 0).expect("made");
    assert!(market::ensure(&mut s.h.server));
    let now = s.h.server.game_now;
    s.h.server.market.list.push(Consignment {
        sale: 1,
        owner: me,
        keeper: keeper.id,
        catalog: 0x6F00_1503,
        store: 0x6F00_150B,
        item,
        template: authored::SWORD,
        price: 100.0,
        offered: 1,
        sold: 0,
        entered: now - market::ON_SALE_SECONDS + market::EXPIRY_LOOK / 2.0,
    });
    ok(play::log_off(&mut s));
    let stops = s.client_ref().script_stops().len();
    ok(play::enter(&mut s, me));
    play::wait(&mut s, 2.0);
    s.h.server.market.expiry_look = Some(s.h.server.game_now);
    let w = *s.h.server.players.keys().next().expect("in the world");
    let told = s.h.net.game_time(s.h.now);
    play::wait(&mut s, market::EXPIRY_LOOK + 2.0);
    assert!(s.h.server.market.list[0].expired(s.h.server.game_now));
    assert!(s.h.server.market.expiry_look.is_some_and(|t| t > told));
    assert!(s.h.server.players.contains_key(&w), "still in the world");
    // Its scripts forget a reminder a day past its removal, and say so when asked.
    ok(play::log_off(&mut s));
    ok(play::enter(&mut s, me));
    play::wait(&mut s, 2.0);
    s.client().act(auberean_client_sdk::Action::Say {
        text: "/consignments".into(),
    });
    play::wait(&mut s, 0.5);
    assert_eq!(
        s.client_ref().shown.last().map(|l| l.text.string_id),
        Some(0xD048_7DAD),
        "none left to remind of"
    );
    assert_eq!(
        s.client_ref().script_stops().len(),
        stops,
        "{:#?}",
        s.client_ref().script_stops()
    );
}
