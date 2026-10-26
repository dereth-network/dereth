//! The test suite's compiled plugin: the XP Meter's experience earned since login and the hourly
//! rate, in a window and on `/xpc`.
//!
//! The same plugin as the Lua XP Meter, written against the WIT interface; its command is `/xpc`
//! so that both can be loaded at once (a command belongs to the first plugin that claims it). `init` does what the
//! Lua plugin's top level does (subscribe, claim the command, declare the window, start the
//! timer); `deliver` is its event handlers. Values the host hands over or takes (event data, the
//! window's tree, the store) are JSON.

use std::cell::RefCell;

use serde_json::{json, Value};

wit_bindgen::generate!({
    path: "../../../../../../docs/plugins/wit",
    world: "plugin",
});

use dereth::plugin::host;

/// This session's experience: earned, since when (the plugin's clock), and in how many gains.
#[derive(Default)]
struct Session {
    earned: i64,
    started: Option<f64>,
    gains: i64,
    window: bool,
}

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::default());
}

/// 1234567 as "1,234,567"; fractions rounded to whole numbers.
fn number(n: f64) -> String {
    // Experience fits in an i64 many times over.
    #[allow(clippy::cast_possible_truncation)]
    let whole = (n + 0.5).floor() as i64;
    let digits = whole.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if whole < 0 {
        out.insert(0, '-');
    }
    out
}

fn clock() -> f64 {
    host::time_clock().unwrap_or(0.0)
}

fn rate(s: &Session) -> f64 {
    let Some(started) = s.started else {
        return 0.0;
    };
    let hours = (clock() - started) / 3600.0;
    if hours <= 0.0 {
        0.0
    } else {
        s.earned as f64 / hours
    }
}

fn set(node: &str, text: &str) {
    let _ = host::ui_set("main", node, "text", &json!(text).to_string());
}

fn refresh() {
    SESSION.with_borrow(|s| {
        if !s.window {
            return;
        }
        set("earned", &number(s.earned as f64));
        set("rate", &format!("{} / hour", number(rate(s))));
        let xp: Value = host::player_get("xp")
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        if let Some(u) = xp["unassigned"].as_f64() {
            set("unassigned", &number(u));
        }
        if let Some(to) = xp["to_level"].as_f64() {
            let text = if to > 0.0 {
                format!("{} to next level", number(to))
            } else {
                "at the level cap".to_owned()
            };
            set("level", &text);
        }
    });
}

fn reset() {
    SESSION.with_borrow_mut(|s| {
        let window = s.window;
        *s = Session {
            window,
            ..Session::default()
        };
    });
    refresh();
}

fn lifetime() -> i64 {
    host::store_get("character", "lifetime")
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

fn label(text: &str) -> Value {
    json!({ "kind": "label", "text": text, "children": [] })
}

fn row(left: &str, id: &str, text: &str) -> Value {
    json!({ "kind": "row", "children": [label(left), { "kind": "label", "id": id, "text": text, "children": [] }] })
}

struct XpMeter;

impl Guest for XpMeter {
    fn init() -> Result<(), String> {
        if host::host_has("ui.window")? {
            let tree = json!({
                "kind": "window",
                "id": "main",
                "title": "XP Meter",
                "width": 24,
                "children": [
                    row("This session:", "earned", "0"),
                    row("Rate:", "rate", "0 / hour"),
                    row("Unassigned:", "unassigned", "-"),
                    { "kind": "label", "id": "level", "text": "", "children": [] },
                    { "kind": "button", "id": "reset", "text": "Reset", "children": [] },
                ],
            });
            host::ui_window(&tree.to_string())?;
            SESSION.with_borrow_mut(|s| s.window = true);
            host::events_subscribe("ui.clicked")?;
        }
        host::events_subscribe("character.xp")?;
        host::events_subscribe("session.login")?;
        host::task_timer(1000.0, true)?;
        host::commands_add(
            "xpc",
            "/xpc shows experience this session and the rate; /xpc reset starts again",
        )?;
        Ok(())
    }

    fn deliver(event: String, data: String) -> Result<String, String> {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        match event.as_str() {
            "character.xp" => {
                let gained = data["gained"].as_i64().unwrap_or(0);
                if gained > 0 {
                    let now = clock();
                    SESSION.with_borrow_mut(|s| {
                        s.started.get_or_insert(now);
                        s.earned += gained;
                        s.gains += 1;
                    });
                    let total = lifetime() + gained;
                    host::store_set("character", "lifetime", &total.to_string())?;
                    refresh();
                }
            }
            "session.login" => reset(),
            "task.timer" => refresh(),
            "ui.clicked" if data["id"] == "reset" => reset(),
            "command" if data["name"] == "xpc" => {
                if data["args"] == "reset" {
                    reset();
                    host::chat_print("XP Meter reset.", Some("system"))?;
                } else {
                    let line = SESSION.with_borrow(|s| {
                        format!(
                            "XP this session: {} in {} gains, {} per hour. Watched in all: {}.",
                            number(s.earned as f64),
                            s.gains,
                            number(rate(s)),
                            number(lifetime() as f64),
                        )
                    });
                    host::chat_print(&line, Some("system"))?;
                }
            }
            _ => {}
        }
        Ok("null".to_owned())
    }
}

export!(XpMeter);
