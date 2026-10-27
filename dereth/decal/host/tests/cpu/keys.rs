//! Behaviour: none (a host inside another program: the game's keys it presses for plugins)
//!
//! In the world of a recorded session, with the player's leave to use the game's keys, the actor
//! plugin's attack, jump and held movement come out of the C interface as the game's own keys,
//! pressed and let go on the host's clock: the target selected first, the player's own binding
//! when Decal reads one, held for the attack's power. Without the leave, or with the game's window
//! behind or its chat line open, those actions fail at once and press nothing; a key the player
//! presses lets go of what the host holds.

use dereth_client_sdk::plugin_api::Value;

use crate::abi::{plugin_dir, settings, Replay, FRAME_RATE};

const ACTOR: &str = "test.actor";

/// The character in the world of `long-solo-play`, the actor loaded, the leave given or not.
fn in_world(keys: bool) -> Replay {
    let mut r = Replay::new(
        &settings(&[plugin_dir("actor")]).with("keys", keys),
        "long-solo-play",
    );
    r.frame();
    r.login();
    r.advance(5.0);
    r.surface.calls.clear();
    r
}

/// One frame a thirtieth of a second on.
fn step(r: &mut Replay) {
    r.now += 1.0 / FRAME_RATE;
    r.frame();
}

/// The player types `/act <what>`; two frames run, as the actor's command and its action take.
fn act(r: &mut Replay, what: &str) {
    assert!(
        r.type_line(&format!("/act {what}")),
        "the actor claims /act"
    );
    step(r);
}

/// The key ops among the ops since `from`: each `(vk, scan, extended, down)`.
fn keys_since(r: &Replay, from: usize) -> Vec<(i64, i64, bool, bool)> {
    r.surface.calls[from..]
        .iter()
        .filter(|o| o.get("op").as_str() == Some("key"))
        .map(|o| {
            (
                o.get("vk").as_i64().unwrap_or(-1),
                o.get("scan").as_i64().unwrap_or(-1),
                o.get("extended").as_bool().unwrap_or(false),
                o.get("down").as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// Frames until a key op with `down` shows, for at most `secs`: the time it showed.
fn until_key(r: &mut Replay, down: bool, secs: f64) -> Option<f64> {
    let end = r.now + secs;
    while r.now < end {
        let from = r.surface.calls.len();
        step(r);
        if keys_since(r, from).iter().any(|k| k.3 == down) {
            return Some(r.now);
        }
    }
    None
}

fn says(r: &mut Replay, line: &str) -> bool {
    r.console(ACTOR).iter().any(|l| l == line)
}

#[test]
fn an_attack_selects_its_target_then_holds_the_players_attack_key_for_its_power() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    // The player's high attack is on 1.
    r.dll
        .bindings(&Value::map().with("CombatHighAttack", 0x31_i64));
    let from = r.surface.calls.len();
    act(&mut r, "strike");
    let pressed_at = r.now;
    let sent = r.surface.calls[from..].to_vec();
    let item = r
        .console(ACTOR)
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix("item\t").and_then(|s| s.parse::<i64>().ok()))
        .expect("the actor names its item");
    let select = sent
        .iter()
        .position(|o| o.get("op").as_str() == Some("select") && o.get("id").as_i64() == Some(item));
    let down = sent
        .iter()
        .position(|o| o.get("op").as_str() == Some("key") && o.get("down").as_bool() == Some(true));
    assert!(
        matches!((select, down), (Some(s), Some(d)) if s < d),
        "the target selected, then the key: {sent:?}"
    );
    assert_eq!(
        keys_since(&r, from),
        vec![(0x31, 0x02, false, true)],
        "the player's 1, at its place"
    );
    let released = until_key(&mut r, false, 2.0).expect("the key is let go");
    let held = released - pressed_at;
    assert!(
        (0.5 - 1e-9..0.5 + 2.0 / FRAME_RATE).contains(&held),
        "held for half the power bar's second: {held}"
    );
}

#[test]
fn without_the_players_leave_an_attack_or_a_held_key_fails_at_once_and_presses_nothing() {
    let mut r = in_world(false);
    r.dll.game_state(true, false);
    let from = r.surface.calls.len();
    act(&mut r, "strike");
    act(&mut r, "hold");
    assert!(keys_since(&r, from).is_empty(), "{:?}", r.surface.calls);
    assert!(says(&mut r, "failed\tcombat.attack\tunavailable"));
    assert!(says(&mut r, "failed\tnav.motion\tunavailable"));
}

#[test]
fn with_the_game_window_behind_or_its_chat_line_open_an_attack_fails_with_why() {
    let mut r = in_world(true);
    r.dll.game_state(false, false);
    let from = r.surface.calls.len();
    act(&mut r, "strike");
    assert!(says(
        &mut r,
        "failed\tcombat.attack\tunavailable (the game window is not in front)"
    ));
    r.dll.game_state(true, true);
    act(&mut r, "jump");
    assert!(says(
        &mut r,
        "failed\tnav.jump\tunavailable (the chat line is open)"
    ));
    assert!(keys_since(&r, from).is_empty(), "{:?}", r.surface.calls);
}

#[test]
fn a_key_the_player_presses_lets_go_of_a_plugins_held_movement_and_ends_it() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    let from = r.surface.calls.len();
    act(&mut r, "hold");
    assert_eq!(
        keys_since(&r, from),
        vec![(0x57, 0x11, false, true)],
        "the game's forward, W"
    );
    step(&mut r);
    let from = r.surface.calls.len();
    r.dll.player_moved();
    step(&mut r);
    step(&mut r);
    assert_eq!(keys_since(&r, from), vec![(0x57, 0x11, false, false)]);
    assert!(
        says(&mut r, "failed\tnav.motion\tinterrupted"),
        "{:?}",
        r.console(ACTOR)
    );
}

#[test]
fn the_game_window_going_behind_lets_go_of_the_keys_held() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    act(&mut r, "hold");
    step(&mut r);
    let from = r.surface.calls.len();
    r.dll.game_state(false, false);
    step(&mut r);
    step(&mut r);
    assert_eq!(keys_since(&r, from), vec![(0x57, 0x11, false, false)]);
    assert!(says(&mut r, "failed\tnav.motion\tinterrupted"));
}

#[test]
fn a_jump_holds_the_jump_key_for_its_charge_and_lets_go_to_leap() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    let from = r.surface.calls.len();
    act(&mut r, "jump");
    let pressed = keys_since(&r, from);
    let pressed_at = r.now;
    assert_eq!(pressed, vec![(0x20, 0x39, false, true)], "Space");
    let released = until_key(&mut r, false, 2.0).expect("the jump key is let go");
    let held = released - pressed_at;
    assert!(
        (0.5 - 1e-9..0.5 + 2.0 / FRAME_RATE).contains(&held),
        "held for the jump's half second: {held}"
    );
}

#[test]
fn stopping_a_plugins_attack_lets_go_of_its_key_at_once() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    act(&mut r, "strike");
    let from = r.surface.calls.len();
    act(&mut r, "stop");
    assert_eq!(
        keys_since(&r, from),
        vec![(0x22, 0x51, true, false)],
        "the game's high attack, Page Down"
    );
}

/// What the actor last named under `label`.
fn named(r: &mut Replay, label: &str) -> u32 {
    r.console(ACTOR)
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix(&format!("{label}\t")))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("the actor names its {label}"))
}

/// The selections among the ops since `from`.
fn selects_since(r: &Replay, from: usize) -> Vec<i64> {
    r.surface.calls[from..]
        .iter()
        .filter(|o| o.get("op").as_str() == Some("select"))
        .filter_map(|o| o.get("id").as_i64())
        .collect()
}

#[test]
fn stopping_a_plugins_attack_clears_the_target_its_attack_selected() {
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    act(&mut r, "strike");
    let item = named(&mut r, "item");
    // The game says the target is selected, as Decal tells of the plugin's own selection.
    r.dll.selected(item);
    let from = r.surface.calls.len();
    act(&mut r, "stop");
    assert_eq!(selects_since(&r, from), vec![0], "the selection cleared");
}

#[test]
fn stopping_an_attack_leaves_alone_a_selection_the_player_made() {
    // The player picked something else after the attack began.
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    act(&mut r, "strike");
    let (item, me) = (named(&mut r, "item"), named(&mut r, "me"));
    r.dll.selected(item);
    r.dll.selected(me);
    let from = r.surface.calls.len();
    act(&mut r, "stop");
    assert_eq!(
        selects_since(&r, from),
        Vec::<i64>::new(),
        "another pick kept"
    );
    // The player had picked the target before the plugin attacked it.
    let mut r = in_world(true);
    r.dll.game_state(true, false);
    act(&mut r, "name");
    let item = named(&mut r, "item");
    r.dll.selected(item);
    act(&mut r, "strike");
    let from = r.surface.calls.len();
    act(&mut r, "stop");
    assert_eq!(
        selects_since(&r, from),
        Vec::<i64>::new(),
        "the player's pick kept"
    );
}

#[test]
fn the_plugin_panels_switch_gives_the_leave_and_the_setting_is_kept() {
    let mut r = in_world(false);
    r.dll.game_state(true, false);
    assert!(r.type_line("/plugin"), "the host's own command");
    let key = "dereth.plugins/panel";
    let (control, state) = r
        .surface
        .control(key, "game_keys")
        .unwrap_or_else(|| panic!("the panel has the switch: {:?}", r.surface.views.get(key)));
    assert_eq!(state.get("checked").as_bool(), Some(false), "{state:?}");
    let from = r.surface.calls.len();
    r.view_event(key, &control, "change", Value::Bool(true));
    let setting = r.surface.calls[from..]
        .iter()
        .find(|o| o.get("op").as_str() == Some("setting"))
        .cloned()
        .expect("the setting is kept");
    assert_eq!(
        (setting.get("name").as_str(), setting.get("value").as_str()),
        (Some("keys"), Some("on"))
    );
    assert!(
        r.surface.printed.iter().any(|l| l
            == "[Plugins] Plugins can use the game's keys now (attacks Page Down, End and Delete; jump Space)."),
        "{:?}",
        r.surface.printed
    );
    let from = r.surface.calls.len();
    act(&mut r, "strike");
    assert_eq!(keys_since(&r, from), vec![(0x22, 0x51, true, true)]);
}
