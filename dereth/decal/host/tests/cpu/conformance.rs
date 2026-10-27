//! Behaviour: none (a host inside another program: the spec's conformance cases it runs)
//!
//! The spec's conformance cases this host runs, through its C interface alone, compared with each
//! case's expectations by Dereth's own checker. The subset is every case over a recording or over
//! frames alone whose steps a player can take through Decal: typing a line, a click or a change on
//! a window's control, a hotkey's button, and Decal's bar button for a plugin. Left out: the cases
//! of a scripted world (it stands in for the server, which here is the retail one), HTTPS (this
//! host offers none), two clients on one bus, the time budget itself (timed), resizing a window and
//! a read-only text box typed into (Decal's views do neither), and laying windows out by the
//! reference measures (this host lays them out in Decal's).

use dereth_client_sdk::plugin_api::Value;
use dereth_testkit::conformance::{case, check, Action, Outcome};

use crate::abi::{settings, Replay};

/// The cases this host runs.
const CASES: &[&str] = &[
    "core/chat-filter",
    "core/chat-lines",
    "core/commands",
    "core/events-order",
    "core/faults",
    "core/refused",
    "core/sandbox",
    "core/store",
    "core/timers",
    "core/ui-window",
    "ui/bar",
    "ui/controls",
    "ui/hotkeys",
    "ui/hud",
    "ui/hud-unlock",
];

/// `plugin/window.node`, or `window.node` of the case's own plugin.
fn owner(id: &str, path: &str) -> (String, String, String) {
    let (plugin, rest) = path.split_once('/').unwrap_or((id, path));
    let (window, node) = rest
        .split_once('.')
        .unwrap_or_else(|| panic!("{path}: want window.node"));
    (plugin.to_owned(), window.to_owned(), node.to_owned())
}

fn click(r: &mut Replay, key: &str, node: &str) {
    let (name, _) = r.surface.control(key, node).unwrap_or_else(|| {
        panic!(
            "no control for {key} {node}: {:?}",
            r.surface.views.get(key)
        )
    });
    r.view_event(key, &name, "click", Value::Nil);
}

/// A number's text, as its stepper shows it.
fn shown_number(r: &Replay, key: &str, step_button: &str) -> f64 {
    let text_name = step_button.trim_end_matches(['d', 'i']);
    r.surface.views[key].1[text_name]
        .get("text")
        .as_str()
        .and_then(|t| t.parse().ok())
        .expect("a number's text")
}

fn set(r: &mut Replay, key: &str, node: &str, value: &Value) {
    let (name, state) = r
        .surface
        .control(key, node)
        .unwrap_or_else(|| panic!("no control for {key} {node}"));
    match state.get("kind").as_str().unwrap_or_default() {
        "toggle" | "input" | "slider" => r.view_event(key, &name, "change", value.clone()),
        "choice" => {
            let options = state.get("options").as_list().unwrap_or_default();
            let i = options
                .iter()
                .position(|o| o.as_str() == value.as_str())
                .expect("one of the choice's options");
            r.view_event(
                key,
                &name,
                "change",
                Value::from(i64::try_from(i).unwrap_or(0)),
            );
        }
        "button" => {
            // A number: stepped down or up until it reads the value.
            let want = value.as_f64().expect("a number");
            for _ in 0..100 {
                let now = shown_number(r, key, &name);
                if (now - want).abs() < 1e-9 {
                    return;
                }
                let button = format!(
                    "{}{}",
                    name.trim_end_matches(['d', 'i']),
                    if want < now { 'd' } else { 'i' }
                );
                r.view_event(key, &button, "click", Value::Nil);
            }
            panic!("{key} {node} never reached {want}");
        }
        other => panic!("{key} {node}: a {other} takes no change here"),
    }
}

fn run(profile_case: &str) -> Outcome {
    let (profile, name) = profile_case.split_once('/').expect("profile/case");
    let c = case(profile, name);
    let id = dereth_testkit::plugins::read_package(&c.dir)
        .manifest()
        .map(|m| m.id.to_string())
        .expect("a manifest");
    let s = settings(std::slice::from_ref(&c.dir));
    let mut r = match &c.scenario {
        Some((session, _, _)) => Replay::new(&s, session),
        None => Replay::frames_only(&s),
    };
    // The plugin loads before the world, so that it hears the login.
    r.frame();
    let total = match &c.scenario {
        Some((_, _, seconds)) => {
            r.login();
            *seconds
        }
        None => c.steps.last().map_or(0.0, |s| s.after),
    };
    for step in &c.steps {
        let wait = step.after - r.since_entry();
        if wait > 0.0 {
            r.advance(wait);
        }
        match &step.action {
            Action::Say(line) => {
                r.type_line(line);
            }
            Action::Click(path) => {
                let (p, w, n) = owner(&id, path);
                click(&mut r, &format!("{p}/{w}"), &n);
            }
            Action::Set(path, value) => {
                let (p, w, n) = owner(&id, path);
                set(&mut r, &format!("{p}/{w}"), &n, value);
            }
            Action::Hotkey(node) => {
                let key = r
                    .surface
                    .views
                    .keys()
                    .find(|k| {
                        k.starts_with(&format!("{id}/")) && r.surface.control(k, node).is_some()
                    })
                    .cloned()
                    .unwrap_or_else(|| panic!("no view holds the hotkey {node}"));
                click(&mut r, &key, node);
            }
            Action::Bar(plugin) => {
                let (key, open) = r
                    .surface
                    .views
                    .iter()
                    .find(|(k, _)| k.starts_with(&format!("{plugin}/")))
                    .map(|(k, v)| (k.clone(), v.0))
                    .unwrap_or_else(|| panic!("{plugin} has no view on Decal's bar"));
                r.view_event(&key, "", if open { "closed" } else { "opened" }, Value::Nil);
            }
            other => panic!("{profile_case}: {other:?} is not a step this host's subset takes"),
        }
    }
    let wait = total - r.since_entry();
    if wait > 0.0 {
        r.advance(wait);
    }
    let report = r.dll.report();
    let lines = |k: &str| -> Vec<String> {
        report
            .get(k)
            .as_list()
            .unwrap_or_default()
            .iter()
            .map(|l| l.as_str().unwrap_or_default().to_owned())
            .collect()
    };
    Outcome {
        status: r.status(&id),
        console: r.console(&id),
        printed: r.surface.printed.clone(),
        hidden: r.surface.hidden.clone(),
        ui: report.get("ui").as_str().unwrap_or_default().to_owned(),
        bar: lines("bar"),
        ..Outcome::default()
    }
}

#[test]
fn every_case_of_the_subset_passes_through_the_c_interface() {
    let mut failures = Vec::new();
    for name in CASES {
        let (profile, case_name) = name.split_once('/').expect("profile/case");
        let c = case(profile, case_name);
        let out = run(name);
        let diffs = check(&c, &out);
        if !diffs.is_empty() {
            failures.push(format!("{name} ({}):\n{}", c.says, diffs.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
