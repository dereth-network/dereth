# Getting started: a compiled plugin

**Provenance: design.** A walk-through of the plugin interface; the rules are in
[wasm.md](../wasm.md), which this page leans on.

A compiled plugin is a WebAssembly component instead of Lua scripts. It reaches the same host
operations under the same permissions, budget and memory cap, and hears the same events. The
difference is the shape: where a Lua plugin registers a handler per event, a compiled plugin
exports two functions, `init` (what a Lua entry file's top level does) and `deliver` (every event,
by name), and every table crosses the boundary as JSON text. Rust is the language shown here;
anything that builds a component from a WIT world can make one.

The test suite's own compiled plugin, the [XP Meter](samples/xp-meter.md) in Rust, is a finished
example ([`dereth/testkit/tests/fixtures/compiled-plugin`](../../../dereth/testkit/tests/fixtures/compiled-plugin));
this page builds the pieces up.

## What you need

- A Rust toolchain with the `wasm32-unknown-unknown` target (`rustup target add
  wasm32-unknown-unknown`).
- This repository, for the WIT file (`docs/plugins/wit/dereth-plugin.wit`) and the author's tool
  (`tools/plugin`, run as `cargo run -p dereth-plugin -- …` from the repository's root).

## 1. The crate

A library crate of type `cdylib`, with `wit-bindgen` to generate the bindings and `serde_json` to
read and write the JSON values:

```toml
# hello/Cargo.toml
[workspace]              # a crate of its own, even inside another workspace's folder

[package]
name = "hello-plugin"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = { version = "0.57", default-features = false, features = ["macros", "realloc"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
strip = true
```

The empty `[workspace]` table keeps cargo from treating the crate as a member of a surrounding
workspace. The release profile is optional; it keeps the module small (the sample's is about 80 kB).

## 2. The bindings and the two exports

`wit_bindgen::generate!` reads the WIT file and makes a module `dereth::plugin::host` with one
function per host operation (each answering `Result<T, String>`), and a trait `Guest` with the
plugin's two exports. `export!` names the type that implements it.

```rust
// hello/src/lib.rs
wit_bindgen::generate!({ path: "<path to docs/plugins/wit>", world: "plugin" });

use dereth::plugin::host;
use serde_json::{json, Value};

struct Hello;

impl Guest for Hello {
    fn init() -> Result<(), String> {
        host::events_subscribe("character.xp")?;
        host::commands_add("hello", "/hello says hello")?;
        Ok(())
    }

    fn deliver(event: String, data: String) -> Result<String, String> {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        match event.as_str() {
            "character.xp" => {
                let gained = data["gained"].as_i64().unwrap_or(0);
                host::console_print(&format!("gained {gained}"))?;
            }
            "command" if data["name"] == "hello" => {
                host::chat_print("Hello from a compiled plugin.", Some("system"))?;
            }
            _ => {}
        }
        Ok("null".to_owned())
    }
}

export!(Hello);
```

`path` is relative to the crate's folder. Each host operation's WIT name is its schema name with
`.` and `_` turned into `-` (`chat.print` is `chat-print`, which the bindings call `chat_print`);
the generated [operations reference](../reference/operations.md) lists every one with its WIT
signature.

An `Err` from `init` fails the load and shows in the plugin's console. An `Err` from `deliver`
counts as a failing handler, as a raised Lua error does ([lifecycle](lifecycle-and-sandbox.md#faults-and-suspension)).

## 3. Events: which arrive, and how

A compiled plugin hears an event once it subscribes to it with `events-subscribe`, as `on` does in
Lua. Four kinds arrive without subscribing, each to the plugin that owns what they are about:

| event | data | the Lua plugin's equivalent |
|---|---|---|
| `task.timer` | `{ "id" }`: the id `task-timer` answered | the function given to `task.every` or `task.after` |
| `command` | `{ "name", "args", "words" }` | the function given to `commands.add` |
| `action.completed` | `{ "id", "value" }` | a promise settling |
| `action.failed` | `{ "id", "reason" }` | a promise failing |

and so do `plugin.unload` (answer a JSON value to hand to the next load) and `plugin.reload` (with
that value), the Lua plugin's `on_unload` and `on_reload`. Every other event's data is the table a
Lua handler receives, as JSON ([events.md](../events.md)).

There are no promises: `act-request(action, args)` asks for any action by its name in
[actions.md](../actions.md) (`act.use`, `magic.cast`, `nav.move_to`, …), with its arguments as a
JSON map (`{ "id": 2147483665 }` for `act.use`), and answers its id; its end arrives later as
`action.completed` or `action.failed` with that id. Keep the ids you are waiting on in the plugin's
state.

## 4. State, windows and the store

The component's memory persists between calls, so the plugin keeps its state in a static; the
sample uses a `thread_local!` `RefCell`. Values that are tables in Lua cross as JSON text:

```rust
// A window: the same tree a Lua plugin declares, as JSON (see ui.md).
let tree = json!({ "kind": "window", "id": "main", "title": "Hello", "children": [
    { "kind": "label", "id": "count", "text": "0", "children": [] },
] });
host::ui_window(&tree.to_string())?;
host::ui_set("main", "count", "text", &json!("1").to_string())?;

// The character's store (opens at login), read and written as JSON.
let seen: i64 = host::store_get("character", "seen")
    .ok()
    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    .and_then(|v| v.as_i64())
    .unwrap_or(0);
host::store_set("character", "seen", &(seen + 1).to_string())?;
```

Check an optional capability with `host::host_has("ui.window")?` before using it, as a Lua plugin
checks `host.has`.

## 5. Build and package

The package is a folder with `plugin.toml` and the component, named by the manifest's `entry`:

```toml
# hello-package/plugin.toml
id = "example.hello"
name = "Hello (compiled)"
version = "0.1.0"
api = "0.5"
entry = "plugin.wasm"
```

From the repository's root, the author's tool builds the crate and writes the component into the
package folder:

```text
cargo run -p dereth-plugin -- build path/to/hello path/to/hello-package
```

It compiles the crate for `wasm32-unknown-unknown` in release (into `target/plugin-builds` under
the current folder, or under `CARGO_TARGET_DIR`), makes the module a component, writes the hash of
the crate's sources into it (the custom section `dereth-source-sha256`) and saves it as
`plugin.wasm`. The package holds no sources: only the manifest and the component (and any data
files). From there it is a package like any other: load it with `--plugins`, and `pack` it as
[install-and-update.md](install-and-update.md) describes.

## 6. What the host checks

At load the host takes the core module out of the component and checks it: it must import only
the interface's functions, from `dereth:plugin/host@0.N.x` with `N` no newer than the host's API
minor version, each with its own signature, and export `memory`, `cabi_realloc`, `init` and
`deliver`. Anything else is refused with the reason (`imports X, which the host does not offer`).
Its memory is capped at the host's limit, and its work is counted in fuel: 100,000 units (function
entries and loop iterations) per millisecond of the call budget, the same count on every host. A
trap (a panic, a runaway loop, memory exhausted) fails the entry as a Lua error would.

The desktop and headless clients run the module in an interpreter; the browser client in the
browser's own WebAssembly.
