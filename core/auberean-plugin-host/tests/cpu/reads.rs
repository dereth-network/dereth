//! What a plugin reads of the game: the view the frame's hook is handed, made by hand here, read
//! through Auberean's Lua bindings; the controls a plugin presses; the game's questions a plugin
//! answers.

use std::collections::BTreeMap;

use auberean_client_contract::control::{Control, ControlId};
use auberean_client_contract::event::{EffectEvent, GameEvent, SessionEvent};
use auberean_client_contract::question::{Asked, Question};
use auberean_client_contract::session::{Character, CharacterList, Phase};
use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::{
    Body, Effect, Effects, Inventory, Item, Player, Selection, Surroundings, Thing, View,
};
use auberean_client_contract::Action;

use crate::common::{frame, frame_seeing, host, host_with, package, printed, Shared, Stores};

const PLAYER: u64 = 0x2130_0000_0000_0001;
const DRUDGE: u64 = 0x8000_0000_0000_00AB;

fn view() -> View {
    let mut items = BTreeMap::new();
    items.insert(
        0x10,
        Item {
            quantity: 3,
            name: "Healing Draught".into(),
            stackable: true,
            ..Item::default()
        },
    );
    View {
        player_id: Some(PLAYER),
        player: Some(Player {
            id: PLAYER,
            name: Some("Tester".into()),
            world_name: "Loopback".into(),
            vitals: Some([80, 100, 40, 50]),
            body: Body {
                cell: 0x0298_0109,
                origin: [12.5, 30.0, 0.0],
                heading: 90.0,
                moving: false,
            },
            ..Player::default()
        }),
        world: Some(Surroundings {
            player: PLAYER,
            things: vec![Thing {
                id: DRUDGE,
                name: Some("Drudge".into()),
                place: Some((0x0298_0109, [20.0, 30.0, 0.0])),
                selectable: true,
                attackable: true,
                ..Thing::default()
            }],
            selection: Selection {
                current: DRUDGE,
                ..Selection::default()
            },
            ..Surroundings::default()
        }),
        inventory: Some(Inventory {
            money: 250,
            items,
            ..Inventory::default()
        }),
        effects: Some(Effects {
            now: 100.0,
            effects: vec![Effect {
                id: 7,
                name: "Haste".into(),
                time_cast: 90.0,
                duration: 60.0,
                ..Effect::default()
            }],
            ..Effects::default()
        }),
        ..View::default()
    }
}

/// A plugin reads the player, the objects around, the selection, the inventory and the effects
/// from the view the frame carries, the ids as text; with no view, a read fails saying so.
///
/// Behaviour: plugins.host.a-plugin-reads-the-game-from-the-view
#[test]
fn a_plugin_reads_the_player_the_objects_the_selection_and_the_inventory_from_the_view() {
    let mut h = host();
    h.request_load(package(
        "test.eyes",
        "auberean",
        "",
        r#"
        on("effects.added", function()
            local p = game.player()
            print(p.name .. " " .. p.id .. " " .. p.health .. "/" .. p.max_health .. " in " .. p.world)
            local o = game.objects()[1]
            print(o.name .. " " .. o.id .. " at " .. o.place.x .. " " .. tostring(o.attackable))
            print("selected " .. game.object(game.selection().current).name)
            local inv = game.inventory()
            print(inv.money .. " and " .. inv.items[1].quantity .. " " .. inv.items[1].name)
            local e = game.effects()[1]
            print(e.name .. " for " .. e.remaining)
        end)
        on("effects.removed", function()
            local ok, err = pcall(game.player)
            print(tostring(ok) .. " " .. tostring(err))
        end)
        "#,
    ));
    let v = view();
    frame_seeing(&mut h, 0.0, Phase::World, &[], &v);
    printed(&mut h);
    frame_seeing(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 7 })],
        &v,
    );
    assert_eq!(
        printed(&mut h),
        [
            "test.eyes: Tester 0x2130000000000001 80/100 in Loopback",
            "test.eyes: Drudge 0x80000000000000ab at 20 true",
            "test.eyes: selected Drudge",
            "test.eyes: 250 and 3 Healing Draught",
            "test.eyes: Haste for 50",
        ]
    );
    frame(
        &mut h,
        0.2,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Removed { effect: 7 })],
    );
    let lines = printed(&mut h);
    assert!(
        lines[0].starts_with("test.eyes: false") && lines[0].contains("no view of the game"),
        "{lines:?}"
    );
}

/// The character's store opens under the character the view names, with no word from the
/// product, and under the world the view names when the product named none.
///
/// Behaviour: plugins.host.the-character-playing-is-the-views
#[test]
fn the_character_store_opens_under_the_character_and_world_the_view_names() {
    let stores = Stores::default();
    let mut h = host_with(Shared::default(), stores.clone());
    h.set_session(None, Some("tester".into()));
    h.request_load(package(
        "test.keeps",
        "auberean",
        "",
        "on('session.enter_world', function() store.character.seen = true end)",
    ));
    let list = GameEvent::Session(SessionEvent::Characters(CharacterList {
        characters: vec![Character {
            id: PLAYER,
            name: "Tester".into(),
            seconds_greyed_out: 0,
        }],
        slots: 6,
        ..CharacterList::default()
    }));
    frame(&mut h, 0.0, Phase::Characters, &[list]);
    let v = view();
    frame_seeing(
        &mut h,
        1.0,
        Phase::World,
        &[GameEvent::Session(SessionEvent::Phase(Phase::World))],
        &v,
    );
    h.flush();
    assert_eq!(
        stores
            .0
            .borrow()
            .get(&(
                "test.keeps".into(),
                "character:Loopback:tester:Tester".into()
            ))
            .map(String::as_str),
        Some("{\"seen\":true}")
    );
}

/// A plugin presses a control of the game's action map and lets it go as a key bound to it
/// would, and lets go of every control, each as the contract's action.
///
/// Behaviour: plugins.host.a-plugin-presses-controls-as-a-key-would
#[test]
fn a_plugin_presses_and_releases_controls_as_a_key_would() {
    let mut h = host();
    h.request_load(package(
        "test.keys",
        "auberean",
        "",
        r#"
        on("effects.added", function()
            act.control(0x10000003)
            act.control(0x10000003, false)
            act.control(0x10000004, 0.5)
            act.release_controls(true)
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
            Action::Control(Control::begin(ControlId(0x1000_0003))),
            Action::Control(Control::end(ControlId(0x1000_0003))),
            Action::Control(Control::at(ControlId(0x1000_0004), 0.5)),
            Action::ReleaseControls { typing: true },
        ]
    );
}

/// A plugin reads the game's questions open from the view (each one's number, kind, words and who
/// it names) and answers one, as the contract's answer.
///
/// Behaviour: plugins.host.a-plugin-reads-and-answers-the-games-questions
#[test]
fn a_plugin_reads_the_games_questions_and_answers_one() {
    let mut h = host();
    h.request_load(package(
        "test.asks",
        "auberean",
        "",
        r#"
        on("effects.added", function()
            for _, q in ipairs(game.questions()) do
                print(q.id .. " " .. q.kind .. " " .. tostring(q.one_answer) .. " " .. (q.leader_name or q.text or "-"))
                if q.kind == "fellowship" then
                    act.answer(q.id, false)
                end
            end
        end)
        "#,
    ));
    let mut v = view();
    v.questions = vec![
        Question {
            id: 4,
            asked: Asked::Died,
            text: TextRef::literal("You have died."),
            at_once: true,
        },
        Question {
            id: 5,
            asked: Asked::Fellowship {
                leader: DRUDGE,
                leader_name: Some(TextRef::literal("Cara")),
                fellowship: None,
            },
            text: TextRef::default(),
            at_once: false,
        },
    ];
    frame_seeing(&mut h, 0.0, Phase::World, &[], &v);
    printed(&mut h);
    let actions = frame_seeing(
        &mut h,
        0.1,
        Phase::World,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
        &v,
    );
    assert_eq!(
        printed(&mut h),
        [
            "test.asks: 4 died true You have died.",
            "test.asks: 5 fellowship false Cara",
        ]
    );
    assert_eq!(
        actions,
        [Action::AnswerQuestion {
            question: 5,
            yes: false
        }]
    );
}
