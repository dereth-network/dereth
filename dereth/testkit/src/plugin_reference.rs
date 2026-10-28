//! The plugin interface's reference pages, generated from the code that defines the interface:
//! the host operations (the API's schema), the permissions, the host capabilities, the UI node
//! kinds, the host's limits and the conformance cases. The pages live in the spec
//! (`docs/plugins/reference/`) and a test holds them to what this module generates, so the
//! reference cannot drift from the code.

use std::fmt::Write as _;

use dereth_plugin_api::permission::{AutoDomain, Permission};
use dereth_plugin_api::schema::{self, Op, Ty};
use dereth_plugin_api::ui::{self, NodeKind};
use dereth_plugin_api::{DomainRule, HostCapability, Limits, WorldPolicy};

use crate::conformance::{self, Case, CASES_DIR};

/// Where the pages are, relative to the workspace root.
pub const REFERENCE_DIR: &str = "docs/plugins/reference";

/// The conformance profiles, in the spec's order.
pub const PROFILES: &[(&str, &str)] = &[
    ("core", "Core"),
    ("ui", "UI"),
    ("overlay", "Overlay"),
    ("nav", "Nav"),
    ("pregame", "Pregame"),
    ("peers", "Peers"),
    ("automation", "Automation"),
    ("ubscript", "UtilityBelt scripts"),
];

/// Every page: its file name in [`REFERENCE_DIR`] and its text.
#[must_use]
pub fn pages() -> Vec<(&'static str, String)> {
    vec![
        ("operations.md", operations()),
        ("permissions.md", permissions()),
        ("capabilities.md", capabilities()),
        ("ui-nodes.md", ui_nodes()),
        ("limits.md", limits()),
        ("conformance-cases.md", conformance_cases()),
    ]
}

fn header(title: &str, intro: &str) -> String {
    format!(
        "# {title}\n\n\
         **Provenance: design.** Generated from the plugin interface's own code, so it cannot \
         drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test \
         cpu plugins::the_plugin_reference` rewrites it.\n\n\
         {intro}\n"
    )
}

/// A table cell: no line breaks, and a `|` escaped.
fn cell(s: &str) -> String {
    s.replace('\n', " ").replace('|', "\\|")
}

// -------------------------------------------------------------------------------------------------
// Operations
// -------------------------------------------------------------------------------------------------

/// What a schema type holds, in words. Exhaustive, so a new type cannot go undescribed.
fn ty_meaning(t: Ty) -> &'static str {
    match t {
        Ty::Bool => "true or false",
        Ty::Int => "a 64-bit signed whole number",
        Ty::Num => "a 64-bit floating-point number",
        Ty::Str => "text",
        Ty::Value => {
            "any plain value: a table, a list, a string, a number, a boolean or nil; JSON text \
             across the WIT boundary"
        }
        Ty::Opt(_) => "absent (Lua `nil`, WIT `none`), or a value of the inner type",
    }
}

fn wit_signature(op: &Op) -> String {
    let params: Vec<String> = op
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty.wit()))
        .collect();
    let result = op.result.map_or_else(|| "_".to_owned(), Ty::wit);
    format!(
        "{}: func({}) -> result<{result}, string>",
        op.wit_name(),
        params.join(", ")
    )
}

fn lua_signature(op: &Op, lua: &str) -> String {
    let params: Vec<String> = op
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.replace('-', "_"), p.ty.luau()))
        .collect();
    let result = op.result.map_or_else(|| "()".to_owned(), Ty::luau);
    format!("{lua}({}) -> {result}", params.join(", "))
}

fn namespace(op: &Op) -> &'static str {
    op.name.split_once('.').map_or(op.name, |(ns, _)| ns)
}

fn operations() -> String {
    let mut s = header(
        "Host operations",
        &format!(
            "Every operation a plugin reaches in its host, from the API's schema (API {}). A compiled \
             plugin imports each as a function of the WIT interface `host` in \
             [`wit/dereth-plugin.wit`](../wit/dereth-plugin.wit) ([wasm.md](../wasm.md)). A Lua \
             plugin reaches each through the Lua API ([api.md](../api.md)); where one Lua function \
             is the operation one to one, it is named here, and [`dereth.d.luau`](../dereth.d.luau) \
             declares it. The rest of the Lua API (`on`, promises, window and object handles, the \
             libraries) is built on these operations in Lua and is described in [api.md](../api.md).\n\n\
             An operation that fails answers an error saying why (a wrong argument says which). A \
             Lua plugin sees it raised; a compiled one receives it as `err`.",
            schema::WIT_VERSION
        ),
    );
    s.push_str("\n## Types\n\n| schema | WIT | Luau | holds |\n|---|---|---|---|\n");
    for t in [
        Ty::Bool,
        Ty::Int,
        Ty::Num,
        Ty::Str,
        Ty::Value,
        Ty::Opt(&Ty::Str),
    ] {
        let name = match t {
            Ty::Opt(_) => "optional".to_owned(),
            other => format!("{other:?}").to_lowercase(),
        };
        let (wit, luau) = match t {
            Ty::Opt(_) => ("option<T>".to_owned(), "T?".to_owned()),
            other => (other.wit(), other.luau()),
        };
        let _ = writeln!(
            s,
            "| {name} | `{wit}` | `{luau}` | {} |",
            cell(ty_meaning(t))
        );
    }
    let mut namespaces: Vec<&str> = Vec::new();
    for op in schema::OPS {
        if !namespaces.contains(&namespace(op)) {
            namespaces.push(namespace(op));
        }
    }
    s.push_str("\n## By namespace\n\n| namespace | operations |\n|---|---|\n");
    for ns in &namespaces {
        let names: Vec<String> = schema::OPS
            .iter()
            .filter(|o| namespace(o) == *ns)
            .map(|o| format!("`{}`", o.name))
            .collect();
        let _ = writeln!(s, "| `{ns}` | {} |", names.join(", "));
    }
    let _ = writeln!(
        s,
        "\n{} operations in {} namespaces.",
        schema::OPS.len(),
        namespaces.len()
    );
    for ns in &namespaces {
        let _ = write!(s, "\n## `{ns}`\n");
        for op in schema::OPS.iter().filter(|o| namespace(o) == *ns) {
            let _ = write!(s, "\n### `{}`\n\n{}\n\n", op.name, op.doc);
            let _ = writeln!(s, "- WIT: `{}`", wit_signature(op));
            match op.lua {
                Some(lua) => {
                    let _ = writeln!(s, "- Lua: `{}`", lua_signature(op, lua));
                }
                None => s.push_str("- Lua: through the Lua API ([api.md](../api.md))\n"),
            }
        }
    }
    s
}

// -------------------------------------------------------------------------------------------------
// Permissions
// -------------------------------------------------------------------------------------------------

/// What a domain rule allows. Exhaustive, so a new rule cannot go undescribed.
fn rule_meaning(r: DomainRule) -> &'static str {
    match r {
        DomainRule::Allow => "actions with or without a gesture",
        DomainRule::InteractiveOnly => {
            "actions only with a live activation (a gesture on the plugin's own surface)"
        }
        DomainRule::Deny => "no actions in the domain",
    }
}

fn permissions() -> String {
    let mut s = header(
        "Permissions",
        "Every permission a manifest may name, what it covers and what a plugin needs to do it. A \
         manifest's permissions tell the player what the plugin uses; nothing is asked. \
         [permissions.md](../permissions.md) is the rule behind them: gestures and automation, \
         and the rate ceilings.",
    );
    s.push_str("\n| permission | covers | who may |\n|---|---|---|\n");
    for p in Permission::all() {
        let covers = match p {
            Permission::Auto(d) => format!("{} The `{}` domain.", p.description(), d.name()),
            other => other.description().to_owned(),
        };
        let _ = writeln!(
            s,
            "| `{}` | {} | {} |",
            p.name(),
            cell(&covers),
            p.availability().description()
        );
    }
    let open = WorldPolicy::unpublished();
    let closed = WorldPolicy::published(std::collections::BTreeMap::new());
    s.push_str(
        "\n## Automation domains and a world's rules\n\n\
         Whether an action may run is the world's rule for its domain ([policy.md](../policy.md)): \
         some rules ask for a gesture behind it. The rules:\n\n\
         | rule | allows |\n|---|---|\n",
    );
    for r in [
        DomainRule::Allow,
        DomainRule::InteractiveOnly,
        DomainRule::Deny,
    ] {
        let _ = writeln!(s, "| `{}` | {} |", r.name(), rule_meaning(r));
    }
    s.push_str(
        "\nEach domain's rule in a world that publishes no policy, and in a published policy that \
         does not name the domain. `session` is logging in, choosing a character and logging out \
         without the player (the `session.control` permission).\n\n\
         | domain | permission | no policy published | published, not named |\n|---|---|---|---|\n",
    );
    for d in AutoDomain::ALL {
        let _ = writeln!(
            s,
            "| `{}` | `{}` | `{}` | `{}` |",
            d.name(),
            Permission::Auto(d).name(),
            open.rule(d).name(),
            closed.rule(d).name()
        );
    }
    let _ = writeln!(
        s,
        "| `session` | `{}` | `{}` | `{}` |",
        Permission::SessionControl.name(),
        open.session.name(),
        closed.session.name()
    );
    s
}

// -------------------------------------------------------------------------------------------------
// Capabilities
// -------------------------------------------------------------------------------------------------

fn capabilities() -> String {
    let mut s = header(
        "Host capabilities",
        "Every capability a host may offer. A plugin lists the ones it cannot run without in \
         `capabilities.required` (a host without one refuses to load it) and the ones it uses when \
         they are there in `capabilities.optional`, checking them at run time with `host.has(name)`. \
         Which host offers which is in the [README](../README.md#host-capabilities).",
    );
    s.push_str("\n| capability | what it is |\n|---|---|\n");
    for c in HostCapability::ALL {
        let _ = writeln!(s, "| `{}` | {} |", c.name(), c.description());
    }
    s
}

// -------------------------------------------------------------------------------------------------
// UI nodes
// -------------------------------------------------------------------------------------------------

fn ui_nodes() -> String {
    let mut s = header(
        "UI node kinds",
        &format!(
            "Every kind of node a declared window or HUD is built from, with the properties each \
             takes and the type of value each property holds. [ui.md](../ui.md) is how windows are \
             declared, changed and closed, and the events they raise.\n\n\
             Every node may also have an `id`, a string unique within its window; a node that holds \
             others lists them as its children. A declaration or a property change that breaks a \
             rule is refused with the rule. A window holds at most {} nodes, at most {} deep, and a \
             list at most {} rows; one text at most {} characters, a window's text at most {} KiB \
             all told, and a canvas at most {} draw commands. `width` and `height` are a node's \
             natural size, in character cells across and lines down (a list's height in rows, a \
             canvas's and a chart's size in its own pixels); `grow`, the least and most sizes \
             and a container's `gap`, `align` and `justify` share a window's room out as \
             [ui.md](../ui.md#layout) says. A HUD holds no {}: its only controls are buttons, \
             toggles and lists.",
            ui::MAX_NODES,
            ui::MAX_DEPTH,
            ui::MAX_LIST_ROWS,
            ui::MAX_TEXT,
            ui::MAX_TREE_TEXT / 1024,
            dereth_plugin_api::draw::MAX_COMMANDS,
            {
                let kinds: Vec<String> = NodeKind::ALL
                    .iter()
                    .filter(|k| !k.in_hud())
                    .map(|k| format!("`{}`", k.name()))
                    .collect();
                kinds.join(", ")
            }
        ),
    );
    s.push_str("\n| kind | holds nodes | properties |\n|---|---|---|\n");
    for k in NodeKind::ALL {
        let props: Vec<String> = k
            .properties()
            .iter()
            .map(|(n, _)| format!("`{n}`"))
            .collect();
        let _ = writeln!(
            s,
            "| [`{}`](#{}) | {} | {} |",
            k.name(),
            k.name(),
            if k.is_container() { "yes" } else { "no" },
            if props.is_empty() {
                "none".to_owned()
            } else {
                props.join(", ")
            }
        );
    }
    for k in NodeKind::ALL {
        let _ = write!(s, "\n## {}\n\n{}\n", k.name(), k.description());
        let props = k.properties();
        if props.is_empty() {
            s.push_str("\nNo properties.\n");
            continue;
        }
        s.push_str("\n| property | takes |\n|---|---|\n");
        for (name, ty) in props {
            let _ = writeln!(s, "| `{name}` | {} |", cell(&ty));
        }
    }
    s
}

// -------------------------------------------------------------------------------------------------
// Limits
// -------------------------------------------------------------------------------------------------

/// One row of the limits page: the limit's name, its value and what it bounds.
type Limit = (&'static str, String, &'static str);

fn bytes(n: usize) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 10 && n.is_multiple_of(1 << 10) {
        format!("{} KiB", n >> 10)
    } else {
        format!("{n} bytes")
    }
}

/// One group of the limits page: its heading and its rows.
fn group(s: &mut String, heading: &str, rows: &[Limit]) {
    let _ = write!(
        s,
        "\n## {heading}\n\n| limit | value | what it bounds |\n|---|---|---|\n"
    );
    for (name, value, what) in rows {
        let _ = writeln!(s, "| {name} | {} | {} |", cell(value), cell(what));
    }
}

fn limits() -> String {
    use dereth_plugin_host::{actions, calls, nav, net, overlay, package, pattern, peers, session};
    let mut s = header(
        "Limits",
        "The numbers the host holds every plugin to, from the host's own constants. Each is \
         described where it applies: the sandbox in the [README](../README.md#how-a-plugin-runs), \
         actions in [actions.md](../actions.md), the rate ceilings in \
         [permissions.md](../permissions.md#rate-ceilings).",
    );
    let lim = Limits::default();
    let secs = |v: f64| format!("{v} s");
    let types: Vec<String> = package::ALLOWED_EXTENSIONS
        .iter()
        .map(|e| format!("`.{e}`"))
        .collect();
    group(
        &mut s,
        "The VM",
        &[
            (
                "memory",
                bytes(lim.memory_bytes),
                "the most memory one plugin's VM may hold (the host's default)",
            ),
            (
                "time budget",
                format!("{} ms", lim.call_budget_ms),
                "how long one entry (loading, or delivering one event) may run before it is \
                 stopped (the host's default)",
            ),
            (
                "compiled work",
                format!("{} units per ms", dereth_plugin_wasm::FUEL_PER_MS),
                "a compiled plugin's budget, counted in function entries and loop iterations, \
                 the same on every host",
            ),
        ],
    );
    group(
        &mut s,
        "Packages",
        &[
            (
                "files",
                package::MAX_FILES.to_string(),
                "the most files one package may hold",
            ),
            (
                "size",
                bytes(package::MAX_BYTES),
                "the most bytes one package may hold, unpacked",
            ),
            (
                "file types",
                types.join(", "),
                "the only file extensions a package may hold",
            ),
        ],
    );
    group(
        &mut s,
        "Output, tasks and stores",
        &[
            (
                "printed lines",
                format!("{} a frame", calls::PRINTS_PER_FRAME),
                "local chat lines one plugin may print in one frame (`chat.print` and `print`)",
            ),
            (
                "printed lines kept",
                format!("{} lines", dereth_plugin_host::slot::CONSOLE_LINES),
                "the lines of what a plugin printed (and its errors) the host keeps",
            ),
            (
                "timer period",
                format!("{} ms", calls::MIN_PERIOD_MS),
                "the shortest period of a repeating timer",
            ),
            (
                "store",
                bytes(calls::SCOPE_QUOTA),
                "the most one store scope may hold, as JSON",
            ),
            (
                "patterns",
                pattern::CACHE.to_string(),
                "regular expressions one plugin keeps compiled; the oldest is dropped past it",
            ),
        ],
    );
    group(
        &mut s,
        "Actions",
        &[
            (
                "activation",
                secs(actions::ACTIVATION_SECS),
                "how long a player's gesture makes the plugin's actions interactive",
            ),
            (
                "queue wait",
                secs(actions::QUEUE_WAIT_SECS),
                "the longest an action waits for its turn before it fails as `busy`",
            ),
            (
                "queued",
                actions::MAX_QUEUED.to_string(),
                "the most actions one plugin may have queued",
            ),
            (
                "appraisals",
                format!("{} a second", actions::APPRAISALS_PER_SECOND),
                "the most appraisals one plugin may ask for",
            ),
            (
                "file requests",
                "1".to_owned(),
                "the file requests one plugin may have waiting for the player",
            ),
            (
                "after a refusal",
                secs(dereth_plugin_host::import::REFUSED_WAIT),
                "how long a plugin's next file request waits after the player refuses one, doubling with each refusal",
            ),
            (
                "refusals",
                dereth_plugin_host::import::REFUSALS.to_string(),
                "the refusals in a session after which a plugin is asked for no more files until it is reloaded",
            ),
        ],
    );
    group(
        &mut s,
        "UI and markers",
        &[
            (
                "nodes",
                ui::MAX_NODES.to_string(),
                "the most nodes one window may hold",
            ),
            (
                "depth",
                ui::MAX_DEPTH.to_string(),
                "the deepest a window's tree may be",
            ),
            (
                "list rows",
                ui::MAX_LIST_ROWS.to_string(),
                "the most rows one list may hold",
            ),
            (
                "text",
                format!("{} characters", ui::MAX_TEXT),
                "the most one text may hold: a property, an entry of a list, a cell, a drawing's words",
            ),
            (
                "window text",
                format!("{} KiB", ui::MAX_TREE_TEXT / 1024),
                "the most text one window may hold, all its nodes' together",
            ),
            (
                "canvas commands",
                dereth_plugin_api::draw::MAX_COMMANDS.to_string(),
                "the most draw commands one canvas may hold",
            ),
            (
                "picture",
                format!(
                    "{} by {} pixels",
                    dereth_plugin_host::pictures::MAX_SIDE,
                    dereth_plugin_host::pictures::MAX_SIDE
                ),
                "the largest picture kept, from the package or fetched",
            ),
            (
                "pictures",
                format!(
                    "{} MiB",
                    dereth_plugin_host::pictures::MAX_DECODED / (1024 * 1024)
                ),
                "the most one plugin's pictures may take, decoded",
            ),
            (
                "markers",
                overlay::MAX_MARKERS.to_string(),
                "the most markers one plugin may keep",
            ),
            (
                "map points",
                overlay::MAX_POINTS.to_string(),
                "the most points one map marker may hold",
            ),
        ],
    );
    group(
        &mut s,
        "Movement and the session",
        &[
            (
                "turn",
                format!("{}°", nav::TURN_TOLERANCE),
                "how far off the target's bearing the character may be and still run; beyond it, it stops and turns in place",
            ),
            (
                "turn again",
                secs(nav::TURN_RETRY_SECS),
                "how long a turn is given to finish before it is asked for again",
            ),
            (
                "player first",
                secs(nav::PLAYER_YIELD_SECS),
                "after the player moves the character, how long a movement no gesture asked for is refused (`player_moving`)",
            ),
            (
                "stuck",
                secs(nav::STUCK_SECS),
                "without coming half a metre closer, a move fails as `stuck`",
            ),
            (
                "slowest pace",
                format!("{} m/s", nav::SLOWEST_PACE),
                "a move slower than this fails as `timeout`",
            ),
            (
                "lost",
                secs(nav::LOST_SECS),
                "a followed object out of sight this long fails the follow as `lost`",
            ),
            ("face", secs(nav::FACE_SECS), "how long a face may take"),
            (
                "entry",
                secs(session::ENTER_SECS),
                "how long entering the world may take",
            ),
            (
                "log-off",
                secs(session::LOGOFF_SECS),
                "how long a log-off may take",
            ),
        ],
    );
    group(
        &mut s,
        "Other clients and the network",
        &[
            (
                "peer message",
                bytes(peers::MAX_MESSAGE),
                "the most one message's data may take, as JSON",
            ),
            (
                "peer sends",
                format!("{} a second", peers::SENDS_PER_SECOND),
                "the most messages one plugin may send",
            ),
            (
                "requests",
                net::IN_FLIGHT.to_string(),
                "the most HTTPS requests one plugin may have outstanding",
            ),
            (
                "answer",
                bytes(net::MAX_BODY),
                "the most bytes an HTTPS answer's body may hold",
            ),
        ],
    );
    s
}

// -------------------------------------------------------------------------------------------------
// Conformance cases
// -------------------------------------------------------------------------------------------------

fn runs_in(c: &Case) -> String {
    if c.pregame.is_some() {
        "a scripted pre-game".to_owned()
    } else if c.scripted.is_some() {
        "a scripted world".to_owned()
    } else if let Some((session, _, _)) = &c.scenario {
        format!("the recorded session `{session}`")
    } else {
        "the host's frames alone".to_owned()
    }
}

/// # Panics
/// Panics when a folder under the cases is not a profile of [`PROFILES`]: a profile the reference
/// does not list is a set of claims nobody reads.
fn conformance_cases() -> String {
    let dir = conformance::workspace_root().join(CASES_DIR);
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = PROFILES.iter().map(|(p, _)| (*p).to_owned()).collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "the conformance profiles on disk and the reference's list"
    );
    let mut s = header(
        "Conformance cases",
        "Every case of the conformance suite, by profile: what it proves (its `expect.toml`'s \
         `says`) and what it runs in. \
         [conformance.md](../conformance.md) is what a case is, what each profile covers and how a \
         host runs them; each case's folder holds its plugin and its expectations.",
    );
    s.push_str("\n| profile | cases |\n|---|---|\n");
    let all: Vec<(&str, &str, Vec<Case>)> = PROFILES
        .iter()
        .map(|(p, title)| (*p, *title, conformance::cases(p)))
        .collect();
    for (p, title, cases) in &all {
        let _ = writeln!(s, "| [{title}](#{p}) | {} |", cases.len());
    }
    for (p, title, cases) in &all {
        let _ = write!(
            s,
            "\n## {title}\n\n| case | proves | runs in |\n|---|---|---|\n"
        );
        for c in cases {
            let _ = writeln!(
                s,
                "| [`{}`](../conformance/{p}/{}/) | {} | {} |",
                c.name,
                c.name,
                cell(&c.says),
                runs_in(c),
            );
        }
    }
    s
}
