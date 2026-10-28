# Compiled plugins: WebAssembly components

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin may be compiled instead of written in Lua: a **WebAssembly component** whose world is
`plugin` in [`wit/dereth-plugin.wit`](wit/dereth-plugin.wit). It reaches the same host
operations a Lua plugin reaches, under the same permissions, budget and memory limit, and hears
the same events. Rust is the language this document shows; any language that makes a component
from a WIT world can make one.

## The interface, and where it comes from

The plugin API's schema (the host operations, each with its typed parameters and answer) is the
one source of two files, generated from it and checked against it by the test suite, so neither
can drift:

- [`wit/dereth-plugin.wit`](wit/dereth-plugin.wit): package `dereth:plugin`, versioned as the API
  (`0.4.0`). The interface `host` has one function per operation, each answering
  `result<T, string>`: the error is the text a Lua plugin would see raised (a missing permission,
  a wrong argument). The world `plugin` imports `host` and exports two functions.
- [`dereth.d.luau`](dereth.d.luau): the Luau type definitions of the Lua functions that are these
  operations one to one (`chat.print`, `files.package`, `regex.match`, ...), for a Luau language
  server. The rest of the Lua API (events, promises, handles, the libraries) is Lua itself,
  described in [api.md](api.md).

Values that are tables in Lua cross as JSON text, the WIT type `value`: an object's fields, an
event's data, a UI tree, a stored value.

Regenerate both files after changing the schema with `DERETH_TEST_GOLDEN=1 cargo test -p
dereth-testkit --test cpu plugins::the_wit_interface`.

## The plugin's side

| export | when | what it does |
|---|---|---|
| `init() -> result<_, string>` | once, when the plugin loads | what a Lua plugin's entry file does at its top level: subscribe to events (`events-subscribe`), claim commands (`commands-add`), declare windows (`ui-window`), start timers (`task-timer`). An `err` fails the load, and the chat says why |
| `deliver(event: string, data: string) -> result<string, string>` | for each event the plugin hears | the event's name and its data (JSON), as a Lua handler receives them; answers a value (JSON; `null` for none), which for `plugin.unload` is the state the next load receives |

What a Lua plugin receives through the Lua API's own dispatch arrives here as events too:

| event | data | Lua equivalent |
|---|---|---|
| `task.timer` | `{ "id": <timer id> }` | the function given to `task.every`/`task.after` |
| `command` | `{ "name", "args", "words" }` | the function given to `commands.add` |
| `action.completed`, `action.failed` | `{ "id", "value" }` / `{ "id", "reason" }` | a promise settling |
| `ui.clicked`, `ui.changed`, ... | as in [ui.md](ui.md) | `on("ui.clicked", ...)` |

`task.timer`, `command` and `action.*` reach the plugin that owns the timer, the command or the
action without subscribing; every other event arrives once the plugin subscribes to it with
`events-subscribe`, as `on` does in Lua.
Actions are asked for with `act-request(action, args)`, which answers the action's id (or none,
when there is no game to act in); its end arrives as `action.completed` or `action.failed` with
that id.

## The package

A compiled plugin's package is a package as [packages.md](packages.md) describes, whose manifest's
`entry` is the component, a `.wasm` file:

```toml
id = "example.xpmeter-compiled"
name = "XP Meter (compiled)"
version = "0.1.0"
api = "0.4"
entry = "plugin.wasm"
```

Its sources are not part of the package (a package holds no source files but Lua's).

## What the host does with it

1. **Takes the core module out.** A component holds the plugin's core module (the one with the
   memory and the allocator, `cabi_realloc`); the host runs that module and carries values across
   by the component model's canonical ABI itself. A core module made to the same ABI is accepted
   as it is.
2. **Checks it.** It must be a valid module; import only the interface's functions, from
   `dereth:plugin/host@0.N.x` with `N` no newer than the host's minor version, each with its
   signature; and export `memory`, `cabi_realloc`, `init` and `deliver`. Anything else is refused
   at load ("imports X, which the host does not offer").
3. **Caps its memory** at the host's memory limit (64 MiB by default): growing past it fails as it
   would at the top of memory, and a module that starts larger is refused.
4. **Counts its work.** Every function entry and every loop iteration costs one unit of fuel, and
   an instruction whose work grows with an operand costs by that operand: filling, copying or
   initialising memory a unit for every 64 bytes, growing memory 1,024 units a page, and filling,
   copying, initialising or growing a table a unit an element. Each `deliver` is given 100,000
   units per millisecond of the call budget (400,000 by default), and `init` (and the `deliver` of
   `context.begin`) twenty-five times that; a module that runs out traps and the entry fails as over its budget. The count is the same on
   every host, so a plugin stopped for its own work stops at the same point everywhere. The host's
   time answering the module's calls is the entry's too: it is taken from the fuel where the
   runtime allows, and the entry is stopped once that time alone is past the budget. A 64-bit
   memory or table is refused.
5. **Runs it** in the host's WebAssembly runtime: on the desktop and headless an interpreter
   (`wasmi`), in the browser the browser's own WebAssembly.

A trap (a panic in the plugin, a runaway loop, memory exhausted) fails the entry as a Lua error
would: the chat says so, and repeated faults suspend the plugin.

## Writing one in Rust

A crate of type `cdylib`, with `wit-bindgen` generating the bindings from the WIT file:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = { version = "0.57", default-features = false, features = ["macros", "realloc"] }
serde_json = "1"
```

```rust
wit_bindgen::generate!({ path: "<path to docs/plugins/wit>", world: "plugin" });
use dereth::plugin::host;

struct Plugin;

impl Guest for Plugin {
    fn init() -> Result<(), String> {
        host::events_subscribe("character.xp")?;
        host::commands_add("hello", "/hello says hello")?;
        Ok(())
    }

    fn deliver(event: String, data: String) -> Result<String, String> {
        if event == "command" {
            host::chat_print("Hello from a compiled plugin.", Some("system"))?;
        }
        Ok("null".to_owned())
    }
}

export!(Plugin);
```

Build it into a package folder with the author's tool, from the repository's root:

```text
dereth-plugin build <crate folder> <package folder>
```

It compiles the crate for `wasm32-unknown-unknown` in release, makes the module a component
(`wit-component`), writes the hash of the crate's sources into it (the custom section
`dereth-source-sha256`) and saves it as `<package folder>/plugin.wasm`. Then `dereth-plugin pack`
works as for any package.

The test suite's compiled plugin,
[`dereth/testkit/tests/fixtures/compiled-plugin`](../../dereth/testkit/tests/fixtures/compiled-plugin),
is the XP Meter compiled from the Rust crate beside it; the suite runs it and the Lua XP Meter over
the same recorded session and requires the same answers.
