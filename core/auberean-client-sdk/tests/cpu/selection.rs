//! Selecting by the selection keys' rules through actions alone, driven on the SDK's driver against
//! a server double, with no scripts: the client keeps the selection and makes the server's calls;
//! and the selection let go as the game lets it go.

use auberean_client_sdk::contract::event::SessionEvent;
use auberean_client_sdk::contract::SelectKind;
use auberean_client_sdk::protocol::physics::{CreateObject, ObjectOnly};
use auberean_client_sdk::protocol::quality::VisualQuality;
use auberean_client_sdk::protocol::types::{ObjectStamp, PhysicsDesc, Placement, WeenieDesc};
use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::calls;
use auberean_client_sdk::runtime::description::set_bool;
use auberean_client_sdk::runtime::host::PanelNotice;
use auberean_client_sdk::runtime::said::native;
use auberean_client_sdk::{no_icons, Action, Driver, FixedStep, GameEvent, Phase};

use crate::driver::{driven_by, s2c, script_with, DoublePort, FRAMES, PLAYER, WORLD};

/// The player's cell (indoors: the radar's shorter range).
const CELL: u32 = 0x0298_0109;
/// Three creatures, ahead, to the right a meter up, and behind; an item just ahead.
const AHEAD: u64 = 0x2130_0000_0000_0010;
const RIGHT: u64 = 0x2130_0000_0000_0011;
const BEHIND: u64 = 0x2130_0000_0000_0012;
const ITEM: u64 = 0x8130_0000_0000_0013;
/// The kind bits of a creature and an item; whether an object may be selected.
const CREATURE: i32 = 0x4F;
const AN_ITEM: i32 = 0x81;
const KIND: u32 = 20;
const SELECTABLE: u32 = 624;
const DEAD: u32 = 279;

/// An object that may be selected, standing at `at` in the player's cell, and its kind.
fn standing(id: u64, kind: i32, at: [f32; 3]) -> (CreateObject, i32) {
    let mut weenie = WeenieDesc::default();
    assert!(set_bool(&mut weenie, SELECTABLE, 1));
    let mut position = Placement {
        cell: CELL,
        ..Placement::default()
    };
    position.frame.origin.x = at[0];
    position.frame.origin.y = at[1];
    position.frame.origin.z = at[2];
    let o = CreateObject {
        id,
        physics: PhysicsDesc {
            bits: PhysicsDesc::POSITION,
            instance_stamp: 1,
            visual_ordering_stamp: 1,
            position,
            ..PhysicsDesc::default()
        },
        weenie,
        ..CreateObject::default()
    };
    (o, kind)
}

/// Into the world with the creatures and the item around the player; the item forgotten by the
/// server after `forget_at` seconds, when given.
fn in_the_world(forget_at: Option<f64>) -> (Driver, DoublePort, FixedStep) {
    in_the_world_with(forget_at, None)
}

/// [`in_the_world`], with one of the objects dead when given.
fn in_the_world_with(forget_at: Option<f64>, dead: Option<u64>) -> (Driver, DoublePort, FixedStep) {
    let mut objects = [
        standing(AHEAD, CREATURE, [0.0, 3.0, 0.0]),
        standing(RIGHT, CREATURE, [4.0, 0.0, 1.0]),
        standing(BEHIND, CREATURE, [0.0, -6.0, 0.0]),
        standing(ITEM, AN_ITEM, [0.0, 1.0, 0.0]),
    ];
    for (o, _) in &mut objects {
        if Some(o.id) == dead {
            assert!(set_bool(&mut o.weenie, DEAD, 1));
        }
    }
    let mut steps = Vec::new();
    for (o, n) in objects.into_iter().zip((5u64..).step_by(2)) {
        // The kind, a quality of the object's own the server sends after making it.
        let kind = ServerMessage::UpdateIntVisual(VisualQuality {
            object: ObjectStamp {
                id: o.0.id,
                instance_stamp: 1,
                other_stamp: 2,
            },
            quality: KIND,
            value: o.1,
        });
        steps.push(s2c(
            3.12,
            1,
            1,
            WORLD | n,
            &ServerMessage::CreateObject(o.0),
        ));
        steps.push(s2c(3.13, 1, 1, WORLD | (n + 1), &kind));
    }
    if let Some(t) = forget_at {
        let forget = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
            id: ITEM,
            instance_stamp: 1,
            other_stamp: 0,
        }));
        steps.push(s2c(t, 1, 1, WORLD | 20, &forget));
    }
    let (mut driver, mut port) = driven_by(script_with(steps));
    let mut clock = FixedStep::new(1.0 / 60.0);
    let mut asked = false;
    for _ in 0..FRAMES {
        driver.frame(&mut clock, &mut port);
        let events = driver.take_events();
        if !asked
            && events
                .iter()
                .any(|e| matches!(e, GameEvent::Session(SessionEvent::Characters(_))))
        {
            driver.act(Action::PlayCharacter { character: PLAYER });
            asked = true;
        }
        let kinds_told = driver
            .model
            .object(BEHIND)
            .is_some_and(|o| o.qualities.ints.contains_key(&KIND));
        if driver.phase() == Phase::World && kinds_told {
            return (driver, port, clock);
        }
    }
    panic!("never in the world with the objects");
}

/// The objects the server was told were selected, from the `from`th message sent on.
fn selected_since(driver: &Driver, from: usize) -> Vec<u64> {
    driver.sent()[from..]
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::ServerCall(c) => [AHEAD, RIGHT, BEHIND, ITEM, PLAYER, 0]
                .into_iter()
                .find(|id| {
                    calls::select(*id)
                        == Action::ServerCall {
                            function: c.function.0,
                            args: c.args.0.clone(),
                        }
                }),
            _ => None,
        })
        .collect()
}

/// One action, and what it had the server told was selected.
fn selects(driver: &mut Driver, action: Action) -> Vec<u64> {
    let from = driver.sent().len();
    driver.act(action);
    selected_since(driver, from)
}

/// Through actions alone and with no scripts, the selection keys' rules choose: the next creature
/// nearest first, then farther (the one a meter up counts a fifth more), round again past the
/// last; the previous backward; the nearest item; the player itself, then the selection before
/// it; the last selection. The client keeps the selection, which the view says, and makes the
/// server's calls for selecting and using it.
///
/// Behaviour: session.selection.actions-alone-select-by-the-keys-rules
#[test]
fn actions_alone_select_by_the_keys_rules_and_without_scripts_make_the_calls() {
    let (mut d, _port, _clock) = in_the_world(None);
    let next = Action::SelectCycle {
        kind: SelectKind::Creature,
        previous: false,
    };
    let previous = Action::SelectCycle {
        kind: SelectKind::Creature,
        previous: true,
    };
    assert_eq!(selects(&mut d, next.clone()), [AHEAD], "the nearest first");
    assert_eq!(selects(&mut d, next.clone()), [RIGHT]);
    assert_eq!(selects(&mut d, next.clone()), [BEHIND]);
    assert_eq!(selects(&mut d, next), [AHEAD], "round again");
    assert_eq!(selects(&mut d, previous), [BEHIND], "backward, round again");
    let nearest_item = Action::SelectNearest {
        kind: SelectKind::Item,
    };
    assert_eq!(selects(&mut d, nearest_item), [ITEM]);
    d.refresh_view(&no_icons);
    let selection = d.view().world.as_ref().map(|w| w.selection);
    assert_eq!(
        selection.map(|s| (s.current, s.last)),
        Some((ITEM, BEHIND)),
        "the view says what the client keeps"
    );
    assert_eq!(selects(&mut d, Action::SelectSelf), [PLAYER]);
    assert_eq!(
        selects(&mut d, Action::SelectSelf),
        [ITEM],
        "again: the one before"
    );
    assert_eq!(selects(&mut d, Action::SelectLast), [PLAYER]);
    let from = d.sent().len();
    d.act(Action::UseSelected);
    let used = d.sent()[from..].iter().any(|s| {
        matches!(&s.message, ClientMessage::ServerCall(c)
        if calls::use_object(PLAYER) == Action::ServerCall {
            function: c.function.0,
            args: c.args.0.clone(),
        })
    });
    assert!(used, "the use call for the selection");
    assert!(
        selects(&mut d, Action::SelectLastAttacker) == [0],
        "nothing has attacked"
    );
    let fellow = Action::SelectNearest {
        kind: SelectKind::Fellow,
    };
    assert!(
        selects(&mut d, fellow).is_empty(),
        "no fellows: the selection stays"
    );
}

/// An object selected that the server takes out of the world is no longer selected: with no
/// scripts the player does not target what attacks it, so nothing is selected in its place.
///
/// Behaviour: session.selection.the-selection-goes-with-its-object
#[test]
fn the_selection_goes_with_its_object_out_of_the_world() {
    let (mut d, mut port, mut clock) = in_the_world(Some(8.0));
    let nearest_item = Action::SelectNearest {
        kind: SelectKind::Item,
    };
    assert_eq!(selects(&mut d, nearest_item), [ITEM]);
    let from = d.sent().len();
    for _ in 0..FRAMES {
        d.frame(&mut clock, &mut port);
        if d.model.object(ITEM).is_none() {
            break;
        }
    }
    assert!(d.model.object(ITEM).is_none(), "forgotten");
    d.frame(&mut clock, &mut port);
    assert_eq!(
        selected_since(&d, from),
        [0],
        "nothing selected in its place"
    );
    d.refresh_view(&no_icons);
    let current = d.view().world.as_ref().map(|w| w.selection.current);
    assert_eq!(current, Some(0));
}

/// The scripts' notice that `object` may (`true`) or may no longer be selected.
fn may_be_selected(object: u64, selectable: bool) -> PanelNotice {
    PanelNotice {
        native: native::SELECTABLE,
        about: Some(object),
        words: vec![u32::from(selectable)],
        objects: vec![None],
        requests: vec![None],
        effect_lines: vec![Vec::new()],
        listed: vec![Vec::new()],
        selections: vec![None],
    }
}

/// When the scripts say the object selected may no longer be selected, the game lets the
/// selection go at its next frame, each notice heard once, whether or not a front end took the
/// notices first: but not a dead object's, and not for a notice about another object, about one
/// that may be selected, or about the player itself.
///
/// Behaviour: session.selection.a-selection-that-may-no-longer-be-selected-is-let-go
#[test]
fn a_selection_that_may_no_longer_be_selected_is_let_go_unless_dead() {
    let (mut d, mut port, mut clock) = in_the_world_with(None, Some(BEHIND));
    let mut told = |d: &mut Driver, notice: Option<PanelNotice>, take: bool| {
        let from = d.sent().len();
        d.notices.extend(notice);
        if take {
            let _ = d.take_notices();
        }
        d.frame(&mut clock, &mut port);
        selected_since(d, from)
    };
    assert_eq!(selects(&mut d, Action::Select { target: AHEAD }), [AHEAD]);
    let nothing: [u64; 0] = [];
    assert_eq!(
        told(&mut d, Some(may_be_selected(RIGHT, false)), false),
        nothing
    );
    assert_eq!(
        told(&mut d, Some(may_be_selected(AHEAD, true)), false),
        nothing
    );
    assert_eq!(
        told(&mut d, Some(may_be_selected(AHEAD, false)), false),
        [0],
        "let go"
    );
    d.refresh_view(&no_icons);
    let current = d.view().world.as_ref().map(|w| w.selection.current);
    assert_eq!(current, Some(0));

    assert_eq!(selects(&mut d, Action::Select { target: AHEAD }), [AHEAD]);
    assert_eq!(told(&mut d, None, false), nothing, "each notice heard once");
    assert_eq!(
        told(&mut d, Some(may_be_selected(AHEAD, false)), true),
        [0],
        "heard though a front end took it first"
    );

    assert_eq!(selects(&mut d, Action::Select { target: BEHIND }), [BEHIND]);
    assert_eq!(
        told(&mut d, Some(may_be_selected(BEHIND, false)), false),
        nothing,
        "a dead one stays selected"
    );
    assert_eq!(selects(&mut d, Action::SelectSelf), [PLAYER]);
    assert_eq!(
        told(&mut d, Some(may_be_selected(PLAYER, false)), false),
        nothing,
        "the player itself"
    );
}
