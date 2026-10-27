//! Behaviour: none (a host inside another program: plugin actions as Decal's calls)
//!
//! In the world of a recorded session, the actor plugin (`tests/plugins/actor`) does one thing at a
//! time on the first item in the character's pack; each comes out of the C interface as the op that
//! makes the retail client send the same request, and an action Decal has no call for fails at
//! once as `unavailable`, sending nothing.

use dereth_client_sdk::plugin_api::Value;

use crate::abi::{plugin_dir, settings, Replay};

const ACTOR: &str = "test.actor";

/// The character in the world of `long-solo-play`, the actor loaded.
fn in_world() -> Replay {
    let mut r = Replay::new(&settings(&[plugin_dir("actor")]), "long-solo-play");
    r.frame();
    r.login();
    r.advance(5.0);
    r.surface.calls.clear();
    r
}

/// What `/act <what>` sent, over the frame it ran in and the next.
fn act(r: &mut Replay, what: &str) -> Vec<Value> {
    let before = r.surface.calls.len();
    assert!(
        r.type_line(&format!("/act {what}")),
        "the actor claims /act"
    );
    r.frame();
    r.surface.calls[before..].to_vec()
}

/// What the actor last named under `label`: the item, the character, the side pack.
fn named(r: &mut Replay, label: &str) -> i64 {
    let lines = r.console(ACTOR);
    lines
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix(&format!("{label}\t")))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("the actor names its {label}: {lines:?}"))
}

/// The op named `name` among `sent`.
fn op(r: &mut Replay, sent: &[Value], name: &str) -> Value {
    sent.iter()
        .find(|o| o.get("op").as_str() == Some(name))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "no {name} op among {sent:?}; the actor says {:?}",
                r.console(ACTOR)
            )
        })
}

#[test]
fn an_item_action_becomes_the_decal_call_that_sends_its_request() {
    let mut r = in_world();
    let sent = act(&mut r, "select");
    let item = named(&mut r, "item");
    assert_eq!(op(&mut r, &sent, "select").get("id").as_i64(), Some(item));
    let sent = act(&mut r, "appraise");
    assert_eq!(op(&mut r, &sent, "appraise").get("id").as_i64(), Some(item));
    r.advance(6.0);
    let sent = act(&mut r, "move");
    let pack = named(&mut r, "pack");
    let m = op(&mut r, &sent, "move");
    assert_eq!(
        (m.get("item").as_i64(), m.get("container").as_i64()),
        (Some(item), Some(pack))
    );
    r.advance(21.0);
    let sent = act(&mut r, "use");
    assert!(
        op(&mut r, &sent, "use")
            .get("id")
            .as_i64()
            .is_some_and(|id| id != item),
        "the food used"
    );
}

#[test]
fn a_chat_line_runs_through_the_clients_own_parser() {
    let mut r = in_world();
    let sent = act(&mut r, "say");
    assert_eq!(
        op(&mut r, &sent, "chat").get("line").as_str(),
        Some("hello from a plugin")
    );
}

#[test]
fn a_move_to_turns_the_character_and_sets_its_run_lock() {
    let mut r = in_world();
    // Decal's reads of the character's place, facing north.
    r.dll.place(0xA9B4_0017, 84.0, 4.0, 94.0, 0.0);
    let sent = act(&mut r, "face");
    let face = op(&mut r, &sent, "face")
        .get("heading")
        .as_f64()
        .expect("a heading");
    assert!((face - 90.0).abs() < 0.5, "faces east: {face}");
    // Facing east now, it runs east.
    r.dll.place(0xA9B4_0017, 84.0, 4.0, 94.0, 90.0);
    let sent = act(&mut r, "walk");
    assert_eq!(op(&mut r, &sent, "autorun").get("on").as_bool(), Some(true));
}

#[test]
fn the_players_own_movement_ends_a_plugins_and_lets_go_of_the_run_lock() {
    let mut r = in_world();
    r.dll.place(0xA9B4_0017, 84.0, 4.0, 94.0, 90.0);
    let sent = act(&mut r, "walk");
    assert_eq!(op(&mut r, &sent, "autorun").get("on").as_bool(), Some(true));
    let before = r.surface.calls.len();
    r.dll.player_moved();
    r.frame();
    r.frame();
    let after = r.surface.calls[before..].to_vec();
    assert_eq!(
        op(&mut r, &after, "autorun").get("on").as_bool(),
        Some(false)
    );
    assert!(
        r.console(ACTOR)
            .iter()
            .any(|l| l == "failed\tnav.move_to\tinterrupted"),
        "{:?}",
        r.console(ACTOR)
    );
}

#[test]
fn an_action_decal_has_no_call_for_fails_at_once_and_sends_nothing() {
    let mut r = in_world();
    for (what, action) in [
        ("attack", "combat.attack"),
        ("split", "items.split"),
        ("jump", "nav.jump"),
    ] {
        let sent = act(&mut r, what);
        assert!(sent.is_empty(), "{what} sent {sent:?}");
        assert!(
            r.console(ACTOR)
                .iter()
                .any(|l| *l == format!("failed\t{action}\tunavailable")),
            "{what}: {:?}",
            r.console(ACTOR)
        );
    }
}
