//! The operator's characters: made with a build before they first play, given a build after
//! they have played, deleted (with the hour, or at once), placed out of the world, exported to
//! another world and imported, and teleported while in the world. Fixture: made-up levels and a
//! made-up skill, the test catalog's item, a made-up quest, new players in the world.

use std::collections::HashMap;

use auberean_protocol::login::Text;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::QuestDef;
use yalain_store::{Store as _, StoredPosition};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, test_template, Harness, TEST_ITEM};
use yalain_world::characters::{self, Destination, QuestAt, Recipe};
use yalain_world::inventory::GUIDE_BOOK;
use yalain_world::levels::{Level, LevelTable};
use yalain_world::player::quality;
use yalain_world::progress::{self, q, Award};
use yalain_world::quests::status;
use yalain_world::skills::{SkillDef, SkillTable};
use yalain_world::state::PlayState;
use yalain_world::{admin, places, quals};

/// A made-up skill to train from level 2 for one credit, raised along a table like the masters'.
const SKILL: u32 = 15;
const TABLE: u32 = 0x6F00_000E;
/// A made-up quest.
const QUEST: u32 = 0x8000_0F01;

fn skills() -> SkillTable {
    let mut skills = HashMap::new();
    skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: 1,
            min_level: 2,
            cap: 50,
            table: TABLE,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    let mut tables = HashMap::new();
    tables.insert(
        TABLE,
        (0..=50u64).map(|n| 2 * n * n.saturating_sub(1)).collect(),
    );
    SkillTable { skills, tables }
}

/// Made-up levels: 2 at 10 experience, 3 at 1000, each a credit and more health and vigor.
fn levels() -> LevelTable {
    let level = |xp, hv, skill_credits| Level {
        xp,
        health: hv,
        vigor: hv,
        skill_credits,
        hero_credits: 0,
    };
    LevelTable {
        levels: vec![level(0, 100, 0), level(10, 130, 1), level(1000, 160, 1)],
    }
}

fn quest() -> QuestDef {
    QuestDef {
        id: QUEST,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (1..=3).map(|n| (n, 0x2500_17EE)).collect(),
        challenge: 0,
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
    }
}

/// A world with the made-up levels, skill, item and quest.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.levels = levels();
    h.server.skill_table = skills();
    h.server.catalog = test_catalog();
    // The guide book a new character is given as it first enters.
    h.server.catalog.insert(test_template(
        GUIDE_BOOK,
        0x200,
        yalain_content::Props::default(),
    ));
    h.server.quests.quests.push(quest());
    h
}

/// A human man's recipe on account `one`.
fn recipe(name: &str) -> Recipe {
    Recipe {
        account: "one".into(),
        name: name.into(),
        race: 1,
        sex: 0x1000,
        ..Recipe::default()
    }
}

/// What the store keeps of a character: its qualities and its play.
fn kept(h: &mut Harness, id: u64) -> (Vec<u8>, PlayState) {
    let st = h
        .server
        .store
        .world_state(id)
        .expect("the store reads")
        .expect("kept");
    let play = PlayState::from_bytes(&st.play).expect("its play reads");
    (st.qualities, play)
}

fn id_of(h: &mut Harness, name: &str) -> u64 {
    h.server
        .store
        .character_named(name)
        .expect("the store reads")
        .expect("there")
        .id
}

fn world_of(h: &Harness, id: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(w, _)| *w)
        .expect("in the world")
}

/// A character made at a level, its skill trained and raised to a rating, holds what a player
/// awarded the same experience holds after training and raising it by its own requests: its
/// level, experience left and spent, health and vigor, credits and skill. Its money, items, quest,
/// hero and place are its build's, and it enters as a new character.
/// Behaviour: server.operator.characters-made-with-a-build
#[test]
fn a_character_made_with_a_build_holds_what_a_player_who_earned_it_holds() {
    let mut h = world();
    let (b, played) = h.player_in_world("one", "Elara");
    let w = world_of(&h, played);
    progress::award(&mut h.server, &mut h.net, w, 1000, Award::Step);
    h.run(5);
    let rating = |h: &Harness| {
        let p = h.server.players.get(&w).expect("in the world");
        let starting = h.server.start.skills.get(&1).cloned().unwrap_or_default();
        let held = yalain_world::skills::held(&starting, &p.play.skills);
        h.server.skill_table.rating(&held, SKILL, false)
    };
    for f in [0x8214_0002u32].into_iter().chain([0x8214_000C; 4]) {
        h.bots[b].send(true, queue::WEENIE, &server_call(f, &[Arg::Int(SKILL)]));
        h.run(10);
    }
    let reached = rating(&h);
    assert!(reached >= 3, "trained and raised by requests: {reached}");
    yalain_world::world::keep_player(&mut h.server, w);
    let (earned_q, earned_play) = kept(&mut h, played);

    let place = StoredPosition {
        cell: 0x0299_0105,
        origin: [12.0, 14.0, 0.5],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let r = Recipe {
        level: Some(3),
        money: Some(2500),
        hero: Some(true),
        skills: vec![(SKILL, reached)],
        items: vec![(TEST_ITEM, 5)],
        quests: vec![(QUEST, QuestAt::Done)],
        place: Some(place),
        account: "made".into(),
        ..recipe("Hand Of Doom")
    };
    let unix = h.unix;
    let said = characters::make(&mut h.server, &r, unix).expect("made");
    assert!(said.contains("Hand Of Doom"), "{said}");
    let made = id_of(&mut h, "Hand Of Doom");
    let (made_q, made_play) = kept(&mut h, made);
    for key in [
        q::LEVEL,
        quality::HEALTH_MAX,
        quality::VIGOR_MAX,
        quality::HEALTH,
        quality::VIGOR,
    ] {
        assert_eq!(
            quals::int(&made_q, key),
            quals::int(&earned_q, key),
            "int {key}"
        );
    }
    assert_eq!(quals::int(&made_q, q::LEVEL), Some(3));
    for key in [q::TOTAL_XP, q::AVAILABLE_XP] {
        assert_eq!(
            quals::long(&made_q, key),
            quals::long(&earned_q, key),
            "long {key}"
        );
    }
    assert_eq!(made_play.credits, earned_play.credits, "credits left");
    assert_eq!(
        made_play.skills, earned_play.skills,
        "the skill as trained and raised"
    );

    // It enters as a new character holding its build.
    let mb = h.at_character_list("made");
    h.enter_world(mb, "made", made);
    h.run(10);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == made)
        .expect("in the world");
    assert_eq!(p.instance, 1, "its first entry");
    assert!(
        p.play.items.iter().any(|i| i.template == GUIDE_BOOK),
        "given a new character's guide book: {:?}",
        p.play.items
    );
    assert_eq!(p.position.cell, place.cell, "at its place");
    assert_eq!(p.position.origin, place.origin);
    assert_eq!(yalain_world::store::money(&p.qualities), 2500);
    assert!(yalain_world::focus::is_hero(&p.qualities));
    assert!(
        p.play
            .items
            .iter()
            .any(|i| i.template == TEST_ITEM && i.quantity == 1 && i.location == 0),
        "its item in its pack: {:?}",
        p.play.items
    );
    assert!(p
        .play
        .quests
        .iter()
        .any(|x| x.quest == QUEST && x.status == status::COMPLETED));
}

/// A build that does not hold (a skill its level may not train), or one for a character that has
/// played, is refused and keeps nothing: no character made, the played one as it was.
/// A build's count of an item that does not stack is that many items, each of one, as a player
/// holds them; of an item that stacks, one stack of that count.
///
/// Behaviour: server.operator.characters-made-with-a-build
#[test]
fn a_build_gives_a_stack_only_of_what_stacks() {
    const STACKS: u32 = TEST_ITEM + 1;
    let mut h = world();
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(10));
    h.server.catalog.insert(test_template(STACKS, 0x200, props));
    let r = Recipe {
        items: vec![(TEST_ITEM, 2), (STACKS, 3)],
        account: "made".into(),
        ..recipe("Two Of Each")
    };
    let unix = h.unix;
    characters::make(&mut h.server, &r, unix).expect("made");
    let made = id_of(&mut h, "Two Of Each");
    let (_, play) = kept(&mut h, made);
    let of = |t: u32| -> Vec<u32> {
        play.items
            .iter()
            .filter(|i| i.template == t)
            .map(|i| i.quantity)
            .collect()
    };
    assert_eq!(of(TEST_ITEM), [1, 1], "each its own item");
    assert_eq!(of(STACKS), [3], "one stack");
}

/// Behaviour: server.operator.characters-made-with-a-build
#[test]
fn a_build_that_does_not_hold_or_a_played_character_is_refused_and_nothing_kept() {
    let mut h = world();
    let unix = h.unix;
    let low = Recipe {
        level: Some(1),
        skills: vec![(SKILL, 2)],
        ..recipe("Lowly")
    };
    let e = characters::make(&mut h.server, &low, unix).expect_err("refused");
    assert!(e.contains("skill 15"), "{e}");
    assert_eq!(
        h.server.store.character_named("Lowly").expect("reads"),
        None,
        "nothing made"
    );
    let missing = Recipe {
        items: vec![(0x4700_FFFF, 1)],
        ..recipe("Emptyhand")
    };
    assert!(characters::make(&mut h.server, &missing, unix).is_err());
    assert_eq!(
        h.server.store.character_named("Emptyhand").expect("reads"),
        None
    );

    let (_, played) = h.player_in_world("one", "Elara");
    let w = world_of(&h, played);
    yalain_world::world::leave(&mut h.server, &mut h.net, w);
    let before = h.server.store.world_state(played).expect("reads");
    let again = Recipe {
        level: Some(3),
        ..recipe("Elara")
    };
    let e = characters::make(&mut h.server, &again, unix).expect_err("refused");
    assert!(e.contains("has played"), "{e}");
    assert_eq!(h.server.store.world_state(played).expect("reads"), before);

    // Made before and never played, it is given its build anew.
    characters::make(&mut h.server, &recipe("Freshly"), unix).expect("made");
    let r = Recipe {
        level: Some(2),
        ..recipe("Freshly")
    };
    characters::make(&mut h.server, &r, unix).expect("given its build");
    let id = id_of(&mut h, "Freshly");
    assert_eq!(quals::int(&kept(&mut h, id).0, q::LEVEL), Some(2));
}

/// Deleting a character in the world boots its login and marks it deleted, as its player's
/// deletion does: grayed out in its account's list for the hour, then gone.
/// Behaviour: server.operator.characters-deleted-by-the-operator
#[test]
fn a_deleted_character_is_booted_and_grayed_out_for_the_hour_then_gone() {
    let mut h = world();
    let (b, id) = h.player_in_world("one", "Sir Kara");
    h.run(5);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let said = characters::delete(&mut h.server, &mut h.net, clock, "sir kara").expect("deleted");
    assert!(said.contains("booted"), "{said}");
    h.run_to(yalain_net::LocalTime(h.now.0 + 25.0));
    let booted = h.bots[b].inbox[from..].iter().any(|r| {
        matches!(r.message(), Some(ServerMessage::AccountBooted(Text(t)))
            if t.string_id == admin::BOOTED)
    });
    assert!(booted, "its login was booted");
    assert!(h.server.players.is_empty(), "it left the world");
    let c = h
        .server
        .store
        .character(id)
        .expect("reads")
        .expect("still kept");
    assert_eq!(c.deleted_at, Some(clock.unix), "deleted now");
    assert!(
        characters::delete(&mut h.server, &mut h.net, clock, "Sir Kara").is_err(),
        "once"
    );
    h.advance(3_601.0);
    h.at_character_list("one");
    assert_eq!(
        h.server.store.character(id).expect("reads"),
        None,
        "gone after the hour"
    );
}

/// A character file carries every number an item holds of its own, in its state: imported under a
/// new id, the item comes back with its value, what its level gave it, its effects, its maker, its
/// wear, its map's place, its color and its inscription. The build beside it names items by template and count only;
/// it isn't what the import reads.
/// Behaviour: server.operator.character-files-keep-items-own-numbers
#[test]
fn a_character_file_keeps_what_each_item_holds_of_its_own() {
    let mut a = world();
    let (_, id) = a.player_in_world("one", "Wanderer");
    let w = world_of(&a, id);
    let made = yalain_world::made::Made {
        prefix: yalain_world::made::prefix(20),
        value: 41,
        damage: Some((110, 0.2)),
        delay: Some(13),
        vigor: Some(5),
        slots: 2,
        harmonic: true,
        durability: Some(200),
        ..yalain_world::made::Made::default()
    };
    let place = StoredPosition {
        cell: 0x0123_0045,
        origin: [1.0, 2.0, 3.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let sword =
        yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 1).expect("given");
    {
        let t = a.server.things.get_mut(sword).expect("made");
        t.value = 41;
        t.own.made = Some(made);
        t.own.min_level = Some(20);
        t.own.arcane_lore = Some(9);
        t.own.dye = Some((0x2000_036A, 0.573));
        t.enchantments = vec![(0x6F00_0F2B, 20.0)];
        t.bonuses = vec![(0x6F00_0F2C, 3.0)];
        t.crafter = Some("Wanderer".to_owned());
        t.durability = Some(150);
        t.destination = Some(place);
        t.binds_on_use = true;
        t.inscription = Some(("Sharp".to_owned(), "Wanderer".to_owned()));
    }
    let treasure =
        yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 1).expect("given");
    a.server.things.get_mut(treasure).expect("made").value = 77;
    a.run(5);
    let file = characters::export(&mut a.server, "Wanderer").expect("exported");

    let mut b = world();
    let unix = b.unix;
    characters::import(&mut b.server, &file, "three", None, unix).expect("imported");
    let new = id_of(&mut b, "Wanderer");
    let bb = b.at_character_list("three");
    b.enter_world(bb, "three", new);
    b.run(10);
    let items: Vec<_> = b
        .server
        .players
        .values()
        .find(|p| p.character == new)
        .expect("in the world")
        .play
        .items
        .iter()
        .map(|h| h.id)
        .collect();
    let of = |value: u32| {
        items
            .iter()
            .filter_map(|i| b.server.things.get(*i))
            .find(|t| t.value == value)
            .unwrap_or_else(|| panic!("no item is worth {value}"))
    };
    let t = of(41);
    assert_eq!(t.own.made, Some(made));
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(t.own.arcane_lore, Some(9));
    assert_eq!(t.own.dye, Some((0x2000_036A, 0.573)));
    assert_eq!(t.enchantments, vec![(0x6F00_0F2B, 20.0)]);
    assert_eq!(t.bonuses, vec![(0x6F00_0F2C, 3.0)]);
    assert_eq!(t.crafter.as_deref(), Some("Wanderer"));
    assert_eq!(t.durability, Some(150));
    assert_eq!(t.destination, Some(place));
    assert!(t.binds_on_use);
    assert_eq!(
        t.inscription,
        Some(("Sharp".to_owned(), "Wanderer".to_owned()))
    );
    assert_eq!(of(77).own, yalain_world::treasure::Own::default());
}

/// A character brought from another world has a new id, and what was bound to it is bound to it
/// still, whoever else's bound item it holds stays another's.
/// Behaviour: server.operator.imported-items-stay-bound-to-their-character
#[test]
fn an_imported_characters_bound_items_stay_bound_to_it() {
    let mut a = world();
    let (_, id) = a.player_in_world("one", "Wanderer");
    let w = world_of(&a, id);
    let mine =
        yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 1).expect("given");
    let theirs =
        yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 1).expect("given");
    a.server.things.get_mut(mine).expect("made").own.bound_to = id;
    a.server.things.get_mut(theirs).expect("made").own.bound_to = id ^ 0xFF;
    a.run(5);
    let file = characters::export(&mut a.server, "Wanderer").expect("exported");

    let mut b = world();
    let (_, here) = b.player_in_world("two", "Elara");
    let unix = b.unix;
    characters::import(&mut b.server, &file, "three", None, unix).expect("imported");
    let new = id_of(&mut b, "Wanderer");
    assert_ne!(new, id);
    assert_ne!(new, here);
    let state = b
        .server
        .store
        .world_state(new)
        .expect("reads")
        .expect("kept");
    let play = PlayState::from_bytes(&state.play).expect("reads");
    let mut bound: Vec<u64> = play.item_states.iter().map(|k| k.own.bound_to).collect();
    bound.retain(|x| *x != 0);
    bound.sort_unstable();
    let mut wanted = vec![new, id ^ 0xFF];
    wanted.sort_unstable();
    assert_eq!(bound, wanted);
}

/// A file that holds one item id twice is refused whole: no character, no account, no item.
/// Behaviour: server.operator.a-file-with-an-item-twice-is-refused
#[test]
fn a_character_file_holding_one_item_id_twice_is_refused() {
    let mut a = world();
    let (_, id) = a.player_in_world("one", "Wanderer");
    let w = world_of(&a, id);
    yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 3);
    a.run(5);
    let mut file = characters::export(&mut a.server, "wanderer").expect("exported");
    let mut play = PlayState::from_bytes(&file.state.play).expect("reads");
    let again = *play
        .items
        .iter()
        .find(|x| x.template == TEST_ITEM)
        .expect("held");
    play.items.push(again);
    file.state.play = play.to_bytes();

    let mut b = world();
    let unix = b.unix;
    let said = characters::import(&mut b.server, &file, "three", None, unix);
    assert!(said.is_err(), "{said:?}");
    assert!(
        b.server.store.name_taken("Wanderer").is_ok_and(|t| !t),
        "no character kept"
    );
    assert!(
        b.server
            .store
            .account_by_name("three")
            .is_ok_and(|acc| acc.is_none()),
        "no account made"
    );
}

/// A character exported from one world and imported onto an account of another, under a new
/// name, is a new character there holding what it held: its level and items (with new ids), its
/// name shown as the new one. A name that is taken is refused and nothing is kept.
/// Behaviour: server.operator.characters-exported-and-imported
#[test]
fn a_character_exported_and_imported_holds_what_it_held_under_its_new_name() {
    let mut a = world();
    let (_, id) = a.player_in_world("one", "Wanderer");
    let w = world_of(&a, id);
    progress::award(&mut a.server, &mut a.net, w, 1000, Award::Step);
    yalain_world::inventory::give(&mut a.server, &mut a.net, w, TEST_ITEM, 3);
    a.run(5);
    let file = characters::export(&mut a.server, "wanderer").expect("exported");
    assert_eq!(file.character.name, "Wanderer");
    let old_items: Vec<u64> = PlayState::from_bytes(&file.state.play)
        .expect("reads")
        .items
        .iter()
        .map(|i| i.id)
        .collect();
    assert!(!old_items.is_empty(), "it held its item");

    let mut b = world();
    let (_, taken) = b.player_in_world("two", "Elara");
    // This world's own items: their ids are this world's, and may be the numbers the other
    // world gave its own.
    let tw = world_of(&b, taken);
    yalain_world::inventory::give(&mut b.server, &mut b.net, tw, TEST_ITEM, 1);
    b.run(5);
    let theirs: Vec<u64> = b
        .server
        .players
        .get(&tw)
        .map(|p| p.play.items.iter().map(|i| i.id).collect())
        .unwrap_or_default();
    assert!(!theirs.is_empty());
    let unix = b.unix;
    assert!(
        characters::import(&mut b.server, &file, "three", Some("Elara"), unix).is_err(),
        "a taken name"
    );
    let held = |b: &mut Harness| {
        let acc = b.server.store.account_by_name("three").expect("reads");
        acc.map_or(0, |acc| {
            b.server.store.characters(acc.id).expect("reads").len()
        })
    };
    assert_eq!(held(&mut b), 0, "nothing kept");
    let said = characters::import(&mut b.server, &file, "three", Some("Far Wanderer"), unix)
        .expect("imported");
    assert!(said.contains("Far Wanderer"), "{said}");
    assert_eq!(held(&mut b), 1);
    let new = id_of(&mut b, "Far Wanderer");
    assert_ne!(new, id);
    assert_ne!(new, taken);
    let (qualities, play) = kept(&mut b, new);
    assert_eq!(quals::int(&qualities, q::LEVEL), Some(3));
    assert_eq!(play.items.len(), old_items.len());
    assert!(
        play.items.iter().all(|i| !theirs.contains(&i.id)),
        "its items' ids are new to this world: {:?} beside {theirs:?}",
        play.items
    );
    let named = quals::read(&qualities).weenie.name;
    assert_eq!(
        named.literal.map(|l| l.text()),
        Some(yalain_world::player::world_name("Far Wanderer", 0x1000))
    );

    let bb = b.at_character_list("three");
    b.enter_world(bb, "three", new);
    b.run(10);
    let p = b
        .server
        .players
        .values()
        .find(|p| p.character == new)
        .expect("in the world");
    assert_eq!(p.name, "Far Wanderer");
    assert!(p
        .play
        .items
        .iter()
        .any(|i| i.template == TEST_ITEM && i.quantity == 3));
    assert!(p.instance > 1, "not a new character");
    assert_eq!(
        p.play
            .items
            .iter()
            .filter(|i| i.template == GUIDE_BOOK)
            .count(),
        1,
        "the guide book it brought, and no second one"
    );
}

/// A character in the world teleported at the console goes through portal space and comes out
/// beside the other character it was sent to, facing it; then at a cell and point; a place the
/// world does not name, or a character not in the world, is refused.
/// Behaviour: server.operator.teleport-at-the-console
#[test]
fn a_teleported_character_goes_through_portal_space_to_where_it_was_sent() {
    let mut h = world();
    let (a, me) = h.player_in_world("one", "Hand Of Doom");
    let (_, other) = h.player_in_world("two", "Sir Kara");
    h.run(5);
    let them = h
        .server
        .players
        .values()
        .find(|p| p.character == other)
        .map(|p| p.position)
        .expect("in the world");
    let from = h.bots[a].inbox.len();
    let clock = h.clock();
    characters::teleport(
        &mut h.server,
        &mut h.net,
        clock,
        "hand of doom",
        &Destination::Character("Sir Kara".into()),
    )
    .expect("sent");
    h.run(5);
    let calls = h.functions(a, from);
    assert!(
        calls.contains(&places::function::ENTER_PORTAL_SPACE),
        "into portal space: {calls:x?}"
    );
    let w = world_of(&h, me);
    assert!(
        h.server.players.get(&w).is_some_and(|p| p.trip.is_some()),
        "on its way"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 3.0));
    let p = h.server.players.get(&w).expect("in the world");
    assert!(p.trip.is_none(), "arrived");
    let d = ((p.position.origin[0] - them.origin[0]).powi(2)
        + (p.position.origin[1] - them.origin[1]).powi(2))
    .sqrt();
    assert!(
        (d - places::BESIDE_LIFESTONE).abs() < 0.01,
        "beside the other: {d}"
    );

    let at = StoredPosition {
        cell: 0x0298_0103,
        origin: [30.0, 40.0, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let clock = h.clock();
    characters::teleport(
        &mut h.server,
        &mut h.net,
        clock,
        "Hand Of Doom",
        &Destination::At(at),
    )
    .expect("sent");
    h.run_to(yalain_net::LocalTime(h.now.0 + 3.0));
    let p = h.server.players.get(&w).expect("in the world");
    assert_eq!((p.position.cell, p.position.origin), (at.cell, at.origin));

    let clock = h.clock();
    for (who, to) in [
        ("Hand Of Doom", Destination::Place("nowhere_at_all".into())),
        ("Hand Of Doom", Destination::Character("Nobody".into())),
        ("Nobody", Destination::At(at)),
    ] {
        assert!(
            characters::teleport(&mut h.server, &mut h.net, clock, who, &to).is_err(),
            "{who} to {to:?}"
        );
    }
}

/// A name with spaces, quoted or written as the rest of the line, reaches the character it names:
/// a gag of `"Sir Kara"` gags Sir Kara, and the words of a line keep a quoted name whole.
/// Behaviour: server.operator.names-with-spaces-at-the-console
#[test]
fn a_name_with_spaces_reaches_the_character_it_names() {
    let mut h = world();
    let (_, id) = h.player_in_world("one", "Sir Kara");
    h.run(5);
    for line in ["gag \"Sir Kara\"", "ungag Sir  Kara"] {
        let c = admin::parse(line).expect("a command").expect("it reads");
        let clock = h.clock();
        let said = admin::run(
            &mut h.server,
            &mut h.net,
            clock,
            &admin::Issuer::Console,
            &c,
        );
        assert!(said[0].starts_with("Sir Kara has been"), "{line}: {said:?}");
    }
    assert!(!h.server.gags.gagged(id), "gagged, then ungagged");
    let words = admin::words("teleport \"Hand Of Doom\" to Sir Kara");
    assert_eq!(words[1].text, "Hand Of Doom");
    assert!(words[1].quoted);
    assert_eq!(admin::name_of(&words[3..]), "Sir Kara");
}

/// A character that has played, given a build out of the world, holds what a player who earned
/// the build's level and trained and raised its skill by its own requests holds (level,
/// experience left and spent, health and vigor, credits, the skill), with the build's money, item
/// and quest; what the build does not name stays (its guide book, its place, its experience above
/// the build's), and given again nothing is added twice. One in the world is refused.
/// Behaviour: server.operator.builds-given-to-played-characters
#[test]
fn a_build_given_to_a_played_character_raises_it_as_play_would_and_keeps_the_rest() {
    let mut h = world();
    let (b, given) = h.player_in_world("one", "Elara");
    let (e, earned) = h.player_in_world("two", "Fen");
    let (wg, we) = (world_of(&h, given), world_of(&h, earned));
    // Both played to level 2.
    progress::award(&mut h.server, &mut h.net, wg, 20, Award::Step);
    progress::award(&mut h.server, &mut h.net, we, 20, Award::Step);
    h.run(5);
    // One earns level 3 and trains and raises the skill by its own requests.
    progress::award(&mut h.server, &mut h.net, we, 980, Award::Step);
    h.run(5);
    for f in [0x8214_0002u32].into_iter().chain([0x8214_000C; 4]) {
        h.bots[e].send(true, queue::WEENIE, &server_call(f, &[Arg::Int(SKILL)]));
        h.run(10);
    }
    let reached = {
        let p = h.server.players.get(&we).expect("in the world");
        let starting = h.server.start.skills.get(&1).cloned().unwrap_or_default();
        let held = yalain_world::skills::held(&starting, &p.play.skills);
        h.server.skill_table.rating(&held, SKILL, false)
    };
    assert!(reached >= 3, "trained and raised by requests: {reached}");
    yalain_world::world::keep_player(&mut h.server, we);
    let (earned_q, earned_play) = kept(&mut h, earned);

    let r = Recipe {
        level: Some(3),
        money: Some(700),
        skills: vec![(SKILL, reached)],
        items: vec![(TEST_ITEM, 2)],
        quests: vec![(QUEST, QuestAt::Phase(2))],
        ..recipe("Elara")
    };
    let unix = h.unix;
    let refused = characters::apply(&mut h.server, &r, unix).expect_err("in the world");
    assert!(refused.contains("in the world"), "{refused}");
    h.bots[b].leave();
    h.run_to(yalain_net::LocalTime(h.now.0 + 25.0));
    assert!(
        h.server.players.values().all(|p| p.character != given),
        "out of the world"
    );
    let before = h
        .server
        .store
        .world_state(given)
        .expect("reads")
        .expect("kept");
    let said = characters::apply(&mut h.server, &r, unix).expect("given");
    assert!(said.contains("Elara"), "{said}");
    let (q_, play) = kept(&mut h, given);
    for key in [
        q::LEVEL,
        quality::HEALTH_MAX,
        quality::VIGOR_MAX,
        quality::HEALTH,
        quality::VIGOR,
    ] {
        assert_eq!(
            quals::int(&q_, key),
            quals::int(&earned_q, key),
            "int {key}"
        );
    }
    assert_eq!(quals::int(&q_, q::LEVEL), Some(3));
    for key in [q::TOTAL_XP, q::AVAILABLE_XP] {
        assert_eq!(
            quals::long(&q_, key),
            quals::long(&earned_q, key),
            "long {key}"
        );
    }
    assert_eq!(play.credits, earned_play.credits, "credits left");
    assert_eq!(
        play.skills, earned_play.skills,
        "the skill as trained and raised"
    );
    assert_eq!(yalain_world::store::money(&q_), 700);
    let count = |play: &PlayState, t: u32| -> u32 {
        play.items
            .iter()
            .filter(|i| i.template == t)
            .map(|i| i.quantity.max(1))
            .sum()
    };
    assert_eq!(
        count(&play, TEST_ITEM),
        2,
        "the build's item: {:?}",
        play.items
    );
    assert_eq!(count(&play, GUIDE_BOOK), 1, "its own item kept");
    assert!(play
        .quests
        .iter()
        .any(|x| x.quest == QUEST && x.status == status::UNDERWAY && x.phase == 2));
    let after = h
        .server
        .store
        .world_state(given)
        .expect("reads")
        .expect("kept");
    assert_eq!(after.position, before.position, "where it left");
    assert_eq!(after.instances, before.instances, "not a new character");

    // Given again, and given less: nothing added twice, nothing lowered.
    characters::apply(&mut h.server, &r, unix).expect("given again");
    let lower = Recipe {
        level: Some(2),
        skills: vec![(SKILL, 1)],
        ..recipe("Elara")
    };
    characters::apply(&mut h.server, &lower, unix).expect("given a lower build");
    let (q2, play2) = kept(&mut h, given);
    assert_eq!(count(&play2, TEST_ITEM), 2, "not added twice");
    assert_eq!(quals::int(&q2, q::LEVEL), Some(3), "not lowered");
    assert_eq!(quals::long(&q2, q::TOTAL_XP), quals::long(&q_, q::TOTAL_XP));
    assert_eq!(play2.skills, play.skills, "the skill not lowered");

    // It enters as it was left, with what it was given.
    let nb = h.at_character_list("one");
    h.enter_world(nb, "one", given);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == given)
        .expect("in the world");
    assert!(p.instance > 1, "not a new character");
    assert_eq!(
        p.play
            .items
            .iter()
            .filter(|i| i.template == GUIDE_BOOK)
            .count(),
        1,
        "no second guide book"
    );
    assert_eq!(yalain_world::store::money(&p.qualities), 700);
}

/// A character deleted at once, in the world or grayed out from a player's deletion, is out of
/// the world and gone from the store straight away, its login booted, and its name makes a new
/// character at once.
/// Behaviour: server.operator.characters-deleted-at-once
#[test]
fn a_character_deleted_at_once_is_gone_and_its_name_free_straight_away() {
    let mut h = world();
    let (b, id) = h.player_in_world("one", "Sir Kara");
    h.run(5);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let said =
        characters::delete_now(&mut h.server, &mut h.net, clock, "sir kara").expect("deleted");
    assert!(said.contains("booted"), "{said}");
    assert!(h.server.players.is_empty(), "out of the world at once");
    assert_eq!(
        h.server.store.character(id).expect("reads"),
        None,
        "gone at once"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 2.0));
    let booted = h.bots[b].inbox[from..].iter().any(|r| {
        matches!(r.message(), Some(ServerMessage::AccountBooted(Text(t)))
            if t.string_id == admin::BOOTED)
    });
    assert!(booted, "its login was booted");
    let unix = h.unix;
    characters::make(&mut h.server, &recipe("Sir Kara"), unix).expect("its name made again");
    let again = id_of(&mut h, "Sir Kara");
    assert_ne!(again, id);

    // Deleted as its player would, it waits out its hour; deleted at once, it goes now.
    let clock = h.clock();
    characters::delete(&mut h.server, &mut h.net, clock, "Sir Kara").expect("deleted");
    let held = characters::make(&mut h.server, &recipe("Sir Kara"), unix).expect_err("grayed");
    assert!(held.contains("deleted"), "{held}");
    characters::delete_now(&mut h.server, &mut h.net, clock, "Sir Kara").expect("gone now");
    assert_eq!(h.server.store.character(again).expect("reads"), None);
    characters::make(&mut h.server, &recipe("Sir Kara"), unix).expect("made again at once");
    assert!(characters::delete_now(&mut h.server, &mut h.net, clock, "Nobody").is_err());
}

/// A character out of the world, played or never played, placed at the console enters there
/// next: at a cell and point, or beside a character in the world; one in the world, or a place
/// the world does not name, is refused.
/// Behaviour: server.operator.characters-placed-out-of-the-world
#[test]
fn a_character_placed_out_of_the_world_enters_there_next() {
    let mut h = world();
    let (b, id) = h.player_in_world("one", "Elara");
    let (_, _other) = h.player_in_world("two", "Sir Kara");
    let at = StoredPosition {
        cell: 0x0298_0103,
        origin: [30.0, 40.0, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let e = characters::place(&mut h.server, "Elara", &Destination::At(at)).expect_err("in it");
    assert!(e.contains("in the world"), "{e}");
    h.bots[b].leave();
    h.run_to(yalain_net::LocalTime(h.now.0 + 25.0));
    let said = characters::place(&mut h.server, "elara", &Destination::At(at)).expect("placed");
    assert!(said.contains("Elara"), "{said}");
    let nb = h.at_character_list("one");
    h.enter_world(nb, "one", id);
    let p = h
        .server
        .players
        .get(&world_of(&h, id))
        .expect("in the world");
    assert_eq!((p.position.cell, p.position.origin), (at.cell, at.origin));

    // Never played, beside another character in the world.
    let unix = h.unix;
    characters::make(&mut h.server, &recipe("Freshly"), unix).expect("made");
    let kara = h
        .server
        .players
        .values()
        .find(|p| p.name == "Sir Kara")
        .map(|p| p.position)
        .expect("in the world");
    characters::place(
        &mut h.server,
        "Freshly",
        &Destination::Character("Sir Kara".into()),
    )
    .expect("placed beside");
    let fresh = id_of(&mut h, "Freshly");
    let st = h
        .server
        .store
        .world_state(fresh)
        .expect("reads")
        .expect("kept");
    let to = st.position.expect("a place");
    let d =
        ((to.origin[0] - kara.origin[0]).powi(2) + (to.origin[1] - kara.origin[1]).powi(2)).sqrt();
    assert!(
        (d - places::BESIDE_LIFESTONE).abs() < 0.01,
        "beside the other: {d}"
    );
    for (who, to) in [
        ("Freshly", Destination::Place("nowhere_at_all".into())),
        ("Freshly", Destination::Character("Nobody".into())),
        ("Nobody", Destination::At(at)),
    ] {
        assert!(
            characters::place(&mut h.server, who, &to).is_err(),
            "{who} to {to:?}"
        );
    }
}

/// A character a killed client left in the world is taken out of it at once at the console,
/// without lingering: out of the world straight away and seen to leave by those beside it, kept
/// where it stood and not deleted, and its account enters it again at once, there. A name not in
/// the world is refused.
/// Behaviour: server.operator.characters-taken-out-of-the-world
#[test]
fn a_character_a_killed_client_left_in_the_world_is_taken_out_at_once_and_kept() {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    let mut h = world();
    let (a, id) = h.player_in_world("one", "Elara");
    let (watcher, _) = h.player_in_world("two", "Torvald");
    // The client's process ends without a word: nothing reaches it, nor the server from it.
    h.bots[a].addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 59_000));
    h.run(20);
    let stood = h
        .server
        .players
        .get(&world_of(&h, id))
        .expect("lingering in the world")
        .position;
    let from = h.bots[watcher].inbox.len();
    let clock = h.clock();
    let said =
        characters::force_logoff(&mut h.server, &mut h.net, clock, "elara").expect("taken out");
    assert!(said.contains("Elara"), "{said}");
    assert!(
        h.server.players.values().all(|p| p.character != id),
        "out of the world at once"
    );
    let left = |m: &ServerMessage| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == id);
    assert!(
        h.wait_for(watcher, from, |m| left(m).then_some(()))
            .is_some(),
        "seen to leave"
    );
    let record = h.server.store.character(id).expect("reads").expect("kept");
    assert_eq!(record.deleted_at, None, "not deleted");
    let kept = h
        .server
        .store
        .world_state(id)
        .expect("reads")
        .and_then(|st| st.position)
        .expect("kept with a place");
    assert_eq!((kept.cell, kept.origin), (stood.cell, stood.origin));

    let b = h.at_character_list("one");
    h.enter_world(b, "one", id);
    let p = h
        .server
        .players
        .get(&world_of(&h, id))
        .expect("in the world again");
    assert_eq!(
        (p.position.cell, p.position.origin),
        (stood.cell, stood.origin)
    );

    let unix = h.unix;
    characters::make(&mut h.server, &recipe("Freshly"), unix).expect("made");
    let clock = h.clock();
    for who in ["Nobody", "Freshly"] {
        let e = characters::force_logoff(&mut h.server, &mut h.net, clock, who)
            .expect_err("not in the world");
        assert!(e.contains("not in the world"), "{e}");
    }
}

/// An exported character carries its build in a recipe's terms: a character made with a build
/// exports that build (its level, money, hero, place, item and quest, each skill it trained at its
/// rating), and one that has played since exports what it holds now, which, given to a new
/// character, makes one holding the same.
/// Behaviour: server.operator.exported-characters-carry-their-build
#[test]
fn an_exported_character_carries_its_build_with_its_skills_ratings() {
    let mut h = world();
    let place = StoredPosition {
        cell: 0x0299_0105,
        origin: [12.0, 14.0, 0.5],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let r = Recipe {
        level: Some(3),
        money: Some(2500),
        hero: Some(true),
        skills: vec![(SKILL, 3)],
        items: vec![(TEST_ITEM, 5)],
        quests: vec![(QUEST, QuestAt::Phase(2))],
        place: Some(place),
        ..recipe("Hand Of Doom")
    };
    let unix = h.unix;
    characters::make(&mut h.server, &r, unix).expect("made");
    let file = characters::export(&mut h.server, "Hand Of Doom").expect("exported");
    let build = file.build.expect("a build");
    let starting = h.server.start.skills.get(&1).cloned().unwrap_or_default();
    assert_eq!(
        Recipe {
            skills: build
                .skills
                .iter()
                .copied()
                .filter(|(s, _)| !starting.iter().any(|(t, _)| t == s))
                .collect(),
            ..build.clone()
        },
        Recipe {
            account: String::new(),
            ..r
        },
        "the build it was made with"
    );
    for (skill, granted) in &starting {
        assert!(
            build.skills.contains(&(*skill, *granted)),
            "its starting skill {skill} at {granted}: {:?}",
            build.skills
        );
    }

    // Played: experience past its level's, its skill raised by the server's rules.
    let (_, id) = h.player_in_world("two", "Elara");
    let w = world_of(&h, id);
    progress::award(&mut h.server, &mut h.net, w, 1500, Award::Step);
    yalain_world::inventory::give(&mut h.server, &mut h.net, w, TEST_ITEM, 2);
    h.run(5);
    let played = characters::export(&mut h.server, "Elara")
        .expect("exported")
        .build
        .expect("a build");
    assert_eq!((played.level, played.experience), (None, Some(1500)));
    assert!(played.items.contains(&(TEST_ITEM, 2)), "{:?}", played.items);
    let again = Recipe {
        account: "three".into(),
        name: "Elara Again".into(),
        ..played.clone()
    };
    characters::make(&mut h.server, &again, unix).expect("made from the build");
    let copy = characters::export(&mut h.server, "Elara Again")
        .expect("exported")
        .build
        .expect("a build");
    assert_eq!(
        Recipe {
            name: played.name.clone(),
            ..copy
        },
        played,
        "the same build"
    );
}
