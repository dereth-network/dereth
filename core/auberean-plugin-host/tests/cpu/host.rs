//! The host with Auberean's Luau bindings, driven by frames made by hand: loading and refusing
//! packages, the game's events reaching handlers, the plugins' requests becoming actions, faults,
//! the frame's share of time, timers, stores and commands.

use std::rc::Rc;
use std::time::Duration;

use auberean_client_contract::event::{
    ChatEvent, EffectEvent, GameEvent, SessionEvent, TradeEvent,
};
use auberean_client_contract::session::{Character, CharacterList, Phase};
use auberean_client_contract::{Action, ChatWindowAction, SocialAction};

use crate::common::{frame, host, host_with, package, printed, Shared, Stores, TestClock};

/// Active before the world and in it, so a test need not enter the world.
const EVERYWHERE: &str = "contexts = [\"pregame\", \"world\"]\n";

const PLAYER: u64 = 0x2130_0000_0000_0001;

/// A manifest for Auberean loads and runs; one for Dereth (or naming no game, which is Dereth's)
/// is refused with the reason told, and only the first is listed.
///
/// Behaviour: plugins.host.a-plugin-for-another-game-is-refused-and-told
#[test]
fn an_auberean_plugin_loads_and_a_dereth_plugin_is_refused_with_the_reason_told() {
    let mut h = host();
    h.request_load(package("test.mine", "auberean", "", "print('up')"));
    h.request_load(package("test.theirs", "dereth", "", "print('up too')"));
    frame(&mut h, 0.0, Phase::Characters, &[]);
    let ids: Vec<String> = h.plugins().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, ["test.mine"]);
    let lines = printed(&mut h);
    assert!(lines.contains(&"test.mine: up".to_owned()), "{lines:?}");
    assert!(
        lines.contains(
            &"test.theirs was not loaded: it is a plugin for Dereth, not Auberean".to_owned()
        ),
        "{lines:?}"
    );
    assert!(!lines.iter().any(|l| l.contains("up too")), "{lines:?}");
}

/// The game's events reach the handlers listening for them, in the order the client told them,
/// with the game's ids as text; an event no handler listens for reaches none.
///
/// Behaviour: plugins.host.the-games-events-reach-handlers-in-order
#[test]
fn the_games_events_reach_the_handlers_listening_in_the_order_told() {
    let mut h = host();
    h.request_load(package(
        "test.ears",
        "auberean",
        EVERYWHERE,
        r#"
        on("session.pregame", function(e) print("listed " .. e.characters[1].name .. " " .. e.characters[1].id) end)
        on("effects.added", function(e) print("effect " .. e.effect) end)
        on("chat.said", function(e) print(e.speaker .. " says " .. e.text) end)
        on("trade.accepted", function(e) print("accepted by " .. e.by) end)
        "#,
    ));
    frame(&mut h, 0.0, Phase::Characters, &[]);
    printed(&mut h);
    let events = [
        GameEvent::Effect(EffectEvent::Added { effect: 7 }),
        GameEvent::Session(SessionEvent::Characters(CharacterList {
            characters: vec![Character {
                id: PLAYER,
                name: "Tester".into(),
                seconds_greyed_out: 0,
            }],
            slots: 6,
            ..CharacterList::default()
        })),
        GameEvent::Chat(ChatEvent::Said {
            speaker: Some(0xAB),
            text: "hello".into(),
        }),
        GameEvent::Effect(EffectEvent::Removed { effect: 7 }),
        GameEvent::Trade(TradeEvent::Accepted { by: PLAYER }),
    ];
    frame(&mut h, 0.1, Phase::Characters, &events);
    assert_eq!(
        printed(&mut h),
        [
            "test.ears: effect 7",
            "test.ears: listed Tester 0x2130000000000001",
            "test.ears: 0x00000000000000ab says hello",
            "test.ears: accepted by 0x2130000000000001",
        ]
    );
}

/// What a plugin asks of the game becomes the client's actions, in the order asked, carried out
/// as the frame ends: a selection, a line said, a chat window's setting, an attack stopped.
///
/// Behaviour: plugins.host.a-plugins-requests-are-the-clients-actions
#[test]
fn what_a_plugin_asks_of_the_game_becomes_the_clients_actions_in_order() {
    let mut h = host();
    h.request_load(package(
        "test.hands",
        "auberean",
        EVERYWHERE,
        r#"
        on("effects.added", function()
            act.select("0x2130000000000001")
            chat.say("hi there")
            chat.window(2, { opacity = 0.5 })
            act.cease_attack("2130000000000001")
        end)
        "#,
    ));
    assert!(frame(&mut h, 0.0, Phase::World, &[]).is_empty());
    let actions = frame(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
    );
    assert_eq!(
        actions,
        [
            Action::Select { target: PLAYER },
            Action::Say {
                text: "hi there".into()
            },
            Action::ChatWindows(ChatWindowAction::Opacity {
                window: 2,
                opacity: 0.5
            }),
            Action::CeaseAttack { target: PLAYER },
        ]
    );
}

/// A plugin's answer to a fellowship's place or an oath offered is the game's own: taken, or turned
/// down with the reasons the game's interface sends (`0x138D` for the place, `0x1585` for the
/// oath).
///
/// Behaviour: plugins.host.a-plugins-answer-to-an-offer-is-the-games-own
#[test]
fn a_plugins_answer_to_a_place_or_an_oath_offered_is_the_games_own() {
    let mut h = host();
    h.request_load(package(
        "test.offers",
        "auberean",
        EVERYWHERE,
        r#"
        on("effects.added", function()
            social.answer_fellowship(true)
            social.answer_fellowship(false)
            social.answer_oath(true)
            social.answer_oath(false)
        end)
        "#,
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    let actions = frame(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
    );
    assert_eq!(
        actions,
        [
            Action::Social(SocialAction::AcceptRecruitment),
            Action::Social(SocialAction::DeclineRecruitment(0x138D)),
            Action::Social(SocialAction::TakeOath),
            Action::Social(SocialAction::RefuseOath(0x1585)),
        ]
    );
}

/// A failing handler is told in the chat, one line each time; the tenth failure within a minute
/// suspends the plugin, which is told how to start it again and hears nothing more.
///
/// Behaviour: plugins.host.a-failing-plugin-is-told-and-suspended
#[test]
fn a_failing_plugin_is_told_each_time_and_suspended_at_the_tenth_failure() {
    let mut h = host();
    h.request_load(package(
        "test.broken",
        "auberean",
        EVERYWHERE,
        "on('effects.added', function(e) error('no effect ' .. e.effect) end)",
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    printed(&mut h);
    for n in 1..=9u32 {
        frame(
            &mut h,
            f64::from(n),
            Phase::World,
            &[GameEvent::Effect(EffectEvent::Added { effect: n })],
        );
        let lines = printed(&mut h);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("test.broken: "), "{lines:?}");
        assert!(lines[0].contains(&format!("no effect {n}")), "{lines:?}");
        assert_eq!(h.plugins()[0].status, "running");
    }
    frame(
        &mut h,
        10.0,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 10 })],
    );
    let lines = printed(&mut h);
    assert!(
        lines.contains(
            &"test.broken was suspended (10 errors within a minute). /plugin reload test.broken starts it again."
                .to_owned()
        ),
        "{lines:?}"
    );
    assert_eq!(h.plugins()[0].status, "suspended");
    frame(
        &mut h,
        11.0,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 11 })],
    );
    assert!(printed(&mut h).is_empty());
}

/// A plugin's entries in one frame have a share of it: past it, the events still due wait, in
/// order, for the next frame, before that frame's own (the time read off a clock the test moves).
///
/// Behaviour: plugins.host.past-its-share-of-a-frame-a-plugins-events-wait
#[test]
fn past_its_share_of_a_frame_a_plugins_events_wait_in_order_for_the_next() {
    let clock = Shared(Rc::new(TestClock::default()));
    let mut h = host_with(clock.clone(), Stores::default());
    h.request_load(package(
        "test.slow",
        "auberean",
        EVERYWHERE,
        "on('effects.added', function(e) print(e.effect) end)",
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    printed(&mut h);
    // The share is two budgets (two minutes here); each entry now reads as seventy seconds.
    clock.0.step.set(Duration::from_secs(70));
    let effects = |from: u32| -> Vec<GameEvent> {
        (from..from + 3)
            .map(|effect| GameEvent::Effect(EffectEvent::Added { effect }))
            .collect()
    };
    frame(&mut h, 1.0, Phase::World, &effects(1));
    assert_eq!(printed(&mut h), ["test.slow: 1", "test.slow: 2"]);
    frame(&mut h, 2.0, Phase::World, &effects(4));
    assert_eq!(printed(&mut h), ["test.slow: 3", "test.slow: 4"]);
    clock.0.step.set(Duration::ZERO);
    frame(&mut h, 3.0, Phase::World, &[]);
    assert_eq!(printed(&mut h), ["test.slow: 5", "test.slow: 6"]);
}

/// A timer is due on the frame clock: not before its time, once at it; a plugin inactive in the
/// session's part (here, before the world) hears no timer until it is active.
///
/// Behaviour: plugins.host.timers-come-due-on-the-frame-clock
#[test]
fn a_timer_comes_due_on_the_frame_clock_and_waits_while_its_plugin_is_inactive() {
    let mut h = host();
    h.request_load(package(
        "test.timer",
        "auberean",
        "",
        "task.after(500, function() print('due') end)",
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    assert_eq!(printed(&mut h), ["test.timer is loaded."]);
    frame(&mut h, 0.3, Phase::World, &[]);
    assert!(printed(&mut h).is_empty(), "not before its time");
    frame(&mut h, 0.6, Phase::Characters, &[]);
    assert!(printed(&mut h).is_empty(), "inactive before the world");
    frame(&mut h, 0.7, Phase::World, &[]);
    assert_eq!(printed(&mut h), ["test.timer: due"]);
    frame(&mut h, 2.0, Phase::World, &[]);
    assert!(printed(&mut h).is_empty(), "once");
}

/// A value a plugin keeps in its global store is written when it unloads and is there again when
/// it loads; a character's store opens once the character playing is known.
///
/// Behaviour: plugins.host.a-plugins-store-outlives-it
#[test]
fn a_plugins_store_is_written_on_unload_and_read_again_on_load() {
    let stores = Stores::default();
    let mut h = host_with(Shared::default(), stores.clone());
    h.set_session(Some("loopback".into()), Some("tester".into()));
    let main = r#"
        store.global.runs = (store.global.runs or 0) + 1
        print("run " .. store.global.runs)
        on("session.enter_world", function()
            store.character.seen = true
            print("kept for " .. session.info().character)
        end)
    "#;
    h.request_load(package("test.keeps", "auberean", EVERYWHERE, main));
    frame(
        &mut h,
        0.0,
        Phase::Characters,
        &[GameEvent::Session(SessionEvent::Characters(
            CharacterList {
                characters: vec![Character {
                    id: PLAYER,
                    name: "Tester".into(),
                    seconds_greyed_out: 0,
                }],
                slots: 6,
                ..CharacterList::default()
            },
        ))],
    );
    h.playing(PLAYER);
    frame(
        &mut h,
        1.0,
        Phase::World,
        &[GameEvent::Session(SessionEvent::Phase(Phase::World))],
    );
    assert_eq!(
        printed(&mut h),
        [
            "test.keeps: run 1",
            "test.keeps is loaded.",
            "test.keeps: kept for Tester"
        ]
    );
    h.request_unload("test.keeps");
    frame(&mut h, 2.0, Phase::World, &[]);
    let saved = stores.0.borrow().clone();
    assert_eq!(
        saved
            .get(&("test.keeps".into(), "global".into()))
            .map(String::as_str),
        Some("{\"runs\":1}")
    );
    assert_eq!(
        saved
            .get(&(
                "test.keeps".into(),
                "character:loopback:tester:Tester".into()
            ))
            .map(String::as_str),
        Some("{\"seen\":true}")
    );
    printed(&mut h);
    h.request_load(package("test.keeps", "auberean", EVERYWHERE, main));
    frame(&mut h, 3.0, Phase::World, &[]);
    assert_eq!(printed(&mut h)[0], "test.keeps: run 2");
}

/// A command a plugin claimed reaches it at the next frame when the player types it, as the
/// player's gesture: a window it shows then opens, where one it shows unasked stays closed; a line
/// no plugin claimed is not the host's.
///
/// Behaviour: plugins.host.a-typed-command-is-the-players-gesture
#[test]
fn a_typed_command_reaches_its_plugin_as_a_gesture_and_opens_the_window_it_shows() {
    let mut h = host();
    h.request_load(package(
        "test.window",
        "auberean",
        EVERYWHERE,
        r#"
        local win = ui.window{ id = "main", title = "Hi", ui.label{ id = "l", text = "-" } }
        win:show()
        commands.add("hi", "says hi", function(args)
            win:get("l").text = "hi " .. args
            win:show()
        end)
        "#,
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    assert!(!h.window("test.window", "main").expect("declared").open);
    assert!(!h.command("/nobody"));
    assert!(h.command("/hi there"));
    frame(&mut h, 0.1, Phase::World, &[]);
    let w = h.window("test.window", "main").expect("declared");
    assert!(w.open);
    assert_eq!(w.tree.find("l").expect("label").text("text"), "hi there");
}

/// The player closing a plugin's window (its close box) closes it and tells the plugin at the next
/// frame; a window already closed is left as it is, and a window the plugin never declared is
/// refused.
///
/// Behaviour: plugins.host.the-player-closing-a-window-is-told-to-its-plugin
#[test]
fn the_player_closing_a_window_closes_it_and_its_plugin_hears_it_once() {
    let mut h = host();
    h.request_load(package(
        "test.closing",
        "auberean",
        EVERYWHERE,
        r#"
        local win = ui.window{ id = "main", title = "Hi", ui.label{ id = "l", text = "-" } }
        on("ui.closed", function(e) chat.print("closed " .. e.window) end)
        "#,
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    h.open_window("test.closing", "main").expect("opens");
    frame(&mut h, 0.1, Phase::World, &[]);
    assert!(h.window("test.closing", "main").expect("declared").open);
    printed(&mut h);
    h.close_window("test.closing", "main").expect("closes");
    assert!(!h.window("test.closing", "main").expect("declared").open);
    frame(&mut h, 0.2, Phase::World, &[]);
    assert_eq!(printed(&mut h), ["test.closing: closed main"]);
    h.close_window("test.closing", "main")
        .expect("already closed");
    frame(&mut h, 0.3, Phase::World, &[]);
    assert!(printed(&mut h).is_empty(), "told once");
    assert!(h.close_window("test.closing", "other").is_err());
}
