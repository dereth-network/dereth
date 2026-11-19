//! Auberean's part of the plugin API against its spec page and its bindings: every event and
//! operation of the game's own is on the page, and each operation is a Lua function the host
//! answers.
//!
//! Behaviour: none (the API's tables against its spec page and bindings; host tests hold the rest)

use auberean_client_contract::session::Phase;
use auberean_plugin_host::{events, notices, ops};
use dereth_plugin_api::Game;

use crate::common::{frame, host, package, printed};

const SPEC: &str = include_str!("../../../../docs/plugins/auberean.md");

#[test]
fn the_spec_page_lists_every_event_and_operation_of_the_games_own() {
    for (name, _) in events::EVENTS.iter().chain(notices::EVENTS) {
        assert!(
            SPEC.contains(&format!("| `{name}` |")),
            "{name} is not on the page"
        );
    }
    for op in ops::OPS {
        let lua = op.lua.expect("each has its Lua function");
        assert!(
            SPEC.contains(&format!("`{lua}(")),
            "{lua} is not on the page"
        );
    }
}

#[test]
fn each_of_the_games_operations_is_a_lua_function_the_host_answers() {
    let mut main = String::new();
    for op in ops::OPS {
        let lua = op.lua.expect("each has its Lua function");
        main.push_str(&format!("assert(type({lua}) == \"function\", \"{lua}\")\n"));
    }
    // Every shared operation with a Lua function of its own is there too.
    for lua in Game::Auberean.ops().filter_map(|o| o.lua) {
        main.push_str(&format!("assert(type({lua}) == \"function\", \"{lua}\")\n"));
    }
    main.push_str("print(\"all there\")\n");
    let mut h = host();
    h.request_load(package("test.api", "auberean", "", &main));
    frame(&mut h, 0.0, Phase::World, &[]);
    assert_eq!(printed(&mut h)[0], "test.api: all there");
}

#[test]
fn an_event_of_another_game_cannot_be_subscribed_to() {
    let mut h = host();
    h.request_load(package(
        "test.wrong",
        "auberean",
        "",
        "local ok, err = pcall(on, 'character.luminance', function() end)\nprint(tostring(ok) .. ' ' .. tostring(err))",
    ));
    frame(&mut h, 0.0, Phase::World, &[]);
    let lines = printed(&mut h);
    assert!(lines[0].starts_with("test.wrong: false"), "{lines:?}");
    assert!(
        lines[0].contains("no event \"character.luminance\""),
        "{lines:?}"
    );
    assert!(events::is_known("effects.added") && events::is_known("chat.line"));
}

#[test]
fn ids_cross_as_text_both_ways() {
    use dereth_plugin_api::Value;
    assert_eq!(events::id_text(0x2130_0000_0000_0001), "0x2130000000000001");
    for v in [
        Value::from("0x2130000000000001"),
        Value::from("2130000000000001"),
    ] {
        assert_eq!(events::parse_id(&v), Some(0x2130_0000_0000_0001));
    }
    assert_eq!(events::parse_id(&Value::Int(42)), Some(42));
    assert_eq!(events::parse_id(&Value::from("0x")), None);
    assert_eq!(events::parse_id(&Value::from("12345678901234567")), None);
    assert_eq!(events::parse_id(&Value::Num(1.5)), None);
}
